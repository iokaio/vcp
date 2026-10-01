// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Explicit installed-helper qualification. No installation, acquisition,
//! provider/model calls, or canonical-store fixtures are created by this target.
#[path = "support/hidden_process.rs"]
mod hidden_process;
#[path = "support/script_path.rs"]
mod script_path;
use codex_utils_pty::JobObject;
use serde_json::{json, Value};
use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;

fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn input(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect(name));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn plain(path: &Path) {
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            assert_eq!(
                fs::symlink_metadata(ancestor).unwrap().file_attributes() & 0x400,
                0,
                "Redirected helper input or output refused"
            );
        }
    }
}
fn overlaps_sync(path: &Path, sync: &Path) -> bool {
    let folded = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    };
    let selected = folded(path);
    let sync = folded(sync);
    selected == sync
        || selected.starts_with(&(sync.clone() + "\\"))
        || sync.starts_with(&(selected + "\\"))
}
fn fresh_private(path: &Path, repo: &Path) {
    assert!(path.is_absolute() && !path.exists());
    assert!(!path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir)));
    plain(path);
    let parent = path.parent().unwrap().canonicalize().unwrap();
    assert!(!parent.starts_with(repo));
    for ancestor in parent.ancestors() {
        assert!(
            !ancestor.join(".git").exists(),
            "Private helper output must be outside checkouts"
        );
    }
    let selected = parent.join(path.file_name().unwrap());
    for name in [
        "OneDrive",
        "OneDriveConsumer",
        "OneDriveCommercial",
        "VCP_TEST_SYNC_ROOT",
    ] {
        if let Some(sync) = std::env::var_os(name) {
            let sync = PathBuf::from(sync);
            let sync = sync.canonicalize().unwrap_or(sync);
            assert!(
                !overlaps_sync(&selected, &sync),
                "Private helper output overlaps a sync root"
            );
        }
    }
    fs::create_dir(path).unwrap();
}

