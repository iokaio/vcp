// SPDX-License-Identifier: Apache-2.0
//! Immutable private DiskANN component. Generation publication belongs to P5-05.
use crate::{
    embedding::{self, Encoded, Specification, MAX_CHUNKS},
    Error, Result,
};
use munarium_datastore::{
    vector::VectorIndex,
    vector_diskann::{DiskAnnVectorIndex, GraphParams},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};
use vcp_domain::WorkspaceId;
use vcp_protocol::{canonical_bytes, digest_bytes};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_RESULTS: usize = 64;
pub const MAX_OVERFETCH: usize = 256;
pub const MAX_EXACT: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Readiness {
    Pending {
        reason: String,
    },
    Failed {
        reason: String,
    },
    Ready {
        specification: String,
        checksum: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    workspace: WorkspaceId,
    specification: Specification,
    engine: String,
    graph_parameters: String,
    rows: Vec<Encoded>,
    graph: Vec<u8>,
}
pub struct Component {
    document: Document,
    index: DiskAnnVectorIndex,
}

#[derive(Debug, PartialEq)]
pub struct Candidate {
    pub chunk: String,
    pub source: String,
    pub distance: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Ann,
    ExactAuthorizedSubset,
    ReducedRecall,
}
pub struct Candidates {
    pub mode: Mode,
    pub rows: Vec<Candidate>,
}
fn graph_parameters() -> Result<String> {
    String::from_utf8(canonical_bytes(&GraphParams::default().to_plan_map())?)
        .map_err(|_| Error::Conflict("graph specification encoding"))
}
fn native(error: impl std::fmt::Display) -> Error {
    Error::Invalid(error.to_string())
}
fn redirected(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT includes junctions, not just symlinks.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
fn private_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or(Error::Conflict("vector private parent required"))?;
    let metadata = std::fs::symlink_metadata(parent).map_err(native)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(Error::Conflict("vector private parent redirected"));
    }
    Ok(())
}
fn regular_file(metadata: &std::fs::Metadata) -> Result<()> {
    if !metadata.is_file() || redirected(metadata) {
        return Err(Error::Conflict("vector component is not a regular file"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Conflict("vector file limit"));
    }
    Ok(())
}

/// Preflight the pinned provider-v1 encoding before its reader can allocate
/// from header lengths. Validate its complete ordinal ID/vector mapping against
/// retained rebuild inputs; matching counts alone cannot establish that link.
fn validate_graph(bytes: &[u8], rows: &[Encoded], spec: &Specification) -> Result<()> {
    struct Cursor<'a> {
        bytes: &'a [u8],
        offset: usize,
    }
    impl<'a> Cursor<'a> {
        fn take(&mut self, count: usize) -> Result<&'a [u8]> {
            let end = self
                .offset
                .checked_add(count)
                .ok_or(Error::Conflict("vector graph length overflow"))?;
            let value = self
                .bytes
                .get(self.offset..end)
                .ok_or(Error::Conflict("truncated vector graph"))?;
            self.offset = end;
            Ok(value)
        }
        fn u32(&mut self) -> Result<u32> {
            Ok(u32::from_le_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| Error::Conflict("vector graph integer"))?,
            ))
        }
        fn u64(&mut self) -> Result<u64> {
            Ok(u64::from_le_bytes(
                self.take(8)?
                    .try_into()
                    .map_err(|_| Error::Conflict("vector graph integer"))?,
            ))
        }
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Conflict("vector graph byte limit"));
    }
    let mut cursor = Cursor { bytes, offset: 0 };
    let params = GraphParams::default();
    if cursor.u32()? != 1
        || cursor.u64()? != spec.dimensions as u64
        || cursor.u64()? != rows.len() as u64
        || cursor.u32()? != params.max_degree
        || cursor.u32()? != params.l_build
        || cursor.u32()? != params.alpha.to_bits()
        || cursor.u32()? != params.l_search
    {
        return Err(Error::Conflict("vector graph header compatibility"));
    }
    for row in rows {
        let length = cursor.u32()? as usize;
        if length != row.identity.id.len() || cursor.take(length)? != row.identity.id.as_bytes() {
            return Err(Error::Conflict("vector graph stable identity mapping"));
        }
    }
    for row in rows {
        for value in &row.vector {
            if cursor.u32()? != value.to_bits() {
                return Err(Error::Conflict("vector graph retained vector mismatch"));
            }
        }
    }
    // The qualified provider uses the f64-accumulated, f32-stored centroid.
    for dimension in 0..spec.dimensions {
        let centroid = (rows
            .iter()
            .map(|row| f64::from(row.vector[dimension]))
            .sum::<f64>()
            / rows.len() as f64) as f32;
        if cursor.u32()? != centroid.to_bits() {
            return Err(Error::Conflict("vector graph centroid mismatch"));
        }
    }
    for _ in 0..=rows.len() {
        let count = cursor.u32()? as usize;
        if count > rows.len() + 1 {
            return Err(Error::Conflict("vector graph adjacency limit"));
        }
        let mut seen = BTreeSet::new();
        for _ in 0..count {
            let neighbor = cursor.u32()? as usize;
            if neighbor > rows.len() || !seen.insert(neighbor) {
                return Err(Error::Conflict("vector graph invalid neighbor"));
            }
        }
    }
    if cursor.offset != bytes.len() {
        return Err(Error::Conflict("vector graph trailing data"));
    }
    Ok(())
}

