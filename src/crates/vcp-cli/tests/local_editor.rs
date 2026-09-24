// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Installed extension-host qualification with isolated profiles and both stores.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use vcp_domain::{
    effect::{Effect, EffectState},
    ids::*,
    revision::*,
    task::Task,
};
use vcp_protocol::command::{Approval, ApprovalState};
use vcp_store::{
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};

fn editor_path(path: &Path) -> String {
    let value = path.to_str().unwrap();
    value.strip_prefix(r"\\?\").unwrap_or(value).to_owned()
}

async fn pending(fixture: &Fixture) {
    let mut store = fixture.reopen().await;
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let effect = Effect {
        id: ToolRunId::new(),
        scope: task.scope.clone(),
        revision: Revision::ZERO,
        steering: task.steering,
        state: EffectState::Proposed,
        operation_digest: "a".repeat(64),
        execution: None,
        exit_code: None,
        observed_changes: vec![],
        cause: task.cause.clone(),
        reason: "retained pending editor inspection".into(),
        redaction: None,
    };
    let approval = Approval {
        id: ApprovalId::new(),
        scope: task.scope.clone(),
        effect: effect.id.clone(),
        effect_revision: effect.revision,
        steering: task.steering,
        operation_digest: effect.operation_digest.clone(),
        actor: fixture.config.actor.clone(),
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::new(u64::MAX),
        state: ApprovalState::Pending,
        revision: Revision::ZERO,
        controller: None,
        owner_epoch: None,
        authority: None,
        binding: None,
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![
                Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Effect,
                        effect.id.as_str(),
                        fixture.config.workspace.clone(),
                        Revision::ZERO,
                        &effect,
                    )
                    .unwrap(),
                },
                Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Approval,
                        approval.id.as_str(),
                        fixture.config.workspace.clone(),
                        Revision::ZERO,
                        &approval,
                    )
                    .unwrap(),
                },
            ],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    store.close().await.unwrap();
}

