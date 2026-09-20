// SPDX-License-Identifier: Apache-2.0
//! VCP's immutable lexical component. Public Tantivy schema/query/writer patterns
//! are adapted from Apache-2.0 Munarium's munarium-datastore/src/lexical.rs.
//! Unlike that adapter, VCP retains exact code scope, has no stop words/stemming,
//! and returns only canonical IDs. Activation and ANN are separate contracts.
use crate::{
    search_record::{Inventory, SearchKind, SearchRecord, TextSource},
    tokenizer, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tantivy::{
    collector::TopDocs,
    query::{
        AllQuery, BooleanQuery, BoostQuery, Occur, PhraseQuery, Query as TantivyQuery, TermQuery,
    },
    schema::{
        Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, INDEXED, STORED,
        STRING,
    },
    Index, IndexReader, ReloadPolicy, TantivyDocument, Term,
};
use vcp_domain::{
    memory::{ClaimKind, Outcome},
    RootId, TaskId, WorkspaceId,
};

pub const SCHEMA_VERSION: u32 = 1;
pub const EXACT_BOOST: f32 = 8.0;
pub const CODE_BOOST: f32 = 2.0;
const MANIFEST: &str = "vcp-lexical.json";

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub documents: usize,
    pub source_bytes: usize,
    pub disk_bytes: u64,
    pub batch_documents: usize,
    pub writer_bytes: usize,
    pub results: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            documents: 10_000,
            source_bytes: 16 * 1024 * 1024,
            disk_bytes: 128 * 1024 * 1024,
            batch_documents: 128,
            writer_bytes: 16 * 1024 * 1024,
            results: 100,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<()> {
        if self.documents == 0
            || self.documents > 100_000
            || self.source_bytes == 0
            || self.source_bytes > 64 * 1024 * 1024
            || self.disk_bytes == 0
            || self.disk_bytes > 512 * 1024 * 1024
            || self.batch_documents == 0
            || self.batch_documents > 1024
            || !(15_000_000..=64 * 1024 * 1024).contains(&self.writer_bytes)
            || self.results == 0
            || self.results > 1000
        {
            return Err(invalid("lexical limits"));
        }
        Ok(())
    }
}

/// All filters are intersected before ranking/truncation. None means unrestricted
/// within the inventory workspace; Some(empty) grants no matching documents.
#[derive(Clone, Debug)]
pub struct Query {
    pub workspace: WorkspaceId,
    pub tasks: Option<Vec<TaskId>>,
    pub roots: Option<Vec<RootId>>,
    pub paths: Option<Vec<String>>,
    pub symbols: Option<Vec<String>>,
    pub kind: Option<SearchKind>,
    pub claim_kind: Option<ClaimKind>,
    pub status: Option<Outcome>,
    pub text: String,
    pub phrase: bool,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Candidate {
    pub id: String,
    pub rank: usize,
    pub score: f32,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Component {
    name: String,
    bytes: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    tokenizer: String,
    engine: String,
    inventory: String,
    count: usize,
    components: Vec<Component>,
}
#[derive(Clone, Copy)]
struct Fields {
    id: Field,
    workspace: Field,
    task: Field,
    root: Field,
    path: Field,
    symbol: Field,
    kind: Field,
    claim_kind: Field,
    status: Field,
    source_version: Field,
    source_digest: Field,
    memory_seq: Field,
    watermark: Field,
    span_start: Field,
    span_end: Field,
    prose: Field,
    code: Field,
}
fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let id = builder.add_text_field("id", STRING | STORED);
    let workspace = builder.add_text_field("workspace", STRING);
    let task = builder.add_text_field("task", STRING);
    let root = builder.add_text_field("root", STRING);
    let path = builder.add_text_field("path", STRING);
    let symbol = builder.add_text_field("symbol", STRING);
    let kind = builder.add_text_field("kind", STRING);
    let claim_kind = builder.add_text_field("claim_kind", STRING);
    let status = builder.add_text_field("status", STRING);
    let source_version = builder.add_text_field("source_version", STRING);
    let source_digest = builder.add_text_field("source_digest", STRING);
    let memory_seq = builder.add_u64_field("memory_seq", INDEXED);
    let watermark = builder.add_u64_field("watermark", INDEXED);
    let span_start = builder.add_u64_field("span_start", INDEXED);
    let span_end = builder.add_u64_field("span_end", INDEXED);
    let options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("whitespace")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let prose = builder.add_text_field("prose", options.clone());
    let code = builder.add_text_field("code", options);
    (
        builder.build(),
        Fields {
            id,
            workspace,
            task,
            root,
            path,
            symbol,
            kind,
            claim_kind,
            status,
            source_version,
            source_digest,
            memory_seq,
            watermark,
            span_start,
            span_end,
            prose,
            code,
        },
    )
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn component_error<T>(_: T) -> Error {
    invalid("lexical component unavailable or malformed")
}
fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn label<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("lexical enum label"))
}
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(invalid("lexical build cancelled"))
    } else {
        Ok(())
    }
}

