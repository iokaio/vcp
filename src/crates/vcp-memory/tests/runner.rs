// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{
    artifact::*,
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
use vcp_store::{artifact::ArtifactWriter, contract::Collection, BackendKind, Store};

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

#[tokio::test]
async fn caught_up_includes_activity_created_during_processing() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, _) = fixture(temp.path(), backend).await;
        let mut store = engine.into_store();
        let progress = runner::step(&mut store, &access, &scope, Timestamp::new(100))
            .await
            .unwrap();
        assert_eq!(progress.completed, 2);
        assert!(
            !progress.caught_up,
            "preference capture creates more activity"
        );
        let progress = runner::step(&mut store, &access, &scope, Timestamp::new(101))
            .await
            .unwrap();
        assert!(progress.caught_up);
        assert!(ingest::inspect(&store, &access)
            .unwrap()
            .iter()
            .all(|job| job.state.finished()));
        let before = store.state().clone();
        let progress = runner::step(&mut store, &access, &scope, Timestamp::new(102))
            .await
            .unwrap();
        assert!(progress.caught_up);
        assert_eq!(store.state(), &before);
    }
}

#[tokio::test]
async fn committed_preference_recovers_lost_completion_without_duplicate_memory() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, scope, access, origin) = fixture(temp.path(), backend).await;
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
            .find(|j| j.origin == origin)
            .unwrap();
        ingest::lease(
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
            .find(|e| e.event.id == origin)
            .unwrap()
            .clone();
        let proposal = preferences::materialize(&mut store, &access, &event)
            .await
            .unwrap()
            .unwrap();
        let committed =
            repository::propose(&mut store, &access, proposal.clone(), Timestamp::new(101))
                .await
                .unwrap();
        assert_eq!(committed.result.resolution.outcome, Outcome::Accepted);
        // Crash after durable promotion but before completing the job lease.
        drop(store);
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let progress = runner::step(&mut store, &access, &scope, Timestamp::new(31_000))
            .await
            .unwrap();
        assert!(progress.completed > 0 && progress.completed <= 2);
        let recovered = ingest::inspect(&store, &access)
            .unwrap()
            .into_iter()
            .find(|j| j.id == job.id)
            .unwrap();
        assert_eq!(recovered.state, JobState::Completed);
        assert_eq!(recovered.attempts, Units::new(2));
        assert_eq!(recovered.results, vec![committed.result.id.clone()]);
        let replay = repository::propose(&mut store, &access, proposal, Timestamp::new(31_001))
            .await
            .unwrap();
        assert_eq!(replay.receipt, committed.receipt);
        assert_eq!(replay.result, committed.result);
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|row| row.collection == Collection::Claim
                    && row.value["document_type"] == "vcp_memory_version_v1")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn malformed_observation_jobs_yield_and_idle_steps_stop_mutating() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, access, _) = fixture(temp.path(), BackendKind::Files).await;
    let mut poison = Vec::new();
    for _ in 0..5 {
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: scope.clone(),
                media_type: "application/json".into(),
                schema: "memory-observations/1".into(),
                source: "malformed fixture".into(),
                channel: Channel::Evidence,
                retention: "workspace".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(b"{invalid observation").unwrap();
        let descriptor = writer.finalize().unwrap();
        issue(&mut engine, &scope, Command::AttachArtifact { descriptor }).await;
        poison.push(
            engine
                .store()
                .state()
                .events
                .last()
                .unwrap()
                .event
                .id
                .clone(),
        );
    }
    let mut store = engine.into_store();
    let first = runner::step(&mut store, &access, &scope, Timestamp::new(200))
        .await
        .unwrap();
    assert_eq!(first.completed, 2);
    assert_eq!(first.deferred, 0);
    assert!(ingest::inspect(&store, &access)
        .unwrap()
        .iter()
        .any(|j| !j.state.finished()));
    for _ in 0..12 {
        let progress = runner::step(&mut store, &access, &scope, Timestamp::new(200))
            .await
            .unwrap();
        assert!(progress.completed + progress.deferred + progress.failed <= 2);
    }
    let jobs = ingest::inspect(&store, &access).unwrap();
    for origin in poison {
        let job = jobs.iter().find(|j| j.origin == origin).unwrap();
        assert_eq!(job.state, JobState::Completed);
        assert!(job.results.is_empty());
        assert!(job
            .finding
            .as_deref()
            .unwrap()
            .contains("invalid_observation"));
    }
    let before = store.state().clone();
    let idle = runner::step(&mut store, &access, &scope, Timestamp::new(200))
        .await
        .unwrap();
    assert!(idle.caught_up);
    assert_eq!(idle.completed + idle.deferred + idle.failed, 0);
    assert_eq!(store.state(), &before);
}

