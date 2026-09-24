// SPDX-License-Identifier: Apache-2.0
//! Explicit TOML subset. Never use Codex's layered/credential configuration loader.
use super::{Error, Result};
pub(crate) fn parse(bytes: &[u8]) -> Result<serde_json::Value> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid("source encoding"))?;
    // Bound syntactic nesting before TOML allocates recursive tables. Multiline
    // strings are deliberately outside this narrow MCP preferences subset.
    if text.contains("\"\"\"") || text.contains("'''") {
        return Err(Error::Invalid("multiline TOML is unsupported"));
    }
    let mut depth = 0usize;
    let mut dots = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    for c in text.chars() {
        if c == '\n' {
            comment = false;
            dots = 0;
        }
        if comment {
            continue;
        }
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && q == '"' {
                escaped = true;
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '#' => comment = true,
            '[' | '{' => {
                depth += 1;
                if depth > 16 {
                    return Err(Error::Limit("source depth"));
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            '.' => {
                dots += 1;
                if dots > 16 {
                    return Err(Error::Limit("dotted key depth"));
                }
            }
            _ => {}
        }
    }
    let value: toml::Value = toml::from_str(text).map_err(|_| Error::Invalid("TOML syntax"))?;
    validate_tree(&value, 0, &mut 0)?;
    serde_json::to_value(value).map_err(|_| Error::Invalid("TOML value"))
}

fn validate_tree(value: &toml::Value, depth: usize, count: &mut usize) -> Result<()> {
    *count += 1;
    if depth > 16 || *count > 16_384 {
        return Err(Error::Limit("source tree"));
    }
    match value {
        toml::Value::Array(values) => {
            for value in values {
                validate_tree(value, depth + 1, count)?;
            }
        }
        toml::Value::Table(values) => {
            for value in values.values() {
                validate_tree(value, depth + 1, count)?;
            }
        }
        _ => {}
    }
    Ok(())
}