fn expected(inventory: &Inventory, limits: Limits) -> Result<BTreeSet<String>> {
    limits.validate()?;
    if inventory.records.len() > limits.documents {
        return Err(invalid("lexical document limit"));
    }
    let mut ids = BTreeSet::new();
    let mut bytes = 0usize;
    for record in &inventory.records {
        let record_bytes = vcp_protocol::canonical_bytes(record)?.len();
        if record_bytes > 256 * 1024 || record.paths.len() > 256 || record.symbols.len() > 256 {
            return Err(invalid("lexical per-document limit"));
        }
        bytes = bytes
            .checked_add(record_bytes)
            .ok_or_else(|| invalid("lexical byte overflow"))?;
        if record.scope.workspace != inventory.workspace
            || record.id.is_empty()
            || record.id.len() > 256
            || !ids.insert(record.id.clone())
            || bytes > limits.source_bytes
        {
            return Err(invalid("lexical inventory scope, identity or byte limit"));
        }
    }
    inventory.validate()?;
    Ok(ids)
}
fn document(record: &SearchRecord, f: Fields) -> Result<TantivyDocument> {
    let mut doc = TantivyDocument::new();
    for (field, value) in [
        (f.id, record.id.as_str()),
        (f.workspace, record.scope.workspace.as_str()),
        (f.task, record.scope.task.as_str()),
        (f.root, record.root.as_str()),
    ] {
        doc.add_text(field, value);
    }
    for path in &record.paths {
        doc.add_text(f.path, path);
    }
    for symbol in &record.symbols {
        doc.add_text(f.symbol, symbol);
    }
    doc.add_text(f.kind, label(&record.kind)?);
    if let Some(kind) = &record.claim_kind {
        doc.add_text(f.claim_kind, label(kind)?);
    }
    doc.add_text(f.status, label(&record.status)?);
    doc.add_text(
        f.source_version,
        match &record.source {
            TextSource::Artifact { id } => id.as_str(),
            TextSource::Claim { version, .. } => version.as_str(),
        },
    );
    doc.add_text(f.source_digest, &record.source_digest);
    doc.add_u64(f.memory_seq, record.memory_seq.get());
    doc.add_u64(f.watermark, record.watermark.get());
    doc.add_u64(f.span_start, record.span.start.get());
    doc.add_u64(f.span_end, record.span.end.get());
    doc.add_text(f.prose, tokenizer::prose_terms(&record.text).join(" "));
    let code = std::iter::once(record.text.as_str())
        .chain(record.paths.iter().map(String::as_str))
        .chain(record.symbols.iter().map(String::as_str))
        .flat_map(tokenizer::code_terms)
        .collect::<Vec<_>>()
        .join(" ");
    doc.add_text(f.code, code);
    Ok(doc)
}

