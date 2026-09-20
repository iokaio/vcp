// SPDX-License-Identifier: Apache-2.0
use vcp_cli::{
    settings::WorkspaceEntry,
    storage::{self, Backend, Storage},
};
use vcp_domain::{accounting::*, ids::*, revision::*, workspace::*};
use vcp_lifecycle::foundation::Config;
use vcp_store::{BackendKind, Store};
fn config(root: &std::path::Path, backend: BackendKind) -> Config {
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: root.join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: root.to_string_lossy().into_owned(),
            repository: "output-fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "fixture".into(),
            model: "fixture/model".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: [
                ChargeCategory::Input,
                ChargeCategory::Output,
                ChargeCategory::CacheRead,
                ChargeCategory::CacheWrite,
                ChargeCategory::Request,
                ChargeCategory::ProviderTool,
            ]
            .into_iter()
            .map(|kind| {
                (
                    kind,
                    Rate {
                        micros: Micros::ZERO,
                        per_units: Units::new(1),
                    },
                )
            })
            .collect(),
        },
        input_ceiling: Units::new(4096),
        output_ceiling: Units::new(1024),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        host_tool_denials: vec![],
    }
}

#[tokio::test]
async fn conversion_reopens_exact_state_and_reconciles_lost_ack_without_changing_old_root() {
    for (source_backend, target_backend) in [
        (BackendKind::Files, Backend::Sqlite),
        (BackendKind::Sqlite, Backend::Files),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let data = base.join("data");
        let workspace = base.join("workspace");
        let directory = data.join("workspaces").join("fixture");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::create_dir(&workspace).unwrap();
        let cfg = config(&directory, source_backend);
        let original = cfg.canonical_root.clone();
        let store = Store::open(&original, source_backend, std::slice::from_ref(&workspace))
            .await
            .unwrap();
        let state = store.state().clone();
        store.close().await.unwrap();
        let entry = WorkspaceEntry {
            rebind_pending: false,
            version: 1,
            config: cfg,
            identity: None,
        };
        std::fs::write(
            directory.join("workspace.json"),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        let preview = storage::execute(
            &Storage::Migrate {
                backend: target_backend,
                preview: true,
                expected_descriptor: None,
                operation: None,
            },
            &data,
            &directory,
            &workspace,
        )
        .await
        .unwrap();
        let command = Storage::Migrate {
            backend: target_backend,
            preview: false,
            expected_descriptor: Some(preview["expected_descriptor"].as_str().unwrap().into()),
            operation: Some(preview["operation"].as_str().unwrap().into()),
        };
        let activated = storage::execute(&command, &data, &directory, &workspace)
            .await
            .unwrap();
        assert_eq!(activated["activated"], true);
        let retry = storage::execute(&command, &data, &directory, &workspace)
            .await
            .unwrap();
        assert_eq!(retry["reconciled"], true);
        let selected: WorkspaceEntry =
            serde_json::from_slice(&std::fs::read(directory.join("workspace.json")).unwrap())
                .unwrap();
        assert_eq!(selected.version, 2);
        let reopened = Store::open(
            &selected.config.canonical_root,
            target_backend.kind(),
            std::slice::from_ref(&workspace),
        )
        .await
        .unwrap();
        assert_eq!(reopened.state(), &state);
        reopened.close().await.unwrap();
        let old = Store::open(&original, source_backend, std::slice::from_ref(&workspace))
            .await
            .unwrap();
        assert_eq!(old.state(), &state);
        old.close().await.unwrap();
        let altered = Storage::Migrate {
            backend: target_backend,
            preview: false,
            expected_descriptor: Some("0".repeat(64)),
            operation: Some(preview["operation"].as_str().unwrap().into()),
        };
        assert!(storage::execute(&altered, &data, &directory, &workspace)
            .await
            .unwrap_err()
            .contains("payload mismatch"));
    }
}
