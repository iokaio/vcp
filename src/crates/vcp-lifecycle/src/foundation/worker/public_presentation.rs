// SPDX-License-Identifier: Apache-2.0
//! Typed retained routing selection; never infer a model from configured policy.
use super::*;
use vcp_domain::artifact::{ArtifactDescriptor, CaptureState, Channel};
use vcp_protocol::methods;

pub(super) fn model(
    context: &Context,
    access: &vcp_engine::Access,
    request: &methods::Inspect,
    page: &mut methods::TaskPresentation,
) {
    let state = context.engine.store().state();
    let Some(artifact) = latest_routing(state, access, request) else {
        return;
    };
    let artifact = &artifact;
    if artifact.spec.source != "retained-codex"
        || artifact.spec.channel != Channel::Evidence
        || artifact.state != CaptureState::Complete
        || artifact.length.get() > 65536
        || artifact.spec.omissions.iter().any(|o| {
            !matches!(
                o,
                vcp_domain::artifact::Omission::AuthenticationHeaders
                    | vcp_domain::artifact::Omission::RecoveryMaterial
            )
        })
    {
        return;
    }
    let Ok(id) = artifact.spec.id.to_string().try_into() else {
        return;
    };
    if context
        .engine
        .public_artifact(
            access,
            &methods::ArtifactRead {
                scope: request.scope.clone(),
                task: request.task.clone(),
                artifact: id,
                offset: 0.into(),
                length: 1,
            },
        )
        .is_err()
    {
        return;
    }
    let mut bytes = Vec::new();
    if context
        .engine
        .store()
        .spool()
        .read(artifact, &mut bytes)
        .is_err()
    {
        return;
    }
    if let Some(model) = selected_model(
        &bytes,
        access.workspace.as_str(),
        request.task.as_str(),
        page.task.steering_revision.as_str(),
        page.task.root.as_str(),
    ) {
        page.model = model;
    }
}
fn latest_routing(
    state: &vcp_store::contract::State,
    access: &vcp_engine::Access,
    request: &methods::Inspect,
) -> Option<ArtifactDescriptor> {
    let workspace: vcp_domain::workspace::Workspace = state
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .ok()?
        .decode()
        .ok()?;
    let mut masks = Vec::new();
    for row in state
        .records
        .values()
        .filter(|row| row.collection == Collection::Tombstone && row.workspace == access.workspace)
    {
        let mask: vcp_domain::retention::RetentionMask = row.decode().ok()?;
        mask.validate().ok()?;
        if mask.workspace != access.workspace || mask.deletion > workspace.deletion {
            return None;
        }
        masks.push(mask);
    }
    // Any hidden/unreadable newer task history makes chronology unknown. Do not
    // skip missing descriptors and accidentally present an older selected model.
    for row in state.events.iter().rev().filter(|row| {
        row.event.workspace == access.workspace
            && row.event.session == access.session
            && row
                .event
                .task
                .as_ref()
                .is_some_and(|task| task.as_str() == request.task.as_str())
    }) {
        if row.redaction.is_some()
            || masks.iter().any(|mask| {
                mask.session == access.session
                    && row.sequence >= mask.first
                    && row.sequence <= mask.last
            })
        {
            return None;
        }
        let mut selected = None;
        for id in &row.event.artifacts {
            let artifact: ArtifactDescriptor = state
                .record(Collection::Artifact, id.as_str(), &access.workspace)
                .ok()?
                .decode()
                .ok()?;
            artifact.validate().ok()?;
            if artifact.spec.id != *id
                || artifact.spec.scope.workspace != access.workspace
                || artifact.spec.scope.session != access.session
                || artifact.spec.scope.task.as_str() != request.task.as_str()
            {
                return None;
            }
            if artifact.spec.schema == "routing-selection/1" {
                if selected.is_some() || masks.iter().any(|mask| mask.artifacts.contains(id)) {
                    return None;
                }
                selected = Some(artifact);
            }
        }
        if selected.is_some() && row.event.kind == vcp_protocol::event::EventKind::ArtifactAttached
        {
            return selected;
        }
    }
    None
}

