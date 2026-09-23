// SPDX-License-Identifier: Apache-2.0
//! Fresh-destination path validation must reach authentication without optional
//! sync configuration, while retaining explicit sync and staging boundaries.
use std::{fs, process::Command};
use vcp_domain::WorkspaceId;
use vcp_store::{
    keys::{LocalKeys, RecoveryDirectory},
    trust_store::TrustStore,
    vault_publish::{Checkpoint, LocalTrust},
};

#[test]
fn fresh_restore_without_sync_roots_reaches_archive_validation_and_preserves_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let data = root.join("data");
    let stage = root.join("staging");
    let recovery = root.join("recovery");
    let sync = root.join("declared-sync");
    let destination = root.join("not-yet-restored");
    let workspace = WorkspaceId::new();
    let trust_path = vcp_cli::backup::trust_path(&data, &workspace);
    for path in [&trust_path, &stage, &recovery, &sync] {
        fs::create_dir_all(path).unwrap();
    }
    let recovery_directory =
        RecoveryDirectory::open(&recovery, &[data.clone(), stage.clone()]).unwrap();
    let keys = LocalKeys::generate().unwrap();
    let copy = keys.export_recovery(&recovery_directory).unwrap();
    let verified = keys.verify_recovery(&copy).unwrap();
    let trust = LocalTrust::enroll(
        &verified,
        workspace.clone(),
        "a".repeat(64),
        Checkpoint {
            sequence: 0,
            deletion: 0,
            parent: None,
        },
    )
    .unwrap();
    drop(TrustStore::enroll(&trust_path, &[stage.clone()], trust).unwrap());
    let source = root.join("unauthenticated.age");
    fs::write(&source, b"deliberately unauthenticated ciphertext").unwrap();
    for backend in ["sqlite", "files"] {
        for (declared, staging, diagnostic) in [
            (None, &stage, "invalid age ciphertext"),
            (Some(&sync), &stage, "invalid age ciphertext"),
            (Some(&data), &stage, "workspace access denied"),
            (None, &data, "workspace access denied"),
        ] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_vcp"));
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            for name in [
                "OneDrive",
                "OneDriveConsumer",
                "OneDriveCommercial",
                "OPENROUTER_API_KEY",
            ] {
                command.env_remove(name);
            }
            command
                .current_dir(&root)
                .args(["--format", "jsonl", "--non-interactive", "--data-dir"])
                .arg(&data)
                .arg("--workspace")
                .arg(&destination)
                .args(["restore", "--workspace-id", workspace.as_str(), "--source"])
                .arg(&source)
                .arg("--key")
                .arg(copy.path())
                .arg("--staging")
                .arg(staging)
                .args(["--backend", backend, "--preview"]);
            if let Some(declared) = declared {
                command.arg("--sync-root").arg(declared);
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(text.contains(diagnostic), "{backend}: {text}");
            assert!(!destination.exists());
            let descriptor = data
                .join("workspaces")
                .join(vcp_protocol::digest_bytes(workspace.as_str().as_bytes()))
                .join("workspace.json");
            assert!(!descriptor.exists());
        }
    }
}