fn components(directory: &Path, limit: u64) -> Result<Vec<Component>> {
    let mut out = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(directory).map_err(component_error)? {
        let entry = entry.map_err(component_error)?;
        let name = entry.file_name().into_string().map_err(component_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(component_error)?;
        if !metadata.is_file() || redirected(&metadata) {
            return Err(invalid("lexical component must contain only regular files"));
        }
        if name == MANIFEST || name.ends_with(".lock") {
            continue;
        }
        total = total
            .checked_add(metadata.len())
            .ok_or_else(|| invalid("lexical disk overflow"))?;
        if total > limit || out.len() >= 4096 {
            return Err(invalid("lexical disk limit"));
        }
        let mut bytes = Vec::new();
        fs::File::open(entry.path())
            .map_err(component_error)?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(component_error)?;
        if bytes.len() as u64 != metadata.len() {
            return Err(invalid("lexical component changed during validation"));
        }
        out.push(Component {
            name,
            bytes: metadata.len(),
            sha256: vcp_protocol::digest_bytes(&bytes),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn check_disk(directory: &Path, limit: u64) -> Result<()> {
    let mut bytes = 0u64;
    for (count, entry) in fs::read_dir(directory)
        .map_err(component_error)?
        .enumerate()
    {
        let metadata = fs::symlink_metadata(entry.map_err(component_error)?.path())
            .map_err(component_error)?;
        if !metadata.is_file() || redirected(&metadata) || count >= 4096 {
            return Err(invalid("lexical component file limit"));
        }
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or_else(|| invalid("lexical disk overflow"))?;
        if bytes > limit {
            return Err(invalid("lexical disk limit"));
        }
    }
    Ok(())
}

/// Builds only into an existing empty caller-owned private directory. A failed
/// or cancelled build has no readiness manifest and must never be activated.
pub fn build(
    directory: &Path,
    inventory: &Inventory,
    limits: Limits,
    cancel: &AtomicBool,
) -> Result<Reader> {
    let ids = expected(inventory, limits)?;
    cancelled(cancel)?;
    let metadata = fs::symlink_metadata(directory).map_err(component_error)?;
    if !metadata.is_dir()
        || redirected(&metadata)
        || fs::read_dir(directory)
            .map_err(component_error)?
            .next()
            .is_some()
    {
        return Err(invalid("lexical build requires an empty private directory"));
    }
    let (schema, fields) = schema();
    let index = Index::create_in_dir(directory, schema).map_err(component_error)?;
    let mut writer = index
        .writer_with_num_threads(1, limits.writer_bytes)
        .map_err(component_error)?;
    writer.set_merge_policy(Box::new(tantivy::merge_policy::NoMergePolicy));
    for batch in inventory.records.chunks(limits.batch_documents) {
        cancelled(cancel)?;
        for record in batch {
            writer
                .add_document(document(record, fields)?)
                .map_err(component_error)?;
        }
        writer.commit().map_err(component_error)?;
        check_disk(directory, limits.disk_bytes)?;
    }
    writer.commit().map_err(component_error)?;
    cancelled(cancel)?;
    let segments = index.searchable_segment_ids().map_err(component_error)?;
    if segments.len() > 1 {
        writer.merge(&segments).wait().map_err(component_error)?;
    }
    writer.wait_merging_threads().map_err(component_error)?;
    let manifest = Manifest {
        schema: SCHEMA_VERSION,
        tokenizer: tokenizer::TOKENIZER_VERSION.into(),
        engine: tantivy::version_string().into(),
        inventory: inventory.digest.clone(),
        count: ids.len(),
        components: components(directory, limits.disk_bytes)?,
    };
    // Validate all IDs before making this component available to open().
    let reader = Reader::checked(index, fields, inventory, ids, limits)?;
    cancelled(cancel)?;
    let bytes = vcp_protocol::canonical_bytes(&manifest)?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("lexical manifest limit"));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(MANIFEST))
        .map_err(component_error)?;
    output.write_all(&bytes).map_err(component_error)?;
    output.sync_all().map_err(component_error)?;
    drop(output);
    drop(reader);
    open(directory, inventory, limits)
}

/// A pinned reader owns its component; callers retain its directory until all
/// readers are dropped. No automatic reload can mix separate generations.
pub struct Reader {
    reader: IndexReader,
    fields: Fields,
    workspace: WorkspaceId,
    ids: BTreeSet<String>,
    limits: Limits,
}
pub fn open(directory: &Path, inventory: &Inventory, limits: Limits) -> Result<Reader> {
    let ids = expected(inventory, limits)?;
    let metadata = fs::symlink_metadata(directory).map_err(component_error)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(invalid("lexical directory is not private storage"));
    }
    let path = directory.join(MANIFEST);
    let meta = fs::symlink_metadata(&path).map_err(component_error)?;
    if !meta.is_file() || redirected(&meta) || meta.len() > 1024 * 1024 {
        return Err(invalid("lexical manifest file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(component_error)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(component_error)?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("lexical manifest size"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(component_error)?;
    if manifest.schema != SCHEMA_VERSION
        || manifest.tokenizer != tokenizer::TOKENIZER_VERSION
        || manifest.engine != tantivy::version_string()
        || manifest.inventory != inventory.digest
        || manifest.count != ids.len()
        || manifest.components != components(directory, limits.disk_bytes)?
    {
        return Err(invalid("incompatible or corrupt lexical component"));
    }
    let index = Index::open_in_dir(directory).map_err(component_error)?;
    let (schema, fields) = schema();
    if index.schema() != schema
        || !index
            .validate_checksum()
            .map_err(component_error)?
            .is_empty()
    {
        return Err(invalid("lexical schema or checksum mismatch"));
    }
    Reader::checked(index, fields, inventory, ids, limits)
}
impl Reader {
    fn checked(
        index: Index,
        fields: Fields,
        inventory: &Inventory,
        ids: BTreeSet<String>,
        limits: Limits,
    ) -> Result<Self> {
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(component_error)?;
        let result = Self {
            reader,
            fields,
            workspace: inventory.workspace.clone(),
            ids,
            limits,
        };
        let searcher = result.reader.searcher();
        if searcher.num_docs() != result.ids.len() as u64 {
            return Err(invalid("lexical inventory count mismatch"));
        }
        let mut found = BTreeSet::new();
        for (_, address) in searcher
            .search(&AllQuery, &TopDocs::with_limit(result.ids.len().max(1)))
            .map_err(component_error)?
        {
            let doc: TantivyDocument = searcher.doc(address).map_err(component_error)?;
            let id = doc
                .get_first(fields.id)
                .and_then(|value| value.as_str())
                .ok_or_else(|| invalid("lexical stable id missing"))?;
            if !found.insert(id.to_owned()) {
                return Err(invalid("duplicate lexical stable id"));
            }
        }
        if found != result.ids {
            return Err(invalid("lexical inventory IDs mismatch"));
        }
        Ok(result)
    }
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
    pub fn search(&self, query: &Query) -> Result<Vec<Candidate>> {
        if query.workspace != self.workspace {
            return Err(Error::Access);
        }
        if query.limit == 0 || query.limit > self.limits.results || query.text.len() > 4096 {
            return Err(invalid("lexical query bounds"));
        }
        let f = self.fields;
        let mut clauses: Vec<(Occur, Box<dyn TantivyQuery>)> =
            vec![(Occur::Must, term(f.workspace, query.workspace.as_str()))];
        filter(
            &mut clauses,
            f.task,
            query
                .tasks
                .as_ref()
                .map(|v| v.iter().map(|id| id.as_str()).collect()),
        )?;
        filter(
            &mut clauses,
            f.root,
            query
                .roots
                .as_ref()
                .map(|v| v.iter().map(|id| id.as_str()).collect()),
        )?;
        filter(
            &mut clauses,
            f.path,
            query
                .paths
                .as_ref()
                .map(|v| v.iter().map(String::as_str).collect()),
        )?;
        filter(
            &mut clauses,
            f.symbol,
            query
                .symbols
                .as_ref()
                .map(|v| v.iter().map(String::as_str).collect()),
        )?;
        if let Some(kind) = &query.kind {
            clauses.push((Occur::Must, term(f.kind, &label(kind)?)));
        }
        if let Some(kind) = &query.claim_kind {
            clauses.push((Occur::Must, term(f.claim_kind, &label(kind)?)));
        }
        if let Some(status) = &query.status {
            clauses.push((Occur::Must, term(f.status, &label(status)?)));
        }
        if !query.text.trim().is_empty() {
            let prose = tokenizer::prose_terms(&query.text);
            let code = tokenizer::code_terms(&query.text);
            if prose.len() + code.len() > 256 {
                return Err(invalid("lexical term limit"));
            }
            let mut alternatives: Vec<(Occur, Box<dyn TantivyQuery>)> = Vec::new();
            if query.phrase && prose.len() > 1 {
                alternatives.push((
                    Occur::Should,
                    Box::new(PhraseQuery::new(
                        prose
                            .iter()
                            .map(|text| Term::from_field_text(f.prose, text))
                            .collect(),
                    )),
                ));
            } else {
                for text in prose {
                    alternatives.push((Occur::Should, term(f.prose, &text)));
                }
                for text in code {
                    alternatives.push((
                        Occur::Should,
                        Box::new(BoostQuery::new(term(f.code, &text), CODE_BOOST)),
                    ));
                }
            }
            for field in [f.path, f.symbol] {
                alternatives.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(term(field, &query.text), EXACT_BOOST)),
                ));
            }
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(alternatives))));
        }
        let searcher = self.reader.searcher();
        let found = searcher
            .search(
                &BooleanQuery::new(clauses),
                &TopDocs::with_limit(query.limit),
            )
            .map_err(component_error)?;
        let mut out = Vec::new();
        for (score, address) in found {
            let doc: TantivyDocument = searcher.doc(address).map_err(component_error)?;
            let id = doc
                .get_first(f.id)
                .and_then(|value| value.as_str())
                .ok_or_else(|| invalid("lexical candidate ID missing"))?;
            if !self.ids.contains(id) || !score.is_finite() {
                return Err(invalid("lexical candidate not in inventory"));
            }
            out.push(Candidate {
                id: id.into(),
                rank: out.len() + 1,
                score,
            });
        }
        Ok(out)
    }
}
fn term(field: Field, text: &str) -> Box<dyn TantivyQuery> {
    Box::new(TermQuery::new(
        Term::from_field_text(field, text),
        IndexRecordOption::Basic,
    ))
}
fn filter(
    clauses: &mut Vec<(Occur, Box<dyn TantivyQuery>)>,
    field: Field,
    values: Option<Vec<&str>>,
) -> Result<()> {
    if let Some(values) = values {
        if values.len() > 256
            || values
                .iter()
                .any(|value| value.is_empty() || value.len() > 4096)
        {
            return Err(invalid("lexical filter bounds"));
        }
        clauses.push((
            Occur::Must,
            Box::new(BooleanQuery::new(
                values
                    .into_iter()
                    .map(|value| (Occur::Should, term(field, value)))
                    .collect(),
            )),
        ));
    }
    Ok(())
}
