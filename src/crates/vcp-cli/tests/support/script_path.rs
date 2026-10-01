// SPDX-License-Identifier: Apache-2.0
//! Interpreter arguments need ordinary Windows paths. Canonical paths remain
//! the authority for identity and containment everywhere else in the harness.
use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Component, Path, PathBuf, Prefix},
};

pub fn argument(script: &Path) -> PathBuf {
    let canonical = fs::canonicalize(script).expect("Existing script required");
    assert!(canonical.is_file(), "Script must be a file");
    ordinary_argument(&canonical)
}

pub fn directory_argument(directory: &Path) -> PathBuf {
    assert!(
        directory.is_absolute(),
        "Existing absolute directory required"
    );
    // Do not resolve a redirected input into an otherwise acceptable directory.
    // The receiving setup and editor retain their ordinary local-path guards.
    for ancestor in directory.ancestors() {
        let metadata =
            fs::symlink_metadata(ancestor).expect("Existing directory ancestor required");
        assert!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "Redirected or non-directory interpreter input refused"
        );
    }
    let canonical = fs::canonicalize(directory).unwrap();
    ordinary_argument(&canonical)
}

fn ordinary_argument(canonical: &Path) -> PathBuf {
    let text = canonical.to_str().expect("Interpreter path must be UTF-8");
    // PowerShell preserves the extended prefix in PSScriptRoot; Node's entry
    // resolver then rejects paths derived from it. Inno also rejects namespace
    // prefixes, and VS Code interprets them as UNC URI authorities. Normalize
    // only arguments; the caller retains canonical identity and containment.
    let ordinary = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text));
    assert!(
        ordinary.is_absolute()
            && matches!(ordinary.components().next(), Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(_))),
        "Interpreter input must be drive-local"
    );
    assert_eq!(
        fs::canonicalize(&ordinary).unwrap(),
        canonical,
        "Interpreter argument must select the identical input"
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

    #[test]
    fn directory_arguments_preserve_identity_and_refuse_files_or_missing_paths() {
        let root = tempfile::Builder::new()
            .prefix("VCP directory 空 é ")
            .tempdir()
            .unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let ordinary = directory_argument(&canonical);
        assert!(!ordinary.to_str().unwrap().starts_with(r"\\?\"));
        assert_eq!(ordinary.canonicalize().unwrap(), canonical);
        let file = canonical.join("keep.txt");
        fs::write(&file, b"unchanged directory evidence").unwrap();
        assert!(std::panic::catch_unwind(|| directory_argument(&file)).is_err());
        assert!(
            std::panic::catch_unwind(|| directory_argument(&canonical.join("missing"))).is_err()
        );
        assert_eq!(fs::read(file).unwrap(), b"unchanged directory evidence");
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
        let workspace = repo.join("workspace 空 é");
        let data = repo.join("data 空 é");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(workspace.join("keep.txt"), b"workspace sentinel").unwrap();
        fs::write(data.join("keep.txt"), b"data sentinel").unwrap();
        fs::write(
            &entry,
            r#"const fs=require('node:fs'),path=require('node:path');
const [workspace,data]=process.argv.slice(2);
process.stdout.write(JSON.stringify({executed:true,filename:__filename,
workspace:fs.realpathSync.native(workspace),data:fs.realpathSync.native(data),
workspaceSentinel:fs.readFileSync(path.join(workspace,'keep.txt'),'utf8'),
dataSentinel:fs.readFileSync(path.join(data,'keep.txt'),'utf8')}));"#,
        )
        .unwrap();
        fs::write(
            &script,
            r#"param([string]$Node,[string]$Workspace,[string]$DataRoot)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
foreach($directory in @($Workspace,$DataRoot)){
    if($directory -notmatch '^[A-Za-z]:\\' -or $directory.Contains('?') -or -not [IO.Directory]::Exists($directory)){throw 'Ordinary existing local directory required'}
}
$redirected=Join-Path $repo 'redirected-data'
New-Item -ItemType Junction -Path $redirected -Target $DataRoot | Out-Null
$info=[Diagnostics.ProcessStartInfo]::new($Node)
$info.UseShellExecute=$false; $info.CreateNoWindow=$true
$info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
$info.StandardOutputEncoding=[Text.UTF8Encoding]::new($false)
$info.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
$info.ArgumentList.Add((Join-Path $repo 'entry.cjs'))
$info.ArgumentList.Add($Workspace);$info.ArgumentList.Add($DataRoot)
$process=[Diagnostics.Process]::Start($info)
$stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
if(-not $process.WaitForExit(10000)){$process.Kill($true);throw 'Node deadline exceeded'}
if($process.ExitCode -ne 0){throw ('Node entrypoint failed: '+$stderr.GetAwaiter().GetResult())}
@{script_root=$PSScriptRoot;repo=$repo;workspace=$Workspace;data=$DataRoot;redirected=$redirected;node=($stdout.GetAwaiter().GetResult()|ConvertFrom-Json)}|ConvertTo-Json -Compress -EscapeHandling EscapeNonAscii
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
            .arg("-Workspace")
            .arg(directory_argument(&workspace.canonicalize().unwrap()))
            .arg("-DataRoot")
            .arg(directory_argument(&data.canonicalize().unwrap()))
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
            (&observed["workspace"], &workspace),
            (&observed["data"], &data),
            (&observed["node"]["workspace"], &workspace),
            (&observed["node"]["data"], &data),
        ] {
            let value = value.as_str().unwrap();
            assert!(!value.starts_with(r"\\?\"));
            assert_eq!(
                Path::new(value).canonicalize().unwrap(),
                expected.canonicalize().unwrap()
            );
        }
        assert_eq!(observed["node"]["workspaceSentinel"], "workspace sentinel");
        assert_eq!(observed["node"]["dataSentinel"], "data sentinel");
        let redirected = Path::new(observed["redirected"].as_str().unwrap());
        assert!(std::panic::catch_unwind(|| directory_argument(redirected)).is_err());
        fs::remove_dir(redirected).unwrap(); // Remove only the owned junction.
        assert_eq!(fs::read(data.join("keep.txt")).unwrap(), b"data sentinel");
    }
}
