use crate::rpc::{RpcProcess,Framing,Dialect};
use anyhow::{Result,anyhow,bail};
use odin_core::{config::CommandConfig,paths::SafeRoot};
use serde_json::{Value,json};
use std::{collections::BTreeMap,sync::Arc,time::Duration};
use tokio::{sync::{Mutex,oneshot},task::JoinHandle};

pub struct DebugAdapter {
    rpc:Arc<RpcProcess>,
    pub capabilities:Value,
    launches:Mutex<BTreeMap<String,JoinHandle<Result<Value>>>>,
}
impl DebugAdapter {
    pub async fn start(config:&CommandConfig,root:&SafeRoot)->Result<Arc<Self>> {
        let rpc=RpcProcess::spawn(&config.program,&config.args,root.root(),Framing::ContentLength,Dialect::Dap,
            Duration::from_secs(config.timeout_secs),json!([]),Value::Null)?;
        let capabilities=rpc.request("initialize",json!({"clientID":"odin","clientName":"Odin","adapterID":"configured-adapter",
            "locale":"en-US","linesStartAt1":true,"columnsStartAt1":true,"pathFormat":"path",
            "supportsVariableType":true,"supportsRunInTerminalRequest":false,"supportsProgressReporting":false})).await?;
        Ok(Arc::new(Self{rpc,capabilities,launches:Mutex::new(BTreeMap::new())}))
    }
    pub async fn request(&self,command:&str,args:Value)->Result<Value>{
        const ALLOWED:&[&str]=&["launch","attach","setBreakpoints","setFunctionBreakpoints","setExceptionBreakpoints",
            "configurationDone","threads","stackTrace","scopes","variables","evaluate","continue","next","stepIn","stepOut",
            "pause","disconnect","terminate","restart","source","loadedSources","exceptionInfo","setVariable"];
        if !ALLOWED.contains(&command){bail!("Unsupported DAP command; no raw command execution fallback");}
        if !args.is_object(){bail!("DAP arguments must be an object");}
        // DAP launch/attach can wait for configurationDone. Never block the
        // only request path while the client still has to send that command.
        if command=="launch"||command=="attach"{
            let mut jobs=self.launches.lock().await;if jobs.len()>=8{bail!("Drain launch tickets before starting another launch");}
            let ticket=format!("launch-{}",odin_core::unix_ms());
            if jobs.contains_key(&ticket){bail!("Launch ticket collision; retry later");}
            let rpc=self.rpc.clone();let command=command.to_string();
            let (started_tx,started_rx)=oneshot::channel();
            let job=tokio::spawn(async move{rpc.request_signaled(&command,args,Some(started_tx)).await});
            if started_rx.await.is_err(){
                return match job.await {Ok(Err(e))=>Err(e),Err(e)=>Err(e.into()),_=>Err(anyhow!("Launch frame was not sent"))};
            }
            jobs.insert(ticket.clone(),job);
            return Ok(json!({"ticket":ticket,"pending":true,"next":"Poll events, configure breakpoints, then send configurationDone if required"}));
        }
        self.rpc.request(command,args).await
    }
    pub async fn poll(&self)->Result<Value>{
        let mut jobs=self.launches.lock().await;let completed=jobs.iter().filter(|(_,j)|j.is_finished()).map(|(id,_)|id.clone()).collect::<Vec<_>>();
        let mut results=Vec::new();
        for id in completed{
            let job=jobs.remove(&id).ok_or_else(||anyhow!("Missing launch ticket"))?;
            let result=match job.await{Ok(Ok(v))=>json!({"ok":true,"result":v}),Ok(Err(e))=>json!({"ok":false,"error":e.to_string()}),Err(e)=>json!({"ok":false,"error":e.to_string()})};
            results.push(json!({"ticket":id,"outcome":result}));
        }
        Ok(json!({"events":self.rpc.events()?,"launch_results":results,"pending_tickets":jobs.keys().collect::<Vec<_>>()}))
    }
}
impl Drop for DebugAdapter{
    fn drop(&mut self){for(_,job)in std::mem::take(self.launches.get_mut()){job.abort();}}
}
