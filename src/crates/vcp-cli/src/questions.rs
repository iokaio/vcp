// SPDX-License-Identifier: Apache-2.0
//! Presentation freshness only: the engine still authorizes every decision.
use vcp_domain::{effect::Effect, revision::Timestamp, task::Task, workspace::Workspace};
use vcp_protocol::command::{Approval, ApprovalState, CommandEnvelope};
use vcp_store::contract::{Collection, State};

/// Historical questions remain inspectable but must not block deliberate resume.
/// Owner identity is not part of a snapshot; live callers additionally check
/// `belongs_to_owner` against an envelope from their authenticated connection.
pub fn actionable(state: &State, approval: &Approval, now: Timestamp) -> Result<bool, String> {
    if approval.state != ApprovalState::Pending
        || approval.expires_at <= now
        || approval.controller.is_none()
        || approval.owner_epoch.is_none()
    {
        return Ok(false);
    }
    let workspace = &approval.scope.workspace;
    let task: Task = state
        .record(Collection::Task, approval.scope.task.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let effect: Effect = state
        .record(Collection::Effect, approval.effect.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if task.scope != approval.scope
        || task.state.terminal()
        || task.steering != approval.steering
        || effect.scope != approval.scope
        || effect.steering != approval.steering
        || effect.revision != approval.effect_revision
        || effect.operation_digest != approval.operation_digest
    {
        return Ok(false);
    }
    let workspace: Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    // The host uses revision zero before a policy is installed. Such a question
    // can still be explicitly denied; allowing it requires engine policy checks.
    let policy = vcp_engine::policy::optional(state, &workspace.id)
        .map_err(|e| e.to_string())?
        .map_or(vcp_domain::revision::PolicyRevision::ZERO, |policy| {
            policy.revision
        });
    Ok(approval.authority == Some(workspace.authority)
        && approval.binding == Some(workspace.binding.revision)
        && approval.policy == policy)
}

pub fn belongs_to_owner(approval: &Approval, owner: &CommandEnvelope) -> bool {
    approval.controller.as_ref() == Some(&owner.controller)
        && approval.owner_epoch == Some(owner.owner_epoch)
        && approval.actor == owner.caller
        && approval.scope.workspace == owner.workspace
        && approval.scope.session == owner.session
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use vcp_domain::{ids::*, revision::*};
    use vcp_store::contract::{key, Record};

    fn fixture() -> (State, Approval) {
        let workspace = WorkspaceId::new();
        let task = TaskId::new();
        let effect = ToolRunId::new();
        let scope = vcp_domain::workspace::Scope {
            workspace: workspace.clone(),
            session: SessionId::new(),
            task: task.clone(),
        };
        let approval = Approval {
            id: ApprovalId::new(),
            scope: scope.clone(),
            effect: effect.clone(),
            effect_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            operation_digest: "a".repeat(64),
            actor: ActorId::new(),
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(100),
            state: ApprovalState::Pending,
            revision: Revision::ZERO,
            controller: Some(ControllerId::new()),
            owner_epoch: Some(OwnerEpoch::new(1)),
            authority: Some(AuthorityRevision::ZERO),
            binding: Some(Revision::ZERO),
        };
        let mut state = State::default();
        for (collection, id, value) in [
            (
                Collection::Task,
                task.to_string(),
                json!({"scope":scope,"root":task,"parent":null,"fork_origin":null,"revision":Revision::ZERO,"steering":SteeringRevision::ZERO,"objectives":[],"state":"paused","fingerprint":{"repository":"a".repeat(64),"buffers":"b".repeat(64),"environment":"c".repeat(64)},"editing":false,"required_checks":[],"cause":EventId::new(),"reason":"fixture"}),
            ),
            (
                Collection::Effect,
                effect.to_string(),
                json!({"id":effect,"scope":scope,"revision":Revision::ZERO,"steering":SteeringRevision::ZERO,"state":"validated","operation_digest":"a".repeat(64),"execution":null,"exit_code":null,"observed_changes":[],"cause":EventId::new(),"reason":"fixture"}),
            ),
            (
                Collection::Workspace,
                workspace.to_string(),
                json!({"id":workspace,"binding":{"host":HostId::new(),"root":"fixture","repository":"fixture","worktree":"main","revision":Revision::ZERO},"trust":"trusted","revision":Revision::ZERO,"authority":AuthorityRevision::ZERO,"deletion":DeletionEpoch::ZERO}),
            ),
            (
                Collection::Access,
                workspace.to_string(),
                json!({"document_type":"vcp_authority_v1","schema_version":1,"id":workspace,"workspace":workspace,"revision":Revision::ZERO,"data":{"kind":"policy","policy":{"workspace":workspace,"revision":Revision::ZERO,"mode":"ask","denials":[],"workspace_roots":[],"automatic_effects":[],"timeout_ceiling_ms":Units::new(1000),"output_ceiling_bytes":ByteCount::new(1000)}}}),
            ),
        ] {
            state.records.insert(
                key(collection, &id),
                Record {
                    collection,
                    id,
                    workspace: workspace.clone(),
                    revision: Revision::ZERO,
                    value,
                    references: Default::default(),
                },
            );
        }
        (state, approval)
    }

    #[test]
    fn expired_or_superseded_questions_do_not_block_resume() {
        let (state, approval) = fixture();
        assert!(actionable(&state, &approval, Timestamp::new(99)).unwrap());
        assert!(!actionable(&state, &approval, Timestamp::new(100)).unwrap());
        for (collection, id, field, value) in [
            (
                Collection::Task,
                approval.scope.task.to_string(),
                "steering",
                json!(Revision::new(1)),
            ),
            (
                Collection::Effect,
                approval.effect.to_string(),
                "revision",
                json!(Revision::new(1)),
            ),
            (
                Collection::Effect,
                approval.effect.to_string(),
                "operation_digest",
                json!("b".repeat(64)),
            ),
            (
                Collection::Workspace,
                approval.scope.workspace.to_string(),
                "authority",
                json!(Revision::new(1)),
            ),
        ] {
            let mut changed = state.clone();
            changed
                .records
                .get_mut(&key(collection, &id))
                .unwrap()
                .value[field] = value;
            assert!(!actionable(&changed, &approval, Timestamp::new(99)).unwrap());
        }
        let mut historical = approval.clone();
        historical.controller = None;
        assert!(!actionable(&state, &historical, Timestamp::new(99)).unwrap());
        let mut changed = state.clone();
        let value: &mut Value = &mut changed
            .records
            .get_mut(&key(Collection::Access, approval.scope.workspace.as_str()))
            .unwrap()
            .value;
        value["data"]["policy"]["revision"] = json!(Revision::new(1));
        assert!(!actionable(&changed, &approval, Timestamp::new(99)).unwrap());
    }

    #[test]
    fn reopened_owner_cannot_inherit_a_pending_question() {
        let (_, approval) = fixture();
        let mut owner = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: approval.scope.workspace.clone(),
            session: approval.scope.session.clone(),
            task: Some(approval.scope.task.clone()),
            caller: approval.actor.clone(),
            controller: approval.controller.clone().unwrap(),
            owner_epoch: approval.owner_epoch.unwrap(),
            expected: Revision::ZERO,
            steering: approval.steering,
            payload: vcp_protocol::command::Command::Inspect,
        };
        assert!(belongs_to_owner(&approval, &owner));
        owner.owner_epoch = OwnerEpoch::new(2);
        assert!(!belongs_to_owner(&approval, &owner));
        owner.owner_epoch = approval.owner_epoch.unwrap();
        owner.controller = ControllerId::new();
        assert!(!belongs_to_owner(&approval, &owner));
    }
}
