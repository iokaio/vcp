// SPDX-License-Identifier: Apache-2.0
//! Index only already-authorized projections. A missing causal parent is a gap,
//! never permission to query another task or infer an unobserved operation.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn index(history: &[Value]) -> Result<Value, String> {
    let mut nodes = Vec::new();
    let mut ids = BTreeSet::new();
    let mut commands: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut artifacts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut records: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut gaps = Vec::new();
    for (page_index, page) in history.iter().enumerate() {
        let rows = page["rows"]
            .as_array()
            .ok_or("diagnostic history rows missing")?;
        if page["gaps"].as_array().is_some_and(|g| !g.is_empty()) {
            gaps.push(
                json!({"kind":"history_visibility","source":format!("/history/{page_index}/gaps")}),
            );
        }
        for (row_index, row) in rows.iter().enumerate() {
            let envelope = &row["event"];
            let event = &envelope["event"];
            let id = event["id"]
                .as_str()
                .ok_or("diagnostic event identity missing")?;
            if !ids.insert(id.to_owned()) {
                return Err("duplicate diagnostic event identity".into());
            }
            let command = event["correlation"]
                .as_str()
                .ok_or("diagnostic command identity missing")?;
            commands.entry(command.into()).or_default().push(id.into());
            // Recorded identities can join observations even where the engine
            // did not populate causation. They are associations, not invented
            // causal edges, and never trigger another evidence read.
            if let Some(facts) = event["data"]["facts"].as_array() {
                for fact in facts {
                    if let (Some(collection), Some(record)) = (fact["collection"].as_str(), fact["id"].as_str()) {
                        records.entry(format!("{collection}:{record}")).or_default().push(id.into());
                    }
                }
            }
            for collection in ["attempt", "reservation", "settlement", "verification"] {
                if let Some(record) = event["data"][collection]["id"].as_str() {
                    records.entry(format!("{collection}:{record}")).or_default().push(id.into());
                }
            }
            if let Some(links) = row["artifact_links"].as_array() {
                for link in links {
                    let artifact = link["id"]
                        .as_str()
                        .ok_or("diagnostic artifact identity missing")?;
                    artifacts
                        .entry(artifact.into())
                        .or_default()
                        .push(id.into());
                    if link["availability"].as_str() != Some("retained") {
                        gaps.push(json!({"kind":"artifact_visibility","event":id,"artifact":artifact,
                            "availability":link["availability"],"source":format!("/history/{page_index}/rows/{row_index}/artifact_links")}));
                    }
                }
            }
            if row["content_truncated"].as_bool() == Some(true) || !envelope["redaction"].is_null()
            {
                gaps.push(json!({"kind":"event_content_incomplete","event":id}));
            }
            nodes.push(json!({"event":id,"kind":event["kind"],"scope":{
                "workspace":event["workspace"],"session":event["session"],"task":event["task"]},
                "command":command,"causation":event["causation"],"watermark":envelope["watermark"],
                "sequence":envelope["sequence"],"source":format!("/history/{page_index}/rows/{row_index}/event")}));
        }
    }
    for node in &nodes {
        if let Some(parent) = node["causation"].as_str() {
            if !ids.contains(parent) {
                gaps.push(json!({"kind":"causal_parent_outside_projection","event":node["event"],"parent":parent}));
            }
        }
    }
    Ok(
        json!({"schema_version":1,"kind":"execution_evidence_index","nodes":nodes,
        "commands":commands,"artifacts":artifacts,"records":records,"gaps":gaps,
        "record_relationships":"shared_record_identity_only",
        "causality":"explicit_event_links_only","quality_verdict":"not_inferred",
        "analysis_status":"required","scope":"authorized_history_projection"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, cause: Option<&str>, kind: &str) -> Value {
        json!({"event":{"event":{"id":id,"correlation":"command-1","causation":cause,
            "workspace":"workspace","session":"session","task":"task","kind":kind},
            "watermark":"4","sequence":"1"},"artifact_links":[],"content_truncated":false})
    }

    #[test]
    fn reconstructs_failure_repair_check_chain_by_explicit_links_not_time() {
        let pages = [
            json!({"rows":[row("failed",None,"verification_recorded"),
            row("repair",Some("failed"),"effect_transition")],"gaps":[]}),
            json!({"rows":[row("passed",Some("repair"),"verification_recorded")],"gaps":[]}),
        ];
        let evidence = index(&pages).unwrap();
        assert_eq!(evidence["nodes"][2]["causation"], "repair");
        assert_eq!(evidence["nodes"][2]["source"], "/history/1/rows/0/event");
        assert_eq!(
            evidence["commands"]["command-1"],
            json!(["failed", "repair", "passed"])
        );
        assert!(evidence["gaps"].as_array().unwrap().is_empty());
        assert_eq!(evidence["quality_verdict"], "not_inferred");
    }

    #[test]
    fn preserves_visibility_gaps_without_fetching_or_copying_private_payloads() {
        let mut event = row("visible", Some("outside"), "diagnostic");
        event["event"]["event"]["data"] = json!({"private_payload":"must_not_be_duplicated"});
        event["content_truncated"] = true.into();
        event["artifact_links"] = json!([{"id":"artifact","availability":"redacted"}]);
        let evidence = index(&[json!({"rows":[event],"gaps":[{"reason":"retention"}]})]).unwrap();
        assert_eq!(evidence["gaps"].as_array().unwrap().len(), 4);
        assert!(!evidence.to_string().contains("must_not_be_duplicated"));
        assert_eq!(evidence["artifacts"]["artifact"], json!(["visible"]));
    }

    #[test]
    fn refuses_ambiguous_identity() {
        let event = row("same", None, "diagnostic");
        assert!(index(&[json!({"rows":[event.clone(),event],"gaps":[]})]).is_err());
    }

    #[test]
    fn joins_retained_attempt_and_effect_observations_without_inventing_causation() {
        let mut admitted = row("admitted", None, "reservation_created");
        admitted["event"]["event"]["data"] = json!({"attempt":{"id":"attempt-1"},"reservation":{"id":"reservation-1"}});
        let mut sent = row("sent", None, "attempt_submitted");
        sent["event"]["event"]["data"] = json!({"attempt":{"id":"attempt-1"}});
        let mut effect = row("effect", None, "effect_transition");
        effect["event"]["event"]["data"] = json!({"facts":[{"collection":"effect","id":"effect-1","value":{"private":"not copied"}}]});
        let evidence = index(&[json!({"rows":[admitted,sent,effect],"gaps":[]})]).unwrap();
        assert_eq!(evidence["records"]["attempt:attempt-1"], json!(["admitted","sent"]));
        assert_eq!(evidence["records"]["effect:effect-1"], json!(["effect"]));
        assert!(evidence["nodes"][1]["causation"].is_null());
        assert!(!evidence.to_string().contains("not copied"));
    }
}
