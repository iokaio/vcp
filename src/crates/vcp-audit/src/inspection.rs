// SPDX-License-Identifier: Apache-2.0
//! Stateless, revision-bound inspection. No projection or terminal tail is authority.
use crate::{
    history::{self, Access},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use vcp_domain::{artifact::*, revision::*, workspace::Scope};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{Collection, Record, State},
    Store,
};

pub const MAX_RANGE: u32 = 64 * 1024;
const PAGE_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    Chain,
    Context,
    Prompts,
    Outputs,
    Routing,
    Policy,
    Tools,
    Costs,
    Verification,
    Memory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionQuery {
    pub id: String,
    pub view: View,
    pub limit: u32,
    pub cursor: Option<Cursor>,
    /// Content is opt-in and requires an artifact target. Offsets count bytes.
    pub range: Option<RangeRequest>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeRequest {
    pub offset: u64,
    pub length: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub version: u32,
    pub watermark: Watermark,
    pub query_digest: String,
    pub access_digest: String,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionPage {
    pub schema_version: u32,
    pub scope: Scope,
    pub source_watermark: Watermark,
    pub view: View,
    pub items: Vec<Value>,
    pub gaps: Vec<Value>,
    pub next_cursor: Option<Cursor>,
}

fn task(record: &Record) -> Option<&str> {
    record
        .value
        .pointer("/scope/task")
        .or_else(|| record.value.pointer("/spec/scope/task"))
        .or_else(|| record.value.pointer("/data/grant/scope/scope/task"))
        .and_then(Value::as_str)
}

fn target<'a>(state: &'a State, access: &Access, id: &str) -> Result<(&'a Record, Scope)> {
    history::authorize(state, access)?;
    let mut candidates = state
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.id == id && task(r).is_some());
    // A task ID is also its ledger ID. Prefer the task as navigation origin.
    let record = state
        .records
        .values()
        .find(|r| r.workspace == access.workspace && r.id == id && r.collection == Collection::Task)
        .or_else(|| candidates.next())
        .ok_or(Error::Access)?;
    let scope: Scope = serde_json::from_value(
        record
            .value
            .get("scope")
            .or_else(|| record.value.pointer("/spec/scope"))
            .or_else(|| record.value.pointer("/data/grant/scope/scope"))
            .cloned()
            .ok_or(Error::Access)?,
    )?;
    if !history::allows(access, Some(&scope.task)) {
        return Err(Error::Access);
    }
    Ok((record, scope))
}

fn selected(record: &Record, view: View) -> bool {
    use Collection::*;
    let channel = record
        .value
        .pointer("/spec/channel")
        .and_then(Value::as_str);
    match view {
        View::Chain => matches!(
            record.collection,
            Task | Turn
                | Effect
                | Artifact
                | Verification
                | Approval
                | Ledger
                | Reservation
                | Attempt
                | Settlement
                | Access
                | Workspace
        ),
        View::Context => record.collection == Artifact && channel == Some("evidence"),
        View::Prompts => {
            record.collection == Attempt
                || record.collection == Artifact && channel == Some("request_body")
        }
        View::Outputs => {
            record.collection == Artifact
                && matches!(
                    channel,
                    Some("response" | "stdout" | "stderr" | "child_transcript")
                )
        }
        View::Routing => {
            matches!(record.collection, Attempt | Settlement)
                || record.collection == Artifact && matches!(channel, Some("response" | "evidence"))
        }
        View::Policy => {
            matches!(record.collection, Workspace | Access | Approval)
                || record.collection == Artifact && channel == Some("evidence")
        }
        View::Tools => matches!(record.collection, Effect | Approval | Artifact),
        View::Costs => matches!(
            record.collection,
            Ledger | Reservation | Attempt | Settlement
        ),
        View::Verification => record.collection == Verification,
        View::Memory => record.collection == Claim,
    }
}

