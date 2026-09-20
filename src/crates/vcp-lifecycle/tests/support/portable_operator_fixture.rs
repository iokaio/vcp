// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_store::contract::Collection;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "exports independent recovery material only to an explicit private qualification directory"]
async fn export_operator_handoff_fixture_with_child_claim_and_real_accounting() {
    let export = std::path::PathBuf::from(
        std::env::var_os("VCP_TEST_U04_EXPORT").expect("explicit private export root"),
    );
    std::fs::create_dir_all(&export).unwrap();
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, config, host, owner, test, thread) =
            super::memory_publication::vector_fixture(backend).await;
        let _source = super::memory_publication::retained_source(&host, &config, thread);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let child = super::portable_accounting::enrich_native(&config).await;
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let workspace_state: vcp_domain::workspace::Workspace = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        host.command(
            Command::SetWorkspaceTrust {
                trust: vcp_domain::workspace::Trust::Trusted,
            },
            None,
            workspace_state.revision,
        )
        .unwrap();

        let workspace = std::path::Path::new(&config.binding.root);
        let executable = std::path::PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
        let template = temp.path().join("empty-template");
        let hooks = temp.path().join("empty-hooks");
        std::fs::create_dir(&template).unwrap();
        std::fs::create_dir(&hooks).unwrap();
        std::fs::write(workspace.join("tracked.txt"), "staged\n").unwrap();
        for args in [
            vec![
                "init".to_string(),
                "--quiet".into(),
                format!("--template={}", template.display()),
            ],
            vec!["add".into(), "--".into(), "tracked.txt".into()],
        ] {
            assert!(std::process::Command::new(&executable)
                .current_dir(workspace)
                .args(["-c", &format!("core.hooksPath={}", hooks.display())])
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        }
        std::fs::write(workspace.join("tracked.txt"), "unstaged\n").unwrap();
        std::fs::write(
            workspace.join("untracked.txt"),
            "required untracked bytes\n",
        )
        .unwrap();
        let git = Arc::new(
            vcp_repository::git::Git::new(
                executable,
                ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"]
                    .into_iter()
                    .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
                    .collect(),
                Duration::from_secs(15),
                4 * 1024 * 1024,
            )
            .unwrap(),
        );
        let checkpoint = host
            .capture_backup_checkpoint(git, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        let state = host.snapshot().unwrap();
        let root_scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        };
        let ledger = vcp_budget::ledger(&state, &root_scope).unwrap();
        assert_eq!(ledger.settled, Micros::new(50));
        assert_eq!(ledger.unresolved, Micros::new(67));
        let claims: Vec<_> = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Claim)
            .map(|r| r.id.clone())
            .collect();
        assert!(!claims.is_empty());
        let attempts = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .count();
        assert_eq!(attempts, 2);
        let output = export.join(match backend {
            BackendKind::Files => "files",
            BackendKind::Sqlite => "sqlite",
        });
        std::fs::create_dir(&output).unwrap();
        let recovery = output.join("recovery");
        let staging = temp.path().join("staging");
        let vault = temp.path().join("vault");
        let trust_path = temp.path().join("trust");
        for path in [&recovery, &staging, &vault, &trust_path] {
            std::fs::create_dir(path).unwrap();
        }
        let forbidden = vec![workspace.to_owned(), config.canonical_root.clone()];
        let keys = vcp_store::keys::LocalKeys::generate().unwrap();
        let copy = keys
            .export_recovery(
                &vcp_store::keys::RecoveryDirectory::open(&recovery, &forbidden).unwrap(),
            )
            .unwrap();
        // Independent operator recovery transfer contains keys only, not ciphertext.
        {
            let handoff = export.parent().unwrap().join("transfer/Recovery");
            let handoff = std::path::PathBuf::from(handoff)
                .join(match backend {
                    BackendKind::Files => "files",
                    BackendKind::Sqlite => "sqlite",
                })
                .join("recovery");
            std::fs::create_dir_all(&handoff).unwrap();
            keys.export_recovery(
                &vcp_store::keys::RecoveryDirectory::open(&handoff, &forbidden).unwrap(),
            )
            .unwrap();
        }
        let keys = keys.verify_recovery(&copy).unwrap();
        let trust = vcp_store::vault_publish::LocalTrust::enroll(
            &keys,
            config.workspace.clone(),
            "f".repeat(64),
            vcp_store::vault_publish::Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let trust =
            vcp_store::trust_store::TrustStore::enroll(&trust_path, &forbidden, trust).unwrap();
        let setup = vcp_lifecycle::foundation::backup::Setup {
            schema_version: 1,
            revision: 0,
            previous: "0".repeat(64),
            workspace: config.workspace.clone(),
            vault,
            staging,
            sync_roots: vec![],
            automatic: false,
        };
        let caps = Arc::new(
            vcp_lifecycle::foundation::backup_run::Capabilities::open(
                trust,
                keys,
                &setup,
                workspace,
                &config.canonical_root,
            )
            .unwrap(),
        );
        let published = host
            .publish_backup(
                caps,
                CommandId::new(),
                Some(vcp_store::snapshot_inputs::Inputs {
                    checkpoint: Some(checkpoint),
                    generations: vec![],
                }),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        let receipt = published.publication.unwrap();
        let ciphertext = output.join("snapshot.age");
        std::fs::copy(
            setup.vault.join(receipt["object"].as_str().unwrap()),
            &ciphertext,
        )
        .unwrap();
        std::fs::write(output.join("fixture.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "workspace":config.workspace,"session":config.session,"root_task":config.root_task,"child_task":child,"claim_records":claims,
            "settled":"50","unresolved":"67","attempts":attempts,"provider_requests":0,
            "lineage":"f".repeat(64),"checkpoint":{"sequence":0,"deletion":0,"parent":null},
            "source":ciphertext,"recovery_directory":recovery,"ciphertext_sha256":receipt["ciphertext_sha256"],"bytes":receipt["bytes"],
            "expected_files":{"tracked.txt":"unstaged\n","untracked.txt":"required untracked bytes\n","source.rs":"pub fn retained_answer() -> u32 { 42 }\n"}
        })).unwrap()).unwrap();
        owner.close().await.unwrap();
    }
}
