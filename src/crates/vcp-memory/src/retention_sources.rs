// SPDX-License-Identifier: Apache-2.0
//! Historical capture metadata, intentionally independent of current filesystem
//! freshness. Moving a workspace must not make its old source history unselectable.
use super::*;
use vcp_domain::retention_selector::{Criterion, Tree};

#[derive(Default)]
pub(super) struct Metadata {
    pub roots: Vec<RootId>,
    pub paths: Vec<String>,
}
fn needed(tree: &Tree) -> bool {
    match tree {
        Tree::All(children) | Tree::Any(children) => children.iter().any(needed),
        Tree::Not(child) => needed(child),
        Tree::Match(Criterion::Root(_) | Criterion::Path(_)) => true,
        _ => false,
    }
}
pub(super) fn metadata(
    store: &Store,
    access: &Access,
    tree: &Tree,
) -> Result<BTreeMap<String, Metadata>> {
    let mut result = BTreeMap::<String, Metadata>::new();
    if !needed(tree) {
        return Ok(result);
    }
    let mut total = 0u64;
    let mut manifests = 0usize;
    let mut associations = 0usize;
    for row in store.state().records.values() {
        if row.workspace != access.workspace || row.collection != Collection::Artifact {
            continue;
        }
        let manifest: ArtifactDescriptor = row.decode()?;
        if !access.allows_task(&manifest.spec.scope.task) {
            continue;
        }
        if !matches!(
            manifest.spec.schema.as_str(),
            "verification-baseline/1"
                | "verification-plan/1"
                | "verification-result/1"
                | "vcp-memory-change/1"
                | "vcp-workspace-checkpoint/1"
        ) || !crate::proof::complete_capture(&manifest)
        {
            continue;
        }
        manifests += 1;
        total = total
            .checked_add(manifest.length.get())
            .ok_or(Error::Conflict("retention source metadata overflow"))?;
        if manifests > 256 || manifest.length.get() > 1024 * 1024 || total > 8 * 1024 * 1024 {
            return Err(Error::Conflict("retention source metadata scan limit"));
        }
        let mut bytes = Vec::new();
        if vcp_audit::history::History::read_artifact(
            store,
            &access.history(),
            &manifest.spec.id,
            &mut bytes,
        )
        .is_err()
        {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let (snapshot, references) = match manifest.spec.schema.as_str() {
            "verification-baseline/1" | "vcp-workspace-checkpoint/1" => {
                (&value["manifest"], &value["sources"])
            }
            "verification-plan/1" | "verification-result/1" => {
                (&value["before"], &value["source_artifacts"])
            }
            "vcp-memory-change/1" => (&value["current"], &value["sources"]),
            _ => continue,
        };
        if snapshot["identity"]["workspace"].as_str() != Some(access.workspace.as_str()) {
            continue;
        }
        let Some(root) = snapshot["identity"]["root"]
            .as_str()
            .and_then(|id| RootId::parse(id).ok())
        else {
            continue;
        };
        let (Some(files), Some(references)) = (snapshot["files"].as_array(), references.as_array())
        else {
            continue;
        };
        if files.len() > 4096 || references.len() > 4096 {
            return Err(Error::Conflict("retention source metadata entry limit"));
        }
        for reference in references {
            let raw_id = if matches!(
                manifest.spec.schema.as_str(),
                "vcp-memory-change/1" | "vcp-workspace-checkpoint/1"
            ) {
                reference["artifact"].as_str()
            } else {
                reference.as_str()
            };
            let Some(id) = raw_id.and_then(|id| ArtifactId::parse(id).ok()) else {
                continue;
            };
            let Some(source) = store
                .state()
                .records
                .get(&key(Collection::Artifact, id.as_str()))
            else {
                continue;
            };
            if source.workspace != access.workspace {
                continue;
            }
            let descriptor: ArtifactDescriptor = source.decode()?;
            if descriptor.spec.scope != manifest.spec.scope
                || descriptor.state == CaptureState::Purged
            {
                continue;
            }
            for file in files {
                if file["root"].as_str() != Some(root.as_str())
                    || file["sha256"] != descriptor.sha256
                    || file["bytes"] != serde_json::json!(descriptor.length)
                {
                    continue;
                }
                let Some(path) = file["path"].as_str() else {
                    continue;
                };
                let path_selector = Selector {
                    schema_version: 1,
                    tree: Tree::Match(Criterion::Path(path.into())),
                };
                if path_selector.normalized().is_err() {
                    continue;
                }
                if matches!(
                    manifest.spec.schema.as_str(),
                    "vcp-memory-change/1" | "vcp-workspace-checkpoint/1"
                ) && (reference["version"]["root"].as_str() != Some(root.as_str())
                    || reference["version"]["path"] != path
                    || reference["version"]["sha256"] != descriptor.sha256)
                {
                    continue;
                }
                associations += 1;
                if associations > 16384 {
                    return Err(Error::Conflict("retention source association limit"));
                }
                let metadata = result.entry(id.to_string()).or_default();
                if !metadata.roots.contains(&root) {
                    metadata.roots.push(root.clone());
                }
                if !metadata.paths.iter().any(|existing| existing == path) {
                    metadata.paths.push(path.into());
                }
            }
        }
    }
    for metadata in result.values_mut() {
        metadata.roots.sort();
        metadata.paths.sort();
    }
    Ok(result)
}
