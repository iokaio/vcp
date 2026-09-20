// SPDX-License-Identifier: Apache-2.0
//! Explicit saved authority and notification cadence. Reads never apply policy.
use crate::{
    access::{self, Access},
    retention::{self, Action, PrunePreview, PruneReceipt},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use vcp_domain::{retention_selector::Selector, *};
use vcp_protocol::event::{EventInput, EventKind};
use vcp_store::{
    contract::{key, CanonicalStore, Collection, Mutation, Record, Transaction},
    Store,
};

pub const DAY_MS: u64 = 86_400_000;
const POLICY: &str = "vcp_retention_policy_v1";
const NOTICE: &str = "vcp_retention_notice_v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Automatic {
    pub selector: Selector,
    pub action: Action,
    pub cadence_days: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: u32,
    pub document_type: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub actor: ActorId,
    pub authority: AuthorityRevision,
    /// Explicit fixed-duration cadence; not a local calendar/DST calculation.
    pub notice_repeat_days: u32,
    /// None is notification-only. No implicit default purge policy exists.
    pub automatic: Option<Automatic>,
}
impl Policy {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.document_type != POLICY
            || !(1..=365).contains(&self.notice_repeat_days)
        {
            return Err(Error::Invalid("retention policy version/cadence".into()));
        }
        if let Some(mode) = &self.automatic {
            if !(1..=365).contains(&mode.cadence_days) {
                return Err(Error::Invalid("automatic retention cadence".into()));
            }
            mode.selector.clone().normalized()?;
        }
        Ok(())
    }
}
fn id(kind: &str, workspace: &WorkspaceId) -> String {
    format!("{kind}-{workspace}")
}
fn authorize(store: &Store, access: &Access, write: bool) -> Result<()> {
    access::authorize(store.state(), access, write)?;
    if access.tasks.is_some() {
        return Err(Error::Access);
    }
    Ok(())
}
pub fn show(store: &Store, access: &Access) -> Result<Policy> {
    authorize(store, access, false)?;
    let value = match store.state().records.get(&key(
        Collection::Projection,
        &id("retention-policy", &access.workspace),
    )) {
        Some(row) => {
            if row.workspace != access.workspace {
                return Err(Error::Access);
            }
            row.decode()?
        }
        None => Policy {
            schema_version: 1,
            document_type: POLICY.into(),
            workspace: access.workspace.clone(),
            revision: Revision::ZERO,
            actor: access.actor.clone(),
            authority: access.authority,
            notice_repeat_days: 7,
            automatic: None,
        },
    };
    value.validate()?;
    Ok(value)
}
async fn put<T: Serialize>(
    store: &mut Store,
    access: &Access,
    name: String,
    expected: Option<Revision>,
    revision: Revision,
    value: &T,
    now: Timestamp,
) -> Result<()> {
    authorize(store, access, true)?;
    let session = store
        .state()
        .records
        .values()
        .find(|r| r.workspace == access.workspace && r.collection == Collection::Session)
        .ok_or(Error::Conflict(
            "retention policy requires a retained session",
        ))?
        .decode::<vcp_domain::workspace::Session>()?;
    let record = Record::typed(
        Collection::Projection,
        name,
        access.workspace.clone(),
        revision,
        value,
    )?;
    let event = EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: session.id,
        task: None,
        actor: access.actor.clone(),
        correlation: CommandId::new(),
        causation: None,
        timestamp: now,
        kind: EventKind::RetentionChanged,
        artifacts: vec![],
        data: serde_json::json!({"version":1,"facts":[record]}),
        metadata: None,
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put { record, expected }],
            events: vec![event],
            command: None,
        })
        .await?;
    Ok(())
}
/// The caller's explicit set operation grants automatic policy authority. Imported
/// serialized Policy values cannot preserve another actor's authority provenance.
pub async fn set(
    store: &mut Store,
    access: &Access,
    expected: Option<Revision>,
    notice_repeat_days: u32,
    automatic: Option<Automatic>,
    now: Timestamp,
) -> Result<Policy> {
    authorize(store, access, true)?;
    let name = id("retention-policy", &access.workspace);
    let current = store
        .state()
        .records
        .get(&key(Collection::Projection, &name))
        .map(|r| r.revision);
    if current != expected {
        return Err(Error::Conflict("retention policy revision changed"));
    }
    let revision = expected.map_or(Ok(Revision::ZERO), |r| r.next())?;
    let automatic = automatic
        .map(|mut mode| -> Result<_> {
            mode.selector = mode.selector.normalized()?;
            Ok(mode)
        })
        .transpose()?;
    let policy = Policy {
        schema_version: 1,
        document_type: POLICY.into(),
        workspace: access.workspace.clone(),
        revision,
        actor: access.actor.clone(),
        authority: access.authority,
        notice_repeat_days,
        automatic,
    };
    policy.validate()?;
    put(store, access, name, expected, revision, &policy, now).await?;
    Ok(policy)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeState {
    pub schema_version: u32,
    pub document_type: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub last_shown: Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aging {
    pub oldest: Option<Timestamp>,
    pub retained_bytes: u64,
    pub bytes_are_exact: bool,
    pub due: bool,
    pub preview_command: String,
    pub policy_command: String,
}
/// Excluded recall still counts as retained history. Maintenance events do not
/// create their own notice loop. Purged event payloads no longer count as history.
pub fn aging(store: &Store, access: &Access, now: Timestamp) -> Result<Aging> {
    let policy = show(store, access)?;
    let events: Vec<_> = store
        .state()
        .events
        .iter()
        .filter(|e| {
            e.event.workspace == access.workspace
                && e.redaction.is_none()
                && e.event.kind != EventKind::RetentionChanged
        })
        .collect();
    let oldest = events.iter().map(|e| e.event.timestamp).min();
    let mut bytes = events.iter().try_fold(0u64, |sum, event| -> Result<u64> {
        Ok(sum.saturating_add(serde_json::to_vec(event)?.len() as u64))
    })?;
    for row in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::Artifact)
    {
        let artifact: vcp_domain::artifact::ArtifactDescriptor = row.decode()?;
        for range in artifact.retained {
            bytes = bytes.saturating_add(range.end.get().saturating_sub(range.start.get()));
        }
    }
    let last = store
        .state()
        .records
        .get(&key(
            Collection::Projection,
            &id("retention-notice", &access.workspace),
        ))
        .map(|r| -> Result<NoticeState> {
            if r.workspace != access.workspace {
                return Err(Error::Access);
            }
            let value: NoticeState = r.decode()?;
            if value.document_type != NOTICE {
                return Err(Error::Invalid("retention notice type".into()));
            }
            Ok(value)
        })
        .transpose()?;
    let due = oldest.is_some_and(|old| now.get().saturating_sub(old.get()) > 30 * DAY_MS)
        && last.is_none_or(|last| {
            now.get().saturating_sub(last.last_shown.get())
                >= u64::from(policy.notice_repeat_days) * DAY_MS
        });
    Ok(Aging {
        oldest,
        retained_bytes: bytes,
        bytes_are_exact: false,
        due,
        preview_command: "history prune --preview".into(),
        policy_command: "retention show".into(),
    })
}
/// Invoke only after actually presenting the nonblocking notice. This stored
/// state is independent of content events and survives restart.
pub async fn acknowledge_notice(store: &mut Store, access: &Access, now: Timestamp) -> Result<()> {
    authorize(store, access, true)?;
    if !aging(store, access, now)?.due {
        return Ok(());
    }
    let name = id("retention-notice", &access.workspace);
    let expected = store
        .state()
        .records
        .get(&key(Collection::Projection, &name))
        .map(|r| r.revision);
    let revision = expected.map_or(Ok(Revision::ZERO), |r| r.next())?;
    put(
        store,
        access,
        name,
        expected,
        revision,
        &NoticeState {
            schema_version: 1,
            document_type: NOTICE.into(),
            workspace: access.workspace.clone(),
            revision,
            last_shown: now,
        },
        now,
    )
    .await
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queued {
    pub policy: Policy,
    pub preview: PrunePreview,
}
pub fn queue(store: &Store, access: &Access, now: Timestamp) -> Result<Option<Queued>> {
    let policy = show(store, access)?;
    let Some(mode) = &policy.automatic else {
        return Ok(None);
    };
    if policy.actor != access.actor || policy.authority != access.authority {
        return Err(Error::Access);
    }
    let latest = store
        .state()
        .records
        .values()
        .filter(|r| {
            r.workspace == access.workspace
                && r.value["document_type"] == "vcp_retention_policy_run_v1"
        })
        .filter(|r| r.value["policy_revision"] == serde_json::json!(policy.revision))
        .filter_map(|r| {
            serde_json::from_value::<Timestamp>(r.value["at"].clone())
                .ok()
                .map(|at| at.get())
        })
        .max();
    if latest.is_some_and(|at| now.get().saturating_sub(at) < u64::from(mode.cadence_days) * DAY_MS)
    {
        return Ok(None);
    }
    let preview = retention::preview(store, access, mode.selector.clone(), mode.action, now)?;
    Ok(Some(Queued { policy, preview }))
}
/// Recheck enabled policy and its granting authority immediately before applying
/// the original exact preview. Disable/edit revokes queued work; prior tombstones
/// remain canonical facts. The controller serializes this with dispatch.
pub async fn apply_queued(
    store: &mut Store,
    access: &Access,
    queued: &Queued,
    now: Timestamp,
) -> Result<PruneReceipt> {
    authorize(store, access, true)?;
    let current = show(store, access)?;
    if current != queued.policy
        || current.automatic.is_none()
        || current.actor != access.actor
        || current.authority != access.authority
    {
        return Err(Error::Conflict(
            "queued policy disabled or authority changed",
        ));
    }
    let mode = current.automatic.as_ref().ok_or(Error::Access)?;
    if queued.preview.selector != mode.selector || queued.preview.action != mode.action {
        return Err(Error::Access);
    }
    let receipt = retention::apply(store, access, &queued.preview, now).await?;
    let name = format!("policy-run-{}", receipt.id);
    if !store
        .state()
        .records
        .contains_key(&key(Collection::Projection, &name))
    {
        put(store,access,name,None,Revision::ZERO,&serde_json::json!({"schema_version":1,"document_type":"vcp_retention_policy_run_v1","policy_revision":current.revision,"at":now,"receipt":receipt.id}),now).await?;
    }
    Ok(receipt)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRun {
    pub schema_version: u32,
    pub document_type: String,
    pub policy_revision: Revision,
    pub at: Timestamp,
    pub receipt: Option<String>,
    #[serde(default = "applied_status")]
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub local_cleanup_complete: Option<bool>,
}
fn applied_status() -> String {
    "applied".into()
}
pub fn latest_run(store: &Store, access: &Access) -> Result<Option<PolicyRun>> {
    authorize(store, access, false)?;
    let mut latest: Option<PolicyRun> = None;
    for row in store.state().records.values().filter(|r| {
        r.workspace == access.workspace
            && r.collection == Collection::Projection
            && r.value["document_type"] == "vcp_retention_policy_run_v1"
    }) {
        let value: PolicyRun = row.decode()?;
        if value.schema_version != 1 {
            return Err(Error::Invalid("retention policy run version".into()));
        }
        if latest.as_ref().is_none_or(|prior| {
            (value.at, value.policy_revision) > (prior.at, prior.policy_revision)
        }) {
            latest = Some(value);
        }
    }
    Ok(latest)
}
/// Evaluate one saved policy at canonical owner startup after recovery. This is
/// never called by read-only inspection and never starts inference or children.
/// No daemon promises execution while the owner is closed. Blocked attempts are
/// durable and respect the same cadence until the policy changes.
pub async fn run_due(
    store: &mut Store,
    access: &Access,
    now: Timestamp,
) -> Result<Option<PolicyRun>> {
    authorize(store, access, true)?;
    let policy = show(store, access)?;
    let Some(mode) = &policy.automatic else {
        return Ok(None);
    };
    if latest_run(store, access)?.is_some_and(|run| {
        run.policy_revision == policy.revision
            && now.get().saturating_sub(run.at.get()) < u64::from(mode.cadence_days) * DAY_MS
    }) {
        return Ok(None);
    }
    let mut run = PolicyRun {
        schema_version: 1,
        document_type: "vcp_retention_policy_run_v1".into(),
        policy_revision: policy.revision,
        at: now,
        receipt: None,
        status: "blocked".into(),
        reason: None,
        local_cleanup_complete: None,
    };
    let mut name = format!("policy-blocked-{}", CommandId::new());
    let queued = match queue(store, access, now) {
        Ok(None) => return Ok(None),
        Ok(Some(queued)) => Some(queued),
        Err(Error::Access) => {
            run.reason = Some("saved_policy_authority_changed".into());
            None
        }
        Err(_) => {
            run.reason = Some("preview_unavailable".into());
            None
        }
    };
    if let Some(queued) = queued {
        if !queued.preview.protected.is_empty() {
            run.reason = Some("protected_dependencies_require_reconciliation".into());
        } else {
            match apply_queued(store, access, &queued, now).await {
                Ok(receipt) => {
                    name = format!("policy-run-{}", receipt.id);
                    run.receipt = Some(receipt.id.clone());
                    let cleaned = if receipt.preview.action == Action::Purge {
                        retention::cleanup(store, access, &receipt.id, now).await
                    } else {
                        Ok(receipt)
                    };
                    match cleaned {
                        Ok(receipt) => {
                            run.local_cleanup_complete = Some(receipt.local_cleanup_complete);
                            run.status = if receipt.local_cleanup_complete {
                                "completed"
                            } else {
                                "cleanup_pending"
                            }
                            .into();
                        }
                        Err(_) => {
                            run.status = "cleanup_pending".into();
                            run.reason = Some("local_cleanup_unavailable".into());
                        }
                    }
                }
                Err(_) => {
                    run.reason = Some("apply_not_confirmed".into());
                }
            }
        }
    }
    // An indeterminate canonical failure must still prevent opening an owner.
    // Successful logical apply is discoverable by its original receipt on retry.
    let expected = store
        .state()
        .records
        .get(&key(Collection::Projection, &name))
        .map(|r| r.revision);
    let revision = expected.map_or(Ok(Revision::ZERO), |r| r.next())?;
    put(store, access, name, expected, revision, &run, now).await?;
    Ok(Some(run))
}

/// Resume at most four already committed purge jobs. This cannot select new
/// payloads; remaining or pinned jobs stay available for explicit cleanup.
pub async fn resume_cleanup(store: &mut Store, access: &Access, now: Timestamp) -> Result<()> {
    authorize(store, access, true)?;
    let mut pending = Vec::new();
    for row in store.state().records.values().filter(|r| {
        r.workspace == access.workspace
            && r.collection == Collection::Projection
            && r.value["document_type"] == "vcp_retention_job_v1"
    }) {
        let job: PruneReceipt = row.decode()?;
        if job.preview.action == Action::Purge && !job.local_cleanup_complete {
            pending.push(job.id);
        }
    }
    for id in pending.into_iter().take(4) {
        if retention::cleanup(store, access, &id, now).await.is_err() && !store.healthy() {
            return Err(Error::Conflict(
                "retention cleanup requires canonical recovery",
            ));
        }
    }
    Ok(())
}
