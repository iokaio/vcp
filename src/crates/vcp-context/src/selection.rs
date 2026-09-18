// SPDX-License-Identifier: Apache-2.0
use crate::manifest::*;
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{ByteCount, Units};
use vcp_repository::instructions::Probe;

pub trait Counter {
    fn count(&self, serialized: &[u8]) -> Result<u64>;
    fn method(&self) -> &str;
    fn estimated(&self) -> bool;
}
/// Explicit conservative estimate for a text-only byte-tokenizing compatibility
/// record. A provider must qualify that assumption; this is never an exact count.
pub struct Utf8ByteCeiling;
impl Counter for Utf8ByteCeiling {
    fn count(&self, serialized: &[u8]) -> Result<u64> {
        Ok(serialized.len() as u64)
    }
    fn method(&self) -> &str {
        "utf8-byte-ceiling/1"
    }
    fn estimated(&self) -> bool {
        true
    }
}

pub fn assemble(
    parts: Vec<Part>,
    revisions: Revisions,
    envelope: Envelope,
    schemas: serde_json::Value,
    instruction_probes: Vec<Probe>,
    counter: &dyn Counter,
    encode: impl Fn(&[Part], &Envelope, &serde_json::Value) -> Result<Vec<u8>>,
) -> Result<Sealed> {
    if parts.len() > 512 || instruction_probes.len() > 4096 {
        return Err(Error::Invalid("manifest bounds"));
    }
    let capacity = envelope.input_capacity()?;
    let mut ids = BTreeSet::new();
    let mut total = 0usize;
    let mut calls = BTreeMap::new();
    let mut results = BTreeSet::new();
    for part in &parts {
        part.validate(&revisions.scope)?;
        if !ids.insert(part.id.clone()) {
            return Err(Error::Invalid("duplicate part ID"));
        }
        total = total
            .checked_add(part.content.bytes()?.len())
            .ok_or(Error::Invalid("content bytes"))?;
        match &part.content {
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                if id.is_empty()
                    || name.is_empty()
                    || !arguments.is_object()
                    || calls.insert(id.clone(), part.id.clone()).is_some()
                {
                    return Err(Error::Invalid("tool call identity/arguments"));
                }
            }
            Content::ToolResult { id, .. } => {
                if !calls.contains_key(id) || !results.insert(id.clone()) {
                    return Err(Error::Invalid("unpaired/duplicate tool result"));
                }
            }
            _ => (),
        }
    }
    if total > 8 * 1024 * 1024 || vcp_protocol::canonical_bytes(&schemas)?.len() > 1024 * 1024 {
        return Err(Error::Invalid("context bytes"));
    }
    if calls.len() != results.len() {
        return Err(Error::Incompatible(
            "unfinished tool pair requires reconciliation or explicit attributed conversion",
        ));
    }
    if !calls.is_empty() && !envelope.supports_tools {
        return Err(Error::Incompatible("tool pairing"));
    }
    for required in [Kind::Operating, Kind::Objective, Kind::TaskState] {
        if !parts.iter().any(|part| part.kind == required) {
            return Err(Error::Invalid("mandatory task foundation missing"));
        }
    }
    let mut selected: Vec<(usize, Part)> = Vec::new();
    let mut excluded = Vec::new();
    let mut optional = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        if part.required() {
            let fragments = uncovered(part, &selected, &mut excluded)?;
            selected.extend(fragments.into_iter().map(|part| (index, part)));
        } else {
            optional.push((index, part));
        }
    }
    // Encode mandatory state and tool pairs together. Intermediate selection
    // never exposes an unpaired call to a provider codec.
    let view = |selected: &[(usize, Part)]| {
        selected
            .iter()
            .map(|(_, part)| part.clone())
            .collect::<Vec<_>>()
    };
    let required_body = encode(&view(&selected), &envelope, &schemas)?;
    if required_body.len() > 16 * 1024 * 1024 || counter.count(&required_body)? > capacity {
        return Err(Error::Capacity);
    }
    optional.sort_by_key(|(index, part)| (part.rank, *index));
    for (index, part) in optional {
        let fragments = uncovered(part, &selected, &mut excluded)?;
        if fragments.is_empty() {
            continue;
        }
        let mut candidate_parts = selected.clone();
        candidate_parts.extend(fragments.iter().cloned().map(|part| (index, part)));
        candidate_parts.sort_by_key(|(index, part)| (*index, part.start));
        if candidate_parts.len() > 4096 {
            return Err(Error::Invalid("selected fragment bounds"));
        }
        let candidate = encode(&view(&candidate_parts), &envelope, &schemas)?;
        if candidate.len() > 16 * 1024 * 1024 || counter.count(&candidate)? > capacity {
            for part in fragments {
                excluded.push(Excluded {
                    id: part.id,
                    reason: "outside selected envelope after serialization".into(),
                    start: part.start,
                    end: part.end,
                });
            }
        } else {
            selected = candidate_parts;
        }
    }
    let included: Vec<_> = selected.into_iter().map(|(_, part)| part).collect();
    let body = encode(&included, &envelope, &schemas)?;
    let estimate = counter.count(&body)?;
    if estimate > capacity || body.len() > 16 * 1024 * 1024 {
        return Err(Error::Capacity);
    }
    let manifest = Manifest {
        version: 1,
        revisions,
        envelope,
        included,
        excluded,
        instruction_probes,
        schemas_sha256: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&schemas)?),
        request_sha256: vcp_protocol::digest_bytes(&body),
        input_estimate: Units::new(estimate),
        estimate_method: counter.method().into(),
        estimated: counter.estimated(),
    };
    Sealed::new(manifest, body)
}

fn slice(part: &Part, start: u64, end: u64) -> Result<Part> {
    let Content::Text { text } = &part.content else {
        return Err(Error::Invalid("non-text evidence overlap"));
    };
    let content = text
        .get((start - part.start.get()) as usize..(end - part.start.get()) as usize)
        .ok_or(Error::Invalid("range splits UTF-8"))?;
    let mut result = part.clone();
    result.id = vcp_protocol::digest_bytes(format!("{}:{start}:{end}", part.id).as_bytes());
    result.start = ByteCount::new(start);
    result.end = ByteCount::new(end);
    result.content = Content::Text {
        text: content.into(),
    };
    Ok(result)
}
fn uncovered(
    part: Part,
    selected: &[(usize, Part)],
    excluded: &mut Vec<Excluded>,
) -> Result<Vec<Part>> {
    if part.kind != Kind::Evidence {
        return Ok(vec![part]);
    }
    let mut fragments = vec![part];
    for (_, old) in selected {
        let mut remaining = Vec::new();
        for part in fragments {
            if old.kind != part.kind
                || old.trust != part.trust
                || old.artifact != part.artifact
                || old.source_hash != part.source_hash
                || old.end <= part.start
                || old.start >= part.end
            {
                remaining.push(part);
                continue;
            }
            let start = old.start.get().max(part.start.get());
            let end = old.end.get().min(part.end.get());
            if slice(old, start, end)?.content != slice(&part, start, end)?.content {
                return Err(Error::Invalid("overlapping source bytes disagree"));
            }
            excluded.push(Excluded {
                id: part.id.clone(),
                reason: "already covered by selected source/version/range".into(),
                start: ByteCount::new(start),
                end: ByteCount::new(end),
            });
            if part.start.get() < start {
                remaining.push(slice(&part, part.start.get(), start)?);
            }
            if part.end.get() > end {
                remaining.push(slice(&part, end, part.end.get())?);
            }
        }
        fragments = remaining;
    }
    Ok(fragments)
}
