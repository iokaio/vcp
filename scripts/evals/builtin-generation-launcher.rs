// SPDX-License-Identifier: Apache-2.0
// Qualification-only executable; never install as a general tool. The prepared
// plan pins the binary, reference source, and portable Node bytes. The optional
// local build receipt binds the observed compiler invocation and embedded paths;
// without that receipt, binary build provenance remains unverified.
use std::{
    env,
    process::{Command, ExitCode, Stdio},
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("U03 check launcher: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let expected = [
        "--test",
        "--test-reporter=tap",
        "--test-concurrency=1",
        "test/cart.test.cjs",
    ];
    if args.len() != expected.len() || args.iter().zip(expected).any(|(a, b)| a != b) {
        return Err("only the frozen U03 verification command is permitted".into());
    }
    let workspace = env::current_dir()?.canonicalize()?;
    let workspace = workspace.to_str().ok_or("UTF-8 workspace required")?;
    // Node's permission parser does not normalize Windows extended path syntax.
    let workspace = workspace.strip_prefix(r"\\?\").unwrap_or(workspace);
    if workspace.starts_with("UNC\\") || workspace.starts_with(r"\\") {
        return Err("local qualification workspace required".into());
    }
    let read_grant = format!("--allow-fs-read={workspace}");
    let status = Command::new(env!("VCP_U03_NODE"))
        .args([
            "--permission",
            "--test-isolation=none",
            "--max-old-space-size=64",
        ])
        .arg(read_grant)
        .args(args)
        .env_clear()
        .env("SystemRoot", env!("VCP_U03_SYSTEMROOT"))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(if status.success() { 0 } else { 1 })
}
