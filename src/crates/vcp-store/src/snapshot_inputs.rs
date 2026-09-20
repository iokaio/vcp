// SPDX-License-Identifier: Apache-2.0
//! Host-captured workspace and derivative inputs. This layer verifies retained
//! canonical artifacts; it never runs Git, reads arbitrary workspace files, or
//! claims a derivative is ready before the destination engine reopens it.
use crate::{
    contract::{Collection, State},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    search::Generation,
    workspace::Workspace,
    ArtifactId, GenerationId, WorkspaceId,
};
use vcp_protocol::canonical_bytes;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub checkpoint: Option<Checkpoint>,
    pub generations: Vec<GenerationInput>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub manifest: ArtifactId,
    pub sources: BTreeMap<String, ArtifactId>,
    pub git: Option<GitArtifacts>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitArtifacts {
    pub status: ArtifactId,
    pub index: ArtifactId,
    pub staged_diff: ArtifactId,
    pub unstaged_diff: ArtifactId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationInput {
    pub id: GenerationId,
    pub inventory: ArtifactId,
    pub lexical_manifest: ArtifactId,
    pub lexical_files: BTreeMap<String, ArtifactId>,
    pub vectors: Option<ArtifactId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    PreservedReopenRequired,
    RebuildRequired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationStatus {
    pub id: GenerationId,
    pub compatibility: Compatibility,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub workspace_checkpoint: bool,
    pub workspace_exclusions: Vec<String>,
    pub generations: Vec<GenerationStatus>,
}
fn descriptor(
    state: &State,
    workspace: &WorkspaceId,
    id: &ArtifactId,
) -> Result<ArtifactDescriptor> {
    let descriptor: ArtifactDescriptor = state
        .record(Collection::Artifact, id.as_str(), workspace)?
        .decode()?;
    if descriptor.state != CaptureState::Complete || descriptor.length.get() > 4 * 1024 * 1024 {
        return Err(Error::Unavailable(
            "backup input incomplete or beyond bound",
        ));
    }
    Ok(descriptor)
}
pub(crate) fn relative(path: &str) -> bool {
    if path.is_empty() || path.len() > 4096 || path.contains(['\\', ':', '\0']) {
        return false;
    }
    path.split('/').all(|part| {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>\"|?*".contains(c))
        {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    })
}
impl Inputs {
    pub(crate) fn validate_with(
        &self,
        state: &State,
        workspace: &WorkspaceId,
        read: &dyn Fn(&ArtifactId) -> Result<Vec<u8>>,
    ) -> Result<Coverage> {
        if self.generations.len() > 64 {
            return Err(Error::Limit("backup generations"));
        }
        let ws: Workspace = state
            .record(Collection::Workspace, workspace.as_str(), workspace)?
            .decode()?;
        let mut exclusions = Vec::new();
        if let Some(checkpoint) = &self.checkpoint {
            if checkpoint.sources.len() > 4096 {
                return Err(Error::Limit("backup workspace files"));
            }
            let manifest_descriptor = descriptor(state, workspace, &checkpoint.manifest)?;
            if manifest_descriptor.spec.schema != "vcp-workspace-checkpoint/1" {
                return Err(Error::Incompatible);
            }
            let bytes = read(&checkpoint.manifest)?;
            let envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
            let manifest = &envelope["manifest"];
            let associations = envelope["sources"]
                .as_array()
                .ok_or(Error::Corruption("checkpoint source associations"))?;
            if associations.len() != checkpoint.sources.len() {
                return Err(Error::Corruption("checkpoint source association closure"));
            }
            let mut required_refs = BTreeSet::new();
            if manifest["version"] != 1
                || manifest["bounded_scan_complete"] != true
                || manifest["identity"]["workspace"].as_str() != Some(workspace.as_str())
                || manifest["identity"]["binding"] != serde_json::to_value(ws.binding.revision)?
                || manifest["identity"]["repository"].as_str()
                    != Some(ws.binding.repository.as_str())
                || manifest["identity"]["worktree"].as_str() != Some(ws.binding.worktree.as_str())
            {
                return Err(Error::Conflict(
                    "backup workspace checkpoint binding or completeness",
                ));
            }
            let files = manifest["files"]
                .as_array()
                .ok_or(Error::Corruption("checkpoint files"))?;
            if files.len() != checkpoint.sources.len() {
                return Err(Error::Corruption("checkpoint source closure"));
            }
            let mut paths = BTreeSet::new();
            for file in files {
                let path = file["path"]
                    .as_str()
                    .ok_or(Error::Corruption("checkpoint file path"))?;
                if !relative(path) || !paths.insert(path.to_lowercase()) {
                    return Err(Error::Access);
                }
                let id = checkpoint
                    .sources
                    .get(path)
                    .ok_or(Error::Corruption("checkpoint source missing"))?;
                let matches = associations
                    .iter()
                    .filter(|entry| {
                        entry["artifact"].as_str() == Some(id.as_str()) && entry["version"] == *file
                    })
                    .count();
                if matches != 1 {
                    return Err(Error::Corruption("checkpoint source association"));
                }
                required_refs.insert(crate::contract::key(Collection::Artifact, id.as_str()));
                let source = descriptor(state, workspace, id)?;
                if file["sha256"].as_str() != Some(source.sha256.as_str())
                    || file["bytes"] != serde_json::to_value(source.length)?
                    || file["binding"] != manifest["identity"]["binding"]
                    || file["root"] != manifest["identity"]["root"]
                {
                    return Err(Error::Corruption("checkpoint source version"));
                }
            }
            let ignored = manifest["exclusions"]
                .as_array()
                .ok_or(Error::Corruption("checkpoint exclusions"))?;
            if ignored.len() > 4096 {
                return Err(Error::Limit("checkpoint exclusions"));
            }
            for item in ignored {
                let text = String::from_utf8(canonical_bytes(item)?)
                    .map_err(|_| Error::Corruption("checkpoint exclusion"))?;
                if text.len() > 8192 {
                    return Err(Error::Limit("checkpoint exclusion"));
                }
                exclusions.push(text);
            }
            match (
                &checkpoint.git,
                manifest.get("git").filter(|g| !g.is_null()),
            ) {
                (Some(git), Some(snapshot)) => {
                    if snapshot["identity"] != manifest["identity"] {
                        return Err(Error::Corruption("Git checkpoint root"));
                    }
                    for (id, field) in [
                        (&git.status, "status_sha256"),
                        (&git.index, "index_sha256"),
                        (&git.staged_diff, "staged_diff_sha256"),
                        (&git.unstaged_diff, "unstaged_diff_sha256"),
                    ] {
                        required_refs
                            .insert(crate::contract::key(Collection::Artifact, id.as_str()));
                        if snapshot[field].as_str()
                            != Some(descriptor(state, workspace, id)?.sha256.as_str())
                        {
                            return Err(Error::Corruption("Git checkpoint bytes"));
                        }
                    }
                }
                (None, None) => {}
                _ => return Err(Error::Corruption("Git checkpoint incomplete")),
            }
        } else {
            exclusions
                .push("workspace checkpoint not supplied; uncaptured files unavailable".into());
        }
        let mut supplied = BTreeSet::new();
        let mut generations = Vec::new();
        for input in &self.generations {
            if !supplied.insert(input.id.clone()) || input.lexical_files.len() > 256 {
                return Err(Error::Limit("duplicate or oversized generation"));
            }
            let generation: Generation = state
                .record(Collection::Generation, input.id.as_str(), workspace)?
                .decode()?;
            generation.validate()?;
            if generation.authority != ws.authority || generation.deletion != ws.deletion {
                return Err(Error::Conflict("generation epoch stale; rebuild required"));
            }
            let inventory: serde_json::Value = serde_json::from_slice(&read(&input.inventory)?)?;
            if inventory["workspace"].as_str() != Some(workspace.as_str()) {
                return Err(Error::Access);
            }
            let rows = inventory["records"]
                .as_array()
                .ok_or(Error::Corruption("generation retained inventory"))?;
            if rows.len() > 16384 {
                return Err(Error::Limit("generation source references"));
            }
            let mut source_refs = BTreeSet::new();
            for row in rows {
                if row["scope"]["workspace"].as_str() != Some(workspace.as_str()) {
                    return Err(Error::Access);
                }
                let (collection, id) = match row["source"]["kind"].as_str() {
                    Some("artifact") => (Collection::Artifact, row["source"]["id"].as_str()),
                    Some("claim") => (Collection::Claim, row["source"]["version"].as_str()),
                    _ => return Err(Error::Corruption("generation source kind")),
                };
                let id = id.ok_or(Error::Corruption("generation source identity"))?;
                let source = state.record(collection, id, workspace)?;
                if source.value["document_type"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("vcp_memory_redacted_"))
                {
                    return Err(Error::Conflict("generation source was purged"));
                }
                if collection == Collection::Artifact
                    && source.decode::<ArtifactDescriptor>()?.state == CaptureState::Purged
                {
                    return Err(Error::Conflict("generation source was purged"));
                }
                source_refs.insert(crate::contract::key(collection, id));
            }
            for artifact in std::iter::once(&input.inventory)
                .chain(std::iter::once(&input.lexical_manifest))
                .chain(input.lexical_files.values())
                .chain(input.vectors.iter())
            {
                if !source_refs.is_subset(
                    &state
                        .record(Collection::Artifact, artifact.as_str(), workspace)?
                        .required_references()?,
                ) {
                    return Err(Error::Corruption(
                        "backup derivative provenance references missing",
                    ));
                }
            }

            if descriptor(state, workspace, &input.inventory)?.sha256
                != generation.inventory_checksum
                || descriptor(state, workspace, &input.lexical_manifest)?.sha256
                    != generation.lexical_checksum
            {
                return Err(Error::Corruption("generation inventory checksum"));
            }
            let lexical: serde_json::Value =
                serde_json::from_slice(&read(&input.lexical_manifest)?)?;
            if lexical["schema"] != generation.lexical_schema
                || lexical["tokenizer"].as_str() != Some(generation.tokenizer.as_str())
                || lexical["inventory"].as_str() != Some(generation.inventory_digest.as_str())
            {
                return Err(Error::Incompatible);
            }
            let parts = lexical["components"]
                .as_array()
                .ok_or(Error::Corruption("lexical component inventory"))?;
            if parts.len() != input.lexical_files.len() {
                return Err(Error::Corruption("lexical component closure"));
            }
            let mut names = BTreeSet::new();
            for part in parts {
                let name = part["name"]
                    .as_str()
                    .ok_or(Error::Corruption("lexical component name"))?;
                if !relative(name) || name.contains('/') || !names.insert(name.to_lowercase()) {
                    return Err(Error::Access);
                }
                let id = input
                    .lexical_files
                    .get(name)
                    .ok_or(Error::Corruption("lexical component missing"))?;
                let d = descriptor(state, workspace, id)?;
                if part["sha256"].as_str() != Some(d.sha256.as_str())
                    || part["bytes"].as_u64() != Some(d.length.get())
                {
                    return Err(Error::Corruption("lexical component bytes"));
                }
            }
            match (&input.vectors, &generation.vector_checksum) {
                (Some(id), Some(hash)) if &descriptor(state, workspace, id)?.sha256 == hash => {}
                (None, None) => {}
                _ => return Err(Error::Corruption("generation vector bytes")),
            }
            generations.push(GenerationStatus{id:input.id.clone(),compatibility:Compatibility::PreservedReopenRequired,reason:"destination engine/specification must reopen and validate before search is ready".into()});
        }
        for record in state.records.values().filter(|r| {
            r.workspace == *workspace
                && r.collection == Collection::Generation
                && r.value["document_type"] == vcp_domain::search::GENERATION
        }) {
            let generation: Generation = record.decode()?;
            if !supplied.contains(&generation.id) {
                generations.push(GenerationStatus{id:generation.id,compatibility:Compatibility::RebuildRequired,reason:"derivative files not captured; rebuild from retained canonical sources; no ready claim".into()});
            }
        }
        Ok(Coverage {
            workspace_checkpoint: self.checkpoint.is_some(),
            workspace_exclusions: exclusions,
            generations,
        })
    }
}

pub(crate) fn generation_component(
    record: &crate::contract::Record,
) -> Result<Option<GenerationId>> {
    if record.collection != Collection::Artifact {
        return Ok(None);
    }
    let descriptor: ArtifactDescriptor = record.decode()?;
    if descriptor.spec.schema != "vcp-backup-generation-component/1" {
        return Ok(None);
    }
    let id = descriptor
        .spec
        .source
        .strip_prefix("generation:")
        .ok_or(Error::Corruption("generation component origin"))?;
    let id = GenerationId::parse(id)?;
    if record.references.iter().any(|reference| {
        !reference.starts_with("artifact:")
            && !reference.starts_with("claim:")
            && reference != &crate::contract::key(Collection::Generation, id.as_str())
    }) {
        return Err(Error::Corruption("generation component reference type"));
    }
    Ok(Some(id))
}
pub(crate) fn component_scope(state: &State, record: &crate::contract::Record) -> Result<()> {
    let Some(id) = generation_component(record)? else {
        return Ok(());
    };
    let descriptor: ArtifactDescriptor = record.decode()?;
    let generation: Generation = state
        .record(Collection::Generation, id.as_str(), &record.workspace)?
        .decode()?;
    generation.validate()?;
    if generation.scope != descriptor.spec.scope {
        return Err(Error::Access);
    }
    for reference in &record.references {
        let target = state
            .records
            .get(reference)
            .ok_or(Error::Corruption("component source missing"))?;
        if target.workspace != record.workspace {
            return Err(Error::Access);
        }
        if target.collection == Collection::Claim
            && !matches!(
                target.value["document_type"].as_str(),
                Some("vcp_memory_version_v1" | "vcp_memory_redacted_version_v1")
            )
        {
            return Err(Error::Corruption("component claim version reference"));
        }
    }
    Ok(())
}
pub(crate) fn cross_task_provenance(
    record: &crate::contract::Record,
    target: &crate::contract::Record,
    reference: &str,
) -> Result<bool> {
    Ok(generation_component(record)?.is_some()
        && record.references.contains(reference)
        && matches!(target.collection, Collection::Artifact | Collection::Claim))
}
