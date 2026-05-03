use std::io::Result;
use std::process::Command;

fn main() -> Result<()> {
    let sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=proto/denpie.proto");
    println!("cargo:rustc-env=DENPIE_BUILD_SHA={sha}");
    prost_build::compile_protos(&["proto/denpie.proto"], &["proto/"])?;
    Ok(())
}
