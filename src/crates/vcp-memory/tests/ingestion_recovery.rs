// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{
    ingestion::JobState,
    memory::Outcome,
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::*,
    *,
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{access::Access, ingest, preferences, repository, runner};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::{contract::State, BackendKind, Store};

async fn issue(engine: &mut Engine<Store>, scope: &Scope, payload: Command) {
    let access = vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: if matches!(&payload, Command::Initialize { .. }) {
            None
        } else {
            Some(scope.task.clone())
        },
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(
            command,
            &access,
            &HostFacts {
                may_execute: true,
                ..HostFacts::inspect(Timestamp::new(100))
            },
        )
        .await
        .unwrap();
}

async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (Engine<Store>, Scope, Access, EventId) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let scope = Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    };
    issue(
        &mut engine,
        &scope,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/runner-fixture".into(),
                repository: "repo".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    issue(
        &mut engine,
        &scope,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"output","value":"concise"}}"#.into(),
                constraints: vec![],
                acceptance: vec!["retain".into()],
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
    )
    .await;
    let origin = engine
        .store()
        .state()
        .events
        .iter()
        .find(|e| e.event.kind == EventKind::TaskCreated)
        .unwrap()
        .event
        .id
        .clone();
    issue(
        &mut engine,
        &scope,
        Command::Transition {
            next: TaskState::Running,
            reason: "fixture admission".into(),
            verification: None,
        },
    )
    .await;
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    (engine, scope, access, origin)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Barrier {
    state: State,
    scope: Scope,
    origin: EventId,
}

fn backend(name: &str) -> BackendKind {
    match name {
        "files" => BackendKind::Files,
        "sqlite" => BackendKind::Sqlite,
        _ => panic!("unknown fixture backend"),
    }
}

