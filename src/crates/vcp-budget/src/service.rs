// SPDX-License-Identifier: Apache-2.0
use crate::{arithmetic::*, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vcp_domain::{
    accounting::*,
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    revision::*,
    task::{Task, TaskState},
    workspace::Scope,
};
use vcp_protocol::{canonical_bytes, digest_bytes, event::*};
use vcp_store::contract::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    pub transaction: TransactionId,
    pub attempt: AttemptId,
    pub reservation: ReservationId,
    pub scope: Scope,
    pub agent: AgentId,
    pub role: RequestRole,
    pub request: ArtifactId,
    pub request_digest: String,
    pub quote: CostQuote,
    pub previous: Option<AttemptId>,
    pub expected_ledger: Revision,
    pub policy: PolicyRevision,
    pub steering: SteeringRevision,
    pub draw_protected: bool,
    pub now: Timestamp,
}
pub struct Actor {
    pub id: ActorId,
    pub now: Timestamp,
}
/// A send capability is constructed only after the durable Submitted receipt.
/// Reopening/repeating submit never manufactures another capability to resend.
pub struct SendPermit {
    attempt: AttemptId,
    request_digest: String,
    receipt: Receipt,
}
impl SendPermit {
    pub fn attempt(&self) -> &AttemptId {
        &self.attempt
    }
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }
    pub fn receipt(&self) -> &Receipt {
        &self.receipt
    }
}
fn task(state: &State, scope: &Scope) -> Result<Task> {
    let value: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
        .decode()?;
    if &value.scope != scope {
        return Err(Error::Conflict("scope"));
    }
    Ok(value)
}
pub fn ledger(state: &State, scope: &Scope) -> Result<Ledger> {
    let task = task(state, scope)?;
    Ok(state
        .record(Collection::Ledger, task.root.as_str(), &scope.workspace)?
        .decode()?)
}
pub fn attempt(state: &State, id: &AttemptId, workspace: &WorkspaceId) -> Result<Attempt> {
    Ok(state
        .record(Collection::Attempt, id.as_str(), workspace)?
        .decode()?)
}
fn reservation(state: &State, attempt: &Attempt) -> Result<Reservation> {
    Ok(state
        .record(
            Collection::Reservation,
            attempt.reservation.as_str(),
            &attempt.scope.workspace,
        )?
        .decode()?)
}
fn put<T: Serialize>(
    collection: Collection,
    id: String,
    scope: &Scope,
    revision: Revision,
    expected: Option<Revision>,
    value: &T,
) -> Result<Mutation> {
    Ok(Mutation::Put {
        expected,
        record: Record::typed(collection, id, scope.workspace.clone(), revision, value)?,
    })
}
fn event(
    actor: &Actor,
    scope: &Scope,
    kind: EventKind,
    data: serde_json::Value,
    artifacts: Vec<ArtifactId>,
) -> EventInput {
    let metadata = data
        .get("attempt")
        .and_then(|value| serde_json::from_value::<Attempt>(value.clone()).ok())
        .map(|attempt| EventMetadata {
            agent: Some(attempt.agent),
            provider: Some(attempt.quote.price.provider),
            model: Some(attempt.quote.price.model),
            paths: vec![],
        });
    EventInput {
        id: EventId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: Some(scope.task.clone()),
        actor: actor.id.clone(),
        correlation: CommandId::new(),
        causation: None,
        timestamp: actor.now,
        kind,
        artifacts,
        data,
        metadata,
    }
}
fn transaction(
    state: &State,
    id: TransactionId,
    mutations: Vec<Mutation>,
    event: EventInput,
) -> Transaction {
    Transaction {
        id,
        expected_watermark: state.watermark,
        mutations,
        events: vec![event],
        command: None,
    }
}
fn running(state: &State, task: &Task) -> Result<()> {
    if task.state != TaskState::Running {
        return Err(Error::Denied("task is not running"));
    }
    let mut parent = task.parent.clone();
    while let Some(id) = parent {
        let ancestor: Task = state
            .record(Collection::Task, id.as_str(), &task.scope.workspace)?
            .decode()?;
        if ancestor.state != TaskState::Running {
            return Err(Error::Denied("ancestor is held"));
        }
        parent = ancestor.parent;
    }
    Ok(())
}
fn descends(
    state: &State,
    task_id: &TaskId,
    ancestor: &TaskId,
    workspace: &WorkspaceId,
) -> Result<bool> {
    let mut next = Some(task_id.clone());
    while let Some(id) = next {
        if &id == ancestor {
            return Ok(true);
        }
        let value: Task = state
            .record(Collection::Task, id.as_str(), workspace)?
            .decode()?;
        next = value.parent;
    }
    Ok(false)
}
fn refresh(state: &State, ledger: &mut Ledger, changed: Option<&Reservation>) -> Result<()> {
    let mut settled = Vec::new();
    let mut active = Vec::new();
    let mut unresolved = Vec::new();
    let mut seen = false;
    for record in state.records.values().filter(|r| {
        r.collection == Collection::Reservation && r.workspace == ledger.scope.workspace
    }) {
        let original: Reservation = record.decode()?;
        let row = if let Some(changed) = changed.filter(|c| c.id == original.id) {
            seen = true;
            changed
        } else {
            &original
        };
        if row.root != ledger.scope.task {
            continue;
        }
        settled.push(row.charged);
        match row.phase {
            ReservationState::Created | ReservationState::Submitted => active.push(row.liability),
            ReservationState::ReconciliationPending => unresolved.push(row.liability),
            _ => {}
        }
    }
    if !seen {
        if let Some(row) = changed {
            settled.push(row.charged);
            match row.phase {
                ReservationState::Created | ReservationState::Submitted => {
                    active.push(row.liability)
                }
                ReservationState::ReconciliationPending => unresolved.push(row.liability),
                _ => {}
            }
        }
    }
    ledger.settled = sum(settled)?;
    ledger.active = sum(active)?;
    ledger.unresolved = sum(unresolved)?;
    ledger.overrun = sum([
        ledger.settled,
        ledger.active,
        ledger.unresolved,
        ledger.protected,
    ])? > ledger.cap;
    Ok(())
}
pub async fn initialize<S: CanonicalStore>(
    store: &mut S,
    scope: Scope,
    cap: Money,
    protected: Micros,
    daily: Option<DailyPolicy>,
    actor: &Actor,
) -> Result<Ledger> {
    let root = task(store.state(), &scope)?;
    if root.parent.is_some() || protected > cap.micros {
        return Err(Error::Denied("invalid root or protected amount"));
    }
    let ledger = Ledger {
        schema_version: 1,
        scope: scope.clone(),
        revision: Revision::ZERO,
        policy: PolicyRevision::ZERO,
        currency: cap.currency,
        cap: cap.micros,
        protected,
        settled: Micros::ZERO,
        active: Micros::ZERO,
        unresolved: Micros::ZERO,
        allocations: BTreeMap::new(),
        daily,
        overrun: false,
    };
    ledger.validate()?;
    let mutation = put(
        Collection::Ledger,
        scope.task.to_string(),
        &scope,
        ledger.revision,
        None,
        &ledger,
    )?;
    let event = event(
        actor,
        &scope,
        EventKind::AccountingResolved,
        serde_json::json!({"schema_version":1,"ledger":ledger,"reason":"root accounting initialized"}),
        vec![],
    );
    store
        .transact(transaction(
            store.state(),
            TransactionId::new(),
            vec![mutation],
            event,
        ))
        .await?;
    Ok(ledger)
}
pub async fn configure<S: CanonicalStore>(
    store: &mut S,
    scope: &Scope,
    expected: Revision,
    cap: Money,
    protected: Micros,
    allocations: BTreeMap<TaskId, Micros>,
    actor: &Actor,
    reason: &str,
) -> Result<Ledger> {
    let mut next = ledger(store.state(), scope)?;
    if scope != &next.scope || next.revision != expected || reason.trim().is_empty() {
        return Err(Error::Conflict("budget policy revision or actor scope"));
    }
    if cap.currency != next.currency {
        return Err(Error::Currency);
    }
    next.revision = next.revision.next()?;
    next.policy = next.policy.next()?;
    next.cap = cap.micros;
    next.protected = protected;
    next.allocations = allocations;
    refresh(store.state(), &mut next, None)?;
    let mutation = put(
        Collection::Ledger,
        next.scope.task.to_string(),
        &next.scope,
        next.revision,
        Some(expected),
        &next,
    )?;
    let event = event(
        actor,
        scope,
        EventKind::AccountingResolved,
        serde_json::json!({"schema_version":1,"ledger":next,"reason":reason}),
        vec![],
    );
    store
        .transact(transaction(
            store.state(),
            TransactionId::new(),
            vec![mutation],
            event,
        ))
        .await?;
    Ok(next)
}

