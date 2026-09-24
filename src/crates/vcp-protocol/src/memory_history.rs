// SPDX-License-Identifier: Apache-2.0
//! Bounded governed version history; every continuation requires current access.
use crate::methods::{Counter, Id, MemoryFinding, Scope};
use serde::{Deserialize, Serialize};

pub const CAPABILITY: &str = "memory/history/1";
pub const MAX_LIMIT: u32 = 32;
pub const MAX_CURSOR_BYTES: usize = 4096;
pub const MAX_SUMMARY_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "MemoryHistoryRequest"))]
pub struct Request {
    pub scope: Scope,
    pub task: Id,
    pub claim: Id,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 32)))]
    pub limit: u32,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))]
    pub cursor: Option<String>,
}
pub fn validate_request(request: &Request) -> Result<(), &'static str> {
    if !(1..=MAX_LIMIT).contains(&request.limit)
        || request.cursor.as_ref().is_some_and(|c| {
            c.is_empty() || c.len() > MAX_CURSOR_BYTES || !c.is_ascii() || c.contains('\0')
        })
    {
        return Err("invalid memory history page bounds");
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "MemoryHistoryVersion"))]
pub struct Version {
    pub finding: MemoryFinding,
    /// Retained canonical origin event IDs; unavailable rows reveal no links.
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub origins: Vec<Id>,
    pub content_truncated: bool,
    pub presentation_compacted: bool,
    pub recall_excluded: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "MemoryHistoryPage"))]
pub struct Page {
    pub scope: Scope,
    pub task: Id,
    pub claim: Id,
    pub watermark: Counter,
    /// Stable immutable memory-sequence upper bound, not event sequence.
    pub at: Counter,
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub versions: Vec<Version>,
    pub next_cursor: Option<String>,
    /// Exhausted this version window; not a claim that evidence is available.
    pub complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_bounds_and_unknown_fields_are_rejected() {
        let mut request: Request = serde_json::from_value(serde_json::json!({"scope":{"workspace":"w","session":"s"},"task":"t","claim":"c","limit":1,"cursor":null})).unwrap();
        assert!(validate_request(&request).is_ok());
        for limit in [0, 33, u32::MAX] {
            request.limit = limit;
            assert!(validate_request(&request).is_err());
        }
        request.limit = 32;
        for cursor in [
            String::new(),
            "x".repeat(4097),
            "secret\0".into(),
            "é".into(),
        ] {
            request.cursor = Some(cursor);
            assert!(validate_request(&request).is_err());
        }
        let mut value = serde_json::to_value(&request).unwrap();
        value["authority"] = 1.into();
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
}
