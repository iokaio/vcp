// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trusted_history_reader_pins_current_and_receipts_across_owner_append() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let config = config(
            &temp.path().join("canonical"),
            &workspace.canonicalize().unwrap(),
            backend,
        );
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let reader = host.history_reader().unwrap();
        let before = host.snapshot().unwrap();
        assert_eq!(reader.current().watermark, before.watermark);
        let receipt = host
            .command(
                Command::CreateTask {
                    root: config.root_task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "Reader remains at its admitted cut".into(),
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
        assert!(host.current_state().unwrap().watermark > reader.current().watermark);
        let mut at = 0u64;
        let mut observed = Vec::new();
        loop {
            let page = reader.page(at.checked_sub(1), 1).unwrap();
            assert_eq!(page.watermark, before.watermark);
            assert_eq!(page.count, before.events.len() as u64);
            at += page.events.len() as u64;
            observed.extend(page.events);
            if at == page.count {
                break;
            }
        }
        assert_eq!(observed, before.events.to_vec());
        for (ordinal, event) in before.events.iter().enumerate() {
            assert_eq!(
                reader.event_at(ordinal as u64).unwrap().as_ref(),
                Some(event)
            );
            assert_eq!(
                reader.event(event.event.id.clone()).unwrap().as_ref(),
                Some(event)
            );
        }
        for command in before.commands.values() {
            assert_eq!(
                reader
                    .command(command.workspace.clone(), command.command.clone())
                    .unwrap()
                    .as_ref(),
                Some(command)
            );
        }
        assert!(reader
            .command(config.workspace.clone(), receipt.command.clone())
            .unwrap()
            .is_none());
        assert!(reader.page(None, 0).is_err());
        assert!(reader.page(None, 4097).is_err());
        assert!(reader
            .event_at(before.events.len() as u64)
            .unwrap()
            .is_none());
        drop(reader);
        owner.close().await.unwrap();
    }
}
