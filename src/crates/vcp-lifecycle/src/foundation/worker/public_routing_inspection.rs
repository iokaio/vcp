// SPDX-License-Identifier: Apache-2.0
//! Selective observer projection; never invokes owner routing controls or captures a report.
use super::{public_connection::PublicConnection, *};
use crate::foundation::routing_state;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Instant};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    retention::RetentionMask,
    workspace::Workspace,
};
use vcp_models::routing as model;
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods, routing_inspection as wire,
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn failure(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "routing status unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable() -> RpcError {
    failure(Code::StoreUnavailable)
}
fn id(value: &str) -> RpcResult<methods::Id> {
    value.to_owned().try_into().map_err(|_| unavailable())
}
fn tag(value: &str) -> RpcResult<String> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(char::is_control)
        || value.contains("://")
    {
        Err(unavailable())
    } else {
        Ok(value.to_owned())
    }
}
fn text(value: &str) -> wire::Text {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1
    }
    wire::Text {
        text: value[..end].to_owned(),
        truncated: end < value.len(),
    }
}
fn mapped<T: Serialize, U: serde::de::DeserializeOwned>(value: T) -> RpcResult<U> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| unavailable())?)
        .map_err(|_| unavailable())
}
fn identity(value: &model::ModelEndpoint) -> RpcResult<wire::Identity> {
    Ok(wire::Identity {
        model: tag(&value.model)?,
        endpoint: tag(&value.endpoint)?,
    })
}
fn summary(value: &model::Policy) -> RpcResult<wire::PolicySummary> {
    Ok(wire::PolicySummary {
        id: tag(&value.id)?,
        parent_id: value.parent.as_deref().map(tag).transpose()?,
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
            .map(|pin| identity(&pin.candidate))
            .transpose()?,
        input_tokens: value.input_tokens.map(|value| value.get().into()),
        output_tokens: value.output_tokens.map(|value| value.get().into()),
        reasoning_effort: value.reasoning_effort.map(mapped).transpose()?,
        retrieval_limits: value
            .retrieval_limits
            .as_ref()
            .map(|limits| wire::RetrievalLimits {
                results: limits.results,
                tokens: limits.tokens.get().into(),
                bytes: limits.bytes.get().into(),
            }),
        escalation_limits: value
            .escalation_limits
            .as_ref()
            .map(|limits| wire::EscalationLimits {
                max_transport_retries: limits.max_transport_retries,
                max_quality_switches: limits.max_quality_switches,
                max_total_attempts: limits.max_total_attempts,
                minimum_repeated_failures: limits.minimum_repeated_failures,
            }),
        broader_task_class: value.broader_task_class.as_deref().map(text),
        allowed_models_count: (value.allowed_models.len() as u64).into(),
        allowed_endpoints_count: (value.allowed_endpoints.len() as u64).into(),
        allowed_groups_count: (value.allowed_groups.len() as u64).into(),
        pin_fallback_count: (value
            .pin
            .as_ref()
            .map_or(0, |pin| pin.fallback_candidates.len()) as u64)
            .into(),
    })
}
fn policy_rows(
    value: &model::Policy,
    representation: wire::Representation,
    rows: &mut Vec<wire::Row>,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<()> {
    for value in &value.allowed_models {
        check()?;
        rows.push(wire::Row::PolicyEntry {
            representation,
            entry: wire::Entry::AllowedModel { value: tag(value)? },
        })
    }
    for value in &value.allowed_endpoints {
        check()?;
        rows.push(wire::Row::PolicyEntry {
            representation,
            entry: wire::Entry::AllowedEndpoint { value: tag(value)? },
        })
    }
    for value in &value.allowed_groups {
        check()?;
        rows.push(wire::Row::PolicyEntry {
            representation,
            entry: wire::Entry::AllowedGroup {
                value: mapped(value)?,
            },
        })
    }
    if let Some(pin) = &value.pin {
        for value in &pin.fallback_candidates {
            check()?;
            rows.push(wire::Row::PolicyEntry {
                representation,
                entry: wire::Entry::PinFallback {
                    model: tag(&value.model)?,
                    endpoint: tag(&value.endpoint)?,
                },
            })
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    digest: String,
    after: usize,
}
impl PublicConnection {
    pub(super) fn routing_status(
        &self,
        request: &wire::Request,
        current: &Access,
    ) -> RpcResult<wire::Page> {
        wire::validate_request(request).map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied))?;
        let request = request.clone();
        let connected = self.connected.clone();
        let worker = host.worker.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| failure(Code::PolicyDenied))?;
                    let start = Instant::now();
                    let check = || {
                        if !connected.load(Ordering::SeqCst)
                            || worker.fenced()
                            || start.elapsed() >= Duration::from_secs(2)
                        {
                            Err(failure(Code::CursorGap))
                        } else {
                            Ok(())
                        }
                    };
                    let ceilings = context.routing_ceilings().map_err(|_| unavailable())?;
                    inspect(
                        &context.engine,
                        &access,
                        &request,
                        ceilings.as_ref(),
                        now(),
                        &check,
                    )
                })())
            })
            .map_err(|_| unavailable())?
    }
}
fn source(
    store: &Store,
    access: &Access,
    workspace: &Workspace,
    tasks: &BTreeSet<TaskId>,
    registry: &routing_state::Published<routing_state::Registry>,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<(methods::EvidenceReference, methods::Id)> {
    let artifact: ArtifactDescriptor = store
        .state()
        .record(
            Collection::Artifact,
            registry.value.raw.as_str(),
            &access.workspace,
        )
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    artifact.validate().map_err(|_| unavailable())?;
    if artifact.spec.scope.workspace != access.workspace
        || artifact.spec.scope.session != access.session
        || !tasks.contains(&artifact.spec.scope.task)
        || artifact.state != CaptureState::Complete
        || artifact.sha256 != registry.value.raw_sha256
        || artifact.spec.schema == "vcp-optimization-forecast-v1"
    {
        return Err(unavailable());
    }
    vcp_store::export_contract::validate_read(
        store.state(),
        access.authority,
        Some(tasks),
        &artifact,
    )
    .map_err(|_| unavailable())?;
    for row in
        store.state().records.values().filter(|row| {
            row.workspace == access.workspace && row.collection == Collection::Tombstone
        })
    {
        check()?;
        let mask: RetentionMask = row.decode().map_err(|_| unavailable())?;
        mask.validate().map_err(|_| unavailable())?;
        if mask.workspace != access.workspace
            || mask.deletion > workspace.deletion
            || mask.artifacts.contains(&artifact.spec.id)
        {
            return Err(unavailable());
        }
    }
    Ok((
        methods::EvidenceReference {
            artifact: id(artifact.spec.id.as_str())?,
            offset: 0.into(),
            length: artifact.length.get().into(),
            sha256: artifact.sha256,
        },
        id(artifact.spec.scope.task.as_str())?,
    ))
}
fn inspect(
    engine: &vcp_engine::Engine<Store>,
    access: &Access,
    request: &wire::Request,
    ceilings: Option<&model::Policy>,
    observed_at: Timestamp,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<wire::Page> {
    wire::validate_request(request).map_err(|_| RpcError::invalid_params())?;
    check()?;
    if request.scope.workspace.as_str() != access.workspace.as_str()
        || request.scope.session.as_str() != access.session.as_str()
    {
        return Err(failure(Code::PolicyDenied));
    }
    let task = match engine
        .query(
            access,
            &vcp_engine::query::Query::Task {
                task: TaskId::parse(request.task.as_str())
                    .map_err(|_| RpcError::invalid_params())?,
            },
        )
        .map_err(|_| failure(Code::PolicyDenied))?
    {
        vcp_engine::query::QueryResult::Task { task, .. } => task,
        _ => return Err(unavailable()),
    };
    if task.redaction.is_some() {
        return Err(failure(Code::PolicyDenied));
    }
    let store = engine.store();
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| failure(Code::PolicyDenied))?
        .decode()
        .map_err(|_| unavailable())?;
    let mut tasks = BTreeSet::new();
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.workspace == access.workspace && row.collection == Collection::Task)
    {
        check()?;
        let task: Task = row.decode().map_err(|_| unavailable())?;
        if task.scope.workspace != access.workspace || task.scope.task.as_str() != row.id {
            return Err(unavailable());
        }
        if task.scope.session == access.session && task.redaction.is_none() {
            tasks.insert(task.scope.task);
        }
    }
    let scoped = vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks: Some(tasks.clone()),
    };
    let persisted = routing_state::current_policy(store, &scoped).map_err(|_| unavailable())?;
    check()?;
    let effective = match (&persisted, ceilings) {
        (Some(persisted), Some(ceilings)) => Some(
            routing_state::effective_policy(persisted.value.clone(), ceilings)
                .map_err(|_| unavailable())?,
        ),
        _ => None,
    };
    let mut registry = wire::Registry::Unavailable {
        reason: wire::RegistryReason::NotConfigured,
    };
    let current_registry = routing_state::current_registry(store, &scoped);
    check()?;
    let mut catalog = None;
    match current_registry {
        Ok(Some(value)) => match source(store, access, &workspace, &tasks, &value, check) {
            Ok((source, source_task)) => {
                registry = wire::Registry::Observed {
                    revision: value.revision.get().into(),
                    catalog_id: tag(&value.value.catalog.id)?,
                    observed_at_ms: value.value.catalog.observed_at.get().into(),
                    effective_at_ms: value
                        .value
                        .catalog
                        .effective_at
                        .map(|value| value.get().into()),
                    source,
                    source_task,
                    source_availability: wire::SourceAvailability::RetainedMetadataOnly,
                };
                catalog = Some(value);
            }
            Err(_) => {
                registry = wire::Registry::Unavailable {
                    reason: wire::RegistryReason::SourceUnavailable,
                }
            }
        },
        Ok(None) => {}
        Err(error) => {
            registry = wire::Registry::Unavailable {
                reason: if error == "catalog evidence scope denied" {
                    wire::RegistryReason::Restricted
                } else {
                    wire::RegistryReason::SourceUnavailable
                },
            }
        }
    }
    check()?;
    let ceilings_digest = ceilings
        .map(|policy| policy.digest().map_err(|_| unavailable()))
        .transpose()?;
    let mut semantic = request.clone();
    semantic.cursor = None;
    let digest = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&(
            &semantic,
            &access.actor,
            workspace.authority,
            workspace.deletion,
            workspace.binding.revision,
            task.revision,
            task.steering,
            &tasks,
            &persisted,
            &catalog,
            &registry,
            &ceilings_digest,
        ))
        .map_err(|_| unavailable())?,
    );
    let start = match &request.cursor {
        None => 0,
        Some(value) => {
            let cursor: Cursor =
                serde_json::from_str(value).map_err(|_| failure(Code::CursorGap))?;
            if cursor.version != 1 || cursor.digest != digest {
                return Err(failure(Code::CursorGap));
            }
            cursor.after
        }
    };
    let mut rows = Vec::new();
    match request.section {
        wire::Section::PolicyEntries => {
            if let Some(value) = &persisted {
                policy_rows(
                    &value.value,
                    wire::Representation::Persisted,
                    &mut rows,
                    check,
                )?
            }
            if let Some(value) = &effective {
                policy_rows(value, wire::Representation::Effective, &mut rows, check)?
            }
        }
        wire::Section::Catalog => {
            if let Some(value) = &catalog {
                let mut candidates = value.value.catalog.entries.iter().collect::<Vec<_>>();
                candidates.sort_by(|a, b| a.identity.cmp(&b.identity));
                for candidate in candidates {
                    check()?;
                    let groups = candidate
                        .memberships
                        .iter()
                        .map(|membership| membership.group)
                        .collect::<BTreeSet<_>>();
                    rows.push(wire::Row::Candidate {
                        model: tag(&candidate.identity.model)?,
                        endpoint: tag(&candidate.identity.endpoint)?,
                        availability: mapped(candidate.availability)?,
                        groups: groups.into_iter().map(mapped).collect::<RpcResult<_>>()?,
                        compatibility_count: (candidate.compatibility.len() as u64).into(),
                        role_evidence_count: (candidate
                            .memberships
                            .iter()
                            .map(|membership| membership.roles.len())
                            .sum::<usize>() as u64)
                            .into(),
                        capability_count: (candidate.capabilities.len() as u64).into(),
                        provenance_count: (candidate.provenance.len() as u64).into(),
                        reasons: candidate
                            .reasons
                            .iter()
                            .take(8)
                            .map(|value| text(value))
                            .collect(),
                        reasons_truncated: candidate.reasons.len() > 8,
                    });
                }
            }
        }
    }
    if start > rows.len() {
        return Err(failure(Code::CursorGap));
    }
    let mut result = wire::Page {
        scope: request.scope.clone(),
        task: request.task.clone(),
        watermark: store.state().watermark.get().into(),
        observed_at_ms: observed_at.get().into(),
        authority_revision: workspace.authority.get().into(),
        deletion_revision: workspace.deletion.get().into(),
        binding_revision: workspace.binding.revision.get().into(),
        persisted: persisted
            .as_ref()
            .map(|value| {
                Ok::<_, RpcError>(wire::PublishedPolicy {
                    revision: value.revision.get().into(),
                    parent_revision: value.parent.map(|value| value.get().into()),
                    actor: id(value.actor.as_str())?,
                    authority_revision: value.authority.get().into(),
                    published_at_ms: value.timestamp.get().into(),
                    policy: summary(&value.value)?,
                })
            })
            .transpose()?,
        effective: match &effective {
            Some(policy) => wire::Effective::Observed {
                ceilings_sha256: ceilings_digest.unwrap_or_default(),
                policy: summary(policy)?,
            },
            None => wire::Effective::Unavailable {
                reason: if persisted.is_none() {
                    wire::EffectiveReason::NotConfigured
                } else {
                    wire::EffectiveReason::HostUnconfigured
                },
            },
        },
        registry,
        optimizer: wire::Optimizer {
            report_capture: wire::ReportCapture::ExplicitMutationRequired,
            preferences: wire::Preferences::WorkspaceAuthorityRequired,
            preview: if ceilings.is_some() {
                wire::Preview::SeparateAuthorizedOperation
            } else {
                wire::Preview::HostCeilingsRequired
            },
            remote_advice: wire::RemoteAdvice::Disabled,
        },
        section: request.section,
        rows: vec![],
        next_cursor: None,
        complete: false,
    };
    let total = rows.len();
    let mut after = start;
    for row in rows.into_iter().skip(start).take(request.limit as usize) {
        check()?;
        result.rows.push(row);
        if vcp_protocol::canonical_bytes(&result)
            .map_err(|_| unavailable())?
            .len()
            > wire::MAX_PAGE_BYTES - 1024
        {
            result.rows.pop();
            break;
        }
        after += 1;
    }
    if after == start && start < total {
        return Err(failure(Code::ResourceLimit));
    }
    result.complete = after == total;
    if !result.complete {
        result.next_cursor = Some(
            serde_json::to_string(&Cursor {
                version: 1,
                digest,
                after,
            })
            .map_err(|_| unavailable())?,
        )
    }
    check()?;
    if vcp_protocol::canonical_bytes(&result)
        .map_err(|_| unavailable())?
        .len()
        > wire::MAX_PAGE_BYTES
    {
        return Err(failure(Code::ResourceLimit));
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
