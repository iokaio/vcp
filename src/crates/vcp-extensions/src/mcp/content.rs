// SPDX-License-Identifier: Apache-2.0
//! Bounded 2025-11-25 external content. No URI dereference, template expansion,
//! subscription, prompt execution, or role promotion. The enclosing wire parser
//! retains its integer-only profile. Binary/multimodal content is explicitly
//! omitted from usable content; no image/audio/base64 execution is implemented.
use super::{
    client::Error,
    identity::{ConnectionIdentity, ContentIdentity, ContentKind},
    registration::Registration,
    schema::{self, CheckedArguments, Schema},
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// Conservative absolute ASCII URI syntax profile: exact bytes, valid scheme,
/// component characters and percent triplets; no resolution or normalization.
/// Authority userinfo and IPvFuture literals are unsupported. An authority is
/// only syntax here, never permission to connect; paths never become OS paths.
pub fn valid_uri(uri: &str) -> bool {
    if uri.len() > 4096 || !uri.is_ascii() {
        return false;
    }
    let Some((scheme, rest)) = uri.split_once(':') else {
        return false;
    };
    if scheme.is_empty()
        || rest.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
    {
        return false;
    }
    let (before_fragment, fragment) = rest
        .split_once('#')
        .map_or((rest, None), |(head, tail)| (head, Some(tail)));
    let (hierarchy, query) = before_fragment
        .split_once('?')
        .map_or((before_fragment, None), |(head, tail)| (head, Some(tail)));
    if query.is_some_and(|v| !component(v, b"/?")) || fragment.is_some_and(|v| !component(v, b"/?"))
    {
        return false;
    }
    let path = if let Some(after_slashes) = hierarchy.strip_prefix("//") {
        let (authority, path) = after_slashes
            .find('/')
            .map_or((after_slashes, ""), |index| {
                (&after_slashes[..index], &after_slashes[index..])
            });
        if authority.contains('@') {
            return false;
        }
        if let Some(ipv6) = authority.strip_prefix('[') {
            let Some((address, suffix)) = ipv6.split_once(']') else {
                return false;
            };
            if address.parse::<std::net::Ipv6Addr>().is_err()
                || (!suffix.is_empty() && !suffix.strip_prefix(':').is_some_and(valid_port))
            {
                return false;
            }
        } else {
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, None), |(host, port)| (host, Some(port)));
            if !component(host, b"")
                || host.contains(':')
                || port.is_some_and(|port| !valid_port(port))
            {
                return false;
            }
        }
        path
    } else {
        hierarchy
    };
    component(path, b"/")
}
fn valid_port(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) && value.parse::<u16>().is_ok()
}
fn component(value: &str, extra: &[u8]) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return false;
            }
            i += 3;
        } else {
            if !bytes[i].is_ascii_alphanumeric()
                && !b"-._~:@!$&'()*+,;=".contains(&bytes[i])
                && !extra.contains(&bytes[i])
            {
                return false;
            }
            i += 1;
        }
    }
    true
}

#[derive(Clone, Serialize)]
pub struct DiscoveredResource {
    pub identity: ContentIdentity,
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub omitted_metadata: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct PromptArgument {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub required: bool,
}
#[derive(Clone, Serialize)]
pub struct DiscoveredPrompt {
    pub identity: ContentIdentity,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<PromptArgument>,
    pub input_schema: Value,
    pub omitted_metadata: Vec<String>,
    #[serde(skip)]
    schema: Schema,
}
impl DiscoveredPrompt {
    pub fn check_arguments(&self, bytes: &[u8]) -> Result<CheckedArguments, schema::Error> {
        self.schema.arguments(bytes)
    }
    pub fn schema_digest(&self) -> &str {
        self.schema.digest()
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentRejection {
    NotAllowed,
    Metadata,
    Arguments,
}
#[derive(Clone, Serialize)]
pub struct RejectedContent {
    pub key: String,
    pub reason: ContentRejection,
}
#[derive(Clone, Serialize)]
pub struct OmittedContent {
    pub kind: String,
}
#[derive(Clone, Serialize)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub omitted_content: Option<OmittedContent>,
}
#[derive(Clone, Serialize)]
pub struct ResourceResult {
    pub contents: Vec<ResourceContent>,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptRole {
    User,
    Assistant,
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptContent {
    Text { text: String },
    Resource { resource: ResourceContent },
    Omitted { kind: String },
}
#[derive(Clone, Serialize)]
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: PromptContent,
}
#[derive(Clone, Serialize)]
pub struct PromptResult {
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

// Debug omits all external text, URIs, names and argument values.
impl fmt::Debug for DiscoveredResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveredResource")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for DiscoveredPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveredPrompt")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for RejectedContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RejectedContent")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for ResourceResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceResult")
            .field("contents", &self.contents.len())
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for PromptResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromptResult")
            .field("messages", &self.messages.len())
            .finish_non_exhaustive()
    }
}

