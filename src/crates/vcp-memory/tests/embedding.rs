// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeSet;
use vcp_domain::{workspace::Scope, SessionId, TaskId, WorkspaceId};
use vcp_memory::{
    embedding::{chunks, Encoded, LocalEmbedding, Specification, CHUNK_BYTES},
    vector::Component,
};

fn scope() -> Scope {
    Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    }
}
fn vectors(scope: &Scope) -> Vec<Encoded> {
    ["pause session", "record receipt", "delete source"]
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let mut input = chunks(
                &format!("source-{index}"),
                scope,
                text,
                &Specification::qualified(),
            )
            .unwrap();
            let mut vector = vec![0.; 384];
            vector[index] = 1.;
            Encoded {
                identity: input.remove(0).identity,
                vector,
            }
        })
        .collect()
}

#[test]
fn chunk_offsets_scope_and_full_specification_define_cache_identity() {
    let scope = scope();
    let spec = Specification::qualified();
    let text = "東京 café function_name ".repeat(30);
    let rows = chunks("source", &scope, &text, &spec).unwrap();
    assert!(rows.len() > 1);
    for row in &rows {
        assert_eq!(row.text, text[row.identity.start..row.identity.end]);
        assert!(row.text.len() <= CHUNK_BYTES);
    }
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<String>(),
        text
    );
    let mut changed = scope.clone();
    changed.task = TaskId::new();
    assert_ne!(
        rows[0].identity.cache_key().unwrap(),
        chunks("source", &changed, &text, &spec).unwrap()[0]
            .identity
            .cache_key()
            .unwrap()
    );
    let original = spec.digest().unwrap();
    let mut changed = spec;
    changed.runtime.push_str("/new");
    assert_ne!(original, changed.digest().unwrap());
    assert!(chunks("source", &scope, &text, &changed).is_err());
}

