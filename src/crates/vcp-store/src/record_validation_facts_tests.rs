// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::*;
use crate::{Error, Result};
use std::collections::BTreeSet;
use vcp_domain::{
    task::{Task, Turn},
    workspace::Session,
    *,
};
use vcp_protocol::command::Approval;
#[path = "../tests/common/mod.rs"]
mod common;

// Frozen pre-memoization validator: the whole cross-record path and its error
// order remain independent of the optimized pure scope-decoding facts.
fn reference(state: &State) -> Result<()> {
    for (key, record) in &state.records {
        if &record.key() != key {
            return Err(Error::Corruption("canonical key"));
        }
        record.validate_shape()?;
        state.validate_memory(record)?;
        crate::memory_review_contract::validate(state, record)?;
        crate::forecast_contract::validate(state, record)?;
        if record.collection == Collection::Access
            && record.value["document_type"] == "vcp_authority_v1"
        {
            use vcp_domain::policy::*;
            let document: AuthorityDocument = record.decode()?;
            if let AuthorityData::Grant { grant } = document.data {
                if let Some(id) = &grant.approval {
                    let approval: Approval = state
                        .record(Collection::Approval, id.as_str(), &record.workspace)?
                        .decode()?;
                    if approval.state != vcp_protocol::command::ApprovalState::Allowed
                        || grant.actor != approval.actor
                        || grant.policy != approval.policy
                        || grant.scope
                            != (GrantScope::Task {
                                scope: approval.scope.clone(),
                            })
                        || grant.target
                            != (GrantTarget::Exact {
                                digest: approval.operation_digest.clone(),
                            })
                        || grant.expires_at != approval.expires_at
                        || Some(grant.authority) != approval.authority
                        || Some(grant.binding) != approval.binding
                    {
                        return Err(Error::Corruption("grant differs from recorded approval"));
                    }
                }
            }
        }
        crate::snapshot_inputs::component_scope(state, record)?;
        for reference in record.required_references()? {
            let target = state
                .records
                .get(&reference)
                .ok_or(Error::Corruption("missing canonical reference"))?;
            if target.workspace != record.workspace {
                return Err(Error::Access);
            }
            let backup_provenance =
                crate::snapshot_inputs::cross_task_provenance(record, target, &reference)?;
            if let (Some(source), Some(target)) = (record.task_scope()?, target.task_scope()?) {
                // Fork and parent links are explicit task relationships. Data
                // belonging to another task cannot be reused as this task's
                // turn input, verification, approval, or effect observation.
                if record.memory_kind()?.is_none()
                    && ingestion_contract::kind(record)?.is_none()
                    && search_contract::kind(record)?.is_none()
                    && !agents_contract::kind(record)?
                    && record.collection != Collection::Task
                    && record.collection != Collection::Ledger
                    && reference.split(':').next() != Some("ledger")
                    && !backup_provenance
                    && source != target
                {
                    return Err(Error::Access);
                }
            }
        }
        if let Some(scope) = record.task_scope()? {
            let task: Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
                .decode()?;
            if task.scope != scope {
                return Err(Error::Access);
            }
        }
        if record.collection == Collection::Session {
            let session: Session = record.decode()?;
            if let Some(boundary) = session.fork_through {
                let turn: Turn = state
                    .record(Collection::Turn, boundary.as_str(), &record.workspace)?
                    .decode()?;
                if Some(&turn.scope.session) != session.fork_origin.as_ref()
                    || turn.state != vcp_domain::task::TurnState::Completed
                {
                    return Err(Error::Corruption(
                        "session fork boundary differs from ancestry",
                    ));
                }
            }
        }
        if record.collection == Collection::Task {
            let task: Task = record.decode()?;
            let mut seen = BTreeSet::from([task.scope.task.clone()]);
            let mut parent = task.parent.clone();
            while let Some(id) = parent {
                if !seen.insert(id.clone()) {
                    return Err(Error::Corruption("task ancestry cycle"));
                }
                let ancestor: Task = state
                    .record(Collection::Task, id.as_str(), &task.scope.workspace)?
                    .decode()?;
                if ancestor.root != task.root || ancestor.scope.session != task.scope.session {
                    return Err(Error::Corruption("task root or session"));
                }
                parent = ancestor.parent;
            }
            let session: Session = state
                .record(
                    Collection::Session,
                    task.scope.session.as_str(),
                    &task.scope.workspace,
                )?
                .decode()?;
            if session.workspace != task.scope.workspace {
                return Err(Error::Access);
            }
        }
    }
    Ok(())
}

