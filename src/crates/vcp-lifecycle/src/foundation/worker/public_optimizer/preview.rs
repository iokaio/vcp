// SPDX-License-Identifier: Apache-2.0
//! Exact bounded review; private interview content never crosses this boundary.
use super::{counter, failure, id, Cached, Code, Proposal, RpcResult};
use crate::foundation::routing_state;
use serde::{de::DeserializeOwned, Serialize};
use vcp_domain::{ByteCount, Units};
use vcp_models::routing as model;
use vcp_protocol::{routing_inspection as status, routing_optimizer as wire};

fn mapped<T: Serialize, U: DeserializeOwned>(value: T) -> RpcResult<U> {
    serde_json::from_value(
        serde_json::to_value(value).map_err(|_| failure(Code::StoreUnavailable))?,
    )
    .map_err(|_| failure(Code::StoreUnavailable))
}
fn native_identity(value: &status::Identity) -> model::ModelEndpoint {
    model::ModelEndpoint {
        model: value.model.clone(),
        endpoint: value.endpoint.clone(),
    }
}
fn identity(value: &model::ModelEndpoint) -> RpcResult<status::Identity> {
    Ok(status::Identity {
        model: tag(&value.model)?,
        endpoint: tag(&value.endpoint)?,
    })
}
fn tag(value: &str) -> RpcResult<String> {
    if value.is_empty()
        || value.len() > 256
        || value.contains("://")
        || value.chars().any(char::is_control)
    {
        Err(failure(Code::ResourceLimit))
    } else {
        Ok(value.to_owned())
    }
}
fn text(value: &str) -> status::Text {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    status::Text {
        text: value[..end].to_owned(),
        truncated: end < value.len(),
    }
}