#[test]
fn graph_preflight_bounds_headers_and_binds_exact_ids_vectors_and_centroid() {
    let scope = scope();
    let rows = vectors(&scope);
    let spec = Specification::qualified();
    let component = Component::build(scope.workspace.clone(), spec.clone(), rows.clone(), &|| {
        false
    })
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("component.json");
    component.save_private(&path, &|| false).unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let graph: Vec<u8> = serde_json::from_value(original["graph"].clone()).unwrap();
    let vector_offset = 36
        + rows
            .iter()
            .map(|row| 4 + row.identity.id.len())
            .sum::<usize>();
    let centroid_offset = vector_offset + rows.len() * spec.dimensions * 4;
    let adjacency_offset = centroid_offset + spec.dimensions * 4;
    for fault in [
        "dimensions",
        "count",
        "parameters",
        "duplicate_id",
        "nonfinite_vector",
        "swapped_vectors",
        "centroid",
        "adjacency",
        "trailing",
    ] {
        let mut changed = graph.clone();
        match fault {
            "dimensions" => changed[4..12].copy_from_slice(&u64::MAX.to_le_bytes()),
            "count" => changed[12..20].copy_from_slice(&u64::MAX.to_le_bytes()),
            "parameters" => changed[32..36].copy_from_slice(&u32::MAX.to_le_bytes()),
            "duplicate_id" => {
                let start = 36 + 4 + rows[0].identity.id.len() + 4;
                changed[start..start + rows[0].identity.id.len()]
                    .copy_from_slice(rows[0].identity.id.as_bytes());
            }
            "nonfinite_vector" => {
                changed[vector_offset..vector_offset + 4].copy_from_slice(&f32::NAN.to_le_bytes())
            }
            "swapped_vectors" => {
                let length = spec.dimensions * 4;
                changed[vector_offset..vector_offset + length]
                    .copy_from_slice(&graph[vector_offset + length..vector_offset + 2 * length]);
                changed[vector_offset + length..vector_offset + 2 * length]
                    .copy_from_slice(&graph[vector_offset..vector_offset + length]);
            }
            "centroid" => changed[centroid_offset..centroid_offset + 4]
                .copy_from_slice(&f32::NAN.to_le_bytes()),
            "adjacency" => changed[adjacency_offset..adjacency_offset + 4]
                .copy_from_slice(&u32::MAX.to_le_bytes()),
            "trailing" => changed.push(0),
            _ => unreachable!(),
        }
        let mut document = original.clone();
        document["graph"] = serde_json::to_value(changed).unwrap();
        let bytes = vcp_protocol::canonical_bytes(&document).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        // A matching physical hash cannot replace structural/mapping validation.
        let checksum = vcp_protocol::digest_bytes(&bytes);
        assert!(
            Component::open(&path, &checksum, &scope.workspace, &spec, &|| false).is_err(),
            "accepted {fault}"
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_junction_cannot_redirect_vector_component_read_or_creation() {
    let scope = scope();
    let spec = Specification::qualified();
    let component = Component::build(
        scope.workspace.clone(),
        spec.clone(),
        vectors(&scope),
        &|| false,
    )
    .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join("target");
    let link = temporary.path().join("redirect");
    std::fs::create_dir(&target).unwrap();
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path $env:VCP_VECTOR_LINK -Target $env:VCP_VECTOR_TARGET -ErrorAction Stop | Out-Null"])
        .env("VCP_VECTOR_LINK", &link).env("VCP_VECTOR_TARGET", &target).status().unwrap();
    assert!(status.success());
    assert!(component
        .save_private(&link.join("vectors.json"), &|| false)
        .is_err());
    assert!(std::fs::read_dir(&target).unwrap().next().is_none());
    let checksum = component
        .save_private(&target.join("vectors.json"), &|| false)
        .unwrap();
    assert!(Component::open(
        &link.join("vectors.json"),
        &checksum,
        &scope.workspace,
        &spec,
        &|| false
    )
    .is_err());
    assert!(Component::open(&target, &checksum, &scope.workspace, &spec, &|| false).is_err());
    std::fs::remove_dir(&link).unwrap();
}

#[test]
fn private_vector_reopen_rejects_corruption_scope_specification_and_cancellation() {
    let scope = scope();
    let rows = vectors(&scope);
    let spec = Specification::qualified();
    let component = Component::build(scope.workspace.clone(), spec.clone(), rows.clone(), &|| {
        false
    })
    .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("vectors.json");
    assert!(component.save_private_bounded(&path, 1, &|| false).is_err());
    assert!(
        !path.exists(),
        "insufficient admitted disk cannot create a partial file"
    );
    assert!(component.save_private(&path, &|| true).is_err());
    assert!(!path.exists());
    let checksum = component.save_private(&path, &|| false).unwrap();
    assert!(
        component.save_private(&path, &|| false).is_err(),
        "existing private component cannot be replaced"
    );
    let reopened = Component::open(&path, &checksum, &scope.workspace, &spec, &|| false).unwrap();
    assert_eq!(reopened.rows(), rows);
    let allowed = BTreeSet::from([rows[1].identity.id.clone()]);
    let result = reopened
        .query(&scope.workspace, &allowed, &rows[0].vector, 3, &|| false)
        .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].chunk, rows[1].identity.id);
    assert!(reopened
        .query(&WorkspaceId::new(), &allowed, &rows[0].vector, 3, &|| false)
        .is_err());
    assert!(reopened
        .query(&scope.workspace, &allowed, &rows[0].vector, 3, &|| true)
        .is_err());
    let mut changed = spec.clone();
    changed.chunker.push_str("/new");
    assert!(Component::open(&path, &checksum, &scope.workspace, &changed, &|| false).is_err());
    assert!(Component::open(&path, &checksum, &WorkspaceId::new(), &spec, &|| false).is_err());
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(Component::open(&path, &checksum, &scope.workspace, &spec, &|| false).is_err());
    assert!(Component::open(
        &temporary.path().join("missing"),
        &checksum,
        &scope.workspace,
        &spec,
        &|| false
    )
    .is_err());
    let mut invalid = rows;
    invalid[0].vector[0] = f32::NAN;
    assert!(Component::build(scope.workspace, spec, invalid, &|| false).is_err());
}

#[test]
fn narrow_authorized_subset_uses_bounded_exact_fallback_without_other_ids() {
    let scope = scope();
    let spec = Specification::qualified();
    let rows: Vec<_> = (0..300)
        .map(|index| {
            let input = chunks(&format!("source-{index}"), &scope, "retained source", &spec)
                .unwrap()
                .remove(0);
            let mut vector = vec![0.; 384];
            vector[0] = if index == 299 { -1. } else { 1. };
            Encoded {
                identity: input.identity,
                vector,
            }
        })
        .collect();
    let authorized = BTreeSet::from([rows[299].identity.id.clone()]);
    let expected = rows[299].identity.id.clone();
    let query = rows[0].vector.clone();
    let component = Component::build(scope.workspace.clone(), spec, rows, &|| false).unwrap();
    let result = component
        .query(&scope.workspace, &authorized, &query, 1, &|| false)
        .unwrap();
    assert_eq!(result.mode, vcp_memory::vector::Mode::ExactAuthorizedSubset);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].chunk, expected);
    assert!((result.rows[0].distance - 2.).abs() < 1e-6);
}