#[tokio::test]
async fn held_child_does_not_starve_later_root_work() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, access, _) = fixture(temp.path(), BackendKind::Files).await;
    for _ in 0..8 {
        runner::step(engine.store_mut(), &access, &scope, Timestamp::new(200))
            .await
            .unwrap();
    }
    let child = Scope {
        task: TaskId::new(),
        ..scope.clone()
    };
    issue(
        &mut engine,
        &child,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: Some(scope.task.clone()),
            fork_origin: None,
            objective: Objective {
                text: "held child".into(),
                constraints: vec![],
                acceptance: vec!["stay held".into()],
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
    issue(
        &mut engine,
        &child,
        Command::Transition {
            next: TaskState::Paused,
            reason: "hold child".into(),
            verification: None,
        },
    )
    .await;
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "text/plain".into(),
            schema: "runner-observation/1".into(),
            source: "later root".into(),
            channel: Channel::Evidence,
            retention: "workspace".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"later root observation").unwrap();
    issue(
        &mut engine,
        &scope,
        Command::AttachArtifact {
            descriptor: writer.finalize().unwrap(),
        },
    )
    .await;
    let later = engine
        .store()
        .state()
        .events
        .last()
        .unwrap()
        .event
        .id
        .clone();
    let progress = runner::step(engine.store_mut(), &access, &scope, Timestamp::new(200))
        .await
        .unwrap();
    assert_eq!(progress.completed, 1);
    let jobs = ingest::inspect(engine.store(), &access).unwrap();
    assert_eq!(
        jobs.iter().find(|j| j.origin == later).unwrap().state,
        JobState::Completed
    );
    let children: Vec<_> = jobs.iter().filter(|j| j.scope.task == child.task).collect();
    assert!(!children.is_empty());
    assert!(children
        .iter()
        .all(|j| j.state == JobState::Pending && j.attempts == Units::ZERO));
}

#[tokio::test]
async fn exhausted_leases_count_toward_the_per_step_job_limit() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, access, _) = fixture(temp.path(), BackendKind::Files).await;
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "text/plain".into(),
            schema: "runner-observation/1".into(),
            source: "third job".into(),
            channel: Channel::Evidence,
            retention: "workspace".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"third job").unwrap();
    issue(
        &mut engine,
        &scope,
        Command::AttachArtifact {
            descriptor: writer.finalize().unwrap(),
        },
    )
    .await;
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
    let jobs = ingest::inspect(&store, &access).unwrap();
    assert_eq!(jobs.len(), 3);
    let mut now = 100;
    for job in jobs {
        let mut current = job;
        for _ in 0..3 {
            current = ingest::lease(
                &mut store,
                &access,
                &current.id,
                current.revision,
                Timestamp::new(now),
                runner::limits(),
            )
            .await
            .unwrap();
            now += 30_001;
        }
    }
    let progress = runner::step(&mut store, &access, &scope, Timestamp::new(now))
        .await
        .unwrap();
    assert_eq!(progress.failed, 2);
    assert_eq!(progress.completed + progress.deferred, 0);
    let jobs = ingest::inspect(&store, &access).unwrap();
    assert_eq!(
        jobs.iter().filter(|j| j.state == JobState::Failed).count(),
        2
    );
    assert_eq!(
        jobs.iter().filter(|j| j.state == JobState::Leased).count(),
        1
    );
    let final_step = runner::step(&mut store, &access, &scope, Timestamp::new(now))
        .await
        .unwrap();
    assert_eq!(final_step.failed, 1);
    let before = store.state().clone();
    runner::step(&mut store, &access, &scope, Timestamp::new(now))
        .await
        .unwrap();
    assert_eq!(store.state(), &before);
}
