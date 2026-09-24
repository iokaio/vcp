// SPDX-License-Identifier: Apache-2.0
//! Task-relevant policy observations are never operation admission capabilities.
use crate::methods::{Counter, Id, Scope, TaskStatus, Trust};
use serde::{Deserialize, Serialize};
pub const CAPABILITY: &str = "policy/inspection/1";
macro_rules! dto { ($name:ident,$wire:literal {$($(#[$attr:meta])* $field:ident:$ty:ty),*$(,)?}) => {
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
    #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub struct $name {$($(#[$attr])* pub $field:$ty),*}
}; }
macro_rules! enumeration { ($name:ident,$wire:literal {$($variant:ident),*}) => {
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
    #[serde(rename_all="snake_case")]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub enum $name {$($variant),*}
}; }
enumeration!(Section,"PolicySection"{Denials,Grants});
enumeration!(Mode,"PolicyMode"{Plan,Ask,Workspace,Autonomous});
enumeration!(Effect,"PolicyEffect"{Read,Write,Execute,Network,Install,Publish,Opaque});
enumeration!(Origin,"PolicyOrigin"{Host,User});
enumeration!(Layer,"PolicyLayer"{Host,Canonical});
enumeration!(Source,"TaskPolicySource"{Workspace,ChildInherited});
enumeration!(Unavailable,"TaskPolicyUnavailable"{PolicyAbsent,BindingUnavailable,ChildScopeUnavailable});
enumeration!(Assessment,"PolicyAssessment"{OperationNotEvaluated});
enumeration!(GrantVisibility,"PolicyGrantVisibility"{TaskAndInheritedOnly});
dto!(Request,"PolicyRead"{scope:Scope,task:Id,section:Section,#[cfg_attr(feature="schema",schemars(range(min=1,max=32)))] limit:u32,#[cfg_attr(feature="schema",schemars(length(min=1,max=4096)))] cursor:Option<String>});
pub fn validate_request(p: &Request) -> Result<(), &'static str> {
    if !(1..=32).contains(&p.limit)
        || p.cursor
            .as_ref()
            .is_some_and(|c| c.is_empty() || c.len() > 4096 || !c.is_ascii() || c.contains('\0'))
    {
        return Err("policy page bounds");
    }
    Ok(())
}
dto!(Text,"PolicyText"{#[cfg_attr(feature="schema",schemars(length(max=512)))] text:String,truncated:bool});
dto!(Summary,"PolicySummary"{revision:Counter,mode:Mode,#[cfg_attr(feature="schema",schemars(length(max=64)))] workspace_roots:Vec<Id>,#[cfg_attr(feature="schema",schemars(length(max=7)))] automatic_effects:Vec<Effect>,timeout_ceiling_ms:Counter,output_ceiling_bytes:Counter});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "TaskEffectivePolicy"))]
pub enum Effective {
    Observed {
        source: Source,
        policy: Summary,
        host_denial_count: Counter,
    },
    Unavailable {
        reason: Unavailable,
    },
}
dto!(Denial,"PolicyDenial"{id:Text,layer:Layer,origin:Origin,reason:Text,#[cfg_attr(feature="schema",schemars(length(max=7)))] effects:Vec<Effect>,tool:Option<Text>,#[cfg_attr(feature="schema",schemars(length(max=64)))] roots:Vec<Id>,#[cfg_attr(feature="schema",schemars(length(max=16)))] paths:Vec<Text>,path_count:Counter});
dto!(Matches,"PolicyGrantMatches"{host:bool,binding:bool,authority:bool,policy:bool,unexpired:bool,not_revoked:bool});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "PolicyGrantScope"))]
pub enum GrantScope {
    Workspace { workspace: Id },
    Session { scope: Scope },
    Task { scope: Scope, task: Id },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "PolicyGrantTarget"))]
pub enum Target {
    Exact {
        operation_sha256: String,
    },
    Configured {
        tool: Text,
        schema_sha256: String,
        arguments_sha256: String,
        #[cfg_attr(feature = "schema", schemars(length(max = 7)))]
        effects: Vec<Effect>,
        #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
        roots: Vec<Id>,
        #[cfg_attr(feature = "schema", schemars(length(max = 16)))]
        paths: Vec<Text>,
        path_count: Counter,
        timeout_ceiling_ms: Counter,
        output_ceiling_bytes: Counter,
    },
}
dto!(Grant,"PolicyGrant"{id:Id,revision:Counter,actor:Id,scope:GrantScope,host:Id,binding_revision:Counter,authority_revision:Counter,policy_revision:Counter,expires_at_ms:Counter,revoked:bool,origin:Origin,reason:Text,approval:Option<Id>,inherited_from:Option<Id>,current_matches:Matches,target:Target});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "PolicyRow"))]
pub enum Row {
    Denial { value: Denial },
    Grant { value: Grant },
}
dto!(Page,"PolicyPage"{scope:Scope,task:Id,watermark:Counter,observed_at_ms:Counter,authority_revision:Counter,deletion_revision:Counter,binding_revision:Counter,host:Id,task_revision:Counter,steering_revision:Counter,task_state:TaskStatus,trust:Trust,persisted:Option<Summary>,effective:Effective,assessment:Assessment,grant_visibility:GrantVisibility,section:Section,#[cfg_attr(feature="schema",schemars(length(max=32)))] rows:Vec<Row>,next_cursor:Option<String>,complete:bool});

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_unknown_fields_fail_closed() {
        let input = serde_json::json!({"scope":{"workspace":"w","session":"s"},"task":"t","section":"grants","limit":32,"cursor":null});
        let mut p: Request = serde_json::from_value(input.clone()).unwrap();
        assert!(validate_request(&p).is_ok());
        for limit in [0, 33] {
            p.limit = limit;
            assert!(validate_request(&p).is_err());
        }
        p.limit = 1;
        p.cursor = Some("x".repeat(4097));
        assert!(validate_request(&p).is_err());
        let mut bad = input;
        bad["controller"] = true.into();
        assert!(serde_json::from_value::<Request>(bad).is_err());
        assert!(serde_json::from_str::<Assessment>("\"allowed\"").is_err());
    }
}
