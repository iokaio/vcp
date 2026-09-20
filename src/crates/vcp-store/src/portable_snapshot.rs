// SPDX-License-Identifier: Apache-2.0
//! Bounded backend-neutral historical archive. Old grants are audit facts, not
//! host authority. No trusted key profile, credentials, OS handles or arbitrary
//! workspace files are read. Import/activation must independently rebind, revoke
//! grants, advance authority and pause tasks before admitting an owner.
use crate::{
    artifact::{read_bounded, reject_link},
    contract::{Collection, State},
    Error, Result, Snapshot, Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ArtifactId, Watermark, WorkspaceId,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub const FORMAT: &str = "vcp-neutral-history/1";
const MAX_PARTS: usize = 4096;
const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Role {
    Specification,
    Seal,
    Chunk { index: u64 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    artifact: ArtifactId,
    role: Role,
    digest: String,
    bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    inputs: crate::snapshot_inputs::Inputs,
    coverage: crate::snapshot_inputs::Coverage,
    format: String,
    workspace: WorkspaceId,
    watermark: Watermark,
    canonical: String,
    parts: Vec<Part>,
    authority: String,
    exclusions: Vec<String>,
}
/// Owned byte inventory, accepted only after strict bounded validation. It is
/// historical material; it does not confer a runnable canonical owner.
pub struct Archive {
    inventory: Inventory,
    state: State,
    objects: BTreeMap<String, Vec<u8>>,
}
fn scoped(state: &State, workspace: &WorkspaceId) -> Result<()> {
    state.validate()?;
    state.record(Collection::Workspace, workspace.as_str(), workspace)?;
    if state.records.values().any(|r| &r.workspace != workspace)
        || state.events.iter().any(|e| &e.event.workspace != workspace)
        || state.commands.values().any(|r| &r.workspace != workspace)
    {
        return Err(Error::Access);
    }
    Ok(())
}
fn add(objects: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) -> Result<String> {
    let digest = digest_bytes(&bytes);
    if !objects.contains_key(&digest) {
        let total = objects.values().try_fold(bytes.len(), |sum, item| {
            sum.checked_add(item.len())
                .ok_or(Error::Limit("snapshot expanded bytes"))
        })?;
        if total > MAX_BYTES || objects.len() >= MAX_PARTS {
            return Err(Error::Limit("snapshot expanded bytes or objects"));
        }
        objects.insert(digest.clone(), bytes);
    }
    Ok(digest)
}
impl Archive {
    pub fn capture(
        store: &Store,
        snapshot: &Snapshot,
        workspace: &WorkspaceId,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        Self::capture_with_inputs(
            store,
            snapshot,
            workspace,
            &crate::snapshot_inputs::Inputs::default(),
            cancelled,
        )
    }
    pub fn capture_with_inputs(
        store: &Store,
        snapshot: &Snapshot,
        workspace: &WorkspaceId,
        inputs: &crate::snapshot_inputs::Inputs,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        let coverage = inputs.validate(store, snapshot.state(), workspace)?;
        scoped(snapshot.state(), workspace)?;
        let mut objects = BTreeMap::new();
        let canonical = add(&mut objects, canonical_bytes(snapshot.state())?)?;
        let mut parts = Vec::new();
        for row in snapshot
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            if cancelled() {
                return Err(Error::Unavailable("snapshot cancelled"));
            }
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.state == CaptureState::Purged {
                continue;
            }
            store.spool().verify(&descriptor)?;
            if store.spool().inspect(&descriptor.spec.id)? != descriptor {
                return Err(Error::Conflict(
                    "artifact advanced beyond snapshot cut; rebuild snapshot",
                ));
            }
            let directory = store.spool().root().join(descriptor.spec.id.as_str());
            reject_link(&directory)?;
            let capture = fs::File::open(directory.join("owner.lock"))?;
            capture
                .try_lock_shared()
                .map_err(|_| Error::Conflict("quiesce artifact writer before snapshot"))?;
            let mut names = fs::read_dir(&directory)?
                .map(|e| e.map(|e| e.file_name()))
                .collect::<std::io::Result<Vec<_>>>()?;
            if names.len() > MAX_PARTS {
                return Err(Error::Limit("artifact snapshot entries"));
            }
            names.sort();
            for name in names {
                let name = name
                    .to_str()
                    .ok_or(Error::Corruption("artifact filename"))?;
                let role = match name {
                    "spec.json" => Role::Specification,
                    "seal.json" => Role::Seal,
                    _ if name.ends_with(".chunk") => {
                        let (index, _) = name
                            .trim_end_matches(".chunk")
                            .split_once('-')
                            .ok_or(Error::Corruption("chunk name"))?;
                        Role::Chunk {
                            index: index
                                .parse()
                                .map_err(|_| Error::Corruption("chunk index"))?,
                        }
                    }
                    _ => continue,
                };
                if parts.len() >= MAX_PARTS {
                    return Err(Error::Limit("snapshot parts"));
                }
                let bytes = read_bounded(&directory.join(name), crate::artifact::CHUNK_BYTES)?;
                let length = bytes.len() as u64;
                let digest = add(&mut objects, bytes)?;
                parts.push(Part {
                    artifact: descriptor.spec.id.clone(),
                    role,
                    digest,
                    bytes: length,
                });
            }
            store.spool().verify(&descriptor)?;
        }
        let inventory = Inventory {
            inputs: inputs.clone(),
            coverage,
            format: FORMAT.into(),
            workspace: workspace.clone(),
            watermark: snapshot.state().watermark,
            canonical,
            parts,
            authority: "historical_only_rebind_required".into(),
            exclusions: vec![
                "host_credentials_and_secret_keys".into(),
                "live_execution_authority".into(),
                "derived_indexes_require_destination_reopen_or_rebuild".into(),
                if inputs.checkpoint.is_some() {
                    "workspace_exclusions_listed_in_checkpoint".into()
                } else {
                    "uncaptured_workspace_files".into()
                },
            ],
        };
        let archive = Self {
            inventory,
            state: snapshot.state().clone(),
            objects,
        };
        archive.validate()?;
        Ok(archive)
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn coverage(&self) -> &crate::snapshot_inputs::Coverage {
        &self.inventory.coverage
    }
    pub fn inputs(&self) -> &crate::snapshot_inputs::Inputs {
        &self.inventory.inputs
    }
    pub fn retained_artifact(&self, id: &ArtifactId) -> Result<Vec<u8>> {
        let descriptor: ArtifactDescriptor = self
            .state
            .record(Collection::Artifact, id.as_str(), &self.inventory.workspace)?
            .decode()?;
        if descriptor.state != CaptureState::Complete || descriptor.length.get() > MAX_BYTES as u64
        {
            return Err(Error::Unavailable("archive artifact not complete"));
        }
        let mut chunks = self
            .inventory
            .parts
            .iter()
            .filter_map(|part| {
                if &part.artifact == id {
                    if let Role::Chunk { index } = part.role {
                        Some((index, part))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        chunks.sort_by_key(|c| c.0);
        let mut bytes = Vec::new();
        for (expected, (index, part)) in chunks.into_iter().enumerate() {
            if index != expected as u64 {
                return Err(Error::Corruption("archive chunk sequence"));
            }
            let chunk = self
                .objects
                .get(&part.digest)
                .ok_or(Error::Corruption("archive chunk missing"))?;
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > MAX_BYTES)
            {
                return Err(Error::Limit("archive artifact expansion"));
            }
            bytes.extend_from_slice(chunk);
        }
        if bytes.len() as u64 != descriptor.length.get()
            || digest_bytes(&bytes) != descriptor.sha256
        {
            return Err(Error::Corruption("archive artifact digest"));
        }
        Ok(bytes)
    }
    pub fn workspace(&self) -> &WorkspaceId {
        &self.inventory.workspace
    }
    pub fn payloads(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut objects = self.objects.clone();
        add(&mut objects, canonical_bytes(&self.inventory)?)?;
        Ok(objects)
    }
    pub fn inventory_digest(&self) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(&self.inventory)?))
    }
    pub fn decode(objects: BTreeMap<String, Vec<u8>>, inventory_digest: &str) -> Result<Self> {
        if objects.len() > MAX_PARTS {
            return Err(Error::Limit("archive objects"));
        }
        let total = objects.values().try_fold(0usize, |sum, b| {
            sum.checked_add(b.len())
                .ok_or(Error::Limit("archive bytes"))
        })?;
        if total > MAX_BYTES {
            return Err(Error::Limit("archive bytes"));
        }
        for (hash, bytes) in &objects {
            if digest_bytes(bytes) != *hash {
                return Err(Error::Corruption("archive object hash"));
            }
        }
        let bytes = objects
            .get(inventory_digest)
            .ok_or(Error::Corruption("archive inventory missing"))?;
        let inventory: Inventory = serde_json::from_slice(bytes)?;
        if canonical_bytes(&inventory)? != *bytes {
            return Err(Error::Corruption("noncanonical archive inventory"));
        }
        let state: State = serde_json::from_slice(
            objects
                .get(&inventory.canonical)
                .ok_or(Error::Corruption("canonical archive object missing"))?,
        )?;
        let mut objects = objects;
        objects.remove(inventory_digest);
        let archive = Self {
            inventory,
            state,
            objects,
        };
        archive.validate()?;
        Ok(archive)
    }
    fn validate(&self) -> Result<()> {
        let inventory = &self.inventory;
        if inventory.format != FORMAT
            || inventory.authority != "historical_only_rebind_required"
            || inventory.parts.len() > MAX_PARTS
            || inventory.watermark != self.state.watermark
        {
            return Err(Error::Incompatible);
        }
        scoped(&self.state, &inventory.workspace)?;
        let coverage =
            inventory
                .inputs
                .validate_with(&self.state, &inventory.workspace, &|id| {
                    self.retained_artifact(id)
                })?;
        if coverage != inventory.coverage {
            return Err(Error::Corruption("archive coverage differs"));
        }
        if self.objects.get(&inventory.canonical) != Some(&canonical_bytes(&self.state)?) {
            return Err(Error::Corruption("canonical archive bytes"));
        }
        let mut referenced = BTreeSet::from([inventory.canonical.clone()]);
        let mut identities = BTreeSet::new();
        let mut specifications = BTreeSet::new();
        for part in &inventory.parts {
            let row = self.state.record(
                Collection::Artifact,
                part.artifact.as_str(),
                &inventory.workspace,
            )?;
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.state == CaptureState::Purged {
                return Err(Error::Corruption("purged archive bytes"));
            }
            let bytes = self
                .objects
                .get(&part.digest)
                .ok_or(Error::Corruption("archive part missing"))?;
            if bytes.len() as u64 != part.bytes
                || digest_bytes(bytes) != part.digest
                || !identities.insert((part.artifact.clone(), canonical_bytes(&part.role)?))
            {
                return Err(Error::Corruption("duplicate or invalid archive part"));
            }
            if part.role == Role::Specification {
                specifications.insert(part.artifact.clone());
            }
            referenced.insert(part.digest.clone());
        }
        for row in self
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let d: ArtifactDescriptor = row.decode()?;
            if d.state != CaptureState::Purged && !specifications.contains(&d.spec.id) {
                return Err(Error::Corruption("artifact archive specification missing"));
            }
        }
        if referenced != self.objects.keys().cloned().collect() {
            return Err(Error::Corruption("unreferenced archive object"));
        }
        Ok(())
    }
    /// Materialize only whitelisted spool roles under a newly owned staging
    /// root. Caller still must validate/revoke/pause before any activation.
    pub(crate) fn write_spool(&self, root: &Path) -> Result<()> {
        fs::create_dir(root)?;
        for row in self
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let d: ArtifactDescriptor = row.decode()?;
            if d.state == CaptureState::Purged {
                continue;
            }
            let dir = root.join(d.spec.id.as_str());
            fs::create_dir(&dir)?;
            for part in self
                .inventory
                .parts
                .iter()
                .filter(|p| p.artifact == d.spec.id)
            {
                let name = match part.role {
                    Role::Specification => "spec.json".into(),
                    Role::Seal => "seal.json".into(),
                    Role::Chunk { index } => format!("{index:020}-{}.chunk", part.digest),
                };
                crate::private_paths::write_private(&dir.join(name), &self.objects[&part.digest])?;
            }
            fs::File::create(dir.join("owner.lock"))?.sync_all()?;
            fs::File::create(dir.join("snapshot.lock"))?.sync_all()?;
        }
        Ok(())
    }
}

