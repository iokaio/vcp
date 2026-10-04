// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use vcp_domain::{
    accounting::*, ids::*, revision::*, task::*, verification::Fingerprint, workspace::*,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::command::Command;
use vcp_store::{contract::Collection, BackendKind};

struct NoRequests(AtomicUsize);
impl ReceiptSource for NoRequests {
    fn fetch<'a>(
        &'a self,
        _: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ReceiptFetch, String>> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err("unexpected metadata request".into()) })
    }
}

async fn fixture(
    temp: &tempfile::TempDir,
    backend: BackendKind,
) -> (crate::settings::WorkspaceEntry, std::path::PathBuf) {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    let config = Config {
        canonical_root: temp.path().join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::new(100),
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
            .map(|category| {
                (
                    category,
                    Rate {
                        micros: Micros::ZERO,
                        per_units: Units::new(1),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        },
        input_ceiling: Units::new(1000),
        output_ceiling: Units::new(100),
        artifact_limit: ByteCount::new(16 * 1024 * 1024),
        max_transport_retries: 0,
        host_tool_denials: vec![],
    };
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "billing-only fixture".into(),
                constraints: vec![],
                acceptance: vec![],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.initialize_root_budget().unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Paused,
            reason: "billing-only maintenance".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    let root = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: config.workspace.clone(),
            root: RootId::parse(config.workspace.as_str()).unwrap(),
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        },
        &workspace,
    )
    .unwrap();
    let identity = Some(crate::binding::capture(&root).unwrap());
    owner.close().await.unwrap();
    drop(host);
    (
        crate::settings::WorkspaceEntry {
            version: 1,
            rebind_pending: false,
            config,
            identity,
        },
        workspace,
    )
}

#[tokio::test]
async fn paused_root_reconciliation_preserves_state_budget_and_never_starts_inference() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut entry, workspace) = fixture(&temp, backend).await;
        let store = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        let before = store.state().clone();
        store.close().await.unwrap();
        // A stale descriptor must not replace the original admitted cap.
        entry.config.cap.micros = Micros::new(999999);
        let source = Arc::new(NoRequests(AtomicUsize::new(0)));
        let (code, report) =
            execute_with_source(&entry, &entry.config.root_task, &workspace, || {
                Ok(source.clone())
            })
            .await
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(report["kind"], "provider_cost_reconciliation");
        assert_eq!(report["metadata_only"], true);
        assert_eq!(report["resumed"], false);
        assert_eq!(report["state"], "paused");
        assert_eq!(report["ledger"]["cap"], "1000");
        let scope: Scope = serde_json::from_value(report["scope"].clone()).unwrap();
        let mut frame = Vec::new();
        crate::app::emit_command_outcome(&mut frame, &report, code, Some(&scope)).unwrap();
        assert_eq!(frame.iter().filter(|byte| **byte == b'\n').count(), 1);
        let framed: serde_json::Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(framed["type"], "result");
        assert_eq!(framed["schema_version"], 1);
        assert_eq!(framed["exit_code"], code);
        assert_eq!(framed["scope"], report["scope"]);
        assert_eq!(framed["data"], report);
        assert!(!framed["correlation"].as_str().unwrap().is_empty());
        assert_eq!(source.0.load(Ordering::SeqCst), 0);
        let store = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&before).unwrap(),
            serde_json::to_value(store.state()).unwrap()
        );
        let (task, ledger) = selected(
            store.state(),
            &entry.config.workspace,
            &entry.config.root_task,
        )
        .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert_eq!(reconciliation_exit(&ledger, false), 0);
        assert_eq!(
            reconciliation_exit(&ledger, true),
            7,
            "zero quoted liability is still unknown until final provider accounting"
        );
        for (active, unresolved, overrun) in [(1, 0, false), (0, 1, false), (0, 0, true)] {
            let mut pending = ledger.clone();
            pending.active = Micros::new(active);
            pending.unresolved = Micros::new(unresolved);
            pending.overrun = overrun;
            assert_eq!(reconciliation_exit(&pending, false), 7);
        }
        let mut changed = store.state().clone();
        let row = changed
            .records
            .values_mut()
            .find(|r| r.collection == Collection::Task)
            .unwrap();
        row.value["state"] = serde_json::json!("running");
        assert!(
            selected(&changed, &entry.config.workspace, &entry.config.root_task)
                .unwrap_err()
                .contains("paused root")
        );
        assert!(selected(store.state(), &WorkspaceId::new(), &entry.config.root_task).is_err());
        let row = changed
            .records
            .values_mut()
            .find(|r| r.collection == Collection::Task)
            .unwrap();
        row.value["state"] = serde_json::json!("paused");
        row.value["parent"] = serde_json::json!(TaskId::new());
        assert!(selected(&changed, &entry.config.workspace, &entry.config.root_task).is_err());
        store.close().await.unwrap();
        let mut called = false;
        assert!(execute_with_source(&entry, &TaskId::new(), &workspace, || {
            called = true;
            Ok(source)
        })
        .await
        .is_err());
        assert!(
            !called,
            "invalid selection must reject before credential or transport lookup"
        );
        let error = execute_with_source(&entry, &entry.config.root_task, &workspace, || {
            Err("fixture credential unavailable".into())
        })
        .await
        .unwrap_err();
        assert_eq!(error, "fixture credential unavailable");
        // A credential/configuration failure still closes the maintenance owner.
        let store = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&before).unwrap(),
            serde_json::to_value(store.state()).unwrap()
        );
        store.close().await.unwrap();
    }
}

#[test]
fn reconciliation_command_needs_no_execution_profile_or_budget() {
    use clap::Parser;
    let temp = tempfile::tempdir().unwrap();
    let task = TaskId::new();
    let cli = crate::args::Cli::try_parse_from([
        "vcp",
        "--workspace",
        temp.path().to_str().unwrap(),
        "tasks",
        "reconcile-cost",
        task.as_str(),
    ])
    .unwrap()
    .validate(None)
    .unwrap();
    assert!(
        matches!(cli.command,crate::args::ValidatedCommand::Tasks(crate::args::Tasks::ReconcileCost {task:found}) if found==task)
    );
}
