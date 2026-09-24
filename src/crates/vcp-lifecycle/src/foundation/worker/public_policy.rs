// SPDX-License-Identifier: Apache-2.0
//! Scoped read-only policy facts, never a prepared-operation admission.
use super::{public_connection::PublicConnection, *};
use std::{collections::BTreeMap, time::Instant};
use vcp_domain::policy as domain;
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods, policy_inspection as wire,
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn error(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "policy inspection unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable() -> RpcError {
    error(Code::StoreUnavailable)
}
fn id(value: impl ToString) -> RpcResult<methods::Id> {
    value.to_string().try_into().map_err(|_| unavailable())
}
fn convert<T: serde::de::DeserializeOwned>(value: impl serde::Serialize) -> RpcResult<T> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| unavailable())?)
        .map_err(|_| unavailable())
}
fn text(value: &str) -> wire::Text {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    wire::Text {
        text: value[..end].into(),
        truncated: end < value.len(),
    }
}
fn bounded_paths(paths: &[String]) -> RpcResult<Vec<wire::Text>> {
    let mut result = Vec::new();
    let mut bytes = 0;
    for path in paths.iter().take(16) {
        let projected = text(path);
        let size = serde_json::to_vec(&projected)
            .map_err(|_| unavailable())?
            .len();
        if bytes + size > 4096 {
            break;
        }
        bytes += size;
        result.push(projected);
    }
    Ok(result)
}
fn summary(value: &domain::Policy) -> RpcResult<wire::Summary> {
    vcp_policy::validate_policy(value).map_err(|_| unavailable())?;
    Ok(wire::Summary {
        revision: value.revision.get().into(),
        mode: convert(value.mode)?,
        workspace_roots: value
            .workspace_roots
            .iter()
            .map(id)
            .collect::<RpcResult<_>>()?,
        automatic_effects: convert(&value.automatic_effects)?,
        timeout_ceiling_ms: value.timeout_ceiling_ms.get().into(),
        output_ceiling_bytes: value.output_ceiling_bytes.get().into(),
    })
}
struct HostPolicy {
    host_denials: Vec<domain::Denial>,
    effective: Option<domain::Policy>,
    inherited: Vec<domain::Grant>,
    parent: Option<TaskId>,
    unavailable: wire::Unavailable,
}
impl PublicConnection {
    pub fn policy_read(&self, request: &wire::Request, current: &Access) -> RpcResult<wire::Page> {
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| error(Code::PolicyDenied))?;
        let request = request.clone();
        let connected = self.connected.clone();
        let worker = host.worker.clone();
        let bindings = host.bindings.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| {
                    let start = Instant::now();
                    let check = || {
                        if !connected.load(Ordering::SeqCst)
                            || worker.fenced()
                            || start.elapsed() >= Duration::from_secs(2)
                        {
                            Err(unavailable())
                        } else {
                            Ok(())
                        }
                    };
                    check()?;
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| error(Code::PolicyDenied))?;
                    let task = task(context.engine.store(), &access, &request)?;
                    let binding = bindings
                        .lock()
                        .map_err(|_| unavailable())?
                        .values()
                        .find(|binding| binding.scope == task.scope)
                        .cloned();
                    let mut facts = HostPolicy {
                        host_denials: context.config.host_tool_denials.clone(),
                        effective: None,
                        inherited: vec![],
                        parent: None,
                        unavailable: wire::Unavailable::BindingUnavailable,
                    };
                    check()?;
                    if let Some(binding) = binding {
                        #[cfg(windows)]
                        match context.child_policy(&binding) {
                            Ok(Some((policy, grants, denials))) => {
                                facts.effective = Some(policy);
                                facts.inherited = grants;
                                facts.host_denials = denials;
                                facts.parent = task.parent.clone();
                            }
                            Ok(None) => {
                                facts.effective = vcp_engine::policy::optional(
                                    context.engine.store().state(),
                                    &access.workspace,
                                )
                                .map_err(|_| unavailable())?
                            }
                            Err(_) => facts.unavailable = wire::Unavailable::ChildScopeUnavailable,
                        }
                        #[cfg(not(windows))]
                        {
                            let _ = binding;
                        }
                    }
                    check()?;
                    inspect(
                        context.engine.store(),
                        &access,
                        &request,
                        &facts,
                        now(),
                        &check,
                    )
                })())
            })
            .map_err(|_| unavailable())?
    }
}
fn task(store: &Store, access: &Access, p: &wire::Request) -> RpcResult<Task> {
    wire::validate_request(p).map_err(|_| RpcError::invalid_params())?;
    if !access.read
        || p.scope.workspace.as_str() != access.workspace.as_str()
        || p.scope.session.as_str() != access.session.as_str()
    {
        return Err(error(Code::PolicyDenied));
    }
    let task: Task = store
        .state()
        .record(Collection::Task, p.task.as_str(), &access.workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if task.scope.workspace != access.workspace
        || task.scope.session != access.session
        || task.scope.task.as_str() != p.task.as_str()
        || task.redaction.is_some()
        || workspace.authority != access.authority
    {
        return Err(error(Code::PolicyDenied));
    }
    Ok(task)
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    digest: String,
    after: String,
}
fn inspect(
    store: &Store,
    access: &Access,
    p: &wire::Request,
    facts: &HostPolicy,
    observed: Timestamp,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<wire::Page> {
    check()?;
    let task = task(store, access, p)?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    let policy = vcp_engine::policy::optional(store.state(), &access.workspace)
        .map_err(|_| unavailable())?;
    let persisted = policy.as_ref().map(summary).transpose()?;
    let effective = if policy.is_none() {
        wire::Effective::Unavailable {
            reason: wire::Unavailable::PolicyAbsent,
        }
    } else if let Some(policy) = &facts.effective {
        wire::Effective::Observed {
            source: if facts.parent.is_some() {
                wire::Source::ChildInherited
            } else {
                wire::Source::Workspace
            },
            policy: summary(policy)?,
            host_denial_count: (facts.host_denials.len() as u64).into(),
        }
    } else {
        wire::Effective::Unavailable {
            reason: facts.unavailable.clone(),
        }
    };
    let mut rows = BTreeMap::new();
    if p.section == wire::Section::Denials {
        for (layer, rules) in [
            (wire::Layer::Host, facts.host_denials.as_slice()),
            (
                wire::Layer::Canonical,
                facts
                    .effective
                    .as_ref()
                    .or(policy.as_ref())
                    .map_or(&[][..], |p| p.denials.as_slice()),
            ),
        ] {
            for (index, rule) in rules.iter().enumerate() {
                check()?;
                let key = format!(
                    "{}-{index:04}",
                    if layer == wire::Layer::Host {
                        "host"
                    } else {
                        "canonical"
                    }
                );
                rows.insert(
                    key,
                    wire::Row::Denial {
                        value: wire::Denial {
                            id: text(&rule.id),
                            layer: layer.clone(),
                            origin: convert(rule.origin)?,
                            reason: text(&rule.reason),
                            effects: convert(&rule.effects)?,
                            tool: rule.tool.as_deref().map(text),
                            roots: rule.roots.iter().map(id).collect::<RpcResult<_>>()?,
                            paths: bounded_paths(&rule.paths)?,
                            path_count: (rule.paths.len() as u64).into(),
                        },
                    },
                );
            }
        }
    } else {
        for row in store.state().records.values().filter(|r| {
            r.collection == Collection::Access
                && r.workspace == access.workspace
                && r.value["document_type"] == "vcp_authority_v1"
        }) {
            check()?;
            let doc: domain::AuthorityDocument = row.decode().map_err(|_| unavailable())?;
            let domain::AuthorityData::Grant { grant } = doc.data else {
                continue;
            };
            if grant.actor != access.actor {
                continue;
            }
            let direct =
                matches!(&grant.scope,domain::GrantScope::Task{scope} if scope==&task.scope);
            let inherited = facts
                .inherited
                .iter()
                .any(|g| g.id == grant.id && g.revision == grant.revision);
            if !direct && !inherited {
                continue;
            }
            let projected = grant_view(
                &grant,
                &workspace,
                policy.as_ref(),
                observed,
                if inherited {
                    facts.parent.as_ref()
                } else {
                    None
                },
            )?;
            rows.insert(grant.id.to_string(), wire::Row::Grant { value: projected });
        }
    }
    // Digest the full visible inventory, not time-dependent expiry flags.
    let grants = store
        .state()
        .records
        .values()
        .filter(|r| r.collection == Collection::Access && r.workspace == access.workspace)
        .map(|row| (&row.id, row.revision))
        .collect::<Vec<_>>();
    let digest = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&(
            &p.scope,
            &p.task,
            &p.section,
            p.limit,
            &access.actor,
            workspace.authority,
            workspace.deletion,
            &workspace.binding,
            task.revision,
            task.steering,
            &grants,
            &facts.host_denials,
            &facts.effective,
            &facts.inherited,
            &facts.parent,
        ))
        .map_err(|_| unavailable())?,
    );
    let cursor: Option<Cursor> = p
        .cursor
        .as_ref()
        .map(|c| serde_json::from_str(c).map_err(|_| RpcError::invalid_params()))
        .transpose()?;
    if cursor
        .as_ref()
        .is_some_and(|c| c.digest != digest || !rows.contains_key(&c.after))
    {
        return Err(error(Code::VersionConflict));
    }
    let mut selected = Vec::new();
    let mut bytes = 0;
    let mut after = None;
    let mut more = false;
    for (key, row) in rows {
        check()?;
        if cursor.as_ref().is_some_and(|c| key <= c.after) {
            continue;
        }
        let size = serde_json::to_vec(&row).map_err(|_| unavailable())?.len();
        if selected.len() >= p.limit as usize || bytes + size > 48000 {
            more = true;
            break;
        }
        bytes += size;
        after = Some(key);
        selected.push(row);
    }
    if more && after.is_none() {
        return Err(error(Code::ResourceLimit));
    }
    let next_cursor = if more {
        Some(
            serde_json::to_string(&Cursor {
                digest,
                after: after.ok_or_else(unavailable)?,
            })
            .map_err(|_| unavailable())?,
        )
    } else {
        None
    };
    let page = wire::Page {
        scope: p.scope.clone(),
        task: p.task.clone(),
        watermark: store.state().watermark.get().into(),
        observed_at_ms: observed.get().into(),
        authority_revision: workspace.authority.get().into(),
        deletion_revision: workspace.deletion.get().into(),
        binding_revision: workspace.binding.revision.get().into(),
        host: id(&workspace.binding.host)?,
        task_revision: task.revision.get().into(),
        steering_revision: task.steering.get().into(),
        task_state: convert(task.state)?,
        trust: convert(workspace.trust)?,
        persisted,
        effective,
        assessment: wire::Assessment::OperationNotEvaluated,
        grant_visibility: wire::GrantVisibility::TaskAndInheritedOnly,
        section: p.section.clone(),
        rows: selected,
        next_cursor,
        complete: !more,
    };
    if serde_json::to_vec(&page).map_err(|_| unavailable())?.len() > 65536 {
        return Err(error(Code::ResourceLimit));
    }
    check()?;
    Ok(page)
}
fn grant_view(
    grant: &domain::Grant,
    workspace: &Workspace,
    policy: Option<&domain::Policy>,
    now: Timestamp,
    parent: Option<&TaskId>,
) -> RpcResult<wire::Grant> {
    let scope = match &grant.scope {
        domain::GrantScope::Workspace { workspace } => wire::GrantScope::Workspace {
            workspace: id(workspace)?,
        },
        domain::GrantScope::Session { workspace, session } => wire::GrantScope::Session {
            scope: methods::Scope {
                workspace: id(workspace)?,
                session: id(session)?,
            },
        },
        domain::GrantScope::Task { scope } => wire::GrantScope::Task {
            scope: methods::Scope {
                workspace: id(&scope.workspace)?,
                session: id(&scope.session)?,
            },
            task: id(&scope.task)?,
        },
    };
    let target = match &grant.target {
        domain::GrantTarget::Exact { digest } => wire::Target::Exact {
            operation_sha256: digest.clone(),
        },
        domain::GrantTarget::Configured {
            tool,
            schema,
            arguments_digest,
            effects,
            roots,
            paths,
            timeout_ms,
            output_bytes,
            ..
        } => wire::Target::Configured {
            tool: text(tool),
            schema_sha256: schema.clone(),
            arguments_sha256: arguments_digest.clone(),
            effects: convert(effects)?,
            roots: roots.iter().map(id).collect::<RpcResult<_>>()?,
            paths: bounded_paths(paths)?,
            path_count: (paths.len() as u64).into(),
            timeout_ceiling_ms: timeout_ms.get().into(),
            output_ceiling_bytes: output_bytes.get().into(),
        },
    };
    Ok(wire::Grant {
        id: id(&grant.id)?,
        revision: grant.revision.get().into(),
        actor: id(&grant.actor)?,
        scope,
        host: id(&grant.host)?,
        binding_revision: grant.binding.get().into(),
        authority_revision: grant.authority.get().into(),
        policy_revision: grant.policy.get().into(),
        expires_at_ms: grant.expires_at.get().into(),
        revoked: grant.revoked,
        origin: convert(grant.origin)?,
        reason: text(&grant.reason),
        approval: grant.approval.as_ref().map(id).transpose()?,
        inherited_from: parent.map(id).transpose()?,
        current_matches: wire::Matches {
            host: grant.host == workspace.binding.host,
            binding: grant.binding == workspace.binding.revision,
            authority: grant.authority == workspace.authority,
            policy: policy.is_some_and(|p| p.revision == grant.policy),
            unexpired: grant.expires_at > now,
            not_revoked: !grant.revoked,
        },
        target,
    })
}
#[cfg(test)]
#[path = "public_policy/tests.rs"]
mod tests;
