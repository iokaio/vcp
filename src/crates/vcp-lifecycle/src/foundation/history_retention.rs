// SPDX-License-Identifier: Apache-2.0
//! Typed local history controls. Execution stays on the canonical owner and
//! never invokes a model, resumes a task, or schedules child work.
use serde::{Deserialize, Serialize};
use vcp_domain::{retention_selector::Selector, *};
use vcp_memory::{
    access::Access,
    retention::{self, Action},
    retention_policy,
};
use vcp_store::Store;
type Result<T, E = String> = std::result::Result<T, E>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    History {
        query: vcp_audit::history_query::Query,
    },
    Memory {
        claim: ClaimId,
        limit: u32,
        cursor: Option<MemoryCursor>,
    },
    Preview {
        selector: Selector,
        action: Action,
    },
    Apply {
        preview: String,
    },
    PreviewPage {
        preview: String,
        offset: u32,
        limit: u32,
    },
    Cleanup {
        receipt: String,
    },
    Policy,
    SetPolicy {
        expected: Option<Revision>,
        notice_repeat_days: u32,
        automatic: Option<retention_policy::Automatic>,
    },
    Notice,
    NoticeShown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCursor {
    pub workspace: WorkspaceId,
    pub access_digest: String,
    pub claim: ClaimId,
    pub at: MemorySeq,
    pub after: u32,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
}
impl Request {
    pub fn mutates(&self) -> bool {
        matches!(
            self,
            Self::Preview { .. }
                | Self::Apply { .. }
                | Self::Cleanup { .. }
                | Self::SetPolicy { .. }
                | Self::NoticeShown
        )
    }
}
/// Shared with the offline local controller: it must hold the exclusive Store
/// owner. A live CanonicalHost invokes this on the same worker as dispatch.
pub async fn execute(
    store: &mut Store,
    access: &Access,
    request: Request,
    now: Timestamp,
) -> Result<serde_json::Value, String> {
    let result = match request {
        Request::History { query } => serde_json::to_value(
            vcp_audit::history_query::query(
                store.state(),
                &vcp_audit::history::Access {
                    workspace: access.workspace.clone(),
                    authority: access.authority,
                    read: access.read,
                    tasks: access.tasks.clone(),
                },
                &query,
            )
            .map_err(|e| e.to_string())?,
        ),
        Request::Memory {
            claim,
            limit,
            cursor,
        } => memory_page(store, access, claim, limit, cursor),
        Request::Preview { selector, action } => {
            let preview = retention::preview(store, access, selector, action, now)
                .map_err(|e| e.to_string())?;
            retention::save_preview(store, access, &preview, now)
                .await
                .map_err(|e| e.to_string())?;
            preview_value(&preview)
        }
        Request::Apply { preview } => {
            let selected =
                retention::load_preview(store, access, &preview).map_err(|e| e.to_string())?;
            let receipt = retention::apply(store, access, &selected, now)
                .await
                .map_err(|e| e.to_string())?;
            let mut value = serde_json::to_value(&receipt).map_err(|e| e.to_string())?;
            value["preview"] = preview_value(&receipt.preview).map_err(|e| e.to_string())?;
            Ok(value)
        }
        Request::PreviewPage {
            preview,
            offset,
            limit,
        } => {
            if !(1..=128).contains(&limit) {
                return Err("preview page limit must be 1..128".into());
            }
            let selected =
                retention::load_preview(store, access, &preview).map_err(|e| e.to_string())?;
            let total =
                selected.selected.len() + selected.dependent.len() + selected.protected.len();
            if offset as usize > total {
                return Err("preview page offset exceeds selection".into());
            }
            let entries:Vec<_>=selected.selected.iter().map(|target|serde_json::json!({"class":"selected","target":target}))
                .chain(selected.dependent.iter().map(|target|serde_json::json!({"class":"dependent","target":target})))
                .chain(selected.protected.iter().map(|item|serde_json::json!({"class":"protected","target":item.target,"reason":item.reason})))
                .skip(offset as usize).take(limit as usize).collect();
            let next = offset as usize + entries.len();
            Ok(
                serde_json::json!({"preview":preview,"total":total,"offset":offset,"entries":entries,"next_offset":(next<total).then_some(next)}),
            )
        }
        Request::Cleanup { receipt } => {
            let receipt = retention::cleanup(store, access, &receipt, now)
                .await
                .map_err(|e| e.to_string())?;
            let mut value = serde_json::to_value(&receipt).map_err(|e| e.to_string())?;
            value["preview"] = preview_value(&receipt.preview).map_err(|e| e.to_string())?;
            Ok(value)
        }
        Request::Policy => {
            serde_json::to_value(retention_policy::show(store, access).map_err(|e| e.to_string())?)
        }
        Request::SetPolicy {
            expected,
            notice_repeat_days,
            automatic,
        } => serde_json::to_value(
            retention_policy::set(store, access, expected, notice_repeat_days, automatic, now)
                .await
                .map_err(|e| e.to_string())?,
        ),
        Request::Notice => serde_json::to_value(
            retention_policy::aging(store, access, now).map_err(|e| e.to_string())?,
        ),
        Request::NoticeShown => {
            retention_policy::acknowledge_notice(store, access, now)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"notice_recorded":true}))
        }
    };
    result.map_err(|e| e.to_string())
}
fn memory_page(
    store: &Store,
    access: &Access,
    claim: ClaimId,
    limit: u32,
    cursor: Option<MemoryCursor>,
) -> Result<serde_json::Value, serde_json::Error> {
    // Errors from canonical authorization are returned through the ordinary
    // adapter error boundary; they are never represented as empty histories.
    fn page(
        store: &Store,
        access: &Access,
        claim: ClaimId,
        limit: u32,
        cursor: Option<MemoryCursor>,
    ) -> Result<serde_json::Value> {
        if !(1..=32).contains(&limit) {
            return Err("memory page limit must be 1..32".into());
        }
        let workspace: vcp_domain::workspace::Workspace = store
            .state()
            .record(
                vcp_store::contract::Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .and_then(|r| r.decode())
            .map_err(|e| e.to_string())?;
        let access_digest = vcp_protocol::digest_bytes(
            &vcp_protocol::canonical_bytes(&(&access.actor, &access.tasks, access.read))
                .map_err(|e| e.to_string())?,
        );
        if cursor.as_ref().is_some_and(|c| {
            c.claim != claim
                || c.workspace != access.workspace
                || c.access_digest != access_digest
                || c.authority != workspace.authority
                || c.deletion != workspace.deletion
        }) {
            return Err("memory cursor requires refresh after scope/retention change".into());
        }
        let history =
            vcp_memory::history::query(store, access, &claim, cursor.as_ref().map(|c| c.at), None)
                .map_err(|e| e.to_string())?;
        let at = cursor.as_ref().map(|c| c.at).unwrap_or_else(|| {
            history
                .versions
                .iter()
                .map(|v| v.memory_seq)
                .max()
                .unwrap_or(MemorySeq::ZERO)
        });
        let after = cursor.as_ref().map_or(0, |c| c.after) as usize;
        if after > history.versions.len() {
            return Err("memory cursor exceeds history".into());
        }
        let mut rows = Vec::new();
        let mut bytes = 0;
        for version in history.versions.iter().skip(after).take(limit as usize) {
            let mut value = serde_json::to_value(version).map_err(|e| e.to_string())?;
            let size = serde_json::to_vec(&value).map_err(|e| e.to_string())?.len();
            if size > 512 * 1024 {
                value["version"] = serde_json::Value::Null;
                value["inspection_truncated"] = serde_json::json!(true);
            }
            let size = serde_json::to_vec(&value).map_err(|e| e.to_string())?.len();
            if !rows.is_empty() && bytes + size > 512 * 1024 {
                break;
            }
            bytes += size;
            rows.push(value);
        }
        let next = after + rows.len();
        Ok(
            serde_json::json!({"workspace":access.workspace,"claim":claim,"watermark":history.watermark,"at":at,"kind":"governed_memory","versions":rows,"next_cursor":(next<history.versions.len()).then_some(MemoryCursor{workspace:access.workspace.clone(),access_digest,claim,at,after:next as u32,authority:workspace.authority,deletion:workspace.deletion})}),
        )
    }
    page(store, access, claim, limit, cursor).map_err(serde::ser::Error::custom)
}
fn preview_value(
    preview: &retention::PrunePreview,
) -> std::result::Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(preview)?;
    for name in ["selected", "dependent", "protected"] {
        let Some(list) = value[name].as_array_mut() else {
            continue;
        };
        let count = list.len();
        list.truncate(128);
        value[format!("{name}_count")] = serde_json::json!(count);
        value[format!("{name}_truncated")] = serde_json::json!(count > 128);
    }
    value["details_command"] = serde_json::json!(format!("prune show {}", preview.id));
    Ok(value)
}
impl super::CanonicalHost {
    pub fn history_retention(&self, request: Request) -> Result<serde_json::Value, String> {
        let mutates = request.mutates();
        let operation = move |context: &mut super::worker::Context| {
            let access = context.memory_access();
            let now = Timestamp::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
            );
            context
                .runtime
                .block_on(execute(context.engine.store_mut(), &access, request, now))
                .map_err(Into::into)
        };
        if mutates {
            self.worker.run(operation)
        } else {
            self.worker.run_cleanup(operation)
        }
    }
}
