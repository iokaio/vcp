// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::Cell;
use vcp_domain::*;
use vcp_protocol::{
    command::CommandReceipt,
    event::{EventEnvelope, EventKind},
};
use vcp_store::{CanonicalHistory, CurrentStateView};

/// Explicit reference reader: deliberately short pages, no mutation or archive API.
struct Reader {
    state: State,
    width: usize,
    reads: Cell<usize>,
    fail: bool,
    empty: bool,
    missing: bool,
    receipt_failure: bool,
}
impl Reader {
    fn new(state: State) -> Self {
        Self {
            state,
            width: 3,
            reads: Cell::new(0),
            fail: false,
            empty: false,
            missing: false,
            receipt_failure: false,
        }
    }
    fn read(&self) -> vcp_store::Result<()> {
        self.reads.set(self.reads.get() + 1);
        if self.fail {
            Err(vcp_store::Error::Unavailable("injected history failure"))
        } else {
            Ok(())
        }
    }
}
impl CanonicalHistory for Reader {
    fn current(&self) -> CurrentStateView<'_> {
        (&self.state).into()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        self.read()?;
        Ok(self.state.events.len() as u64)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        self.read()?;
        if self.empty {
            return Ok(vec![]);
        }
        let start = after.map_or(0, |n| n as usize + 1);
        Ok(self
            .state
            .events
            .iter()
            .skip(start)
            .take(limit.min(self.width))
            .cloned()
            .collect())
    }
    async fn history_event_at(&self, ordinal: u64) -> vcp_store::Result<Option<EventEnvelope>> {
        self.read()?;
        Ok(self.state.events.get(ordinal as usize).cloned())
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.read()?;
        Ok(if self.missing {
            None
        } else {
            self.state
                .events
                .iter()
                .find(|e| e.event.id == *id)
                .cloned()
        })
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        self.read()?;
        if self.receipt_failure {
            return Err(vcp_store::Error::Unavailable("injected receipt failure"));
        }
        self.state.command(workspace, command, digest)
    }
}

