// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Real installed editor, renderer DOM clicks and governed native inspector replies.
#[path = "support/editor_inspector_fixture.rs"]
mod inspector_fixture;
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::{Client, Fixture};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
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
async fn installed_editor_inspectors_use_real_renderer_and_preserve_controller_both_stores() {
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
        let fixture = Fixture::new(backend).await;
        let expected = inspector_fixture::seed(&fixture).await;
        let mut controller = Client::spawn("local-bridge");
        let mut launch = fixture.bootstrap("controller");
        launch["transport"] = json!("windows_pipe");
        launch["observer_reconnect"] = json!(true);
        controller.send(launch);
        let ready = controller.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        let capabilities = [
            "controller/read",
            "controller/acquire",
            "workspace/open",
            "workspace/setTrust",
            "workspace/binding/1",
            "task/read",
            "artifact/read",
            "memory/forgetPreview",
            "memory/forget",
            "memory/forgetRead",
            "memory/retention/1",
        ];
        let initialized = controller.rpc(1,"initialize",json!({"protocol_version":"1.0","client":{"name":"inspector-external-owner","version":"1"},"capabilities":capabilities,"required_capabilities":capabilities}));
        assert!(initialized.get("error").is_none(), "{initialized}");
        let acquired = controller.rpc(2, "controller/acquire", json!({"scope":fixture.scope(),"command_id":vcp_domain::CommandId::new(),"expected_revision":null}));
        assert!(acquired.get("error").is_none(), "{acquired}");
        let output =
            repo.join(format!("artifacts/p4-inspector-editor-{:?}", backend).to_lowercase());
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
        let driver = extensions.join("vcp-test.editor-inspector-driver-0.0.1");
        fs::create_dir(&driver).unwrap();
        fs::write(driver.join("package.json"),r#"{"name":"editor-inspector-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
        fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode'),fs=require('node:fs');exports.activate=()=>setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});"#).unwrap();

        let result = profile.path().join("result.json");
        let input = profile.path().join("input.json");
        let owner_request = profile.path().join("owner-request.json");
        let owner_ack = profile.path().join("owner-ack.json");
        fs::write(&input,serde_json::to_vec(&json!({
            "code":native(&code),"installed":true,"cdp":true,"extension":native(&installed_extension),"driver":native(&driver),
            "runner":native(&repo.join("src/packages/vscode/tests/inspector-host.cjs")),"workspaceFile":native(&workspace),
            "userData":native(&user),"extensions":native(&extensions),"result":native(&result),
            "stdout":native(&output.join("stdout.log")),"stderr":native(&output.join("stderr.log")),
            "diagnostics":native(&output.join("editor-logs")),"runtimeEvidence":native(&output.join("runtime.json")),
            "workspace":native(&fixture.workspace),"data":native(&fixture.data),"executable":env!("CARGO_BIN_EXE_vcp"),
            "reference":ready["observer_reconnect"],"expected":expected,"expirePreview":backend == BackendKind::Files,"ownerRequest":native(&owner_request),"ownerAck":native(&owner_ack),"marker":native(&profile.path().join("reload.json"))
        })).unwrap()).unwrap();
        let script = repo.join("src/packages/vscode/scripts/run-extension-host.ps1");
        let mut process = Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(native(&script))
            .arg("-InputFile")
            .arg(native(&input))
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(130);
        let mut id = 10u64;
        let mut completed = std::collections::BTreeSet::new();
        let status = loop {
            if let Some(status) = process.try_wait().unwrap() {
                break status;
            }
            if owner_request.exists() {
                let bytes = fs::read(&owner_request).unwrap();
                assert!(bytes.len() <= 128);
                let action: String = serde_json::from_slice(&bytes).unwrap();
                assert!(
                    completed.insert(action.clone()),
                    "owned marker action cannot replay"
                );
                let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    owner_action(
                        &mut controller,
                        &mut id,
                        &fixture,
                        &initialized["result"]["execution_host"]["id"],
                        &expected,
                        &action,
                    )
                }))
                .unwrap_or_else(|_| json!({"failed":true}));
                fs::remove_file(&owner_request).unwrap();
                let next_ack = owner_ack.with_extension("partial");
                fs::write(
                    &next_ack,
                    serde_json::to_vec(&json!({"action":action,"result":answer})).unwrap(),
                )
                .unwrap();
                if owner_ack.exists() {
                    fs::remove_file(&owner_ack).unwrap();
                }
                fs::rename(next_ack, &owner_ack).unwrap();
            }
            if std::time::Instant::now() >= deadline {
                process.kill().unwrap();
                panic!("owned editor fixture deadline");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
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
        assert_eq!(
            completed,
            std::collections::BTreeSet::from(["preview".into(), "purge".into(), "revoke".into()])
        );
        assert_eq!(evidence["retentionInvalidation"], true);
        assert_eq!(evidence["authorityInvalidation"], true);
        assert_eq!(evidence["pruningPreview"], true);
        assert_eq!(evidence["stalePruningPreview"], true);
        if backend == BackendKind::Files {
            assert_eq!(evidence["pruningExpiry"], true);
        }
        assert!(controller.finish().await.0.success());
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        let task: vcp_domain::task::Task = store
            .state()
            .record(
                vcp_store::contract::Collection::Task,
                fixture.config.root_task.as_str(),
                &fixture.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, vcp_domain::task::TaskState::Cancelled);
        assert!(!store.state().records.values().any(|row| matches!(
            row.collection,
            vcp_store::contract::Collection::Attempt | vcp_store::contract::Collection::Effect
        )));
        store.close().await.unwrap();
    }
}

