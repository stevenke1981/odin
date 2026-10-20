use std::process::Command;

fn output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty() && !value.contains(['\r', '\n'])).then_some(value)
}

fn main() {
    // Build identity is informational. A source archive without .git stays UNKNOWN.
    let revision = output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "UNKNOWN".into());
    let dirty = match Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--"])
        .status()
        .ok()
        .and_then(|s| s.code())
    {
        Some(0) => "false",
        Some(1) => "true",
        _ => "UNKNOWN",
    };
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let compiler = output(&rustc, &["--version"]).unwrap_or_else(|| "UNKNOWN".into());
    println!("cargo:rustc-env=ODIN_BUILD_REVISION={revision}");
    println!("cargo:rustc-env=ODIN_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=ODIN_BUILD_COMPILER={compiler}");
    println!(
        "cargo:rustc-env=ODIN_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=ODIN_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_default()
    );
    // Rerun when tracked Rust/manifests or the checked-out revision change.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=../../.git");
    for name in ["HEAD", "index"] {
        if let Some(path) = output("git", &["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = output("git", &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = output("git", &["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}
