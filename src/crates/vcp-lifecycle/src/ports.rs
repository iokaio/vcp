// SPDX-License-Identifier: Apache-2.0
// Attributed behavioral adaptation of Google Gemini CLI, Copyright 2025–2026
// Google LLC, revision 6a466a7e2fe2b1255752c1e74f69b31f0216084d:
// policy/stable-stringify.ts and scheduler/state-manager.ts. See the component
// provenance record. Rust authority, resource and receipt rules are VCP additions.
//! Neutral P0 preparation/scheduling state; never launches tools or models.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Qualified JSON subset: ASCII object keys and integral safe JS numbers.
/// Deliberately rejects JavaScript-only values and cross-language numeric edges.
pub fn canonical(value: &Value) -> Result<String, String> {
    canonical_inner(value, true)
}
fn canonical_inner(value: &Value, top: bool) -> Result<String, String> {
    Ok(match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let mut entries = Vec::new();
            for key in keys {
                if !key.is_ascii() {
                    return Err("non-ASCII keys are outside the qualified subset".into());
                }
                let pair = format!(
                    "{}:{}",
                    serde_json::to_string(key).unwrap(),
                    canonical_inner(&map[key], false)?
                );
                entries.push(if top { format!("\0{pair}\0") } else { pair });
            }
            format!("{{{}}}", entries.join(","))
        }
        Value::Array(array) => format!(
            "[{}]",
            array
                .iter()
                .map(|v| canonical_inner(v, false))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        Value::Number(number) => {
            let n = number.as_i64().ok_or("non-integral/oversized number")?;
            if n.unsigned_abs() > 9_007_199_254_740_991 {
                return Err("unsafe JS integer".into());
            }
            n.to_string()
        }
        _ => value.to_string(),
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub revision: u64,
    pub hash: String,
}
#[derive(Clone, Debug)]
struct Call {
    revision: u64,
    hash: String,
    resources: BTreeSet<String>,
    admitted: bool,
    cancelled: bool,
    terminal: Option<String>,
}
#[derive(Default)]
pub struct PreparedCalls {
    calls: BTreeMap<String, Call>,
    completed: Vec<(String, String)>,
}
impl PreparedCalls {
    pub fn prepare(
        &mut self,
        id: &str,
        args: &Value,
        resources: &[String],
    ) -> Result<Approval, String> {
        if id.is_empty() || self.calls.contains_key(id) {
            return Err("duplicate/empty call".into());
        }
        let hash = super::integration::digest(canonical(args)?.as_bytes());
        self.calls.insert(
            id.into(),
            Call {
                revision: 0,
                hash: hash.clone(),
                resources: resources.iter().cloned().collect(),
                admitted: false,
                cancelled: false,
                terminal: None,
            },
        );
        Ok(Approval { revision: 0, hash })
    }
    pub fn rewrite(&mut self, id: &str, args: &Value) -> Result<Approval, String> {
        let hash = super::integration::digest(canonical(args)?.as_bytes());
        let call = self.calls.get_mut(id).ok_or("unknown call")?;
        if call.admitted || call.cancelled || call.terminal.is_some() {
            return Err("call no longer preparable".into());
        }
        call.revision = call.revision.checked_add(1).ok_or("revision overflow")?;
        call.hash = hash.clone();
        Ok(Approval {
            revision: call.revision,
            hash,
        })
    }
    /// A trusted caller supplies the ceiling decision and durable intent ack.
    /// The DTO cannot assert user/client authority for itself.
    pub fn admit(
        &mut self,
        id: &str,
        approval: &Approval,
        ceiling: bool,
        durable: bool,
    ) -> Result<(), String> {
        let call = self.calls.get(id).ok_or("unknown call")?;
        if !ceiling
            || !durable
            || call.cancelled
            || call.admitted
            || call.terminal.is_some()
            || call.hash != approval.hash
            || call.revision != approval.revision
        {
            return Err("authority or prepared revision rejected".into());
        }
        if self.calls.values().any(|other| {
            other.admitted
                && other.terminal.is_none()
                && !other.resources.is_disjoint(&call.resources)
        }) {
            return Err("resource busy".into());
        }
        self.calls.get_mut(id).unwrap().admitted = true;
        Ok(())
    }
    pub fn cancel(&mut self, id: &str) -> Result<(), String> {
        let call = self.calls.get_mut(id).ok_or("unknown call")?;
        if call.terminal.is_some() {
            return Err("terminal call".into());
        }
        call.cancelled = true;
        if !call.admitted {
            call.terminal = Some("cancelled".into());
            self.completed.push((id.into(), "cancelled".into()));
        }
        // An executing cancellation keeps its resource until an observed receipt.
        Ok(())
    }
    pub fn receipt(&mut self, id: &str, outcome: &str) -> Result<(), String> {
        let call = self.calls.get_mut(id).ok_or("unknown call")?;
        if !call.admitted || call.terminal.is_some() {
            return Err("receipt without live dispatch".into());
        }
        let result = if call.cancelled {
            format!("cancelled:{outcome}")
        } else {
            outcome.into()
        };
        call.terminal = Some(result.clone());
        self.completed.push((id.into(), result));
        Ok(())
    }
    pub fn completed(&self) -> &[(String, String)] {
        &self.completed
    }
}
