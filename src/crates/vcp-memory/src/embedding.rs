// SPDX-License-Identifier: Apache-2.0
//! File-only CPU vectors. The host supplies authorized retained bytes and owns admission.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use vcp_domain::workspace::Scope;
use vcp_embedding::{MiniLm, DIMENSIONS, MAX_BATCH, MAX_TOKENS};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub const MAX_CHUNKS: usize = 1024;
pub const CHUNK_BYTES: usize = 192;
pub const MAX_SOURCE_BYTES: usize = 192 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    pub version: u32,
    pub assets: String,
    pub runtime: String,
    pub tokenizer: String,
    pub preprocessing: String,
    pub chunker: String,
    pub max_tokens: usize,
    pub truncation: String,
    pub dimensions: usize,
    pub pooling: String,
    pub normalization: String,
    pub metric: String,
}
impl Specification {
    pub fn qualified() -> Self {
        Self {
            version: 1,
            assets: MiniLm::specification_sha256(),
            runtime: "candle/0.11.0/cpu-f32".into(),
            tokenizer: "tokenizers/0.22.2/pinned-json".into(),
            preprocessing: "identity-utf8/1".into(),
            chunker: "utf8-bytes/1/max192/no-overlap".into(),
            max_tokens: MAX_TOKENS,
            truncation: "reject".into(),
            dimensions: DIMENSIONS,
            pooling: "attention-mask-mean".into(),
            normalization: "l2".into(),
            metric: "cosine".into(),
        }
    }
    pub fn digest(&self) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(self)?))
    }
    pub fn validate(&self) -> Result<()> {
        if self != &Self::qualified() {
            return Err(Error::Conflict("unsupported embedding specification"));
        }
        Ok(())
    }
}

/// Offsets are into the exact retained UTF-8 text passed to `chunks`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub source: String,
    pub scope: Scope,
    pub source_digest: String,
    pub start: usize,
    pub end: usize,
    pub content_digest: String,
    pub specification: String,
}
impl Identity {
    pub fn cache_key(&self) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(self)?))
    }
    pub(crate) fn validate(&self, specification: &str) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 512
            || self.source.is_empty()
            || self.source.len() > 512
            || self.start >= self.end
            || self.end > 1024 * 1024
            || self.end - self.start > CHUNK_BYTES
            || self.specification != specification
            || !vcp_domain::accounting::valid_hash(&self.source_digest)
            || !vcp_domain::accounting::valid_hash(&self.content_digest)
        {
            return Err(Error::Conflict("invalid embedding chunk identity"));
        }
        let expected = digest_bytes(&canonical_bytes(&(
            "embedding-chunk/1",
            &self.source,
            &self.scope,
            &self.source_digest,
            self.start,
            self.end,
            &self.specification,
        ))?);
        if self.id != expected {
            return Err(Error::Conflict("embedding chunk identity mismatch"));
        }
        Ok(())
    }
}
pub struct Chunk {
    pub identity: Identity,
    pub text: String,
}

/// Convert a freshly authorized search inventory. A vector source is the
/// canonical SearchRecord ID; subspans still address the original retained text.
pub fn inventory_chunks(inventory: &crate::search_record::Inventory) -> Result<Vec<Chunk>> {
    inventory.validate()?;
    let spec = Specification::qualified();
    let mut output = Vec::new();
    for record in &inventory.records {
        let mut rows = chunks(&record.id, &record.scope, &record.text, &spec)?;
        for row in &mut rows {
            let offset = usize::try_from(record.span.start.get())
                .map_err(|_| Error::Conflict("embedding source offset"))?;
            row.identity.start = row
                .identity
                .start
                .checked_add(offset)
                .ok_or(Error::Conflict("embedding source offset"))?;
            row.identity.end = row
                .identity
                .end
                .checked_add(offset)
                .ok_or(Error::Conflict("embedding source offset"))?;
            row.identity.source_digest = record.source_digest.clone();
            row.identity.id = digest_bytes(&canonical_bytes(&(
                "embedding-chunk/1",
                &row.identity.source,
                &row.identity.scope,
                &row.identity.source_digest,
                row.identity.start,
                row.identity.end,
                &row.identity.specification,
            ))?);
            row.identity.validate(&row.identity.specification)?;
        }
        if output.len() + rows.len() > MAX_CHUNKS {
            return Err(Error::Conflict("embedding inventory chunk limit"));
        }
        output.extend(rows);
    }
    Ok(output)
}

pub fn check(cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() {
        Err(Error::Conflict("local embedding cancelled"))
    } else {
        Ok(())
    }
}

