// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Final installed bytes only. Tests real Ctrl+C/Ctrl+Break at the read-only
//! paused-task chooser, not cancellation/draining of active provider work.
#[path = "support/hidden_process.rs"]
mod hidden_process;
#[path = "support/launcher_console.rs"]
mod launcher_console;
#[path = "support/local_fixture.rs"]
mod local_fixture;

use codex_utils_pty::JobObject;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use vcp_store::BackendKind;

fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect(name));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn hash(path: &Path) -> String {
    vcp_protocol::digest_reader(fs::File::open(path).unwrap())
        .unwrap()
        .0
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn archive(result: &Path, name: &Value) -> PathBuf {
    let name = name.as_str().unwrap();
    assert!(!name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']));
    result.parent().unwrap().join(name)
}

fn node_script_argument(script: &Path) -> PathBuf {
    let canonical = fs::canonicalize(script).unwrap();
    let text = canonical.to_str().expect("Node script path must be UTF-8");
    // Node's CommonJS entry resolver does not accept Rust's Windows extended
    // prefix. Keep canonical identities elsewhere; change this argument only.
    let ordinary = PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text));
    assert!(ordinary.is_absolute(), "Node script must be drive-local");
    assert_eq!(fs::canonicalize(&ordinary).unwrap(), canonical);
    ordinary
}

fn verifier_reason(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr);
    // Only fixed categories enter public test logs. Raw stderr may contain
    // private paths and remains in the private evidence directory.
    if text.contains("EISDIR") && text.contains("node:internal/modules/") {
        "node-entry-resolution"
    } else if text
        .contains("Production artifact rejected: Distribution payload differs from manifest")
    {
        "distribution-payload-mismatch"
    } else if text.contains("Production artifact rejected:") {
        "production-artifact-rejected"
    } else if text.contains("node:internal/") {
        "node-runtime-error"
    } else {
        "unclassified-verifier-output"
    }
}

async fn observe_verifier(
    node: &Path,
    script: &Path,
    arguments: &[&Path],
    output: &Path,
    suffix: &str,
) -> Value {
    let stdout = output.join(format!("payload-{suffix}.stdout"));
    let stderr = output.join(format!("payload-{suffix}.stderr"));
    let stdout_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout)
        .unwrap();
    let mut stderr_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&stderr)
        .unwrap();
    let mut command = tokio::process::Command::new(node);
    command
        .arg(node_script_argument(script))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(stdout_file.try_clone().unwrap())
        .stderr(stderr_file.try_clone().unwrap());
    launcher_console::clean_environment(&mut command);
    let job = JobObject::create_without_breakaway().unwrap();
    let mut child = hidden_process::spawn(&job, &mut command).await.unwrap();
    let began = Instant::now();
    let mut limit = None;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if began.elapsed() >= Duration::from_secs(90) {
            limit = Some("deadline");
            break None;
        }
        if stdout_file.metadata().unwrap().len() + stderr_file.metadata().unwrap().len()
            > 1024 * 1024
        {
            limit = Some("output-limit");
            break None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let clean = launcher_console::drain(&job, Duration::from_secs(2))
        .await
        .unwrap();
    if !clean {
        job.terminate().unwrap();
    }
    let stopped = launcher_console::drain(&job, Duration::from_secs(10))
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let stdout_bytes = stdout_file.metadata().unwrap().len();
    let stderr_bytes = stderr_file.metadata().unwrap().len();
    let success = status.is_some_and(|value| value.success())
        && clean
        && stopped
        && stdout_bytes + stderr_bytes <= 1024 * 1024;
    let mut diagnostic = Vec::new();
    stderr_file.seek(SeekFrom::Start(0)).unwrap();
    stderr_file
        .take(16 * 1024)
        .read_to_end(&mut diagnostic)
        .unwrap();
    json!({"status":if success { "pass" } else { "fail" },
        "exit_code":status.and_then(|value| value.code()),"limit":limit,
        "natural_tree_exit":clean,"tree_stopped":stopped,"forced_cleanup":!clean,
        "stdout_bytes":stdout_bytes,"stderr_bytes":stderr_bytes,
        "reason":if success { "verified" } else { verifier_reason(&diagnostic) }})
}

