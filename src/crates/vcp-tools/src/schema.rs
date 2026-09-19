// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde_json::{json, Value};
/// The same literal schema is hashed in approvals and advertised to the gateway.
pub fn definition(name: &str) -> Result<Value> {
    let (description,properties,required)=match name {
        "vcp_read"=>("Read one scoped UTF-8 file, at most 1 MiB.",json!({"path":{"type":"string"},"max_bytes":{"type":"integer"}}),vec!["path","max_bytes"]),
        "vcp_list"=>("List a scoped directory, at most 10000 entries. Use an empty path for the registered root.",json!({"path":{"type":"string"},"max_entries":{"type":"integer"}}),vec!["path","max_entries"]),
        "vcp_search"=>("Search literal text in bounded discovered files; completeness and exclusions are explicit.",json!({"query":{"type":"string"},"max_hits":{"type":"integer"}}),vec!["query","max_hits"]),
        "vcp_patch"=>("Prepare a literal apply_patch change set. Exact unambiguous source matches and current file versions are required; authority is checked separately.",json!({"patch":{"type":"string"}}),vec!["patch"]),
        _=>return Err(Error::Invalid("unknown registered tool")),
    };
    Ok(
        json!({"type":"function","name":name,"description":description,"strict":true,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}),
    )
}
pub fn definitions() -> Value {
    Value::Array(
        ["vcp_read", "vcp_list", "vcp_search", "vcp_patch"]
            .into_iter()
            .map(|name| definition(name).unwrap())
            .collect(),
    )
}
impl Request {
    pub fn from_call(name: &str, arguments: &str) -> Result<Self> {
        definition(name)?;
        if arguments.len() > 256 * 1024 {
            return Err(Error::Invalid("argument ceiling"));
        }
        let mut value: Value = serde_json::from_str(arguments)?;
        let object = value
            .as_object_mut()
            .ok_or(Error::Invalid("object arguments"))?;
        if object.contains_key("tool") {
            return Err(Error::Invalid("tool identity is not an argument"));
        }
        object.insert(
            "tool".into(),
            Value::String(name.strip_prefix("vcp_").unwrap().into()),
        );
        Ok(serde_json::from_value(value)?)
    }
}
