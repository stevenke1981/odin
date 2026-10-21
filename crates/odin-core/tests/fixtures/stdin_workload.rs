//! Fixed process-pipe fixture; this is not a language/formatter backend.
use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("echo") => {
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input)?;
            io::stdout().write_all(&input)?;
        }
        Some("duplex") => {
            io::stdout().write_all(&vec![b'x'; 262_144])?;
            io::stderr().write_all(&vec![b'e'; 262_144])?;
            io::stdout().flush()?;
            io::stderr().flush()?;
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input)?;
            io::stdout().write_all(&input)?;
        }
        Some("invalid") => {
            io::stdin().read_to_end(&mut Vec::new())?;
            io::stdout().write_all(b"\xff\xfe")?;
            io::stderr().write_all(b"\xff")?;
        }
        Some("exit") => std::process::exit(23),
        Some("early-success") => io::stdout().write_all(b"partial")?,
        Some("no-read") => std::thread::sleep(std::time::Duration::from_secs(60)),
        _ => std::process::exit(87),
    }
    Ok(())
}
