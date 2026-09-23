// SPDX-License-Identifier: Apache-2.0
//! Scoped public retention over the shared exact-preview pipeline.
//! The caller retains its authenticated task ceiling through every stage.
use crate::{
    access::{self, Access},
    retention::{self, Action, PrunePreview, PruneReceipt, Target},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{ids::*, retention_selector::Selector, revision::*, task::Task, workspace::Scope};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{command_key, Collection, Receipt, State},
    Store,
};
#[path = "retention_limits.rs"]
mod limits;

/// A retained ceiling is evidence of the original selection, never a grant.
/// Every operation intersects it with the caller's current authenticated rights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub scope: Scope,
    pub tasks: BTreeSet<TaskId>,
    pub command: CommandId,
    pub command_digest: String,
}
#[derive(Clone)]
pub struct Preview {
    scope: Scope,
    tasks: BTreeSet<TaskId>,
    value: PrunePreview,
}
impl Preview {
    pub fn id(&self) -> &str {
        &self.value.id
    }
    pub fn digest(&self) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(&(
            &self.scope,
            &self.tasks,
            &self.value,
        ))?))
    }
    /// Only the scoped selection leaves the shared boundary. The wire adapter
    /// must use bounded target pages and cleanup counts, not serialize raw rows.
    pub fn selection(&self) -> &PrunePreview {
        &self.value
    }
    /// Count the complete private cache value without allocating its serialized
    /// form. The caller's aggregate cache budget includes the task ceiling.
    pub fn cache_bytes(&self, limit: usize) -> Result<usize> {
        struct Count {
            remaining: usize,
            used: usize,
        }
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.remaining = self.remaining.checked_sub(bytes.len()).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "retention preview cache limit",
                    )
                })?;
                self.used += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count {
            remaining: limit,
            used: 0,
        };
        serde_json::to_writer(&mut count, &(&self.scope, &self.tasks, &self.value))
            .map_err(|_| Error::Conflict("retention preview cache limit"))?;
        Ok(count.used)
    }
}
pub struct Apply {
    pub command: CommandId,
    pub command_digest: String,
    pub expected_revision: Revision,
    pub steering: SteeringRevision,
}
pub struct Commit {
    pub receipt: Receipt,
    pub job: PruneReceipt,
}

