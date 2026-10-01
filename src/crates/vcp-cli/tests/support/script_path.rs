// SPDX-License-Identifier: Apache-2.0
//! Interpreter arguments need ordinary Windows paths. Canonical paths remain
//! the authority for identity and containment everywhere else in the harness.
use std::{
    fs,
    path::{Component, Path, PathBuf, Prefix},
};

pub fn argument(script: &Path) -> PathBuf {
    let canonical = fs::canonicalize(script).expect("Existing script required");
    assert!(canonical.is_file(), "Script must be a file");
    let text = canonical.to_str().expect("Script path must be UTF-8");
    // PowerShell preserves the extended prefix in PSScriptRoot; Node's entry
    // resolver then rejects paths derived from it. Normalize only this argument.
    let ordinary = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text));
    assert!(
        ordinary.is_absolute()
            && matches!(ordinary.components().next(), Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(_))),
        "Script must be drive-local"
    );
    assert_eq!(
        fs::canonicalize(&ordinary).unwrap(),
        canonical,
        "Interpreter argument must select the identical script"
    );
    ordinary
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_utils_pty::JobObject;
    use std::{process::Stdio, time::Duration};
    use tokio::process::Command;

    #[test]
    fn refuses_missing_script_and_directory() {
        let root = tempfile::tempdir().unwrap();
        assert!(std::panic::catch_unwind(|| argument(root.path())).is_err());
        assert!(std::panic::catch_unwind(|| argument(&root.path().join("missing.ps1"))).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn powershell_to_node_preserves_spaces_unicode_and_script_identity() {
        let node = PathBuf::from(std::env::var_os("VCP_TEST_NODE").expect("Pinned Node required"));
        let pwsh =
            PathBuf::from(std::env::var_os("VCP_TEST_PWSH").expect("Pinned PowerShell required"));
        assert!(node.is_absolute() && node.is_file() && pwsh.is_absolute() && pwsh.is_file());
        let private = tempfile::Builder::new()
            .prefix("VCP script path 空 é ")
            .tempdir()
            .unwrap();
        let repo = private.path().join("fixture repo");
        let scripts = repo.join("scripts/release");
        fs::create_dir_all(&scripts).unwrap();
        let script = scripts.join("probe.ps1");
        let entry = repo.join("entry.cjs");
        fs::write(
            &entry,
            r#"process.stdout.write(JSON.stringify({executed:true,filename:__filename}));"#,
        )
        .unwrap();
        fs::write(
            &script,
            r#"param([string]$Node)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$info=[Diagnostics.ProcessStartInfo]::new($Node)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true
$info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
$info.StandardOutputEncoding=[Text.UTF8Encoding]::new($false)
$info.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
$info.ArgumentList.Add((Join-Path $repo 'entry.cjs'))
$process=[Diagnostics.Process]::Start($info)
$stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
if(-not $process.WaitForExit(10000)){$process.Kill($true);throw 'Node deadline exceeded'}
if($process.ExitCode -ne 0){throw ('Node entrypoint failed: '+$stderr.GetAwaiter().GetResult())}
@{script_root=$PSScriptRoot;repo=$repo;node=($stdout.GetAwaiter().GetResult()|ConvertFrom-Json)}|ConvertTo-Json -Compress -EscapeHandling EscapeNonAscii
"#,
        )
        .unwrap();
        let canonical = script.canonicalize().unwrap();
        assert!(canonical.to_str().unwrap().starts_with(r"\\?\"));
        let selected = argument(&canonical);
        assert!(!selected.to_str().unwrap().starts_with(r"\\?\"));
        assert_eq!(selected.canonicalize().unwrap(), canonical);
        let stdout = private.path().join("stdout.json");
        let stderr = private.path().join("stderr.log");
        let mut command = Command::new(pwsh);
        command.env_clear();
        for name in [
            "SystemRoot",
            "WINDIR",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .env("TEMP", private.path())
            .env("TMP", private.path())
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(selected)
            .arg("-Node")
            .arg(node)
            .current_dir(private.path())
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let job = JobObject::create_without_breakaway().unwrap();
        let mut child = crate::hidden_process::spawn(&job, &mut command)
            .await
            .unwrap();
        let completion = tokio::time::timeout(Duration::from_secs(20), async {
            let status = child.wait().await.unwrap();
            while job.active_process_count().unwrap() != 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            status
        })
        .await;
        if completion.is_err() {
            job.terminate().unwrap();
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        }
        if !completion.is_ok_and(|status| status.success()) {
            let retained = private.keep();
            panic!(
                "Synthetic PowerShell-to-Node probe failed; fixture retained at {}: {}",
                retained.display(),
                String::from_utf8_lossy(&fs::read(stderr).unwrap())
            );
        }
        assert_eq!(job.active_process_count().unwrap(), 0);
        let observed: serde_json::Value =
            serde_json::from_slice(&fs::read(stdout).unwrap()).unwrap();
        assert_eq!(observed["node"]["executed"], true);
        for (value, expected) in [
            (&observed["script_root"], &scripts),
            (&observed["repo"], &repo),
            (&observed["node"]["filename"], &entry),
        ] {
            let value = value.as_str().unwrap();
            assert!(!value.starts_with(r"\\?\"));
            assert_eq!(
                Path::new(value).canonicalize().unwrap(),
                expected.canonicalize().unwrap()
            );
        }
    }
}