pub(super) fn edits(values: &[wire::Edit]) -> RpcResult<Vec<routing_state::Edit>> {
    wire::validate_edits(values).map_err(|_| vcp_protocol::jsonrpc::RpcError::invalid_params())?;
    values
        .iter()
        .map(|value| {
            Ok(match value {
                wire::Edit::RetrievalLimits(v) => routing_state::Edit::RetrievalLimits(
                    v.as_ref()
                        .map(|v| -> RpcResult<_> {
                            Ok(model::RetrievalLimits {
                                results: v.results,
                                tokens: Units::new(counter(&v.tokens)?),
                                bytes: ByteCount::new(counter(&v.bytes)?),
                            })
                        })
                        .transpose()?,
                ),
                wire::Edit::InputTokens(v) => routing_state::Edit::InputTokens(
                    v.as_ref().map(|v| counter(v).map(Units::new)).transpose()?,
                ),
                wire::Edit::OutputTokens(v) => routing_state::Edit::OutputTokens(
                    v.as_ref().map(|v| counter(v).map(Units::new)).transpose()?,
                ),
                wire::Edit::EscalationMaxTransportRetries(v) => {
                    routing_state::Edit::EscalationMaxTransportRetries(*v)
                }
                wire::Edit::EscalationMaxQualitySwitches(v) => {
                    routing_state::Edit::EscalationMaxQualitySwitches(*v)
                }
                wire::Edit::EscalationMaxTotalAttempts(v) => {
                    routing_state::Edit::EscalationMaxTotalAttempts(*v)
                }
                wire::Edit::EscalationMinimumRepeatedFailures(v) => {
                    routing_state::Edit::EscalationMinimumRepeatedFailures(*v)
                }
                wire::Edit::ReasoningEffort(v) => {
                    routing_state::Edit::ReasoningEffort(v.map(mapped).transpose()?)
                }
                wire::Edit::Profile(v) => routing_state::Edit::Profile(mapped(v)?),
                wire::Edit::Ordering(v) => routing_state::Edit::Ordering(mapped(v)?),
                wire::Edit::QualityFloorBps(v) => routing_state::Edit::QualityFloorBps(*v),
                wire::Edit::MinimumSamples(v) => routing_state::Edit::MinimumSamples(*v),
                wire::Edit::MaximumEvidenceAgeMs(v) => {
                    routing_state::Edit::MaximumEvidenceAgeMs(counter(v)?)
                }
                wire::Edit::AllowedModels(v) => {
                    routing_state::Edit::AllowedModels(v.iter().cloned().collect())
                }
                wire::Edit::AllowedEndpoints(v) => {
                    routing_state::Edit::AllowedEndpoints(v.iter().cloned().collect())
                }
                wire::Edit::AllowedGroups(v) => routing_state::Edit::AllowedGroups(
                    v.iter().map(mapped).collect::<RpcResult<_>>()?,
                ),
                wire::Edit::Pin(v) => routing_state::Edit::Pin(v.as_ref().map(|v| model::Pin {
                    candidate: native_identity(&v.candidate),
                    fallback_candidates:
                        v.fallback_candidates.iter().map(native_identity).collect(),
                })),
            })
        })
        .collect()
}
fn selected(values: &[routing_state::Edit]) -> RpcResult<Vec<wire::Edit>> {
    let result = values
        .iter()
        .map(|value| {
            Ok(match value {
                routing_state::Edit::RetrievalLimits(v) => {
                    wire::Edit::RetrievalLimits(v.as_ref().map(|v| status::RetrievalLimits {
                        results: v.results,
                        tokens: v.tokens.get().into(),
                        bytes: v.bytes.get().into(),
                    }))
                }
                routing_state::Edit::InputTokens(v) => {
                    wire::Edit::InputTokens(v.map(|v| v.get().into()))
                }
                routing_state::Edit::OutputTokens(v) => {
                    wire::Edit::OutputTokens(v.map(|v| v.get().into()))
                }
                routing_state::Edit::EscalationMaxTransportRetries(v) => {
                    wire::Edit::EscalationMaxTransportRetries(*v)
                }
                routing_state::Edit::EscalationMaxQualitySwitches(v) => {
                    wire::Edit::EscalationMaxQualitySwitches(*v)
                }
                routing_state::Edit::EscalationMaxTotalAttempts(v) => {
                    wire::Edit::EscalationMaxTotalAttempts(*v)
                }
                routing_state::Edit::EscalationMinimumRepeatedFailures(v) => {
                    wire::Edit::EscalationMinimumRepeatedFailures(*v)
                }
                routing_state::Edit::ReasoningEffort(v) => {
                    wire::Edit::ReasoningEffort(v.map(mapped).transpose()?)
                }
                routing_state::Edit::Profile(v) => wire::Edit::Profile(mapped(v)?),
                routing_state::Edit::Ordering(v) => wire::Edit::Ordering(mapped(v)?),
                routing_state::Edit::QualityFloorBps(v) => wire::Edit::QualityFloorBps(*v),
                routing_state::Edit::MinimumSamples(v) => wire::Edit::MinimumSamples(*v),
                routing_state::Edit::MaximumEvidenceAgeMs(v) => {
                    wire::Edit::MaximumEvidenceAgeMs((*v).into())
                }
                routing_state::Edit::AllowedModels(v) => {
                    wire::Edit::AllowedModels(v.iter().cloned().collect())
                }
                routing_state::Edit::AllowedEndpoints(v) => {
                    wire::Edit::AllowedEndpoints(v.iter().cloned().collect())
                }
                routing_state::Edit::AllowedGroups(v) => {
                    wire::Edit::AllowedGroups(v.iter().map(mapped).collect::<RpcResult<_>>()?)
                }
                routing_state::Edit::Pin(v) => wire::Edit::Pin(
                    v.as_ref()
                        .map(|v| -> RpcResult<_> {
                            Ok(wire::Pin {
                                candidate: identity(&v.candidate)?,
                                fallback_candidates: v
                                    .fallback_candidates
                                    .iter()
                                    .map(identity)
                                    .collect::<RpcResult<_>>()?,
                            })
                        })
                        .transpose()?,
                ),
            })
        })
        .collect::<RpcResult<Vec<_>>>()?;
    wire::validate_edits(&result).map_err(|_| failure(Code::ResourceLimit))?;
    Ok(result)
}
fn policy(value: &model::Policy) -> RpcResult<wire::ReviewedPolicy> {
    value
        .validate()
        .map_err(|_| failure(Code::StoreUnavailable))?;
    if value.allowed_models.len() > 64
        || value.allowed_endpoints.len() > 64
        || value.allowed_groups.len() > 4
        || value
            .pin
            .as_ref()
            .is_some_and(|v| v.fallback_candidates.len() > 64)
    {
        return Err(failure(Code::ResourceLimit));
    }
    let summary = status::PolicySummary {
        id: value.id.clone(),
        parent_id: value.parent.clone(),
        profile: mapped(value.profile)?,
        quality_floor_bps: value.quality_floor_bps,
        minimum_samples: value.minimum_samples,
        maximum_evidence_age_ms: value.maximum_evidence_age_ms.into(),
        deny_data_collection: value.deny_data_collection,
        require_zdr: value.require_zdr,
        ordering: mapped(&value.ordering)?,
        pin: value
            .pin
            .as_ref()
            .map(|v| identity(&v.candidate))
            .transpose()?,
        input_tokens: value.input_tokens.map(|v| v.get().into()),
        output_tokens: value.output_tokens.map(|v| v.get().into()),
        reasoning_effort: value.reasoning_effort.map(mapped).transpose()?,
        retrieval_limits: value
            .retrieval_limits
            .as_ref()
            .map(|v| status::RetrievalLimits {
                results: v.results,
                tokens: v.tokens.get().into(),
                bytes: v.bytes.get().into(),
            }),
        escalation_limits: value
            .escalation_limits
            .as_ref()
            .map(|v| status::EscalationLimits {
                max_transport_retries: v.max_transport_retries,
                max_quality_switches: v.max_quality_switches,
                max_total_attempts: v.max_total_attempts,
                minimum_repeated_failures: v.minimum_repeated_failures,
            }),
        broader_task_class: value.broader_task_class.as_deref().map(|v| status::Text {
            text: v.to_owned(),
            truncated: false,
        }),
        allowed_models_count: (value.allowed_models.len() as u64).into(),
        allowed_endpoints_count: (value.allowed_endpoints.len() as u64).into(),
        allowed_groups_count: (value.allowed_groups.len() as u64).into(),
        pin_fallback_count: (value
            .pin
            .as_ref()
            .map_or(0, |v| v.fallback_candidates.len()) as u64)
            .into(),
    };
    Ok(wire::ReviewedPolicy {
        summary,
        allowed_models: value
            .allowed_models
            .iter()
            .map(|v| tag(v))
            .collect::<RpcResult<_>>()?,
        allowed_endpoints: value
            .allowed_endpoints
            .iter()
            .map(|v| tag(v))
            .collect::<RpcResult<_>>()?,
        allowed_groups: value
            .allowed_groups
            .iter()
            .map(mapped)
            .collect::<RpcResult<_>>()?,
        pin_fallback: value
            .pin
            .as_ref()
            .map(|v| {
                v.fallback_candidates
                    .iter()
                    .map(identity)
                    .collect::<RpcResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default(),
    })
}
fn changed(a: &model::Policy, b: &model::Policy) -> Vec<wire::Field> {
    use wire::Field::*;
    let mut out = Vec::new();
    macro_rules! field {
        ($f:ident,$v:ident) => {
            if a.$f != b.$f {
                out.push($v);
            }
        };
    }
    field!(retrieval_limits, RetrievalLimits);
    field!(input_tokens, InputTokens);
    field!(output_tokens, OutputTokens);
    field!(reasoning_effort, ReasoningEffort);
    field!(profile, Profile);
    field!(ordering, Ordering);
    field!(quality_floor_bps, QualityFloorBps);
    field!(minimum_samples, MinimumSamples);
    field!(maximum_evidence_age_ms, MaximumEvidenceAgeMs);
    field!(allowed_models, AllowedModels);
    field!(allowed_endpoints, AllowedEndpoints);
    field!(allowed_groups, AllowedGroups);
    field!(pin, Pin);
    field!(deny_data_collection, DenyDataCollection);
    field!(require_zdr, RequireZdr);
    field!(broader_task_class, BroaderTaskClass);
    macro_rules! escalation {
        ($f:ident,$v:ident) => {
            if a.escalation_limits.as_ref().and_then(|v| v.$f)
                != b.escalation_limits.as_ref().and_then(|v| v.$f)
            {
                out.push($v);
            }
        };
    }
    escalation!(max_transport_retries, EscalationMaxTransportRetries);
    escalation!(max_quality_switches, EscalationMaxQualitySwitches);
    escalation!(max_total_attempts, EscalationMaxTotalAttempts);
    escalation!(minimum_repeated_failures, EscalationMinimumRepeatedFailures);
    out
}
pub(super) fn view(cached: &Cached) -> RpcResult<wire::PreviewView> {
    let (operation, base, target, prior, persisted, effective, ceilings, selected, uncertainty) =
        match &cached.proposal {
            Proposal::Apply(v) => (
                wire::Operation::Apply,
                v.base,
                None,
                &v.prior,
                &v.persisted,
                &v.effective,
                &v.ceilings_digest,
                selected(&v.selected)?,
                v.uncertainty.as_slice(),
            ),
            Proposal::Rollback(v) => (
                wire::Operation::Rollback,
                v.base,
                Some(v.target),
                &v.prior,
                &v.persisted,
                &v.effective,
                &v.ceilings_digest,
                Vec::new(),
                &[][..],
            ),
        };
    let expires = cached
        .expires
        .saturating_duration_since(std::time::Instant::now())
        .as_millis();
    if expires == 0 {
        return Err(failure(Code::CursorGap));
    }
    let value = wire::PreviewView {
        scope: cached.scope.clone(),
        preview_id: id(&cached.id)?,
        preview_sha256: cached.sha256.clone(),
        operation,
        report: cached.report.clone(),
        base_policy_revision: base.get().into(),
        target_policy_revision: target.map(|v| v.get().into()),
        authority_revision: cached.workspace.authority.get().into(),
        deletion_revision: cached.workspace.deletion.get().into(),
        binding_revision: cached.workspace.binding.revision.get().into(),
        ceilings_sha256: ceilings.clone(),
        expires_in_ms: expires.min(60000) as u32,
        prior: policy(prior)?,
        persisted: policy(persisted)?,
        effective: policy(effective)?,
        selected,
        changed: changed(prior, persisted),
        clamped: changed(persisted, effective),
        uncertainty: uncertainty.iter().take(32).map(|v| text(v)).collect(),
        uncertainty_truncated: uncertainty.len() > 32,
        assessment: wire::Assessment::NotDispatchAuthority,
    };
    if serde_json::to_vec(&value)
        .map_err(|_| failure(Code::StoreUnavailable))?
        .len()
        > wire::MAX_PREVIEW_BYTES
    {
        return Err(failure(Code::ResourceLimit));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_edits_roundtrip_exact_values_and_clamp_fields() {
        let values:Vec<wire::Edit>=serde_json::from_value(serde_json::json!([
            {"field":"retrieval_limits","value":{"results":4,"tokens":"2048","bytes":"4096"}},
            {"field":"input_tokens","value":"9007199254740993"},
            {"field":"output_tokens","value":null},
            {"field":"escalation_max_transport_retries","value":0},
            {"field":"escalation_max_quality_switches","value":2},
            {"field":"escalation_max_total_attempts","value":4},
            {"field":"escalation_minimum_repeated_failures","value":2},
            {"field":"reasoning_effort","value":"minimal"},
            {"field":"profile","value":"low"},
            {"field":"ordering","value":["total_cost","latency","quality","capability"]},
            {"field":"quality_floor_bps","value":9000},
            {"field":"minimum_samples","value":21},
            {"field":"maximum_evidence_age_ms","value":"1000"},
            {"field":"allowed_models","value":["a","b"]},
            {"field":"allowed_endpoints","value":["endpoint"]},
            {"field":"allowed_groups","value":["low"]},
            {"field":"pin","value":{"candidate":{"model":"a","endpoint":"endpoint"},"fallback_candidates":[{"model":"b","endpoint":"endpoint"}]}}
        ])).unwrap();
        assert_eq!(selected(&edits(&values).unwrap()).unwrap(), values);
        let mut duplicate = values;
        duplicate.push(duplicate[0].clone());
        assert!(edits(&duplicate).is_err());
    }
    #[test]
    fn policy_review_preserves_full_sets_and_rejects_unreviewable_large_sets() {
        use std::collections::BTreeSet;
        let prior = model::Policy {
            schema_version: 1,
            id: String::new(),
            parent: None,
            profile: model::Profile::Low,
            allowed_models: BTreeSet::from(["a".into(), "b".into()]),
            allowed_endpoints: BTreeSet::from(["endpoint".into()]),
            allowed_groups: BTreeSet::from([model::Group::Low]),
            quality_floor_bps: 8000,
            minimum_samples: 20,
            maximum_evidence_age_ms: 1000,
            deny_data_collection: true,
            require_zdr: true,
            ordering: vec![
                model::Preference::TotalCost,
                model::Preference::Latency,
                model::Preference::Quality,
                model::Preference::Capability,
            ],
            pin: Some(model::Pin {
                candidate: model::ModelEndpoint {
                    model: "a".into(),
                    endpoint: "endpoint".into(),
                },
                fallback_candidates: BTreeSet::from([model::ModelEndpoint {
                    model: "b".into(),
                    endpoint: "endpoint".into(),
                }]),
            }),
            broader_task_class: Some("retained-class".into()),
            output_tokens: None,
            input_tokens: None,
            escalation_limits: None,
            reasoning_effort: None,
            retrieval_limits: None,
        }
        .seal()
        .unwrap();
        let shown = policy(&prior).unwrap();
        assert_eq!(shown.allowed_models, vec!["a", "b"]);
        assert_eq!(shown.pin_fallback.len(), 1);
        assert_eq!(
            shown.summary.broader_task_class.unwrap().text,
            "retained-class"
        );
        let mut effective = prior.clone();
        effective.input_tokens = Some(Units::new(10));
        effective.require_zdr = false;
        effective.pin.as_mut().unwrap().fallback_candidates.clear();
        let difference = changed(&prior, &effective);
        assert_eq!(
            difference,
            vec![
                wire::Field::InputTokens,
                wire::Field::Pin,
                wire::Field::RequireZdr
            ]
        );
        let mut large = prior;
        large.allowed_models = (0..65).map(|i| format!("model-{i}")).collect();
        let large = large.seal().unwrap();
        assert!(policy(&large).is_err());
        let unicode = text(&"文".repeat(512));
        assert!(unicode.truncated);
        assert!(unicode.text.len() <= 512);
    }
}
