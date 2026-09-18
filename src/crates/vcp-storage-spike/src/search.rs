// SPDX-License-Identifier: Apache-2.0
//! Rebuild disposable binary caches from restored canonical vectors/text.
use crate::{digest, Result, View};
use munarium_datastore::{
    lexical::{self, LexicalIndex, LexicalPlan, PlanTerm, TantivyLexicalIndex},
    records::ChunkRecord,
    vector::VectorIndex,
    vector_diskann::{DiskAnnVectorIndex, GraphParams},
};
use std::{collections::BTreeMap, fs, path::Path, time::Instant};

pub fn rebuild(view: &View, directory: &Path) -> Result<serde_json::Value> {
    view.validate()?;
    fs::create_dir(directory)?;
    let started = Instant::now();
    let entries: Vec<_> = view
        .vectors
        .iter()
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect();
    let ann = DiskAnnVectorIndex::build(3, &entries, GraphParams::default())?;
    let binary = ann.to_bytes()?;
    fs::write(directory.join("diskann.bin"), &binary)?;
    let mut rows = Vec::new();
    for (n, record) in view
        .records
        .iter()
        .filter(|r| view.vectors.contains_key(&r.id))
        .enumerate()
    {
        let text = record.payload["text"]
            .as_str()
            .ok_or_else(|| crate::reject("missing canonical search text"))?
            .to_owned();
        rows.push(ChunkRecord {
            chunk_id: record.id.clone(),
            source_id: record.id.clone(),
            source_path: format!("{}.txt", record.id),
            node_id: None,
            ordinal: n as u32,
            text_sha256: digest(text.as_bytes()),
            text,
            metadata: BTreeMap::new(),
        });
    }
    let lexical = lexical::build(&rows)?;
    fs::write(directory.join("tantivy.bin"), &lexical)?;
    let reopened = DiskAnnVectorIndex::from_bytes(&fs::read(directory.join("diskann.bin"))?)?;
    let lexical = TantivyLexicalIndex::open(&fs::read(directory.join("tantivy.bin"))?)?;
    let semantic = reopened.vector_candidates(&[0.0, 1.0, 0.0], 2)?;
    let plan = LexicalPlan {
        terms: vec![PlanTerm::user("new")],
        minimum_should_match: 1,
        ..Default::default()
    };
    let hits = lexical.lexical_candidates(&plan, 2)?;
    if semantic.first().map(|c| c.chunk_id.as_str()) != Some("claim-new")
        || !hits.iter().any(|c| c.chunk_id == "claim-new")
    {
        return Err(crate::reject("restored search oracle mismatch"));
    }
    let result = serde_json::json!({"status":"pass","canonical_sequence":view.sequence,"searchable_generation":view.generation,
        "retained_vectors":entries.len(),"dimensions":3,"model":"synthetic-public-unit-vectors-v1","diskann":"0.56.0",
        "tantivy":lexical::engine_revision(),"time_to_search_us":started.elapsed().as_micros(),"network_inference":false});
    fs::write(
        directory.join("generation.json"),
        serde_json::to_vec(&result)?,
    )?;
    Ok(result)
}
pub fn reject_incompatible_cache(bytes: &[u8]) -> Result<()> {
    if DiskAnnVectorIndex::from_bytes(bytes).is_ok() {
        return Err(crate::reject("incompatible cache was accepted"));
    }
    Ok(())
}
