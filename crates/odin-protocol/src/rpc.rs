use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_HEADER: usize = 8192;
#[derive(Clone, Copy, Debug)]
pub enum Framing {
    ContentLength,
    Newline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    JsonRpc,
    Dap,
}
type Pending = Arc<StdMutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, String>>>>>;
type Events = Arc<StdMutex<VecDeque<Value>>>;
#[derive(Clone)]
struct Cancellation {
    pending: Pending,
    stdin: Arc<Mutex<ChildStdin>>,
    framing: Framing,
    dialect: Dialect,
    timeout: Duration,
    supported: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
}
impl Cancellation {
    async fn send(&self, id: u64) {
        if self.closed.load(Ordering::SeqCst) || !self.supported.load(Ordering::SeqCst) {
            return;
        }
        let message = if self.dialect == Dialect::JsonRpc {
            json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}})
        } else {
            let seq = self.next_id.fetch_add(1, Ordering::Relaxed);
            json!({"seq":seq,"type":"request","command":"cancel","arguments":{"requestId":id}})
        };
        let mut guard = WriteGuard {
            closed: self.closed.clone(),
            pending: self.pending.clone(),
            complete: false,
        };
        if write_frame_bounded(&self.stdin, self.framing, &message, self.timeout)
            .await
            .is_err()
        {
            self.closed.store(true, Ordering::SeqCst);
        } else {
            guard.complete = true;
        }
    }
}
/// Dropping a cancelled caller removes its pending sender as well.
struct PendingRegistration {
    pending: Pending,
    id: u64,
    cancellation: Option<Cancellation>,
}
impl Drop for PendingRegistration {
    fn drop(&mut self) {
        let removed = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&self.id))
            .is_some();
        if removed
            && let (Some(cancellation), Ok(runtime)) = (
                self.cancellation.take(),
                tokio::runtime::Handle::try_current(),
            )
        {
            let id = self.id;
            runtime.spawn(async move {
                cancellation.send(id).await;
            });
        }
    }
}
/// Cancellation during a pipe write can leave a partial frame. Poison that
/// connection rather than appending another request to a corrupted stream.
struct WriteGuard {
    closed: Arc<AtomicBool>,
    pending: Pending,
    complete: bool,
}
impl Drop for WriteGuard {
    fn drop(&mut self) {
        if !self.complete {
            self.closed.store(true, Ordering::SeqCst);
            if let Ok(mut pending) = self.pending.lock() {
                for (_, sender) in pending.drain() {
                    let _=sender.send(Err("Backend connection closed during frame write; operation outcome may be unknown".into()));
                }
            }
        }
    }
}

