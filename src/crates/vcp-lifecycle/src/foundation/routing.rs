// SPDX-License-Identifier: Apache-2.0
//! Explicit local routing configuration. Catalog evidence cannot grant authority.
use super::routing_state::{self, Edit, Preview, Question};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vcp_domain::{CommandId, Revision, Timestamp};
use vcp_models::{
    catalog::Snapshot,
    routing::{CatalogRevision, CostEstimate, Policy},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    Compare {
        baseline: String,
        current: String,
    },
    Report {
        from: Option<Timestamp>,
        until: Timestamp,
    },
    Answer {
        expected: Option<Revision>,
        question: Question,
        value: String,
    },
    Preview {
        report: String,
        selected: Vec<Edit>,
    },
    Apply {
        command: CommandId,
        preview: Box<Preview>,
    },
    Rollback {
        command: CommandId,
        expected: Revision,
        target: Revision,
    },
}
/// Invoked only by an authenticated current local owner. No request grants
/// provider access, starts a model, deletes evidence or raises a spending cap.
pub async fn execute(
    store: &mut vcp_store::Store,
    access: &vcp_memory::access::Access,
    request: Request,
    ceilings: Option<&Policy>,
    now: Timestamp,
) -> Result<serde_json::Value, String> {
    let value = match request {
        Request::Compare { baseline, current } => serde_json::to_value(
            routing_state::compare_reports(store, access, &baseline, &current)?,
        )
        .map_err(|e| e.to_string())?,
        Request::Status => {
            let interview = routing_state::interview(store, access)?;
            serde_json::json!({"registry": routing_state::current_registry(store, access)?, "policy": routing_state::current_policy(store, access)?, "next_question":routing_state::next_question(&interview), "interview":interview, "remote_advice":"disabled; no qualified host dispatch"})
        }
        Request::Report { from, until } => {
            let report = routing_state::save_report(
                store,
                access,
                routing_state::HistoryWindow { from, until },
                now,
            )
            .await?;
            let interview = routing_state::interview(store, access)?;
            serde_json::json!({"next_question":routing_state::next_question_for_report(&interview, &report),"report":report,"interview":interview})
        }
        Request::Answer {
            expected,
            question,
            value,
        } => {
            let interview =
                routing_state::answer(store, access, expected, question, value, now).await?;
            serde_json::json!({"next_question":routing_state::next_question(&interview),"interview":interview})
        }
        Request::Preview { report, selected } => serde_json::to_value(routing_state::preview(
            store,
            access,
            &report,
            selected,
            ceilings.ok_or("configure trusted routing ceilings before preview")?,
        )?)
        .map_err(|e| e.to_string())?,
        Request::Apply { command, preview } => serde_json::to_value(
            routing_state::apply(
                store,
                access,
                command,
                &preview,
                ceilings.ok_or("configure trusted routing ceilings before apply")?,
                now,
            )
            .await?,
        )
        .map_err(|e| e.to_string())?,
        Request::Rollback {
            command,
            expected,
            target,
        } => serde_json::to_value(
            routing_state::rollback(
                store,
                access,
                command,
                expected,
                target,
                ceilings.ok_or("configure trusted routing ceilings before rollback")?,
                now,
            )
            .await?,
        )
        .map_err(|e| e.to_string())?,
    };
    Ok(value)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    /// Explicit local policy only; absence preserves ordinary fixed selection.
    #[serde(default)]
    pub escalation: Option<vcp_models::escalation::Policy>,
    pub catalog: CatalogRevision,
    pub policy: Policy,
    pub task_class: String,
    pub estimates: Vec<CostEstimate>,
    /// Exact endpoint-catalog bytes keyed by qualified snapshot ID. Embedded
    /// strings preserve the digest of the observed JSON, including whitespace.
    pub raw_catalogs: BTreeMap<String, String>,
}
impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        self.catalog.validate().map_err(|e| e.to_string())?;
        self.policy.validate().map_err(|e| e.to_string())?;
        if let Some(policy) = &self.escalation {
            policy.validate().map_err(|e| e.to_string())?;
        }
        if self.task_class.is_empty()
            || self.task_class.len() > 128
            || self.estimates.len() > 128
            || self.raw_catalogs.len() > 128
            || self.raw_catalogs.values().map(String::len).sum::<usize>() > 16 * 1024 * 1024
        {
            return Err("bounded routing configuration required".into());
        }
        for candidate in &self.catalog.entries {
            if let Some(snapshot) = &candidate.snapshot {
                let raw = self
                    .raw_catalogs
                    .get(&snapshot.id)
                    .ok_or("routing snapshot needs its original endpoint catalog")?;
                let verified = Snapshot::from_endpoints(
                    raw.as_bytes(),
                    snapshot.observed_at,
                    snapshot.valid_until,
                    snapshot.compatibility.clone(),
                )
                .map_err(|e| e.to_string())?;
                if &verified != snapshot {
                    return Err("routing snapshot differs from captured endpoint catalog".into());
                }
            }
        }
        Ok(())
    }
}

impl super::CanonicalHost {
    pub fn configure_routing(&self, configuration: Configuration) -> Result<(), String> {
        self.worker
            .run(move |context| context.configure_routing(configuration))
    }
    pub fn routing_control(&self, request: Request) -> Result<serde_json::Value, String> {
        self.worker
            .run(move |context| context.routing_control(request))
    }
}
