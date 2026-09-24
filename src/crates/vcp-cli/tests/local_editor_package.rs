// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
#[path = "support/local_fixture.rs"]
mod local_fixture;
#[path = "support/editor_package.rs"]
mod package;
use package::{native, Package};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use vcp_store::BackendKind;
async fn run(
    package: &Package,
    repo: &Path,
    code: &Path,
    user: &Path,
    extensions: &Path,
    fixture: &local_fixture::Fixture,
    workspace: &Path,
    mode: &str,
    version: &str,
    engine: &Path,
    output: &Path,
) -> Value {
    let runner = package.copy_driver(repo, "package-host.cjs");
    let script = package.launcher(repo);
    let driver = package.root.join("package-driver-source");
    fs::create_dir_all(&driver).unwrap();
    fs::write(driver.join("package.json"),r#"{"name":"package-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
    fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode'),fs=require('node:fs');exports.activate=()=>setImmediate(async()=>{try{const i=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(i.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});"#).unwrap();
    package.install_driver(repo, code, user, extensions, &driver);
    let result = package.root.join(format!("{mode}-result.json"));
    let input = package.root.join(format!("{mode}-input.json"));
    fs::write(&input,serde_json::to_vec(&json!({"restrictedPath":true,"code":native(code),"editorRoot":native(code.parent().unwrap()),"installed":true,"runner":native(&runner),"workspaceFile":native(workspace),"userData":native(user),"extensions":native(extensions),"result":native(&result),"stdout":native(&output.join(format!("{mode}-stdout.log"))),"stderr":native(&output.join(format!("{mode}-stderr.log"))),"diagnostics":native(&output.join(format!("{mode}-editor-logs"))),"runtimeEvidence":native(&output.join(format!("{mode}-runtime.json"))),"workspace":native(&fixture.workspace),"data":native(&fixture.data),"executable":native(engine),"scope":fixture.scope(),"task":fixture.config.root_task,"mode":mode,"version":version,"checkout":native(repo),"reloadMarker":native(&package.root.join("reload.json"))})).unwrap()).unwrap();
    let mut command = package.launch(&script, &input);
    let status = tokio::task::spawn_blocking(move || command.status().unwrap())
        .await
        .unwrap();
    if let Ok(bytes) = fs::read(&result) {
        fs::write(output.join(format!("{mode}-result.json")), bytes).unwrap();
    }
    assert!(
        status.success(),
        "editor launcher failed: {}",
        output.display()
    );
    let result: Value = serde_json::from_slice(&fs::read(result).unwrap()).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "actual pinned editor VSIX installation outside checkout; normal token required"]
async fn actual_vsix_install_update_failure_replacement_and_uninstall_preserve_state_both_stores() {
    let pinned = tempfile::tempdir().unwrap();
    package::pin_inputs(pinned.path());
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let code =
        PathBuf::from(std::env::var_os("VCP_TEST_CODE").expect("exact qualified editor required"));
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = local_fixture::Fixture::new(backend).await;
        let owned = tempfile::tempdir().unwrap();
        assert!(!owned.path().starts_with(&repo));
        let package = Package::prepare(&owned.path().join("distribution"));
        let output = repo.join(format!("artifacts/p4-package-editor-{backend:?}").to_lowercase());
        fs::create_dir_all(&output).unwrap();
        let user = owned.path().join("user-data");
        let extensions = owned.path().join("extensions");
        fs::create_dir_all(user.join("User")).unwrap();
        fs::create_dir(&extensions).unwrap();
        fs::write(user.join("User/settings.json"),serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","files.autoSave":"off"})).unwrap()).unwrap();
        let workspace = owned.path().join("package.code-workspace");
        fs::write(
            &workspace,
            serde_json::to_vec(&json!({"folders":[{"path":native(&fixture.workspace)}]})).unwrap(),
        )
        .unwrap();
        let independent = owned.path().join("independent-recovery-key");
        let key =
            vcp_protocol::digest_bytes(vcp_domain::ActorId::new().as_str().as_bytes()).into_bytes();
        fs::write(&independent, &key).unwrap();
        let sentinel = fixture.data.join("preserve-user-data");
        fs::write(
            &sentinel,
            b"independent canonical data root survives extension lifecycle",
        )
        .unwrap();
        package.install(&code, &user, &extensions);
        let first = run(
            &package,
            &repo,
            &code,
            &user,
            &extensions,
            &fixture,
            &workspace,
            "install",
            "0.1.0",
            &package.engine,
            &output,
        )
        .await;
        assert_eq!(first["reloaded"], true);
        assert!(first["unsupportedNegotiation"].is_object());
        assert_eq!(first["developmentPathAbsent"], true);
        {
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            local_fixture::assert_offline_paused(&store, &fixture.config);
        }
        run(
            &package,
            &repo,
            &code,
            &user,
            &extensions,
            &fixture,
            &workspace,
            "restart",
            "0.1.0",
            &package.engine,
            &output,
        )
        .await;
        // An interrupted download is a distinctly named truncated archive. The actual
        // editor installer rejects it and retains the installed known-good version.
        // Explicit disconnect removes the rendezvous reference while the native
        // observer owner retains its normal idle grace. Prove that owner released
        // canonical storage before a fresh launch, as required for replacement.
        {
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            local_fixture::assert_offline_paused(&store, &fixture.config);
        }
        let corrupt = owned.path().join("interrupted-successor.vsix");
        fs::write(&corrupt, b"PK\x03\x04incomplete qualification download").unwrap();
        let failed = package.editor_cli(
            &code,
            &user,
            &extensions,
            &["--install-extension", &native(&corrupt), "--force"],
        );
        assert!(!failed.status.success());
        let retained = package.editor_cli(
            &code,
            &user,
            &extensions,
            &["--list-extensions", "--show-versions"],
        );
        assert!(retained.status.success());
        assert!(
            String::from_utf8_lossy(&retained.stdout)
                .lines()
                .any(|line| line.trim() == "vcp.vcp-local@0.1.0"),
            "failed VSIX update must retain installed original"
        );
        run(
            &package,
            &repo,
            &code,
            &user,
            &extensions,
            &fixture,
            &workspace,
            "failed-update",
            "0.1.0",
            &package.engine,
            &output,
        )
        .await;
        let successor = PathBuf::from(
            std::env::var_os("VCP_TEST_VSIX_UPDATE")
                .expect("separately packaged qualification successor required"),
        );
        let update = package.editor_cli(
            &code,
            &user,
            &extensions,
            &["--install-extension", &native(&successor), "--force"],
        );
        assert!(
            update.status.success(),
            "{}",
            String::from_utf8_lossy(&update.stderr)
        );
        {
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            local_fixture::assert_offline_paused(&store, &fixture.config);
        }
        let replacement = package.root.join("native-release-b");
        package::copy_tree(package.engine.parent().unwrap(), &replacement);
        let replaced = replacement.join("vcp.exe");
        run(
            &package,
            &repo,
            &code,
            &user,
            &extensions,
            &fixture,
            &workspace,
            "replacement",
            "0.1.1",
            &replaced,
            &output,
        )
        .await;
        run(
            &package,
            &repo,
            &code,
            &user,
            &extensions,
            &fixture,
            &workspace,
            "missing",
            "0.1.1",
            &owned.path().join("absent-release/vcp.exe"),
            &output,
        )
        .await;
        let uninstall = package.editor_cli(
            &code,
            &user,
            &extensions,
            &["--uninstall-extension", "vcp.vcp-local"],
        );
        assert!(uninstall.status.success());
        let listed = package.editor_cli(
            &code,
            &user,
            &extensions,
            &["--list-extensions", "--show-versions"],
        );
        assert!(listed.status.success());
        assert!(!String::from_utf8_lossy(&listed.stdout)
            .lines()
            .any(|line| line.starts_with("vcp.vcp-local@")));
        assert_eq!(fs::read(&independent).unwrap(), key);
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"independent canonical data root survives extension lifecycle"
        );
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        local_fixture::assert_offline_paused(&store, &fixture.config);
        fs::write(output.join("lifecycle.json"),serde_json::to_vec_pretty(&json!({"ok":true,"backend":format!("{backend:?}"),"actualVsixInstall":true,"actualVersionUpdate":"0.1.0 -> qualification-only 0.1.1 same code","truncatedArchiveRejected":true,"engineReplacement":"distinct path, same qualified native build; no downgrade claim","missingEngine":true,"actualUninstall":true,"dataPreserved":true,"independentKeyPreserved":true,"unsupportedNegotiation":"real native protocol99.0 rejection; not oldbinary downgrade"})).unwrap()).unwrap();
    }
}
