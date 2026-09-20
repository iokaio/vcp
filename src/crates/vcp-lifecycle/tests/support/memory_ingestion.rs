// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_audit::inspection::{InspectionQuery, View};
use vcp_domain::ingestion::{Job, JobState};
use vcp_store::contract::State;

fn jobs(state: &State) -> Vec<Job> {
    state
        .records
        .values()
        .filter(|row| {
            row.collection == Collection::Claim
                && row.value["document_type"] == "vcp_ingestion_job_v1"
        })
        .map(|row| row.decode().unwrap())
        .collect()
}
fn root(host: &CanonicalHost, config: &Config) -> Task {
    host.snapshot()
        .unwrap()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}
fn transition(host: &CanonicalHost, config: &Config, state: TaskState) {
    host.command(
        Command::Transition {
            next: state,
            reason: "explicit fixture lifecycle boundary".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        root(host, config).revision,
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_memory_steps_are_bounded_pause_aware_and_never_started_by_inspection() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        task(&host, &config, config.root_task.clone(), None);
        let started = host.snapshot().unwrap();
        let initial = jobs(&started);
        assert!(
            !initial.is_empty(),
            "admitted commands must create durable ingestion work"
        );
        assert!(
            initial.iter().any(|job| job.state == JobState::Completed),
            "admitted commands must make bounded automatic progress"
        );
        assert!(
            initial.len() <= 2,
            "one automatic step processes at most two origin events here"
        );

        transition(&host, &config, TaskState::Paused);
        let paused_jobs = jobs(&host.snapshot().unwrap());
        for index in 0..9 {
            let current = root(&host, &config);
            host.command(
                Command::ObserveFingerprint {
                    fingerprint: Fingerprint {
                        repository: format!("{index:064x}"),
                        ..current.fingerprint
                    },
                },
                Some(config.root_task.clone()),
                current.revision,
            )
            .unwrap();
        }
        let paused = host.snapshot().unwrap();
        assert_eq!(
            jobs(&paused),
            paused_jobs,
            "paused commands retain raw observations without scheduling extraction"
        );
        let progress = host.maintain_memory().unwrap();
        assert_eq!(progress.completed, 0);
        assert_eq!(progress.deferred, 0);
        for _ in 0..3 {
            host.inspect(InspectionQuery {
                id: config.root_task.to_string(),
                view: View::Memory,
                limit: 16,
                cursor: None,
                range: None,
            })
            .unwrap();
            assert_eq!(
                host.snapshot().unwrap(),
                paused,
                "inspection/snapshot and paused maintenance cannot mutate canonical state"
            );
        }

        // A terminal failure still leaves observed work to summarize locally.
        // No retained model thread or external process exists in this fixture.
        transition(&host, &config, TaskState::Failed);
        let before = jobs(&host.snapshot().unwrap());
        assert!(
            before.iter().any(|job| !job.state.finished()),
            "one automatic step must leave a visible bounded backlog"
        );
        let mut progressed = false;
        for _ in 0..16 {
            let before = host.snapshot().unwrap();
            let before_done = jobs(&before)
                .iter()
                .filter(|job| job.state == JobState::Completed)
                .count();
            let progress = host.maintain_memory().unwrap();
            assert!(progress.completed + progress.deferred <= 2);
            progressed |= progress.completed > 0;
            let after = host.snapshot().unwrap();
            let current = jobs(&after);
            let after_done = current
                .iter()
                .filter(|job| job.state == JobState::Completed)
                .count();
            assert_eq!(after_done - before_done, progress.completed);
            if progress.caught_up && current.iter().all(|job| job.state.finished()) {
                break;
            }
        }
        assert!(progressed);
        let drained = host.snapshot().unwrap();
        let completed = jobs(&drained);
        assert!(completed.iter().all(|job| job.state == JobState::Completed));
        let external: Vec<_> = drained
            .events
            .iter()
            .filter(|event| event.event.kind == vcp_protocol::event::EventKind::FingerprintObserved)
            .collect();
        assert_eq!(external.len(), 9);
        for event in external {
            let observed: Vec<_> = completed
                .iter()
                .filter(|job| job.origin == event.event.id)
                .collect();
            assert_eq!(
                observed.len(),
                1,
                "each observed source revision has one durable outcome"
            );
            assert!(
                observed[0]
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.contains("unknown")),
                "unobserved edit authorship must remain explicitly unknown"
            );
        }
        assert!(!drained
            .events
            .iter()
            .any(|event| event.event.data["component"] == "memory_ingestion"
                && event.event.data["status"] == "maintenance_failed"));
        host.maintain_memory().unwrap();
        assert_eq!(
            host.snapshot().unwrap(),
            drained,
            "idle maintenance is read-only after draining"
        );
        owner.close().await.unwrap();
        drop(host);
        let (reopened, reopened_owner) = CanonicalHost::open(config.clone()).unwrap();
        assert_eq!(
            jobs(&reopened.snapshot().unwrap()),
            completed,
            "reopen preserves queue origin/result identities"
        );
        reopened_owner.close().await.unwrap();
        drop(reopened);
    }
}
