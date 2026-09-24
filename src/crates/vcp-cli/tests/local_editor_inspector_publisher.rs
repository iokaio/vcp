// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Installed renderer drives the real native encrypted publisher; no cloud service.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use vcp_cli::backup::{self, Backup, Keys};
use vcp_store::{contract::Collection, BackendKind, Store};

fn native(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(path.to_str().unwrap())
        .to_owned()
}
fn require_single_link_git(path: &Path) {
    use std::os::windows::io::AsRawHandle;
    let file = fs::File::open(path).unwrap();
    let mut information = std::mem::MaybeUninit::zeroed();
    // SAFETY: the owned file and native output layout remain valid for this call.
    assert_ne!(
        unsafe {
            windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                file.as_raw_handle(),
                information.as_mut_ptr(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { information.assume_init() }.nNumberOfLinks,
        1,
        "publisher fixture requires the independently pinned single-link Git binary"
    );
}
fn copy_extension(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            copy_extension(&entry.path(), &target.join(entry.file_name()));
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
        }
    }
}
async fn reopen(path: &Path, backend: BackendKind) -> Store {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        match Store::open(path, backend, &[]).await {
            Ok(store) => return store,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "owned engine did not release store: {error}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
fn assert_no_private_profile(directory: &Path, profile: &Path) {
    let needles = [
        native(profile),
        profile.to_string_lossy().into_owned(),
        profile.file_name().unwrap().to_string_lossy().into_owned(),
    ];
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            assert_no_private_profile(&entry.path(), profile);
        } else if entry
            .file_name()
            .to_string_lossy()
            .starts_with("state.vscdb")
        {
            let bytes = fs::read(entry.path()).unwrap();
            for needle in &needles {
                assert!(
                    !bytes
                        .windows(needle.len())
                        .any(|part| part == needle.as_bytes()),
                    "native profile must not persist in extension state"
                );
            }
            assert!(!bytes
                .windows(b"AGE-SECRET-KEY".len())
                .any(|part| part == b"AGE-SECRET-KEY"));
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires pinned native VSCode and normal token"]
async fn installed_editor_publishes_encrypted_backup_and_reloads_only_as_observer() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let code = repo.join("artifacts/p4-editor-archive-1.138.0/runtime/Code.exe");
    assert!(code.is_file(), "qualified archive required");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let server = wiremock::MockServer::start().await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "complete");
        let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
        require_single_link_git(&git);
        for args in [vec!["init", "--quiet"], vec!["add", "--", "value.txt"]] {
            assert!(Command::new(&git)
                .current_dir(&fixture.workspace)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        }
        let entry = fixture.seed(backend).await;
        let workspace = PathBuf::from(&entry.config.binding.root);
        let recovery = fixture.data.join("publisher-recovery");
        let vault = fixture.data.join("publisher-vault");
        let staging = fixture.data.join("publisher-staging");
        for path in [&recovery, &vault, &staging] {
            fs::create_dir(path).unwrap();
        }
        let enrolled = backup::keys(
            &Keys::Create {
                recovery_dir: recovery.clone(),
                sync_root: vec![],
            },
            &fixture.data,
            &workspace,
            Some(&entry),
            None,
        )
        .unwrap();
        let key = PathBuf::from(enrolled["recovery_copy"].as_str().unwrap());
        backup::configure(
            &Backup::Configure {
                vault: vault.clone(),
                staging: staging.clone(),
                expected_revision: None,
                sync_root: vec![],
                automatic: false,
                manual_only: true,
            },
            &fixture.data,
            &workspace,
            &entry,
        )
        .unwrap();
        let publisher_profile = fixture.data.join("private-publisher.json");
        fs::write(
            &publisher_profile,
            serde_json::to_vec(&json!({"version":1,"key":key,"git":git})).unwrap(),
        )
        .unwrap();
        let output = repo
            .join(format!("artifacts/p4-inspector-publisher-editor-{backend:?}").to_lowercase());
        fs::create_dir_all(&output).unwrap();
        let profile = tempfile::tempdir_in(&output).unwrap();
        let user = profile.path().join("user-data");
        fs::create_dir_all(user.join("User")).unwrap();
        fs::create_dir_all(user.join("shared-data/sharedStorage")).unwrap();
        fs::write(user.join("User/settings.json"),serde_json::to_vec(&json!({"security.workspace.trust.enabled":true,"security.workspace.trust.startupPrompt":"never","security.workspace.trust.emptyWindow":false,"update.mode":"none","extensions.autoUpdate":false,"extensions.autoCheckUpdates":false,"telemetry.telemetryLevel":"off","workbench.startupEditor":"none","window.restoreWindows":"all","files.autoSave":"off","files.hotExit":"onExitAndWindowClose"})).unwrap()).unwrap();
        let workspace_file = profile.path().join("publisher.code-workspace");
        fs::write(
            &workspace_file,
            serde_json::to_vec(&json!({"folders":[{"path":native(&workspace)}]})).unwrap(),
        )
        .unwrap();
        let seeded=Command::new(std::env::var_os("VCP_TEST_NODE").unwrap()).args(["-e",r#"const{DatabaseSync}=require('node:sqlite');const{pathToFileURL}=require('node:url');const db=new DatabaseSync(process.argv[1]);db.exec('CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE,value BLOB)');db.prepare('INSERT INTO ItemTable(key,value) VALUES(?,?)').run('content.trust.model.key',JSON.stringify({uriTrustInfo:process.argv.slice(2).map(path=>{const u=pathToFileURL(path);return{trusted:true,uri:{scheme:'file',authority:u.host,path:decodeURIComponent(u.pathname)}}})}));db.close();"#]).arg(user.join("shared-data/sharedStorage/state.vscdb")).arg(native(&workspace)).arg(native(&workspace_file)).status().unwrap();
        assert!(seeded.success());
        let extensions = profile.path().join("extensions");
        fs::create_dir(&extensions).unwrap();
        let installed = extensions.join("vcp.vcp-local-0.1.0");
        copy_extension(&repo.join("artifacts/p4-vscode-extension"), &installed);
        let driver = extensions.join("vcp-test.editor-publisher-driver-0.0.1");
        fs::create_dir(&driver).unwrap();
        fs::write(driver.join("package.json"),r#"{"name":"editor-publisher-driver","publisher":"vcp-test","version":"0.0.1","engines":{"vscode":"1.138.0"},"activationEvents":["*"],"main":"./driver.cjs","extensionKind":["workspace"],"capabilities":{"untrustedWorkspaces":{"supported":true}}}"#).unwrap();
        fs::write(driver.join("driver.cjs"),r#"const vscode=require('vscode'),fs=require('node:fs');exports.activate=()=>setImmediate(async()=>{try{const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));await require(input.runner).run();}catch{}finally{void vscode.commands.executeCommand('workbench.action.quit');}});"#).unwrap();
        let result = profile.path().join("result.json");
        let input = profile.path().join("input.json");
        fs::write(&input,serde_json::to_vec(&json!({"code":native(&code),"installed":true,"cdp":true,"extension":native(&installed),"driver":native(&driver),"runner":native(&repo.join("src/packages/vscode/tests/inspector-publisher-host.cjs")),"workspaceFile":native(&workspace_file),"userData":native(&user),"extensions":native(&extensions),"result":native(&result),"stdout":native(&output.join("stdout.log")),"stderr":native(&output.join("stderr.log")),"diagnostics":native(&output.join("editor-logs")),"runtimeEvidence":native(&output.join("runtime.json")),"workspace":native(&workspace),"data":native(&fixture.data),"executable":env!("CARGO_BIN_EXE_vcp"),"profile":native(&publisher_profile),"marker":native(&profile.path().join("reload.json"))})).unwrap()).unwrap();
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
            serde_json::from_slice(&fs::read(&result).unwrap()).unwrap();
        assert_eq!(evidence["ok"], true, "{evidence}");
        assert_no_private_profile(&user.join("User"), &publisher_profile);
        let store = reopen(&entry.config.canonical_root, backend).await;
        let jobs: Vec<vcp_store::snapshot_jobs::Job> = store
            .state()
            .records
            .values()
            .filter(|r| {
                r.collection == Collection::SnapshotPin
                    && r.value["document_type"] == "vcp_snapshot_job_v1"
            })
            .map(|r| r.decode().unwrap())
            .collect();
        assert_eq!(jobs.len(), 1, "reload must not start another publication");
        let job = &jobs[0];
        assert_eq!(job.id.as_str(), evidence["operation"].as_str().unwrap());
        assert_eq!(job.stage, vcp_store::snapshot_jobs::Stage::Published);
        assert!(!job.active);
        assert!(job.pins.is_empty());
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command == job.id)
                .count(),
            1
        );
        assert!(!store
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        let object = vault.join(
            job.publication.as_ref().unwrap()["object"]
                .as_str()
                .unwrap(),
        );
        let encrypted = fs::read(&object).unwrap();
        assert!(encrypted.starts_with(b"age-encryption.org/v1\n"));
        let forbidden = backup::forbidden(&workspace, &entry.config.canonical_root, &[]).unwrap();
        let trust = vcp_store::trust_store::TrustStore::open(
            &backup::trust_path(&fixture.data, &entry.config.workspace),
            &forbidden,
        )
        .unwrap();
        let directory = vcp_store::keys::RecoveryDirectory::open(
            key.parent().unwrap(),
            &[
                workspace.clone(),
                entry.config.canonical_root.clone(),
                vault.clone(),
                staging.clone(),
            ],
        )
        .unwrap();
        let copy = directory
            .open_copy(key.file_stem().unwrap().to_str().unwrap())
            .unwrap();
        let manifest: vcp_store::vault_crypto::Manifest = serde_json::from_value(
            serde_json::to_value(job).unwrap()["finalization"]["manifest"].clone(),
        )
        .unwrap();
        let verified = trust
            .trust()
            .verify_known_head(
                &object,
                &copy,
                &manifest,
                vcp_store::vault_crypto::Limits::default(),
            )
            .unwrap();
        assert_eq!(manifest.workspace, entry.config.workspace);
        assert_eq!(verified.manifest_digest().len(), 64);
        drop(verified);
        drop(copy);
        drop(trust);
        store.close().await.unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
