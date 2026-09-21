// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::agents::*;
pub(super) fn kind(record: &Record) -> Result<bool> {
    if record.value["document_type"] == GRAPH {
        if record.collection != Collection::Projection {
            return Err(Error::Corruption("graph collection"));
        }
        return Ok(true);
    }
    if record.value["document_type"]
        .as_str()
        .is_some_and(|kind| kind.starts_with("vcp_task_graph_"))
    {
        return Err(Error::Incompatible);
    }
    Ok(false)
}
pub(super) fn shape(record: &Record) -> Result<()> {
    let graph: TaskGraph = record.decode()?;
    graph.validate()?;
    if record.id != graph_id(&graph.scope.task)
        || record.workspace != graph.scope.workspace
        || record.revision != graph.revision
    {
        return Err(Error::Corruption("graph identity"));
    }
    Ok(())
}
pub(super) fn references(record: &Record) -> Result<BTreeSet<String>> {
    let graph: TaskGraph = record.decode()?;
    let mut refs = BTreeSet::from([
        key(Collection::Task, graph.scope.task.as_str()),
        key(Collection::Ledger, graph.scope.task.as_str()),
    ]);
    for (id, child) in &graph.children {
        refs.insert(key(Collection::Task, id.as_str()));
        refs.insert(key(Collection::Task, child.parent.as_str()));
        refs.insert(key(Collection::Artifact, child.snapshot.as_str()));
        if let Some(registration) = &child.registration {
            refs.insert(key(Collection::Artifact, registration.as_str()));
        }
        for grant in child.grants.keys() {
            refs.insert(key(Collection::Access, grant.as_str()));
        }
    }
    for result in graph.results.values().flatten() {
        refs.insert(key(Collection::Artifact, result.packet.as_str()));
        refs.insert(key(Collection::Artifact, result.plan.as_str()));
        if let Some(effect) = &result.effect {
            refs.insert(key(Collection::Effect, effect.as_str()));
        }
    }
    Ok(refs)
}
pub(super) fn transition(previous: &Record, next: &Record) -> Result<()> {
    if kind(previous)? != kind(next)? {
        return Err(Error::Conflict("graph document kind"));
    }
    if !kind(previous)? {
        return Ok(());
    }
    let before: TaskGraph = previous.decode()?;
    let after: TaskGraph = next.decode()?;
    if before.scope != after.scope
        || before.limits != after.limits
        || before
            .children
            .keys()
            .any(|id| !after.children.contains_key(id))
    {
        return Err(Error::Conflict("immutable graph ownership or limits"));
    }
    for (id, prior) in &before.children {
        let mut current = after.children[id].clone();
        current.dependencies = prior.dependencies.clone();
        if &current != prior {
            return Err(Error::Conflict("immutable child assignment"));
        }
    }
    for (id, ready) in &before.ready {
        if after.ready.get(id) != Some(ready) {
            return Err(Error::Conflict("immutable workspace receipt"));
        }
    }
    for (id, results) in &before.results {
        if after
            .results
            .get(id)
            .is_none_or(|next| !next.starts_with(results))
        {
            return Err(Error::Conflict("append-only child result history"));
        }
    }
    Ok(())
}
pub(super) fn validate(state: &State) -> Result<()> {
    for record in state.records.values() {
        if !kind(record)? {
            continue;
        }
        let graph: TaskGraph = record.decode()?;
        let root: Task = state
            .record(
                Collection::Task,
                graph.scope.task.as_str(),
                &graph.scope.workspace,
            )?
            .decode()?;
        if root.scope != graph.scope || root.parent.is_some() {
            return Err(Error::Corruption("graph root"));
        }
        let ledger: Ledger = state
            .record(
                Collection::Ledger,
                graph.scope.task.as_str(),
                &graph.scope.workspace,
            )?
            .decode()?;
        for row in state
            .records
            .values()
            .filter(|r| r.collection == Collection::Task && r.workspace == graph.scope.workspace)
        {
            let child: Task = row.decode()?;
            if child.root == graph.scope.task
                && child.parent.is_some()
                && !child.state.terminal()
                && !graph.children.contains_key(&child.scope.task)
            {
                return Err(Error::Corruption("active child missing graph assignment"));
            }
        }
        for (id, child) in &graph.children {
            let task: Task = state
                .record(Collection::Task, id.as_str(), &graph.scope.workspace)?
                .decode()?;
            if task.root != graph.scope.task
                || task.scope.session != graph.scope.session
                || task.parent.as_ref() != Some(&child.parent)
                || ledger.allocations.get(id) != Some(&child.allocation)
                || task.editing != (child.mode == ChildMode::IsolatedWrite)
            {
                return Err(Error::Corruption("child task or allocation binding"));
            }
            for (artifact, digest) in std::iter::once((&child.snapshot, &child.snapshot_digest))
                .chain(
                    child
                        .registration
                        .iter()
                        .zip(child.registration_digest.iter()),
                )
            {
                let artifact: ArtifactDescriptor = state
                    .record(
                        Collection::Artifact,
                        artifact.as_str(),
                        &graph.scope.workspace,
                    )?
                    .decode()?;
                if artifact.spec.scope.task != child.parent
                    || artifact.spec.scope.workspace != graph.scope.workspace
                    || artifact.spec.scope.session != graph.scope.session
                    || artifact.state != vcp_domain::artifact::CaptureState::Complete
                    || &artifact.sha256 != digest
                {
                    return Err(Error::Corruption("child input artifact binding"));
                }
            }
            for result in graph.results.get(id).into_iter().flatten() {
                for (artifact_id, packet) in [(&result.packet, true), (&result.plan, false)] {
                    let artifact: ArtifactDescriptor = state
                        .record(
                            Collection::Artifact,
                            artifact_id.as_str(),
                            &graph.scope.workspace,
                        )?
                        .decode()?;
                    let schema = artifact.spec.schema.as_str();
                    if artifact.spec.scope.task != child.parent
                        || artifact.spec.scope.session != graph.scope.session
                        || artifact.state != vcp_domain::artifact::CaptureState::Complete
                        || (packet && schema != "child-result-packet/1")
                        || (!packet
                            && !matches!(
                                schema,
                                "child-integration-plan/1" | "child-integration-admission/1"
                            ))
                    {
                        return Err(Error::Corruption("child result artifact binding"));
                    }
                }
                if let Some(effect) = &result.effect {
                    let effect: vcp_domain::effect::Effect = state
                        .record(Collection::Effect, effect.as_str(), &graph.scope.workspace)?
                        .decode()?;
                    if effect.scope.task != child.parent
                        || effect.scope.session != graph.scope.session
                    {
                        return Err(Error::Corruption("child result effect binding"));
                    }
                }
            }
        }
    }
    Ok(())
}
fn capacity(state: &State, graph: &TaskGraph) -> Result<()> {
    let ledger: Ledger = state
        .record(
            Collection::Ledger,
            graph.scope.task.as_str(),
            &graph.scope.workspace,
        )?
        .decode()?;
    // Admission checks capacity; later actual charges must still be recorded
    // even if a provider overruns its reservation. Never reject settlement.
    // Allocations subdivide a parent cap; cancellation never releases them.
    // Count a parent's own spend once, plus direct-child allocations. Nested
    // allocations are already contained in their ancestor's allocation.
    for parent in std::iter::once(&graph.scope.task).chain(graph.children.keys()) {
        let cap = if parent == &graph.scope.task {
            ledger
                .cap
                .get()
                .checked_sub(ledger.protected.get())
                .ok_or(Error::Conflict("protected root allocation"))?
        } else {
            graph.children[parent].allocation.get()
        };
        let mut used = 0u64;
        for (id, allocation) in &ledger.allocations {
            let task: Task = state
                .record(Collection::Task, id.as_str(), &graph.scope.workspace)?
                .decode()?;
            if task.parent.as_ref() == Some(parent) {
                used = used
                    .checked_add(allocation.get())
                    .ok_or(Error::Corruption("allocation overflow"))?;
            }
        }
        for row in state.records.values().filter(|r| {
            r.collection == Collection::Reservation && r.workspace == graph.scope.workspace
        }) {
            let reservation: Reservation = row.decode()?;
            if &reservation.scope.task == parent {
                used = used
                    .checked_add(reservation.charged.get())
                    .and_then(|v| v.checked_add(reservation.liability.get()))
                    .ok_or(Error::Corruption("allocation exposure overflow"))?;
            }
        }
        if used > cap {
            return Err(Error::Conflict("child allocations exceed parent capacity"));
        }
    }
    Ok(())
}
pub(super) fn publication(before: &State, after: &State) -> Result<()> {
    for record in after.records.values() {
        if !kind(record)? {
            continue;
        }
        let graph: TaskGraph = record.decode()?;
        let previous = before
            .records
            .get(&key(Collection::Projection, &record.id))
            .map(Record::decode::<TaskGraph>)
            .transpose()?;
        if graph.children.keys().any(|id| {
            previous
                .as_ref()
                .is_none_or(|prior| !prior.children.contains_key(id))
        }) {
            capacity(after, &graph)?;
        }
        for (id, spec) in &graph.children {
            let child: Task = after
                .record(Collection::Task, id.as_str(), &graph.scope.workspace)?
                .decode()?;
            let prior = previous.as_ref().and_then(|g| g.children.get(id));
            if prior.is_none() {
                let parent: Task = before
                    .record(
                        Collection::Task,
                        spec.parent.as_str(),
                        &graph.scope.workspace,
                    )?
                    .decode()?;
                if before
                    .records
                    .contains_key(&key(Collection::Task, id.as_str()))
                    || child.state != vcp_domain::task::TaskState::Pending
                    || parent.state != vcp_domain::task::TaskState::Running
                    || (!parent.editing && spec.mode == ChildMode::IsolatedWrite)
                    || parent.steering != spec.parent_steering
                    || graph.ready.contains_key(id)
                    || child
                        .objectives
                        .last()
                        .is_none_or(|o| o.acceptance.is_empty())
                {
                    return Err(Error::Conflict(
                        "child must be atomically registered pending before materialization",
                    ));
                }
            } else if prior.is_some_and(|p| p.dependencies != spec.dependencies)
                && child.state != vcp_domain::task::TaskState::Pending
            {
                return Err(Error::Conflict("running child dependencies are immutable"));
            }
        }
    }
    Ok(())
}