fn equivalent(state: &State) {
    assert_eq!(
        super::super::current_validation_candidate::validate_records(
            state.into(),
            &mut crate::historical_facts::StateEventFacts::new(state),
        )
        .map_err(|e| e.to_string()),
        reference(state).map_err(|e| e.to_string())
    );
    assert_eq!(
        state.validate_records().map_err(|e| e.to_string()),
        reference(state).map_err(|e| e.to_string())
    );
}

#[test]
fn current_record_scope_memo_matches_reference_after_mutation_and_corruption() {
    let (mut state, _) = State::default().prepare(&common::initial()).unwrap();
    equivalent(&state);
    for index in 0..32 {
        let mut child = common::task();
        child.scope.task = TaskId::parse(format!("child-{index}")).unwrap();
        child.parent = Some(common::task().scope.task);
        let row = Record::typed(
            Collection::Task,
            child.scope.task.to_string(),
            child.scope.workspace.clone(),
            child.revision,
            &child,
        )
        .unwrap();
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: row,
            }],
            events: vec![],
            command: None,
        };
        state = state.prepare(&tx).unwrap().0;
        equivalent(&state);
        for change in 0..8 {
            let mut invalid = state.clone();
            let row = invalid
                .records
                .get_mut(&key(Collection::Task, child.scope.task.as_str()))
                .unwrap();
            match change {
                0 => row.workspace = WorkspaceId::new(),
                1 => row.value["scope"]["session"] = serde_json::json!("absent-session"),
                2 => row.value["parent"] = serde_json::json!(child.scope.task),
                3 => row.value["root"] = serde_json::json!("absent-root"),
                4 => row.value["scope"]["task"] = serde_json::json!("wrong-id"),
                5 => {
                    row.references.insert("task:absent-task".into());
                }
                6 => row.id = "duplicate-malformed-key".into(),
                7 => row.value["scope"] = serde_json::Value::Null,
                _ => unreachable!(),
            }
            assert!(reference(&invalid).is_err());
            equivalent(&invalid);
        }
        let mut rejected = tx.clone();
        rejected.expected_watermark = state.watermark;
        assert!(state.prepare(&rejected).is_err());
        equivalent(&state);
    }
    // Changed current task dependencies are decoded afresh on the next pass.
    let row = state.records.get_mut("task:task").unwrap();
    row.value["scope"]["session"] = serde_json::json!("absent-session");
    equivalent(&state);
    assert!(reference(&state).is_err());
}

#[test]
fn local_facts_reuse_only_successful_decoding_and_keep_map_keys_distinct() {
    let (state, _) = State::default().prepare(&common::initial()).unwrap();
    let row = state.records.get("task:task").unwrap();
    let mut facts = RecordFacts::default();
    for _ in 0..100 {
        assert_eq!(
            facts.scope("task:task", row).unwrap(),
            row.task_scope().unwrap()
        );
    }
    assert_eq!(facts.scopes.len(), 1);
    let mut invalid = row.clone();
    invalid.value["scope"] = serde_json::Value::Null;
    for _ in 0..2 {
        assert!(facts.scope("different-map-key", &invalid).is_err());
    }
    assert_eq!(facts.scopes.len(), 1);
}
