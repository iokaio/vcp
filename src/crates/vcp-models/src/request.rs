// SPDX-License-Identifier: Apache-2.0
use crate::{catalog::Snapshot, *};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::manifest::{Content, Envelope, Kind, Part, VerifiedContext};
use vcp_domain::{Timestamp, Units};

#[derive(Clone, Debug)]
pub struct Tools(BTreeMap<String, Value>);
impl Tools {
    pub fn parse(value: &Value) -> Result<Self> {
        let rows = value
            .as_array()
            .ok_or(Error::Capability("function tools array"))?;
        if rows.len() > 64 || vcp_protocol::canonical_bytes(value)?.len() > 1024 * 1024 {
            return Err(Error::Limit("tool schemas"));
        }
        let mut tools = BTreeMap::new();
        for row in rows {
            let object = row.as_object().ok_or(Error::Capability("function tool"))?;
            if object.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "type" | "name" | "description" | "parameters" | "strict"
                )
            }) || row["type"].as_str() != Some("function")
            {
                return Err(Error::Capability("local function tools only"));
            }
            let name = row["name"]
                .as_str()
                .filter(|n| {
                    !n.is_empty()
                        && n.len() <= 64
                        && n.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
                .ok_or(Error::Capability("tool name"))?;
            if row
                .get("strict")
                .is_some_and(|v| !v.is_null() && !v.is_boolean())
                || row.get("description").is_some_and(|v| !v.is_string())
            {
                return Err(Error::Capability("tool options"));
            }
            let mut nodes = 0;
            schema(&row["parameters"], 0, &mut nodes)?;
            if row["parameters"]["type"].as_str() != Some("object")
                || tools
                    .insert(name.into(), row["parameters"].clone())
                    .is_some()
            {
                return Err(Error::Capability("unique object function schema"));
            }
        }
        Ok(Self(tools))
    }
    pub fn validate_call(&self, name: &str, arguments: &str) -> Result<Value> {
        if arguments.len() > 256 * 1024 {
            return Err(Error::Limit("function arguments"));
        }
        let value: Value = serde_json::from_str(arguments)?;
        let schema = self
            .0
            .get(name)
            .ok_or(Error::Protocol("unknown function"))?;
        conforms(schema, &value, 0)?;
        Ok(value)
    }
}
fn schema(value: &Value, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes += 1;
    if depth > 8 || *nodes > 512 {
        return Err(Error::Limit("schema structure"));
    }
    let object = value
        .as_object()
        .ok_or(Error::Capability("schema object"))?;
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "type"
                | "description"
                | "properties"
                | "required"
                | "additionalProperties"
                | "items"
                | "enum"
        )
    }) {
        return Err(Error::Capability("unsupported schema keyword"));
    }
    let (kind, _) = schema_kind(value)?;
    if !matches!(
        kind,
        "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
    ) {
        return Err(Error::Capability("schema type"));
    }
    if let Some(values) = value.get("enum") {
        if values
            .as_array()
            .is_none_or(|a| a.is_empty() || a.len() > 128)
        {
            return Err(Error::Capability("enum"));
        }
    }
    if kind == "object" {
        let properties = value["properties"]
            .as_object()
            .ok_or(Error::Capability("explicit object properties"))?;
        if value["additionalProperties"] != false {
            return Err(Error::Capability("bounded object keys required"));
        }
        let mut seen = BTreeSet::new();
        if let Some(required) = value.get("required") {
            for name in required
                .as_array()
                .ok_or(Error::Capability("required keys"))?
            {
                let name = name.as_str().ok_or(Error::Capability("required key"))?;
                if !properties.contains_key(name) || !seen.insert(name) {
                    return Err(Error::Capability("required key identity"));
                }
            }
        }
        for nested in properties.values() {
            schema(nested, depth + 1, nodes)?;
        }
    } else if kind == "array" {
        schema(&value["items"], depth + 1, nodes)?;
    }
    Ok(())
}
fn schema_kind(schema: &Value) -> Result<(&str, bool)> {
    if let Some(kind) = schema["type"].as_str() {
        return Ok((kind, false));
    }
    // Explicit nullable strings cover bounded terminal input. General unions
    // require their own compatibility qualification.
    if let Some(kinds) = schema["type"].as_array() {
        if kinds.len() == 2
            && kinds.contains(&Value::String("string".into()))
            && kinds.contains(&Value::String("null".into()))
        {
            return Ok(("string", true));
        }
    }
    Err(Error::Capability("explicit supported schema type"))
}
fn conforms(schema: &Value, value: &Value, depth: usize) -> Result<()> {
    if depth > 8 {
        return Err(Error::Limit("argument nesting"));
    }
    if schema
        .get("enum")
        .is_some_and(|e| !e.as_array().unwrap().contains(value))
    {
        return Err(Error::Protocol("argument enum"));
    }
    let (kind, nullable) = schema_kind(schema)?;
    if nullable && value.is_null() {
        return Ok(());
    }
    let valid = match kind {
        "object" => {
            let values = value
                .as_object()
                .ok_or(Error::Protocol("object arguments"))?;
            let properties = schema["properties"].as_object().unwrap();
            if let Some(required) = schema["required"].as_array() {
                if required
                    .iter()
                    .any(|k| !values.contains_key(k.as_str().unwrap()))
                {
                    return Err(Error::Protocol("missing required argument"));
                }
            }
            for (key, value) in values {
                conforms(
                    properties
                        .get(key)
                        .ok_or(Error::Protocol("unknown argument"))?,
                    value,
                    depth + 1,
                )?;
            }
            true
        }
        "array" => {
            let values = value.as_array().ok_or(Error::Protocol("array argument"))?;
            if values.len() > 4096 {
                return Err(Error::Limit("argument array"));
            }
            for item in values {
                conforms(&schema["items"], item, depth + 1)?;
            }
            true
        }
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::Protocol("argument type"))
    }
}

