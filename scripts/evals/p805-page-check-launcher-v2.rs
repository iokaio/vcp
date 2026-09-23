// SPDX-License-Identifier: Apache-2.0
// Future qualification only; adapted from the frozen p8-owner-v3 launcher.
// Accept the visible fixture command and the host verification command, then
// normalize both to one pinned, permission-fenced Node invocation. This does
// not change the frozen fixture, prior cohort or its acceptance outcome.
use std::{
    env,
    ffi::OsString,
    process::{Command, ExitCode, Stdio},
};

const VISIBLE: &[&str] = &["--test", "test/page.test.cjs"];
const CANONICAL: &[&str] = &[
    "--test",
    "--test-reporter=tap",
    "--test-concurrency=1",
    "test/page.test.cjs",
];

fn normalized(args: &[OsString]) -> Option<&'static [&'static str]> {
    [VISIBLE, CANONICAL]
        .iter()
        .any(|expected| {
            args.len() == expected.len() && args.iter().zip(*expected).all(|(a, b)| a == b)
        })
        .then_some(CANONICAL)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("U03 check launcher v2: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let args = normalized(&args).ok_or("only the two declared U03 test commands are permitted")?;
    let workspace = env::current_dir()?.canonicalize()?;
    let workspace = workspace.to_str().ok_or("UTF-8 workspace required")?;
    let workspace = workspace.strip_prefix(r"\\?\").unwrap_or(workspace);
    if workspace.starts_with("UNC\\") || workspace.starts_with(r"\\") {
        return Err("local qualification workspace required".into());
    }
    let mut command = Command::new(env!("VCP_P805_NODE"));
    command
        .args([
            "--permission",
            "--test-isolation=none",
            "--max-old-space-size=64",
        ])
        .arg(format!("--allow-fs-read={workspace}"))
        .args(args)
        .env_clear()
        .env("SystemRoot", env!("VCP_P805_SYSTEMROOT"))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    Ok(if command.status()?.success() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn declared_forms_normalize_to_identical_execution() {
        assert_eq!(normalized(&arguments(VISIBLE)), Some(CANONICAL));
        assert_eq!(normalized(&arguments(CANONICAL)), Some(CANONICAL));
    }

    #[test]
    fn every_other_command_or_added_authority_is_rejected() {
        for args in [
            vec![],
            vec!["--version"],
            vec!["--eval", "process.exit(0)"],
            vec!["--test", "../test/page.test.cjs"],
            vec!["--test", "test\\page.test.cjs"],
            vec!["--test", "test/other.cjs"],
            vec!["--test", "test/page.test.cjs", "--allow-net"],
            vec![
                "--test",
                "--test-concurrency=1",
                "--test-reporter=tap",
                "test/page.test.cjs",
            ],
            vec![
                "--test",
                "--test-reporter=tap",
                "--test-concurrency=2",
                "test/page.test.cjs",
            ],
            vec!["--test", "test/page.test.cjs; calc.exe"],
        ] {
            assert_eq!(normalized(&arguments(&args)), None, "{args:?}");
        }
        for extra in [
            "--allow-net",
            "--allow-child-process",
            "--allow-fs-write=*",
            "--import=evil.cjs",
        ] {
            let mut args = arguments(CANONICAL);
            args.push(extra.into());
            assert_eq!(normalized(&args), None);
        }
    }
}