pub struct RpcProcess {
    child: Mutex<Child>,
    process_tree: StdMutex<Option<odin_core::process::ProcessTree>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    events: Events,
    latest_diagnostics: Arc<StdMutex<HashMap<String, Value>>>,
    closed: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
    cancel_supported: Arc<AtomicBool>,
    server_status: Arc<StdMutex<Option<Value>>>,
    progress: Arc<StdMutex<HashMap<String, Value>>>,
    reader: JoinHandle<()>,
    framing: Framing,
    dialect: Dialect,
    timeout: Duration,
}
impl RpcProcess {
    #[expect(
        clippy::too_many_arguments,
        reason = "Preserve the established public RPC construction API"
    )]
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: &Path,
        framing: Framing,
        dialect: Dialect,
        timeout: Duration,
        workspace: Value,
        settings: Value,
    ) -> Result<Arc<Self>> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        odin_core::process::configure_child(command.as_std_mut());
        let mut child = command
            .spawn()
            .with_context(|| format!("Cannot start backend executable: {program}"))?;
        let process_tree = odin_core::process::ProcessTree::attach(&child)?;
        let stdin = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("Missing child stdin"))?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Missing child stdout"))?;
        let pending: Pending = Arc::new(StdMutex::new(HashMap::new()));
        let events: Events = Arc::new(StdMutex::new(VecDeque::new()));
        let latest_diagnostics = Arc::new(StdMutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let server_status = Arc::new(StdMutex::new(None));
        let progress = Arc::new(StdMutex::new(HashMap::new()));
        let (status, progress_events) = (server_status.clone(), progress.clone());
        let (p, e, d, c, w) = (
            pending.clone(),
            events.clone(),
            latest_diagnostics.clone(),
            closed.clone(),
            stdin.clone(),
        );
        let reader = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let outcome:Result<()> = async {
                while let Some(bytes)=read_frame(&mut reader,framing).await? {
                    let message:Value=serde_json::from_slice(&bytes).context("Backend emitted invalid JSON")?;
                    let response_id=if dialect==Dialect::Dap && message["type"]=="response" { message.get("request_seq").and_then(Value::as_u64) }
                        else if message.get("method").is_none() { message.get("id").and_then(Value::as_u64) } else {None};
                    if let Some(id)=response_id {
                        let sender=p.lock().map_err(|_|anyhow!("Pending lock poisoned"))?.remove(&id);
                        if let Some(sender)=sender {
                            let result=if dialect==Dialect::Dap {
                                if message.get("success").and_then(Value::as_bool)==Some(true) {Ok(message.get("body").cloned().unwrap_or(Value::Null))}
                                else {Err(message.to_string())}
                            } else if let Some(error)=message.get("error") {Err(error.to_string())}
                            else {Ok(message.get("result").cloned().unwrap_or(Value::Null))};
                            let _=sender.send(result);
                        }
                        continue;
                    }
                    if dialect==Dialect::JsonRpc && message.get("method").is_some() && message.get("id").is_some() {
                        let method=message["method"].as_str().unwrap_or("");
                        let id=message["id"].clone();
                        let response=match method {
                            "workspace/configuration" => {
                                let items=message["params"]["items"].as_array().map(|a|a.iter().map(|item|{
                                    let section=item["section"].as_str().unwrap_or("");
                                    if section.is_empty(){settings.clone()}else{settings.get(section).cloned().unwrap_or_else(||settings.clone())}
                                }).collect::<Vec<_>>()).unwrap_or_default();
                                json!({"jsonrpc":"2.0","id":id,"result":items})
                            },
                            "workspace/workspaceFolders" => json!({"jsonrpc":"2.0","id":id,"result":workspace}),
                            "window/workDoneProgress/create" => json!({"jsonrpc":"2.0","id":id,"result":null}),
                            "ping" => json!({"jsonrpc":"2.0","id":id,"result":{}}),
                            "workspace/applyEdit" => json!({"jsonrpc":"2.0","id":id,"result":{"applied":false,"failureReason":"Prepare an Odin transaction; implicit edits are disabled"}}),
                            _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Backend-initiated method not supported by Odin"}}),
                        };
                        write_frame_bounded(&w,framing,&response,timeout).await?;continue;
                    }
                    if dialect==Dialect::Dap && message["type"]=="request" {
                        write_frame_bounded(&w,framing,&json!({"seq":0,"type":"response","request_seq":message["seq"],
                            "command":message["command"],"success":false,"message":"Reverse requests are disabled"}),timeout).await?;
                        continue;
                    }
                    if message["method"]=="textDocument/publishDiagnostics"
                        && let Some(uri)=message["params"]["uri"].as_str() {
                            let mut map=d.lock().map_err(|_|anyhow!("Diagnostics lock poisoned"))?;
                            if map.len()<1024 || map.contains_key(uri) {map.insert(uri.into(),message["params"].clone());}
                    }
                    if message["method"]=="experimental/serverStatus" {
                        *status.lock().map_err(|_|anyhow!("Status lock poisoned"))?=Some(message["params"].clone());
                    }
                    if message["method"]=="$/progress" {
                        let token=message["params"]["token"].to_string();
                        let mut active=progress_events.lock().map_err(|_|anyhow!("Progress lock poisoned"))?;
                        if message["params"]["value"]["kind"]=="end" {active.remove(&token);}
                        else if active.len()<128 || active.contains_key(&token) {active.insert(token,message["params"]["value"].clone());}
                    }
                    let mut queue=e.lock().map_err(|_|anyhow!("Events lock poisoned"))?;
                    if queue.len()>=256 {queue.pop_front();}queue.push_back(message);
                }
                Ok(())
            }.await;
            c.store(true, Ordering::SeqCst);
            let reason = outcome
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Backend process exited".into());
            if let Ok(mut pending) = p.lock() {
                for (_, sender) in pending.drain() {
                    let _ = sender.send(Err(reason.clone()));
                }
            }
        });
        Ok(Arc::new(Self {
            child: Mutex::new(child),
            process_tree: StdMutex::new(Some(process_tree)),
            stdin,
            pending,
            events,
            latest_diagnostics,
            closed,
            next_id: Arc::new(AtomicU64::new(1)),
            cancel_supported: Arc::new(AtomicBool::new(dialect == Dialect::JsonRpc)),
            server_status,
            progress,
            reader,
            framing,
            dialect,
            timeout,
        }))
    }
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    pub fn set_cancel_supported(&self, supported: bool) {
        self.cancel_supported.store(supported, Ordering::SeqCst);
    }
    fn cancellation(&self) -> Cancellation {
        Cancellation {
            pending: self.pending.clone(),
            stdin: self.stdin.clone(),
            framing: self.framing,
            dialect: self.dialect,
            timeout: self.timeout,
            supported: self.cancel_supported.clone(),
            closed: self.closed.clone(),
            next_id: self.next_id.clone(),
        }
    }
    pub fn status(&self) -> Result<Value> {
        Ok(json!({"closed":self.is_closed(),
            "pending_requests":self.pending.lock().map_err(|_|anyhow!("Pending lock poisoned"))?.len(),
            "server_status":self.server_status.lock().map_err(|_|anyhow!("Status lock poisoned"))?.clone(),
            "active_progress":self.progress.lock().map_err(|_|anyhow!("Progress lock poisoned"))?.clone()}))
    }
    /// Called after a protocol-specific graceful exit notification. A backend
    /// that ignores it is killed and reaped; no request is retried.
    pub async fn wait_for_exit(&self) -> Result<()> {
        let result = {
            let mut child = self.child.lock().await;
            tokio::time::timeout(self.timeout, child.wait()).await
        };
        match result {
            Ok(status) => {
                status?;
                self.closed.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(_) => {
                self.terminate().await?;
                bail!("Backend shutdown timed out; process was terminated")
            }
        }
    }
    pub async fn terminate(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.process_tree
            .lock()
            .map_err(|_| anyhow!("Process tree lock poisoned"))?
            .take();
        self.reader.abort();
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_none() {
            child.start_kill()?;
            tokio::time::timeout(self.timeout.max(Duration::from_secs(1)), child.wait())
                .await
                .map_err(|_| anyhow!("Backend process cleanup timed out"))??;
        }
        Ok(())
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.request_signaled(method, params, None).await
    }
    /// Signal only after the complete frame has reached the subprocess pipe.
    /// DAP launch must be sent before a subsequent configurationDone request.
    pub(crate) async fn request_signaled(
        &self,
        method: &str,
        params: Value,
        started: Option<oneshot::Sender<()>>,
    ) -> Result<Value> {
        if self.is_closed() {
            bail!("Backend is closed; explicitly restart it");
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow!("Pending lock poisoned"))?;
            if self.is_closed() {
                bail!("Backend is closed; explicitly restart it");
            }
            if pending.len() >= 64 {
                bail!("Backend request queue is full");
            }
            pending.insert(id, tx);
        }
        let mut registration = PendingRegistration {
            pending: self.pending.clone(),
            id,
            cancellation: None,
        };
        let message = if self.dialect == Dialect::Dap {
            json!({"seq":id,"type":"request","command":method,"arguments":params})
        } else {
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        };
        if let Err(error) = self.send_value(&message).await {
            self.pending
                .lock()
                .map_err(|_| anyhow!("Pending lock poisoned"))?
                .remove(&id);
            return Err(error);
        }
        registration.cancellation = Some(self.cancellation());
        if let Some(started) = started {
            let _ = started.send(());
        }
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(error))) => Err(anyhow!(error)),
            Ok(Err(_)) => Err(anyhow!("Backend response channel closed")),
            Err(_) => {
                self.pending
                    .lock()
                    .map_err(|_| anyhow!("Pending lock poisoned"))?
                    .remove(&id);
                self.cancellation().send(id).await;
                bail!(
                    "Backend timeout; operation outcome may be unknown; mutating calls are NEVER auto-retried"
                )
            }
        }
    }
    async fn send_value(&self, value: &Value) -> Result<()> {
        if self.is_closed() {
            bail!("Backend is closed; explicitly restart it");
        }
        let mut guard = WriteGuard {
            closed: self.closed.clone(),
            pending: self.pending.clone(),
            complete: false,
        };
        let result = write_frame_bounded(&self.stdin, self.framing, value, self.timeout).await;
        guard.complete = result.is_ok();
        if result.is_err() {
            self.closed.store(true, Ordering::SeqCst);
        }
        result
    }
    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        if self.dialect != Dialect::JsonRpc {
            bail!("JSON-RPC notifications are not DAP messages");
        }
        self.send_value(&json!({"jsonrpc":"2.0","method":method,"params":params}))
            .await
    }
    pub fn events(&self) -> Result<Vec<Value>> {
        Ok(self
            .events
            .lock()
            .map_err(|_| anyhow!("Event lock poisoned"))?
            .drain(..)
            .collect())
    }
    pub fn diagnostics(&self, uri: &str) -> Result<Option<Value>> {
        Ok(self
            .latest_diagnostics
            .lock()
            .map_err(|_| anyhow!("Diagnostics lock poisoned"))?
            .get(uri)
            .cloned())
    }
    pub fn invalidate_diagnostics(&self, uri: &str) -> Result<()> {
        self.latest_diagnostics
            .lock()
            .map_err(|_| anyhow!("Diagnostics lock poisoned"))?
            .remove(uri);
        Ok(())
    }
}
impl Drop for RpcProcess {
    fn drop(&mut self) {
        self.reader.abort();
        let _ = self.child.get_mut().start_kill();
    }
}
async fn write_frame_bounded(
    writer: &Mutex<ChildStdin>,
    framing: Framing,
    value: &Value,
    timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout,write_frame(writer,framing,value)).await
        .map_err(|_|anyhow!("Backend write timeout; explicitly restart the backend; operation outcome may be unknown"))?
}
async fn write_frame(writer: &Mutex<ChildStdin>, framing: Framing, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        bail!("Message exceeds frame limit");
    }
    let mut writer = writer.lock().await;
    match framing {
        Framing::ContentLength => {
            writer
                .write_all(format!("Content-Length: {}\r\n\r\n", bytes.len()).as_bytes())
                .await?
        }
        Framing::Newline => {}
    }
    writer.write_all(&bytes).await?;
    if matches!(framing, Framing::Newline) {
        writer.write_all(b"\n").await?;
    }
    writer.flush().await?;
    Ok(())
}
async fn bounded_line<R: AsyncBufRead + Unpin>(reader: &mut R, limit: usize) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Ok(line);
        }
        let n = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(buffer.len());
        if line.len() + n > limit {
            bail!("Line exceeds frame limit");
        }
        let complete = buffer[n - 1] == b'\n';
        line.extend_from_slice(&buffer[..n]);
        reader.consume(n);
        if complete {
            return Ok(line);
        }
    }
}
pub async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    framing: Framing,
) -> Result<Option<Vec<u8>>> {
    if matches!(framing, Framing::Newline) {
        let line = bounded_line(reader, MAX_FRAME).await?;
        if line.is_empty() {
            return Ok(None);
        }
        return Ok(Some(line));
    }
    let mut length = None;
    let mut header_size = 0;
    loop {
        let line = bounded_line(reader, MAX_HEADER).await?;
        if line.is_empty() {
            if header_size == 0 {
                return Ok(None);
            }
            bail!("EOF inside frame header");
        }
        header_size += line.len();
        if header_size > MAX_HEADER {
            bail!("Header too large");
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let line = std::str::from_utf8(&line)?;
        if !line.is_ascii() {
            bail!("Frame headers must be ASCII");
        }
        if !line.contains(':') {
            bail!("Malformed frame header");
        }
        if let Some((name, value)) = line.trim().split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            if length.is_some() {
                bail!("Duplicate Content-Length");
            }
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.ok_or_else(|| anyhow!("Missing Content-Length"))?;
    if length > MAX_FRAME {
        bail!("Frame too large");
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn malformed_headers_are_rejected() {
        for input in [
            b"garbage\r\nContent-Length: 2\r\n\r\n{}".as_slice(),
            b"Content-Length: 2\r\nX-Name: \xff\r\n\r\n{}".as_slice(),
        ] {
            let mut reader = BufReader::new(input);
            assert!(
                read_frame(&mut reader, Framing::ContentLength)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn framed_json_roundtrip() {
        let input = b"Content-Length: 2\r\n\r\n{}";
        let mut r = BufReader::new(&input[..]);
        assert_eq!(
            read_frame(&mut r, Framing::ContentLength).await.unwrap(),
            Some(b"{}".to_vec())
        );
    }
    #[tokio::test]
    async fn duplicate_length_is_rejected() {
        let input = b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}";
        let mut r = BufReader::new(&input[..]);
        assert!(read_frame(&mut r, Framing::ContentLength).await.is_err());
    }
    #[tokio::test]
    async fn truncated_frame_is_rejected() {
        let input = b"Content-Length: 12\r\n\r\n{}";
        let mut r = BufReader::new(&input[..]);
        assert!(read_frame(&mut r, Framing::ContentLength).await.is_err());
    }
    #[tokio::test]
    async fn oversize_frame_is_rejected() {
        let input = b"Content-Length: 999999999\r\n\r\n";
        let mut r = BufReader::new(&input[..]);
        assert!(read_frame(&mut r, Framing::ContentLength).await.is_err());
    }
    #[tokio::test]
    async fn clean_eof() {
        let mut r = BufReader::new(&b""[..]);
        assert!(
            read_frame(&mut r, Framing::ContentLength)
                .await
                .unwrap()
                .is_none()
        );
    }
}