fn authorize(
    store: &Store,
    access: &Access,
    scope: &Scope,
    write: bool,
) -> Result<BTreeSet<TaskId>> {
    access::authorize(store.state(), access, write)?;
    if scope.workspace != access.workspace || !access.allows_task(&scope.task) {
        return Err(Error::Access);
    }
    let tasks = access.tasks.as_ref().ok_or(Error::Access)?;
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &access.workspace)?
        .decode()?;
    if task.scope != *scope {
        return Err(Error::Access);
    }
    // Scope validation never converts Some(tasks) into workspace-wide authority.
    for id in tasks {
        let task: Task = store
            .state()
            .record(Collection::Task, id.as_str(), &access.workspace)?
            .decode()?;
        if task.scope.session != scope.session {
            return Err(Error::Access);
        }
    }
    Ok(tasks.clone())
}
pub(crate) fn target_allowed(
    state: &State,
    scope: &Scope,
    tasks: &BTreeSet<TaskId>,
    target: &Target,
) -> Result<bool> {
    let own = retention::scope(state, target)?;
    Ok(own.is_some_and(|own| {
        own.workspace == scope.workspace
            && own.session == scope.session
            && tasks.contains(&own.task)
    }))
}
pub(crate) fn authorize_targets(
    store: &Store,
    access: &Access,
    binding: &Binding,
    preview: &PrunePreview,
    write: bool,
) -> Result<()> {
    let current = authorize(store, access, &binding.scope, write)?;
    let allowed = current.intersection(&binding.tasks).cloned().collect();
    for target in preview.selected.union(&preview.dependent) {
        if !target_allowed(store.state(), &binding.scope, &allowed, target)? {
            return Err(Error::Access);
        }
    }
    Ok(())
}
pub fn preview(
    store: &Store,
    access: &Access,
    scope: Scope,
    selector: Selector,
    action: Action,
    now: Timestamp,
) -> Result<Preview> {
    let tasks = authorize(store, access, &scope, false)?;
    limits::check(store.state())?;
    let value = retention::preview_scoped(store, access, &scope, selector, action, now)?;
    Ok(Preview {
        scope,
        tasks,
        value,
    })
}
/// Authorize a frozen page without rerunning its selector. Current deletion and
/// authority must still match; neither a new task nor a wider role expands it.
pub fn validate_preview(
    store: &Store,
    access: &Access,
    scope: &Scope,
    preview: &Preview,
) -> Result<()> {
    let current = authorize(store, access, scope, false)?;
    let workspace = access::authorize(store.state(), access, false)?;
    if scope != &preview.scope
        || access.actor != preview.value.actor
        || workspace.authority != preview.value.authority
        || workspace.deletion != preview.value.deletion
    {
        return Err(Error::Access);
    }
    let allowed = current.intersection(&preview.tasks).cloned().collect();
    for target in preview
        .value
        .selected
        .union(&preview.value.dependent)
        .chain(preview.value.protected.iter().map(|p| &p.target))
    {
        if !target_allowed(store.state(), scope, &allowed, target)? {
            return Err(Error::Access);
        }
    }
    Ok(())
}
/// Reconcile before looking up an expiring connection-local preview. Once the
/// command is accepted, its exact selected IDs live in the durable job itself.
pub fn replay(
    store: &Store,
    access: &Access,
    scope: &Scope,
    preview_id: &str,
    request: &Apply,
) -> Result<Option<Commit>> {
    authorize(store, access, scope, true)?;
    let Some(command) = store
        .state()
        .commands
        .get(&command_key(&scope.workspace, &request.command))
    else {
        return Ok(None);
    };
    if command.digest != request.command_digest {
        return Err(Error::Conflict("retention command payload conflict"));
    }
    let job = read_job(store, access, scope, &format!("prune-{preview_id}"))?;
    let binding = job.public.as_ref().ok_or(Error::Access)?;
    if binding.command != request.command || binding.command_digest != request.command_digest {
        return Err(Error::Conflict("retention job command identity"));
    }
    let receipt = store
        .state()
        .transactions
        .get(&command.transaction)
        .ok_or(Error::Conflict("retention receipt unavailable"))?
        .clone();
    if receipt.command.as_ref() != Some(command) {
        return Err(Error::Conflict("retention receipt integrity"));
    }
    Ok(Some(Commit { receipt, job }))
}
pub async fn apply(
    store: &mut Store,
    access: &Access,
    preview: &Preview,
    expected_digest: &str,
    request: &Apply,
    now: Timestamp,
) -> Result<Commit> {
    if let Some(commit) = replay(store, access, &preview.scope, preview.id(), request)? {
        return Ok(commit);
    }
    limits::check(store.state())?;
    if preview.digest()? != expected_digest {
        return Err(Error::Conflict("retention preview digest changed"));
    }
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            preview.scope.task.as_str(),
            &access.workspace,
        )?
        .decode()?;
    if task.redaction.is_some()
        || task.revision != request.expected_revision
        || task.steering != request.steering
    {
        return Err(Error::Conflict("retention task precondition changed"));
    }
    let binding = Binding {
        scope: preview.scope.clone(),
        tasks: preview.tasks.clone(),
        command: request.command.clone(),
        command_digest: request.command_digest.clone(),
    };
    authorize_targets(store, access, &binding, &preview.value, true)?;
    retention::apply_scoped(store, access, &preview.value, &binding, now).await
}
pub fn read_job(store: &Store, access: &Access, scope: &Scope, id: &str) -> Result<PruneReceipt> {
    authorize(store, access, scope, false)?;
    let job: PruneReceipt = store
        .state()
        .record(Collection::Projection, id, &access.workspace)?
        .decode()?;
    let binding = job.public.as_ref().ok_or(Error::Access)?;
    if binding.scope != *scope || job.workspace != access.workspace {
        return Err(Error::Access);
    }
    authorize_targets(store, access, binding, &job.preview, false)?;
    Ok(job)
}
/// Continue only the already accepted job. Physical rewrite is limited to its
/// authorized exact IDs. Shared/mixed generations remain explicitly pending for
/// separately authorized maintenance; no task-scoped access is widened for GC.
pub async fn cleanup(
    store: &mut Store,
    access: &Access,
    scope: &Scope,
    id: &str,
    now: Timestamp,
) -> Result<PruneReceipt> {
    let job = read_job(store, access, scope, id)?;
    let binding = job.public.as_ref().ok_or(Error::Access)?;
    authorize_targets(store, access, binding, &job.preview, true)?;
    retention::cleanup_scoped(store, access, scope, id, now).await
}