/// Stable canonical-key ordering; a changed snapshot explicitly requires restart.
pub fn records(state: &State, access: &Access, query: &InspectionQuery) -> Result<InspectionPage> {
    let workspace = history::authorize(state, access)?;
    let (_, scope) = target(state, access, &query.id)?;
    if query.limit == 0 || query.limit > 128 || query.range.is_some() {
        return Err(Error::Limit);
    }
    let query_digest = digest_bytes(&canonical_bytes(&(
        &access.workspace,
        &query.id,
        query.view,
        query.limit,
    ))?);
    let access_digest = digest_bytes(&canonical_bytes(&access.tasks)?);
    let expected = Cursor {
        version: 1,
        watermark: state.watermark,
        query_digest,
        access_digest,
        authority: workspace.authority,
        deletion: workspace.deletion,
        after: String::new(),
    };
    if let Some(cursor) = &query.cursor {
        let mut checked = expected.clone();
        checked.after = cursor.after.clone();
        if cursor != &checked {
            return Err(Error::Restart("inspection source, query or access changed"));
        }
    }
    let masks = history::masks(state, &access.workspace)?;
    if masks.iter().any(|m| m.deletion > workspace.deletion) {
        return Err(Error::Integrity("retention epoch"));
    }
    let mut page = InspectionPage {
        schema_version: 1,
        scope,
        source_watermark: state.watermark,
        view: query.view,
        items: vec![],
        gaps: vec![],
        next_cursor: None,
    };
    if query.view == View::Memory {
        page.gaps.push(json!({"visibility":"unavailable","reason":"memory search and evidence navigation are not ready; P5-06"}));
    }
    if query.view == View::Routing {
        page.gaps.push(json!({"visibility":"unavailable","reason":"fixed qualified model; grouped routing decisions and exclusions are not ready", "requested_model":"attempt.quote.price.model", "served_model":"captured response bytes when observed; never inferred from requested model"}));
    }
    let after = query.cursor.as_ref().map_or("", |c| c.after.as_str());
    let mut bytes = 0;
    let mut last = String::new();
    for (key, record) in &state.records {
        if key.as_str() <= after || record.workspace != access.workspace {
            continue;
        }
        let scoped = task(record) == Some(page.scope.task.as_str());
        let shared = matches!(
            record.collection,
            Collection::Workspace | Collection::Access
        ) && task(record).is_none()
            && access.tasks.is_none();
        if !(scoped || shared) || !selected(record, query.view) {
            continue;
        }
        let removed = masks
            .iter()
            .find(|m| m.artifacts.iter().any(|id| id.as_str() == record.id));
        let mut item = if let Some(mask) = removed {
            json!({"reference":key,"visibility":"pruned","reason":mask.reason,"source":record.value.pointer("/spec/source")})
        } else if record.collection == Collection::Claim
            && record.value["document_type"] == "vcp_ingestion_job_v1"
        {
            let job: vcp_domain::ingestion::Job = record.decode()?;
            job.validate()?;
            let source_pruned = state
                .events
                .iter()
                .find(|event| event.event.id == job.origin)
                .is_none_or(|event| {
                    masks.iter().any(|mask| {
                        mask.session == event.event.session
                            && event.sequence >= mask.first
                            && event.sequence <= mask.last
                    })
                });
            json!({"reference":key,"id":job.id,"visibility":"available","state":job.state,
                "origin_watermark":job.origin_watermark,"attempts":job.attempts,"max_attempts":job.max_attempts,
                "not_before":job.not_before,"lease":job.lease,"results":job.results,
                "finding":if source_pruned {None} else {job.finding.as_ref()},
                "last_failure":if source_pruned {None} else {job.last_failure.as_ref()},
                "origin_visibility":if source_pruned {"pruned"} else {"retained"}})
        } else if record.collection == Collection::Claim
            && record.value["document_type"] == "vcp_ingestion_cursor_v1"
        {
            let cursor: vcp_domain::ingestion::Cursor = record.decode()?;
            cursor.validate()?;
            json!({"reference":key,"visibility":"available","cursor":cursor})
        } else if record.collection == Collection::Claim {
            // Raw claim payloads may derive from currently hidden or pruned
            // sources. A generic inspector exposes identity, never those bytes.
            json!({"reference":key,"collection":record.collection,"id":record.id,"revision":record.revision,
                "visibility":"governed_query_required","reason":"use governed memory history for current source authorization"})
        } else {
            json!({"reference":key,"collection":record.collection,"id":record.id,"revision":record.revision,
                "visibility":"available","references":record.required_references()?,"record":record.value})
        };
        if removed.is_none() && record.collection == Collection::Attempt {
            item["model_identity"] = json!({
                "requested": record.value.pointer("/quote/price/model"),
                "requested_provider": record.value.pointer("/quote/price/provider"),
                "served": null,
                "served_visibility": "not_normalized",
                "served_source": "inspect captured response artifacts; requested identity is not proof of served identity"
            });
        }
        let mut size = canonical_bytes(&item)?.len();
        if size > PAGE_BYTES {
            item = json!({"reference":key,"visibility":"truncated","reason":"record exceeds inspection page capacity","serialized_bytes":size});
            size = canonical_bytes(&item)?.len();
        }
        if page.items.len() == query.limit as usize || bytes + size > PAGE_BYTES {
            let mut next = expected.clone();
            next.after = last.clone();
            page.next_cursor = Some(next);
            break;
        }
        bytes += size;
        last = key.clone();
        page.items.push(item);
    }
    if query.view == View::Chain && page.next_cursor.is_none() {
        for event in &state.events {
            let key = format!("z:event:{:020}", event.watermark.get());
            if key.as_str() <= after
                || event.event.workspace != access.workspace
                || event.event.task.as_ref() != Some(&page.scope.task)
            {
                continue;
            }
            let removed = masks.iter().find(|m| {
                m.session == event.event.session
                    && event.sequence >= m.first
                    && event.sequence <= m.last
            });
            let mut item = if let Some(mask) = removed {
                json!({"reference":key,"visibility":"pruned","reason":mask.reason})
            } else {
                json!({"reference":key,"visibility":"available","event":history::public_event(event)})
            };
            let mut size = canonical_bytes(&item)?.len();
            if size > PAGE_BYTES {
                item = json!({"reference":key,"visibility":"truncated","reason":"event exceeds inspection page capacity","serialized_bytes":size});
                size = canonical_bytes(&item)?.len();
            }
            if page.items.len() == query.limit as usize || bytes + size > PAGE_BYTES {
                let mut next = expected.clone();
                next.after = last.clone();
                page.next_cursor = Some(next);
                break;
            }
            bytes += size;
            last = key;
            page.items.push(item);
        }
    }
    Ok(page)
}

