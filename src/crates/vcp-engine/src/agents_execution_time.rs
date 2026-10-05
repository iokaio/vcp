// SPDX-License-Identifier: Apache-2.0
//! Explicit owner revision of child execution time, never grant expiry.
use super::*;
use vcp_domain::artifact::{ArtifactDescriptor, CaptureState, Channel};

/// Native owner input, deliberately not a deserializable public command.
pub struct NativeExecutionTimeEvidence {
    pub expected_graph: Revision,
    pub children: BTreeSet<TaskId>,
    pub evidence: ArtifactId,
}

impl<S: CanonicalStore> crate::Engine<S> {
    /// The root may be running during startup configuration; selected children
    /// must be nonterminal and not running. The owning host separately requires
    /// closed provider streams before requesting this configuration transition.
    pub async fn suspend_child_execution_time(
        &mut self,
        scope: &Scope,
        evidence: NativeExecutionTimeEvidence,
        access: &crate::Access,
        host: &crate::HostFacts,
    ) -> Result<Option<Receipt>> {
        self.authorize(access)?;
        if !access.write
            || access.workspace != scope.workspace
            || access.session != scope.session
            || !host.may_execute
        {
            return Err(Error::Access);
        }
        let root = task(self.store().current(), scope, &scope.task)?;
        if root.parent.is_some() || root.root != scope.task || root.state.terminal() {
            return Err(Error::Access);
        }
        let mut graph = graph(self.store().current(), scope, &root.root)?.ok_or(Error::Target)?;
        if evidence.children.is_empty() || evidence.children.len() > 128 {
            return Err(Error::Target);
        }
        let workspace: Workspace = self
            .store()
            .current()
            .record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )?
            .decode()?;
        let policy = crate::policy::current(self.store().current(), &scope.workspace)?;
        if workspace.trust != Trust::Trusted || host.policy != policy.revision {
            return Err(Error::Access);
        }
        for id in &evidence.children {
            let spec = graph.children.get(id).ok_or(Error::Target)?;
            let child = task(self.store().current(), scope, id)?;
            let parent = task(self.store().current(), scope, &spec.parent)?;
            if spec.actor != access.actor
                || spec.authority != workspace.authority
                || spec.binding != workspace.binding.revision
                || spec.policy != policy.revision
                || spec.parent_steering != parent.steering
                || child.state == TaskState::Running
                || child.state.terminal()
                || parent.state.terminal()
                || graph.cleanup.contains_key(id)
                || spec.deadline.is_unbounded()
            {
                return Err(Error::Access);
            }
        }
        // Retrying the same selected original assignments creates no new policy meaning.
        if evidence
            .children
            .iter()
            .all(|id| graph.execution_time.contains_key(id))
        {
            return Ok(None);
        }
        if graph.revision != evidence.expected_graph {
            return Err(vcp_domain::Error::Stale.into());
        }
        let descriptor: ArtifactDescriptor = self
            .store()
            .current()
            .record(
                Collection::Artifact,
                evidence.evidence.as_str(),
                &scope.workspace,
            )?
            .decode()?;
        if descriptor.state != CaptureState::Complete
            || descriptor.spec.scope != *scope
            || descriptor.spec.channel != Channel::Evidence
            || descriptor.spec.schema != "child-execution-time/1"
        {
            return Err(Error::Access);
        }
        graph.revision = graph.revision.next()?;
        for id in &evidence.children {
            let spec = &graph.children[id];
            graph
                .execution_time
                .entry(id.clone())
                .or_insert_with(|| ChildExecutionTime {
                    version: 1,
                    original: spec.deadline,
                    effective: Limit::Unbounded,
                    graph_revision: graph.revision,
                    recorded_at: host.now,
                    actor: access.actor.clone(),
                    authority: workspace.authority,
                    policy: policy.revision,
                    binding: workspace.binding.revision,
                    evidence: evidence.evidence.clone(),
                });
        }
        // A selected descendant cannot widen beyond an unconverted finite ancestor.
        graph.validate()?;
        let event = vcp_protocol::event::EventInput {
            id: EventId::new(),
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(scope.task.clone()),
            actor: access.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: host.now,
            kind: vcp_protocol::event::EventKind::ChildGraphChanged,
            artifacts: vec![evidence.evidence.clone()],
            metadata: None,
            data: serde_json::json!({"schema_version":1,"child_execution_time":{"children":evidence.children,"graph_revision":graph.revision,"effective":Limit::<Timestamp>::Unbounded}}),
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.store().current().watermark,
            mutations: vec![Mutation::Put {
                expected: Some(evidence.expected_graph),
                record: Record::typed(
                    Collection::Projection,
                    graph_id(&scope.task),
                    scope.workspace.clone(),
                    graph.revision,
                    &graph,
                )?,
            }],
            events: vec![event],
            command: None,
        };
        Ok(Some(self.store_mut().transact(transaction).await?))
    }
}