/// The caller first resolves canonical authorization and retention. Whitespace
/// chunks are explicitly omitted; all other spans retain exact source offsets.
pub fn chunks(
    source: &str,
    scope: &Scope,
    text: &str,
    specification: &Specification,
) -> Result<Vec<Chunk>> {
    specification.validate()?;
    if source.is_empty() || source.len() > 512 || text.len() > MAX_SOURCE_BYTES {
        return Err(Error::Conflict("embedding source limit"));
    }
    let specification = specification.digest()?;
    let source_digest = digest_bytes(text.as_bytes());
    let mut rows = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let content = &text[start..end];
        if !content.trim().is_empty() {
            let id = digest_bytes(&canonical_bytes(&(
                "embedding-chunk/1",
                source,
                scope,
                &source_digest,
                start,
                end,
                &specification,
            ))?);
            rows.push(Chunk {
                identity: Identity {
                    id,
                    source: source.into(),
                    scope: scope.clone(),
                    source_digest: source_digest.clone(),
                    start,
                    end,
                    content_digest: digest_bytes(content.as_bytes()),
                    specification: specification.clone(),
                },
                text: content.into(),
            });
        }
        start = end;
    }
    if rows.len() > MAX_CHUNKS {
        return Err(Error::Conflict("embedding chunk limit"));
    }
    Ok(rows)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Encoded {
    pub identity: Identity,
    pub vector: Vec<f32>,
}
pub(crate) fn validate_vector(vector: &[f32]) -> Result<()> {
    let norm = vector.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    if vector.len() != DIMENSIONS || !norm.is_finite() || (norm - 1.).abs() > 0.001 {
        return Err(Error::Conflict("invalid normalized embedding vector"));
    }
    Ok(())
}

pub struct LocalEmbedding {
    model: MiniLm,
    specification: Specification,
    cache: BTreeMap<String, Encoded>,
}
impl LocalEmbedding {
    pub fn load(root: &Path, cancelled: &dyn Fn() -> bool) -> Result<Self> {
        check(cancelled)?;
        let model = MiniLm::load(root).map_err(|e| Error::Invalid(e.to_string()))?;
        check(cancelled)?;
        Ok(Self {
            model,
            specification: Specification::qualified(),
            cache: BTreeMap::new(),
        })
    }
    pub fn specification(&self) -> &Specification {
        &self.specification
    }
    /// Bounded cache restore; caller must still reauthorize input chunks on use.
    pub fn restore(&mut self, rows: &[Encoded], cancelled: &dyn Fn() -> bool) -> Result<()> {
        check(cancelled)?;
        if rows.len() > MAX_CHUNKS {
            return Err(Error::Conflict("embedding cache limit"));
        }
        let spec = self.specification.digest()?;
        let mut cache = BTreeMap::new();
        for row in rows {
            row.identity.validate(&spec)?;
            validate_vector(&row.vector)?;
            if cache
                .insert(row.identity.cache_key()?, row.clone())
                .is_some()
            {
                return Err(Error::Conflict("duplicate embedding cache identity"));
            }
        }
        check(cancelled)?;
        self.cache = cache;
        Ok(())
    }
    pub fn embed(
        &mut self,
        inputs: &[Chunk],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Encoded>> {
        check(cancelled)?;
        if inputs.len() > MAX_CHUNKS {
            return Err(Error::Conflict("embedding batch limit"));
        }
        let spec = self.specification.digest()?;
        let mut seen = std::collections::BTreeSet::new();
        for row in inputs {
            row.identity.validate(&spec)?;
            if !seen.insert(row.identity.id.clone())
                || row.text.trim().is_empty()
                || row.text.len() != row.identity.end - row.identity.start
                || digest_bytes(row.text.as_bytes()) != row.identity.content_digest
            {
                return Err(Error::Conflict("embedding input identity mismatch"));
            }
        }
        let mut output = Vec::with_capacity(inputs.len());
        for batch in inputs.chunks(MAX_BATCH) {
            check(cancelled)?;
            if self.cache.len() + batch.len() > MAX_CHUNKS {
                self.cache.clear();
            }
            let missing: Vec<_> = batch
                .iter()
                .filter(|row| {
                    row.identity
                        .cache_key()
                        .map_or(true, |key| !self.cache.contains_key(&key))
                })
                .collect();
            let texts: Vec<_> = missing.iter().map(|row| row.text.as_str()).collect();
            if !texts.is_empty() {
                let encoded = self
                    .model
                    .embed(&texts)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                check(cancelled)?;
                for (input, encoded) in missing.into_iter().zip(encoded) {
                    if encoded.truncated {
                        return Err(Error::Conflict("embedding input truncated"));
                    }
                    validate_vector(&encoded.vector)?;
                    self.cache.insert(
                        input.identity.cache_key()?,
                        Encoded {
                            identity: input.identity.clone(),
                            vector: encoded.vector,
                        },
                    );
                }
            }
            for input in batch {
                output.push(
                    self.cache
                        .get(&input.identity.cache_key()?)
                        .ok_or(Error::Conflict("embedding cache batch missing"))?
                        .clone(),
                );
            }
        }
        check(cancelled)?;
        Ok(output)
    }
    pub fn query(&self, text: &str, cancelled: &dyn Fn() -> bool) -> Result<Vec<f32>> {
        check(cancelled)?;
        if text.len() > CHUNK_BYTES {
            return Err(Error::Conflict("embedding query limit"));
        }
        let mut rows = self
            .model
            .embed(&[text])
            .map_err(|e| Error::Invalid(e.to_string()))?;
        check(cancelled)?;
        let row = rows
            .pop()
            .ok_or(Error::Conflict("embedding query missing"))?;
        if row.truncated {
            return Err(Error::Conflict("embedding query truncated"));
        }
        validate_vector(&row.vector)?;
        Ok(row.vector)
    }
}
