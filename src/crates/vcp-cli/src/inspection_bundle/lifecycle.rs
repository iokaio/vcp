// SPDX-License-Identifier: Apache-2.0
//! Reopened inspections expose the original owner's bounded timing window.
//! History projection remains the authority for event visibility and scope.
use serde_json::{json, Value};
use std::collections::BTreeSet;
use vcp_domain::workspace::Scope;
use vcp_lifecycle::foundation::execution_diagnostics::{Phase, Snapshot};
use vcp_protocol::event::EventKind;
use vcp_store::contract::State;

pub(super) fn retained(
    state: &State,
    history: &[Value],
    scope: &Scope,
) -> Result<Vec<Value>, String> {
    let visible: BTreeSet<_> = history
        .iter()
        .flat_map(|page| page["rows"].as_array().into_iter().flatten())
        .filter(|row| {
            row["visibility"] == "retained_raw_history" && row["event"]["redaction"].is_null()
        })
        .filter_map(|row| row["event"]["event"]["id"].as_str())
        .collect();
    let mut retained = Vec::new();
    for envelope in &state.events {
        let event = &envelope.event;
        if event.kind != EventKind::Diagnostic
            || !visible.contains(event.id.as_str())
            || envelope.redaction.is_some()
            || event.workspace != scope.workspace
            || event.session != scope.session
            || event.task.as_ref() != Some(&scope.task)
            || event.data.get("execution_diagnostics").is_none()
        {
            continue;
        }
        retained.push(validate(envelope, scope)?);
    }
    Ok(retained)
}

fn validate(envelope: &vcp_protocol::event::EventEnvelope, scope: &Scope) -> Result<Value, String> {
    let event = &envelope.event;
    if event.data["version"] != 1 || event.data["capture_boundary"] != "owner_drained" {
        return Err("unsupported retained execution diagnostics".into());
    }
    let snapshot: Snapshot = serde_json::from_value(event.data["execution_diagnostics"].clone())
        .map_err(|_| "invalid retained execution diagnostics")?;
    let source_watermark: vcp_domain::Watermark =
        serde_json::from_value(event.data["source_watermark"].clone())
            .map_err(|_| "invalid diagnostic source watermark")?;
    if source_watermark >= envelope.watermark {
        return Err("diagnostic source cut must precede its retention event".into());
    }
    let mut sequences = BTreeSet::new();
    if snapshot.schema_version != 1
        || snapshot.owner.is_empty()
        || snapshot.window != "current_owner_only"
        || snapshot.complete_history
        || snapshot.capacity != 256
        || snapshot.observations.len() > snapshot.capacity
        || snapshot.encodings.len() > 64
        || (!snapshot.encoding_available
            && (!snapshot.encodings.is_empty() || snapshot.encodings_dropped != 0))
        || snapshot.encodings.iter().any(|encoding| {
            encoding.scope != *scope
                || encoding
                    .started_micros
                    .checked_add(encoding.elapsed_micros)
                    .is_none_or(|end| end > snapshot.snapshot_micros)
                || encoding.work.encode_failures > encoding.work.encode_calls
                || encoding.work.validation_failures > encoding.work.validation_calls
                || encoding.work.encode_micros > encoding.elapsed_micros
                || encoding.work.validation_micros > encoding.elapsed_micros
                || encoding.catalog.len() != 64
                || !encoding.catalog.bytes().all(|b| b.is_ascii_hexdigit())
                || encoding.request_sha256.as_ref().is_some_and(|digest| {
                    digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit())
                })
        })
        || snapshot.observations.iter().any(|observation| {
            observation.scope != *scope
                || !sequences.insert(observation.sequence)
                || observation
                    .started_micros
                    .checked_add(observation.elapsed_micros)
                    .is_none_or(|end| end > snapshot.snapshot_micros)
                || observation.call_id.as_ref().is_some_and(|id| {
                    observation.phase != Phase::ToolDispatch
                        || id.is_empty()
                        || id.len() > 256
                        || id.chars().any(char::is_control)
                })
        })
    {
        return Err("retained execution diagnostic scope or window invalid".into());
    }
    // General history summaries truncate large payloads. Resolve only IDs
    // already authorized above, preserving compacted/purged visibility.
    Ok(json!({"event":event.id,"watermark":envelope.watermark,
            "source_watermark":source_watermark,
            "capture_boundary":"owner_drained", "snapshot":snapshot}))
}