async fn child(fixture: &Fixture) -> TaskId {
    let (host, owner) =
        vcp_lifecycle::foundation::CanonicalHost::open(fixture.config.clone()).unwrap();
    let root: Task = host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let child = TaskId::new();
    host.command(
        vcp_protocol::command::Command::CreateTask {
            root: fixture.config.root_task.clone(),
            parent: Some(fixture.config.root_task.clone()),
            fork_origin: None,
            objective: vcp_domain::task::Objective {
                text: "child editor inspection <script>untrusted</script>".into(),
                constraints: vec!["preserve child scope".into()],
                acceptance: vec!["explicit review".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: root.fingerprint,
            editing: false,
            required_checks: vec![],
        },
        Some(child.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        vcp_protocol::command::Command::Transition {
            next: vcp_domain::task::TaskState::Paused,
            reason: "child requires explicit resume".into(),
            verification: None,
        },
        Some(child.clone()),
        Revision::ZERO,
    )
    .unwrap();
    owner.close().await.unwrap();
    child
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires VS Code 1.138 and staged extension; run explicitly"]
async fn editor_observer_commands_preserve_both_canonical_stores() {
    let code = PathBuf::from(
        std::env::var_os("VCP_TEST_CODE").expect("VS Code 1.138 executable required"),
    );
    assert!(code.is_absolute() && code.is_file());
    let fixtures = [
        Fixture::new(BackendKind::Files).await,
        Fixture::new(BackendKind::Sqlite).await,
    ];
    let trust_fixture = Fixture::new(BackendKind::Sqlite).await;
    let (trust_host, trust_owner) =
        vcp_lifecycle::foundation::CanonicalHost::open(trust_fixture.config.clone()).unwrap();
    trust_host
        .command(
            vcp_protocol::command::Command::SetWorkspaceTrust {
                trust: vcp_domain::workspace::Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    trust_owner.close().await.unwrap();
    drop(trust_host);
    pending(&trust_fixture).await;
    let moved_fixture = Fixture::new(BackendKind::Files).await;
    pending(&moved_fixture).await;
    let moved_root = moved_fixture.workspace.with_file_name("moved-workspace");
    fs::rename(&moved_fixture.workspace, &moved_root).unwrap();
    let reload_fixture = Fixture::new(BackendKind::Sqlite).await;
    pending(&reload_fixture).await;
    let mut controller = Client::spawn("local-bridge");
    let mut bootstrap = reload_fixture.bootstrap("controller");
    bootstrap["transport"] = json!("windows_pipe");
    bootstrap["observer_reconnect"] = json!(true);
    controller.send(bootstrap);
    let ready = controller.receive();
    controller.initialize();
    accepted(&controller.rpc(2, "controller/acquire", json!({"scope":reload_fixture.scope(),"command_id":"editor-external-owner","expected_revision":null})));
    let held_controller = controller.rpc(
        3,
        "controller/read",
        json!({"scope":reload_fixture.scope()}),
    );
    let observer_reference = ready["observer_reconnect"].clone();
    assert!(observer_reference.is_object());
    assert!(observer_reference.get("ticket").is_none());
    let mut before = Vec::new();
    let mut children = Vec::new();
    let mut cli_views = Vec::new();
    for fixture in &fixtures {
        children.push(child(fixture).await);
        pending(fixture).await;
        let store = fixture.reopen().await;
        let root: Task = store
            .state()
            .record(
                Collection::Task,
                fixture.config.root_task.as_str(),
                &fixture.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        cli_views.push(
            vcp_cli::terminal::view_at(
                store.state(),
                &root.scope,
                &fixture.config.price.model,
                Timestamp::new(1),
            )
            .unwrap(),
        );
        before.push(store.state().clone());
        store.close().await.unwrap();
        fs::create_dir_all(fixture.workspace.join(".vscode")).unwrap();
        fs::write(fixture.workspace.join(".vscode/settings.json"),r#"{"vcp.engineExecutable":"C:\\workspace-injected-never-run.exe","vcp.dataDirectory":"C:\\workspace-injected-never-open"}"#).unwrap();
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = repo.join("artifacts/p4-extension-host");
    fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let user = profile.path().join("user-data");
    fs::create_dir_all(user.join("User")).unwrap();
    let extensions = profile.path().join("extensions");
    fs::create_dir(&extensions).unwrap();
    fs::write(user.join("User/settings.json"),serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","security.workspace.trust.emptyWindow":false,"update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","window.restoreWindows":"none"})).unwrap()).unwrap();
    let workspace = profile.path().join("qualification.code-workspace");
    fs::write(&workspace,serde_json::to_vec(&json!({"folders":fixtures.iter().map(|fixture|json!({"name":"same-root","path":editor_path(&fixture.workspace)})).chain(std::iter::once(json!({"name":"reload-root","path":editor_path(&reload_fixture.workspace)}))).chain(std::iter::once(json!({"name":"moved-root","path":editor_path(&moved_root)}))).chain(std::iter::once(json!({"name":"trust-root","path":editor_path(&trust_fixture.workspace)}))).collect::<Vec<_>>() })).unwrap()).unwrap();
    let driver = profile.path().join("qualification-driver");
    fs::create_dir(&driver).unwrap();
    fs::write(driver.join("package.json"), serde_json::to_vec(&json!({"name":"qualification-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}})).unwrap()).unwrap();
    fs::write(driver.join("driver.cjs"), r#"const vscode=require('vscode');const fs=require('node:fs');exports.activate=()=>{setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});};"#).unwrap();
    let result = profile.path().join("result.json");
    let input = profile.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({"code":editor_path(&code),"executable":editor_path(Path::new(env!("CARGO_BIN_EXE_vcp"))),"extension":editor_path(&repo.join("artifacts/p4-vscode-extension")),"driver":editor_path(&driver),"runner":editor_path(&repo.join("src/packages/vscode/tests/extension-host.cjs")),"workspaceFile":editor_path(&workspace),"userData":editor_path(&user),"extensions":editor_path(&extensions),"result":editor_path(&result),"stdout":editor_path(&output.join("stdout.log")),"stderr":editor_path(&output.join("stderr.log")),"diagnostics":editor_path(&output.join("editor-logs")),"runtimeEvidence":editor_path(&output.join("runtime.json")),"unselected":editor_path(&profile.path().join("not-a-workspace-folder")),"trustFixture":{"workspace":editor_path(&trust_fixture.workspace),"data":editor_path(&trust_fixture.data),"scope":trust_fixture.scope()},"movedFixture":{"workspace":editor_path(&moved_root),"data":editor_path(&moved_fixture.data),"scope":moved_fixture.scope(),"rootId":RootId::parse(moved_fixture.config.workspace.as_str()).unwrap(),"bindingRevision":moved_fixture.config.binding.revision.next().unwrap()},"reloadFixture":{"workspace":editor_path(&reload_fixture.workspace),"data":editor_path(&reload_fixture.data),"scope":reload_fixture.scope(),"reference":observer_reference},"reloadMarker":editor_path(&profile.path().join("reload-phase.json")),"fixtures":fixtures.iter().zip(&children).zip(&cli_views).map(|((fixture,child),cli)|json!({"workspace":editor_path(&fixture.workspace),"data":editor_path(&fixture.data),"scope":fixture.scope(),"host":fixture.config.binding.host,"canonicalRoot":fixture.config.binding.root,"rootId":RootId::parse(fixture.config.workspace.as_str()).unwrap(),"bindingRevision":fixture.config.binding.revision.get().to_string(),"rootTask":fixture.config.root_task,"childTask":child,"cli":cli})).collect::<Vec<_>>() })).unwrap()).unwrap();
    let script = repo.join("src/packages/vscode/scripts/run-extension-host.ps1");
    let status = tokio::task::spawn_blocking(move || {
        Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(editor_path(&script))
            .arg("-InputFile")
            .arg(editor_path(&input))
            .status()
            .unwrap()
    })
    .await
    .unwrap();
    if !status.success() {
        fs::write(
            output.join("failed-profile.txt"),
            editor_path(profile.path()),
        )
        .unwrap();
        let _ = profile.keep();
    }
    if let Ok(bytes) = fs::read(&result) {
        fs::write(output.join("result.json"), bytes).unwrap();
    }
    assert!(
        status.success(),
        "editor qualification failed; inspect artifacts/p4-extension-host logs"
    );
    let result_bytes = fs::read(result).unwrap();
    let mut result: serde_json::Value = serde_json::from_slice(&result_bytes).unwrap();
    result["harness_launches"] = json!(1);
    result["storage_mode"] = json!("normal persisted workspace storage; no extensionTestsPath");
    assert_eq!(result["ok"], true);
    assert_eq!(result["version"], "1.138.0");
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert_eq!(result["reloadVerified"], true);
    assert_eq!(result["movedVerified"], true);
    assert_eq!(result["trustVerified"], true);
    assert_eq!(result["taskViewsVerified"], true);
    assert_eq!(result["cliParityVerified"], true);
    assert_eq!(result["droppedSubscriptionVerified"], true);
    let trust_store = trust_fixture.reopen_within(Duration::from_secs(45)).await;
    let trust_workspace: vcp_domain::workspace::Workspace = trust_store
        .state()
        .record(
            Collection::Workspace,
            trust_fixture.config.workspace.as_str(),
            &trust_fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        trust_workspace.trust,
        vcp_domain::workspace::Trust::Untrusted
    );
    assert_eq!(trust_workspace.revision, Revision::new(2));
    assert_eq!(trust_workspace.authority.get(), 2);
    let trust_task: Task = trust_store
        .state()
        .record(
            Collection::Task,
            trust_fixture.config.root_task.as_str(),
            &trust_fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(trust_task.state, vcp_domain::task::TaskState::Paused);
    let trust_approvals: Vec<Approval> = trust_store
        .state()
        .records
        .values()
        .filter(|row| row.collection == Collection::Approval)
        .map(|row| row.decode().unwrap())
        .collect();
    assert_eq!(trust_approvals.len(), 1);
    assert_eq!(trust_approvals[0].state, ApprovalState::Pending);
    trust_store.close().await.unwrap();
    let moved_store = moved_fixture.reopen_within(Duration::from_secs(45)).await;
    let moved_workspace: vcp_domain::workspace::Workspace = moved_store
        .state()
        .record(
            Collection::Workspace,
            moved_fixture.config.workspace.as_str(),
            &moved_fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(moved_workspace.id, moved_fixture.config.workspace);
    assert_eq!(moved_workspace.binding.root, moved_root.to_string_lossy());
    assert_eq!(
        moved_workspace.binding.revision,
        moved_fixture.config.binding.revision.next().unwrap()
    );
    assert_eq!(
        moved_workspace.trust,
        vcp_domain::workspace::Trust::Untrusted
    );
    let task: Task = moved_store
        .state()
        .record(
            Collection::Task,
            moved_fixture.config.root_task.as_str(),
            &moved_fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(task.state, vcp_domain::task::TaskState::Paused);
    let approvals: Vec<Approval> = moved_store
        .state()
        .records
        .values()
        .filter(|row| row.collection == Collection::Approval)
        .map(|row| row.decode().unwrap())
        .collect();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].state, ApprovalState::Pending);
    moved_store.close().await.unwrap();
    assert_eq!(
        controller.rpc(
            4,
            "controller/read",
            json!({"scope":reload_fixture.scope()})
        )["result"],
        held_controller["result"]
    );
    assert!(controller.finish().await.0.success());
    for (fixture, before) in fixtures.iter().zip(before) {
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        assert_eq!(
            store.state(),
            &before,
            "observer UI must not acquire, resume, answer, or mutate canonical state"
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires VS Code 1.138 and staged extension; run explicitly"]
async fn editor_task_actions_preserve_command_identity_across_reload() {
    let code = PathBuf::from(
        std::env::var_os("VCP_TEST_CODE").expect("VS Code 1.138 executable required"),
    );
    let fixture = Fixture::new(BackendKind::Sqlite).await;
    let child = child(&fixture).await;
    let (host, owner) =
        vcp_lifecycle::foundation::CanonicalHost::open(fixture.config.clone()).unwrap();
    host.command(
        vcp_protocol::command::Command::SetWorkspaceTrust {
            trust: vcp_domain::workspace::Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .unwrap();
    owner.close().await.unwrap();
    drop(host);
    pending(&fixture).await;
    // A historical owner is deliberately stale when the actual editor starts
    // its engine. Freshness projection is not decision authority.
    let mut store = fixture.reopen().await;
    let mut approval: Approval = store
        .state()
        .records
        .values()
        .find(|record| record.collection == Collection::Approval)
        .unwrap()
        .decode()
        .unwrap();
    approval.controller = Some(ControllerId::new());
    approval.owner_epoch = Some(OwnerEpoch::new(1));
    approval.authority = Some(AuthorityRevision::new(1));
    approval.binding = Some(Revision::ZERO);
    approval.revision = Revision::new(1);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: Some(Revision::ZERO),
                record: Record::typed(
                    Collection::Approval,
                    approval.id.as_str(),
                    fixture.config.workspace.clone(),
                    approval.revision,
                    &approval,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let root: Task = store
        .state()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let cli = vcp_cli::terminal::view_at(
        store.state(),
        &root.scope,
        &fixture.config.price.model,
        Timestamp::new(1),
    )
    .unwrap();
    store.close().await.unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = repo.join("artifacts/p4-task-extension-host");
    fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let user = profile.path().join("user-data");
    fs::create_dir_all(user.join("User")).unwrap();
    fs::create_dir_all(user.join("shared-data/sharedStorage")).unwrap();
    let extensions = profile.path().join("extensions");
    fs::create_dir(&extensions).unwrap();
    fs::write(user.join("User/settings.json"), serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","security.workspace.trust.emptyWindow":false,"update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","window.restoreWindows":"none"})).unwrap()).unwrap();
    let workspace = profile.path().join("qualification.code-workspace");
    fs::write(
        &workspace,
        serde_json::to_vec(&json!({"folders":[{"path":editor_path(&fixture.workspace)}]})).unwrap(),
    )
    .unwrap();
    // VS Code 1.138's bundled workspace trust service reads this exact application
    // storage key. Seed only the disposable folder/workspace, with trust enabled;
    // do not use --disable-workspace-trust or change the user's profile.
    let seeded = Command::new(std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"))
        .args(["-e", r#"const fs=require('node:fs');const{DatabaseSync}=require('node:sqlite');const{pathToFileURL}=require('node:url');const db=new DatabaseSync(process.argv[1]);db.exec('CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)');const uriTrustInfo=process.argv.slice(2).map(path=>{const url=pathToFileURL(path);return{trusted:true,uri:{scheme:'file',authority:url.host,path:decodeURIComponent(url.pathname)}}});db.prepare('INSERT INTO ItemTable(key,value) VALUES(?,?)').run('content.trust.model.key',JSON.stringify({uriTrustInfo}));db.close();"#])
        .arg(user.join("shared-data/sharedStorage/state.vscdb")).arg(editor_path(&fixture.workspace)).arg(editor_path(&workspace)).status().unwrap();
    assert!(seeded.success());
    let driver = profile.path().join("qualification-driver");
    fs::create_dir(&driver).unwrap();
    fs::write(driver.join("package.json"),serde_json::to_vec(&json!({"name":"qualification-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}})).unwrap()).unwrap();
    fs::write(driver.join("driver.cjs"), r#"const vscode=require('vscode');const fs=require('node:fs');exports.activate=()=>{setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});};"#).unwrap();
    let result = profile.path().join("result.json");
    let input = profile.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({"mode":"tasks","code":editor_path(&code),"executable":editor_path(Path::new(env!("CARGO_BIN_EXE_vcp"))),"extension":editor_path(&repo.join("artifacts/p4-vscode-extension")),"driver":editor_path(&driver),"runner":editor_path(&repo.join("src/packages/vscode/tests/extension-host.cjs")),"workspaceFile":editor_path(&workspace),"userData":editor_path(&user),"extensions":editor_path(&extensions),"result":editor_path(&result),"stdout":editor_path(&output.join("stdout.log")),"stderr":editor_path(&output.join("stderr.log")),"diagnostics":editor_path(&output.join("editor-logs")),"runtimeEvidence":editor_path(&output.join("runtime.json")),"reloadMarker":editor_path(&profile.path().join("reload-phase.json")),"fixture":{"workspace":editor_path(&fixture.workspace),"data":editor_path(&fixture.data),"scope":fixture.scope(),"rootTask":fixture.config.root_task,"childTask":child,"approval":approval.id,"cli":cli}})).unwrap()).unwrap();
    let script = repo.join("src/packages/vscode/scripts/run-extension-host.ps1");
    let status = tokio::task::spawn_blocking(move || {
        Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(editor_path(&script))
            .arg("-InputFile")
            .arg(editor_path(&input))
            .status()
            .unwrap()
    })
    .await
    .unwrap();
    if let Ok(bytes) = fs::read(&result) {
        fs::write(output.join("result.json"), bytes).unwrap();
    }
    if !status.success() {
        fs::write(
            output.join("failed-profile.txt"),
            editor_path(profile.path()),
        )
        .unwrap();
        let _ = profile.keep();
    }
    assert!(
        status.success(),
        "trusted editor qualification failed; inspect artifacts/p4-task-extension-host"
    );
    let result: serde_json::Value = serde_json::from_slice(&fs::read(result).unwrap()).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["actualTrusted"], true);
    assert_eq!(result["reloadVerified"], true);
    assert_eq!(result["cliParityVerified"], true);
    assert_eq!(result["duplicateVerified"], true);
    assert_eq!(result["staleVerified"], true);
    let store = fixture.reopen_within(Duration::from_secs(45)).await;
    let child: Task = store
        .state()
        .record(Collection::Task, child.as_str(), &fixture.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(child.state, vcp_domain::task::TaskState::Cancelled);
    let approval: Approval = store
        .state()
        .record(
            Collection::Approval,
            approval.id.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        approval.state,
        ApprovalState::Pending,
        "stale owner cannot answer original question"
    );
    let cancel = result["cancelCommand"].as_str().unwrap();
    assert_eq!(
        store
            .state()
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == cancel)
            .count(),
        1,
        "duplicate clicks retain one durable command"
    );
    let denied = result["staleCommand"].as_str().unwrap();
    assert!(
        !store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == denied),
        "stale approval was rejected before durable mutation"
    );
    store.close().await.unwrap();
}
