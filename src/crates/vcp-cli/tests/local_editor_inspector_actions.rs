// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Positive configured optimizer/cost UI over an isolated synthetic loopback provider.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
#[path = "support/editor_inspector_routing.rs"]
mod routing_fixture;
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

fn assert_no_retained_inspector_content(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            assert_no_retained_inspector_content(&entry.path());
        } else if entry
            .file_name()
            .to_string_lossy()
            .starts_with("state.vscdb")
        {
            let bytes = fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(b"inspector-native-sentinel".len())
                    .any(|part| part == b"inspector-native-sentinel"),
                "inspector content persisted in editor state"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires pinned native VSCode and normal token"]
async fn installed_editor_optimizer_actions_and_cost_use_real_configured_engine_both_stores() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let code = repo.join("artifacts/p4-editor-archive-1.138.0/runtime/Code.exe");
    assert!(
        code.is_file(),
        "qualified archive is required; no runtime fallback"
    );
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let provider = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                requests.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(execution_fixture::response(2, "complete"))
            })
            .mount(&provider)
            .await;
        let fixture = execution_fixture::Fixture::new(&provider.uri(), "complete");
        routing_fixture::configure(&fixture.profile);
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let output = repo
            .join(format!("artifacts/p4-inspector-actions-editor-{:?}", backend).to_lowercase());
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
        let driver = extensions.join("vcp-test.editor-inspector-actions-driver-0.0.1");
        fs::create_dir(&driver).unwrap();
        fs::write(driver.join("package.json"),r#"{"name":"editor-inspector-actions-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
        fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode'),fs=require('node:fs');exports.activate=()=>setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});"#).unwrap();

        let result = profile.path().join("result.json");
        let input = profile.path().join("input.json");
        fs::write(&input,serde_json::to_vec(&json!({
            "code":native(&code),"installed":true,"cdp":true,"extension":native(&installed_extension),"driver":native(&driver),
            "runner":native(&repo.join("src/packages/vscode/tests/inspector-actions-host.cjs")),"workspaceFile":native(&workspace),
            "userData":native(&user),"extensions":native(&extensions),"result":native(&result),
            "stdout":native(&output.join("stdout.log")),"stderr":native(&output.join("stderr.log")),
            "diagnostics":native(&output.join("editor-logs")),"runtimeEvidence":native(&output.join("runtime.json")),
            "workspace":native(&fixture.workspace),"data":native(&fixture.data),"executable":env!("CARGO_BIN_EXE_vcp"),
            "profile":native(&fixture.profile),"marker":native(&profile.path().join("reload.json"))
        })).unwrap()).unwrap();
        let script = repo.join("src/packages/vscode/scripts/run-extension-host.ps1");
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
        assert_no_retained_inspector_content(&user.join("User"));
        assert!(
            count.load(Ordering::SeqCst) > 0 && count.load(Ordering::SeqCst) <= 8,
            "bounded private loopback requests"
        );
        for field in [
            "actualRenderer",
            "costLedger",
            "reportCapture",
            "apply",
            "stalePreview",
            "rollback",
            "noReplay",
        ] {
            assert_eq!(evidence[field], true, "{field}");
        }
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
        let workspace: vcp_domain::workspace::Workspace = store
            .state()
            .record(
                vcp_store::contract::Collection::Workspace,
                entry.config.workspace.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let access = vcp_memory::access::Access {
            workspace: entry.config.workspace.clone(),
            actor: entry.config.actor.clone(),
            authority: workspace.authority,
            read: true,
            write: false,
            tasks: None,
        };
        let policy = vcp_lifecycle::foundation::routing_state::current_policy(&store, &access)
            .unwrap()
            .unwrap();
        assert_eq!(policy.revision.get(), 3);
        assert!(store
            .state()
            .records
            .values()
            .any(|row| row.collection == vcp_store::contract::Collection::Ledger));
        store.close().await.unwrap();
    }
}
