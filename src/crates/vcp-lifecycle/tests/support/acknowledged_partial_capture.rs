// SPDX-License-Identifier: Apache-2.0
//! An acknowledged deliberate process stop is retained history, not owner loss.
use super::*;
use codex_protocol::ThreadId;
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Mutation, Record, Transaction},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_acknowledged_partial_output_allows_new_owner_admission() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let cfg = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(cfg.clone()).unwrap();
        let binding = task(&host, &cfg, cfg.root_task.clone(), None);
        let thread = ThreadId::new();
        host.register(thread, binding.clone()).unwrap();
        let mut partials = Vec::new();
        for channel in [Channel::Stdout, Channel::Stderr, Channel::ChildTranscript] {
            let output = host.open_output(thread, channel).unwrap();
            output.write(b"acknowledged partial\0\xff").unwrap();
            let descriptor = output.finish_partial().unwrap();
            assert_eq!(descriptor.state, CaptureState::Aborted);
            assert!(descriptor.spec.omissions.contains(&Omission::ExplicitAbort));
            partials.push(descriptor);
        }
        let before = host.snapshot().unwrap();
        owner.close().await.unwrap();
        drop(host);

        let mut next = cfg.clone();
        next.root_task = TaskId::new();
        let (host, owner) = CanonicalHost::open(next.clone()).unwrap();
        let binding = task(&host, &next, next.root_task.clone(), None);
        let thread = ThreadId::new();
        host.register(thread, binding).unwrap();
        let output = host.open_output(thread, Channel::Stdout).unwrap();
        output.write(b"new owner admitted").unwrap();
        assert_eq!(output.finish().unwrap().state, CaptureState::Complete);
        let after = host.snapshot().unwrap();
        for descriptor in partials {
            let retained: ArtifactDescriptor = after
                .record(
                    Collection::Artifact,
                    descriptor.spec.id.as_str(),
                    &cfg.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(retained, descriptor);
            assert_eq!(
                host.read_artifact(descriptor.spec.id).unwrap(),
                b"acknowledged partial\0\xff"
            );
        }
        for (id, receipt) in &before.transactions {
            assert_eq!(after.transactions.get(id), Some(receipt));
        }
        assert!(after.events.starts_with(&before.events));
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unacknowledged_pending_mismatched_failed_and_provider_captures_stay_fenced() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for case in [
            "unacknowledged",
            "pending",
            "mismatched",
            "capture_failure",
            "provider",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let cfg = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(cfg.clone()).unwrap();
            let binding = task(&host, &cfg, cfg.root_task.clone(), None);
            owner.close().await.unwrap();
            drop(host);
            let mut store = vcp_store::Store::open(&cfg.canonical_root, backend, &[])
                .await
                .unwrap();
            let spec = ArtifactSpec {
                id: ArtifactId::new(),
                scope: binding.scope.clone(),
                media_type: "application/octet-stream".into(),
                schema: if case == "provider" {
                    "retained-model-response/1"
                } else {
                    "retained-full-output/1"
                }
                .into(),
                source: "startup recovery guard fixture".into(),
                channel: if case == "provider" {
                    Channel::Response
                } else {
                    Channel::Stdout
                },
                retention: "full-work-history".into(),
                omissions: if case == "capture_failure" {
                    vec![Omission::CaptureFailure]
                } else {
                    vec![]
                },
            };
            let id = spec.id.clone();
            let mut writer = store.spool().create(spec).unwrap();
            writer.write_chunk(b"unresolved retained prefix").unwrap();
            let mut descriptor = if case == "pending" {
                store.spool().inspect(&id).unwrap()
            } else {
                writer.abort().unwrap()
            };
            drop(writer);
            if case != "unacknowledged" {
                if case == "mismatched" {
                    descriptor.sha256 = "a".repeat(64);
                }
                let record = Record::typed(
                    Collection::Artifact,
                    id.to_string(),
                    cfg.workspace.clone(),
                    Revision::ZERO,
                    &descriptor,
                )
                .unwrap();
                let acknowledged = store
                    .transact(Transaction {
                        id: TransactionId::new(),
                        expected_watermark: store.state().watermark,
                        mutations: vec![Mutation::Put {
                            expected: None,
                            record,
                        }],
                        events: vec![],
                        command: None,
                    })
                    .await;
                if case == "mismatched" {
                    // The store rejects the false acknowledgement before it
                    // can become canonical; the physical orphan stays fenced.
                    assert!(matches!(acknowledged, Err(vcp_store::Error::Corruption(_))));
                } else {
                    acknowledged.unwrap();
                }
            }
            let physical = store.spool().inspect(&id).unwrap();
            let before = store.state().clone();
            store.close().await.unwrap();
            let (host, owner) = CanonicalHost::open(cfg.clone()).unwrap();
            // Inspection can publish the startup fence, but cannot reconcile it.
            let after = host.snapshot().unwrap();
            assert!(
                host.register(ThreadId::new(), binding).is_err(),
                "{backend:?}/{case}"
            );
            for (id, receipt) in &before.transactions {
                assert_eq!(after.transactions.get(id), Some(receipt));
            }
            owner.close().await.unwrap();
            drop(host);
            let store = vcp_store::Store::open(&cfg.canonical_root, backend, &[])
                .await
                .unwrap();
            assert_eq!(store.spool().inspect(&id).unwrap(), physical);
            store.close().await.unwrap();
        }
    }
}
