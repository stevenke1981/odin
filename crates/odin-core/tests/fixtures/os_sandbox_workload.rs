//! Trusted native sandbox probe. Compiled by integration tests; not part of Odin runtime.
use std::{
    env, fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("echo") => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            io::stdout().write_all(&bytes)?;
            io::stderr().write_all(b"fixture-stderr\n")?;
        }
        Some("check") => {
            let readable = &args[1];
            let writable = &args[2];
            let denied = &args[3];
            let outside = &args[4];
            let read_allowed = fs::read_to_string(format!("{readable}/allowed.txt"))? == "allowed";
            fs::write(format!("{writable}/created.txt"), b"created")?;
            let write_allowed = fs::read(format!("{writable}/created.txt"))? == b"created";
            let readonly_write_denied =
                fs::write(format!("{readable}/should-not-exist.txt"), b"bad").is_err()
                    && fs::write(format!("{readable}/allowed.txt"), b"bad").is_err();
            let denied_read = fs::read(format!("{denied}/secret.txt")).is_err();
            let denied_write = fs::write(format!("{denied}/should-not-exist.txt"), b"bad").is_err();
            let outside_read = fs::read(format!("{outside}/outside.txt")).is_err();
            let outside_write =
                fs::write(format!("{outside}/should-not-exist.txt"), b"bad").is_err();
            let outside_execute_denied = Command::new(format!("{outside}/workload.exe"))
                .args(["exit", "0"])
                .status()
                .is_err();
            let junction_read_denied =
                fs::read(format!("{readable}/junction/outside.txt")).is_err();
            let environment_filtered = env::var_os("ODIN_MXC_EXECUTOR").is_none();
            let private_temp = env::temp_dir();
            // Native MXC sets a per-package TEMP below the private grant but
            // does not create it; creating directories remains confined.
            fs::create_dir_all(&private_temp)?;
            let scratch_result = fs::write(private_temp.join("scratch.txt"), b"private");
            let scratch_write = scratch_result.is_ok();
            if !scratch_write {
                eprintln!(
                    "scratch diagnostic temp_dir={private_temp:?}; TEMP={:?}; TMP={:?}; result={scratch_result:?}",
                    env::var_os("TEMP"),
                    env::var_os("TMP")
                );
            }
            let address: SocketAddr = format!("127.0.0.1:{}", args[5])
                .parse()
                .expect("trusted test port");
            let network_denied =
                TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err();
            println!(
                "{{\"read_allowed\":{read_allowed},\"write_allowed\":{write_allowed},\"readonly_write_denied\":{readonly_write_denied},\"denied_read\":{denied_read},\"denied_write\":{denied_write},\"outside_read\":{outside_read},\"outside_write\":{outside_write},\"outside_execute_denied\":{outside_execute_denied},\"junction_read_denied\":{junction_read_denied},\"environment_filtered\":{environment_filtered},\"scratch_write\":{scratch_write},\"network_denied\":{network_denied}}}"
            );
        }
        Some("child-check") => {
            let status = Command::new(env::current_exe()?)
                .arg("check")
                .args(&args[1..])
                .status()?;
            std::process::exit(status.code().unwrap_or(99));
        }
        Some("tree") => {
            let child = Command::new(env::current_exe()?)
                .args(["heartbeat", &args[1], &args[2]])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            println!(
                "{{\"parent_pid\":{},\"child_pid\":{}}}",
                std::process::id(),
                child.id()
            );
            io::stdout().flush()?;
            loop {
                thread::sleep(Duration::from_millis(20));
            }
        }
        Some("heartbeat") => {
            let outside_denied = fs::read(&args[2]).is_err();
            for counter in 0u64.. {
                fs::write(&args[1], format!("{counter}:{outside_denied}"))?;
                thread::sleep(Duration::from_millis(20));
            }
        }
        Some("exit") => std::process::exit(args[1].parse::<i32>().expect("trusted exit code")),
        _ => std::process::exit(87),
    }
    Ok(())
}
