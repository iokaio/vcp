// SPDX-License-Identifier: Apache-2.0
//! Deterministic event folds. This package has no model/tool transport dependency.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vcp_domain::{
    accounting::{Attempt, Ledger},
    effect::Effect,
    ids::*,
    revision::*,
    task::Task,
    workspace::Workspace,
};
use vcp_protocol::{canonical_bytes, digest_bytes, event::*};
use vcp_store::contract::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub schema_version: u32,
    pub projector_version: u32,
    pub workspace: WorkspaceId,
    pub watermark: Watermark,
    pub sequences: BTreeMap<SessionId, SessionSeq>,
    pub tasks: BTreeMap<TaskId, Task>,
    pub effects: BTreeMap<ToolRunId, Effect>,
    pub ledgers: BTreeMap<TaskId, Ledger>,
    pub attempts: BTreeMap<AttemptId, Attempt>,
    pub binding: Option<Workspace>,
    pub event_count: Units,
    pub event_kinds: BTreeMap<String, Units>,
}
impl View {
    pub fn new(workspace: WorkspaceId, version: u32) -> Result<Self> {
        if ![1, 2].contains(&version) {
            return Err(Error::Version);
        }
        Ok(Self {
            schema_version: 1,
            projector_version: version,
            workspace,
            watermark: Watermark::ZERO,
            sequences: BTreeMap::new(),
            tasks: BTreeMap::new(),
            effects: BTreeMap::new(),
            ledgers: BTreeMap::new(),
            attempts: BTreeMap::new(),
            binding: None,
            event_count: Units::ZERO,
            event_kinds: BTreeMap::new(),
        })
    }
    pub fn apply(&mut self, event: &EventEnvelope) -> Result<bool> {
        if event.version != 1 {
            return Err(Error::Version);
        }
        if event.event.workspace != self.workspace {
            return Err(Error::Access);
        }
        let previous = self
            .sequences
            .get(&event.event.session)
            .copied()
            .unwrap_or_default();
        if event.sequence <= previous {
            return Ok(false);
        }
        if event.sequence != previous.next()? || event.watermark < self.watermark {
            return Err(Error::Integrity("event gap or order"));
        }
        let mut next = self.clone();
        if let Some(facts) = event
            .event
            .data
            .get("facts")
            .and_then(|value| value.as_array())
        {
            for fact in facts {
                match fact["collection"].as_str() {
                    Some("workspace") => {
                        let workspace: Workspace = serde_json::from_value(fact["value"].clone())?;
                        if workspace.id != self.workspace {
                            return Err(Error::Access);
                        }
                        next.binding = Some(workspace);
                    }
                    Some("task") => {
                        let task: Task = serde_json::from_value(fact["value"].clone())?;
                        if task.scope.workspace != self.workspace {
                            return Err(Error::Access);
                        }
                        next.tasks.insert(task.scope.task.clone(), task);
                    }
                    Some("effect") => {
                        let effect: Effect = serde_json::from_value(fact["value"].clone())?;
                        if effect.scope.workspace != self.workspace {
                            return Err(Error::Access);
                        }
                        next.effects.insert(effect.id.clone(), effect);
                    }
                    _ => {}
                }
            }
        }
        if let Some(value) = event.event.data.get("ledger") {
            let ledger: Ledger = serde_json::from_value(value.clone())?;
            if ledger.scope.workspace != self.workspace {
                return Err(Error::Access);
            }
            next.ledgers.insert(ledger.scope.task.clone(), ledger);
        }
        if let Some(value) = event.event.data.get("attempt") {
            let attempt: Attempt = serde_json::from_value(value.clone())?;
            if attempt.scope.workspace != self.workspace {
                return Err(Error::Access);
            }
            next.attempts.insert(attempt.id.clone(), attempt);
        }
        next.sequences
            .insert(event.event.session.clone(), event.sequence);
        next.watermark = event.watermark;
        next.event_count = next.event_count.next()?;
        if next.projector_version >= 2 {
            let kind = serde_json::to_value(&event.event.kind)?
                .as_str()
                .ok_or(Error::Integrity("event kind"))?
                .to_owned();
            let count = next.event_kinds.entry(kind).or_default();
            *count = count.next()?;
        }
        *self = next;
        Ok(true)
    }
    pub fn semantic_digest(&self) -> Result<String> {
        // Version 2 adds diagnostics only. Compare authoritative facts and the
        // same input boundary before activating it beside the version 1 fold.
        let mut semantic = self.clone();
        semantic.projector_version = 1;
        semantic.event_kinds.clear();
        Ok(digest_bytes(&canonical_bytes(&semantic)?))
    }
}
pub fn rebuild(
    state: &State,
    workspace: &WorkspaceId,
    version: u32,
    watermark: Watermark,
) -> Result<View> {
    if watermark > state.watermark {
        return Err(Error::Integrity("snapshot watermark"));
    }
    let mut result = View::new(workspace.clone(), version)?;
    for event in state
        .events
        .iter()
        .filter(|event| &event.event.workspace == workspace && event.watermark <= watermark)
    {
        result.apply(event)?;
    }
    result.watermark = watermark;
    Ok(result)
}
fn view_id(workspace: &WorkspaceId, version: u32) -> String {
    format!("history-v{version}-{workspace}")
}
fn active_id(workspace: &WorkspaceId) -> String {
    format!("history-active-{workspace}")
}
pub fn active(state: &State, workspace: &WorkspaceId) -> Result<Option<View>> {
    let Some(pointer) = state
        .records
        .get(&key(Collection::Projection, &active_id(workspace)))
    else {
        return Ok(None);
    };
    if &pointer.workspace != workspace {
        return Err(Error::Access);
    }
    let version = u32::try_from(
        pointer.value["projector_version"]
            .as_u64()
            .ok_or(Error::Version)?,
    )
    .map_err(|_| Error::Version)?;
    let view: View = state
        .record(
            Collection::Projection,
            &view_id(workspace, version),
            workspace,
        )?
        .decode()?;
    if pointer.value["semantic_digest"] != view.semantic_digest()?
        || pointer.value["watermark"] != serde_json::to_value(view.watermark)?
    {
        return Err(Error::Integrity("projection activation pointer"));
    }
    Ok(Some(view))
}
fn mutation(
    state: &State,
    workspace: &WorkspaceId,
    id: String,
    value: &impl Serialize,
) -> Result<Mutation> {
    let previous = state
        .records
        .get(&key(Collection::Projection, &id))
        .map(|r| r.revision);
    let revision = previous
        .map(Revision::next)
        .transpose()?
        .unwrap_or_default();
    Ok(Mutation::Put {
        expected: previous,
        record: Record::typed(
            Collection::Projection,
            id,
            workspace.clone(),
            revision,
            value,
        )?,
    })
}
/// Projection content and input watermark publish in one canonical transaction.
/// A new version is rebuilt beside the active version and compared at the same
/// snapshot watermark. No recovery path can dispatch external work.
pub fn prepare_activation(
    state: &State,
    workspace: &WorkspaceId,
    version: u32,
) -> Result<(Transaction, View)> {
    state.record(Collection::Workspace, workspace.as_str(), workspace)?;
    let current = active(state, workspace)?;
    let view =
        if let Some(old) = current
            .as_ref()
            .filter(|old| old.projector_version == version)
        {
            let mut next = old.clone();
            for event in state.events.iter().filter(|event| {
                event.event.workspace == *workspace && event.watermark > old.watermark
            }) {
                next.apply(event)?;
            }
            next.watermark = state.watermark;
            next
        } else {
            rebuild(state, workspace, version, state.watermark)?
        };
    if let Some(old) = current {
        let comparable = rebuild(state, workspace, old.projector_version, state.watermark)?;
        if comparable.semantic_digest()? != view.semantic_digest()? {
            return Err(Error::Integrity(
                "new projector disagrees at the same watermark",
            ));
        }
    }
    let pointer = serde_json::json!({"schema_version":1,"projector_version":version,"watermark":view.watermark,"semantic_digest":view.semantic_digest()?});
    let mutations = vec![
        mutation(state, workspace, view_id(workspace, version), &view)?,
        mutation(state, workspace, active_id(workspace), &pointer)?,
    ];
    Ok((
        Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations,
            events: vec![],
            command: None,
        },
        view,
    ))
}
pub async fn publish<S: CanonicalStore>(
    store: &mut S,
    workspace: &WorkspaceId,
    version: u32,
) -> Result<View> {
    if let Some(current) = active(store.state(), workspace)? {
        if current.projector_version == version
            && !store.state().events.iter().any(|event| {
                event.event.workspace == *workspace && event.watermark > current.watermark
            })
        {
            return Ok(current);
        }
    }
    let (transaction, view) = prepare_activation(store.state(), workspace, version)?;
    store.transact(transaction).await?;
    Ok(view)
}