#[tokio::test]
async fn ingestion_child() {
    let Some(directory) = std::env::var_os("VCP_INGESTION_CRASH_DIRECTORY") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let phase = std::env::var("VCP_INGESTION_CRASH_PHASE").unwrap();
    let backend = backend(&std::env::var("VCP_INGESTION_CRASH_BACKEND").unwrap());
    let (engine, scope, access, origin) = fixture(&directory.join("store"), backend).await;
    let mut store = engine.into_store();
    let through = store.state().watermark;
    ingest::enqueue(
        &mut store,
        &access,
        &scope,
        &runner::specification(),
        through,
        runner::limits(),
    )
    .await
    .unwrap();
    let job = ingest::inspect(&store, &access)
        .unwrap()
        .into_iter()
        .find(|job| job.origin == origin)
        .unwrap();
    if phase != "queued" {
        let leased = ingest::lease(
            &mut store,
            &access,
            &job.id,
            job.revision,
            Timestamp::new(100),
            runner::limits(),
        )
        .await
        .unwrap();
        let event = store
            .state()
            .events
            .iter()
            .find(|event| event.event.id == origin)
            .unwrap()
            .clone();
        let proposal = preferences::materialize(&mut store, &access, &event)
            .await
            .unwrap()
            .unwrap();
        let commit = repository::propose(&mut store, &access, proposal, Timestamp::new(101))
            .await
            .unwrap();
        assert_eq!(commit.result.resolution.outcome, Outcome::Accepted);
        if phase == "completed" {
            ingest::complete(
                &mut store,
                &access,
                &job.id,
                &leased.lease.unwrap().token,
                Timestamp::new(102),
                vec![commit.result.id],
                Some("durable completion barrier".into()),
            )
            .await
            .unwrap();
        } else {
            assert_eq!(phase, "proposal");
        }
    }
    let marker = Barrier {
        state: store.state().clone(),
        scope,
        origin,
    };
    use std::io::Write;
    let mut file = std::fs::File::create(directory.join("barrier.pending")).unwrap();
    file.write_all(&serde_json::to_vec(&marker).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    std::fs::rename(
        directory.join("barrier.pending"),
        directory.join("barrier.json"),
    )
    .unwrap();
    // Parent kills this process while the store remains open: no destructor,
    // normal owner shutdown or in-process adapter reopen substitutes for death.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::hint::black_box(&store);
    }
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn killed_process_recovers_cursor_proposal_and_completion_barriers_on_both_stores() {
    for name in ["files", "sqlite"] {
        for phase in ["queued", "proposal", "completed"] {
            let directory = tempfile::tempdir().unwrap();
            let log = std::fs::File::create(directory.path().join("child.log")).unwrap();
            let mut child = ChildGuard(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "ingestion_child", "--nocapture"])
                    .env("VCP_INGESTION_CRASH_DIRECTORY", directory.path())
                    .env("VCP_INGESTION_CRASH_BACKEND", name)
                    .env("VCP_INGESTION_CRASH_PHASE", phase)
                    .stdout(log.try_clone().unwrap())
                    .stderr(log)
                    .spawn()
                    .unwrap(),
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
            while !directory.path().join("barrier.json").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before barrier: {}",
                    std::fs::read_to_string(directory.path().join("child.log")).unwrap()
                );
                assert!(
                    std::time::Instant::now() < deadline,
                    "child failed to reach {name}/{phase} barrier"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            let barrier: Barrier = serde_json::from_slice(
                &std::fs::read(directory.path().join("barrier.json")).unwrap(),
            )
            .unwrap();
            child.0.kill().unwrap();
            assert!(
                !child.0.wait().unwrap().success(),
                "fixture must terminate abnormally"
            );
            let access = Access {
                workspace: barrier.scope.workspace.clone(),
                actor: ActorId::parse("owner").unwrap(),
                authority: AuthorityRevision::ZERO,
                read: true,
                write: true,
                tasks: None,
            };
            let mut store = Store::open(&directory.path().join("store"), backend(name), &[])
                .await
                .unwrap();
            assert_eq!(store.state(),&barrier.state,"kill/reopen must preserve every committed record, cursor, origin event and watermark at {name}/{phase}");
            let initial_jobs = ingest::inspect(&store, &access).unwrap();
            let original = initial_jobs
                .iter()
                .find(|job| job.origin == barrier.origin)
                .unwrap()
                .clone();
            assert_eq!(
                original.state,
                match phase {
                    "queued" => JobState::Pending,
                    "proposal" => JobState::Leased,
                    _ => JobState::Completed,
                }
            );
            for tick in 0..12 {
                let progress = runner::step(
                    &mut store,
                    &access,
                    &barrier.scope,
                    Timestamp::new(31_000 + tick),
                )
                .await
                .unwrap();
                assert!(progress.completed + progress.deferred + progress.failed <= 2);
                if progress.caught_up
                    && ingest::inspect(&store, &access)
                        .unwrap()
                        .iter()
                        .all(|job| job.state.finished())
                {
                    break;
                }
            }
            let jobs = ingest::inspect(&store, &access).unwrap();
            assert!(jobs.iter().all(|job| job.state == JobState::Completed));
            let selected = runner::specification().event_kinds;
            for event in &store.state().events {
                let kind = serde_json::to_value(&event.event.kind).unwrap();
                if kind
                    .as_str()
                    .is_some_and(|kind| selected.iter().any(|name| name == kind))
                {
                    assert_eq!(
                        jobs.iter()
                            .filter(|job| job.origin == event.event.id)
                            .count(),
                        1,
                        "every selected raw origin must have exactly one completed job"
                    );
                }
            }
            for old in initial_jobs {
                assert_eq!(
                    jobs.iter()
                        .filter(|job| job.id == old.id
                            && job.origin == old.origin
                            && job.cursor == old.cursor)
                        .count(),
                    1
                );
            }
            let recovered = jobs.iter().find(|job| job.id == original.id).unwrap();
            assert_eq!(
                recovered.attempts,
                Units::new(if phase == "proposal" { 2 } else { 1 })
            );
            if phase == "completed" {
                assert_eq!(
                    recovered, &original,
                    "completed jobs must not be leased or rewritten"
                );
            }
            assert_eq!(
                &store.state().events[..barrier.state.events.len()],
                &barrier.state.events
            );
            for tag in [
                "vcp_memory_proposal_v1",
                "vcp_memory_version_v1",
                "vcp_memory_result_v1",
                "vcp_memory_index_intent_v1",
            ] {
                let rows: Vec<_> = store
                    .state()
                    .records
                    .values()
                    .filter(|row| row.value["document_type"] == tag)
                    .collect();
                assert_eq!(rows.len(), 1, "no duplicate {tag} after {name}/{phase}");
                for previous in barrier
                    .state
                    .records
                    .values()
                    .filter(|row| row.value["document_type"] == tag)
                {
                    assert_eq!(
                        rows[0], previous,
                        "persisted memory identity/result must survive retries"
                    );
                }
            }
            let proposal: vcp_domain::memory::ProposalRecord = store
                .state()
                .records
                .values()
                .find(|row| row.value["document_type"] == "vcp_memory_proposal_v1")
                .unwrap()
                .decode()
                .unwrap();
            let before = store.state().clone();
            let replay = repository::propose(
                &mut store,
                &access,
                proposal.proposal.clone(),
                Timestamp::new(32_000),
            )
            .await
            .unwrap();
            assert_eq!(recovered.results, vec![replay.result.id.clone()]);
            assert_eq!(
                store.state(),
                &before,
                "same command retry must not allocate versions, sequences or index intents"
            );
            let replay_again = repository::propose(
                &mut store,
                &access,
                proposal.proposal,
                Timestamp::new(33_000),
            )
            .await
            .unwrap();
            assert_eq!(replay.receipt, replay_again.receipt);
            assert_eq!(replay.result, replay_again.result);
            runner::step(&mut store, &access, &barrier.scope, Timestamp::new(34_000))
                .await
                .unwrap();
            assert_eq!(
                store.state(),
                &before,
                "drained maintenance must not advance cursor repeatedly"
            );
        }
    }
}
