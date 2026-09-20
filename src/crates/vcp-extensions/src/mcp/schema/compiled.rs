// SPDX-License-Identifier: Apache-2.0
//! Closed MCP schema profile. No general JSON Schema compliance claim.
use super::exact::{parse, Budget, Decimal, Error, Limits, Result};
use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

const NODES: usize = 1024;
const REFERENCES: usize = 256;
const GRAPH_DEPTH: usize = 64;
const BRANCHES: usize = 16;
const MEMBERS: usize = 128;

#[derive(Clone)]
pub(super) struct Schema {
    nodes: Vec<Node>,
    root: usize,
    canonical: Vec<u8>,
    limits: Limits,
}
impl std::fmt::Debug for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schema")
            .field("nodes", &self.nodes.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
enum Node {
    Boolean(bool),
    Rule(Box<Rule>),
}
#[derive(Clone, Default)]
struct Rule {
    types: Option<(Kind, bool)>,
    enumeration: Option<Vec<Value>>,
    constant: Option<Value>,
    properties: BTreeMap<String, usize>,
    required: Vec<String>,
    additional: Option<usize>,
    items: Option<usize>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    unique: bool,
    minimum: Option<Decimal>,
    maximum: Option<Decimal>,
    exclusive_minimum: Option<Decimal>,
    exclusive_maximum: Option<Decimal>,
    multiple: Option<Decimal>,
    all: Vec<usize>,
    any: Vec<usize>,
    one: Vec<usize>,
    not: Option<usize>,
    reference: Option<usize>,
}
#[derive(Clone, Copy)]
enum Kind {
    Object,
    Array,
    String,
    Number,
    Integer,
    Boolean,
    Null,
}
impl Schema {
    pub fn compile(bytes: &[u8], limits: Limits) -> Result<Self> {
        let (value, canonical, mut budget) = parse(bytes, limits)?.into_parts();
        let object = value.as_object().ok_or(Error::Schema)?;
        if object.get("type").and_then(Value::as_str) != Some("object") {
            return Err(Error::Schema);
        }
        let mut compiler = Compiler {
            nodes: vec![],
            definitions: BTreeMap::new(),
            references: vec![],
            budget: &mut budget,
        };
        if let Some(defs) = object.get("$defs") {
            let defs = defs.as_object().ok_or(Error::Schema)?;
            if defs.len() > MEMBERS {
                return Err(Error::Bound);
            }
            for (name, definition) in defs {
                compiler.budget.charge(1 + name.len())?;
                if name.len() > 256 {
                    return Err(Error::Bound);
                }
                let node = compiler.node(definition, 0, false)?;
                compiler.definitions.insert(name.clone(), node);
            }
        }
        let root = compiler.node(&value, 0, true)?;
        compiler.resolve()?;
        let mut colors = vec![0; compiler.nodes.len()];
        let mut heights = vec![0; compiler.nodes.len()];
        // Even unused definitions must have valid predicates and an acyclic graph.
        for id in 0..compiler.nodes.len() {
            graph(
                id,
                &compiler.nodes,
                &mut colors,
                &mut heights,
                0,
                compiler.budget,
            )?;
        }
        Ok(Self {
            nodes: compiler.nodes,
            root,
            canonical,
            limits,
        })
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn arguments(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let (value, canonical, mut budget) = parse(bytes, self.limits)?.into_parts();
        if !value.is_object() || !self.accepts(self.root, &value, &mut budget, 0)? {
            return Err(Error::Arguments);
        }
        Ok(canonical)
    }
    /// Private test seam: caller owns the same remaining budget after parsing.
    /// Values must come from the scoped exact parser, not an f64-backed decoder.
    #[cfg(test)]
    fn validates_parsed(&self, value: &Value, budget: &mut Budget) -> Result<bool> {
        if !value.is_object() {
            return Ok(false);
        }
        self.accepts(self.root, value, budget, 0)
    }
    fn accepts(&self, id: usize, value: &Value, budget: &mut Budget, depth: usize) -> Result<bool> {
        if depth > GRAPH_DEPTH {
            return Err(Error::Bound);
        }
        budget.charge(1)?;
        let node = self.nodes.get(id).ok_or(Error::Schema)?;
        let rule = match node {
            Node::Boolean(accepts) => return Ok(*accepts),
            Node::Rule(rule) => rule,
        };
        if let Some((kind, nullable)) = rule.types {
            let accepts = if nullable && value.is_null() {
                true
            } else {
                match kind {
                    Kind::Object => value.is_object(),
                    Kind::Array => value.is_array(),
                    Kind::String => value.is_string(),
                    Kind::Number => value.is_number(),
                    Kind::Integer => match value.as_number() {
                        Some(number) => Decimal::parse(&number.to_string(), budget)?.is_integer(),
                        None => false,
                    },
                    Kind::Boolean => value.is_boolean(),
                    Kind::Null => value.is_null(),
                }
            };
            if !accepts {
                return Ok(false);
            }
        }
        if let Some(options) = &rule.enumeration {
            let mut matches = false;
            for option in options {
                if equal(value, option, budget, 0)? {
                    matches = true;
                    break;
                }
            }
            if !matches {
                return Ok(false);
            }
        }
        if let Some(constant) = &rule.constant {
            if !equal(value, constant, budget, 0)? {
                return Ok(false);
            }
        }
        if let Some(object) = value.as_object() {
            for key in &rule.required {
                budget.charge(1 + key.len())?;
                if !object.contains_key(key) {
                    return Ok(false);
                }
            }
            for (key, value) in object {
                budget.charge(1 + key.len())?;
                if let Some(id) = rule.properties.get(key).copied().or(rule.additional) {
                    if !self.accepts(id, value, budget, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
        }
        if let Some(array) = value.as_array() {
            if rule.min_items.is_some_and(|n| (array.len() as u64) < n)
                || rule.max_items.is_some_and(|n| (array.len() as u64) > n)
            {
                return Ok(false);
            }
            if let Some(id) = rule.items {
                for item in array {
                    if !self.accepts(id, item, budget, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            if rule.unique {
                for (i, left) in array.iter().enumerate() {
                    for right in &array[i + 1..] {
                        if equal(left, right, budget, 0)? {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        if let Some(string) = value.as_str() {
            budget.charge(string.len())?;
            let length = string.chars().count() as u64;
            if rule.min_length.is_some_and(|n| length < n)
                || rule.max_length.is_some_and(|n| length > n)
            {
                return Ok(false);
            }
        }
        if value.is_number()
            && (rule.minimum.is_some()
                || rule.maximum.is_some()
                || rule.exclusive_minimum.is_some()
                || rule.exclusive_maximum.is_some()
                || rule.multiple.is_some())
        {
            let number = Decimal::parse(&value.to_string(), budget)?;
            if let Some(bound) = &rule.minimum {
                if number.compare(bound, budget)? == Ordering::Less {
                    return Ok(false);
                }
            }
            if let Some(bound) = &rule.maximum {
                if number.compare(bound, budget)? == Ordering::Greater {
                    return Ok(false);
                }
            }
            if let Some(bound) = &rule.exclusive_minimum {
                if number.compare(bound, budget)? != Ordering::Greater {
                    return Ok(false);
                }
            }
            if let Some(bound) = &rule.exclusive_maximum {
                if number.compare(bound, budget)? != Ordering::Less {
                    return Ok(false);
                }
            }
            if let Some(divisor) = &rule.multiple {
                if !number.multiple_of(divisor, budget)? {
                    return Ok(false);
                }
            }
        }
        if let Some(reference) = rule.reference {
            if !self.accepts(reference, value, budget, depth + 1)? {
                return Ok(false);
            }
        }
        for id in &rule.all {
            if !self.accepts(*id, value, budget, depth + 1)? {
                return Ok(false);
            }
        }
        if !rule.any.is_empty() {
            let mut found = false;
            for id in &rule.any {
                if self.accepts(*id, value, budget, depth + 1)? {
                    found = true;
                    break;
                }
            }
            if !found {
                return Ok(false);
            }
        }
        if !rule.one.is_empty() {
            let mut found = false;
            for id in &rule.one {
                if self.accepts(*id, value, budget, depth + 1)? {
                    if found {
                        return Ok(false);
                    }
                    found = true;
                }
            }
            if !found {
                return Ok(false);
            }
        }
        if let Some(id) = rule.not {
            if self.accepts(id, value, budget, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
struct Compiler<'a> {
    nodes: Vec<Node>,
    definitions: BTreeMap<String, usize>,
    references: Vec<(usize, String)>,
    budget: &'a mut Budget,
}
impl Compiler<'_> {
    fn node(&mut self, value: &Value, depth: usize, root: bool) -> Result<usize> {
        if depth > GRAPH_DEPTH || self.nodes.len() >= NODES {
            return Err(Error::Bound);
        }
        self.budget.charge(1)?;
        let id = self.nodes.len();
        self.nodes.push(Node::Boolean(false));
        if let Some(value) = value.as_bool() {
            self.nodes[id] = Node::Boolean(value);
            return Ok(id);
        }
        let object = value.as_object().ok_or(Error::Schema)?;
        let mut rule = Rule::default();
        for (key, value) in object {
            self.budget.charge(1 + key.len())?;
            match key.as_str() {
                "$schema"
                    if value.as_str() == Some("https://json-schema.org/draft/2020-12/schema") => {}
                "$defs" if root => (), // All root definitions compiled independently first.
                "title" | "description" | "$comment" if value.is_string() => (),
                "default" => (),
                "examples" if value.as_array().is_some_and(|v| v.len() <= MEMBERS) => (),
                "type" => rule.types = Some(types(value)?),
                "enum" => {
                    let values = value.as_array().ok_or(Error::Schema)?;
                    if values.is_empty() || values.len() > MEMBERS {
                        return Err(Error::Bound);
                    }
                    for (i, left) in values.iter().enumerate() {
                        for right in &values[i + 1..] {
                            if equal(left, right, self.budget, 0)? {
                                return Err(Error::Schema);
                            }
                        }
                    }
                    rule.enumeration = Some(values.clone());
                }
                "const" => rule.constant = Some(value.clone()),
                "properties" => {
                    let values = value.as_object().ok_or(Error::Schema)?;
                    if values.len() > MEMBERS {
                        return Err(Error::Bound);
                    }
                    for (key, value) in values {
                        self.budget.charge(1 + key.len())?;
                        if key.len() > 256 {
                            return Err(Error::Bound);
                        }
                        let child = self.node(value, depth + 1, false)?;
                        rule.properties.insert(key.clone(), child);
                    }
                }
                "required" => {
                    let values = value.as_array().ok_or(Error::Schema)?;
                    if values.len() > MEMBERS {
                        return Err(Error::Bound);
                    }
                    let mut seen = BTreeSet::new();
                    for value in values {
                        let key = value.as_str().ok_or(Error::Schema)?;
                        self.budget.charge(1 + key.len())?;
                        if key.len() > 256 {
                            return Err(Error::Bound);
                        }
                        if !seen.insert(key.to_owned()) {
                            return Err(Error::Schema);
                        }
                        rule.required.push(key.to_owned());
                    }
                }
                "additionalProperties" => {
                    rule.additional = Some(self.node(value, depth + 1, false)?)
                }
                "items" => rule.items = Some(self.node(value, depth + 1, false)?),
                "minItems" => rule.min_items = Some(count(value)?),
                "maxItems" => rule.max_items = Some(count(value)?),
                "minLength" => rule.min_length = Some(count(value)?),
                "maxLength" => rule.max_length = Some(count(value)?),
                "uniqueItems" => rule.unique = value.as_bool().ok_or(Error::Schema)?,
                "minimum" => rule.minimum = Some(number(value, self.budget)?),
                "maximum" => rule.maximum = Some(number(value, self.budget)?),
                "exclusiveMinimum" => rule.exclusive_minimum = Some(number(value, self.budget)?),
                "exclusiveMaximum" => rule.exclusive_maximum = Some(number(value, self.budget)?),
                "multipleOf" => {
                    let divisor = number(value, self.budget)?;
                    if divisor.compare(&Decimal::parse("0", self.budget)?, self.budget)?
                        != Ordering::Greater
                    {
                        return Err(Error::Schema);
                    }
                    rule.multiple = Some(divisor);
                }
                "allOf" => rule.all = self.branches(value, depth)?,
                "anyOf" => rule.any = self.branches(value, depth)?,
                "oneOf" => rule.one = self.branches(value, depth)?,
                "not" => rule.not = Some(self.node(value, depth + 1, false)?),
                "$ref" => {
                    if self.references.len() >= REFERENCES {
                        return Err(Error::Bound);
                    }
                    let name = local_reference(value.as_str().ok_or(Error::Schema)?)?;
                    self.budget.charge(1 + name.len())?;
                    self.references.push((id, name));
                }
                _ => return Err(Error::Schema),
            }
        }
        self.nodes[id] = Node::Rule(Box::new(rule));
        Ok(id)
    }
    fn branches(&mut self, value: &Value, depth: usize) -> Result<Vec<usize>> {
        let values = value.as_array().ok_or(Error::Schema)?;
        if values.is_empty() || values.len() > BRANCHES {
            return Err(Error::Bound);
        }
        let mut result = Vec::new();
        for value in values {
            self.budget.charge(1)?;
            result.push(self.node(value, depth + 1, false)?);
        }
        Ok(result)
    }
    fn resolve(&mut self) -> Result<()> {
        for (id, name) in &self.references {
            self.budget.charge(1 + name.len())?;
            let target = *self.definitions.get(name).ok_or(Error::Schema)?;
            let Some(Node::Rule(rule)) = self.nodes.get_mut(*id) else {
                return Err(Error::Schema);
            };
            rule.reference = Some(target);
        }
        Ok(())
    }
}
fn kind(value: &str) -> Result<Kind> {
    Ok(match value {
        "object" => Kind::Object,
        "array" => Kind::Array,
        "string" => Kind::String,
        "number" => Kind::Number,
        "integer" => Kind::Integer,
        "boolean" => Kind::Boolean,
        "null" => Kind::Null,
        _ => return Err(Error::Schema),
    })
}
fn types(value: &Value) -> Result<(Kind, bool)> {
    if let Some(value) = value.as_str() {
        return Ok((kind(value)?, false));
    }
    let values = value.as_array().ok_or(Error::Schema)?;
    if values.len() != 2 {
        return Err(Error::Schema);
    }
    let first = values[0].as_str().ok_or(Error::Schema)?;
    let second = values[1].as_str().ok_or(Error::Schema)?;
    let primitive = match (first, second) {
        ("null", other) | (other, "null") => other,
        _ => return Err(Error::Schema),
    };
    let kind = kind(primitive)?;
    if !matches!(
        kind,
        Kind::String | Kind::Number | Kind::Integer | Kind::Boolean
    ) {
        return Err(Error::Schema);
    }
    Ok((kind, true))
}
fn count(value: &Value) -> Result<u64> {
    value.as_u64().ok_or(Error::Schema)
}
fn number(value: &Value, budget: &mut Budget) -> Result<Decimal> {
    if !value.is_number() {
        return Err(Error::Schema);
    }
    Decimal::parse(&value.to_string(), budget)
}
fn local_reference(value: &str) -> Result<String> {
    let name = value.strip_prefix("#/$defs/").ok_or(Error::Schema)?;
    // A $ref is a URI reference before it is a JSON Pointer. This profile does
    // not percent-decode URI fragments, so '%' must fail rather than resolving
    // a same-spelling literal definition to a different target. Admit only the
    // RFC3986 ASCII fragment characters within one pointer segment; '/' must be
    // encoded as ~1, and '~' is checked below as a pointer escape.
    if name.len() > 512
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@?".contains(&b))
    {
        return Err(Error::Schema);
    }
    let mut chars = name.chars();
    let mut decoded = String::new();
    while let Some(c) = chars.next() {
        decoded.push(if c == '~' {
            match chars.next() {
                Some('0') => '~',
                Some('1') => '/',
                _ => return Err(Error::Schema),
            }
        } else {
            c
        });
    }
    if decoded.len() > 256 {
        return Err(Error::Bound);
    }
    Ok(decoded)
}
fn children(node: &Node) -> Vec<usize> {
    let Node::Rule(rule) = node else {
        return vec![];
    };
    rule.properties
        .values()
        .copied()
        .chain(rule.additional)
        .chain(rule.items)
        .chain(rule.all.iter().copied())
        .chain(rule.any.iter().copied())
        .chain(rule.one.iter().copied())
        .chain(rule.not)
        .chain(rule.reference)
        .collect()
}
fn graph(
    id: usize,
    nodes: &[Node],
    colors: &mut [u8],
    heights: &mut [usize],
    depth: usize,
    budget: &mut Budget,
) -> Result<usize> {
    if depth > GRAPH_DEPTH {
        return Err(Error::Bound);
    }
    budget.charge(1)?;
    match colors.get(id).copied().ok_or(Error::Schema)? {
        1 => return Err(Error::Schema),
        2 => return Ok(heights[id]),
        _ => (),
    }
    colors[id] = 1;
    let mut height = 1;
    for child in children(nodes.get(id).ok_or(Error::Schema)?) {
        budget.charge(1)?;
        height = height.max(
            graph(child, nodes, colors, heights, depth + 1, budget)?
                .checked_add(1)
                .ok_or(Error::Bound)?,
        );
        if height > GRAPH_DEPTH {
            return Err(Error::Bound);
        }
    }
    colors[id] = 2;
    heights[id] = height;
    Ok(height)
}
/// Exact recursive equality shared by enum/const/uniqueness. No float conversion.
pub fn equal(left: &Value, right: &Value, budget: &mut Budget, depth: usize) -> Result<bool> {
    if depth > 32 {
        return Err(Error::Bound);
    }
    budget.charge(1)?;
    Ok(match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => {
            budget.charge(a.len().max(b.len()))?;
            a == b
        }
        (Value::Number(a), Value::Number(b)) => {
            Decimal::parse(&a.to_string(), budget)?
                .compare(&Decimal::parse(&b.to_string(), budget)?, budget)?
                == Ordering::Equal
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !equal(a, b, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (key, value) in a {
                budget.charge(1 + key.len())?;
                let Some(other) = b.get(key) else {
                    return Ok(false);
                };
                if !equal(value, other, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}

#[cfg(test)]
#[path = "compiled_tests.rs"]
mod tests;
