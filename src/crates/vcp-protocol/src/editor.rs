// SPDX-License-Identifier: Apache-2.0
//! Versioned editor wire boundary. Text supplied here is transient unless explicitly captured.
use crate::methods::{Counter, Id, Mutation, Scope};
use serde::{Deserialize, Serialize};
macro_rules! dto { ($name:ident { $($(#[$a:meta])* $field:ident: $ty:ty),* $(,)? }) => {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema", derive(schemars::JsonSchema))]
    pub struct $name { $($(#[$a])* pub $field: $ty),* }
}; }
pub const CAPABILITY: &str = "editor/prepared-edits/1";
dto!(Position {
    line: u32,
    character: u32
});
dto!(Range {
    start: Position,
    end: Position
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EndOfLine {
    Lf,
    Crlf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Encoding {
    Unknown,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}
dto!(Diagnostics {
    #[cfg_attr(feature="schema", schemars(length(min=1,max=128)))] collector: String,
    observed_at: Counter, observed_document_version: Counter, producer_document_version: Option<Counter>,
    #[cfg_attr(feature="schema", schemars(range(max=64)))] count: u32,
    truncated: bool,
    #[cfg_attr(feature="schema", schemars(regex(pattern="^[0-9a-f]{64}(?![\\s\\S])")))] sha256: String
});
dto!(DocumentObservation {
    host: Id, open_id: Id,
    #[cfg_attr(feature="schema", schemars(length(min=1,max=32768)))] uri: String,
    #[cfg_attr(feature="schema", schemars(length(min=1,max=32768)))] relative_path: String,
    version: Counter,
    #[cfg_attr(feature="schema", schemars(regex(pattern="^[0-9a-f]{64}(?![\\s\\S])")))] content_sha256: String,
    dirty: bool,
    #[cfg_attr(feature="schema", schemars(length(max=65536)))] content: Option<String>,
    #[cfg_attr(feature="schema", schemars(regex(pattern="^[0-9a-f]{64}(?![\\s\\S])")))] disk_sha256: Option<String>,
    #[cfg_attr(feature="schema", schemars(length(max=128)))] language: String,
    eol: EndOfLine, encoding: Encoding,
    #[cfg_attr(feature="schema", schemars(length(max=32)))] selections: Vec<Range>,
    capture: bool,
    #[serde(default)] diagnostics: Option<Diagnostics>
});
dto!(EditorContext { scope: Scope, mutation: Mutation, task: Id, #[cfg_attr(feature="schema", schemars(length(max=16)))] documents: Vec<DocumentObservation>, #[serde(default)] #[cfg_attr(feature="schema", schemars(length(max=16)))] closed: Vec<Id> });
dto!(Observation { id: Id, document: DocumentObservation, root: Id, disk_fingerprint: String, artifact: Option<Id> });
dto!(ContextView { generation: Id, revision: Counter, #[cfg_attr(feature="schema", schemars(length(max=16)))] observations: Vec<Observation> });
dto!(TextEdit {
    range: Range,
    #[cfg_attr(feature = "schema", schemars(length(max = 65536)))]
    text: String
});
dto!(FileEdits { observation: Id, #[cfg_attr(feature="schema", schemars(length(min=1,max=128)))] edits: Vec<TextEdit> });
dto!(EditorPrepare { scope: Scope, mutation: Mutation, task: Id, generation: Id, #[cfg_attr(feature="schema", schemars(length(min=1,max=16)))] files: Vec<FileEdits> });
dto!(EditorChangeRead {
    scope: Scope,
    task: Id,
    change: Id
});
dto!(EditorDispatch {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    change: Id,
    generation: Id,
    #[cfg_attr(feature = "schema", schemars(range(max = 15)))]
    file: u32
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EditorOutcome {
    Applied,
    Rejected,
    Unknown,
}
dto!(EditorChangeResult {
    scope: Scope,
    mutation: Mutation,
    task: Id,
    change: Id,
    generation: Id,
    #[cfg_attr(feature = "schema", schemars(range(max = 15)))]
    file: u32,
    execution: Id,
    outcome: EditorOutcome,
    document: DocumentObservation
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FileState {
    Prepared,
    Dispatched,
    Applied,
    Rejected,
    Unknown,
}
dto!(FileView { file: u32, observation: Id, effect: Id, uri: String, relative_path: String, host: Id, open_id: Id, version: Counter, content_sha256: String, disk_sha256: String, disk_fingerprint: String, after_sha256: String, edits_digest: String, operation_digest: String, state: FileState, execution: Option<Id>, observed_observation: Option<Id>, observed_version: Option<Counter>, observed_sha256: Option<String>, dirty: bool });
dto!(ChangeView { scope: Scope, task: Id, change: Id, revision: Counter, generation: Id, root: Id, binding_revision: Counter, authority_revision: Counter, policy_revision: Counter, steering_revision: Counter, #[cfg_attr(feature="schema", schemars(length(max=16)))] files: Vec<FileView>, buffers_unverified: bool });
dto!(DispatchView {
    change: ChangeView,
    file: u32,
    execution: Id,
    apply: bool
});
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn range(value: &Range) -> bool {
    (value.start.line, value.start.character) <= (value.end.line, value.end.character)
}
pub fn validate_document(value: &DocumentObservation) -> Result<(), &'static str> {
    if value.uri.is_empty()
        || value.uri.len() > 32768
        || value.uri.contains('\0')
        || value.relative_path.is_empty()
        || value.relative_path.len() > 32768
        || value.relative_path.contains('\0')
        || !hash(&value.content_sha256)
        || value.disk_sha256.as_ref().is_some_and(|v| !hash(v))
        || value.language.len() > 128
        || value.selections.len() > 32
        || value.selections.iter().any(|v| !range(v))
        || value.diagnostics.as_ref().is_some_and(|v| {
            v.collector.is_empty()
                || v.collector.len() > 128
                || v.collector.contains('\0')
                || v.count > 64
                || !hash(&v.sha256)
        })
    {
        return Err("invalid editor observation");
    }
    if let Some(content) = &value.content {
        if content.len() > 65536 || crate::digest_bytes(content.as_bytes()) != value.content_sha256
        {
            return Err("editor content mismatch");
        }
    }
    Ok(())
}
pub fn validate_context(value: &EditorContext) -> Result<(), &'static str> {
    if value.documents.len() + value.closed.len() == 0
        || value.documents.len() + value.closed.len() > 16
    {
        return Err("editor document limit");
    }
    let mut closed = std::collections::BTreeSet::new();
    if value.closed.iter().any(|id| !closed.insert(id.as_str())) {
        return Err("duplicate closed observation");
    }
    for document in &value.documents {
        validate_document(document)?;
    }
    Ok(())
}
pub fn validate_prepare(value: &EditorPrepare) -> Result<(), &'static str> {
    if value.files.is_empty() || value.files.len() > 16 {
        return Err("editor file limit");
    }
    let mut ids = std::collections::BTreeSet::new();
    for file in &value.files {
        if !ids.insert(file.observation.as_str()) || file.edits.is_empty() || file.edits.len() > 128
        {
            return Err("editor edit limit or duplicate document");
        }
        for edit in &file.edits {
            if !range(&edit.range) || edit.text.len() > 65536 {
                return Err("invalid editor edit");
            }
        }
    }
    Ok(())
}