struct Candidate {
    native_result: PathBuf,
    launcher: PathBuf,
    engine: PathBuf,
    node: PathBuf,
    before: BTreeMap<PathBuf, String>,
    identity: Value,
}
impl Candidate {
    fn load() -> Self {
        let native_result = required("VCP_BETA_NATIVE_RESULT");
        let setup_result = required("VCP_BETA_SETUP_RESULT");
        let launcher = fs::canonicalize(required("VCP_BETA_INSTALLED_LAUNCHER")).unwrap();
        let node = fs::canonicalize(required("VCP_TEST_NODE")).unwrap();
        assert_eq!(launcher.file_name().unwrap(), "vcp.exe");
        let install = launcher.parent().unwrap().join("engine");
        // Reuse production no-follow ownership, active-pointer and image checks.
        let selected = vcp_cli::installation::select(&install).unwrap();
        let engine = fs::canonicalize(&selected.executable).unwrap();
        let native = read(&native_result);
        let setup = read(&setup_result);
        assert_eq!(native["schema"], "vcp-distribution-result/1");
        assert_eq!(native["status"], "release-candidate");
        assert_eq!(setup["schema"], "vcp-setup-result/1");
        assert_eq!(setup["status"], "qualification-required");
        assert_eq!(
            setup["candidate_id"],
            native["manifest"]["release"]["candidate_id"]
        );
        assert!(setup["candidate_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(setup["native_archive_sha256"], native["archive_sha256"]);
        assert_eq!(
            setup["build_receipt_sha256"],
            native["manifest"]["build"]["receipt_sha256"]
        );
        let zip = archive(&native_result, &native["package"]);
        let setup_exe = archive(&setup_result, &setup["archive"]["file"]);
        assert_eq!(hash(&zip), native["archive_sha256"]);
        assert_eq!(hash(&setup_exe), setup["archive"]["sha256"]);
        assert_eq!(hash(&launcher), setup["launcher_sha256"]);
        let expected = install
            .join("releases")
            .join(native["archive_sha256"].as_str().unwrap())
            .join("vcp.exe");
        assert_eq!(engine, fs::canonicalize(expected).unwrap());
        let manifest = engine.parent().unwrap().join("manifest.json");
        assert_eq!(read(&manifest), native["manifest"]);
        let before: BTreeMap<_, _> = [
            native_result.clone(),
            setup_result,
            zip,
            setup_exe,
            launcher.clone(),
            engine.clone(),
            manifest,
            install.join("active.json"),
            install.join(".vcp-install-owned.json"),
            node.clone(),
        ]
        .into_iter()
        .map(|path| {
            let digest = hash(&path);
            (path, digest)
        })
        .collect();
        let identity = json!({"release":native["manifest"]["release"],
            "native_result_sha256":hash(&native_result),"native_archive_sha256":native["archive_sha256"],
            "setup_archive_sha256":setup["archive"]["sha256"],"launcher_sha256":setup["launcher_sha256"],
            "engine_sha256":hash(&engine),"build_receipt_sha256":setup["build_receipt_sha256"],
            "installed_default_data":selected.data,"explicit_synthetic_data_override":true,
            "node_sha256":hash(&node)});
        Self {
            native_result,
            launcher,
            engine,
            node,
            before,
            identity,
        }
    }
    fn unchanged(&self) {
        for (path, before) in &self.before {
            assert_eq!(
                &hash(path),
                before,
                "candidate input changed: {}",
                path.display()
            );
        }
    }
    async fn verify_payload(&self, repo: &Path, output: &Path, suffix: &str) {
        let script = fs::canonicalize(repo.join("scripts/evals/production-package.cjs")).unwrap();
        assert!(
            script.starts_with(repo),
            "verifier must remain inside checkout"
        );
        let summary = observe_verifier(
            &self.node,
            &script,
            &[&self.native_result, self.engine.parent().unwrap()],
            output,
            suffix,
        )
        .await;
        assert_eq!(
            summary["status"], "pass",
            "final native payload verification failed: {summary}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn payload_verifier_node_entry_handles_canonical_spaces_and_unicode() {
    let temporary = tempfile::Builder::new()
        .prefix("vcp verifier café 日本語 ")
        .tempdir()
        .unwrap()
        .keep();
    let root = fs::canonicalize(&temporary).unwrap();
    let node = fs::canonicalize(required("VCP_TEST_NODE")).unwrap();
    let script = root.join("entry verifier.cjs");
    fs::write(root.join("sibling.cjs"), "module.exports = 'loaded';").unwrap();
    fs::write(
        &script,
        r#"const assert = require('node:assert/strict');
assert.equal(require('./sibling.cjs'), 'loaded');
assert.ok(process.argv[2].startsWith('\\\\?\\'));
assert.ok(!process.argv[1].startsWith('\\\\?\\'));
process.stdout.write('entry-loaded');"#,
    )
    .unwrap();
    let ordinary = node_script_argument(&script);
    assert!(!ordinary.to_str().unwrap().starts_with(r"\\?\"));
    assert_eq!(fs::canonicalize(&ordinary).unwrap(), script);
    let observed = observe_verifier(&node, &script, &[&root], &temporary, "entry").await;
    assert_eq!(observed["status"], "pass", "{observed}");
    assert_eq!(observed["exit_code"], 0);
    assert_eq!(observed["forced_cleanup"], false);
    assert_eq!(
        fs::read(temporary.join("payload-entry.stdout")).unwrap(),
        b"entry-loaded"
    );

    fs::write(&script, "console.error('Production artifact rejected: Distribution payload differs from manifest: private sentinel'); process.exitCode = 7;").unwrap();
    let rejected = observe_verifier(&node, &script, &[&root], &temporary, "rejected").await;
    assert_eq!(rejected["status"], "fail");
    assert_eq!(rejected["exit_code"], 7);
    assert_eq!(rejected["reason"], "distribution-payload-mismatch");
    assert_eq!(rejected["natural_tree_exit"], true);
    assert_eq!(rejected["tree_stopped"], true);
    assert!(!rejected.to_string().contains("private sentinel"));
}

#[test]
fn payload_verifier_diagnostics_never_echo_untrusted_stderr() {
    assert_eq!(
        verifier_reason(b"EISDIR private-path node:internal/modules/cjs/loader"),
        "node-entry-resolution"
    );
    assert_eq!(
        verifier_reason(b"Production artifact rejected: private-path"),
        "production-artifact-rejected"
    );
    assert_eq!(
        verifier_reason(b"private contents"),
        "unclassified-verifier-output"
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "owned hidden-console subprocess only; requires a private broker input"]
async fn launcher_console_broker() {
    let input_path = required("VCP_BETA_CONSOLE_BROKER_INPUT");
    let input = read(&input_path);
    let outcome = if input["broker_self_test"] == true {
        launcher_console::self_test().await
    } else {
        launcher_console::broker(&input).await
    };
    let report = match &outcome {
        Ok(value) => value.clone(),
        Err(error) => json!({"status":"fail","error":error}),
    };
    save(Path::new(input["output"].as_str().unwrap()), &report);
    assert!(
        outcome.is_ok(),
        "owned console case failed; private broker result retained"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hidden_owned_console_event_mechanics() {
    // Exercises the isolated console APIs only, with no product executable or
    // installation. Its result is never a final-candidate qualification row.
    let temporary = tempfile::Builder::new()
        .prefix("vcp-console-mechanics-")
        .tempdir()
        .unwrap()
        .keep();
    let input = temporary.join("input.json");
    save(
        &input,
        &json!({"broker_self_test":true,"output":temporary.join("result.json")}),
    );
    let result = launcher_console::run_broker(&input)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{error}; private mechanics evidence at {}",
                temporary.display()
            )
        });
    assert_eq!(result["schema"], "vcp-console-broker-self-test/1");
    assert_eq!(result["events"], json!([0, 1]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires final setup/native receipts and their installed launcher; no provider/model calls"]
async fn final_installed_launcher_console_cancellation_preserves_both_stores() {
    let repo =
        fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap();
    let output = required("VCP_BETA_LAUNCHER_CONSOLE_OUTPUT");
    assert!(!output.exists(), "fresh private console output required");
    let parent = fs::canonicalize(output.parent().unwrap()).unwrap();
    assert!(
        !parent.starts_with(&repo),
        "synthetic fixture evidence must be private and outside checkout"
    );
    fs::create_dir(&output).unwrap();
    let mut report = json!({"schema":"vcp-beta-launcher-console/1","status":"fail",
        "qualification_executable_sha256":hash(&std::env::current_exe().unwrap()),
        "source_files":[
            {"path":"tests/beta_launcher_console.rs","sha256":hash(&repo.join("src/crates/vcp-cli/tests/beta_launcher_console.rs"))},
            {"path":"tests/support/launcher_console.rs","sha256":hash(&repo.join("src/crates/vcp-cli/tests/support/launcher_console.rs"))},
            {"path":"tests/support/hidden_process.rs","sha256":hash(&repo.join("src/crates/vcp-cli/tests/support/hidden_process.rs"))},
            {"path":"tests/support/local_fixture.rs","sha256":hash(&repo.join("src/crates/vcp-cli/tests/support/local_fixture.rs"))}],
        "limits":{"readiness_seconds":30,"event_exit_seconds":10,"broker_seconds":60,"cleanup_seconds":10},
        "limitations":["Real console cancellation at the read-only paused-task chooser only; no active provider, model or memory work.",
            "Synthetic Files/SQLite histories on the current host; not clean-host acceptance, recovery from active work or paid-task cancellation.",
            "Explicit private --workspace/--data-dir fixtures override the separately validated installed default data binding."],
        "cases":[]});
    for backend in ["Files", "Sqlite"] {
        for signal in ["Ctrl+C", "Ctrl+Break"] {
            for mode in ["direct", "launcher"] {
                report["cases"].as_array_mut().unwrap().push(
                    json!({"backend":backend,"signal":signal,"mode":mode,"status":"not-run"}),
                );
            }
        }
    }
    let result = output.join("result.json");
    save(&result, &report);
    let candidate = Candidate::load();
    report["candidate"] = candidate.identity.clone();
    save(&result, &report);
    candidate.verify_payload(&repo, &output, "before").await;
    let mut row_index = 0;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut fixture = local_fixture::Fixture::new(backend).await;
        let private = fixture.preserve_private_root();
        let sentinel = fixture.workspace.join("user-work-sentinel.txt");
        fs::write(
            &sentinel,
            b"Synthetic user work must remain byte-identical.\r\n",
        )
        .unwrap();
        let sentinel_hash = hash(&sentinel);
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        local_fixture::assert_offline_paused(&store, &fixture.config);
        let before = serde_json::to_value(store.state()).unwrap();
        store.close().await.unwrap();
        let state_path = output.join(format!("{backend:?}-state-before.json"));
        save(&state_path, &before);
        for signal in [0u32, 1] {
            let mut baseline = None;
            for mode in ["direct", "launcher"] {
                let name = format!("{backend:?}-{signal}-{mode}");
                let input_path = output.join(format!("{name}-input.json"));
                let executable = if mode == "direct" {
                    &candidate.engine
                } else {
                    &candidate.launcher
                };
                save(
                    &input_path,
                    &json!({"mode":mode,"signal":signal,"executable":executable,"engine":candidate.engine,
                    "workspace":fixture.workspace,"data":fixture.data,"output":output.join(format!("{name}-broker.json"))}),
                );
                report["cases"][row_index]["status"] = json!("fail");
                report["cases"][row_index]["fixture_root"] = json!(private);
                report["cases"][row_index]["command"] = json!({"executable":executable,
                    "arguments":["--workspace",fixture.workspace,"--data-dir",fixture.data,"workspace","discover"]});
                save(&result, &report);
                let observation = launcher_console::run_broker(&input_path).await;
                match &observation {
                    Ok(value) => report["cases"][row_index]["observation"] = value.clone(),
                    Err(error) => report["cases"][row_index]["error"] = json!(error),
                }
                save(&result, &report);
                let after = fixture.reopen_within(Duration::from_secs(45)).await;
                local_fixture::assert_offline_paused(&after, &fixture.config);
                let state = serde_json::to_value(after.state()).unwrap();
                after.close().await.unwrap();
                let after_path = output.join(format!("{name}-state-after.json"));
                save(&after_path, &state);
                report["cases"][row_index]["canonical_unchanged"] = json!(state == before);
                report["cases"][row_index]["state_before_sha256"] = json!(hash(&state_path));
                report["cases"][row_index]["state_after_sha256"] = json!(hash(&after_path));
                report["cases"][row_index]["workspace_sentinel_unchanged"] =
                    json!(hash(&sentinel) == sentinel_hash);
                save(&result, &report);
                assert_eq!(
                    state,
                    before,
                    "console cancellation changed canonical state; private evidence {}",
                    result.display()
                );
                assert_eq!(hash(&sentinel), sentinel_hash);
                let observation = observation.unwrap_or_else(|error| {
                    panic!("{error}; private evidence {}", result.display())
                });
                let code = observation["engine_exit_u32"].as_u64().unwrap();
                if mode == "direct" {
                    baseline = Some(code);
                }
                assert_eq!(
                    baseline,
                    Some(code),
                    "launcher differs from direct final native console exit"
                );
                report["cases"][row_index]["direct_exit_u32"] = json!(baseline.unwrap());
                report["cases"][row_index]["status"] = json!("pass");
                save(&result, &report);
                row_index += 1;
            }
        }
    }
    candidate.unchanged();
    candidate.verify_payload(&repo, &output, "after").await;
    report["candidate_unchanged"] = json!(true);
    report["status"] = json!("pass");
    save(&result, &report);
    eprintln!(
        "Final installed chooser console evidence: {}",
        result.display()
    );
}