fn owner_action(
    controller: &mut Client,
    id: &mut u64,
    fixture: &Fixture,
    host: &Value,
    expected: &Value,
    action: &str,
) -> Value {
    let mut call = |method: &str, params: Value| {
        *id += 1;
        controller.rpc(*id, method, params)
    };
    let ownership = call("controller/read", json!({"scope":fixture.scope()}));
    assert_eq!(
        ownership["result"]["value"]["ownership"], "this_connection",
        "editor must not take external ownership"
    );
    match action {
        "preview" | "purge" => {
            let preview = call(
                "memory/forgetPreview",
                json!({"scope":fixture.scope(),"task":fixture.config.root_task,"selector":{"schema_version":1,"tree":{"operator":"match","value":{"kind":"path","value":"src/retention-only"}}},"action":"purge","limit":128}),
            );
            assert!(preview.get("error").is_none(), "{preview}");
            let preview = &preview["result"]["value"];
            assert_ne!(preview["selected_count"], "0");
            assert_eq!(preview["protected_count"], "0");
            if action == "preview" {
                return json!({"preview":preview["preview"]});
            }
            let task = call(
                "task/read",
                json!({"scope":fixture.scope(),"task":fixture.config.root_task}),
            );
            assert!(task.get("error").is_none(), "{task}");
            let task = &task["result"]["value"];
            let applied = call(
                "memory/forget",
                json!({"scope":fixture.scope(),"task":fixture.config.root_task,"mutation":{"command_id":vcp_domain::CommandId::new(),"expected_revision":task["revision"],"steering_revision":task["steering_revision"]},"preview":preview["preview"],"preview_digest":preview["digest"]}),
            );
            assert!(applied.get("error").is_none(), "{applied}");
            assert_eq!(
                applied["result"]["value"]["job"]["logical_unavailable"],
                true
            );
            let retained = call(
                "artifact/read",
                json!({"scope":fixture.scope(),"task":fixture.config.root_task,"artifact":expected["retention_artifact"],"offset":"0","length":1024}),
            );
            assert!(
                retained.get("error").is_some(),
                "purged artifact must fail actual authorized reread"
            );
            json!({"logical_unavailable":true})
        }
        "revoke" => {
            let workspace = call(
                "workspace/open",
                json!({"command_id":vcp_domain::CommandId::new(),"host":host,"root":fixture.config.binding.root}),
            );
            assert!(workspace.get("error").is_none(), "{workspace}");
            let workspace = &workspace["result"]["value"];
            assert_eq!(workspace["trust"], "trusted");
            let revoked = call(
                "workspace/setTrust",
                json!({"scope":fixture.scope(),"mutation":{"command_id":vcp_domain::CommandId::new(),"expected_revision":workspace["revision"],"steering_revision":"0"},"expected_binding_revision":workspace["binding_revision"],"trusted":false}),
            );
            assert!(revoked.get("error").is_none(), "{revoked}");
            json!({"accepted":true,"prior_external_ownership":true})
        }
        _ => panic!("unrecognized owned marker action"),
    }
}