/// Explicit native gate: an absent model is a failure when this test is selected.
#[test]
#[ignore = "requires explicitly provisioned VCP_MINILM_ASSETS; run native offline qualification"]
fn real_cpu_batches_cache_reopen_and_exact_oracle() {
    let assets =
        std::env::var_os("VCP_MINILM_ASSETS").expect("explicit verified asset directory required");
    let mut model = LocalEmbedding::load(std::path::Path::new(&assets), &|| false).unwrap();
    let scope = scope();
    let spec = model.specification().clone();
    let texts = [
        "Pause the running task before changing its instructions.",
        "A durable receipt records the completed tool execution.",
        "Delete obsolete source records and rebuild the search index.",
    ];
    let mut inputs = Vec::new();
    for index in 0..18 {
        inputs.extend(
            chunks(
                &format!("source-{index}"),
                &scope,
                &format!("{} Fixture number {index}.", texts[index % 3]),
                &spec,
            )
            .unwrap(),
        );
    }
    let rows = model.embed(&inputs, &|| false).unwrap();
    assert_eq!(rows.len(), 18);
    assert_eq!(rows, model.embed(&inputs, &|| false).unwrap());
    assert!(model.embed(&inputs, &|| true).is_err());
    let query = model.query("Pause a running task", &|| false).unwrap();
    let component = Component::build(scope.workspace.clone(), spec.clone(), rows.clone(), &|| {
        false
    })
    .unwrap();
    let ids = rows.iter().map(|row| row.identity.id.clone()).collect();
    let result = component
        .query(&scope.workspace, &ids, &query, 3, &|| false)
        .unwrap();
    let mut exact: Vec<_> = rows
        .iter()
        .map(|row| {
            let dot = query
                .iter()
                .zip(&row.vector)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>();
            let qnorm = query
                .iter()
                .map(|v| f64::from(*v).powi(2))
                .sum::<f64>()
                .sqrt();
            let rnorm = row
                .vector
                .iter()
                .map(|v| f64::from(*v).powi(2))
                .sum::<f64>()
                .sqrt();
            (1. - dot / (qnorm * rnorm), row.identity.id.clone())
        })
        .collect();
    exact.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| &row.chunk)
            .collect::<BTreeSet<_>>(),
        exact[..3].iter().map(|row| &row.1).collect()
    );
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("real-vectors.json");
    let digest = component.save_private(&path, &|| false).unwrap();
    drop(component);
    let reopened = Component::open(&path, &digest, &scope.workspace, &spec, &|| false).unwrap();
    assert_eq!(
        reopened
            .query(&scope.workspace, &ids, &query, 3, &|| false)
            .unwrap()
            .rows,
        result.rows
    );
    let mut fresh = LocalEmbedding::load(std::path::Path::new(&assets), &|| false).unwrap();
    fresh.restore(reopened.rows(), &|| false).unwrap();
    assert_eq!(fresh.embed(&inputs, &|| false).unwrap(), rows);
}