#[tokio::test]
async fn unavailable_acceptance_receipt_stays_explicitly_unknown() {
    use vcp_protocol::{command::CommandResult, methods};
    let (mut state, access, task) = tests::fixture();
    let mut initial: task::Task = state
        .record(Collection::Task, task.as_str(), &access.workspace)
        .unwrap()
        .decode()
        .unwrap();
    initial.state = task::TaskState::Pending;
    initial.reason = "public run accepted; retained construction and dispatch pending".into();
    initial.cause = state.events[0].event.id.clone();
    initial.objectives[0].source = initial.cause.clone();
    let id = |s: &str| -> methods::Id { s.to_owned().try_into().unwrap() };
    let request = methods::TurnStart {
        scope: methods::Scope {
            workspace: id(access.workspace.as_str()),
            session: id(initial.scope.session.as_str()),
        },
        mutation: methods::Mutation {
            command_id: id(state.events[0].event.correlation.as_str()),
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: id(task.as_str()),
        turn: id("initial-turn"),
        objective: initial.objectives[0].text.clone(),
        constraints: initial.objectives[0].constraints.clone(),
        acceptance: initial.objectives[0].acceptance.clone(),
        budget: methods::Budget {
            cap_micros: Limit::Finite(1000000.into()),
            currency: methods::Currency::Usd,
            max_requests: 3,
            deadline_seconds: Limit::Finite(30),
        },
    };
    for event in state.events.iter_mut().skip(1) {
        event.event.kind = EventKind::Diagnostic;
    }
    state.events[0].event.data = json!({"schema_version":1,"facts":[{"collection":"task","id":task,"revision":"0","value":initial}],
        "public_start":{"schema_version":1,"task":task,"turn":request.turn,"budget":request.budget}});
    let event = &state.events[0];
    let receipt = CommandReceipt {
        version: 1,
        command: event.event.correlation.clone(),
        workspace: access.workspace.clone(),
        digest: methods::Call::TurnStart(request)
            .digest(event.event.actor.as_str())
            .unwrap(),
        transaction: TransactionId::new(),
        watermark: event.watermark,
        first_event: event.sequence,
        last_event: event.sequence,
        result: CommandResult::Accepted {
            revision: Revision::ZERO,
        },
    };
    state.commands.insert(
        vcp_store::contract::command_key(&access.workspace, &receipt.command),
        receipt,
    );
    let mut reader = Reader::new(state);
    let proved = collect_store(&reader, &access, &task).await.unwrap();
    assert_eq!(
        proved["effective_constraints"]["original_acceptance"]["status"],
        "recorded"
    );
    reader.receipt_failure = true;
    let unavailable = collect_store(&reader, &access, &task).await.unwrap();
    assert_eq!(
        unavailable["effective_constraints"]["original_acceptance"]["status"],
        "unknown"
    );
    assert!(unavailable["effective_constraints"]["original_acceptance"]
        .get("budget")
        .is_none());
}

/// Lazy unrelated payloads exercise >64MiB logical history without allocating
/// a full State in this fixture. Native cut integrity is tested separately.
struct LargeReader {
    base: Reader,
    payload_bytes: Cell<usize>,
}
impl LargeReader {
    fn row(&self, ordinal: usize) -> EventEnvelope {
        if ordinal < self.base.state.events.len() {
            return self.base.state.events[ordinal].clone();
        }
        let mut event = self.base.state.events[0].clone();
        event.event.id = EventId::parse(format!("unrelated-{ordinal}")).unwrap();
        event.sequence = SessionSeq::new(ordinal as u64 + 1);
        event.event.task = None;
        event.event.kind = EventKind::Diagnostic;
        event.event.data = json!({"payload":"x".repeat(6*1024*1024)});
        self.payload_bytes
            .set(self.payload_bytes.get() + 6 * 1024 * 1024);
        event
    }
}
impl CanonicalHistory for LargeReader {
    fn current(&self) -> CurrentStateView<'_> {
        self.base.current()
    }
    async fn history_event_count(&self) -> vcp_store::Result<u64> {
        Ok(self.base.state.events.len() as u64 + 11)
    }
    async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> vcp_store::Result<Vec<EventEnvelope>> {
        assert!(limit > 0 && limit <= 4096);
        let next = after.map_or(0, |n| n as usize + 1);
        if next >= self.base.state.events.len() + 11 {
            Ok(vec![])
        } else {
            Ok(vec![self.row(next)])
        }
    }
    async fn history_event_at(&self, ordinal: u64) -> vcp_store::Result<Option<EventEnvelope>> {
        Ok(if ordinal < self.base.state.events.len() as u64 + 11 {
            Some(self.row(ordinal as usize))
        } else {
            None
        })
    }
    async fn history_event(&self, id: &EventId) -> vcp_store::Result<Option<EventEnvelope>> {
        self.base.history_event(id).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> vcp_store::Result<Option<CommandReceipt>> {
        self.base.command_receipt(workspace, command, digest).await
    }
}
#[tokio::test]
async fn scoped_bundle_fits_while_unrelated_logical_history_exceeds_legacy_capacity() {
    let (state, access, task) = tests::fixture();
    let reader = LargeReader {
        base: Reader::new(state),
        payload_bytes: Cell::new(0),
    };
    let bundle = collect_store(&reader, &access, &task).await.unwrap();
    assert!(11 * 6 * 1024 * 1024 > vcp_store::contract::MAX_STATE_BYTES);
    assert!(reader.payload_bytes.get() >= 11 * 6 * 1024 * 1024);
    assert!(serde_json::to_vec(&bundle).unwrap().len() < MAX_BYTES);
    assert!(!bundle.to_string().contains("unrelated-"));
}
fn normalized(mut value: Value) -> Value {
    value["collection"]["elapsed_micros"] = json!(0);
    value
}
fn diagnostic(state: &mut State, index: usize, scope: &workspace::Scope) {
    let event = &mut state.events[index];
    event.event.kind = EventKind::Diagnostic;
    event.event.data = json!({"version":1,"capture_boundary":"owner_drained","source_watermark":"0",
        "execution_diagnostics":{"schema_version":1,"owner":"prior-owner","window":"current_owner_only",
            "snapshot_micros":1000,"complete_history":false,"available":true,"capacity":256,"dropped":3,
            "observations":(0..100).map(|sequence| json!({"sequence":sequence,"phase":"provider_exchange",
                "scope":scope,"turn":null,"attempt":null,"started_micros":10,"elapsed_micros":20,"status":"interrupted"})).collect::<Vec<_>>()}});
}

#[tokio::test]
async fn bounded_bundle_matches_reference_with_short_pages_masks_and_expanded_diagnostics() {
    let (mut state, access, task) = tests::fixture();
    let selected: task::Task = state
        .record(Collection::Task, task.as_str(), &access.workspace)
        .unwrap()
        .decode()
        .unwrap();
    diagnostic(&mut state, 0, &selected.scope);
    diagnostic(&mut state, 129, &selected.scope);
    for masked in [false, true] {
        if masked {
            let mask = retention::RetentionMask {
                schema_version: 1,
                workspace: access.workspace.clone(),
                session: selected.scope.session.clone(),
                first: SessionSeq::new(1),
                last: SessionSeq::new(1),
                artifacts: vec![],
                deletion: DeletionEpoch::ZERO,
                reason: "fixture mask".into(),
            };
            let row = vcp_store::contract::Record::typed(
                Collection::Tombstone,
                "mask",
                access.workspace.clone(),
                Revision::ZERO,
                &mask,
            )
            .unwrap();
            state.records.insert(row.key(), row);
        }
        let expected = normalized(collect(&state, &access, &task).unwrap());
        let reader = Reader::new(state.clone());
        let actual = normalized(collect_store(&reader, &access, &task).await.unwrap());
        assert_eq!(actual, expected);
        assert_eq!(
            actual["retained_lifecycle_diagnostics"]
                .as_array()
                .unwrap()
                .len(),
            if masked { 1 } else { 2 }
        );
        assert!(
            reader.reads.get() > 40,
            "fixture actually crosses short source pages"
        );
    }
}

#[tokio::test]
async fn bounded_bundle_denies_before_history_and_rejects_failed_or_missing_reads() {
    let (state, access, task) = tests::fixture();
    let mut reader = Reader::new(state);
    reader.fail = true;
    let denied = Access {
        workspace: access.workspace.clone(),
        authority: access.authority,
        read: false,
        tasks: None,
    };
    assert!(collect_store(&reader, &denied, &task)
        .await
        .unwrap_err()
        .contains("access denied"));
    assert_eq!(reader.reads.get(), 0);
    assert!(collect_store(&reader, &access, &task)
        .await
        .unwrap_err()
        .contains("injected history failure"));
    reader.fail = false;
    reader.empty = true;
    assert!(collect_store(&reader, &access, &task).await.is_err());
    reader.empty = false;
    let selected: task::Task = reader
        .state
        .record(Collection::Task, task.as_str(), &access.workspace)
        .unwrap()
        .decode()
        .unwrap();
    diagnostic(&mut reader.state, 129, &selected.scope);
    reader.missing = true;
    assert!(collect_store(&reader, &access, &task)
        .await
        .unwrap_err()
        .contains("visible diagnostic event missing"));
}

#[tokio::test]
async fn diagnostic_expansion_obeys_remaining_bundle_budget_and_visibility() {
    let (mut state, access, task) = tests::fixture();
    let selected: task::Task = state
        .record(Collection::Task, task.as_str(), &access.workspace)
        .unwrap()
        .decode()
        .unwrap();
    diagnostic(&mut state, 129, &selected.scope);
    let bundle = collect(&state, &access, &task).unwrap();
    let mut history = bundle["history"].as_array().unwrap().clone();
    let reader = Reader::new(state);
    assert!(
        lifecycle::retained_store(&reader, &history, &selected.scope, 1)
            .await
            .unwrap_err()
            .contains("byte limit")
    );
    for visibility in ["purged", "compacted_presentation_raw_retained"] {
        for row in history
            .iter_mut()
            .flat_map(|page| page["rows"].as_array_mut().unwrap())
        {
            if row["event"]["event"]["kind"] == "diagnostic" {
                row["visibility"] = json!(visibility);
            }
        }
        reader.reads.set(0);
        assert!(
            lifecycle::retained_store(&reader, &history, &selected.scope, MAX_BYTES)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reader.reads.get(),
            0,
            "hidden payload must never be fetched"
        );
    }
}
