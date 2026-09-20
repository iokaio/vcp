// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::retention_selector::{Criterion, Selector, Tree};
use vcp_lifecycle::foundation::history_retention::Request;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_owner_browses_previews_and_applies_exact_selection_without_starting_work() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        task(&host, &config, config.root_task.clone(), None);
        let child = TaskId::new();
        task(
            &host,
            &config,
            child.clone(),
            Some(config.root_task.clone()),
        );
        for id in [&config.root_task, &child] {
            host.command(
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "explicit fixture pause".into(),
                    verification: None,
                },
                Some(id.clone()),
                Revision::new(1),
            )
            .unwrap();
        }
        let before = host.snapshot().unwrap();
        let selector = Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Workspace(config.workspace.clone())),
        };
        let query = vcp_audit::history_query::Query {
            selector: selector.clone(),
            text: None,
            limit: 128,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        let page = host.history_retention(Request::History { query }).unwrap();
        assert!(!page["rows"].as_array().unwrap().is_empty());
        host.history_retention(Request::Policy).unwrap();
        host.history_retention(Request::Notice).unwrap();
        assert_eq!(host.snapshot().unwrap().watermark, before.watermark);
        let purge = host
            .history_retention(Request::Preview {
                selector: selector.clone(),
                action: vcp_memory::retention::Action::Purge,
            })
            .unwrap();
        assert!(purge["protected_count"].as_u64().unwrap() > 0);
        let protected_watermark = host.snapshot().unwrap().watermark;
        assert!(host
            .history_retention(Request::Apply {
                preview: purge["id"].as_str().unwrap().to_owned()
            })
            .is_err());
        assert_eq!(host.snapshot().unwrap().watermark, protected_watermark);
        let preview = host
            .history_retention(Request::Preview {
                selector,
                action: vcp_memory::retention::Action::Compact,
            })
            .unwrap();
        let id = preview["id"].as_str().unwrap().to_owned();
        let receipt = host
            .history_retention(Request::Apply {
                preview: id.clone(),
            })
            .unwrap();
        assert_eq!(receipt["preview"]["id"], id);
        assert_eq!(receipt["preview"]["selected"], preview["selected"]);
        let after = host.snapshot().unwrap();
        for id in [&config.root_task, &child] {
            let t: Task = after
                .record(Collection::Task, id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(t.state, TaskState::Paused);
        }
        assert_eq!(
            before
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .count(),
            after
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .count()
        );
        owner.close().await.unwrap();
    }
}