pub(crate) fn string(value: Option<&Value>, max: usize) -> Result<&str, Error> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .ok_or(Error::Protocol)
}
fn optional(object: &Map<String, Value>, key: &str, max: usize) -> Result<Option<String>, Error> {
    object
        .get(key)
        .map(|v| {
            v.as_str()
                .filter(|s| s.len() <= max)
                .map(str::to_owned)
                .ok_or(Error::Protocol)
        })
        .transpose()
}
fn omitted_metadata(object: &Map<String, Value>, known: &[&str]) -> Vec<String> {
    // Record only fixed known omissions; unknown extension names never become
    // executable semantics or unbounded rejection/display text.
    ["annotations", "icons", "_meta"]
        .iter()
        .filter(|key| !known.contains(key) && object.contains_key(**key))
        .map(|key| (*key).to_owned())
        .collect()
}
fn hash(value: &Value) -> Result<String, Error> {
    vcp_protocol::canonical_bytes(value)
        .map(|bytes| vcp_protocol::digest_bytes(&bytes))
        .map_err(|_| Error::Protocol)
}
fn resource(
    value: &Value,
    connection: ConnectionIdentity,
    revision: u64,
) -> Result<DiscoveredResource, Error> {
    let object = value.as_object().ok_or(Error::Protocol)?;
    let uri = string(object.get("uri"), 4096)?.to_owned();
    if !valid_uri(&uri) {
        return Err(Error::Protocol);
    }
    // Profile 2 sizes are exact mathematical nonnegative integers bounded by u64.
    if object.get("size").is_some_and(|v| v.as_u64().is_none()) {
        return Err(Error::Protocol);
    }
    Ok(DiscoveredResource {
        identity: ContentIdentity::new(
            connection,
            ContentKind::Resource,
            uri.clone(),
            hash(value)?,
            revision,
        )
        .map_err(|_| Error::Protocol)?,
        uri,
        name: string(object.get("name"), 256)?.to_owned(),
        title: optional(object, "title", 256)?,
        description: optional(object, "description", 4096)?,
        mime_type: optional(object, "mimeType", 256)?,
        omitted_metadata: omitted_metadata(object, &[]),
    })
}
fn prompt(
    value: &Value,
    connection: ConnectionIdentity,
    revision: u64,
) -> Result<DiscoveredPrompt, Error> {
    let object = value.as_object().ok_or(Error::Protocol)?;
    let name = string(object.get("name"), 256)?.to_owned();
    let mut arguments = Vec::new();
    let mut properties = Map::new();
    let mut required = Vec::new();
    if let Some(values) = object.get("arguments") {
        let values = values.as_array().ok_or(Error::Protocol)?;
        if values.len() > 64 {
            return Err(Error::Bounds);
        }
        for value in values {
            let arg = value.as_object().ok_or(Error::Protocol)?;
            if arg
                .keys()
                .any(|key| !["name", "title", "description", "required"].contains(&key.as_str()))
            {
                return Err(Error::Arguments);
            }
            let name = string(arg.get("name"), 128)?.to_owned();
            if properties
                .insert(name.clone(), json!({"type":"string","maxLength":65536}))
                .is_some()
            {
                return Err(Error::Arguments);
            }
            let is_required = arg
                .get("required")
                .map(|v| v.as_bool().ok_or(Error::Arguments))
                .transpose()?
                .unwrap_or(false);
            if is_required {
                required.push(name.clone());
            }
            arguments.push(PromptArgument {
                name,
                title: optional(arg, "title", 256)?,
                description: optional(arg, "description", 4096)?,
                required: is_required,
            });
        }
    }
    let input_schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let schema = Schema::compile(
        &vcp_protocol::canonical_bytes(&input_schema).map_err(|_| Error::Arguments)?,
        schema::Limits::default(),
    )
    .map_err(|_| Error::Arguments)?;
    Ok(DiscoveredPrompt {
        identity: ContentIdentity::new(
            connection,
            ContentKind::Prompt,
            name.clone(),
            hash(value)?,
            revision,
        )
        .map_err(|_| Error::Protocol)?,
        name,
        title: optional(object, "title", 256)?,
        description: optional(object, "description", 4096)?,
        arguments,
        input_schema,
        schema,
        omitted_metadata: omitted_metadata(object, &[]),
    })
}

