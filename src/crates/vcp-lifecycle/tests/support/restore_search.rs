// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::restore_search;
use vcp_protocol::command::Command;
use vcp_store::Store;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restored_lexical_rebuild_preserves_untrusted_paused_authority_and_retries_without_writes()
{
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (_temp, mut config, host, owner, test, thread) =
            memory_publication::vector_fixture(backend).await;
        memory_publication::retained_source(&host, &config, thread);
        let current: Workspace = host
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
            Command::Rebind {
                binding: config.binding.clone(),
            },
            None,
            current.revision,
        )
        .unwrap();
        let before = host.snapshot().unwrap();
        let workspace: Workspace = before
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.trust, Trust::Untrusted);
        config.binding = workspace.binding.clone();
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        assert!(
            restore_search::rebuild_after_restore(&config, Arc::new(AtomicBool::new(true)))
                .await
                .is_err()
        );
        let shared_cancel = Arc::new(AtomicBool::new(false));
        let result = restore_search::rebuild_after_restore(&config, shared_cancel.clone())
            .await
            .unwrap();
        assert!(!shared_cancel.load(std::sync::atomic::Ordering::Acquire));
        assert!(result.lexical_ready && result.rebuilt);
        assert!(
            result.indexed_records > 0,
            "retained preference must remain lexically searchable"
        );
        assert!(result.source_reauthorization_required);
        assert!(result
            .degraded
            .iter()
            .any(|d| d == "workspace_remains_untrusted"));
        let store = Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        let after: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(after, workspace);
        let access = vcp_memory::access::Access {
            workspace: config.workspace.clone(),
            actor: config.actor.clone(),
            authority: after.authority,
            read: true,
            write: false,
            tasks: None,
        };
        let publisher = vcp_memory::publication::Publisher::open_existing(
            &store.canonical_anchor().join("search-generations"),
        )
        .unwrap();
        let view = publisher.recover(&store, &access).unwrap().view.unwrap();
        let matches = view
            .lexical
            .search(&vcp_memory::lexical::Query {
                workspace: config.workspace.clone(),
                tasks: None,
                roots: None,
                paths: None,
                symbols: None,
                kind: None,
                claim_kind: None,
                status: None,
                text: "concise".into(),
                phrase: false,
                limit: 10,
            })
            .unwrap();
        assert!(
            !matches.is_empty(),
            "rebuilt lexical component must answer retained preference query"
        );
        assert!(view
            .inventory
            .records
            .iter()
            .all(|row| row.kind != vcp_memory::search_record::SearchKind::Source));
        drop(view);
        drop(publisher);

        let task: Task = store
            .state()
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert!(store
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::LocalResources
                && r.value["source"]
                    .as_str()
                    .is_some_and(|s| s.contains("restore-lexical"))));
        let watermark = store.state().watermark;
        store.close().await.unwrap();
        let repeated = restore_search::rebuild_after_restore(&config, shared_cancel.clone())
            .await
            .unwrap();
        assert!(!shared_cancel.load(std::sync::atomic::Ordering::Acquire));
        assert!(repeated.lexical_ready && !repeated.rebuilt);
        assert_eq!(repeated.generation, result.generation);
        let store = Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();
    }
}