impl Component {
    pub fn build(
        workspace: WorkspaceId,
        specification: Specification,
        rows: Vec<Encoded>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        embedding::check(cancelled)?;
        specification.validate()?;
        Self::validate_rows(&workspace, &specification, &rows)?;
        let entries = rows
            .iter()
            .map(|row| (row.identity.id.clone(), row.vector.clone()))
            .collect::<Vec<_>>();
        // Qualified provider inserts synchronously. Bound its entire input, then
        // observe cancellation before any component bytes can be retained.
        let index =
            DiskAnnVectorIndex::build(specification.dimensions, &entries, GraphParams::default())
                .map_err(native)?;
        embedding::check(cancelled)?;
        let graph = index.to_bytes().map_err(native)?;
        Ok(Self {
            document: Document {
                version: 1,
                workspace,
                specification,
                engine: "diskann/0.56.0/full-f32/cosine/provider-v1".into(),
                graph_parameters: graph_parameters()?,
                rows,
                graph,
            },
            index,
        })
    }
    fn validate_rows(
        workspace: &WorkspaceId,
        spec: &Specification,
        rows: &[Encoded],
    ) -> Result<()> {
        if rows.is_empty() || rows.len() > MAX_CHUNKS {
            return Err(Error::Conflict("vector component size"));
        }
        let mut ids = BTreeSet::new();
        let digest = spec.digest()?;
        for row in rows {
            row.identity.validate(&digest)?;
            embedding::validate_vector(&row.vector)?;
            if row.identity.scope.workspace != *workspace || !ids.insert(&row.identity.id) {
                return Err(Error::Conflict(
                    "vector component scope or duplicate identity",
                ));
            }
        }
        Ok(())
    }
    pub fn rows(&self) -> &[Encoded] {
        &self.document.rows
    }
    /// Creates a new private file exclusively, flushes it and returns its digest.
    /// A failed/cancelled file remains private and is never a ready component.
    pub fn save_private(&self, path: &Path, cancelled: &dyn Fn() -> bool) -> Result<String> {
        self.save_private_bounded(path, MAX_FILE_BYTES, cancelled)
    }
    /// Host passes the remaining admitted temporary-disk budget after sampling
    /// its owned directory. Refuse oversized output before creating any file.
    pub fn save_private_bounded(
        &self,
        path: &Path,
        available_bytes: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String> {
        embedding::check(cancelled)?;
        // Ancestors and concurrent directory replacement remain the trusted
        // caller's ownership boundary; never follow a redirected immediate root.
        private_parent(path)?;
        let bytes = self.bounded_bytes(available_bytes, cancelled)?;
        embedding::check(cancelled)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(native)?;
        file.write_all(&bytes).map_err(native)?;
        file.sync_all().map_err(native)?;
        embedding::check(cancelled)?;
        Ok(digest_bytes(&bytes))
    }
    /// Write into a private, caller-owned handle. The caller retains ownership
    /// and synchronizes the handle before publishing the returned digest.
    pub fn write_bounded(
        &self,
        sink: &mut impl Write,
        available_bytes: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String> {
        let bytes = self.bounded_bytes(available_bytes, cancelled)?;
        sink.write_all(&bytes).map_err(native)?;
        embedding::check(cancelled)?;
        Ok(digest_bytes(&bytes))
    }
    fn bounded_bytes(&self, available_bytes: u64, cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>> {
        embedding::check(cancelled)?;
        let bytes = canonical_bytes(&self.document)?;
        if bytes.len() as u64 > MAX_FILE_BYTES.min(available_bytes) {
            return Err(Error::Conflict("vector file limit"));
        }
        embedding::check(cancelled)?;
        Ok(bytes)
    }
    /// The digest must come from the trusted generation descriptor, not this file.
    pub fn open(
        path: &Path,
        checksum: &str,
        workspace: &WorkspaceId,
        expected: &Specification,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        embedding::check(cancelled)?;
        expected.validate()?;
        private_parent(path)?;
        regular_file(&std::fs::symlink_metadata(path).map_err(native)?)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_FLAG_OPEN_REPARSE_POINT: inspect the opened file itself.
            options.custom_flags(0x0020_0000);
        }
        let file = options.open(path).map_err(native)?;
        regular_file(&file.metadata().map_err(native)?)?;
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(native)?;
        if bytes.len() as u64 > MAX_FILE_BYTES || digest_bytes(&bytes) != checksum {
            return Err(Error::Conflict("vector file integrity"));
        }
        let document: Document = serde_json::from_slice(&bytes)?;
        if document.version != 1
            || document.workspace != *workspace
            || document.specification != *expected
            || document.engine != "diskann/0.56.0/full-f32/cosine/provider-v1"
            || document.graph_parameters != graph_parameters()?
        {
            return Err(Error::Conflict("vector component compatibility"));
        }
        Self::validate_rows(workspace, expected, &document.rows)?;
        validate_graph(&document.graph, &document.rows, expected)?;
        embedding::check(cancelled)?;
        let index = DiskAnnVectorIndex::from_bytes(&document.graph).map_err(native)?;
        if index.dimensions() != expected.dimensions
            || index.len() != document.rows.len()
            || index.params().to_plan_map() != GraphParams::default().to_plan_map()
        {
            return Err(Error::Conflict("vector graph compatibility"));
        }
        embedding::check(cancelled)?;
        Ok(Self { document, index })
    }
    /// `authorized` is a fresh canonical inventory's chunk-ID set, supplied by
    /// the authenticated host, never model arguments or persisted index metadata.
    /// No snippets, foreign IDs or source content are returned or diagnosed.
    pub fn query(
        &self,
        workspace: &WorkspaceId,
        authorized: &BTreeSet<String>,
        query: &[f32],
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Candidates> {
        embedding::check(cancelled)?;
        if workspace != &self.document.workspace {
            return Err(Error::Access);
        }
        if limit == 0 || limit > MAX_RESULTS || authorized.len() > MAX_CHUNKS {
            return Err(Error::Conflict("vector query limit"));
        }
        embedding::validate_vector(query)?;
        let count = self
            .document
            .rows
            .iter()
            .filter(|row| authorized.contains(&row.identity.id))
            .count();
        let candidates = self
            .index
            .vector_candidates(query, MAX_OVERFETCH.min(self.index.len()))
            .map_err(native)?;
        let mut rows = Vec::new();
        for candidate in candidates {
            if !authorized.contains(&candidate.chunk_id) {
                continue;
            }
            let row = self
                .document
                .rows
                .iter()
                .find(|row| row.identity.id == candidate.chunk_id)
                .ok_or(Error::Conflict("vector graph identity mismatch"))?;
            rows.push(Candidate {
                chunk: row.identity.id.clone(),
                source: row.identity.source.clone(),
                distance: candidate.score,
            });
        }
        let mode = if rows.len() < limit.min(count) && count <= MAX_EXACT {
            rows = self
                .document
                .rows
                .iter()
                .filter(|row| authorized.contains(&row.identity.id))
                .map(|row| {
                    let dot = query
                        .iter()
                        .zip(&row.vector)
                        .map(|(a, b)| f64::from(*a) * f64::from(*b))
                        .sum::<f64>();
                    let left = query
                        .iter()
                        .map(|v| f64::from(*v).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    let right = row
                        .vector
                        .iter()
                        .map(|v| f64::from(*v).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    Candidate {
                        chunk: row.identity.id.clone(),
                        source: row.identity.source.clone(),
                        distance: (1. - dot / (left * right)) as f32,
                    }
                })
                .collect();
            Mode::ExactAuthorizedSubset
        } else if rows.len() < limit.min(count) {
            Mode::ReducedRecall
        } else {
            Mode::Ann
        };
        rows.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.chunk.cmp(&b.chunk))
        });
        rows.truncate(limit);
        embedding::check(cancelled)?;
        Ok(Candidates { mode, rows })
    }
}
