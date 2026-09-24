// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual P4-03 workflow over both canonical backends and a loopback provider.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use vcp_store::BackendKind;
fn native(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(path.to_str().unwrap())
        .to_owned()
}

fn copy_extension(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "staged extension must contain ordinary files"
        );
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_extension(&entry.path(), &destination);
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires pinned native VSCode and normal token"]
async fn editor_workflow_records_partial_and_interrupted_buffer_edits_both_stores() {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let code = PathBuf::from(std::env::var_os("VCP_TEST_CODE").expect("pinned editor required"));
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                calls.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(execution_fixture::response(0, "complete"))
                    .set_delay(Duration::from_secs(90))
            })
            .mount(&server)
            .await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "complete");
        fs::write(fixture.workspace.join("second.txt"), "second\n").unwrap();
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        config["affected_paths"] = json!(["value.txt", "second.txt"]);
        fs::write(&fixture.profile, serde_json::to_vec(&config).unwrap()).unwrap();
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let name = if backend == BackendKind::Files {
            "files"
        } else {
            "sqlite"
        };
        let output = repo.join(format!("artifacts/p4-editor-workflow-{name}"));
        fs::create_dir_all(&output).unwrap();
        let profile = tempfile::tempdir_in(&output).unwrap();
        let user = profile.path().join("user-data");
        fs::create_dir_all(user.join("User")).unwrap();
        fs::create_dir_all(user.join("shared-data/sharedStorage")).unwrap();
        fs::write(user.join("User/settings.json"),serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","security.workspace.trust.emptyWindow":false,"update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","window.restoreWindows":"all","files.autoSave":"off","files.hotExit":"onExitAndWindowClose","editor.formatOnSave":false})).unwrap()).unwrap();
        let outside = profile.path().join("other-root");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("value.txt"), "outside\n").unwrap();
        let workspace = profile.path().join("qualification.code-workspace");
        fs::write(&workspace,serde_json::to_vec(&json!({"folders":[{"name":"same-name","path":native(&fixture.workspace)},{"name":"same-name","path":native(&outside)}]})).unwrap()).unwrap();
        let seeded=Command::new(std::env::var_os("VCP_TEST_NODE").expect("pinned Node required")).args(["-e",r#"const{DatabaseSync}=require('node:sqlite');const{pathToFileURL}=require('node:url');const db=new DatabaseSync(process.argv[1]);db.exec('CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE,value BLOB)');db.prepare('INSERT INTO ItemTable(key,value) VALUES(?,?)').run('content.trust.model.key',JSON.stringify({uriTrustInfo:process.argv.slice(2).map(path=>{const u=pathToFileURL(path);return{trusted:true,uri:{scheme:'file',authority:u.host,path:decodeURIComponent(u.pathname)}}})}));db.close();"#]).arg(user.join("shared-data/sharedStorage/state.vscdb")).arg(native(&fixture.workspace)).arg(native(&outside)).arg(native(&workspace)).status().unwrap();
        assert!(seeded.success());
        let extensions = profile.path().join("extensions");
        fs::create_dir(&extensions).unwrap();
        let installed_extension = extensions.join("vcp.vcp-local-0.1.0");
        copy_extension(
            &repo.join("artifacts/p4-vscode-extension"),
            &installed_extension,
        );
        let driver = extensions.join("vcp-test.editor-workflow-driver-0.0.1");
        fs::create_dir(&driver).unwrap();
        fs::write(driver.join("package.json"),r#"{"name":"editor-workflow-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
        fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode'),fs=require('node:fs');exports.activate=()=>setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});"#).unwrap();
        let result = profile.path().join("result.json");
        let input = profile.path().join("input.json");
        fs::write(&input,serde_json::to_vec(&json!({"code":native(&code),"installed":true,"extension":native(&installed_extension),"driver":native(&driver),"runner":native(&repo.join("src/packages/vscode/tests/editor-workflow-host.cjs")),"workspaceFile":native(&workspace),"userData":native(&user),"extensions":native(&extensions),"result":native(&result),"stdout":native(&output.join("stdout.log")),"stderr":native(&output.join("stderr.log")),"diagnostics":native(&output.join("editor-logs")),"runtimeEvidence":native(&output.join("runtime.json")),"workspace":native(&fixture.workspace),"data":native(&fixture.data),"profile":native(&fixture.profile),"executable":env!("CARGO_BIN_EXE_vcp"),"outside":native(&outside.join("value.txt")),"marker":native(&profile.path().join("reload.json"))})).unwrap()).unwrap();
        let script = repo.join("src/packages/vscode/scripts/run-extension-host.ps1");
        let started = std::time::Instant::now();
        let status = tokio::task::spawn_blocking(move || {
            Command::new("pwsh")
                .args(["-NoProfile", "-File"])
                .arg(native(&script))
                .arg("-InputFile")
                .arg(native(&input))
                .status()
                .unwrap()
        })
        .await
        .unwrap();
        if let Ok(bytes) = fs::read(&result) {
            fs::write(output.join("result.json"), bytes).unwrap();
        }
        assert!(status.success(), "inspect {}", output.display());
        let evidence: serde_json::Value =
            serde_json::from_slice(&fs::read(result).unwrap()).unwrap();
        if evidence["ok"] != true {
            fs::write(output.join("failed-profile.txt"), native(&profile.keep())).unwrap();
        }
        assert_eq!(evidence["ok"], true, "{evidence}");
        for field in [
            "rootMismatch",
            "appliedReceipt",
            "saveClose",
            "reopen",
            "rename",
            "delete",
            "stalePreview",
            "partialFiles",
            "reload",
            "undoBeforeReceipt",
            "noReplay",
        ] {
            assert_eq!(evidence[field], true, "{field}");
        }
        assert!(started.elapsed() < Duration::from_secs(90));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "42\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("second.txt")).unwrap(),
            "second\n"
        );
        assert_eq!(
            fs::read_to_string(outside.join("value.txt")).unwrap(),
            "outside\n"
        );
        let store = tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                match vcp_store::Store::open(&entry.config.canonical_root, backend, &[]).await {
                    Ok(store) => break store,
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
        .unwrap();
        let changes: Vec<vcp_domain::editor::ChangeSet> = store
            .state()
            .records
            .values()
            .filter(|row| row.value["document_type"] == vcp_domain::editor::CHANGE)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(changes.len(), 4);
        assert!(changes.iter().all(|change| change.schema_version == 2));
        assert!(store.state().records.values().any(|row| {
            row.value["document_type"] == "vcp_editor_buffers_v2"
                && row.value["schema_version"] == 2
        }));
        fs::write(
            output.join("canonical-changes.json"),
            serde_json::to_vec_pretty(&changes).unwrap(),
        )
        .unwrap();
        assert!(changes.iter().any(|change| change.files.len() == 2
            && change.files[0].state == vcp_domain::editor::FileState::Applied
            && change.files[1].state == vcp_domain::editor::FileState::Rejected
            && change.files[1].execution.is_none()));
        let interrupted = changes
            .iter()
            .find(|change| change.id == evidence["interrupted"].as_str().unwrap())
            .unwrap();
        assert!(matches!(
            interrupted.files[0].state,
            vcp_domain::editor::FileState::Dispatched | vcp_domain::editor::FileState::Unknown
        ));
        let effect = store
            .state()
            .records
            .values()
            .filter(|row| row.collection == vcp_store::contract::Collection::Effect)
            .find(|row| row.id == interrupted.files[0].effect.as_str())
            .unwrap();
        let effect: vcp_domain::effect::Effect = effect.decode().unwrap();
        fs::write(
            output.join("interrupted-effect.json"),
            serde_json::to_vec_pretty(&effect).unwrap(),
        )
        .unwrap();
        assert_eq!(effect.execution, interrupted.files[0].execution);
        assert!(matches!(
            effect.state,
            vcp_domain::effect::EffectState::DispatchRecorded
                | vcp_domain::effect::EffectState::OutcomeUnknown
        ));
        store.close().await.unwrap();
    }
}