#[derive(Clone)]
pub(crate) enum Descriptor {
    Resource(DiscoveredResource),
    Prompt(DiscoveredPrompt),
}
pub(crate) struct Catalog {
    kind: ContentKind,
    pub(crate) entries: BTreeMap<String, Descriptor>,
    revision: u64,
    pub(crate) listing: bool,
    building: BTreeMap<String, Descriptor>,
    rejected: Vec<RejectedContent>,
    names: BTreeSet<String>,
    cursors: BTreeSet<String>,
    cursor: Option<String>,
    pages: u64,
    bytes: u64,
}
pub(crate) struct Page {
    pub(crate) complete: bool,
    pub(crate) rejected: Vec<RejectedContent>,
}
impl Catalog {
    pub(crate) fn new(kind: ContentKind) -> Self {
        Self {
            kind,
            entries: BTreeMap::new(),
            revision: 0,
            listing: false,
            building: BTreeMap::new(),
            rejected: vec![],
            names: BTreeSet::new(),
            cursors: BTreeSet::new(),
            cursor: None,
            pages: 0,
            bytes: 0,
        }
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.building.clear();
        self.rejected.clear();
        self.names.clear();
        self.cursors.clear();
        self.cursor = None;
        self.listing = false;
    }
    pub(crate) fn request(&mut self, max_pages: u64) -> Result<Value, Error> {
        if !self.listing {
            self.clear();
            self.revision = self.revision.checked_add(1).ok_or(Error::Bounds)?;
            self.listing = true;
            self.pages = 0;
            self.bytes = 0;
        }
        if self.pages >= max_pages {
            return Err(Error::Bounds);
        }
        Ok(self
            .cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({"cursor":cursor})))
    }
    pub(crate) fn page(
        &mut self,
        result: &Map<String, Value>,
        connection: &ConnectionIdentity,
        registration: &Registration,
        bytes: usize,
    ) -> Result<Page, Error> {
        self.pages = self.pages.checked_add(1).ok_or(Error::Bounds)?;
        self.bytes = self.bytes.checked_add(bytes as u64).ok_or(Error::Bounds)?;
        if !self.listing
            || self.pages > registration.limits.pages
            || self.bytes > registration.limits.total_discovery_bytes
        {
            return Err(Error::Bounds);
        }
        let (field, key, allowed) = match self.kind {
            ContentKind::Resource => ("resources", "uri", &registration.allowed_resources),
            ContentKind::Prompt => ("prompts", "name", &registration.allowed_prompts),
        };
        let values = result
            .get(field)
            .and_then(Value::as_array)
            .ok_or(Error::Protocol)?;
        for value in values {
            let object = value.as_object().ok_or(Error::Protocol)?;
            let key = string(
                object.get(key),
                if self.kind == ContentKind::Resource {
                    4096
                } else {
                    256
                },
            )?
            .to_owned();
            if self.kind == ContentKind::Resource && !valid_uri(&key) {
                return Err(Error::Protocol);
            }
            if !self.names.insert(key.clone()) {
                return Err(Error::Protocol);
            }
            if self.names.len() as u64 > registration.limits.tools {
                return Err(Error::Bounds);
            }
            if !allowed.contains(&key) {
                self.rejected.push(RejectedContent {
                    key,
                    reason: ContentRejection::NotAllowed,
                });
                continue;
            }
            let parsed = match self.kind {
                ContentKind::Resource => {
                    resource(value, connection.clone(), self.revision).map(Descriptor::Resource)
                }
                ContentKind::Prompt => {
                    prompt(value, connection.clone(), self.revision).map(Descriptor::Prompt)
                }
            };
            match parsed {
                Ok(value) => {
                    self.building.insert(key, value);
                }
                Err(Error::Bounds) => return Err(Error::Bounds),
                Err(error) => self.rejected.push(RejectedContent {
                    key,
                    reason: if error == Error::Arguments {
                        ContentRejection::Arguments
                    } else {
                        ContentRejection::Metadata
                    },
                }),
            }
        }
        self.cursor = result
            .get("nextCursor")
            .map(|v| string(Some(v), 1024).map(str::to_owned))
            .transpose()?;
        if let Some(cursor) = &self.cursor {
            if !self.cursors.insert(cursor.clone()) || self.pages >= registration.limits.pages {
                return Err(Error::Bounds);
            }
            return Ok(Page {
                complete: false,
                rejected: vec![],
            });
        }
        self.listing = false;
        self.entries = std::mem::take(&mut self.building);
        Ok(Page {
            complete: true,
            rejected: std::mem::take(&mut self.rejected),
        })
    }
}
fn resource_content(value: &Value) -> Result<ResourceContent, Error> {
    let object = value.as_object().ok_or(Error::Protocol)?;
    let uri = string(object.get("uri"), 4096)?.to_owned();
    if !valid_uri(&uri) || object.contains_key("text") == object.contains_key("blob") {
        return Err(Error::Protocol);
    }
    let (text, omitted_content) = if let Some(text) = object.get("text") {
        (Some(text.as_str().ok_or(Error::Protocol)?.to_owned()), None)
    } else {
        object
            .get("blob")
            .and_then(Value::as_str)
            .ok_or(Error::Protocol)?;
        (
            None,
            Some(OmittedContent {
                kind: "binary_resource".into(),
            }),
        )
    };
    Ok(ResourceContent {
        uri,
        mime_type: optional(object, "mimeType", 256)?,
        text,
        omitted_content,
    })
}
pub(crate) fn resource_result(result: &Map<String, Value>) -> Result<ResourceResult, Error> {
    let values = result
        .get("contents")
        .and_then(Value::as_array)
        .ok_or(Error::Protocol)?;
    if values.len() > 64 {
        return Err(Error::Bounds);
    }
    Ok(ResourceResult {
        contents: values
            .iter()
            .map(resource_content)
            .collect::<Result<_, _>>()?,
    })
}
pub(crate) fn prompt_result(result: &Map<String, Value>) -> Result<PromptResult, Error> {
    let values = result
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(Error::Protocol)?;
    if values.len() > 64 {
        return Err(Error::Bounds);
    }
    let mut messages = Vec::new();
    for value in values {
        let message = value.as_object().ok_or(Error::Protocol)?;
        let role = match message.get("role").and_then(Value::as_str) {
            Some("user") => PromptRole::User,
            Some("assistant") => PromptRole::Assistant,
            _ => return Err(Error::Protocol),
        };
        let block = message
            .get("content")
            .and_then(Value::as_object)
            .ok_or(Error::Protocol)?;
        let content = match block.get("type").and_then(Value::as_str) {
            Some("text") => PromptContent::Text {
                text: block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or(Error::Protocol)?
                    .to_owned(),
            },
            Some("resource") => PromptContent::Resource {
                resource: resource_content(block.get("resource").ok_or(Error::Protocol)?)?,
            },
            Some(kind @ ("image" | "audio" | "resource_link")) => {
                PromptContent::Omitted { kind: kind.into() }
            }
            _ => return Err(Error::Protocol),
        };
        messages.push(PromptMessage { role, content });
    }
    Ok(PromptResult {
        description: optional(result, "description", 4096)?,
        messages,
    })
}
