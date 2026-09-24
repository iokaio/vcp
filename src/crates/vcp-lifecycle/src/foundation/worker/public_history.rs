// SPDX-License-Identifier: Apache-2.0
//! Observer history navigation through the same governed query as the CLI.
use super::{public_connection::PublicConnection, *};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Instant};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    history as wire,
    jsonrpc::RpcError,
    methods,
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn failure(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "governed history navigation unavailable".into(),
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
fn audit_error(error: vcp_audit::Error) -> RpcError {
    match error {
        vcp_audit::Error::Access => failure(Code::PolicyDenied),
        vcp_audit::Error::Restart(_) | vcp_audit::Error::Removed => failure(Code::CursorGap),
        vcp_audit::Error::Limit => failure(Code::ResourceLimit),
        vcp_audit::Error::UnsupportedFilter(_) => failure(Code::CapabilityUnavailable),
        _ => unavailable(),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    actor: ActorId,
    scope: methods::Scope,
    task: Option<methods::Id>,
    query: String,
    inner: vcp_audit::history_query::Cursor,
}
impl PublicConnection {
    pub(super) fn history_query(
        &self,
        request: &wire::Query,
        current: &Access,
    ) -> RpcResult<wire::Page> {
        request.validate().map_err(|_| RpcError::invalid_params())?;
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
                    inspect(context.engine.store(), &access, &request, &check)
                })())
            })
            .map_err(|_| unavailable())?
    }
}
fn inspect(
    store: &Store,
    access: &Access,
    request: &wire::Query,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<wire::Page> {
    request.validate().map_err(|_| RpcError::invalid_params())?;
    check()?;
    if !access.read
        || request.scope.workspace.as_str() != access.workspace.as_str()
        || request.scope.session.as_str() != access.session.as_str()
    {
        return Err(failure(Code::PolicyDenied));
    }
    let session: vcp_domain::workspace::Session = store
        .state()
        .record(
            Collection::Session,
            access.session.as_str(),
            &access.workspace,
        )
        .map_err(|_| failure(Code::PolicyDenied))?
        .decode()
        .map_err(|_| unavailable())?;
    if session.workspace != access.workspace || session.id != access.session {
        return Err(failure(Code::PolicyDenied));
    }
    let mut tasks = BTreeSet::new();
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.collection == Collection::Task && row.workspace == access.workspace)
    {
        check()?;
        let task: Task = row.decode().map_err(|_| unavailable())?;
        if task.scope.workspace != row.workspace || task.scope.task.as_str() != row.id {
            return Err(unavailable());
        }
        if task.scope.session == access.session
            && task.redaction.is_none()
            && request
                .task
                .as_ref()
                .is_none_or(|id| id.as_str() == task.scope.task.as_str())
        {
            tasks.insert(task.scope.task);
        }
    }
    if request.task.is_some() && tasks.is_empty() {
        return Err(failure(Code::PolicyDenied));
    }
    let memory_access = vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks: Some(tasks.clone()),
    };
    let mut semantic = request.clone();
    semantic.cursor = None;
    let digest = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&semantic).map_err(|_| unavailable())?,
    );
    let cursor = request
        .cursor
        .as_ref()
        .map(|text| serde_json::from_str::<Cursor>(text).map_err(|_| failure(Code::CursorGap)))
        .transpose()?;
    if cursor.as_ref().is_some_and(|cursor| {
        cursor.version != 1
            || cursor.actor != access.actor
            || cursor.scope != request.scope
            || cursor.task != request.task
            || cursor.query != digest
    }) {
        return Err(failure(Code::CursorGap));
    }
    let selector = match &request.selector {
        Some(selector) => selector
            .normalized()
            .map_err(|_| RpcError::invalid_params())?,
        None => vcp_domain::retention_selector::Selector {
            schema_version: 1,
            tree: vcp_domain::retention_selector::Tree::Match(
                vcp_domain::retention_selector::Criterion::Workspace(access.workspace.clone()),
            ),
        },
    };
    let query = vcp_audit::history_query::Query {
        selector,
        text: request.text.clone(),
        limit: request.limit,
        cursor: cursor.map(|cursor| cursor.inner),
        artifact: request
            .artifact
            .as_ref()
            .map(|id| ArtifactId::parse(id.as_str()).map_err(|_| RpcError::invalid_params()))
            .transpose()?,
        expand_compacted: request.expand_compacted,
    };
    let page = vcp_audit::history_query::query_session(
        store.state(),
        &vcp_audit::history::Access {
            workspace: access.workspace.clone(),
            authority: access.authority,
            read: access.read,
            tasks: request.task.as_ref().map(|_| tasks),
        },
        &query,
        &access.session,
    )
    .map_err(audit_error)?;
    check()?;
    let origins = page
        .rows
        .iter()
        .map(|row| row.event.event.id.clone())
        .collect();
    let (links, truncated) =
        vcp_memory::history::origin_links_with_check(store, &memory_access, &origins, &|| {
            check().map_err(|_| vcp_memory::Error::Conflict("history query interrupted"))
        })
        .map_err(|error| match error {
            vcp_memory::Error::Access => failure(Code::PolicyDenied),
            _ => unavailable(),
        })?;
    check()?;
    let next_cursor = page
        .next_cursor
        .map(|inner| {
            serde_json::to_string(&Cursor {
                version: 1,
                actor: access.actor.clone(),
                scope: request.scope.clone(),
                task: request.task.clone(),
                query: digest,
                inner,
            })
            .map_err(|_| unavailable())
        })
        .transpose()?;
    if next_cursor.as_ref().is_some_and(|value| value.len() > 8192) {
        return Err(failure(Code::ResourceLimit));
    }
    let mut rows = Vec::new();
    for row in page.rows {
        let event = row.event.event;
        if event.session != access.session {
            return Err(failure(Code::PolicyDenied));
        }
        let task = event.task.as_ref();
        let mut content_truncated = row.content_truncated || row.artifact_links.len() > 128;
        let metadata = event
            .metadata
            .filter(|metadata| {
                let bounded = metadata.paths.len() <= 64
                    && vcp_protocol::canonical_bytes(metadata)
                        .is_ok_and(|bytes| bytes.len() <= 4096)
                    && metadata.paths.iter().map(|path| path.len()).sum::<usize>() <= 4096
                    && metadata.paths.iter().all(|path| path.len() <= 4096)
                    && metadata
                        .provider
                        .as_ref()
                        .is_none_or(|value| value.len() <= 256)
                    && metadata
                        .model
                        .as_ref()
                        .is_none_or(|value| value.len() <= 256);
                content_truncated |= !bounded;
                bounded
            })
            .map(|metadata| {
                Ok::<_, RpcError>(wire::Metadata {
                    agent: metadata
                        .agent
                        .as_ref()
                        .map(|value| id(value.as_str()))
                        .transpose()?,
                    provider: metadata.provider,
                    model: metadata.model,
                    paths: metadata.paths,
                })
            })
            .transpose()?;
        rows.push(wire::Row {
            id: id(event.id.as_str())?,
            session: id(event.session.as_str())?,
            task: task.map(|task| id(task.as_str())).transpose()?,
            sequence: row.event.sequence.get().into(),
            timestamp_ms: event.timestamp.get().into(),
            kind: serde_json::to_value(event.kind)
                .map_err(|_| unavailable())?
                .as_str()
                .ok_or_else(unavailable)?
                .to_owned(),
            actor: id(event.actor.as_str())?,
            command_id: id(event.correlation.as_str())?,
            visibility: row.visibility,
            content_truncated,
            recall_excluded: row.recall_excluded,
            compacted: row.compacted,
            artifacts: row
                .artifact_links
                .into_iter()
                .take(128)
                .map(|link| {
                    Ok(wire::ArtifactLink {
                        id: id(link.id.as_str())?,
                        availability: link.availability,
                        original_bytes: link.original_bytes.map(|bytes| bytes.get().into()),
                    })
                })
                .collect::<RpcResult<_>>()?,
            metadata,
        });
    }
    let mut result = wire::Page {
        scope: request.scope.clone(),
        task: request.task.clone(),
        source_watermark: page.source_watermark.get().into(),
        observed_watermark: store.state().watermark.get().into(),
        newer_events: page.newer_events.into(),
        search_scope: page.search_scope,
        rows,
        gaps: page
            .gaps
            .into_iter()
            .map(|gap| {
                Ok(wire::Gap {
                    session: id(gap.session.as_str())?,
                    first: gap.first.get().into(),
                    last: gap.last.get().into(),
                    reason: "removed_by_current_retention".into(),
                })
            })
            .collect::<RpcResult<_>>()?,
        claim_links: links
            .into_iter()
            .map(|link| {
                Ok(wire::ClaimLink {
                    origin: id(link.origin.as_str())?,
                    claim: id(link.claim.as_str())?,
                    version: id(link.version.as_str())?,
                    memory_sequence: link.memory_seq.get().into(),
                })
            })
            .collect::<RpcResult<_>>()?,
        claim_links_truncated: truncated,
        complete: next_cursor.is_none(),
        next_cursor,
    };
    check()?;
    while vcp_protocol::canonical_bytes(&result)
        .map_err(|_| unavailable())?
        .len()
        > wire::MAX_PAGE_BYTES
    {
        if result.claim_links.pop().is_some() {
            check()?;
            result.claim_links_truncated = true;
            continue;
        }
        return Err(failure(Code::ResourceLimit));
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
