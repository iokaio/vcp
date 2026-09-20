// SPDX-License-Identifier: Apache-2.0
//! Closed exact admission profile mcp-schema/2, not general JSON Schema compliance.
use serde_json::Value;
use std::fmt;

mod compiled;
mod exact;

pub const PROFILE: &str = "mcp-schema/2";

/// Additional sensitive-value alias for capture filtering after MCP numeric
/// normalization. Accepts only a whole JSON number token within this profile's
/// numeric bounds; strings, whitespace and compound JSON values return None.
/// This is not authority, admission, or a general secret-matching decision. The
/// caller must also retain the original sensitive value and protect this alias.
pub fn numeric_capture_alias(value: &str) -> Option<String> {
    let mut budget = exact::Budget::new(1024, exact::NUMBER_BYTES);
    exact::Decimal::parse(value, &mut budget)
        .ok()
        .map(|number| number.token())
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub depth: usize,
    pub nodes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024,
            depth: 16,
            nodes: 4096,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("MCP JSON or validation exceeds configured bounds")]
    Bounds,
    #[error("MCP JSON is invalid or contains duplicate or reserved keys")]
    Json,
    #[error("MCP schema uses unsupported or invalid semantics")]
    Schema,
    #[error("MCP arguments do not conform to the admitted schema")]
    Arguments,
}
impl From<exact::Error> for Error {
    fn from(error: exact::Error) -> Self {
        match error {
            exact::Error::Bound => Self::Bounds,
            exact::Error::Syntax | exact::Error::Duplicate | exact::Error::Reserved => Self::Json,
            exact::Error::InvalidDivisor | exact::Error::Schema => Self::Schema,
            exact::Error::Arguments => Self::Arguments,
        }
    }
}
#[derive(Clone)]
pub struct Schema {
    compiled: compiled::Schema,
    digest: String,
}
impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema")
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct CheckedArguments {
    canonical: Vec<u8>,
    digest: String,
    schema_digest: String,
}
impl fmt::Debug for CheckedArguments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckedArguments")
            .field("bytes", &self.canonical.len())
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
impl CheckedArguments {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn schema_digest(&self) -> &str {
        &self.schema_digest
    }
}
impl Schema {
    pub fn compile(bytes: &[u8], limits: Limits) -> Result<Self, Error> {
        let compiled = compiled::Schema::compile(bytes, exact_limits(limits, false)?)?;
        // Bind interpretation as well as data. Protocol canonical v1 is unchanged;
        // this domain separation prevents reuse of an integer-profile schema ID.
        let mut identity = Vec::with_capacity(PROFILE.len() + 1 + compiled.canonical_bytes().len());
        identity.extend_from_slice(PROFILE.as_bytes());
        identity.push(0);
        identity.extend_from_slice(compiled.canonical_bytes());
        Ok(Self {
            compiled,
            digest: vcp_protocol::digest_bytes(&identity),
        })
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        self.compiled.canonical_bytes()
    }
    pub fn arguments(&self, bytes: &[u8]) -> Result<CheckedArguments, Error> {
        let canonical = self.compiled.arguments(bytes)?;
        Ok(CheckedArguments {
            digest: vcp_protocol::digest_bytes(&canonical),
            canonical,
            schema_digest: self.digest.clone(),
        })
    }
}
fn exact_limits(limits: Limits, wire: bool) -> Result<exact::Limits, Error> {
    if limits.bytes == 0
        || limits.bytes > 1024 * 1024
        || limits.depth == 0
        || limits.depth > 32
        || limits.nodes == 0
        || limits.nodes > 65536
    {
        return Err(Error::Bounds);
    }
    Ok(exact::Limits {
        bytes: limits.bytes,
        depth: limits.depth,
        nodes: limits.nodes,
        numeric_digits: if wire { 65536 } else { 16 * 1024 },
        work: if wire { 10_000_000 } else { 1_000_000 },
    })
}
/// Parse an untrusted complete frame into exact Values before any generic Value
/// decoder can reinterpret private serde keys. Numeric metadata remains data.
pub(crate) fn bounded_json(bytes: &[u8], limits: Limits) -> Result<Value, Error> {
    let (value, _, _) = exact::parse(bytes, exact_limits(limits, true)?)?.into_parts();
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_capture_alias_is_exact_bounded_and_whole_token_only() {
        for (input, expected) in [
            ("0.3000", "3e-1"),
            ("-0", "0"),
            ("-0.000e+128", "0"),
            ("1.0", "1"),
            ("1e128", "1e+128"),
            ("1e-128", "1e-128"),
            ("9007199254740993.123", "9007199254740993123e-3"),
        ] {
            assert_eq!(numeric_capture_alias(input).as_deref(), Some(expected));
        }
        for input in [
            "",
            " 0.3",
            "0.3 ",
            "\"0.3000\"",
            "{\"n\":0.3}",
            "[0.3]",
            "true",
            "null",
            "01",
            "+1",
            "1.",
            "1e",
            "1e129",
            "1e-129",
            "NaN",
            "Infinity",
            "0.3 0.4",
        ] {
            assert!(numeric_capture_alias(input).is_none());
        }
        assert!(numeric_capture_alias(&"9".repeat(129)).is_none());
        assert!(numeric_capture_alias(&format!("0.{}", "0".repeat(255))).is_none());
    }
    #[test]
    fn schema_profile_is_domain_separated_from_original_integer_schema_hash() {
        let schema = Schema::compile(br#"{"type":"object"}"#, Limits::default()).unwrap();
        assert_ne!(
            schema.digest(),
            vcp_protocol::digest_bytes(schema.canonical_bytes())
        );
        let mut framed = PROFILE.as_bytes().to_vec();
        framed.push(0);
        framed.extend_from_slice(schema.canonical_bytes());
        assert_eq!(schema.digest(), vcp_protocol::digest_bytes(&framed));
    }
    #[test]
    fn schema_api_retains_bounds_and_exact_checked_identity() {
        assert_eq!(
            Schema::compile(
                br#"{"type":"object"}"#,
                Limits {
                    depth: 0,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
            Error::Bounds
        );
        let schema = Schema::compile(br#"{"type":"object"}"#, Limits::default()).unwrap();
        let args = schema
            .arguments(br#"{"n":0.10000000000000000000001}"#)
            .unwrap();
        assert_eq!(args.schema_digest(), schema.digest());
        assert_eq!(
            args.canonical_bytes(),
            br#"{"n":10000000000000000000001e-23}"#
        );
        assert_eq!(
            args.digest(),
            vcp_protocol::digest_bytes(args.canonical_bytes())
        );
    }
}