#[test]
fn private_output_excludes_sync_path_spelling_variants() {
    for output in [
        r"C:\Owned\Sync\child",
        "c:/owned/SYNC/child",
        r"\\?\C:\Owned\Sync\child",
    ] {
        assert!(overlaps_sync(
            Path::new(output),
            Path::new(r"C:\Owned\Sync")
        ));
        assert!(overlaps_sync(
            Path::new(r"C:\Owned\Sync"),
            Path::new(output)
        ));
    }
    assert!(overlaps_sync(
        Path::new("C:/Owned/Sync/"),
        Path::new(r"C:\Owned\Sync")
    ));
    assert!(!overlaps_sync(
        Path::new(r"C:\Owned\Sync-other"),
        Path::new(r"C:\Owned\Sync")
    ));
}
async fn observe(command: &mut Command, private: &Path, seconds: u64) -> Value {
    let stdout = private.join("runner.stdout");
    let stderr = private.join("runner.stderr");
    command
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap());
    let job = JobObject::create_without_breakaway().unwrap();
    let mut child = hidden_process::spawn(&job, command).await.unwrap();
    let began = Instant::now();
    let output_bytes =
        || fs::metadata(&stdout).unwrap().len() + fs::metadata(&stderr).unwrap().len();
    let mut exceeded = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if began.elapsed() >= Duration::from_secs(seconds) || output_bytes() > 4 * 1024 * 1024 {
            exceeded = true;
            break None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let grace = Instant::now();
    while !exceeded
        && job.active_process_count().unwrap() != 0
        && grace.elapsed() < Duration::from_secs(5)
    {
        if output_bytes() > 4 * 1024 * 1024 {
            exceeded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let forced = job.active_process_count().unwrap() != 0;
    if forced {
        job.terminate().unwrap();
    }
    let cleanup = Instant::now();
    while job.active_process_count().unwrap() != 0 && cleanup.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stopped = job.active_process_count().unwrap() == 0;
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    exceeded |= output_bytes() > 4 * 1024 * 1024;
    let passed = status.is_some_and(|status| status.success()) && stopped && !forced && !exceeded;
    let result = json!({"schema":"vcp-helper-process-observation/1", "status":if passed {"pass"} else {"fail"},
        "runner_deadline_seconds":seconds,"descendant_grace_seconds":5,"runner_wall_ms":began.elapsed().as_millis(),
        "exit_code":status.and_then(|status| status.code()),"output_limit_bytes":4*1024*1024,"output_bytes":output_bytes(),
        "job_active_processes_zero":stopped,"forced_descendant_cleanup":forced,"limit_exceeded":exceeded});
    save(&private.join("supervision.json"), &result);
    result
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helper_supervision_refuses_retained_descendants_and_output_overflow() {
    let node = input("VCP_TEST_NODE");
    for (name, source, expected, forced) in [
        ("natural", "require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},500)'],{stdio:'ignore',windowsHide:true,detached:true}).unref()", "pass", false),
        ("retained", "require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},120000)'],{stdio:'ignore',windowsHide:true,detached:true}).unref()", "fail", true),
        ("oversized", "process.stdout.write(Buffer.alloc(5*1024*1024))", "fail", false),
    ] {
        let private = tempfile::tempdir().unwrap();
        let mut command = Command::new(&node); command.args(["-e", source]);
        let result = observe(&mut command, private.path(), 10).await;
        assert_eq!(result["status"], expected, "{name}: {result}");
        assert_eq!(result["job_active_processes_zero"], true, "{name}");
        if name != "oversized" { assert_eq!(result["forced_descendant_cleanup"], forced, "{name}"); }
        else { assert_eq!(result["limit_exceeded"], true); }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit final installed production payload, existing helper dependencies and private output required"]
async fn final_installed_helpers_preserve_payload_and_drain_process_tree() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let private = input("VCP_BETA_HELPER_OUTPUT");
    fresh_private(&private, &repo);
    // Retain private files and the independently owned installation on failure.
    let mut report = json!({"schema":"vcp-installed-helper-qualification/1","status":"fail"});
    let script_path_source = repo.join("src/crates/vcp-cli/tests/support/script_path.rs");
    let script_path_hash = || {
        vcp_protocol::digest_reader(fs::File::open(&script_path_source).unwrap())
            .unwrap()
            .0
    };
    report["script_path_source_sha256"] = json!(script_path_hash());
    save(&private.join("result.json"), &report);
    let engine = input("VCP_BETA_INSTALLED_EXECUTABLE");
    let engine_root = engine.parent().unwrap().parent().unwrap().parent().unwrap();
    let held = vcp_cli::installation::select(engine_root).unwrap();
    assert_eq!(
        held.executable.canonicalize().unwrap(),
        engine.canonicalize().unwrap()
    );
    let pwsh = input("VCP_TEST_PWSH");
    plain(&pwsh);
    let mut command = Command::new(&pwsh);
    command.env_clear();
    for name in [
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "OneDrive",
        "OneDriveConsumer",
        "OneDriveCommercial",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let system = input("SystemRoot");
    command
        .env(
            "PATH",
            std::env::join_paths([system.join("System32"), system]).unwrap(),
        )
        .env("TEMP", &private)
        .env("TMP", &private)
        .args(["-NoProfile", "-File"])
        .arg(script_path::argument(
            &repo.join("scripts/release/helper-qualification.ps1"),
        ));
    for (flag, name) in [
        ("-NativeResult", "VCP_BETA_NATIVE_RESULT"),
        ("-InstalledEngine", "VCP_BETA_INSTALLED_EXECUTABLE"),
        ("-Python", "VCP_TEST_PYTHON"),
        ("-Node", "VCP_TEST_NODE"),
        ("-BrowserProject", "VCP_TEST_BROWSER_PROJECT"),
    ] {
        let path = input(name);
        plain(&path);
        command.arg(flag).arg(path);
    }
    command
        .arg("-OutputRoot")
        .arg(private.join("observations"))
        .arg("-QualificationExecutable")
        .arg(std::env::current_exe().unwrap())
        .current_dir(&private);
    if let Some(sync) = std::env::var_os("VCP_TEST_SYNC_ROOT") {
        command.arg("-SyncRoots").arg(sync);
    }
    let supervision = observe(&mut command, &private, 900).await;
    report["process_tree"] = supervision.clone();
    save(&private.join("result.json"), &report);
    assert_eq!(
        supervision["status"],
        "pass",
        "Private helper evidence retained at {}",
        private.display()
    );
    let observed: Value =
        serde_json::from_slice(&fs::read(private.join("runner.stdout")).unwrap()).unwrap();
    report["observations"] = observed.clone();
    save(&private.join("result.json"), &report);
    assert_eq!(observed["schema"], "vcp-installed-helper-observations/1");
    assert_eq!(observed["status"], "observations-passed");
    assert_eq!(observed["complete_payload_before_after"], true);
    assert_eq!(observed["dependencies_unchanged"], true);
    let cases = observed["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for (case, name) in cases
        .iter()
        .zip(["pdf", "spreadsheet", "mcp", "authoring", "browser"])
    {
        assert_eq!(case["id"], name);
        assert_eq!(case["status"], "pass");
    }
    for (index, expected) in [(0, 4), (1, 5), (2, 7)] {
        assert_eq!(cases[index]["observation"]["tests_run"], expected);
    }
    let after = vcp_cli::installation::select(engine_root).unwrap();
    assert_eq!(held.executable, after.executable);
    assert_eq!(held.data, after.data);
    assert_eq!(report["script_path_source_sha256"], script_path_hash());
    report["status"] = json!("pass");
    save(&private.join("result.json"), &report);
    eprintln!(
        "Installed helper observations passed; private receipt {}",
        private.join("result.json").display()
    );
}