pub fn envelope(
    snapshot: &Snapshot,
    output: Units,
    margin: Units,
    now: Timestamp,
) -> Result<Envelope> {
    snapshot.current(now)?;
    if output == Units::ZERO || output > snapshot.max_output {
        return Err(Error::Capability("output ceiling"));
    }
    Ok(Envelope {
        model: snapshot.compatibility.model.clone(),
        catalog: snapshot.id.clone(),
        compatibility: snapshot.compatibility.id.clone(),
        context: Units::new(
            snapshot
                .context
                .get()
                .min(snapshot.max_input.get().saturating_add(output.get())),
        ),
        output,
        overhead: Units::ZERO,
        margin,
        supports_tools: true,
        preserves_trust: true,
    })
}

/// Deterministic text/function Responses conversion. Inputs cannot inject body
/// options, provider plugins, remote tools, credentials or gateway fallbacks.
pub fn encode(
    parts: &[Part],
    envelope: &Envelope,
    schemas: &Value,
    snapshot: &Snapshot,
) -> Result<Vec<u8>> {
    let expected = self::envelope(
        snapshot,
        envelope.output,
        envelope.margin,
        snapshot.observed_at,
    )?;
    if envelope != &expected
        || envelope.model != snapshot.compatibility.model
        || envelope.catalog != snapshot.id
        || envelope.compatibility != snapshot.compatibility.id
        || envelope.output > snapshot.max_output
    {
        return Err(Error::Stale);
    }
    let tools = Tools::parse(schemas)?;
    let mut input = Vec::new();
    for part in parts {
        match &part.content {
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                let arguments = String::from_utf8(vcp_protocol::canonical_bytes(arguments)?)
                    .map_err(|_| Error::Protocol("arguments encoding"))?;
                tools.validate_call(name, &arguments)?;
                input.push(
                    json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments}),
                );
            }
            Content::ToolResult { id, output } => {
                input.push(json!({"type":"function_call_output","call_id":id,"output":output}))
            }
            Content::Text { text } => {
                let role = match part.kind {
                    Kind::Operating => "system",
                    Kind::ProjectInstruction | Kind::Skill => "developer",
                    _ => "user",
                };
                // Delimit untrusted text as JSON with provenance, never as body
                // fields or an operating-role string chosen by that text.
                let quoted = serde_json::to_string(
                    &json!({"kind":part.kind,"trust":part.trust,"artifact":part.artifact,"source_sha256":part.source_hash,"range":[part.start,part.end],"applicable_paths":part.applicable_paths,"text":text}),
                )?;
                input.push(json!({"type":"message","role":role,"content":[{"type":"input_text","text":quoted}]}));
            }
        }
    }
    let c = &snapshot.compatibility;
    let price_text = |category| {
        let micros = snapshot.price.rates[&category].micros.get();
        format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
    };
    Ok(vcp_protocol::canonical_bytes(
        &json!({"model":envelope.model,"input":input,"tools":schemas,"tool_choice":"auto","parallel_tool_calls":true,"max_output_tokens":envelope.output.get(),"stream":true,"store":false,"provider":{"only":[c.endpoint],"order":[c.endpoint],"allow_fallbacks":false,"require_parameters":true,"data_collection":if c.deny_data_collection{"deny"}else{"allow"},"zdr":c.require_zdr,"max_price":{"prompt":price_text(vcp_domain::accounting::ChargeCategory::Input),"completion":price_text(vcp_domain::accounting::ChargeCategory::Output),"request":c.request_price_limit}}}),
    )?)
}

/// Rebuild from immutable captured views and require byte identity before the
/// canonical host admits the actual body. This is not a send permit.
pub fn validate_sealed(
    context: &VerifiedContext,
    snapshot: &Snapshot,
    schemas: &Value,
    now: Timestamp,
) -> Result<()> {
    snapshot.current(now)?;
    let sealed = context.sealed();
    let m = &sealed.manifest;
    if m.input_estimate.get() != sealed.body().len() as u64
        || m.input_estimate > snapshot.max_input
        || m.estimate_method != "utf8-byte-ceiling/1"
        || m.schemas_sha256 != vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(schemas)?)
        || encode(&m.included, &m.envelope, schemas, snapshot)? != sealed.body()
    {
        return Err(Error::Stale);
    }
    Ok(())
}