/// Calculate against one snapshot. The returned transaction still contains all
/// expected revisions; a losing race must recalculate, never reuse its allow.
pub fn prepare_admission(
    state: &State,
    input: &Admission,
    actor: &Actor,
) -> Result<(Transaction, Attempt)> {
    let task = task(state, &input.scope)?;
    running(state, &task)?;
    if task.steering != input.steering {
        return Err(Error::Conflict("steering changed"));
    }
    validate_quote(&input.quote, input.now)?;
    let mut root = ledger(state, &input.scope)?;
    if root.revision != input.expected_ledger || root.policy != input.policy {
        return Err(Error::Conflict("ledger or policy changed"));
    }
    if root.currency != input.quote.amount.currency {
        return Err(Error::Currency);
    }
    let request: ArtifactDescriptor = state
        .record(
            Collection::Artifact,
            input.request.as_str(),
            &input.scope.workspace,
        )?
        .decode()?;
    if request.spec.scope != input.scope
        || request.state != CaptureState::Complete
        || request.sha256 != input.request_digest
    {
        return Err(Error::Conflict("captured request differs"));
    }
    if input
        .previous
        .as_ref()
        .is_some_and(|id| id == &input.attempt)
    {
        return Err(Error::Conflict("retry identity"));
    }
    if let Some(previous) = &input.previous {
        let prior = attempt(state, previous, &input.scope.workspace)?;
        if prior.scope != input.scope
            || prior.root != task.root
            || matches!(
                prior.phase,
                ReservationState::Created | ReservationState::Submitted
            )
        {
            return Err(Error::Conflict(
                "retry predecessor is not reconciled or uncertain",
            ));
        }
    }
    let amount = input.quote.amount.micros;
    let draw = if input.draw_protected {
        if input.role != RequestRole::Verification || root.protected < amount {
            return Err(Error::Exhausted("protected verification reserve"));
        }
        amount
    } else {
        Micros::ZERO
    };
    let protected = Micros::new(root.protected.get() - draw.get());
    if sum([
        root.settled,
        root.active,
        root.unresolved,
        protected,
        amount,
    ])? > root.cap
    {
        return Err(Error::Exhausted("root cap"));
    }
    for (child, cap) in &root.allocations {
        if !descends(state, &input.scope.task, child, &input.scope.workspace)? {
            continue;
        }
        let mut committed = Vec::new();
        for record in state.records.values().filter(|r| {
            r.collection == Collection::Reservation && r.workspace == input.scope.workspace
        }) {
            let row: Reservation = record.decode()?;
            if row.root == task.root
                && descends(state, &row.scope.task, child, &input.scope.workspace)?
            {
                committed.push(sum([row.charged, row.liability])?);
            }
        }
        if add(sum(committed)?, amount)? > *cap {
            return Err(Error::Exhausted("child allocation"));
        }
    }
    let day = day(
        input.now,
        root.daily.as_ref().map_or(0, |d| d.utc_offset_minutes),
    )?;
    if let Some(daily) = &root.daily {
        let mut today = Vec::new();
        for record in state.records.values().filter(|r| {
            r.collection == Collection::Reservation && r.workspace == input.scope.workspace
        }) {
            let row: Reservation = record.decode()?;
            if row.root == task.root {
                today.push(if row.day == day {
                    sum([row.charged, row.liability])?
                } else {
                    row.liability
                });
            }
        }
        if sum([sum(today)?, amount, protected])? > daily.cap {
            return Err(Error::Exhausted("local root daily cap"));
        }
    }
    let reservation = Reservation {
        schema_version: 1,
        id: input.reservation.clone(),
        scope: input.scope.clone(),
        root: task.root.clone(),
        attempt: input.attempt.clone(),
        revision: Revision::ZERO,
        phase: ReservationState::Created,
        amount: input.quote.amount.clone(),
        charged: Micros::ZERO,
        liability: amount,
        protected_draw: draw,
        protected_returned: Micros::ZERO,
        day,
        role: input.role,
    };
    let attempt = Attempt {
        schema_version: 1,
        id: input.attempt.clone(),
        scope: input.scope.clone(),
        root: task.root,
        reservation: input.reservation.clone(),
        revision: Revision::ZERO,
        phase: ReservationState::Created,
        role: input.role,
        agent: input.agent.clone(),
        previous: input.previous.clone(),
        request: input.request.clone(),
        request_digest: input.request_digest.clone(),
        admission_digest: digest_bytes(&canonical_bytes(input)?),
        steering: input.steering,
        quote: input.quote.clone(),
        admitted_policy: input.policy,
        send_intent: None,
        observation_mode: None,
        usage_watermark: Units::ZERO,
        charged: Micros::ZERO,
        uncertain: None,
        provider_request: None,
    };
    let expected = root.revision;
    root.revision = root.revision.next()?;
    root.protected = protected;
    refresh(state, &mut root, Some(&reservation))?;
    let mutations = vec![
        put(
            Collection::Ledger,
            root.scope.task.to_string(),
            &root.scope,
            root.revision,
            Some(expected),
            &root,
        )?,
        put(
            Collection::Reservation,
            reservation.id.to_string(),
            &reservation.scope,
            reservation.revision,
            None,
            &reservation,
        )?,
        put(
            Collection::Attempt,
            attempt.id.to_string(),
            &attempt.scope,
            attempt.revision,
            None,
            &attempt,
        )?,
    ];
    let event = event(
        actor,
        &input.scope,
        EventKind::ReservationCreated,
        serde_json::json!({"schema_version":1,"attempt":attempt,"reservation":reservation,"ledger":root}),
        vec![input.request.clone()],
    );
    Ok((
        transaction(state, input.transaction.clone(), mutations, event),
        attempt,
    ))
}
pub async fn reserve<S: CanonicalStore>(
    store: &mut S,
    input: Admission,
    actor: &Actor,
) -> Result<Attempt> {
    if let Some(record) = store
        .state()
        .records
        .get(&key(Collection::Attempt, input.attempt.as_str()))
    {
        let existing: Attempt = record.decode()?;
        if existing.scope != input.scope
            || existing.admission_digest != digest_bytes(&canonical_bytes(&input)?)
        {
            return Err(Error::Conflict("attempt ID reused"));
        }
        return Ok(existing);
    }
    let (transaction, attempt) = prepare_admission(store.state(), &input, actor)?;
    store.transact(transaction).await?;
    Ok(attempt)
}
pub fn prepare_captured_admission(
    state: &State,
    input: &Admission,
    captured: &ArtifactDescriptor,
    actor: &Actor,
) -> Result<(Transaction, Attempt)> {
    if captured.spec.scope != input.scope
        || captured.spec.id != input.request
        || captured.sha256 != input.request_digest
        || captured.state != CaptureState::Complete
    {
        return Err(Error::Conflict("request capture scope or digest"));
    }
    let key = key(Collection::Artifact, captured.spec.id.as_str());
    let mut staged = state.clone();
    let existing = state.records.get(&key);
    if let Some(record) = existing {
        if record.decode::<ArtifactDescriptor>()? != *captured {
            return Err(Error::Conflict("capture already differs"));
        }
    }
    let record = Record::typed(
        Collection::Artifact,
        captured.spec.id.to_string(),
        input.scope.workspace.clone(),
        Revision::ZERO,
        captured,
    )?;
    if existing.is_none() {
        staged.records.insert(key, record.clone());
    }
    let (mut transaction, attempt) = prepare_admission(&staged, input, actor)?;
    if existing.is_none() {
        transaction.mutations.insert(
            0,
            Mutation::Put {
                expected: None,
                record,
            },
        );
    }
    Ok((transaction, attempt))
}
pub async fn reserve_captured<S: CanonicalStore>(
    store: &mut S,
    input: Admission,
    captured: ArtifactDescriptor,
    actor: &Actor,
) -> Result<Attempt> {
    if store
        .state()
        .records
        .contains_key(&key(Collection::Attempt, input.attempt.as_str()))
    {
        return reserve(store, input, actor).await;
    }
    let (transaction, attempt) =
        prepare_captured_admission(store.state(), &input, &captured, actor)?;
    store.transact(transaction).await?;
    Ok(attempt)
}
pub async fn record_local_resources<S: CanonicalStore>(
    store: &mut S,
    resources: LocalResources,
    actor: &Actor,
) -> Result<()> {
    if let Some(existing) = store
        .state()
        .records
        .get(&key(Collection::LocalResources, resources.id.as_str()))
    {
        if existing.decode::<LocalResources>()? != resources {
            return Err(Error::Conflict("resource observation ID reused"));
        }
        return Ok(());
    }
    task(store.state(), &resources.scope)?;
    let mutation = put(
        Collection::LocalResources,
        resources.id.to_string(),
        &resources.scope,
        Revision::ZERO,
        None,
        &resources,
    )?;
    let event = event(
        actor,
        &resources.scope,
        EventKind::LocalResourcesObserved,
        serde_json::json!({"schema_version":1,"local_resources":resources}),
        vec![],
    );
    store
        .transact(transaction(
            store.state(),
            TransactionId::new(),
            vec![mutation],
            event,
        ))
        .await?;
    Ok(())
}
async fn persist<S: CanonicalStore>(
    store: &mut S,
    old: &Attempt,
    mut next: Attempt,
    mut reservation: Reservation,
    actor: &Actor,
    kind: EventKind,
    extra: Vec<Mutation>,
    artifacts: Vec<ArtifactId>,
    send_event: Option<EventId>,
) -> Result<Receipt> {
    let mut root = ledger(store.state(), &old.scope)?;
    let expected = root.revision;
    root.revision = root.revision.next()?;
    let reservation_revision = reservation.revision;
    next.revision = old.revision.next()?;
    reservation.revision = reservation.revision.next()?;
    reservation.phase = next.phase;
    reservation.charged = next.charged;
    reservation.liability = if matches!(
        next.phase,
        ReservationState::Created
            | ReservationState::Submitted
            | ReservationState::ReconciliationPending
    ) {
        Micros::new(
            reservation
                .amount
                .micros
                .get()
                .saturating_sub(next.charged.get()),
        )
    } else {
        Micros::ZERO
    };
    if next.phase == ReservationState::Released && reservation.protected_returned == Micros::ZERO {
        root.protected = add(root.protected, reservation.protected_draw)?;
        reservation.protected_returned = reservation.protected_draw;
    }
    refresh(store.state(), &mut root, Some(&reservation))?;
    let mut mutations = vec![
        put(
            Collection::Ledger,
            root.scope.task.to_string(),
            &root.scope,
            root.revision,
            Some(expected),
            &root,
        )?,
        put(
            Collection::Attempt,
            next.id.to_string(),
            &next.scope,
            next.revision,
            Some(old.revision),
            &next,
        )?,
        put(
            Collection::Reservation,
            reservation.id.to_string(),
            &reservation.scope,
            reservation.revision,
            Some(reservation_revision),
            &reservation,
        )?,
    ];
    mutations.extend(extra);
    let mut event = event(
        actor,
        &old.scope,
        kind,
        serde_json::json!({"schema_version":1,"attempt":next,"reservation":reservation,"ledger":root}),
        artifacts,
    );
    if let Some(id) = send_event {
        event.id = id;
    }
    Ok(store
        .transact(transaction(
            store.state(),
            TransactionId::new(),
            mutations,
            event,
        ))
        .await?)
}
pub async fn submit<S: CanonicalStore>(
    store: &mut S,
    id: &AttemptId,
    scope: &Scope,
    expected: Revision,
    actor: &Actor,
) -> Result<SendPermit> {
    let old = attempt(store.state(), id, &scope.workspace)?;
    if &old.scope != scope || old.revision != expected || old.phase != ReservationState::Created {
        return Err(Error::Conflict("attempt already submitted or changed"));
    }
    let task = task(store.state(), scope)?;
    running(store.state(), &task)?;
    let root = ledger(store.state(), scope)?;
    if old.steering != task.steering || old.admitted_policy != root.policy || root.overrun {
        return Err(Error::Denied("authority or budget changed before send"));
    }
    validate_quote(&old.quote, actor.now)?;
    let mut next = old.clone();
    next.phase = ReservationState::Submitted;
    let send = EventId::new();
    next.send_intent = Some(send.clone());
    let reserved = reservation(store.state(), &old)?;
    let receipt = persist(
        store,
        &old,
        next,
        reserved,
        actor,
        EventKind::AttemptSubmitted,
        vec![],
        vec![old.request.clone()],
        Some(send),
    )
    .await?;
    Ok(SendPermit {
        attempt: id.clone(),
        request_digest: old.request_digest,
        receipt,
    })
}
pub async fn hold_uncertain<S: CanonicalStore>(
    store: &mut S,
    id: &AttemptId,
    scope: &Scope,
    actor: &Actor,
    reason: &str,
) -> Result<()> {
    let old = attempt(store.state(), id, &scope.workspace)?;
    if &old.scope != scope || reason.trim().is_empty() {
        return Err(Error::Conflict("uncertain outcome scope/reason"));
    }
    if old.phase == ReservationState::ReconciliationPending {
        return Ok(());
    }
    if old.phase != ReservationState::Submitted {
        return Err(Error::Conflict("only submitted work can become uncertain"));
    }
    let mut next = old.clone();
    next.phase = ReservationState::ReconciliationPending;
    next.uncertain = Some(reason.into());
    let reserved = reservation(store.state(), &old)?;
    persist(
        store,
        &old,
        next,
        reserved,
        actor,
        EventKind::LiabilityRetained,
        vec![],
        vec![],
        None,
    )
    .await?;
    Ok(())
}
pub async fn release_before_send<S: CanonicalStore>(
    store: &mut S,
    id: &AttemptId,
    scope: &Scope,
    actor: &Actor,
) -> Result<()> {
    let old = attempt(store.state(), id, &scope.workspace)?;
    if &old.scope != scope {
        return Err(Error::Conflict("release scope"));
    }
    if old.phase == ReservationState::Released {
        return Ok(());
    }
    if old.phase != ReservationState::Created || old.send_intent.is_some() {
        return Err(Error::Denied("no positive no-send state; retain liability"));
    }
    let mut next = old.clone();
    next.phase = ReservationState::Released;
    let reserved = reservation(store.state(), &old)?;
    persist(
        store,
        &old,
        next,
        reserved,
        actor,
        EventKind::ReservationReleased,
        vec![],
        vec![],
        None,
    )
    .await?;
    Ok(())
}
pub async fn observe<S: CanonicalStore>(
    store: &mut S,
    observation: UsageObservation,
    actor: &Actor,
) -> Result<Settlement> {
    if let Some(record) = store
        .state()
        .records
        .get(&key(Collection::Settlement, observation.id.as_str()))
    {
        let existing: Settlement = record.decode()?;
        if existing.observation != observation {
            return Err(Error::Conflict("usage observation ID reused"));
        }
        return Ok(existing);
    }
    let old = attempt(
        store.state(),
        &observation.attempt,
        &observation.scope.workspace,
    )?;
    if old.scope != observation.scope
        || matches!(
            old.phase,
            ReservationState::Created | ReservationState::Released
        )
        || observation.provider_request.trim().is_empty()
    {
        return Err(Error::Conflict("usage lacks submitted attempt"));
    }
    if observation.amount.currency != old.quote.amount.currency {
        return Err(Error::Currency);
    }
    if old
        .provider_request
        .as_ref()
        .is_some_and(|id| id != &observation.provider_request)
    {
        return Err(Error::Conflict("provider request identity changed"));
    }
    let mut next = old.clone();
    let mode = match observation.mode {
        UsageMode::Cumulative { .. } => "cumulative",
        UsageMode::Incremental { .. } => "incremental",
    };
    if old.observation_mode.as_ref().is_some_and(|m| m != mode) {
        return Err(Error::Conflict("overlapping cumulative/incremental modes"));
    }
    let mut applied = true;
    let amount = match observation.mode {
        UsageMode::Cumulative { version } => {
            if version == Units::ZERO {
                return Err(Error::Conflict("usage version starts at one"));
            }
            if version <= old.usage_watermark {
                applied = false;
                old.charged
            } else {
                next.usage_watermark = version;
                observation.amount.micros
            }
        }
        UsageMode::Incremental { start, end } => {
            if start != old.usage_watermark || end <= start {
                return Err(Error::Conflict(
                    "incremental coverage overlaps or has a gap",
                ));
            }
            next.usage_watermark = end;
            add(old.charged, observation.amount.micros)?
        }
    };
    if amount < old.charged && observation.correction.is_none() {
        return Err(Error::Conflict(
            "negative cumulative correction needs explicit provenance",
        ));
    }
    if let Some(resolution) = &observation.correction {
        let root = ledger(store.state(), &old.scope)?;
        if resolution.actor != actor.id
            || resolution.policy != root.policy
            || resolution.reason.trim().is_empty()
        {
            return Err(Error::Conflict("resolution actor/policy/reason"));
        }
    }
    let (direction, adjustment) = if amount > old.charged {
        (
            AdjustmentDirection::Debit,
            Micros::new(amount.get() - old.charged.get()),
        )
    } else if amount < old.charged {
        (
            AdjustmentDirection::Credit,
            Micros::new(old.charged.get() - amount.get()),
        )
    } else {
        (AdjustmentDirection::None, Micros::ZERO)
    };
    let settlement = Settlement {
        schema_version: 1,
        id: observation.id.clone(),
        scope: old.scope.clone(),
        attempt: old.id.clone(),
        observation: observation.clone(),
        applied,
        direction,
        adjustment,
        total: amount,
        normalization_version: 1,
    };
    next.observation_mode = Some(mode.into());
    next.provider_request = Some(observation.provider_request.clone());
    next.charged = amount;
    if applied && observation.final_usage {
        next.phase = ReservationState::Settled;
        next.uncertain = None;
        if let Some(resolution) = &observation.correction {
            if !resolution.remaining_uncertainty.trim().is_empty() {
                next.phase = ReservationState::ExplicitlyResolved;
                next.uncertain = Some(resolution.remaining_uncertainty.clone());
            }
        }
    }
    if applied
        && !observation.final_usage
        && matches!(
            old.phase,
            ReservationState::Settled | ReservationState::ExplicitlyResolved
        )
    {
        next.phase = ReservationState::ReconciliationPending;
        next.uncertain = Some("new usage observation is not final".into());
    }
    let mutation = put(
        Collection::Settlement,
        settlement.id.to_string(),
        &settlement.scope,
        Revision::ZERO,
        None,
        &settlement,
    )?;
    let reserved = reservation(store.state(), &old)?;
    persist(
        store,
        &old,
        next,
        reserved,
        actor,
        EventKind::UsageReconciled,
        vec![mutation],
        vec![observation.raw],
        None,
    )
    .await?;
    Ok(settlement)
}