fn selected_model(
    bytes: &[u8],
    workspace: &str,
    task: &str,
    steering: &str,
    root: &str,
) -> Option<methods::PresentationModel> {
    let decision: vcp_models::routing::RoutingDecision = serde_json::from_slice(bytes).ok()?;
    decision.validate().ok()?;
    if decision.input.workspace.as_str() != workspace
        || decision.input.root.as_str() != root
        || decision.input.task.as_str() != task
        || decision.input.steering.get().to_string() != steering
    {
        return None;
    }
    let selected = decision.selected?;
    let candidate = decision
        .candidates
        .iter()
        .find(|candidate| candidate.identity == selected && candidate.exclusions.is_empty())?;
    if selected.model.is_empty()
        || selected.model.chars().count() > 4096
        || selected.model.chars().any(char::is_control)
    {
        return None;
    }
    Some(methods::PresentationModel {
        id: Some(methods::PresentationText {
            text: selected.model,
            truncated: false,
        }),
        group: candidate.group.map(|group| methods::PresentationText {
            text: match group {
                vcp_models::routing::Group::Frontier => "frontier",
                vcp_models::routing::Group::High => "high",
                vcp_models::routing::Group::Medium => "medium",
                vcp_models::routing::Group::Low => "low",
            }
            .into(),
            truncated: false,
        }),
        source: methods::PresentationSource::Observed,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_selection_rejects_wrong_scope_steering_and_invalid_identity() {
        let mut decision: vcp_models::routing::RoutingDecision = serde_json::from_value(serde_json::json!({
            "schema_version":1,"id":"", "input": {
                "workspace":"workspace","root":"task","task":"task","input_revision":"0","steering":"0",
                "input_digest":"c".repeat(64),"catalog":"d".repeat(64),"policy":"e".repeat(64),
                "role":"main","task_class":"fixture","now":"1","required_capabilities":[],"excluded":[],
                "input_tokens":"1","output_tokens":"1","available":{"currency":"USD","micros":"1000"},
                "protected_verification":"0","estimates":[]
            },"profile":"low","ordering":["total_cost","latency","quality","capability"],
            "candidates":[{"identity":{"model":"observed/model","endpoint":"qualified-endpoint"},"exclusions":[],"source_reasons":[],"group":"low","quality_bps":9000,"samples":20,"latency_p95_ms":10,"total_estimate":null,"evidence_refs":[],"broader_cohort_used":null,"assumptions":[]}],
            "selected":{"model":"observed/model","endpoint":"qualified-endpoint"},"fallback_from":null,"immediate_reservation":null
        })).unwrap();
        decision.id = decision.digest().unwrap();
        let bytes = serde_json::to_vec(&decision).unwrap();
        let model = selected_model(&bytes, "workspace", "task", "0", "task").unwrap();
        assert_eq!(model.id.unwrap().text, "observed/model");
        assert_eq!(model.group.unwrap().text, "low");
        assert!(selected_model(&bytes, "foreign", "task", "0", "task").is_none());
        assert!(selected_model(&bytes, "workspace", "other", "0", "task").is_none());
        assert!(selected_model(&bytes, "workspace", "task", "1", "task").is_none());
        decision.id = "f".repeat(64);
        assert!(selected_model(
            &serde_json::to_vec(&decision).unwrap(),
            "workspace",
            "task",
            "0",
            "task"
        )
        .is_none());
    }
    #[test]
    fn latest_routing_does_not_fall_back_across_hidden_or_missing_history() {
        use vcp_domain::{
            artifact::ArtifactSpec,
            retention::RetentionMask,
            workspace::{Binding, Trust, Workspace},
        };
        use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
        use vcp_store::contract::{key, Record, State};
        let access = vcp_engine::Access {
            actor: ActorId::new(),
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: false,
            bootstrap: false,
        };
        let task = TaskId::new();
        let request = methods::Inspect {
            scope: methods::Scope {
                workspace: access.workspace.to_string().try_into().unwrap(),
                session: access.session.to_string().try_into().unwrap(),
            },
            task: task.to_string().try_into().unwrap(),
            target: None,
            cursor: None,
            limit: 8,
        };
        let mut state = State::default();
        let workspace = Workspace {
            id: access.workspace.clone(),
            binding: Binding {
                host: HostId::new(),
                root: "fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
            trust: Trust::Untrusted,
            revision: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
        };
        state.records.insert(
            key(Collection::Workspace, access.workspace.as_str()),
            Record::typed(
                Collection::Workspace,
                access.workspace.to_string(),
                access.workspace.clone(),
                Revision::ZERO,
                &workspace,
            )
            .unwrap(),
        );
        for (name, sequence) in [("z-earlier", 1), ("a-later", 2)] {
            let id = ArtifactId::parse(name).unwrap();
            let artifact = ArtifactDescriptor {
                spec: ArtifactSpec {
                    id: id.clone(),
                    scope: Scope {
                        workspace: access.workspace.clone(),
                        session: access.session.clone(),
                        task: task.clone(),
                    },
                    media_type: "application/json".into(),
                    schema: "routing-selection/1".into(),
                    source: "retained-codex".into(),
                    channel: Channel::Evidence,
                    retention: "history".into(),
                    omissions: vec![],
                },
                state: CaptureState::Complete,
                length: ByteCount::ZERO,
                sha256: "a".repeat(64),
                retained: vec![vcp_domain::artifact::Range {
                    start: ByteCount::ZERO,
                    end: ByteCount::ZERO,
                }],
            };
            state.records.insert(
                key(Collection::Artifact, name),
                Record::typed(
                    Collection::Artifact,
                    name,
                    access.workspace.clone(),
                    Revision::ZERO,
                    &artifact,
                )
                .unwrap(),
            );
            state.events.push(EventEnvelope {
                version: 1,
                sequence: SessionSeq::new(sequence),
                watermark: Watermark::new(sequence),
                redaction: None,
                event: EventInput {
                    id: EventId::new(),
                    workspace: access.workspace.clone(),
                    session: access.session.clone(),
                    task: Some(task.clone()),
                    actor: access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(sequence),
                    kind: EventKind::ArtifactAttached,
                    artifacts: vec![id],
                    data: serde_json::json!({}),
                    metadata: None,
                },
            });
        }
        assert_eq!(
            latest_routing(&state, &access, &request)
                .unwrap()
                .spec
                .id
                .as_str(),
            "a-later"
        );
        let newest = state
            .records
            .remove(&key(Collection::Artifact, "a-later"))
            .unwrap();
        assert!(latest_routing(&state, &access, &request).is_none());
        state
            .records
            .insert(key(Collection::Artifact, "a-later"), newest);
        let mask = RetentionMask {
            schema_version: 1,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            first: SessionSeq::new(2),
            last: SessionSeq::new(2),
            artifacts: vec![],
            deletion: DeletionEpoch::ZERO,
            reason: "hide newest routing event".into(),
        };
        state.records.insert(
            key(Collection::Tombstone, "mask"),
            Record::typed(
                Collection::Tombstone,
                "mask",
                access.workspace.clone(),
                Revision::ZERO,
                &mask,
            )
            .unwrap(),
        );
        assert!(latest_routing(&state, &access, &request).is_none());
    }
}
