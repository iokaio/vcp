// SPDX-License-Identifier: Apache-2.0
//! Caller-bound atomic optimizer publications. The authenticated host retains
//! its controller guard across these calls; serialized inputs cannot mint it.
use super::*;
use vcp_domain::workspace::Trust;
use vcp_protocol::command::{CommandReceipt, CommandResult};
use vcp_store::contract::ReceiptInput;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Access,
    Invalid,
    CommandConflict,
    Stale,
    Limit,
    Unavailable,
    OutcomeUnknown,
    Cancelled,
}
pub type PublicResult<T> = std::result::Result<T, Error>;
pub type Check<'a> = dyn Fn() -> PublicResult<()> + 'a;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub actor: ActorId,
    pub id: CommandId,
    pub digest: String,
    pub expected_revision: Revision,
    pub expected_binding_revision: Revision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Session,
    Workspace,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Report {
        report: String,
        coverage: Coverage,
        source_tasks: Option<BTreeSet<TaskId>>,
    },
    Policy {
        value: ApplyReceipt,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    command: Command,
    outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub receipt: CommandReceipt,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadReport {
    pub report: OptimizationReport,
    pub coverage: Coverage,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackPreview {
    pub base: Revision,
    pub target: Revision,
    pub prior: Policy,
    pub persisted: Policy,
    pub effective: Policy,
    pub ceilings_digest: String,
    pub interview: Interview,
}
fn binding_id(workspace: &WorkspaceId, command: &CommandId) -> String {
    format!(
        "optimizer-public-{}",
        digest_bytes(format!("{workspace}:{command}").as_bytes())
    )
}
fn scope(
    store: &Store,
    access: &Access,
    session: &SessionId,
    write: bool,
) -> PublicResult<Workspace> {
    authorize(store, access, write).map_err(|_| Error::Access)?;
    let expected_session = session;
    let session: Session = store
        .state()
        .record(Collection::Session, session.as_str(), &access.workspace)
        .map_err(|_| Error::Access)?
        .decode()
        .map_err(|_| Error::Unavailable)?;
    if session.workspace != access.workspace || session.id != *expected_session {
        return Err(Error::Access);
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| Error::Access)?
        .decode()
        .map_err(|_| Error::Unavailable)?;
    if workspace.id != access.workspace {
        return Err(Error::Access);
    }
    Ok(workspace)
}
fn admission(
    store: &Store,
    access: &Access,
    command: &Command,
    fresh: bool,
    trusted: bool,
) -> PublicResult<()> {
    if command.workspace != access.workspace || command.actor != access.actor {
        return Err(Error::Access);
    }
    if command.digest.len() != 64
        || !command
            .digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(Error::Invalid);
    }
    let workspace = scope(store, access, &command.session, true)?;
    if fresh
        && (workspace.revision != command.expected_revision
            || workspace.binding.revision != command.expected_binding_revision)
    {
        return Err(Error::Stale);
    }
    if fresh && trusted && workspace.trust != Trust::Trusted {
        return Err(Error::Access);
    }
    Ok(())
}
/// Replay is checked before revision or ephemeral preview checks. The caller
/// must still hold current authenticated write/controller authority.
pub fn replay(store: &Store, access: &Access, command: &Command) -> PublicResult<Option<Commit>> {
    admission(store, access, command, false, false)?;
    let Some(receipt) = store
        .state()
        .command(&access.workspace, &command.id, &command.digest)
        .map_err(|_| Error::CommandConflict)?
    else {
        return Ok(None);
    };
    let binding: Binding = read(store, access, &binding_id(&access.workspace, &command.id))
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::CommandConflict)?;
    if binding.command != *command {
        return Err(Error::CommandConflict);
    }
    if !store.state().events.iter().any(|event| {
        event.watermark == receipt.watermark
            && event.event.session == command.session
            && event.event.workspace == command.workspace
            && event.event.actor == command.actor
            && event.event.correlation == command.id
    }) {
        return Err(Error::Unavailable);
    }
    let transaction = store
        .state()
        .transactions
        .get(&receipt.transaction)
        .ok_or(Error::Unavailable)?;
    if transaction.command.as_ref() != Some(&receipt) {
        return Err(Error::Unavailable);
    }
    Ok(Some(Commit {
        receipt,
        outcome: binding.outcome,
    }))
}
pub(crate) async fn commit_report(
    store: &mut Store,
    access: &Access,
    command: &Command,
    coverage: Coverage,
    report: &OptimizationReport,
    mutations: Vec<Mutation>,
    _events: Vec<EventInput>,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    let outcome = Outcome::Report {
        report: report.id.clone(),
        coverage,
        source_tasks: report.source_tasks.clone(),
    };
    publish(
        store,
        access,
        command,
        outcome,
        Revision::ZERO,
        mutations,
        vec![],
        now,
        false,
        check,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn publish(
    store: &mut Store,
    access: &Access,
    command: &Command,
    outcome: Outcome,
    revision: Revision,
    mut mutations: Vec<Mutation>,
    mut events: Vec<EventInput>,
    now: Timestamp,
    trusted: bool,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    check()?;
    admission(store, access, command, true, trusted)?;
    let binding = Binding {
        command: command.clone(),
        outcome: outcome.clone(),
    };
    let mut record = row(
        access,
        binding_id(&access.workspace, &command.id),
        Revision::ZERO,
        &binding,
    )
    .map_err(|_| Error::Unavailable)?;
    match &outcome {
        Outcome::Report { report, .. } => {
            record
                .references
                .insert(key(Collection::Projection, report));
        }
        Outcome::Policy { value } => {
            record.references.insert(key(
                Collection::Projection,
                &revision_name("policy", access, value.published.revision),
            ));
        }
    }
    mutations.push(Mutation::Put {
        record,
        expected: None,
    });
    // Source descriptors/manifests retain their original scopes. Public receipt
    // events must all belong to the authenticated caller's session; attaching a
    // foreign source artifact here would violate the canonical event contract.
    events.push(EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: command.session.clone(),
        task: None,
        actor: access.actor.clone(),
        correlation: command.id.clone(),
        causation: None,
        timestamp: now,
        kind: EventKind::Diagnostic,
        artifacts: vec![],
        data: serde_json::json!({"version":1,"optimizer_publication":true}),
        metadata: None,
    });
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations,
            events,
            command: Some(ReceiptInput {
                command: command.id.clone(),
                workspace: access.workspace.clone(),
                session: command.session.clone(),
                digest: command.digest.clone(),
                result: CommandResult::Accepted { revision },
            }),
        })
        .await
        .map_err(|_| Error::OutcomeUnknown)?;
    replay(store, access, command)
        .map_err(|_| Error::OutcomeUnknown)?
        .ok_or(Error::OutcomeUnknown)
}
fn coverage(
    store: &Store,
    access: &Access,
    session: &SessionId,
    value: Coverage,
    check: &Check<'_>,
) -> PublicResult<()> {
    match (value, &access.tasks) {
        (Coverage::Workspace, None) => Ok(()),
        (Coverage::Session, Some(tasks)) => {
            for id in tasks {
                check()?;
                let task: Task = store
                    .state()
                    .record(Collection::Task, id.as_str(), &access.workspace)
                    .map_err(|_| Error::Access)?
                    .decode()
                    .map_err(|_| Error::Unavailable)?;
                if task.scope.session != *session
                    || task.scope.workspace != access.workspace
                    || task.redaction.is_some()
                {
                    return Err(Error::Access);
                }
            }
            Ok(())
        }
        _ => Err(Error::Access),
    }
}
pub async fn capture(
    store: &mut Store,
    access: &Access,
    command: &Command,
    value: Coverage,
    window: HistoryWindow,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    check()?;
    if let Some(receipt) = replay(store, access, command)? {
        return Ok(receipt);
    }
    admission(store, access, command, true, false)?;
    coverage(store, access, &command.session, value, check)?;
    let result = report_with_check(store, access, window, &|| {
        check().map_err(|_| "optimizer interrupted".into())
    });
    check()?;
    let report = result.map_err(|_| Error::Unavailable)?;
    forecast_reports::save_public(store, access, report, command, value, now, check, false).await
}
/// Qualification-only interruption leaves finalized bytes unreferenced and no
/// command acceptance. The ordinary path must safely accept an exact retry.
#[cfg(feature = "qualification")]
pub async fn qualification_capture_after_spool(
    store: &mut Store,
    access: &Access,
    command: &Command,
    value: Coverage,
    window: HistoryWindow,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    check()?;
    if let Some(receipt) = replay(store, access, command)? {
        return Ok(receipt);
    }
    admission(store, access, command, true, false)?;
    coverage(store, access, &command.session, value, check)?;
    let report = report_with_check(store, access, window, &|| {
        check().map_err(|_| "optimizer interrupted".into())
    })
    .map_err(|_| Error::Unavailable)?;
    forecast_reports::save_public(store, access, report, command, value, now, check, true).await
}
/// Binding identities are public capture command IDs, never internal report IDs.
pub fn read_report(
    store: &Store,
    access: &Access,
    session: &SessionId,
    capture: &CommandId,
    check: &Check<'_>,
) -> PublicResult<ReadReport> {
    check()?;
    scope(store, access, session, false)?;
    let binding: Binding = read(store, access, &binding_id(&access.workspace, capture))
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    if binding.command.session != *session
        || binding.command.workspace != access.workspace
        || binding.command.actor != access.actor
    {
        return Err(Error::Access);
    }
    let Outcome::Report {
        report,
        source_tasks,
        coverage,
    } = binding.outcome
    else {
        return Err(Error::Invalid);
    };
    if coverage == Coverage::Workspace && access.tasks.is_some() {
        return Err(Error::Access);
    }
    if source_tasks
        .as_ref()
        .is_some_and(|tasks| tasks.iter().any(|task| !access.allows_task(task)))
    {
        return Err(Error::Access);
    }
    let result = load_report_with_check(store, access, &report, &|| {
        check().map_err(|_| "optimizer interrupted".into())
    });
    check()?;
    let report = result.map_err(|_| Error::Unavailable)?;
    if report.source_tasks != source_tasks {
        return Err(Error::Unavailable);
    }
    for id in source_tasks
        .iter()
        .flat_map(|tasks| tasks.iter())
        .chain(report.tasks.iter())
    {
        check()?;
        let task: Task = store
            .state()
            .record(Collection::Task, id.as_str(), &access.workspace)
            .map_err(|_| Error::Access)?
            .decode()
            .map_err(|_| Error::Unavailable)?;
        if task.scope.workspace != access.workspace
            || task.scope.task != *id
            || task.redaction.is_some()
            || (coverage == Coverage::Session && task.scope.session != *session)
        {
            return Err(Error::Access);
        }
    }
    check()?;
    Ok(ReadReport {
        report,
        coverage,
        command: binding.command,
    })
}
pub fn rollback_preview(
    store: &Store,
    access: &Access,
    expected: Revision,
    target: Revision,
    ceilings: &Policy,
    check: &Check<'_>,
) -> PublicResult<RollbackPreview> {
    check()?;
    global_write(store, access).map_err(|_| Error::Access)?;
    let current = current_policy(store, access)
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    if current.revision != expected || target >= expected {
        return Err(Error::Stale);
    }
    let prior: Published<Policy> = read(store, access, &revision_name("policy", access, target))
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    let mut restored = prior.value;
    restored.parent = Some(current.value.id.clone());
    restored = restored.seal().map_err(|_| Error::Invalid)?;
    let value = RollbackPreview {
        base: expected,
        target,
        prior: current.value,
        persisted: restored.clone(),
        effective: effective_policy(restored, ceilings).map_err(|_| Error::Invalid)?,
        ceilings_digest: ceilings.digest().map_err(|_| Error::Invalid)?,
        interview: interview(store, access).map_err(|_| Error::Unavailable)?,
    };
    check()?;
    Ok(value)
}
pub async fn apply(
    store: &mut Store,
    access: &Access,
    command: &Command,
    proposal: &Preview,
    ceilings: &Policy,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    check()?;
    if let Some(receipt) = replay(store, access, command)? {
        return Ok(receipt);
    }
    admission(store, access, command, true, true)?;
    global_write(store, access).map_err(|_| Error::Access)?;
    if proposal.selected.is_empty() {
        return Err(Error::Invalid);
    }
    let checked = preview_with_check(
        store,
        access,
        &proposal.report,
        proposal.selected.clone(),
        ceilings,
        &|| check().map_err(|_| "optimizer interrupted".into()),
    );
    check()?;
    let current = checked.map_err(|_| Error::Stale)?;
    if current != *proposal {
        return Err(Error::Stale);
    }
    publish_policy(
        store,
        access,
        command,
        proposal.base,
        proposal.persisted.clone(),
        proposal.effective.clone(),
        digest_bytes(&canonical_bytes(proposal).map_err(|_| Error::Invalid)?),
        now,
        check,
    )
    .await
}
pub async fn rollback(
    store: &mut Store,
    access: &Access,
    command: &Command,
    proposal: &RollbackPreview,
    ceilings: &Policy,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    check()?;
    if let Some(receipt) = replay(store, access, command)? {
        return Ok(receipt);
    }
    admission(store, access, command, true, true)?;
    if rollback_preview(
        store,
        access,
        proposal.base,
        proposal.target,
        ceilings,
        check,
    )? != *proposal
    {
        return Err(Error::Stale);
    }
    publish_policy(
        store,
        access,
        command,
        proposal.base,
        proposal.persisted.clone(),
        proposal.effective.clone(),
        digest_bytes(&canonical_bytes(proposal).map_err(|_| Error::Invalid)?),
        now,
        check,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn publish_policy(
    store: &mut Store,
    access: &Access,
    command: &Command,
    base: Revision,
    persisted: Policy,
    effective: Policy,
    digest: String,
    now: Timestamp,
    check: &Check<'_>,
) -> PublicResult<Commit> {
    let (published, mutations) = publication(store, access, "policy", Some(base), persisted, now)
        .map_err(|_| Error::Stale)?;
    let revision = published.revision;
    let outcome = Outcome::Policy {
        value: ApplyReceipt {
            command: command.id.clone(),
            digest,
            published,
            effective,
        },
    };
    publish(
        store,
        access,
        command,
        outcome,
        revision,
        mutations,
        vec![],
        now,
        true,
        check,
    )
    .await
}
