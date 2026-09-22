// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde_json::{json, Value};
/// The same literal schema is hashed in approvals and advertised to the gateway.
pub fn definition(name: &str) -> Result<Value> {
    let (description,properties,required)=match name {
        "vcp_read"=>("Read one workspace-relative UTF-8 file path, not an artifact ID. Include all four arguments. max_bytes must be a JSON integer; start_line and end_line must each be a JSON integer or null, never a quoted number or an empty string. For a whole-file read use {\"path\":\"relative/file.txt\",\"max_bytes\":65536,\"start_line\":null,\"end_line\":null} (max_bytes at most 1048576). Otherwise line bounds are 1-based and inclusive. Ranges expose a byte-bounded part of a source up to 64 MiB, with full source version and next_line continuation; complete means the entire file was returned.",json!({"path":{"type":"string"},"max_bytes":{"type":"integer"},"start_line":{"type":["integer","null"]},"end_line":{"type":["integer","null"]}}),vec!["path","max_bytes","start_line","end_line"]),
        "vcp_list"=>("List a scoped directory, at most 10000 entries. Use an empty path for the registered root.",json!({"path":{"type":"string"},"max_entries":{"type":"integer"}}),vec!["path","max_entries"]),
        "vcp_search"=>("Search scoped text one line at a time; literal is the default, regex supports anchors, alternation and Unicode. Include all six arguments. max_hits must be a JSON integer; max_files and max_scan_bytes must each be a JSON integer or null, never a quoted number or an empty string. Use null for default optional bounds, for example {\"query\":\"symbol\",\"max_hits\":20,\"mode\":null,\"path_pattern\":null,\"max_files\":null,\"max_scan_bytes\":null}. Optional path_pattern is a regex over normalized root-relative paths (use / separators). Discovery honors ignores and is bounded to 10000 entries, 1 MiB per file and 8 MiB total; max_files and max_scan_bytes can lower these ceilings. Completeness and exclusions are explicit.",json!({"query":{"type":"string"},"max_hits":{"type":"integer"},"mode":{"type":["string","null"],"enum":["literal","regex",null]},"path_pattern":{"type":["string","null"]},"max_files":{"type":["integer","null"]},"max_scan_bytes":{"type":["integer","null"]}}),vec!["query","max_hits","mode","path_pattern","max_files","max_scan_bytes"]),
        "vcp_patch"=>("Prepare a literal patch string using this format:\n*** Begin Patch\n*** Update File: relative/path\n@@\n-old text\n+new text\n*** End Patch\nUse workspace-relative paths. Prefix unchanged context lines with a space. Add files with *** Add File: path and + lines; delete with *** Delete File: path. Read current source first: exact unambiguous matches and current file versions are required; authority is checked separately.",json!({"patch":{"type":"string"}}),vec!["patch"]),
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