pub(super) async fn retained_store(
    reader: &impl vcp_store::CanonicalHistory,
    history: &[Value],
    scope: &Scope,
    remaining_bytes: usize,
) -> Result<Vec<Value>, String> {
    let watermark = reader.current().watermark;
    let mut retained = Vec::new();
    let mut retained_bytes = 0usize;
    let mut seen = BTreeSet::new();
    // Query pages preserve global ordinal order. Resolve only visible Diagnostic
    // identities, never masked/compacted rows or an unscoped historical scan.
    for row in history
        .iter()
        .flat_map(|page| page["rows"].as_array().into_iter().flatten())
    {
        if row["visibility"] != "retained_raw_history"
            || !row["event"]["redaction"].is_null()
            || row["event"]["event"]["kind"] != "diagnostic"
        {
            continue;
        }
        let id = vcp_domain::EventId::parse(
            row["event"]["event"]["id"]
                .as_str()
                .ok_or("visible diagnostic identity missing")?,
        )
        .map_err(|e| e.to_string())?;
        if !seen.insert(id.clone()) {
            return Err("duplicate visible diagnostic identity".into());
        }
        let envelope = reader
            .history_event(&id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("visible diagnostic event missing")?;
        let event = &envelope.event;
        if event.id != id || envelope.watermark > watermark {
            return Err("visible diagnostic identity or source cut mismatch".into());
        }
        if event.kind != EventKind::Diagnostic
            || envelope.redaction.is_some()
            || event.workspace != scope.workspace
            || event.session != scope.session
            || event.task.as_ref() != Some(&scope.task)
        {
            return Err("visible diagnostic scope mismatch".into());
        }
        if event.data.get("execution_diagnostics").is_some() {
            let value = validate(&envelope, scope)?;
            retained_bytes = retained_bytes
                .checked_add(serde_json::to_vec(&value).map_err(|e| e.to_string())?.len())
                .ok_or("inspection bundle byte limit exceeded")?;
            if retained_bytes > remaining_bytes {
                return Err("inspection bundle byte limit exceeded by retained diagnostics".into());
            }
            retained.push(value);
        }
    }
    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopened_bundle_resolves_bounded_summary_only_through_visible_scoped_history() {
        let (mut state, access, task) = super::super::tests::fixture();
        let scope: Scope = state
            .record(
                vcp_store::contract::Collection::Task,
                task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode::<vcp_domain::task::Task>()
            .unwrap()
            .scope;
        let event = state.events.last_mut().unwrap();
        event.event.kind = EventKind::Diagnostic;
        event.event.data = json!({"version":1,"capture_boundary":"owner_drained","source_watermark":"0",
            "execution_diagnostics":{"schema_version":1,"owner":"prior-owner","window":"current_owner_only",
                "snapshot_micros":1000,"complete_history":false,"available":true,"capacity":256,"dropped":3,
                "observations":(0..100).map(|sequence| json!({"sequence":sequence,"phase":"provider_exchange",
                    "scope":scope,"turn":null,"attempt":null,"started_micros":10,"elapsed_micros":20,"status":"interrupted"})).collect::<Vec<_>>()}});
        let id = event.event.id.clone();
        let bundle = super::super::collect(&state, &access, &task).unwrap();
        assert_eq!(
            bundle["retained_lifecycle_diagnostics"][0]["event"],
            id.as_str()
        );
        assert_eq!(
            bundle["retained_lifecycle_diagnostics"][0]["snapshot"]["observations"]
                .as_array()
                .unwrap()
                .len(),
            100
        );
        let mut history = bundle["history"].as_array().unwrap().clone();
        let row = history
            .iter_mut()
            .flat_map(|page| page["rows"].as_array_mut().unwrap())
            .find(|row| row["event"]["event"]["id"] == id.as_str())
            .unwrap();
        assert_eq!(row["content_truncated"], true);
        row["visibility"] = json!("purged");
        assert!(retained(&state, &history, &scope).unwrap().is_empty());
        let row = history
            .iter_mut()
            .flat_map(|page| page["rows"].as_array_mut().unwrap())
            .find(|row| row["event"]["event"]["id"] == id.as_str())
            .unwrap();
        row["visibility"] = json!("compacted_presentation_raw_retained");
        assert!(retained(&state, &history, &scope).unwrap().is_empty());
    }

    #[test]
    fn retained_snapshot_rejects_foreign_observations() {
        let (mut state, access, task) = super::super::tests::fixture();
        let scope: Scope = state
            .record(
                vcp_store::contract::Collection::Task,
                task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode::<vcp_domain::task::Task>()
            .unwrap()
            .scope;
        let mut foreign = scope.clone();
        foreign.task = vcp_domain::TaskId::new();
        let event = state.events.last_mut().unwrap();
        event.event.kind = EventKind::Diagnostic;
        event.event.data = json!({"version":1,"capture_boundary":"owner_drained","source_watermark":"0",
            "execution_diagnostics":{"schema_version":1,"owner":"prior-owner","window":"current_owner_only",
                "snapshot_micros":100,"complete_history":false,"available":true,"capacity":256,"dropped":0,
                "observations":[{"sequence":1,"phase":"verification","scope":foreign,"turn":null,"attempt":null,
                    "started_micros":0,"elapsed_micros":10,"status":"succeeded"}]}});
        assert!(super::super::collect(&state, &access, &task)
            .unwrap_err()
            .contains("scope or window"));
    }

    #[test]
    fn retained_encoding_work_preserves_availability_and_rejects_scope_or_counter_spoofing() {
        let (mut state, access, task) = super::super::tests::fixture();
        let scope = state
            .record(
                vcp_store::contract::Collection::Task,
                task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode::<vcp_domain::task::Task>()
            .unwrap()
            .scope;
        let row = json!({"scope":scope,"turn":null,"attempt":null,"purpose":"sealed_validation",
            "catalog":"a".repeat(64),"reasoning":null,"request_sha256":"b".repeat(64),
            "started_micros":10,"elapsed_micros":50,
            "work":{"encode_calls":1,"encode_failures":0,"encoded_bytes":1000,"encode_micros":20,
                "validation_calls":1,"validation_failures":0,"validation_micros":30}});
        let event = state.events.last_mut().unwrap();
        event.event.kind = EventKind::Diagnostic;
        event.event.data = json!({"version":1,"capture_boundary":"owner_drained","source_watermark":"0",
            "execution_diagnostics":{"schema_version":1,"owner":"prior-owner","window":"current_owner_only",
                "snapshot_micros":100,"complete_history":false,"available":true,"capacity":256,"dropped":0,
                "observations":[],"encoding_available":true,"encodings_dropped":2,"encodings":[row]}});
        let expected = event.event.data.clone();
        let bundle = super::super::collect(&state, &access, &task).unwrap();
        assert_eq!(
            bundle["retained_lifecycle_diagnostics"][0]["snapshot"]["encoding_available"],
            true
        );
        assert_eq!(
            bundle["retained_lifecycle_diagnostics"][0]["snapshot"]["encodings"][0]["work"]
                ["encoded_bytes"],
            1000
        );
        for mutation in 0..5 {
            let mut data = expected.clone();
            let snapshot = &mut data["execution_diagnostics"];
            match mutation {
                0 => snapshot["encodings"][0]["scope"]["task"] = json!(vcp_domain::TaskId::new()),
                1 => snapshot["encodings"][0]["work"]["encode_failures"] = json!(2),
                2 => snapshot["encodings"][0]["work"]["validation_micros"] = json!(51),
                3 => snapshot["encoding_available"] = json!(false),
                _ => snapshot["encodings"] = json!(vec![row.clone(); 65]),
            }
            state.events.last_mut().unwrap().event.data = data;
            assert!(super::super::collect(&state, &access, &task)
                .unwrap_err()
                .contains("scope or window"));
        }
    }
}
