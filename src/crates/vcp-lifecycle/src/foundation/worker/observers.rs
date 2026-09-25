// SPDX-License-Identifier: Apache-2.0
//! Canonical root-owned local observation tasks. These typed projections are
//! not agent children: they have no thread, executable capability or allocation.
use super::super::{
    observers::Configuration,
    routing_state::{cycles, HistoryWindow},
    scheduler,
};
use super::*;
use codex_protocol::ThreadId;
use serde::{Deserialize, Serialize};
use std::{cell::Cell, collections::BTreeSet};
use vcp_engine::observers::{
    self as contract,
    subscription::{Input, State as ObserverState, Work},
};
use vcp_memory::retention::{recall_allowed, Target};

const TYPE: &str = "vcp_observer_state_v1";
const MAX_BYTES: usize = 1024 * 1024;
const PAGE: usize = 128;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Count streaming JSON bytes without allocating a snapshot copy. Every write
/// also cooperates with the local capture deadline, including string payloads.
struct SourceBudget {
    bytes: usize,
    until: std::time::Instant,
}
impl SourceBudget {
    fn check(&self) -> std::io::Result<()> {
        if std::time::Instant::now() >= self.until {
            return Err(std::io::Error::other(
                "observer source capture deadline exceeded",
            ));
        }
        Ok(())
    }
}
impl std::io::Write for SourceBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.check()?;
        if bytes.len() > MAX_SOURCE_BYTES.saturating_sub(self.bytes) {
            return Err(std::io::Error::other(
                "observer source snapshot exceeds 4 MiB byte ceiling",
            ));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.check()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    document_type: String,
    scope: Scope,
    revision: Revision,
    owner: String,
    state: ObserverState,
    notice: String,
    elapsed_millis: u64,
}
pub(in crate::foundation) struct Prepared {
    pub work: Work,
    pub source: cycles::Prepared,
    pub permit: ComputePermit,
}
pub(in crate::foundation) struct ComputePermit(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for ComputePermit {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}
pub(in crate::foundation) enum Selection {
    Idle,
    Debounce(u64),
    Compute(Prepared),
}

fn id(root: &TaskId) -> String {
    format!(
        "observer-{}",
        vcp_protocol::digest_bytes(root.as_str().as_bytes())
    )
}

impl Context {
    fn observer_retained_available(&self) -> Result<bool> {
        Ok(recall_allowed(
            self.engine.store().state(),
            &self.config.workspace,
            &Target::Record(key(Collection::Projection, &id(&self.config.root_task))),
        )?)
    }
    fn observer_owner(&self) -> Result<String> {
        Ok(vcp_protocol::digest_bytes(&canonical_bytes(&(
            self.engine.controller(),
            self.engine.owner_epoch(),
        ))?))
    }
    fn observer_document(&self) -> Result<Option<Document>> {
        if !self.observer_retained_available()? {
            return Err("observer state was excluded or purged; its budget cannot be reset".into());
        }
        let Some(row) = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Projection, &id(&self.config.root_task)))
        else {
            return Ok(None);
        };
        let document: Document = row.decode()?;
        if document.schema_version != 1
            || document.document_type != TYPE
            || document.scope.task != self.config.root_task
            || document.scope.workspace != self.config.workspace
            || document.scope.session != self.config.session
            || document.revision != row.revision
        {
            return Err("observer state scope or version mismatch".into());
        }
        document.state.validate()?;
        Ok(Some(document))
    }
    fn save_observer(&mut self, document: Document) -> Result<()> {
        self.save_observer_progress(document, true)
    }
    fn save_observer_progress(&mut self, mut document: Document, emit_event: bool) -> Result<()> {
        self.engine.authorize(&self.access)?;
        if !self.observer_retained_available()? {
            return Err("observer state was excluded or purged; publication denied".into());
        }
        let record_id = id(&self.config.root_task);
        let previous = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Projection, &record_id))
            .map(|r| r.revision);
        document.revision = previous
            .map(Revision::next)
            .transpose()?
            .unwrap_or_default();
        document.state.validate()?;
        if canonical_bytes(&document)?.len() > MAX_BYTES {
            return Err("observer durable state limit exceeded".into());
        }
        let mut record = Record::typed(
            Collection::Projection,
            &record_id,
            self.config.workspace.clone(),
            document.revision,
            &document,
        )?;
        if let Some(previous) = self
            .engine
            .store()
            .state()
            .records
            .get(&key(Collection::Projection, &record_id))
        {
            record
                .references
                .extend(previous.references.iter().cloned());
        }
        record
            .references
            .insert(key(Collection::Task, self.config.root_task.as_str()));
        for attempt in &document.state.attempts {
            record
                .references
                .insert(key(Collection::Task, attempt.work.input.task.as_str()));
        }
        // Admission inputs derive from retained verification records even when
        // computation abstains or has not yet published a proposal. Keep those
        // dependencies in the canonical retention closure as well.
        if (!document.state.attempts.is_empty() || document.state.queued.is_some())
            && self.engine.store().state().records.len() <= 4096
        {
            for row in self.engine.store().state().records.values().filter(|r| {
                r.collection == Collection::Verification && r.workspace == self.config.workspace
            }) {
                // Store validation already established this typed scope. Borrow
                // its identity without copying a potentially large check body,
                // including when source-byte preflight just caused abstention.
                if row.value["scope"]["task"].as_str() == Some(self.config.root_task.as_str()) {
                    record
                        .references
                        .insert(key(Collection::Verification, &row.id));
                }
            }
        }
        for proposal in &document.state.proposals {
            for verification in &proposal.verifications {
                record
                    .references
                    .insert(key(Collection::Verification, verification.as_str()));
            }
        }
        let event = vcp_protocol::event::EventInput {
            id: EventId::new(),
            workspace: document.scope.workspace.clone(),
            session: document.scope.session.clone(),
            task: Some(document.scope.task.clone()),
            actor: self.config.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: now(),
            kind: vcp_protocol::event::EventKind::Diagnostic,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"observer":contract::VERSION,"observation_task":record_id,"revision":document.revision,"notice":document.notice,"provider_requests":0}),
            metadata: None,
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.engine.store().state().watermark,
            mutations: vec![Mutation::Put {
                record,
                expected: previous,
            }],
            events: if emit_event { vec![event] } else { vec![] },
            command: None,
        };
        self.runtime
            .block_on(self.engine.store_mut().transact(transaction))?;
        Ok(())
    }
    pub(in crate::foundation) fn observers_enabled(&self) -> bool {
        self.observers.as_ref().is_some_and(|c| c.enabled)
    }
    pub(in crate::foundation) fn configure_observers(
        &mut self,
        configuration: Configuration,
    ) -> Result<()> {
        configuration.validate()?;
        self.engine.authorize(&self.access)?;
        if !self.owner_alive || self.authority_pending {
            return Err("observer setup requires current owner".into());
        }
        let existing = self.observer_document()?;
        if !configuration.enabled && existing.is_none() {
            self.observers = Some(configuration);
            return Ok(());
        }
        let mut document = match existing {
            Some(document) => document,
            None => Document {
                schema_version: 1,
                document_type: TYPE.into(),
                scope: Scope {
                    workspace: self.config.workspace.clone(),
                    session: self.config.session.clone(),
                    task: self.config.root_task.clone(),
                },
                revision: Revision::ZERO,
                owner: self.observer_owner()?,
                state: ObserverState::new(
                    self.config.root_task.clone(),
                    configuration.limits.clone(),
                )?,
                notice: "Explicit local observer configuration; factual advice only".into(),
                elapsed_millis: 0,
            },
        };
        if document.state.limits != configuration.limits {
            return Err(
                "observer limits are fixed for this root; retain its existing bounded budget"
                    .into(),
            );
        }
        if document.owner != self.observer_owner()? {
            document.state.recover()?;
            document.owner = self.observer_owner()?;
        }
        document.state.set_enabled(configuration.enabled)?;
        if configuration.enabled {
            // Explicit native owner configuration supplies the existing root
            // cap even before any model request has initialized its ledger.
            // This does not reserve or charge model work.
            self.ensure_coding_ledger()?;
        }
        self.save_observer(document)?;
        self.observers = Some(configuration);
        Ok(())
    }
    pub(super) fn recover_observers(&mut self) -> Result<()> {
        if !self.observer_retained_available()? {
            return Ok(());
        }
        if let Some(mut document) = self.observer_document()? {
            document.state.recover()?;
            document.owner = self.observer_owner()?;
            document.notice="Owner reopened; interrupted observation tasks remain historical and are never replayed".into();
            self.save_observer(document)?;
        }
        Ok(())
    }
    fn observer_input(&self, binding: &ThreadBinding, deadline: Timestamp) -> Result<Input> {
        self.validate_binding(binding)?;
        let state = self.engine.store().state();
        let limit = self
            .observers
            .as_ref()
            .map_or(4096, |c| c.limits.steps_per_attempt as usize);
        if state.records.len().saturating_add(state.events.len()) > limit {
            return Err("observer source snapshot exceeds its step ceiling".into());
        }
        let mut capture = SourceBudget {
            bytes: 0,
            until: std::time::Instant::now()
                + std::time::Duration::from_millis(
                    self.observers
                        .as_ref()
                        .map_or(2000, |c| c.limits.deadline_ms),
                ),
        };
        serde_json::to_writer(&mut capture, state)?;
        capture.check()?;
        let task: Task = state
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                binding.scope.workspace.as_str(),
                &binding.scope.workspace,
            )?
            .decode()?;
        let policy = vcp_engine::policy::optional(state, &binding.scope.workspace)?;
        let mut verifications = Vec::new();
        let mut watermark = Watermark::ZERO;
        // Source capture has a separate bounded step budget. The input identity
        // contains only this task's canonical verification records, not observer
        // publication or hook output, so observations cannot trigger themselves.
        for row in state
            .records
            .values()
            .filter(|r| r.collection == Collection::Verification)
        {
            capture.check()?;
            let verification: vcp_domain::verification::Verification = row.decode()?;
            if verification.scope == binding.scope {
                if verification.redaction.is_some()
                    || !recall_allowed(
                        state,
                        &binding.scope.workspace,
                        &Target::Record(key(Collection::Verification, &row.id)),
                    )?
                {
                    return Err("observer source verification is no longer readable".into());
                }
                capture.check()?;
                verifications.push((
                    row.id.clone(),
                    row.revision,
                    vcp_protocol::digest_bytes(&canonical_bytes(&verification)?),
                ));
            }
            if verifications.len() > 4096 {
                return Err("observer verification input limit exceeded".into());
            }
        }
        for event in state.events.iter().rev() {
            capture.check()?;
            if event.event.task.as_ref() == Some(&binding.scope.task)
                && event.event.kind == vcp_protocol::event::EventKind::VerificationRecorded
            {
                watermark = event.watermark;
                break;
            }
        }
        capture.check()?;
        let input = Input {
            root: task.root,
            task: task.scope.task,
            steering: task.steering,
            task_revision: task.revision,
            authority: workspace.authority,
            deletion: workspace.deletion,
            input_digest: vcp_protocol::digest_bytes(&canonical_bytes(&(
                task.fingerprint,
                task.steering,
                task.revision,
                workspace.authority,
                workspace.deletion,
                policy,
            ))?),
            pattern_digest: vcp_protocol::digest_bytes(&canonical_bytes(&verifications)?),
            watermark,
            deadline,
        };
        capture.check()?;
        Ok(input)
    }
    pub(in crate::foundation) fn observer_status(
        &self,
        binding: &ThreadBinding,
    ) -> Result<serde_json::Value> {
        self.engine.authorize(&self.access)?;
        self.validate_binding(binding)?;
        if !self.observer_retained_available()? {
            return Ok(
                serde_json::json!({"enabled":false,"observer":contract::VERSION,
                "state":null,"notice":"Observer state excluded or purged; retained budget cannot be reset",
                "provider_requests":0,"charged_micros":0,"uncertain_liability_micros":0}),
            );
        }
        let Some(mut document) = self.observer_document()? else {
            return Ok(
                serde_json::json!({"enabled":false,"observer":contract::VERSION,"state":null,"provider_requests":0,"charged_micros":0,"uncertain_liability_micros":0}),
            );
        };
        // The qualified subscription is root-only: revalidate the authorized
        // root source once, rather than rehashing it for every retained fact.
        let current = if !document.state.proposals.is_empty()
            && binding.scope.task == self.config.root_task
            && self.can_start(binding).is_ok()
        {
            self.observer_input(binding, now()).ok()
        } else {
            None
        };
        let latest_failed = current.is_some() && self.observer_latest_failed(binding);
        for proposal in &mut document.state.proposals {
            if proposal.input.task != binding.scope.task
                || !current
                    .as_ref()
                    .is_some_and(|input| input.input_digest == proposal.input.input_digest)
                || !latest_failed
            {
                proposal.disposition = contract::proposal::Disposition::Historical;
            }
        }
        document.state.proposals.retain(|proposal| {
            proposal.verifications.iter().all(|id| {
                recall_allowed(
                    self.engine.store().state(),
                    &binding.scope.workspace,
                    &Target::Record(key(Collection::Verification, id.as_str())),
                )
                .unwrap_or(false)
            })
        });
        let bytes = canonical_bytes(&document)?.len();
        Ok(
            serde_json::json!({"enabled":self.observers_enabled()&&document.state.enabled,"observer":contract::VERSION,"state":document.state,"notice":document.notice,"measured":{"elapsed_millis":document.elapsed_millis,"serialized_bytes":bytes},"provider_requests":0,"charged_micros":0,"uncertain_liability_micros":0,"authority":"factual local advice only; no automatic action or grant","observation_task_kind":"root_owned_local_projection"}),
        )
    }
    pub(in crate::foundation) fn prepare_observer(
        &mut self,
        binding: &ThreadBinding,
        runtime: &crate::Lifecycle,
        thread: ThreadId,
        generation: u64,
    ) -> Result<Selection> {
        if !self.observers_enabled() || !self.observer_retained_available()? {
            return Ok(Selection::Idle);
        }
        // Initial qualified subscription is root-only. Child task event cursors
        // are not silently consumed as though this were a general task observer.
        if binding.scope.task != self.config.root_task {
            return Ok(Selection::Idle);
        }
        let Some(mut document) = self.observer_document()? else {
            return Ok(Selection::Idle);
        };
        if !document.state.enabled {
            return Ok(Selection::Idle);
        }
        if self.can_start(binding).is_err()
            || scheduler::check_generation(runtime, thread, generation).is_err()
        {
            return Ok(Selection::Idle);
        }
        let ledger = match vcp_budget::ledger(self.engine.store().state(), &binding.scope) {
            Ok(ledger) => ledger,
            Err(_) => {
                let notice = "Observer paused: shared root cost ledger unavailable";
                if document.notice != notice || document.state.queued.is_some() {
                    document.notice = notice.into();
                    document.state.queued = None;
                    self.save_observer(document)?;
                }
                return Ok(Selection::Idle);
            }
        };
        if ledger.overrun
            || ledger
                .settled
                .get()
                .saturating_add(ledger.active.get())
                .saturating_add(ledger.unresolved.get())
                >= ledger.cap.get()
        {
            let notice = "Observer paused: shared root cost cap exhausted";
            if document.notice != notice || document.state.queued.is_some() {
                document.notice = notice.into();
                document.state.queued = None;
                self.save_observer(document)?;
            }
            return Ok(Selection::Idle);
        }
        let old = document.state.clone();
        let state = self.engine.store().state();
        let start = state
            .events
            .partition_point(|e| e.watermark <= document.state.cursor);
        let mut end = (start + PAGE).min(state.events.len());
        while end < state.events.len()
            && end > start
            && state.events[end].watermark == state.events[end - 1].watermark
            && end - start < 4096
        {
            end += 1;
        }
        if end < state.events.len()
            && end > start
            && state.events[end].watermark == state.events[end - 1].watermark
        {
            return Err("observer event transaction exceeds bounded page".into());
        }
        let relevant = state.events[start..end].iter().any(|e| {
            e.event.task.as_ref() == Some(&binding.scope.task)
                && e.event.kind == vcp_protocol::event::EventKind::VerificationRecorded
        });
        // Cursor-only commits emit no event, so even a full page of our own
        // receipts advances without either starvation or a feedback loop.
        let cutoff = state.events[start..end].last().map(|e| e.watermark);
        let time = now();
        if relevant {
            match self.observer_input(
                binding,
                Timestamp::new(time.get().saturating_add(
                    document.state.limits.deadline_ms + document.state.limits.debounce_ms,
                )),
            ) {
                Ok(input) => {
                    document.state.enqueue(input, true)?;
                }
                Err(error) => {
                    document.notice = format!("Observer abstained: {error}");
                    document.state.queued = None;
                    if let Some(cutoff) = cutoff {
                        document.state.advance_cursor(cutoff)?;
                    }
                    self.save_observer(document)?;
                    return Ok(Selection::Idle);
                }
            }
        }
        if let Some(cutoff) = cutoff {
            if cutoff > document.state.cursor {
                document.state.advance_cursor(cutoff)?;
            }
        }
        if let Some(queued) = &document.state.queued {
            let current = self.observer_input(binding, queued.deadline);
            if !current.as_ref().is_ok_and(|current| {
                contract::dedup::key(current).ok() == contract::dedup::key(queued).ok()
            }) {
                document.state.queued = None;
                document.notice =
                    "Queued observation input changed; discarded before admission".into();
            }
        }
        // Pin the runtime admission barrier through durable Running publication.
        let runtime_state = runtime
            .0
            .state
            .lock()
            .map_err(|_| "observer runtime fence unavailable")?;
        if !runtime_state.attached
            || runtime_state.held(thread)
            || !runtime_state
                .entries
                .get(&thread)
                .is_some_and(|entry| entry.admission_generation == generation)
        {
            return Ok(Selection::Idle);
        }
        if self
            .observer_busy
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return Ok(Selection::Idle);
        }
        let permit = ComputePermit(self.observer_busy.clone());
        let work = match document.state.begin(time, true) {
            Ok(work) => work,
            Err(error) => {
                document.notice = format!("Observer admission stopped: {error}");
                document.state.queued = None;
                self.save_observer(document)?;
                return Ok(Selection::Idle);
            }
        };
        let Some(work) = work else {
            let debounce = document
                .state
                .queued
                .is_some()
                .then_some(document.state.limits.debounce_ms);
            if document.state != old {
                self.save_observer_progress(document, false)?;
            }
            return Ok(debounce.map_or(Selection::Idle, Selection::Debounce));
        };
        document.notice = "Bounded local observation task admitted; zero model requests".into();
        self.save_observer(document)?;
        drop(runtime_state);
        // A durable Running attempt exists before capture/computation. The
        // callback bounds canonical traversal and checks pause/deadline often.
        let steps = Cell::new(0u32);
        let check = || -> super::super::routing_state::Result<()> {
            let value = steps.get().saturating_add(1);
            steps.set(value);
            if value > work.reserved_steps
                || now() >= work.deadline
                || scheduler::check_generation(runtime, thread, generation).is_err()
            {
                return Err("observer capture step/deadline/admission bound".into());
            }
            Ok(())
        };
        let mut access = self.routing_access();
        if !access.allows_task(&binding.scope.task) {
            return Err("observer source scope denied".into());
        }
        access.tasks = Some(BTreeSet::from([binding.scope.task.clone()]));
        let source = cycles::prepare(
            self.engine.store(),
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(time.get().saturating_add(1)),
            },
            &check,
        );
        match source {
            Ok(source) => Ok(Selection::Compute(Prepared {
                work,
                source,
                permit,
            })),
            Err(error) => {
                let mut document = self.observer_document()?.ok_or("observer state absent")?;
                document.state.complete_without_proposal(&work, now())?;
                document.notice = format!("Observer abstained: {error}");
                self.save_observer(document)?;
                Ok(Selection::Idle)
            }
        }
    }
    pub(in crate::foundation) fn complete_observer(
        &mut self,
        binding: &ThreadBinding,
        runtime: &crate::Lifecycle,
        thread: ThreadId,
        generation: u64,
        work: Work,
        result: std::result::Result<cycles::Evidence, String>,
        elapsed: u64,
    ) -> Result<()> {
        let mut document = self.observer_document()?.ok_or("observer state absent")?;
        let current_result = self.observer_input(binding, work.input.deadline);
        // Hold the runtime barrier through final publication, so a pause cannot
        // slip between the last generation check and a Current receipt.
        let runtime_state = runtime
            .0
            .state
            .lock()
            .map_err(|_| "observer runtime fence unavailable")?;
        let generation_current = runtime_state.attached
            && !runtime_state.held(thread)
            && runtime_state
                .entries
                .get(&thread)
                .is_some_and(|entry| entry.admission_generation == generation);
        let live = current_result.is_ok()
            && self.can_start(binding).is_ok()
            && generation_current
            && document.owner == self.observer_owner()?
            && self.observer_budget_available(binding);
        let current = current_result.unwrap_or_else(|_| work.input.clone());
        let finished_at = now();
        match result {
            Ok(evidence) => {
                let repetition = evidence.repetitions.iter().find(|r| {
                    r.task == work.input.task
                        && r.steering == work.input.steering
                        && r.task_revision == work.input.task_revision
                });
                if let Some(repetition) = repetition.filter(|r| {
                    r.verifications
                        .iter()
                        .all(|id| self.observer_source_readable(binding, id))
                }) {
                    if finished_at >= work.deadline {
                        document
                            .state
                            .complete_without_proposal(&work, finished_at)?;
                        document.notice = "Observer result expired before publication".into();
                        document.elapsed_millis = document.elapsed_millis.saturating_add(elapsed);
                        return self.save_observer(document);
                    }
                    document.state.complete_fact(
                        &work,
                        &current,
                        live,
                        finished_at,
                        repetition.pattern_digest.clone(),
                        evidence.id,
                        repetition.verifications.clone(),
                    )?;
                    document.notice="Exact failed-verification pattern repeated; inspect grouped source references. This is not a stall diagnosis or action permission".into();
                } else {
                    document
                        .state
                        .complete_without_proposal(&work, finished_at)?;
                    document.notice =
                        "No exact verification repetition found in the authorized bounded window"
                            .into();
                }
            }
            Err(error) => {
                document
                    .state
                    .complete_without_proposal(&work, finished_at)?;
                document.notice = format!("Observer abstained: {error}");
            }
        }
        document.elapsed_millis = document.elapsed_millis.saturating_add(elapsed);
        self.save_observer(document)
    }
    pub(in crate::foundation) fn abandon_observer(&mut self, work: Work) -> Result<()> {
        let Some(mut document) = self.observer_document()? else {
            return Ok(());
        };
        let Some(attempt) = document.state.attempts.iter_mut().find(|attempt| {
            attempt.work == work && attempt.status == contract::subscription::AttemptStatus::Running
        }) else {
            return Ok(());
        };
        attempt.status = contract::subscription::AttemptStatus::Interrupted;
        document.notice="Observation waiter ended; bounded local work is interrupted and never replayed automatically".into();
        self.save_observer(document)
    }
    fn observer_source_readable(&self, binding: &ThreadBinding, id: &VerificationId) -> bool {
        let state = self.engine.store().state();
        state
            .record(
                Collection::Verification,
                id.as_str(),
                &binding.scope.workspace,
            )
            .is_ok_and(|r| {
                // These identity fields were type-checked by the canonical
                // store. Publication needs no copy of the verification body.
                r.value["redaction"].is_null()
                    && r.value["scope"]["workspace"].as_str()
                        == Some(binding.scope.workspace.as_str())
                    && r.value["scope"]["session"].as_str() == Some(binding.scope.session.as_str())
                    && r.value["scope"]["task"].as_str() == Some(binding.scope.task.as_str())
            })
            && recall_allowed(
                state,
                &binding.scope.workspace,
                &Target::Record(key(Collection::Verification, id.as_str())),
            )
            .unwrap_or(false)
    }
    fn observer_budget_available(&self, binding: &ThreadBinding) -> bool {
        vcp_budget::ledger(self.engine.store().state(), &binding.scope).is_ok_and(|l| {
            !l.overrun
                && l.settled
                    .get()
                    .saturating_add(l.active.get())
                    .saturating_add(l.unresolved.get())
                    < l.cap.get()
        })
    }
    fn observer_latest_failed(&self, binding: &ThreadBinding) -> bool {
        use vcp_domain::verification::{CheckOutcome, Verification};
        let state = self.engine.store().state();
        let Some(event) = state.events.iter().rev().find(|e| {
            e.event.task.as_ref() == Some(&binding.scope.task)
                && e.event.kind == vcp_protocol::event::EventKind::VerificationRecorded
        }) else {
            return false;
        };
        event.event.data["facts"].as_array().is_some_and(|facts| {
            facts
                .iter()
                .filter(|f| f["collection"] == "verification")
                .any(|fact| {
                    serde_json::from_value::<Verification>(fact["value"].clone()).is_ok_and(|v| {
                        v.redaction.is_none()
                            && v.checks
                                .iter()
                                .any(|c| matches!(c.outcome, CheckOutcome::Failed { .. }))
                    })
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::SerializeSeq;

    #[test]
    fn source_byte_preflight_aborts_stream_before_large_input_is_materialized() {
        struct Repeated<'a>(&'a Cell<usize>);
        impl Serialize for Repeated<'_> {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                let chunk = "x".repeat(4096);
                let mut sequence = serializer.serialize_seq(Some(1_000_000))?;
                for _ in 0..1_000_000 {
                    self.0.set(self.0.get() + 1);
                    sequence.serialize_element(&chunk)?;
                }
                sequence.end()
            }
        }
        let visits = Cell::new(0);
        let mut budget = SourceBudget {
            bytes: 0,
            until: std::time::Instant::now() + std::time::Duration::from_secs(30),
        };
        let error = serde_json::to_writer(&mut budget, &Repeated(&visits)).unwrap_err();
        assert!(error.to_string().contains("4 MiB byte ceiling"));
        assert!(visits.get() < 1100);
        assert!(budget.bytes <= MAX_SOURCE_BYTES);
    }

    #[test]
    fn source_byte_preflight_checks_deadline_before_payload() {
        let mut budget = SourceBudget {
            bytes: 0,
            until: std::time::Instant::now(),
        };
        let error = serde_json::to_writer(&mut budget, &"bounded").unwrap_err();
        assert!(error.to_string().contains("capture deadline"));
        assert_eq!(budget.bytes, 0);
    }
}
