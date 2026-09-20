// SPDX-License-Identifier: Apache-2.0
//! Persistent bounded work queues driven by the existing host, never a daemon.
use crate::{
    access::{self, Access},
    Error, Result,
};
use vcp_domain::{
    ingestion::*,
    memory::{DocumentType as MemoryDocument, ProposalResult},
    task::{Task, TaskState},
    workspace::Scope,
    *,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{key, CanonicalStore, Collection, Mutation, Record, State, Transaction},
    Store,
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub scanned_events: usize,
    pub new_jobs: usize,
    pub batch_bytes: usize,
    pub pending_jobs: usize,
    pub concurrency: usize,
    pub attempts: u64,
    pub lease_ms: u64,
}
impl Limits {
    fn validate(self) -> Result<()> {
        if !(1..=1024).contains(&self.scanned_events)
            || !(1..=256).contains(&self.new_jobs)
            || !(1024..=2 * 1024 * 1024).contains(&self.batch_bytes)
            || !(1..=10_000).contains(&self.pending_jobs)
            || !(1..=16).contains(&self.concurrency)
            || !(1..=16).contains(&self.attempts)
            || !(1..=60_000).contains(&self.lease_ms)
        {
            return Err(Error::Invalid(
                "ingestion limits outside bounded contract".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Enqueued {
    pub cursor: Cursor,
    pub jobs: Vec<CommandId>,
    /// True means the fixed requested watermark was completely scanned.
    pub caught_up: bool,
    pub quota_reached: bool,
}

pub fn cursor_id(scope: &Scope, extractor: &ExtractorSpec) -> Result<CommandId> {
    Ok(CommandId::parse(digest_bytes(&canonical_bytes(&(
        "ingestion-cursor/1",
        scope,
        extractor,
    ))?))?)
}
fn job_id(cursor: &CommandId, origin: &EventId) -> Result<CommandId> {
    Ok(CommandId::parse(digest_bytes(&canonical_bytes(&(
        "ingestion-job/1",
        cursor,
        origin,
    ))?))?)
}
fn task(state: &State, access: &Access, scope: &Scope) -> Result<Task> {
    if scope.workspace != access.workspace || !access.allows_task(&scope.task) {
        return Err(Error::Access);
    }
    let value: Task = state
        .record(Collection::Task, scope.task.as_str(), &access.workspace)?
        .decode()?;
    if value.scope != *scope {
        return Err(Error::Access);
    }
    Ok(value)
}
fn unfenced(state: &State, access: &Access, scope: &Scope) -> Result<()> {
    let mut current = task(state, access, scope)?;
    let mut seen = std::collections::BTreeSet::new();
    loop {
        if !seen.insert(current.scope.task.clone()) {
            return Err(Error::Invalid("ingestion ancestry cycle".into()));
        }
        if !matches!(
            current.state,
            TaskState::Running | TaskState::Completed | TaskState::Failed
        ) {
            return Err(Error::Conflict("ingestion task or ancestor is held"));
        }
        let Some(parent) = current.parent else {
            return Ok(());
        };
        let scope = Scope {
            task: parent,
            ..current.scope
        };
        current = task(state, access, &scope)?;
    }
}
fn cursor_record(value: &Cursor) -> Result<Record> {
    value.validate()?;
    let mut record = Record::typed(
        Collection::Claim,
        value.id.to_string(),
        value.scope.workspace.clone(),
        value.revision,
        value,
    )?;
    record
        .references
        .insert(key(Collection::Task, value.scope.task.as_str()));
    Ok(record)
}
fn job_record(value: &Job) -> Result<Record> {
    value.validate()?;
    let mut record = Record::typed(
        Collection::Claim,
        value.id.to_string(),
        value.scope.workspace.clone(),
        value.revision,
        value,
    )?;
    record
        .references
        .insert(key(Collection::Task, value.scope.task.as_str()));
    record
        .references
        .insert(key(Collection::Claim, value.cursor.as_str()));
    record.references.extend(
        value
            .results
            .iter()
            .map(|id| key(Collection::Projection, id.as_str())),
    );
    Ok(record)
}
fn jobs(state: &State, workspace: &WorkspaceId) -> Result<Vec<Job>> {
    state
        .records
        .values()
        .filter(|r| {
            r.collection == Collection::Claim
                && &r.workspace == workspace
                && r.value["document_type"] == "vcp_ingestion_job_v1"
        })
        .map(|r| {
            let job: Job = r.decode()?;
            job.validate()?;
            Ok(job)
        })
        .collect()
}
fn transaction(state: &State, mutations: Vec<Mutation>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations,
        events: vec![],
        command: None,
    }
}

/// Atomically preserve pending jobs before advancing the event cursor. Limits
/// stop before an unqueued event; a replay returns its existing durable job.
pub async fn enqueue(
    store: &mut Store,
    access: &Access,
    scope: &Scope,
    extractor: &ExtractorSpec,
    through: Watermark,
    limits: Limits,
) -> Result<Enqueued> {
    limits.validate()?;
    extractor.validate()?;
    access::authorize(store.state(), access, true)?;
    let root = task(store.state(), access, scope)?;
    if root.parent.is_some() || root.root != scope.task || through > store.state().watermark {
        return Err(Error::Invalid("ingestion stream root or watermark".into()));
    }
    let id = cursor_id(scope, extractor)?;
    let previous: Option<Cursor> = store
        .state()
        .records
        .get(&key(Collection::Claim, id.as_str()))
        .map(Record::decode)
        .transpose()?;
    if let Some(previous) = &previous {
        previous.validate()?;
        if previous.scope != *scope || previous.extractor != *extractor {
            return Err(Error::Access);
        }
        if through < previous.scanned_through {
            return Err(Error::Conflict("ingestion watermark regressed"));
        }
    }
    let mut cursor = previous.clone().unwrap_or(Cursor {
        document_type: DocumentType::Cursor,
        schema_version: 1,
        id,
        scope: scope.clone(),
        revision: Revision::ZERO,
        extractor: extractor.clone(),
        after: Units::ZERO,
        scanned_through: Watermark::ZERO,
    });
    let mut pending = jobs(store.state(), &access.workspace)?
        .iter()
        .filter(|j| !j.state.finished())
        .count();
    let mut mutations = Vec::new();
    let mut added = Vec::new();
    let mut bytes = 0;
    let mut quota_reached = false;
    let start = usize::try_from(cursor.after.get())
        .map_err(|_| Error::Invalid("ingestion cursor overflow".into()))?;
    if start > store.state().events.len() {
        return Err(Error::Invalid(
            "ingestion cursor exceeds retained event stream".into(),
        ));
    }
    for (offset, event) in store
        .state()
        .events
        .iter()
        .enumerate()
        .skip(start)
        .take(limits.scanned_events)
    {
        if event.watermark > through {
            break;
        }
        let kind = serde_json::to_value(&event.event.kind)?;
        let selected = event.event.workspace == access.workspace
            && event.event.session == scope.session
            && kind.as_str().is_some_and(|kind| {
                extractor
                    .event_kinds
                    .iter()
                    .any(|candidate| candidate == kind)
            });
        if selected {
            if let Some(origin_task) = &event.event.task {
                let origin: Task = store
                    .state()
                    .record(Collection::Task, origin_task.as_str(), &access.workspace)?
                    .decode()?;
                if origin.root == scope.task {
                    task(store.state(), access, &origin.scope)?;
                    let id = job_id(&cursor.id, &event.event.id)?;
                    if !store
                        .state()
                        .records
                        .contains_key(&key(Collection::Claim, id.as_str()))
                    {
                        let value = Job {
                            document_type: DocumentType::Job,
                            schema_version: 1,
                            id: id.clone(),
                            scope: origin.scope,
                            root: scope.task.clone(),
                            revision: Revision::ZERO,
                            cursor: cursor.id.clone(),
                            extractor: extractor.clone(),
                            origin: event.event.id.clone(),
                            origin_watermark: event.watermark,
                            state: JobState::Pending,
                            attempts: Units::ZERO,
                            max_attempts: Units::new(limits.attempts),
                            lease: None,
                            not_before: Timestamp::ZERO,
                            last_failure: None,
                            results: vec![],
                            finding: None,
                        };
                        let record = job_record(&value)?;
                        let size = canonical_bytes(&record)?.len();
                        if added.len() >= limits.new_jobs
                            || pending >= limits.pending_jobs
                            || bytes + size > limits.batch_bytes
                        {
                            quota_reached = true;
                            break;
                        }
                        bytes += size;
                        pending += 1;
                        added.push(id);
                        mutations.push(Mutation::Put {
                            expected: None,
                            record,
                        });
                    }
                }
            }
        }
        cursor.after = Units::new(offset as u64 + 1);
        cursor.scanned_through = event.watermark;
    }
    let next = usize::try_from(cursor.after.get())
        .map_err(|_| Error::Invalid("ingestion cursor overflow".into()))?;
    let caught_up = store
        .state()
        .events
        .get(next)
        .is_none_or(|event| event.watermark > through);
    // Track examined events, not our own queue-only transaction watermark;
    // otherwise every idle poll would persist another cursor transaction.
    let changed = previous
        .as_ref()
        .is_none_or(|p| p.after != cursor.after || p.scanned_through != cursor.scanned_through);
    if changed {
        cursor.revision = previous
            .as_ref()
            .map(|p| p.revision.next())
            .transpose()?
            .unwrap_or(Revision::ZERO);
        mutations.push(Mutation::Put {
            expected: previous.as_ref().map(|p| p.revision),
            record: cursor_record(&cursor)?,
        });
        let tx = transaction(store.state(), mutations);
        // The same bound also covers the cursor and transaction envelope.
        if canonical_bytes(&tx)?.len() > limits.batch_bytes {
            return Err(Error::Invalid(
                "ingestion transaction exceeds batch byte limit".into(),
            ));
        }
        store.transact(tx).await?;
    }
    Ok(Enqueued {
        cursor,
        jobs: added,
        caught_up,
        quota_reached,
    })
}

pub fn inspect(store: &Store, access: &Access) -> Result<Vec<Job>> {
    access::authorize(store.state(), access, false)?;
    Ok(jobs(store.state(), &access.workspace)?
        .into_iter()
        .filter(|j| access.allows_task(&j.scope.task))
        .collect())
}

async fn update(store: &mut Store, mut job: Job, expected: Revision) -> Result<Job> {
    job.revision = expected.next()?;
    let tx = transaction(
        store.state(),
        vec![Mutation::Put {
            expected: Some(expected),
            record: job_record(&job)?,
        }],
    );
    store.transact(tx).await?;
    Ok(job)
}
fn load(store: &Store, access: &Access, id: &CommandId) -> Result<Job> {
    access::authorize(store.state(), access, true)?;
    let value: Job = store
        .state()
        .record(Collection::Claim, id.as_str(), &access.workspace)?
        .decode()?;
    value.validate()?;
    task(store.state(), access, &value.scope)?;
    Ok(value)
}

/// Caller must already hold the host's local-maintenance admission permit.
/// This persistent lease prevents duplicates; it does not authorize a worker.
pub async fn lease(
    store: &mut Store,
    access: &Access,
    id: &CommandId,
    expected: Revision,
    now: Timestamp,
    limits: Limits,
) -> Result<Job> {
    limits.validate()?;
    let mut job = load(store, access, id)?;
    if job.revision != expected {
        return Err(Error::Conflict("stale ingestion job"));
    }
    unfenced(store.state(), access, &job.scope)?;
    if job.state.finished()
        || job.not_before > now
        || job.lease.as_ref().is_some_and(|l| l.expires_at > now)
    {
        return Err(Error::Conflict("ingestion job is not eligible"));
    }
    if job.attempts >= job.max_attempts {
        job.state = JobState::Failed;
        job.lease = None;
        job.last_failure = Some("bounded extraction attempts exhausted".into());
        return update(store, job, expected).await;
    }
    let active = jobs(store.state(), &access.workspace)?
        .iter()
        .filter(|j| j.lease.as_ref().is_some_and(|l| l.expires_at > now))
        .count();
    if active >= limits.concurrency {
        return Err(Error::Conflict("ingestion concurrency limit"));
    }
    job.attempts = job.attempts.next()?;
    job.state = JobState::Leased;
    job.lease = Some(Lease {
        token: CommandId::new(),
        owner: access.actor.clone(),
        expires_at: Timestamp::new(
            now.get()
                .checked_add(limits.lease_ms)
                .ok_or(Error::Invalid("ingestion lease overflow".into()))?,
        ),
    });
    update(store, job, expected).await
}

fn owns(job: &Job, access: &Access, token: &CommandId, now: Timestamp) -> Result<()> {
    if !job
        .lease
        .as_ref()
        .is_some_and(|l| l.token == *token && l.owner == access.actor && l.expires_at > now)
    {
        return Err(Error::Conflict("ingestion lease expired or replaced"));
    }
    Ok(())
}

pub async fn complete(
    store: &mut Store,
    access: &Access,
    id: &CommandId,
    token: &CommandId,
    now: Timestamp,
    results: Vec<CommandId>,
    finding: Option<String>,
) -> Result<Job> {
    let mut job = load(store, access, id)?;
    if job.state == JobState::Completed {
        return if job.results == results && job.finding == finding {
            Ok(job)
        } else {
            Err(Error::Conflict("ingestion completion changed"))
        };
    }
    owns(&job, access, token, now)?;
    for id in &results {
        let result: ProposalResult = store
            .state()
            .record(Collection::Projection, id.as_str(), &access.workspace)?
            .decode()?;
        result.validate()?;
        if result.document_type != MemoryDocument::Result || result.scope != job.scope {
            return Err(Error::Access);
        }
        let proposal: vcp_domain::memory::ProposalRecord = store
            .state()
            .record(
                Collection::Claim,
                result.proposal.as_str(),
                &access.workspace,
            )?
            .decode()?;
        if !proposal.proposal.origins.contains(&job.origin)
            || proposal.proposal.extractor != job.extractor.identity()
        {
            return Err(Error::Conflict(
                "ingestion result belongs to another origin",
            ));
        }
    }
    let expected = job.revision;
    job.state = JobState::Completed;
    job.lease = None;
    job.results = results;
    job.finding = finding;
    update(store, job, expected).await
}

pub async fn defer(
    store: &mut Store,
    access: &Access,
    id: &CommandId,
    token: &CommandId,
    now: Timestamp,
    not_before: Timestamp,
    reason: String,
) -> Result<Job> {
    let mut job = load(store, access, id)?;
    owns(&job, access, token, now)?;
    if not_before < now {
        return Err(Error::Invalid("ingestion retry time regressed".into()));
    }
    let expected = job.revision;
    job.state = if job.attempts >= job.max_attempts {
        JobState::Failed
    } else {
        JobState::Deferred
    };
    job.lease = None;
    job.not_before = not_before;
    job.last_failure = Some(reason);
    update(store, job, expected).await
}

pub async fn cancel(
    store: &mut Store,
    access: &Access,
    id: &CommandId,
    expected: Revision,
    reason: String,
) -> Result<Job> {
    let mut job = load(store, access, id)?;
    if job.revision != expected {
        return Err(Error::Conflict("stale ingestion cancellation"));
    }
    if job.state.finished() {
        return Ok(job);
    }
    job.state = JobState::Cancelled;
    job.lease = None;
    job.last_failure = Some(reason);
    update(store, job, expected).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{
        task::Objective,
        verification::Fingerprint,
        workspace::{Binding, Session, Trust, Workspace},
    };
    use vcp_protocol::event::{EventInput, EventKind};
    use vcp_store::BackendKind;

    fn limits() -> Limits {
        Limits {
            scanned_events: 16,
            new_jobs: 8,
            batch_bytes: 64 * 1024,
            pending_jobs: 8,
            concurrency: 1,
            attempts: 2,
            lease_ms: 100,
        }
    }
    async fn fixture(backend: BackendKind) -> (tempfile::TempDir, Store, Access, Scope) {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temp.path().join("store"), backend, &[])
            .await
            .unwrap();
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let actor = ActorId::new();
        let event = EventId::new();
        let workspace = Workspace {
            id: scope.workspace.clone(),
            binding: Binding {
                host: HostId::new(),
                root: temp.path().to_string_lossy().into_owned(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
            trust: Trust::Trusted,
            revision: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
        };
        let session = Session {
            id: scope.session.clone(),
            workspace: scope.workspace.clone(),
            revision: Revision::ZERO,
            configuration: Revision::ZERO,
            fork_origin: None,
            fork_through: None,
        };
        let task = Task {
            scope: scope.clone(),
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            objectives: vec![Objective {
                text: "observe".into(),
                constraints: vec![],
                acceptance: vec!["retain evidence".into()],
                source: event.clone(),
                steering: SteeringRevision::ZERO,
            }],
            state: TaskState::Running,
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
            cause: event.clone(),
            reason: "fixture".into(),
        };
        let records = vec![
            Record::typed(
                Collection::Workspace,
                scope.workspace.to_string(),
                scope.workspace.clone(),
                Revision::ZERO,
                &workspace,
            )
            .unwrap(),
            Record::typed(
                Collection::Session,
                scope.session.to_string(),
                scope.workspace.clone(),
                Revision::ZERO,
                &session,
            )
            .unwrap(),
            Record::typed(
                Collection::Task,
                scope.task.to_string(),
                scope.workspace.clone(),
                Revision::ZERO,
                &task,
            )
            .unwrap(),
        ];
        let mut tx = transaction(
            store.state(),
            records
                .into_iter()
                .map(|record| Mutation::Put {
                    expected: None,
                    record,
                })
                .collect(),
        );
        tx.events.push(EventInput {
            id: event,
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(scope.task.clone()),
            actor: actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(1),
            kind: EventKind::TaskCreated,
            artifacts: vec![],
            data: serde_json::json!({}),
            metadata: None,
        });
        store.transact(tx).await.unwrap();
        let access = Access {
            workspace: scope.workspace.clone(),
            actor,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        (temp, store, access, scope)
    }
    fn extractor() -> ExtractorSpec {
        ExtractorSpec {
            name: "fixture".into(),
            version: 1,
            event_kinds: vec!["task_created".into()],
        }
    }
    async fn set_state(store: &mut Store, scope: &Scope, state: TaskState) {
        let mut task: Task = store
            .state()
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let expected = task.revision;
        task.revision = expected.next().unwrap();
        task.state = state;
        let record = Record::typed(
            Collection::Task,
            scope.task.to_string(),
            scope.workspace.clone(),
            task.revision,
            &task,
        )
        .unwrap();
        let tx = transaction(
            store.state(),
            vec![Mutation::Put {
                expected: Some(expected),
                record,
            }],
        );
        store.transact(tx).await.unwrap();
    }

    #[tokio::test]
    async fn queue_reopen_retry_and_pause_preserve_exactly_one_origin_job() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let (temp, mut store, access, scope) = fixture(backend).await;
            let through = store.state().watermark;
            let first = enqueue(&mut store, &access, &scope, &extractor(), through, limits())
                .await
                .unwrap();
            assert!(first.caught_up);
            assert_eq!(first.jobs.len(), 1);
            let id = first.jobs[0].clone();
            let watermark = store.state().watermark;
            let retry = enqueue(
                &mut store,
                &access,
                &scope,
                &extractor(),
                watermark,
                limits(),
            )
            .await
            .unwrap();
            assert!(retry.jobs.is_empty());
            assert_eq!(
                store.state().watermark,
                watermark,
                "idle polls cannot write themselves into backlog"
            );
            store.close().await.unwrap();
            let mut store = Store::open(&temp.path().join("store"), backend, &[])
                .await
                .unwrap();
            assert_eq!(inspect(&store, &access).unwrap().len(), 1);
            set_state(&mut store, &scope, TaskState::Paused).await;
            assert!(lease(
                &mut store,
                &access,
                &id,
                Revision::ZERO,
                Timestamp::new(2),
                limits()
            )
            .await
            .is_err());
            set_state(&mut store, &scope, TaskState::Running).await;
            let leased = lease(
                &mut store,
                &access,
                &id,
                Revision::ZERO,
                Timestamp::new(2),
                limits(),
            )
            .await
            .unwrap();
            let token = leased.lease.unwrap().token;
            set_state(&mut store, &scope, TaskState::Paused).await;
            let done = complete(
                &mut store,
                &access,
                &id,
                &token,
                Timestamp::new(3),
                vec![],
                Some("no supported claim in this origin".into()),
            )
            .await
            .unwrap();
            assert_eq!(done.state, JobState::Completed);
            let watermark = store.state().watermark;
            let replay = complete(
                &mut store,
                &access,
                &id,
                &token,
                Timestamp::new(200),
                vec![],
                done.finding.clone(),
            )
            .await
            .unwrap();
            assert_eq!(done, replay);
            assert_eq!(store.state().watermark, watermark);
            store.close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn expired_leases_reject_late_results_and_retries_end_visibly() {
        let (_temp, mut store, access, scope) = fixture(BackendKind::Sqlite).await;
        let watermark = store.state().watermark;
        let queued = enqueue(
            &mut store,
            &access,
            &scope,
            &extractor(),
            watermark,
            limits(),
        )
        .await
        .unwrap();
        let id = &queued.jobs[0];
        let first = lease(
            &mut store,
            &access,
            id,
            Revision::ZERO,
            Timestamp::new(1),
            limits(),
        )
        .await
        .unwrap();
        let old_token = &first.lease.as_ref().unwrap().token;
        assert!(complete(
            &mut store,
            &access,
            id,
            old_token,
            Timestamp::new(101),
            vec![],
            Some("late".into())
        )
        .await
        .is_err());
        let second = lease(
            &mut store,
            &access,
            id,
            first.revision,
            Timestamp::new(101),
            limits(),
        )
        .await
        .unwrap();
        assert_ne!(first.lease, second.lease);
        assert!(complete(
            &mut store,
            &access,
            id,
            old_token,
            Timestamp::new(102),
            vec![],
            Some("stale owner".into())
        )
        .await
        .is_err());
        let failed = defer(
            &mut store,
            &access,
            id,
            &second.lease.unwrap().token,
            Timestamp::new(102),
            Timestamp::new(110),
            "missing evidence after bounded retries".into(),
        )
        .await
        .unwrap();
        assert_eq!(failed.state, JobState::Failed);
        assert_eq!(failed.attempts, Units::new(2));
        assert!(lease(
            &mut store,
            &access,
            id,
            failed.revision,
            Timestamp::new(110),
            limits()
        )
        .await
        .is_err());
        store.close().await.unwrap();
    }
}
