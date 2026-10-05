// SPDX-License-Identifier: Apache-2.0
//! Facts from the authorized canonical cut. Accepted settings are historical
//! facts; absent owner-runtime configuration is never inferred from them.
use super::*;
use vcp_domain::{
    accounting::Ledger,
    workspace::{Session, Workspace},
    Limit,
};

fn unknown(reason: &str) -> Value {
    json!({"status":"unknown","reason":reason})
}

pub(super) fn project(
    state: &State,
    access: &Access,
    task: &Task,
    workspace: &Workspace,
) -> Result<Value, String> {
    if task.scope.workspace != access.workspace || workspace.id != access.workspace {
        return Err("constraint projection scope mismatch".into());
    }
    let session: Session = state
        .record(
            Collection::Session,
            task.scope.session.as_str(),
            &access.workspace,
        )
        .and_then(|row| row.decode())
        .map_err(|error| error.to_string())?;
    if session.id != task.scope.session || session.workspace != task.scope.workspace {
        return Err("constraint projection session mismatch".into());
    }
    let can_read_root = access
        .tasks
        .as_ref()
        .is_none_or(|tasks| tasks.contains(&task.root));
    let (ledger, accepted) = if can_read_root {
        let ledger = match state.records.get(&vcp_store::contract::key(
            Collection::Ledger,
            task.root.as_str(),
        )) {
            None => unknown("canonical root ledger is absent"),
            Some(row) => {
                let ledger: Ledger = row.decode().map_err(|error| error.to_string())?;
                if row.workspace != access.workspace
                    || ledger.scope.workspace != access.workspace
                    || ledger.scope.task != task.root
                {
                    return Err("constraint projection ledger mismatch".into());
                }
                json!({"status":"recorded","origin":{"collection":"ledger","id":task.root,"revision":ledger.revision,"policy_revision":ledger.policy},
                    "currency":ledger.currency,"cap":ledger.cap,"protected_micros":ledger.protected,
                    "known_settled_micros":ledger.settled,"active_reserved_micros":ledger.active,
                    "unresolved_reserved_liability_micros":ledger.unresolved,
                    "actual_final_total":unknown("a ledger balance alone does not prove complete final billing observations"),
                    "recorded_daily_policy":ledger.daily.map(|daily| json!({"cap":Limit::Finite(daily.cap),"scope":daily.scope,"utc_offset_minutes":daily.utc_offset_minutes,"applies_under_recorded_root_limit":!ledger.cap.is_unbounded()})),
                    "meaning":"current canonical monetary constraints; reservation liability is not observed final cost"})
            }
        };
        let root: Task = state
            .record(Collection::Task, task.root.as_str(), &access.workspace)
            .and_then(|row| row.decode())
            .map_err(|error| error.to_string())?;
        let accepted = match vcp_engine::public_start::retained_start_budget(state, &root.scope) {
            Ok(Some(start)) => {
                json!({"status":"recorded","origin":"validated public-start acceptance","accepted_at":start.accepted_at,"budget":start.budget,
                "meaning":"original accepted facts, not proof of the current owner runtime configuration"})
            }
            Ok(None) => unknown("proved legacy genesis has no public-start budget"),
            Err(_) => unknown(
                "public-start acceptance is unavailable or cannot be proved from this retained cut",
            ),
        };
        (ledger, accepted)
    } else {
        (
            unknown("root ledger is outside authorized task scope"),
            unknown("root acceptance is outside authorized task scope"),
        )
    };
    let policy = match vcp_engine::policy::optional(state, &access.workspace)
        .map_err(|error| error.to_string())?
    {
        Some(policy) => {
            json!({"status":"recorded","origin":{"collection":"access","id":access.workspace,"policy_revision":policy.revision},
            "autonomy":policy.mode,"process_timeout_ceiling_ms":Limit::Finite(policy.timeout_ceiling_ms),
            "process_output_ceiling_bytes":Limit::Finite(policy.output_ceiling_bytes),
            "meaning":"independent authority/process bounds; not a task deadline"})
        }
        None => unknown("canonical authority policy is absent"),
    };
    Ok(
        json!({"schema_version":1,"source_watermark":state.watermark,"scope":task.scope,
        "revisions":{"task":task.revision,"steering":task.steering,"session":session.revision,"session_configuration":session.configuration,
            "workspace":workspace.revision,"binding":workspace.binding.revision,"authority":workspace.authority,"deletion":workspace.deletion},
        "monetary":ledger,"original_acceptance":accepted,"authority_policy":policy,
        "task_deadline":unknown("active owner coding configuration is not contained in this canonical state projection"),
        "execution_revision":unknown("session configuration revision is not a recorded executable/source revision"),
        "retention_automation":unknown("retention runtime policy is not inferred from record existence"),
        "qualification":"recorded canonical facts only; no enforcement change or live-owner verification"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{Micros, PolicyRevision, Revision};
    use vcp_store::contract::Record;

    #[test]
    fn legacy_finite_and_tagged_unbounded_ledger_limits_remain_distinct_from_unknown() {
        for cap in [json!("12345"), json!({"version":1,"kind":"unbounded"})] {
            let (mut state, access, task) = super::super::tests::fixture();
            let task_record: Task = state
                .record(Collection::Task, task.as_str(), &access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            let ledger = Ledger {
                schema_version: 1,
                scope: task_record.scope,
                revision: Revision::new(2),
                policy: PolicyRevision::new(3),
                currency: "USD".to_owned().try_into().unwrap(),
                cap: Limit::Finite(Micros::new(12345)),
                protected: Micros::ZERO,
                settled: Micros::new(25),
                active: Micros::ZERO.into(),
                unresolved: Micros::new(77).into(),
                allocations: Default::default(),
                daily: Some(vcp_domain::accounting::DailyPolicy {
                    cap: Micros::new(5000),
                    utc_offset_minutes: 0,
                    scope: "local_root".into(),
                }),
                overrun: false,
            };
            let mut row = Record::typed(
                Collection::Ledger,
                task.to_string(),
                access.workspace.clone(),
                ledger.revision,
                &ledger,
            )
            .unwrap();
            row.value["cap"] = cap.clone();
            state.records.insert(row.key(), row);
            let before = serde_json::to_value(&state).unwrap();
            let bundle = collect(&state, &access, &task).unwrap();
            let facts = &bundle["effective_constraints"];
            assert_eq!(
                facts["monetary"]["cap"]["kind"],
                if cap.is_string() {
                    "finite"
                } else {
                    "unbounded"
                }
            );
            if cap.is_string() {
                assert_eq!(facts["monetary"]["cap"]["value"], "12345");
            }
            assert_eq!(
                facts["monetary"]["unresolved_reserved_liability_micros"],
                "77"
            );
            assert_eq!(facts["monetary"]["actual_final_total"]["status"], "unknown");
            assert!(facts["monetary"]["actual_final_total"]
                .get("value")
                .is_none());
            assert_eq!(facts["monetary"]["origin"]["revision"], "2");
            assert_eq!(facts["task_deadline"]["status"], "unknown");
            assert_eq!(
                facts["monetary"]["recorded_daily_policy"]["applies_under_recorded_root_limit"],
                cap.is_string()
            );
            assert_eq!(before, serde_json::to_value(&state).unwrap());
        }
    }

    #[test]
    fn absent_ledger_and_foreign_session_do_not_become_zero_or_unbounded() {
        let (mut state, access, task) = super::super::tests::fixture();
        let bundle = collect(&state, &access, &task).unwrap();
        assert_eq!(
            bundle["effective_constraints"]["monetary"]["status"],
            "unknown"
        );
        assert!(bundle["effective_constraints"]["monetary"]
            .get("cap")
            .is_none());
        let selected: Task = state
            .record(Collection::Task, task.as_str(), &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let session = state
            .records
            .get_mut(&vcp_store::contract::key(
                Collection::Session,
                selected.scope.session.as_str(),
            ))
            .unwrap();
        session.value["workspace"] = json!(vcp_domain::WorkspaceId::new());
        assert!(collect(&state, &access, &task).is_err());
    }

    #[test]
    fn child_only_access_does_not_read_root_financial_records() {
        let (mut state, mut access, task) = super::super::tests::fixture();
        let mut selected: Task = state
            .record(Collection::Task, task.as_str(), &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        selected.root = vcp_domain::TaskId::new();
        access.tasks = Some(std::collections::BTreeSet::from([task]));
        // Malformed hidden data must not even be decoded through this projection.
        let mut hidden = state
            .record(
                Collection::Task,
                selected.scope.task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .clone();
        hidden.collection = Collection::Ledger;
        hidden.id = selected.root.to_string();
        hidden.value = json!({"private":"must not be read"});
        state.records.insert(hidden.key(), hidden);
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let facts = project(&state, &access, &selected, &workspace).unwrap();
        assert_eq!(facts["monetary"]["status"], "unknown");
        assert!(facts["monetary"]["reason"]
            .as_str()
            .unwrap()
            .contains("authorized task scope"));
        assert!(!facts.to_string().contains("must not be read"));
    }
}