/// Historical staging proof. It deliberately contains no Store, owner permit,
/// activation pointer or format marker accepted by Store::open.
pub struct StagedHistory {
    root: std::path::PathBuf,
    state: State,
    _directory: crate::private_paths::Directory,
}
impl StagedHistory {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn state(&self) -> &State {
        &self.state
    }
}
impl Archive {
    pub fn stage_history(
        &self,
        private_parent: &Path,
        forbidden: &[std::path::PathBuf],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<StagedHistory> {
        self.validate()?;
        let parent = crate::private_paths::Directory::open(private_parent, forbidden)?;
        if cancelled() {
            return Err(Error::Unavailable("snapshot staging cancelled"));
        }
        let root = parent.path.join(vcp_domain::TransactionId::new().as_str());
        fs::create_dir(&root)?;
        let directory = crate::private_paths::Directory::open(&root, forbidden)?;
        crate::private_paths::write_private(
            &root.join("canonical-history.json"),
            &canonical_bytes(&self.state)?,
        )?;
        if cancelled() {
            return Err(Error::Unavailable(
                "snapshot staging cancelled; owned history remains",
            ));
        }
        self.write_spool(&root.join("spool"))?;
        let spool = crate::artifact::Spool::open(
            &root.join("spool"),
            forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
        )?;
        for row in self
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            if cancelled() {
                return Err(Error::Unavailable(
                    "snapshot staging cancelled; owned history remains",
                ));
            }
            let descriptor: ArtifactDescriptor = row.decode()?;
            if descriptor.state != CaptureState::Purged {
                spool.verify(&descriptor)?;
            }
        }
        crate::private_paths::write_private(
            &root.join("historical-only.json"),
            &canonical_bytes(&self.inventory)?,
        )?;
        Ok(StagedHistory {
            root,
            state: self.state.clone(),
            _directory: directory,
        })
    }
}