/// Run under the canonical owner's serialization so current access/retention
/// cannot change between the check and the returned range.
pub fn inspect(store: &Store, access: &Access, query: &InspectionQuery) -> Result<InspectionPage> {
    let Some(range) = &query.range else {
        return records(store.state(), access, query);
    };
    if range.length == 0 || range.length > MAX_RANGE || query.cursor.is_some() {
        return Err(Error::Limit);
    }
    let workspace = history::authorize(store.state(), access)?;
    let (record, scope) = target(store.state(), access, &query.id)?;
    if record.collection != Collection::Artifact {
        return Err(Error::Integrity("range target must be an artifact"));
    }
    let descriptor: ArtifactDescriptor = record.decode()?;
    let masks = history::masks(store.state(), &access.workspace)?;
    if masks.iter().any(|m| m.deletion > workspace.deletion) {
        return Err(Error::Integrity("retention epoch"));
    }
    let mut page = InspectionPage {
        schema_version: 1,
        scope,
        source_watermark: store.state().watermark,
        view: query.view,
        items: vec![],
        gaps: vec![],
        next_cursor: None,
    };
    if let Some(mask) = masks
        .iter()
        .find(|m| m.artifacts.contains(&descriptor.spec.id))
    {
        page.gaps.push(json!({"artifact":descriptor.spec.id,"source":descriptor.spec.source,"visibility":"pruned","reason":mask.reason,"requested_range":range}));
        return Ok(page);
    }
    if range.offset > descriptor.length.get() {
        return Err(Error::Limit);
    }
    let end = range
        .offset
        .saturating_add(range.length as u64)
        .min(descriptor.length.get());
    // The spool verifies the full digest while retaining only this bounded range.
    // Its immutable chunk format has no authenticated random-access index yet.
    let mut sink = RangeSink {
        start: range.offset,
        end,
        position: 0,
        bytes: Vec::new(),
    };
    match store.spool().read(&descriptor, &mut sink) {
        Ok(()) => {}
        Err(vcp_store::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            page.gaps.push(json!({"artifact":descriptor.spec.id,"source":descriptor.spec.source,"visibility":"missing","requested_range":range}));
            return Ok(page);
        }
        Err(e) => return Err(e.into()),
    }
    // Byte arrays preserve split UTF-8 exactly. Adapters JSON-escape the optional text.
    let text = std::str::from_utf8(&sink.bytes).ok();
    page.items.push(
        json!({"artifact":descriptor.spec.id,"descriptor":descriptor,
        "visibility":"available","representation":"captured_bytes","encoding":"byte_array",
        "range":{"start":range.offset,"end":end},"bytes":sink.bytes,"text":text,
        "next_offset":(end < descriptor.length.get()).then_some(end)}),
    );
    if !descriptor.spec.omissions.is_empty() || descriptor.state != CaptureState::Complete {
        page.gaps.push(json!({"artifact":descriptor.spec.id,"visibility":"omitted","capture_state":descriptor.state,"omissions":descriptor.spec.omissions,"reason":"only retained observed bytes are available; not reconstructed"}));
    }
    let excluded: Vec<_> = descriptor
        .spec
        .omissions
        .iter()
        .filter(|omission| {
            matches!(
                omission,
                Omission::AuthenticationHeaders | Omission::RecoveryMaterial
            )
        })
        .collect();
    if !excluded.is_empty() {
        page.gaps.push(
            json!({"artifact":descriptor.spec.id,"visibility":"redacted","omissions":excluded,
            "range":null,"reason":"excluded at capture boundary; no retained byte offsets exist"}),
        );
    }
    Ok(page)
}

struct RangeSink {
    start: u64,
    end: u64,
    position: u64,
    bytes: Vec<u8>,
}
impl std::io::Write for RangeSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.position + bytes.len() as u64;
        let start = self.start.max(self.position);
        let end = self.end.min(next);
        if start < end {
            self.bytes.extend_from_slice(
                &bytes[(start - self.position) as usize..(end - self.position) as usize],
            );
        }
        self.position = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
