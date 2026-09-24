// SPDX-License-Identifier: Apache-2.0
//! Governed history navigation. Event facts and captured content are never wire rows.
use crate::{
    memory_retention::Selector,
    methods::{Counter, Id, Scope},
};
use serde::{Deserialize, Serialize};
pub const CAPABILITY: &str = "history/query/1";
pub const MAX_PAGE_BYTES: usize = 64 * 1024;
macro_rules! dto { ($name:ident, $wire:literal { $($(#[$a:meta])* $field:ident: $ty:ty),* $(,)? }) => {
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
    #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub struct $name { $($(#[$a])* pub $field:$ty),* }
}; }
dto!(Query, "HistoryQuery" {
    scope:Scope, task:Option<Id>, selector:Option<Selector>,
    #[cfg_attr(feature="schema",schemars(length(min=1,max=512)))] text:Option<String>,
    artifact:Option<Id>, expand_compacted:bool,
    #[cfg_attr(feature="schema",schemars(range(min=1,max=128)))] limit:u32,
    #[cfg_attr(feature="schema",schemars(length(min=1,max=8192)))] cursor:Option<String>
});
dto!(Metadata, "HistoryMetadata" { agent:Option<Id>, provider:Option<String>, model:Option<String>, paths:Vec<String> });
dto!(ArtifactLink, "HistoryArtifactLink" { id:Id, availability:String, original_bytes:Option<Counter> });
dto!(Row, "HistoryRow" { id:Id,session:Id,task:Option<Id>,sequence:Counter,timestamp_ms:Counter,kind:String,actor:Id,command_id:Id,visibility:String,content_truncated:bool,recall_excluded:bool,compacted:bool,artifacts:Vec<ArtifactLink>,metadata:Option<Metadata> });
dto!(Gap, "HistoryGap" {
    session: Id,
    first: Counter,
    last: Counter,
    reason: String
});
dto!(ClaimLink, "HistoryClaimLink" {
    origin: Id,
    claim: Id,
    version: Id,
    memory_sequence: Counter
});
dto!(Page, "HistoryPage" { scope:Scope,task:Option<Id>,source_watermark:Counter,observed_watermark:Counter,newer_events:Counter,search_scope:String,rows:Vec<Row>,gaps:Vec<Gap>,claim_links:Vec<ClaimLink>,claim_links_truncated:bool,next_cursor:Option<String>,complete:bool });
impl Query {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=128).contains(&self.limit)
            || self
                .text
                .as_ref()
                .is_some_and(|text| text.is_empty() || text.len() > 512 || text.contains('\0'))
            || self
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 8192)
        {
            return Err("history query bounds");
        }
        if let Some(selector) = &self.selector {
            selector.normalized()?;
        }
        Ok(())
    }
}
pub fn validate_query(value: &Query) -> Result<(), &'static str> {
    value.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_query_bounds_and_unknown_fields_fail_closed() {
        let input = serde_json::json!({"scope":{"workspace":"workspace","session":"session"},"task":null,"selector":null,"text":null,"artifact":null,"expand_compacted":false,"limit":128,"cursor":null});
        let mut query: Query = serde_json::from_value(input.clone()).unwrap();
        assert!(query.validate().is_ok());
        query.limit = 129;
        assert!(query.validate().is_err());
        query.limit = 1;
        query.text = Some("x".repeat(513));
        assert!(query.validate().is_err());
        query.text = None;
        query.cursor = Some("x".repeat(8193));
        assert!(query.validate().is_err());
        let mut unknown = input;
        unknown["raw_facts"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Query>(unknown).is_err());
    }
}