/// Derivative bytes preserved for a qualified destination engine to reopen.
/// This type deliberately has no `ready` flag or canonical activation method.
pub struct StagedGeneration {
    root: std::path::PathBuf,
    _directory: crate::private_paths::Directory,
}
impl StagedGeneration {
    pub fn root(&self) -> &Path {
        &self.root
    }
}
impl Archive {
    pub fn stage_generation(
        &self,
        id: &vcp_domain::GenerationId,
        private_parent: &Path,
        forbidden: &[std::path::PathBuf],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<StagedGeneration> {
        self.validate()?;
        let input = self
            .inventory
            .inputs
            .generations
            .iter()
            .find(|g| &g.id == id)
            .ok_or(Error::Unavailable("generation rebuild required"))?;
        let parent = crate::private_paths::Directory::open(private_parent, forbidden)?;
        if cancelled() {
            return Err(Error::Unavailable("generation staging cancelled"));
        }
        let root = parent.path.join(id.as_str());
        fs::create_dir(&root)?;
        let directory = crate::private_paths::Directory::open(&root, forbidden)?;
        let lexical = root.join("lexical");
        fs::create_dir(&lexical)?;
        for (path, artifact) in [
            (root.join("inventory.json"), &input.inventory),
            (lexical.join("vcp-lexical.json"), &input.lexical_manifest),
        ] {
            crate::private_paths::write_private(&path, &self.retained_artifact(artifact)?)?;
        }
        for (name, artifact) in &input.lexical_files {
            if cancelled() {
                return Err(Error::Unavailable(
                    "generation staging cancelled; owned files remain",
                ));
            }
            if !crate::snapshot_inputs::relative(name) || name.contains('/') {
                return Err(Error::Access);
            }
            crate::private_paths::write_private(
                &lexical.join(name),
                &self.retained_artifact(artifact)?,
            )?;
        }
        if let Some(artifact) = &input.vectors {
            crate::private_paths::write_private(
                &root.join("vectors.json"),
                &self.retained_artifact(artifact)?,
            )?;
        }
        let generation: vcp_domain::search::Generation = self
            .state
            .record(
                Collection::Generation,
                id.as_str(),
                &self.inventory.workspace,
            )?
            .decode()?;
        crate::private_paths::write_private(
            &root.join("manifest.json"),
            &canonical_bytes(&generation)?,
        )?;
        Ok(StagedGeneration {
            root,
            _directory: directory,
        })
    }
}
