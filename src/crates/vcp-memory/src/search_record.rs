// SPDX-License-Identifier: Apache-2.0
//! Authorized canonical inventory and exact-byte chunk identities for indexing.
use crate::{
    access::{self, Access},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::{ArtifactDescriptor, Range},
    ids::*,
    memory::*,
    revision::*,
    task::Task,
    verification::Fingerprint,
    workspace::{Scope, Workspace},
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{contract::Collection, Store};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkerSpec {
    pub version: u32,
    pub max_bytes: usize,
    pub overlap_bytes: usize,
}
impl Default for ChunkerSpec {
    fn default() -> Self {
        Self {
            version: 1,
            max_bytes: 2048,
            overlap_bytes: 0,
        }
    }
}
impl ChunkerSpec {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1
            || !(64..=16384).contains(&self.max_bytes)
            || self.overlap_bytes > self.max_bytes / 4
        {
            return Err(Error::Invalid(
                "unsupported bounded chunker specification".into(),
            ));
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(digest_bytes(&canonical_bytes(&(
            "exact-utf8-newline-preferred/1",
            self,
        ))?))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub records: usize,
    pub source_bytes: usize,
    pub total_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 4096,
            source_bytes: 256 * 1024,
            total_bytes: 8 * 1024 * 1024,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<()> {
        if self.records == 0
            || self.records > 16384
            || self.source_bytes == 0
            || self.source_bytes > 1024 * 1024
            || self.total_bytes == 0
            || self.total_bytes > 32 * 1024 * 1024
        {
            return Err(Error::Invalid("search inventory limits".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Source,
    Claim,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextSource {
    Artifact {
        id: ArtifactId,
    },
    Claim {
        version: ClaimVersionId,
        claim: ClaimId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRecord {
    pub id: String,
    pub scope: Scope,
    pub root: RootId,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub kind: SearchKind,
    pub claim_kind: Option<ClaimKind>,
    pub source: TextSource,
    pub source_digest: String,
    pub span: Range,
    pub applicability: Option<Applicability>,
    pub status: Outcome,
    pub evidence_status: EvidenceStatus,
    pub memory_seq: MemorySeq,
    pub watermark: Watermark,
    /// Ephemeral authorized build input. Readers return candidate IDs and load
    /// retained canonical bytes again only after today's authorization checks.
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// Missing for denied scope: no hidden path, text or identity is disclosed.
    pub source: Option<TextSource>,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub workspace: WorkspaceId,
    pub watermark: Watermark,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub chunker_digest: String,
    pub records: Vec<SearchRecord>,
    pub exclusions: Vec<Exclusion>,
    pub digest: String,
}
impl SearchRecord {
    pub fn calculate_id(&self, chunker: &str) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(&(
            &self.scope,
            &self.root,
            &self.paths,
            &self.symbols,
            &self.source,
            &self.source_digest,
            &self.span,
            chunker,
        ))?))
    }
}
impl Inventory {
    pub fn calculate_digest(&self) -> Result<String> {
        let mut value = self.clone();
        value.digest.clear();
        Ok(digest_bytes(&canonical_bytes(&value)?))
    }
    /// Structural integrity only; canonical authorization belongs to inventory().
    pub fn validate(&self) -> Result<()> {
        if self.records.len() > 16384
            || self.exclusions.len() > 32768
            || self.digest != self.calculate_digest()?
        {
            return Err(Error::Invalid("search inventory integrity".into()));
        }
        let mut ids = BTreeSet::new();
        let mut bytes = 0usize;
        for record in &self.records {
            bytes = bytes.saturating_add(record.text.len());
            if record.scope.workspace != self.workspace
                || record.span.end.get().checked_sub(record.span.start.get())
                    != Some(record.text.len() as u64)
                || record.text.is_empty()
                || record.text.len() > 16384
                || record.id != record.calculate_id(&self.chunker_digest)?
                || !ids.insert(&record.id)
                || bytes > 32 * 1024 * 1024
            {
                return Err(Error::Invalid("search record integrity".into()));
            }
        }
        Ok(())
    }
}

/// A source must be named in an authorized retained native manifest, rather
/// than acquiring a path merely because a caller labels some captured bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub manifest: ArtifactId,
    pub artifact: ArtifactId,
    pub root: RootId,
    pub path: String,
    pub symbols: Vec<String>,
    pub fingerprint: Fingerprint,
}

/// Slices original UTF-8 without normalization. All offsets index retained bytes.
pub fn spans(text: &str, spec: &ChunkerSpec) -> Result<Vec<Range>> {
    spec.validate()?;
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + spec.max_bytes).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end < text.len() {
            if let Some(newline) = text[start..end].rfind('\n') {
                if newline + 1 >= spec.max_bytes / 2 {
                    end = start + newline + 1;
                }
            }
        }
        result.push(Range {
            start: ByteCount::new(start as u64),
            end: ByteCount::new(end as u64),
        });
        if end == text.len() {
            break;
        }
        let mut next = end.saturating_sub(spec.overlap_bytes);
        while !text.is_char_boundary(next) {
            next += 1;
        }
        start = next.max(start + 1);
        while !text.is_char_boundary(start) {
            start += 1;
        }
    }
    Ok(result)
}

fn read(
    store: &Store,
    access: &Access,
    id: &ArtifactId,
    max: usize,
) -> Result<Option<(ArtifactDescriptor, Vec<u8>)>> {
    let Some(row) = store
        .state()
        .records
        .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
    else {
        return Ok(None);
    };
    if row.workspace != access.workspace {
        return Err(Error::Access);
    }
    let artifact: ArtifactDescriptor = row.decode()?;
    if !access.allows_task(&artifact.spec.scope.task) {
        return Err(Error::Access);
    }
    if artifact.length.get() > max as u64 || !crate::proof::complete_capture(&artifact) {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(artifact.length.get() as usize);
    match vcp_audit::history::History::read_artifact(store, &access.history(), id, &mut bytes) {
        Ok(_) => Ok(Some((artifact, bytes))),
        Err(vcp_audit::Error::Access) => Err(Error::Access),
        Err(_) => Ok(None),
    }
}
fn path_valid(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && !path.split('/').any(|part| matches!(part, "" | "." | ".."))
}
fn source_current(
    store: &Store,
    workspace: &Workspace,
    source: &SourceBinding,
    descriptor: &ArtifactDescriptor,
    manifest: &ArtifactDescriptor,
    bytes: &[u8],
) -> Result<bool> {
    if descriptor.spec.scope != manifest.spec.scope
        || !path_valid(&source.path)
        || source.symbols.len() > 64
        || source
            .symbols
            .iter()
            .any(|s| s.is_empty() || s.len() > 1024)
    {
        return Ok(false);
    }
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            descriptor.spec.scope.task.as_str(),
            &workspace.id,
        )?
        .decode()?;
    if task.fingerprint != source.fingerprint {
        return Ok(false);
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(false);
    };
    let (snapshot, ids) = match manifest.spec.schema.as_str() {
        "verification-plan/1" => (value.get("before"), value.get("source_artifacts")),
        "verification-baseline/1" => (value.get("manifest"), value.get("sources")),
        "verification-result/1" if value["applicability"] == "current" => {
            (value.get("before"), value.get("source_artifacts"))
        }
        "vcp-memory-change/1" => (value.get("current"), None),
        _ => return Ok(false),
    };
    let Some(snapshot) = snapshot else {
        return Ok(false);
    };
    if snapshot["bounded_scan_complete"] != true
        || snapshot["identity"]["workspace"].as_str() != Some(workspace.id.as_str())
        || snapshot["identity"]["root"].as_str() != Some(source.root.as_str())
        || snapshot["identity"]["repository"].as_str()
            != Some(workspace.binding.repository.as_str())
        || snapshot["identity"]["worktree"].as_str() != Some(workspace.binding.worktree.as_str())
        || snapshot["identity"]["binding"] != serde_json::to_value(workspace.binding.revision)?
        || digest_bytes(&canonical_bytes(snapshot)?) != source.fingerprint.repository
    {
        return Ok(false);
    }
    let listed = if let Some(ids) = ids {
        ids.as_array().is_some_and(|ids| {
            ids.iter()
                .any(|id| id.as_str() == Some(source.artifact.as_str()))
        })
    } else {
        value["sources"].as_array().is_some_and(|sources| {
            sources.iter().any(|entry| {
                entry["artifact"].as_str() == Some(source.artifact.as_str())
                    && entry["version"]["root"].as_str() == Some(source.root.as_str())
                    && entry["version"]["path"] == source.path
                    && entry["version"]["sha256"] == descriptor.sha256
            })
        })
    };
    Ok(listed
        && snapshot["files"].as_array().is_some_and(|files| {
            files.iter().any(|file| {
                file["root"].as_str() == Some(source.root.as_str())
                    && file["path"] == source.path
                    && file["sha256"] == descriptor.sha256
                    && file["bytes"] == serde_json::json!(descriptor.length)
            })
        }))
}

fn chunk(
    mut record: SearchRecord,
    text: &str,
    spec: &ChunkerSpec,
    chunker: &str,
) -> Result<Vec<SearchRecord>> {
    let mut records = Vec::new();
    for span in spans(text, spec)? {
        record.span = span;
        record.text = text[record.span.start.get() as usize..record.span.end.get() as usize].into();
        record.id = record.calculate_id(chunker)?;
        records.push(record.clone());
    }
    Ok(records)
}
fn exclude(output: &mut Inventory, source: Option<TextSource>, reason: &str) {
    output.exclusions.push(Exclusion {
        source,
        reason: reason.into(),
    });
}
fn append(output: &mut Inventory, records: Vec<SearchRecord>, limits: Limits, total: &mut usize) {
    let bytes: usize = records.iter().map(|r| r.text.len()).sum();
    if output.records.len() + records.len() > limits.records
        || bytes > limits.total_bytes.saturating_sub(*total)
    {
        exclude(
            output,
            records.first().map(|r| r.source.clone()),
            "inventory_limit",
        );
    } else {
        *total += bytes;
        output.records.extend(records);
    }
}

/// Builds a fixed authorized snapshot inventory. Callers retain this inventory
/// identity while building private lexical/vector components; no active pointer
/// or index intent is changed here.
pub fn inventory(
    store: &Store,
    access: &Access,
    sources: &[SourceBinding],
    spec: &ChunkerSpec,
    limits: Limits,
) -> Result<Inventory> {
    limits.validate()?;
    spec.validate()?;
    if sources.len() > limits.records {
        return Err(Error::Invalid("source binding inventory limit".into()));
    }
    let workspace = access::authorize(store.state(), access, false)?;
    let mut output = Inventory {
        workspace: workspace.id.clone(),
        watermark: store.state().watermark,
        authority: workspace.authority,
        deletion: workspace.deletion,
        chunker_digest: spec.digest()?,
        records: vec![],
        exclusions: vec![],
        digest: String::new(),
    };
    let mut total = 0;
    for row in store.state().records.values().filter(|r| {
        r.workspace == workspace.id
            && r.collection == Collection::Claim
            && r.value["document_type"] == vcp_domain::redaction::VERSION
    }) {
        let version: vcp_domain::redaction::RedactedVersion = row.decode()?;
        if crate::access::redacted_scope(store.state(), access, &version.scope, &version.sources)
            .is_err()
        {
            exclude(&mut output, None, "denied");
            continue;
        }
        exclude(
            &mut output,
            Some(TextSource::Claim {
                version: version.id,
                claim: version.claim,
            }),
            "purged",
        );
    }
    let versions = crate::repository::versions(store.state(), &workspace.id)?;
    if versions.len() > 16384 {
        return Err(Error::Invalid("claim inventory scan limit".into()));
    }
    let mut histories = BTreeMap::new();
    for version in versions {
        let source = TextSource::Claim {
            version: version.id.clone(),
            claim: version.proposal.claim.clone(),
        };
        if !access.allows_task(&version.scope.task) {
            exclude(&mut output, None, "denied");
            continue;
        }
        let claim = version.proposal.claim.clone();
        if !histories.contains_key(&claim) {
            match crate::history::query(store, access, &claim, None, None) {
                Ok(history) => {
                    histories.insert(claim.clone(), Some(history));
                }
                Err(Error::Access) => {
                    histories.insert(claim.clone(), None);
                }
                Err(error) => return Err(error),
            }
        }
        let Some(history) = histories.get(&claim).and_then(Option::as_ref) else {
            exclude(&mut output, None, "denied");
            continue;
        };
        if version.proposal.applicability.repository != workspace.binding.repository
            || version.proposal.applicability.worktree != workspace.binding.worktree
            || version
                .proposal
                .applicability
                .roots
                .iter()
                .any(|root| root.as_str() != workspace.id.as_str())
        {
            exclude(&mut output, Some(source), "stale_binding");
            continue;
        }
        let Some(view) = history.versions.iter().find(|v| v.id == version.id) else {
            exclude(&mut output, Some(source), "unavailable_version");
            continue;
        };
        if view.version.is_none() {
            exclude(&mut output, Some(source), "pruned");
            continue;
        }
        if !view.current && version.resolution.outcome != Outcome::Disputed {
            exclude(&mut output, Some(source), "superseded");
            continue;
        }
        if !view.applicable {
            exclude(&mut output, Some(source), "stale_or_missing_evidence");
            continue;
        }
        let text = &version.proposal.statement;
        let roots = if version.proposal.applicability.roots.is_empty() {
            vec![RootId::parse(workspace.id.as_str())?]
        } else {
            version.proposal.applicability.roots.clone()
        };
        for root in roots {
            let record = SearchRecord {
                id: String::new(),
                scope: version.scope.clone(),
                root,
                paths: version.proposal.applicability.paths.clone(),
                symbols: version.proposal.applicability.symbols.clone(),
                kind: SearchKind::Claim,
                claim_kind: Some(version.proposal.value.kind()),
                source: source.clone(),
                source_digest: digest_bytes(text.as_bytes()),
                span: Range {
                    start: ByteCount::ZERO,
                    end: ByteCount::ZERO,
                },
                applicability: Some(version.proposal.applicability.clone()),
                status: version.resolution.outcome,
                evidence_status: version.resolution.evidence_status,
                memory_seq: version.memory_seq,
                watermark: version.canonical_watermark,
                text: String::new(),
            };
            let records = chunk(record, text, spec, &output.chunker_digest)?;
            append(&mut output, records, limits, &mut total);
        }
    }
    for binding in sources {
        let source = TextSource::Artifact {
            id: binding.artifact.clone(),
        };
        let read_result = (|| -> Result<Option<(ArtifactDescriptor, Vec<u8>)>> {
            let Some((descriptor, bytes)) =
                read(store, access, &binding.artifact, limits.source_bytes)?
            else {
                return Ok(None);
            };
            let Some((manifest, manifest_bytes)) =
                read(store, access, &binding.manifest, limits.source_bytes)?
            else {
                return Ok(None);
            };
            if !source_current(
                store,
                &workspace,
                binding,
                &descriptor,
                &manifest,
                &manifest_bytes,
            )? {
                return Ok(None);
            }
            Ok(Some((descriptor, bytes)))
        })();
        let (descriptor, bytes) = match read_result {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                exclude(
                    &mut output,
                    Some(source),
                    "unavailable_or_stale_source_binding",
                );
                continue;
            }
            Err(Error::Access) => {
                exclude(&mut output, None, "denied");
                continue;
            }
            Err(error) => return Err(error),
        };
        if !crate::retention::recall_allowed(
            store.state(),
            &access.workspace,
            &crate::retention::Target::Record(vcp_store::contract::key(
                Collection::Artifact,
                binding.artifact.as_str(),
            )),
        )? {
            exclude(&mut output, Some(source), "recall_excluded");
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            exclude(&mut output, Some(source), "unsupported_encoding");
            continue;
        };
        if text.contains('\0') {
            exclude(&mut output, Some(source), "binary_source");
            continue;
        }
        if text.is_empty() {
            exclude(&mut output, Some(source), "empty_source");
            continue;
        }
        if binding
            .symbols
            .iter()
            .any(|symbol| !observed_symbol(text, symbol))
        {
            exclude(&mut output, Some(source), "unobserved_symbol");
            continue;
        }
        let record = SearchRecord {
            id: String::new(),
            scope: descriptor.spec.scope,
            root: binding.root.clone(),
            paths: vec![binding.path.clone()],
            symbols: binding.symbols.clone(),
            kind: SearchKind::Source,
            claim_kind: None,
            source,
            source_digest: descriptor.sha256,
            span: Range {
                start: ByteCount::ZERO,
                end: ByteCount::ZERO,
            },
            applicability: Some(Applicability {
                repository: workspace.binding.repository.clone(),
                worktree: workspace.binding.worktree.clone(),
                roots: vec![binding.root.clone()],
                paths: vec![binding.path.clone()],
                symbols: binding.symbols.clone(),
                branch: None,
                fingerprint: Some(binding.fingerprint.clone()),
                conditions: BTreeMap::new(),
                valid_from: None,
                valid_until: None,
            }),
            status: Outcome::Accepted,
            evidence_status: EvidenceStatus::Observed,
            memory_seq: MemorySeq::ZERO,
            watermark: store.state().watermark,
            text: String::new(),
        };
        let records = chunk(record, text, spec, &output.chunker_digest)?;
        append(&mut output, records, limits, &mut total);
    }
    output.records.sort_by(|a, b| a.id.cmp(&b.id));
    output.records.dedup_by(|a, b| a.id == b.id);
    output.digest = output.calculate_digest()?;
    Ok(output)
}

fn observed_symbol(text: &str, symbol: &str) -> bool {
    if symbol.is_empty()
        || !symbol
            .chars()
            .all(|c| c.is_alphanumeric() || "_:.".contains(c))
    {
        return false;
    }
    let identifier = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(symbol).any(|(start, value)| {
        !text[..start].chars().next_back().is_some_and(identifier)
            && !text[start + value.len()..]
                .chars()
                .next()
                .is_some_and(identifier)
    })
}
