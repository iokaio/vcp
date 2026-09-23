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
    let mut before = Vec::new();
    for fixture in &fixtures {
        pending(fixture).await;
        let store = fixture.reopen().await;
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
    fs::write(&workspace,serde_json::to_vec(&json!({"folders":fixtures.iter().map(|fixture|json!({"name":"same-root","path":editor_path(&fixture.workspace)})).collect::<Vec<_>>() })).unwrap()).unwrap();
    let result = profile.path().join("result.json");
    let input = profile.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({"code":editor_path(&code),"executable":editor_path(Path::new(env!("CARGO_BIN_EXE_vcp"))),"extension":editor_path(&repo.join("artifacts/p4-vscode-extension")),"runner":editor_path(&repo.join("src/packages/vscode/tests/extension-host.cjs")),"workspaceFile":editor_path(&workspace),"userData":editor_path(&user),"extensions":editor_path(&extensions),"result":editor_path(&result),"stdout":editor_path(&output.join("stdout.log")),"stderr":editor_path(&output.join("stderr.log")),"diagnostics":editor_path(&output.join("editor-logs")),"runtimeEvidence":editor_path(&output.join("runtime.json")),"unselected":editor_path(&profile.path().join("not-a-workspace-folder")),"fixtures":fixtures.iter().map(|fixture|json!({"workspace":editor_path(&fixture.workspace),"data":editor_path(&fixture.data),"scope":fixture.scope(),"host":fixture.config.binding.host,"canonicalRoot":fixture.config.binding.root,"rootId":RootId::parse(fixture.config.workspace.as_str()).unwrap(),"bindingRevision":fixture.config.binding.revision.get().to_string()})).collect::<Vec<_>>() })).unwrap()).unwrap();
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
    assert!(
        status.success(),
        "editor qualification failed; inspect artifacts/p4-extension-host logs"
    );
    let result_bytes = fs::read(result).unwrap();
    let result: serde_json::Value = serde_json::from_slice(&result_bytes).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["version"], "1.138.0");
    fs::write(output.join("result.json"), result_bytes).unwrap();
    for (fixture, before) in fixtures.iter().zip(before) {
        let store = fixture.reopen_within(Duration::from_secs(20)).await;
        assert_eq!(
            store.state(),
            &before,
            "observer UI must not acquire, resume, answer, or mutate canonical state"
        );
        store.close().await.unwrap();
    }
}
