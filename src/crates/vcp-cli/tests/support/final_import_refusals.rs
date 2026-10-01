// SPDX-License-Identifier: Apache-2.0
//! Final-artifact admission refusals. Successful MCP execution remains covered
//! separately by the qualification-only loopback tests; no provider override is
//! present here. The caller must verify the complete installed package and own
//! this entire test process tree in a bounded no-breakaway Job.
use super::*;
use std::{collections::BTreeMap, os::windows::fs::MetadataExt, path::Path};
use vcp_domain::controller::Lease;
use vcp_protocol::{command::CommandResult, event::EventKind};
use vcp_store::contract::{self, State};

fn hash(path: &Path) -> String {
    vcp_protocol::digest_reader(fs::File::open(path).unwrap())
        .unwrap()
        .0
}
fn save(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn plain(path: &Path) {
    assert!(path.is_absolute());
    for parent in path.ancestors() {
        if let Ok(metadata) = fs::symlink_metadata(parent) {
            assert_eq!(
                metadata.file_attributes() & 0x400,
                0,
                "Redirected qualification input"
            );
        }
    }
}
fn input(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect(name));
    plain(&path);
    path
}
struct Report {
    path: PathBuf,
    value: Value,
}
impl Report {
    fn save(&self) {
        save(&self.path, &self.value);
    }
}
impl Drop for Report {
    fn drop(&mut self) {
        if self.value["status"] != "pass" {
            self.value["status"] = json!("fail");
            self.value["failure"] = json!(
                "Observation did not complete; preserve private fixtures and runner captures"
            );
        }
        // Do not double-panic if the original failure was a filesystem failure.
        let _ = fs::write(&self.path, serde_json::to_vec_pretty(&self.value).unwrap());
    }
}
impl Fixture {
    fn final_fixture(root: &Path, binary: &Path, node: &Path) -> Self {
        let mut temp = tempfile::Builder::new()
            .prefix("import-case-")
            .tempdir_in(root)
            .unwrap();
        temp.disable_cleanup(true);
        Self::at(temp, binary.to_owned(), None, Some(node.to_owned()))
    }
    pub(super) async fn final_capture(
        &self,
        args: &[&str],
        close_stdout: bool,
    ) -> std::process::Output {
        let capture = self
            ._temp
            .path()
            .join(format!("command-{}", vcp_domain::CommandId::new()));
        let stdout = capture.with_extension("stdout");
        let stderr = capture.with_extension("stderr");
        let mut command = tokio::process::Command::from(self.command(args));
        command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(if close_stdout {
                Stdio::piped()
            } else {
                Stdio::from(fs::File::create(&stdout).unwrap())
            })
            .stderr(fs::File::create(&stderr).unwrap());
        save(
            &capture.with_extension("json"),
            &json!({"executable":self.binary,"arguments":args,"deadline_seconds":60,"output_ceiling_bytes":4194304,"stdout_closed_before_admission":close_stdout}),
        );
        let mut child = command.spawn().unwrap();
        if close_stdout {
            drop(child.stdout.take());
        }
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            let bytes: u64 = [&stdout, &stderr]
                .iter()
                .map(|path| fs::metadata(path).map_or(0, |m| m.len()))
                .sum();
            if began.elapsed() > Duration::from_secs(60) || bytes > 4 * 1024 * 1024 {
                child.start_kill().unwrap();
                let _ = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
                panic!(
                    "Final import command exceeded bounded time/output; private captures retained"
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let stdout = if close_stdout {
            vec![]
        } else {
            fs::read(stdout).unwrap()
        };
        let stderr = fs::read(stderr).unwrap();
        assert!(stdout.len() + stderr.len() <= 4 * 1024 * 1024);
        assert!(!String::from_utf8_lossy(&stdout).contains(SECRET));
        assert!(!String::from_utf8_lossy(&stderr).contains(SECRET));
        std::process::Output {
            status,
            stdout,
            stderr,
        }
    }
    pub(super) async fn seed_final(&self, backend: BackendKind) -> WorkspaceEntry {
        let name = backend_name(backend);
        let configured = self
            .final_capture(&["storage", "configure", "--backend", name], false)
            .await;
        assert!(configured.status.success());
        // Ordinary production bootstrap writes a real trusted policy/descriptor.
        // The closed stdout stops it before submission. Independently, the cap
        // cannot fund even one request if that output boundary ever regresses.
        let seeded = self
            .final_capture(
                &[
                    "run",
                    "Seed paused import refusal history",
                    "--autonomy",
                    "autonomous",
                ],
                true,
            )
            .await;
        assert_eq!(seeded.status.code(), Some(1));
        let directory = vcp_cli::settings::workspace_directory(
            &self.data,
            &self.workspace.canonicalize().unwrap(),
        )
        .unwrap()
        .unwrap();
        let entry: WorkspaceEntry =
            serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
        assert_eq!(entry.config.backend, backend);
        assert_eq!(entry.config.cap.micros.get(), 1);
        let request = &entry.config.price.rates[&vcp_domain::accounting::ChargeCategory::Request];
        // Catalog rates use normalized units. Assert the exact one-request
        // cost, independently of that denominator, before testing admission.
        assert_ne!(request.per_units.get(), 0);
        assert_eq!(
            u128::from(request.micros.get()),
            100 * u128::from(request.per_units.get())
        );
        let store = reopen_within(&entry, Duration::from_secs(45)).await;
        no_dispatch(store.state());
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                entry.config.root_task.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        store.close().await.unwrap();
        entry
    }
}
fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Files => "files",
        BackendKind::Sqlite => "sqlite",
    }
}
fn no_dispatch(state: &State) -> Value {
    for collection in [
        Collection::Attempt,
        Collection::Reservation,
        Collection::Settlement,
        Collection::Effect,
    ] {
        assert!(
            !state
                .records
                .values()
                .any(|row| row.collection == collection),
            "Unexpected {collection:?}"
        );
    }
    assert!(!state.events.iter().any(|event| matches!(
        event.event.kind,
        EventKind::AttemptSubmitted
            | EventKind::ReservationCreated
            | EventKind::UsageReconciled
            | EventKind::LiabilityRetained
    )));
    for record in state
        .records
        .values()
        .filter(|row| row.collection == Collection::Ledger)
    {
        let ledger: Ledger = record.decode().unwrap();
        assert_eq!(ledger.cap.get(), 1);
        assert_eq!(ledger.active.get(), 0);
        assert_eq!(ledger.settled.get(), 0);
        assert_eq!(ledger.unresolved.get(), 0);
        assert!(!ledger.overrun);
    }
    json!({"provider_attempts":0,"reservations":0,"send_intents":0,"settlements":0,"effects":0})
}
fn preserved(before: &State, after: &State, entry: &WorkspaceEntry) {
    let config = &entry.config;
    let lease_id = contract::controller_lease_id(&config.workspace, &config.session).unwrap();
    let lease_key = contract::key(Collection::Access, &lease_id);
    let immutable = |state: &State| {
        state
            .records
            .iter()
            .filter(|(key, _)| *key != &lease_key)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(
        immutable(before),
        immutable(after),
        "Task, accounting, policy or unrelated record changed"
    );
    assert!(after.events.starts_with(&before.events));
    let lease: Lease = after.records.get(&lease_key).unwrap().decode().unwrap();
    lease.validate().unwrap();
    assert_eq!(lease.id, lease_id);
    assert_eq!(lease.workspace, config.workspace);
    assert_eq!(lease.session, config.session);
    assert!(lease.holder.is_none(), "Controller must be released");
    let mut correlations = BTreeSet::new();
    let mut last = None;
    for envelope in &after.events[before.events.len()..] {
        let event = &envelope.event;
        assert_eq!(event.kind, EventKind::AccessChanged);
        assert_eq!(event.workspace, config.workspace);
        assert_eq!(event.session, config.session);
        assert_eq!(event.actor, config.actor);
        assert!(
            event.task.is_none()
                && event.artifacts.is_empty()
                && event.metadata.is_none()
                && envelope.redaction.is_none()
        );
        let facts = event.data["facts"].as_array().unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0]["collection"], "access");
        assert_eq!(facts[0]["id"], lease_id);
        let value: Lease = serde_json::from_value(facts[0]["value"].clone()).unwrap();
        value.validate().unwrap();
        assert_eq!(value.id, lease_id);
        assert_eq!(value.workspace, config.workspace);
        assert_eq!(value.session, config.session);
        last = Some(value);
        assert!(correlations.insert(event.correlation.clone()));
    }
    assert_eq!(last.as_ref(), Some(&lease));
    for (key, value) in &before.commands {
        assert_eq!(after.commands.get(key), Some(value));
    }
    for (key, value) in &before.transactions {
        assert_eq!(after.transactions.get(key), Some(value));
    }
    let commands: Vec<_> = after
        .commands
        .iter()
        .filter(|(key, _)| !before.commands.contains_key(*key))
        .map(|(_, value)| value)
        .collect();
    assert_eq!(commands.len(), correlations.len());
    assert_eq!(
        after.transactions.len(),
        before.transactions.len() + commands.len()
    );
    for command in commands {
        assert!(correlations.contains(&command.command));
        assert_eq!(command.workspace, config.workspace);
        assert!(matches!(command.result, CommandResult::Accepted { .. }));
        assert!(after.transactions.contains_key(&command.transaction));
    }
    no_dispatch(after);
}
async fn snapshot(fixture: &Fixture, entry: &WorkspaceEntry, label: &str) -> State {
    let store = reopen_within(entry, Duration::from_secs(45)).await;
    let state = store.state().clone();
    store.close().await.unwrap();
    save(&fixture._temp.path().join(format!("{label}.json")), &state);
    state
}
async fn control(fixture: &Fixture, entry: &WorkspaceEntry, mode: &str) -> Value {
    let selected;
    if mode == "cli" {
        let output = fixture
            .final_capture(
                &[
                    "run",
                    "Observe the unaffordable request boundary",
                    "--autonomy",
                    "autonomous",
                ],
                false,
            )
            .await;
        assert_eq!(
            output.status.code(),
            Some(5),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let frames: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let result = frames
            .iter()
            .rev()
            .find(|row| row["type"] == "result")
            .unwrap();
        selected = result["scope"]["task"].as_str().unwrap().to_owned();
    } else {
        selected = if mode == "start" {
            ROOT.into()
        } else {
            entry.config.root_task.to_string()
        };
        let mut client = fixture.launch("controller", Some(&selected));
        acquire(&mut client, entry, "final-import-control-owner");
        let request = if mode == "start" {
            start(entry)
        } else {
            import_execution::resume(&mut client, entry)
        };
        wire::accepted(&client.rpc(
            4,
            if mode == "start" {
                "turn/start"
            } else {
                "session/resume"
            },
            request,
        ));
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            let view = import_execution::selected_task(&mut client, entry, &selected);
            if view["state"] != "running" {
                assert_eq!(view["state"], "paused");
                break;
            }
            assert!(
                Instant::now() < until,
                "Unchanged control must reach budget refusal"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(client.finish().await.0.success());
    }
    let state = snapshot(fixture, entry, "control-after").await;
    no_dispatch(&state);
    let turns: Vec<Turn> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Turn)
        .map(|row| row.decode().unwrap())
        .filter(|turn: &Turn| turn.scope.task.as_str() == selected)
        .collect();
    assert_eq!(turns.len(), 1, "Control needs one actual admitted turn");
    assert_eq!(turns[0].state, TurnState::BudgetExhausted);
    assert!(!fixture.data.join("mcp-observed.txt").exists());
    assert_eq!(
        fs::read(fixture.workspace.join("value.txt")).unwrap(),
        b"41\n"
    );
    json!({"backend":backend_name(entry.config.backend),"mode":mode,"status":"pass","task":selected,"turn":turns[0].id,"budget_exhausted":true,"provider_attempts":0,"reservations":0,"send_intents":0,"mcp_dispatched":false,"fixture_root":fixture._temp.path(),"canonical_after_sha256":hash(&fixture._temp.path().join("control-after.json"))})
}
async fn refusal(fixture: &Fixture, entry: &WorkspaceEntry, change: &str, mode: &str) -> Value {
    let before = snapshot(fixture, entry, "canonical-before").await;
    no_dispatch(&before);
    let root = if mode == "start" {
        ROOT
    } else {
        entry.config.root_task.as_str()
    };
    let mut client = fixture.launch("controller", Some(root));
    acquire(&mut client, entry, "final-import-refusal-owner");
    match change {
        "base" => {
            let mut bytes = fs::read(&fixture.profile).unwrap();
            bytes.push(b' ');
            fs::write(&fixture.profile, bytes).unwrap();
        }
        "revision" => fixture.reselect_import().await,
        "first-import" => fixture.import_restrictions().await,
        "content" => {
            let mut bytes = fs::read(fixture.import_revision()).unwrap();
            bytes.push(b' ');
            fs::write(fixture.import_revision(), bytes).unwrap();
        }
        "corrupt" => fs::write(fixture.import_revision(), b"{broken chain").unwrap(),
        "removed" => fs::remove_file(fixture.import_revision()).unwrap(),
        _ => unreachable!(),
    }
    let request = if mode == "start" {
        start(entry)
    } else {
        import_execution::resume(&mut client, entry)
    };
    let rejected = client.rpc(
        4,
        if mode == "start" {
            "turn/start"
        } else {
            "session/resume"
        },
        request,
    );
    assert_eq!(
        rejected["error"]["data"]["details"]["code"], "POLICY_DENIED",
        "{change}/{mode}: {rejected}"
    );
    if mode == "resume" {
        assert_eq!(
            import_execution::selected_task(&mut client, entry, root)["state"],
            "paused"
        );
    }
    assert!(client.finish().await.0.success());
    let mut after = snapshot(fixture, entry, "canonical-after").await;
    preserved(&before, &after, entry);
    let mut cli_exit = None;
    let mut reconnect_rejected = None;
    if matches!(change, "base" | "corrupt") {
        let output = fixture
            .final_capture(
                &[
                    "run",
                    "Refuse invalid persisted import",
                    "--autonomy",
                    "autonomous",
                ],
                false,
            )
            .await;
        assert_eq!(output.status.code(), Some(2));
        let mut restarted = fixture.bridge();
        restarted.send(json!({"schema":"vcp-local-bootstrap/1","workspace":fixture.workspace,"data":fixture.data,"role":"controller","execution":{"profile":fixture.profile,"provider_credential":SECRET,"credentials":{}}}));
        let (status, diagnostics) = restarted.rejected().await;
        assert!(!status.success());
        assert!(!diagnostics.contains(SECRET));
        let restarted_state = snapshot(fixture, entry, "canonical-reconnect-after").await;
        assert_eq!(
            after, restarted_state,
            "Refused CLI/reconnect mutated canonical state"
        );
        after = restarted_state;
        cli_exit = Some(2);
        reconnect_rejected = Some(true);
    }
    no_dispatch(&after);
    assert!(!after.commands.values().any(|receipt| matches!(
        receipt.command.as_str(),
        "compiled-start-once" | "import-resume"
    )));
    assert!(!fixture.data.join("mcp-observed.txt").exists());
    assert_eq!(
        fs::read(fixture.workspace.join("value.txt")).unwrap(),
        b"41\n"
    );
    json!({"backend":backend_name(entry.config.backend),"mode":mode,"change":change,"status":"pass","code":"POLICY_DENIED","immutable_records_preserved":true,"provider_attempts":0,"reservations":0,"send_intents":0,"mcp_dispatched":false,"cli_exit_code":cli_exit,"reconnect_rejected":reconnect_rejected,"fixture_root":fixture._temp.path(),"canonical_before_sha256":hash(&fixture._temp.path().join("canonical-before.json")),"canonical_after_sha256":hash(&fixture._temp.path().join("canonical-after.json"))})
}
async fn observations(
    root: &Path,
    binary: &Path,
    node: &Path,
    report: &mut Report,
    sentinel: Option<&MockServer>,
) {
    let fixture = || {
        let fixture = Fixture::final_fixture(root, binary, node);
        if let Some(sentinel) = sentinel {
            // This branch belongs only to the separately named source test.
            // It cannot be selected by the final installed entry point.
            let mut profile: Value =
                serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
            profile["qualification_endpoint"] = json!(format!("{}/v1", sentinel.uri()));
            fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        }
        fixture
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mode in ["cli", "start", "resume"] {
            let fixture = fixture();
            report.value["current"] = json!({"backend":backend_name(backend),"control":mode,"fixture_root":fixture._temp.path()});
            report.save();
            fixture.import_peer(false);
            fixture.import_restrictions().await;
            let entry = fixture.seed(backend).await;
            let row = control(&fixture, &entry, mode).await;
            report.value["controls"].as_array_mut().unwrap().push(row);
            report.save();
            if let Some(sentinel) = sentinel {
                assert!(sentinel.received_requests().await.unwrap().is_empty());
            }
        }
        for change in [
            "base",
            "revision",
            "content",
            "corrupt",
            "removed",
            "first-import",
        ] {
            for mode in ["start", "resume"] {
                let fixture = fixture();
                report.value["current"] = json!({"backend":backend_name(backend),"change":change,"mode":mode,"fixture_root":fixture._temp.path()});
                report.save();
                fixture.import_peer(false);
                if change != "first-import" {
                    fixture.import_restrictions().await;
                }
                if change == "corrupt" {
                    fixture.reselect_import().await;
                }
                let entry = fixture.seed(backend).await;
                let row = refusal(&fixture, &entry, change, mode).await;
                report.value["cases"].as_array_mut().unwrap().push(row);
                report.save();
                if let Some(sentinel) = sentinel {
                    assert!(sentinel.received_requests().await.unwrap().is_empty());
                }
            }
        }
    }
    assert_eq!(report.value["controls"].as_array().unwrap().len(), 6);
    assert_eq!(report.value["cases"].as_array().unwrap().len(), 24);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_fixture_controls_and_import_refusals_have_zero_loopback_dispatch() {
    let root = tempfile::Builder::new()
        .prefix("vcp-source-import-refusals-")
        .tempdir()
        .unwrap()
        .keep();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_vcp"));
    let node = input("VCP_TEST_NODE");
    let server = MockServer::start().await;
    let mut report = Report {
        path: root.join("result.json"),
        value: json!({"schema":"vcp-source-import-refusals/1","status":"running","scope":"source-only qualification engine and loopback dispatch sentinel; no final-artifact observation","controls":[],"cases":[],"current":null}),
    };
    report.save();
    eprintln!(
        "Source-only import refusal fixtures retained at {}",
        root.display()
    );
    observations(&root, &binary, &node, &mut report, Some(&server)).await;
    assert!(server.received_requests().await.unwrap().is_empty());
    report.value["loopback_requests"] = json!(0);
    report.value["current"] = Value::Null;
    report.value["status"] = json!("pass");
    report.save();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit final installed engine, package-bound supervisor and fresh private output required"]
async fn final_installed_configuration_changes_refuse_start_and_resume_on_both_stores() {
    let binary = input("VCP_BETA_INSTALLED_EXECUTABLE");
    let root = input("VCP_BETA_IMPORT_OUTPUT");
    let node = input("VCP_TEST_NODE");
    let powershell = input("VCP_TEST_PWSH");
    assert!(binary.is_file() && node.is_file() && powershell.is_file() && !root.exists());
    for ancestor in root.ancestors() {
        assert!(
            !ancestor.join(".git").exists(),
            "Private output must be outside every checkout"
        );
    }
    let engine_root = binary.parent().unwrap().parent().unwrap().parent().unwrap();
    let installed = vcp_cli::installation::select(engine_root).unwrap();
    assert_eq!(
        installed.executable.canonicalize().unwrap(),
        binary.canonicalize().unwrap()
    );
    fs::create_dir(&root).unwrap();
    let copied_node = root.join("node.exe");
    fs::copy(&node, &copied_node).unwrap();
    let node_hash = hash(&node);
    assert_eq!(hash(&copied_node), node_hash);
    let engine_hash = hash(&binary);
    let powershell_hash = hash(&powershell);
    let mut report = Report {
        path: root.join("result.json"),
        value: json!({"schema":"vcp-final-import-refusals/1","status":"running","engine":binary,"engine_sha256":engine_hash,"qualification_executable_sha256":hash(&std::env::current_exe().unwrap()),"node_sha256":node_hash,"powershell_sha256":powershell_hash,"controls":[],"cases":[],"current":null,"minimum_request_micros":100,"cap_micros":1,"limitations":["Canonical admission and state observations; no independent network capture or OS network-denial claim.","Synthetic unaffordable requests never qualify successful provider/MCP execution, tool allowlist enforcement or execution deadlines.","Final source/payload provenance and complete process-tree supervision belong to the enclosing package-bound runner.","Controller lease changes are intentional; rejected task/accounting/policy records and acknowledged history must remain preserved."]}),
    };
    report.save();
    observations(&root, &binary, &copied_node, &mut report, None).await;
    assert_eq!(hash(&binary), engine_hash);
    assert_eq!(hash(&node), node_hash);
    assert_eq!(hash(&copied_node), node_hash);
    assert_eq!(hash(&powershell), powershell_hash);
    let after = vcp_cli::installation::select(engine_root).unwrap();
    assert_eq!(installed.executable, after.executable);
    assert_eq!(installed.data, after.data);
    report.value["current"] = Value::Null;
    report.value["status"] = json!("pass");
    report.save();
    eprintln!(
        "Final installed configuration refusal observations retained privately at {}",
        root.display()
    );
}
