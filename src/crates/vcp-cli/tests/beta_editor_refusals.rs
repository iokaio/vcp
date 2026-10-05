// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Refusal/preservation observations of installed production artifacts. The
//! independently hashed harness never supplies a live binding or provider.
#[path = "support/editor_supervision.rs"]
mod editor_supervision;
#[path = "support/hidden_process.rs"]
mod hidden_process;
#[path = "support/local_fixture.rs"]
mod local_fixture;
#[path = "support/script_path.rs"]
mod script_path;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use vcp_domain::{
    controller::Lease,
    workspace::{Trust, Workspace},
};
use vcp_lifecycle::foundation::Config;
use vcp_protocol::{command::CommandResult, event::EventKind};
use vcp_store::contract::CanonicalStore;
use vcp_store::{
    contract::{self, Collection, State},
    BackendKind,
};

const SOURCE: &[u8] = b"original human work\n";
fn hash(path: &Path) -> String {
    vcp_protocol::digest_reader(fs::File::open(path).unwrap())
        .unwrap()
        .0
}
fn save(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn runner(repo: &Path) -> Command {
    let node = PathBuf::from(std::env::var_os("VCP_TEST_NODE").expect("Pinned Node required"));
    assert!(node.is_absolute() && node.is_file());
    let pwsh = std::env::var_os("VCP_TEST_PWSH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|directory| directory.join("pwsh.exe"))
                .find(|file| file.is_absolute() && file.is_file())
        })
        .expect("PowerShell 7 required");
    assert!(pwsh.is_absolute() && pwsh.is_file());
    let mut command = Command::new(&pwsh);
    command.env_clear();
    for name in [
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "TEMP",
        "TMP",
        "ProgramFiles",
        "ProgramFiles(x86)",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let system =
        PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root required"));
    command.env(
        "PATH",
        std::env::join_paths([
            node.parent().unwrap().to_owned(),
            pwsh.parent().unwrap().to_owned(),
            system.join("System32"),
            system,
        ])
        .unwrap(),
    );
    command
        .args(["-NoProfile", "-File"])
        .arg(script_path::argument(
            &repo.join("scripts/release/editor-refusals.ps1"),
        ));
    command
}
fn preserved(
    before: &State,
    after: &State,
    config: &Config,
    trusted: bool,
) -> Result<Value, String> {
    let workspace_key = contract::key(Collection::Workspace, config.workspace.as_str());
    let lease_id = contract::controller_lease_id(&config.workspace, &config.session)
        .map_err(|e| e.to_string())?;
    let lease_key = contract::key(Collection::Access, &lease_id);
    let immutable = |state: &State| {
        state
            .records
            .iter()
            .filter(|(key, _)| *key != &workspace_key && *key != &lease_key)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    if immutable(before) != immutable(after) {
        return Err("Task, accounting or unrelated canonical record changed".into());
    }
    let original = before
        .records
        .get(&workspace_key)
        .ok_or("Original workspace missing")?;
    let mut workspace: Workspace = original.decode().map_err(|e| e.to_string())?;
    if workspace.trust != Trust::Untrusted {
        return Err("Fixture must begin untrusted".into());
    }
    if trusted {
        workspace.trust = Trust::Trusted;
        workspace.revision = workspace.revision.next().map_err(|e| e.to_string())?;
        workspace.authority = workspace.authority.next().map_err(|e| e.to_string())?;
    }
    let mut expected = original.clone();
    expected.revision = workspace.revision;
    expected.value = serde_json::to_value(&workspace).map_err(|e| e.to_string())?;
    if after.records.get(&workspace_key) != Some(&expected) {
        return Err("Workspace changed beyond the single explicit trust grant".into());
    }
    let lease: Lease = after
        .records
        .get(&lease_key)
        .ok_or("Controller observations missing")?
        .decode()
        .map_err(|e| e.to_string())?;
    lease.validate().map_err(|e| e.to_string())?;
    if lease.holder.is_some()
        || lease.workspace != config.workspace
        || lease.session != config.session
    {
        return Err("Controller remains held or belongs to another scope".into());
    }
    if !after.events.starts_with(&before.events) {
        return Err("Acknowledged history changed".into());
    }
    let events = &after.events[before.events.len()..];
    let mut correlations = BTreeSet::new();
    let mut grants = 0;
    let mut last_lease = None;
    for envelope in events {
        let event = &envelope.event;
        if event.kind != EventKind::AccessChanged
            || event.task.is_some()
            || !event.artifacts.is_empty()
            || event.metadata.is_some()
            || envelope.redaction.is_some()
            || event.workspace != config.workspace
            || event.session != config.session
            || event.actor != config.actor
        {
            return Err("Unexpected canonical event during refusal observation".into());
        }
        let facts = event.data["facts"]
            .as_array()
            .ok_or("Authority facts missing")?;
        if facts.len() != 1 {
            return Err("Unexpected authority facts".into());
        }
        let fact = &facts[0];
        if fact["collection"] == "workspace"
            && fact["id"] == config.workspace.as_str()
            && fact["value"] == expected.value
            && trusted
        {
            grants += 1;
        } else if fact["collection"] == "access" && fact["id"] == lease_id {
            let value: Lease =
                serde_json::from_value(fact["value"].clone()).map_err(|e| e.to_string())?;
            value.validate().map_err(|e| e.to_string())?;
            if value.workspace != config.workspace
                || value.session != config.session
                || value.id != lease_id
            {
                return Err("Controller fact scope changed".into());
            }
            last_lease = Some(value);
        } else {
            return Err("Unexpected canonical authority mutation".into());
        }
        if !correlations.insert(event.correlation.clone()) {
            return Err("Unexpected multiple events per authority command".into());
        }
    }
    if grants != usize::from(trusted) {
        return Err("Explicit trust grant count differs".into());
    }
    if last_lease.as_ref() != Some(&lease) {
        return Err("Final controller lease lacks its authority event".into());
    }
    for (key, value) in &before.commands {
        if after.commands.get(key) != Some(value) {
            return Err("Acknowledged command changed".into());
        }
    }
    for (key, value) in &before.transactions {
        if after.transactions.get(key) != Some(value) {
            return Err("Acknowledged transaction changed".into());
        }
    }
    let commands = after
        .commands
        .iter()
        .filter(|(key, _)| !before.commands.contains_key(*key))
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    if commands.len() != correlations.len()
        || after.transactions.len() != before.transactions.len() + commands.len()
    {
        return Err("Unexplained command/transaction added".into());
    }
    for command in &commands {
        if !correlations.contains(&command.command)
            || command.workspace != config.workspace
            || !matches!(command.result, CommandResult::Accepted { .. })
            || !after.transactions.contains_key(&command.transaction)
        {
            return Err("Unexpected authority command receipt".into());
        }
    }
    Ok(
        json!({"immutable_records_preserved":true,"acknowledged_history_preserved":true,
        "explicit_trust_grants":grants,"authority_events":events.len(),"authority_commands":commands.len(),
        "controller_released":true,"task_paused":true,"provider_attempts":0,"edit_effects":0}),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn editor_refusal_state_guard_limits_intentional_authority_changes() {
    use vcp_domain::{
        controller::{Holder, Reason},
        ids::{CommandId, ControllerId},
        revision::{OwnerEpoch, Revision, Timestamp},
    };
    use vcp_protocol::command::Command as GovernedCommand;
    // Start from real canonical history and a real governed trust transition.
    let fixture = local_fixture::Fixture::new(BackendKind::Files).await;
    let store = fixture.reopen().await;
    let before = store.archive_state().await.unwrap();
    store.close().await.unwrap();
    let (host, owner) =
        vcp_lifecycle::foundation::CanonicalHost::open(fixture.config.clone()).unwrap();
    host.command(
        GovernedCommand::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    owner.close().await.unwrap();
    drop(host);
    let mut engine = vcp_engine::Engine::new(fixture.reopen().await).unwrap();
    let workspace: Workspace = engine
        .store()
        .current()
        .record(
            Collection::Workspace,
            fixture.config.workspace.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let access = vcp_engine::Access {
        actor: fixture.config.actor.clone(),
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        authority: workspace.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    let connection = ControllerId::new();
    engine
        .acquire_controller(
            &access,
            &connection,
            CommandId::new(),
            None,
            Timestamp::new(1),
        )
        .await
        .unwrap();
    let token = engine.controller_token(&access, &connection).unwrap();
    engine
        .release_controller(
            &access,
            &connection,
            CommandId::new(),
            &token,
            token.revision(),
            Reason::Released,
            Timestamp::new(2),
        )
        .await
        .unwrap();
    let after = engine.store().archive_state().await.unwrap();
    engine.into_store().close().await.unwrap();
    let id =
        contract::controller_lease_id(&fixture.config.workspace, &fixture.config.session).unwrap();
    let lease: Lease = after
        .record(Collection::Access, &id, &fixture.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert!(preserved(&before, &after, &fixture.config, true).is_ok());
    assert!(preserved(&before, &after, &fixture.config, false).is_err());
    let mut changed = after.clone();
    changed
        .records
        .get_mut(&contract::key(
            Collection::Task,
            fixture.config.root_task.as_str(),
        ))
        .unwrap()
        .value["state"] = json!("running");
    assert!(preserved(&before, &changed, &fixture.config, true).is_err());
    let mut changed = after.clone();
    changed
        .records
        .get_mut(&contract::key(
            Collection::Workspace,
            fixture.config.workspace.as_str(),
        ))
        .unwrap()
        .value["binding"]["root"] = json!("other-root");
    assert!(preserved(&before, &changed, &fixture.config, true).is_err());
    let mut changed = after.clone();
    changed.events[0].event.data = json!({});
    assert!(preserved(&before, &changed, &fixture.config, true).is_err());
    let mut changed = after.clone();
    changed.commands.values_mut().next().unwrap().command = CommandId::new();
    assert!(preserved(&before, &changed, &fixture.config, true).is_err());
    let mut changed = after.clone();
    let held = Lease {
        holder: Some(Holder {
            actor: fixture.config.actor.clone(),
            connection: ControllerId::new(),
            process_owner: ControllerId::new(),
            owner_epoch: OwnerEpoch::ZERO,
        }),
        reason: Reason::Acquired,
        revision: Revision::ZERO,
        ..lease
    };
    changed
        .records
        .get_mut(&contract::key(Collection::Access, &id))
        .unwrap()
        .value = serde_json::to_value(held).unwrap();
    assert!(preserved(&before, &changed, &fixture.config, true).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit final artifacts, pinned editor and a fresh registration-free normal Windows user required"]
async fn final_installed_candidate_editor_refusals_preserve_both_stores() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = std::env::var_os("VCP_BETA_EDITOR_REFUSALS_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| tempfile::tempdir().unwrap().keep());
    assert!(output.is_absolute() && !output.starts_with(&repo));
    fs::create_dir_all(&output).unwrap();
    assert_eq!(
        fs::read_dir(&output).unwrap().count(),
        0,
        "New empty private output required"
    );
    assert!(
        !output.join("result.json").exists(),
        "Fresh evidence output required"
    );
    let sources = [
        "src/crates/vcp-cli/tests/beta_editor_refusals.rs",
        "src/crates/vcp-cli/tests/support/editor_supervision.rs",
        "src/crates/vcp-cli/tests/support/hidden_process.rs",
        "src/crates/vcp-cli/tests/support/script_path.rs",
        "src/crates/vcp-cli/tests/support/local_fixture.rs",
        "scripts/release/editor-refusals.ps1",
        "scripts/release/editor-refusals.cjs",
        "scripts/release/editor-refusals-driver.cjs",
        "scripts/release/editor-lifecycle.cjs",
        "scripts/release/editor-layout.ps1",
        "scripts/release/candidate-runtime.ps1",
        "scripts/evals/production-package.cjs",
        "scripts/release/provenance.cjs",
        "src/packages/vscode/scripts/run-extension-host.ps1",
        "release/candidate-tools.json",
    ];
    let harness = json!({"qualification_executable_sha256":hash(&std::env::current_exe().unwrap()),
        "source_files":sources.iter().map(|file|json!({"path":file,"sha256":hash(&repo.join(file))})).collect::<Vec<_>>()});
    save(&output.join("harness.json"), &harness);
    let mut reports = Vec::new();
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut fixture = local_fixture::Fixture::new(backend).await;
        let fixture_root = fixture.preserve_private_root();
        let private = output.join(format!("{backend:?}"));
        fs::create_dir(&private).unwrap();
        let installed_root = private.join("installed");
        for name in ["typing", "undo", "reopen"] {
            fs::write(fixture.workspace.join(format!("{name}.txt")), SOURCE).unwrap();
        }
        let descriptor = fixture
            .config
            .canonical_root
            .parent()
            .unwrap()
            .join("workspace.json");
        let descriptor_hash = hash(&descriptor);
        let sentinel = fixture.data.join("independent-user-data");
        fs::write(&sentinel, b"preserve independent data").unwrap();
        let key = private.join("independent-key-sentinel");
        fs::write(&key, b"synthetic independent key; no credential").unwrap();
        let store = fixture.reopen().await;
        let before = store.archive_state().await.unwrap();
        store.close().await.unwrap();
        save(&private.join("state-before.json"), &before);
        let mut observations = Vec::new();
        for mode in ["restricted", "trusted", "finish"] {
            let mut command = runner(&repo);
            for (flag, variable) in [
                ("-NativeResult", "VCP_BETA_NATIVE_RESULT"),
                ("-SetupResult", "VCP_BETA_SETUP_RESULT"),
                ("-VsixManifest", "VCP_BETA_VSIX_MANIFEST"),
                ("-Code", "VCP_TEST_CODE"),
            ] {
                let path = PathBuf::from(std::env::var_os(variable).expect(variable));
                assert!(path.is_absolute() && path.is_file());
                command.arg(flag).arg(path);
            }
            command
                .arg("-Workspace")
                .arg(script_path::directory_argument(&fixture.workspace))
                .arg("-DataRoot")
                .arg(script_path::directory_argument(&fixture.data))
                .arg("-Scope")
                .arg(fixture.scope().to_string())
                .arg("-Task")
                .arg(fixture.config.root_task.as_str())
                .arg("-OutputRoot")
                .arg(&installed_root)
                .arg("-Mode")
                .arg(mode);
            let mut report =
                editor_supervision::editor_observation(&mut command, &private, &fixture_root, mode)
                    .await;
            assert_eq!(report["status"], "pass");
            assert_eq!(report["mode"], mode);
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            local_fixture::assert_offline_paused(&store, &fixture.config);
            save(
                &private.join(format!("state-{mode}.json")),
                &store.archive_state().await.unwrap(),
            );
            report["canonical_preservation"] = preserved(
                &before,
                &store.archive_state().await.unwrap(),
                &fixture.config,
                mode != "restricted",
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{mode}: {error}; private installation/evidence retained at {}",
                    private.display()
                )
            });
            store.close().await.unwrap();
            assert_eq!(hash(&descriptor), descriptor_hash);
            for name in ["typing", "undo", "reopen"] {
                assert_eq!(
                    fs::read(fixture.workspace.join(format!("{name}.txt"))).unwrap(),
                    SOURCE
                );
            }
            assert_eq!(fs::read(&sentinel).unwrap(), b"preserve independent data");
            assert_eq!(
                fs::read(&key).unwrap(),
                b"synthetic independent key; no credential"
            );
            assert!(!installed_root.join("wrong-data/workspaces").exists());
            assert_eq!(
                fs::read_dir(installed_root.join("uninitialized"))
                    .unwrap()
                    .count(),
                0
            );
            if mode == "finish" {
                assert_eq!(report["uninstalled"], true);
            } else {
                assert_eq!(report["forbidden_rpc_count"], 0);
                assert_eq!(report["final_observer"], true);
            }
            observations.push(report);
        }
        reports.push(json!({"backend":format!("{backend:?}"),"observations":observations}));
    }
    for row in harness["source_files"].as_array().unwrap() {
        assert_eq!(
            hash(&repo.join(row["path"].as_str().unwrap())),
            row["sha256"]
        );
    }
    let report = json!({"schema":"vcp-editor-refusal-observations/1","status":"pass","harness":harness,"stores":reports,
        "limitations":"Synthetic paused history and actual installed extension-host refusals; no successful prepare/apply, provider work or human UI acceptance."});
    save(&output.join("result.json"), &report);
    eprintln!(
        "Final installed editor refusal evidence: {}",
        output.join("result.json").display()
    );
}
