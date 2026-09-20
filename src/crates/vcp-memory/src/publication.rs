// SPDX-License-Identifier: Apache-2.0
//! Coherent derived generations. Only the canonical transaction activates files.
use crate::{
    access::{self, Access},
    embedding::{self, LocalEmbedding, Specification},
    lexical,
    search_record::{Inventory, TextSource},
    tokenizer, vector, Error, Result,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use vcp_domain::{
    memory::{IndexIntent, IndexStatus, ProposalResult},
    search::{Active, Generation, ACTIVE, GENERATION},
    workspace::Scope,
    *,
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{
    contract::{key, CanonicalStore, Collection, Mutation, Receipt, Record, State, Transaction},
    Store,
};

const MAX_INVENTORY_BYTES: u64 = 64 * 1024 * 1024;
#[path = "publication_snapshot.rs"]
mod snapshot;
pub use snapshot::SnapshotFiles;
fn io(error: impl std::fmt::Display) -> Error {
    Error::Invalid(error.to_string())
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
fn active(state: &State, workspace: &WorkspaceId) -> Result<Option<Active>> {
    state
        .records
        .get(&key(Collection::Generation, workspace.as_str()))
        .map(|row| {
            let value: Active = row.decode()?;
            value.validate()?;
            if &value.workspace != workspace {
                return Err(Error::Access);
            }
            Ok(value)
        })
        .transpose()
}
fn generation(state: &State, id: &GenerationId, workspace: &WorkspaceId) -> Result<Generation> {
    let result: Generation = state
        .record(Collection::Generation, id.as_str(), workspace)?
        .decode()?;
    result.validate()?;
    Ok(result)
}

pub struct Snapshot {
    scope: Scope,
    inventory: Inventory,
    previous: Option<Active>,
    intents: Vec<IndexIntent>,
    memory_seq: MemorySeq,
}
/// Capture exactly N. Workspace publication requires the complete authorized
/// inventory; a restricted task view cannot acknowledge workspace-wide intents.
pub fn capture(
    store: &Store,
    access: &Access,
    scope: &Scope,
    inventory: Inventory,
) -> Result<Snapshot> {
    let workspace = access::authorize(store.state(), access, true)?;
    inventory.validate()?;
    if access.tasks.is_some()
        || scope.workspace != access.workspace
        || inventory.workspace != access.workspace
        || inventory.watermark != store.state().watermark
        || inventory.authority != workspace.authority
        || inventory.deletion != workspace.deletion
    {
        return Err(Error::Access);
    }
    let mut represented = BTreeSet::new();
    for source in inventory.records.iter().map(|r| &r.source).chain(
        inventory
            .exclusions
            .iter()
            .filter_map(|r| r.source.as_ref()),
    ) {
        if let TextSource::Claim { version, .. } = source {
            represented.insert(version.clone());
        }
    }
    let mut intents = Vec::new();
    let mut memory_seq = MemorySeq::ZERO;
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.workspace == access.workspace)
    {
        if row.collection == Collection::IndexIntent
            && row.value["document_type"] == "vcp_memory_index_intent_v1"
        {
            let intent: IndexIntent = row.decode()?;
            if intent
                .versions
                .iter()
                .any(|version| !represented.contains(version))
            {
                return Err(Error::Conflict(
                    "publication inventory omits an intent version",
                ));
            }
            intents.push(intent);
        }
        if row.collection == Collection::Projection
            && row.value["document_type"] == vcp_domain::redaction::RESULT
        {
            memory_seq = memory_seq.max(
                row.decode::<vcp_domain::redaction::RedactedResult>()?
                    .memory_seq,
            );
        }
        if row.collection == Collection::Projection
            && row.value["document_type"] == "vcp_memory_result_v1"
        {
            let result: ProposalResult = row.decode()?;
            memory_seq = memory_seq.max(result.memory_seq);
        }
    }
    if intents.len() > 4096 {
        return Err(Error::Conflict("publication intent limit"));
    }
    intents.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Snapshot {
        scope: scope.clone(),
        inventory,
        previous: active(store.state(), &access.workspace)?,
        intents,
        memory_seq,
    })
}

#[derive(Clone)]
pub struct Prepared {
    manifest: Generation,
    previous: Option<Active>,
    intents: Vec<IndexIntent>,
}
/// Created only after components are frozen and reopened outside the owner.
/// File handles deny write/delete sharing on qualified Windows hosts and remain
/// alive through the canonical activation transaction.
pub struct Validated {
    prepared: Prepared,
    manager: Arc<Mutex<Pins>>,
    _files: Vec<fs::File>,
    _pin: Pin,
}
impl Validated {
    pub fn manifest(&self) -> &Generation {
        self.prepared.manifest()
    }
}
enum VectorInput<'a> {
    AdmittedModel(Option<&'a mut LocalEmbedding>),
    Prepared { path: &'a Path, checksum: &'a str },
}
impl Prepared {
    pub fn manifest(&self) -> &Generation {
        &self.manifest
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Barrier {
    BeforeLexical,
    AfterLexical,
    BeforeVector,
    AfterVector,
    BeforeManifest,
    AfterManifest,
    BeforeActivation,
    AfterActivation,
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    Ok(())
}
fn bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() || redirected(&metadata) || metadata.len() > max {
        return Err(Error::Conflict("generation file boundary"));
    }
    let mut bytes = Vec::new();
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(io)?;
    let opened = file.metadata().map_err(io)?;
    if !opened.is_file() || redirected(&opened) || opened.len() > max {
        return Err(Error::Conflict("opened generation file boundary"));
    }
    file.take(max + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > max {
        return Err(Error::Conflict("generation file limit"));
    }
    Ok(bytes)
}

#[derive(Default)]
struct Pins {
    readers: BTreeMap<GenerationId, usize>,
    retired: BTreeSet<GenerationId>,
}
pub struct Publisher {
    root: PathBuf,
    pins: Arc<Mutex<Pins>>,
    _directory: fs::File,
}
static MANAGERS: std::sync::OnceLock<Mutex<BTreeMap<PathBuf, std::sync::Weak<Mutex<Pins>>>>> =
    std::sync::OnceLock::new();

/// Pin an owned directory's namespace while allowing child-file writes. Ancestors
/// above the enrolled private root remain the trusted host's responsibility.
fn hold_directory(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(3)
            .custom_flags(0x0020_0000 | 0x0200_0000);
    }
    let file = options.open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(Error::Conflict("opened generation directory boundary"));
    }
    Ok(file)
}

#[cfg(windows)]
fn freeze_files(root: &Path) -> Result<Vec<fs::File>> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    fn visit(path: &Path, depth: usize, bytes: &mut u64, files: &mut Vec<fs::File>) -> Result<()> {
        if depth > 8 || files.len() >= 4096 {
            return Err(Error::Conflict("generation freeze limit"));
        }
        let metadata = fs::symlink_metadata(path).map_err(io)?;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Conflict("generation freeze reparse point"));
        }
        // Tantivy reader coordination files carry no component content and
        // lexical::components explicitly excludes them from the manifest.
        if metadata.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".lock"))
        {
            return Ok(());
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000 | if metadata.is_dir() { 0x0200_0000 } else { 0 })
            .open(path)
            .map_err(io)?;
        let opened = file.metadata().map_err(io)?;
        if redirected(&opened)
            || opened.is_dir() != metadata.is_dir()
            || opened.is_file() != metadata.is_file()
        {
            return Err(Error::Conflict("opened generation freeze boundary"));
        }
        *bytes = bytes
            .checked_add(metadata.len())
            .ok_or(Error::Conflict("generation freeze size"))?;
        if *bytes > 640 * 1024 * 1024 {
            return Err(Error::Conflict("generation freeze byte limit"));
        }
        files.push(file);
        if metadata.is_dir() {
            for child in fs::read_dir(path).map_err(io)? {
                visit(&child.map_err(io)?.path(), depth + 1, bytes, files)?;
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, 0, &mut 0, &mut files)?;
    Ok(files)
}
#[cfg(not(windows))]
fn freeze_files(_: &Path) -> Result<Vec<fs::File>> {
    Err(Error::Conflict(
        "publication activation file-sharing boundary requires qualified Windows host",
    ))
}

struct Pin {
    id: GenerationId,
    pins: Arc<Mutex<Pins>>,
}
impl Drop for Pin {
    fn drop(&mut self) {
        if let Ok(mut pins) = self.pins.lock() {
            if let Some(count) = pins.readers.get_mut(&self.id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    pins.readers.remove(&self.id);
                }
            }
        }
    }
}
pub struct View {
    pub manifest: Generation,
    pub inventory: Inventory,
    pub lexical: lexical::Reader,
    pub vector: Option<vector::Component>,
    // Last field: all actual component readers drop before GC may acquire files.
    _pin: Pin,
}
pub struct Recovery {
    pub view: Option<View>,
    pub canonical_head: Watermark,
    pub failed_components: Vec<GenerationId>,
    pub rebuild_required: bool,
}
pub struct GarbagePolicy {
    pub retained: BTreeSet<GenerationId>,
    pub retention_idle: bool,
    pub historical_deletion_allowed: bool,
}
impl Publisher {
    /// Trusted host resource accounting; never expose this private path in query diagnostics.
    pub fn storage_root(&self) -> &Path {
        &self.root
    }
    /// One manager per canonical owner. The owner lease excludes another process.
    pub fn new(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).map_err(io)?;
        Self::open_existing(root)
    }
    /// Read-only opening never creates a missing generation directory.
    pub fn open_existing(root: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(root).map_err(io)?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(Error::Conflict("generation root boundary"));
        }
        let directory = hold_directory(root)?;
        let root = root.canonicalize().map_err(io)?;
        let mut managers = MANAGERS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| Error::Conflict("generation manager registry"))?;
        managers.retain(|_, pins| pins.strong_count() != 0);
        let pins = managers
            .get(&root)
            .and_then(std::sync::Weak::upgrade)
            .unwrap_or_else(|| {
                let pins = Arc::new(Mutex::new(Pins::default()));
                managers.insert(root.clone(), Arc::downgrade(&pins));
                pins
            });
        Ok(Self {
            root,
            pins,
            _directory: directory,
        })
    }
    fn directory(&self, id: &GenerationId) -> Result<PathBuf> {
        let path = self.root.join(id.as_str());
        let metadata = fs::symlink_metadata(&path).map_err(io)?;
        if !metadata.is_dir()
            || redirected(&metadata)
            || path.canonicalize().map_err(io)?.parent() != Some(self.root.as_path())
        {
            return Err(Error::Conflict("generation directory boundary"));
        }
        Ok(path)
    }
    /// Qualification/explicitly admitted callers only. The caller owns the CPU
    /// permit through model lifetime; this method never schedules hidden work.
    /// Production hosts should adopt their already-admitted component below.
    pub fn prepare(
        &self,
        snapshot: Snapshot,
        model: Option<&mut LocalEmbedding>,
        cancel: &AtomicBool,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Prepared> {
        self.prepare_inner(snapshot, VectorInput::AdmittedModel(model), cancel, barrier)
    }
    /// Adopt the exact checksummed output of admitted host CPU work. A resource
    /// observation may advance canonical N without changing any chunk identity;
    /// every source/scope/digest/span/specification must still match the new view.
    pub fn prepare_with_vectors(
        &self,
        snapshot: Snapshot,
        private_component: &Path,
        trusted_checksum: &str,
        cancel: &AtomicBool,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Prepared> {
        self.prepare_inner(
            snapshot,
            VectorInput::Prepared {
                path: private_component,
                checksum: trusted_checksum,
            },
            cancel,
            barrier,
        )
    }
    fn prepare_inner(
        &self,
        snapshot: Snapshot,
        input: VectorInput<'_>,
        cancel: &AtomicBool,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Prepared> {
        let cancelled = || cancel.load(Ordering::Acquire);
        embedding::check(&cancelled)?;
        let id = GenerationId::new();
        let directory = self.root.join(id.as_str());
        fs::create_dir(&directory).map_err(io)?;
        let _directory = hold_directory(&directory)?;
        fs::create_dir(directory.join("lexical")).map_err(io)?;
        let _lexical_directory = hold_directory(&directory.join("lexical"))?;
        let inventory_bytes = canonical_bytes(&snapshot.inventory)?;
        if inventory_bytes.len() as u64 > MAX_INVENTORY_BYTES {
            return Err(Error::Conflict("generation inventory limit"));
        }
        write_new(&directory.join("inventory.json"), &inventory_bytes)?;
        barrier(Barrier::BeforeLexical);
        drop(lexical::build(
            &directory.join("lexical"),
            &snapshot.inventory,
            lexical::Limits::default(),
            cancel,
        )?);
        barrier(Barrier::AfterLexical);
        let lexical_bytes = bounded(&directory.join("lexical/vcp-lexical.json"), 1024 * 1024)?;
        let mut vector_checksum = None;
        barrier(Barrier::BeforeVector);
        match input {
            VectorInput::Prepared { path, checksum } => {
                let component = vector::Component::open(
                    path,
                    checksum,
                    &snapshot.inventory.workspace,
                    &Specification::qualified(),
                    &cancelled,
                )?;
                let expected = embedding::inventory_chunks(&snapshot.inventory)?;
                if expected.len() != component.rows().len()
                    || expected
                        .iter()
                        .zip(component.rows())
                        .any(|(a, b)| a.identity != b.identity)
                {
                    return Err(Error::Conflict("admitted vector inventory changed"));
                }
                // Read again into a bounded buffer and bind the bytes being copied
                // to the trusted receipt, closing replacement between open/copy.
                let bytes = bounded(path, 32 * 1024 * 1024)?;
                if digest_bytes(&bytes) != checksum {
                    return Err(Error::Conflict("admitted vector bytes changed"));
                }
                embedding::check(&cancelled)?;
                write_new(&directory.join("vectors.json"), &bytes)?;
                embedding::check(&cancelled)?;
                vector_checksum = Some(checksum.to_owned());
            }
            VectorInput::AdmittedModel(Some(model)) if !snapshot.inventory.records.is_empty() => {
                let chunks = embedding::inventory_chunks(&snapshot.inventory)?;
                if !chunks.is_empty() {
                    let vectors = model.embed(&chunks, &cancelled)?;
                    let component = vector::Component::build(
                        snapshot.scope.workspace.clone(),
                        Specification::qualified(),
                        vectors,
                        &cancelled,
                    )?;
                    vector_checksum =
                        Some(component.save_private(&directory.join("vectors.json"), &cancelled)?);
                }
            }
            VectorInput::AdmittedModel(_) => (),
        }
        barrier(Barrier::AfterVector);
        let empty_complete = snapshot.inventory.records.is_empty();
        let full = vector_checksum.is_some() || empty_complete;
        let manifest = Generation {
            document_type: GENERATION.into(),
            schema_version: 1,
            id,
            scope: snapshot.scope,
            revision: Revision::ZERO,
            transaction: TransactionId::new(),
            previous: snapshot.previous.as_ref().map(|a| a.generation.clone()),
            canonical_watermark: snapshot.inventory.watermark,
            memory_seq: if full {
                snapshot.memory_seq
            } else {
                MemorySeq::ZERO
            },
            authority: snapshot.inventory.authority,
            deletion: snapshot.inventory.deletion,
            inventory_digest: snapshot.inventory.digest.clone(),
            inventory_checksum: digest_bytes(&inventory_bytes),
            lexical_schema: lexical::SCHEMA_VERSION,
            tokenizer: tokenizer::TOKENIZER_VERSION.into(),
            lexical_checksum: digest_bytes(&lexical_bytes),
            embedding_specification: Specification::qualified().digest()?,
            vector_checksum,
            empty_complete,
            vector_deficits: if full {
                vec![]
            } else {
                snapshot
                    .inventory
                    .records
                    .iter()
                    .map(|r| r.id.clone())
                    .collect()
            },
            covered_intents: if full {
                snapshot.intents.iter().map(|i| i.id.clone()).collect()
            } else {
                vec![]
            },
        };
        manifest.validate()?;
        embedding::check(&cancelled)?;
        barrier(Barrier::BeforeManifest);
        write_new(
            &directory.join("manifest.json"),
            &canonical_bytes(&manifest)?,
        )?;
        // Files are individually flushed; parent-directory crash durability still
        // relies on the platform boundary exercised by native kill fixtures.
        self.open_components(&manifest)?;
        barrier(Barrier::AfterManifest);
        embedding::check(&cancelled)?;
        Ok(Prepared {
            manifest,
            previous: snapshot.previous,
            intents: snapshot.intents,
        })
    }
    fn open_components(
        &self,
        manifest: &Generation,
    ) -> Result<(Inventory, lexical::Reader, Option<vector::Component>)> {
        manifest.validate()?;
        if manifest.lexical_schema != lexical::SCHEMA_VERSION
            || manifest.tokenizer != tokenizer::TOKENIZER_VERSION
            || manifest.embedding_specification != Specification::qualified().digest()?
        {
            return Err(Error::Conflict("generation compatibility"));
        }
        let directory = self.directory(&manifest.id)?;
        let saved: Generation =
            serde_json::from_slice(&bounded(&directory.join("manifest.json"), 1024 * 1024)?)?;
        if &saved != manifest {
            return Err(Error::Conflict("generation manifest integrity"));
        }
        let bytes = bounded(&directory.join("inventory.json"), MAX_INVENTORY_BYTES)?;
        if digest_bytes(&bytes) != manifest.inventory_checksum {
            return Err(Error::Conflict("generation inventory integrity"));
        }
        let inventory: Inventory = serde_json::from_slice(&bytes)?;
        inventory.validate()?;
        if manifest.empty_complete != inventory.records.is_empty()
            || inventory.digest != manifest.inventory_digest
            || inventory.workspace != manifest.scope.workspace
            || inventory.watermark != manifest.canonical_watermark
            || inventory.authority != manifest.authority
            || inventory.deletion != manifest.deletion
        {
            return Err(Error::Conflict("generation snapshot mismatch"));
        }
        if digest_bytes(&bounded(
            &directory.join("lexical/vcp-lexical.json"),
            1024 * 1024,
        )?) != manifest.lexical_checksum
        {
            return Err(Error::Conflict("generation lexical integrity"));
        }
        let lexical = lexical::open(
            &directory.join("lexical"),
            &inventory,
            lexical::Limits::default(),
        )?;
        let vector = manifest
            .vector_checksum
            .as_ref()
            .map(|checksum| {
                vector::Component::open(
                    &directory.join("vectors.json"),
                    checksum,
                    &manifest.scope.workspace,
                    &Specification::qualified(),
                    &|| false,
                )
            })
            .transpose()?;
        if let Some(vector) = &vector {
            let expected = embedding::inventory_chunks(&inventory)?;
            if expected.len() != vector.rows().len()
                || expected
                    .iter()
                    .zip(vector.rows())
                    .any(|(a, b)| a.identity != b.identity)
            {
                return Err(Error::Conflict("generation vector coverage"));
            }
        } else if manifest.vector_deficits
            != inventory
                .records
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
        {
            return Err(Error::Conflict("generation vector deficit mismatch"));
        }
        Ok((inventory, lexical, vector))
    }
    pub async fn publish(
        &self,
        store: &mut Store,
        access: &Access,
        prepared: &Prepared,
        now: Timestamp,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Receipt> {
        access::authorize(store.state(), access, true)?;
        if access.tasks.is_some() || prepared.manifest.scope.workspace != access.workspace {
            return Err(Error::Access);
        }
        // A lost reply is resolved from canonical evidence, even when derived
        // files subsequently need rebuilding. Do not run the activation twice.
        if let Some(receipt) = store
            .state()
            .transactions
            .get(&prepared.manifest.transaction)
        {
            if generation(store.state(), &prepared.manifest.id, &access.workspace)?
                != prepared.manifest
            {
                return Err(Error::Conflict("publication retry identity"));
            }
            return Ok(receipt.clone());
        }
        // Existing qualification path remains valid; production performs this
        // expensive phase on a blocking CPU worker using validate_prepared().
        let validated = self.validate_prepared(prepared.clone(), &AtomicBool::new(false))?;
        self.activate(store, access, &validated, now, barrier).await
    }
    pub fn validate_prepared(&self, prepared: Prepared, cancel: &AtomicBool) -> Result<Validated> {
        embedding::check(&|| cancel.load(Ordering::Acquire))?;
        let mut pins = self
            .pins
            .lock()
            .map_err(|_| Error::Conflict("generation pin lock"))?;
        if pins.retired.contains(&prepared.manifest.id) {
            return Err(Error::Conflict("generation is retired"));
        }
        let files = freeze_files(&self.directory(&prepared.manifest.id)?)?;
        self.open_components(&prepared.manifest)?;
        embedding::check(&|| cancel.load(Ordering::Acquire))?;
        let id = prepared.manifest.id.clone();
        *pins.readers.entry(id.clone()).or_default() += 1;
        Ok(Validated {
            prepared,
            manager: self.pins.clone(),
            _files: files,
            _pin: Pin {
                id,
                pins: self.pins.clone(),
            },
        })
    }
    /// No component reads, hashing, parsing or native index work occurs here.
    pub async fn activate(
        &self,
        store: &mut Store,
        access: &Access,
        validated: &Validated,
        now: Timestamp,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Receipt> {
        if !Arc::ptr_eq(&self.pins, &validated.manager) {
            return Err(Error::Conflict("publication manager mismatch"));
        }
        self.activate_prepared(store, access, &validated.prepared, now, barrier)
            .await
    }
    async fn activate_prepared(
        &self,
        store: &mut Store,
        access: &Access,
        prepared: &Prepared,
        now: Timestamp,
        barrier: &dyn Fn(Barrier),
    ) -> Result<Receipt> {
        let workspace = access::authorize(store.state(), access, true)?;
        let manifest = &prepared.manifest;
        if access.tasks.is_some() || manifest.scope.workspace != access.workspace {
            return Err(Error::Access);
        }
        if let Some(receipt) = store.state().transactions.get(&manifest.transaction) {
            if generation(store.state(), &manifest.id, &access.workspace)? != *manifest {
                return Err(Error::Conflict("publication retry identity"));
            }
            return Ok(receipt.clone());
        }
        if workspace.authority != manifest.authority
            || workspace.deletion != manifest.deletion
            || active(store.state(), &access.workspace)? != prepared.previous
        {
            return Err(Error::Conflict("publication snapshot invalidated"));
        }
        let revision = prepared
            .previous
            .as_ref()
            .map(|a| a.revision.next())
            .transpose()?
            .unwrap_or(Revision::ZERO);
        let next = Active {
            document_type: ACTIVE.into(),
            schema_version: 1,
            id: access.workspace.clone(),
            workspace: access.workspace.clone(),
            revision,
            generation: manifest.id.clone(),
            transaction: manifest.transaction.clone(),
        };
        let mut manifest_record = Record::typed(
            Collection::Generation,
            manifest.id.as_str(),
            access.workspace.clone(),
            Revision::ZERO,
            manifest,
        )?;
        manifest_record.references.extend(
            manifest
                .covered_intents
                .iter()
                .map(|id| key(Collection::IndexIntent, id.as_str())),
        );
        let mut active_record = Record::typed(
            Collection::Generation,
            access.workspace.as_str(),
            access.workspace.clone(),
            revision,
            &next,
        )?;
        active_record
            .references
            .insert(key(Collection::Generation, manifest.id.as_str()));
        let mut mutations = vec![
            Mutation::Put {
                expected: None,
                record: manifest_record,
            },
            Mutation::Put {
                expected: prepared.previous.as_ref().map(|a| a.revision),
                record: active_record,
            },
        ];
        for captured in prepared
            .intents
            .iter()
            .filter(|intent| manifest.covered_intents.contains(&intent.id))
        {
            let mut current: IndexIntent = store
                .state()
                .record(
                    Collection::IndexIntent,
                    captured.id.as_str(),
                    &access.workspace,
                )?
                .decode()?;
            if &current != captured {
                return Err(Error::Conflict("publication intent changed"));
            }
            if current.status != IndexStatus::Ready {
                let expected = current.revision;
                current.revision = expected.next()?;
                current.status = IndexStatus::Ready;
                mutations.push(Mutation::Put {
                    expected: Some(expected),
                    record: Record::typed(
                        Collection::IndexIntent,
                        current.id.as_str(),
                        access.workspace.clone(),
                        current.revision,
                        &current,
                    )?,
                });
            }
        }
        let event = EventInput {
            id: EventId::new(),
            workspace: access.workspace.clone(),
            session: manifest.scope.session.clone(),
            task: Some(manifest.scope.task.clone()),
            actor: access.actor.clone(),
            correlation: CommandId::parse(manifest.transaction.as_str())?,
            causation: None,
            timestamp: now,
            kind: EventKind::Diagnostic,
            artifacts: vec![],
            data: serde_json::json!({"component":"search_publication","generation":manifest.id,"watermark":manifest.canonical_watermark}),
            metadata: None,
        };
        let transaction = Transaction {
            id: manifest.transaction.clone(),
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![event],
            command: None,
        };
        barrier(Barrier::BeforeActivation);
        let receipt = store.transact(transaction).await?;
        barrier(Barrier::AfterActivation);
        Ok(receipt)
    }
    /// Components are pinned while holding the same mutex used by retirement.
    /// Corrupt canonical records fail; corrupt derivatives may fall back visibly.
    pub fn recover(&self, store: &Store, access: &Access) -> Result<Recovery> {
        self.recover_with_check(store, access, &|| Ok(()))
    }
    pub fn recover_with_check(
        &self,
        store: &Store,
        access: &Access,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Recovery> {
        self.recover_state(store.state(), access, check)
    }
    /// A canonical snapshot keeps the authoritative cut and artifact/root pins
    /// alive while native component reopen runs outside the owner worker.
    pub fn recover_snapshot(
        &self,
        snapshot: &vcp_store::Snapshot,
        access: &Access,
    ) -> Result<Recovery> {
        self.recover_snapshot_with_check(snapshot, access, &|| Ok(()))
    }
    pub fn recover_snapshot_with_check(
        &self,
        snapshot: &vcp_store::Snapshot,
        access: &Access,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Recovery> {
        self.recover_state(snapshot.state(), access, check)
    }
    fn recover_state(
        &self,
        state: &State,
        access: &Access,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Recovery> {
        check()?;
        let workspace = access::authorize(state, access, false)?;
        if access.tasks.is_some() {
            return Err(Error::Access);
        }
        let mut pins = self
            .pins
            .lock()
            .map_err(|_| Error::Conflict("generation pin lock"))?;
        let mut next = active(state, &access.workspace)?.map(|active| active.generation);
        let mut failed = Vec::new();
        let mut visited = BTreeSet::new();
        for _ in 0..64 {
            check()?;
            let Some(id) = next else {
                break;
            };
            if !visited.insert(id.clone()) {
                return Err(Error::Conflict("canonical generation cycle"));
            }
            let manifest = generation(state, &id, &access.workspace)?;
            next = manifest.previous.clone();
            if manifest.authority != workspace.authority || manifest.deletion != workspace.deletion
            {
                failed.push(id);
                continue;
            }
            if !pins.retired.contains(&id) {
                if let Ok((inventory, lexical, vector)) = self.open_components(&manifest) {
                    check()?;
                    *pins.readers.entry(id.clone()).or_default() += 1;
                    let rebuild_required = !failed.is_empty()
                        || (!manifest.empty_complete && manifest.vector_checksum.is_none())
                        || !manifest.vector_deficits.is_empty();
                    return Ok(Recovery {
                        view: Some(View {
                            manifest,
                            inventory,
                            lexical,
                            vector,
                            _pin: Pin {
                                id,
                                pins: self.pins.clone(),
                            },
                        }),
                        canonical_head: state.watermark,
                        failed_components: failed,
                        rebuild_required,
                    });
                }
            }
            failed.push(id);
        }
        check()?;
        Ok(Recovery {
            view: None,
            canonical_head: state.watermark,
            failed_components: failed,
            rebuild_required: true,
        })
    }
    /// Explicit host retention policy is required; snapshot pins conservatively
    /// protect all generations until typed per-generation pinning is available.
    pub fn collect(
        &self,
        store: &Store,
        access: &Access,
        id: &GenerationId,
        policy: &GarbagePolicy,
    ) -> Result<bool> {
        let workspace = access::authorize(store.state(), access, true)?;
        let obsolete =
            generation(store.state(), id, &access.workspace)?.deletion < workspace.deletion;
        if access.tasks.is_some() {
            return Err(Error::Access);
        }
        let mut pins = self
            .pins
            .lock()
            .map_err(|_| Error::Conflict("generation pin lock"))?;
        if !policy.retention_idle
            || !policy.historical_deletion_allowed
            || policy.retained.contains(id)
            || pins.readers.get(id).copied().unwrap_or(0) > 0
            || active(store.state(), &access.workspace)?
                .is_some_and(|active| active.generation == *id && !obsolete)
            || store
                .state()
                .records
                .values()
                .any(|row| row.workspace == access.workspace && vcp_store::snapshot_pin_active(row))
        {
            return Ok(false);
        }
        generation(store.state(), id, &access.workspace)?;
        // Hold through deletion so snapshot acquisition and generation cleanup
        // cannot race, including snapshots with no retained artifacts.
        let Some(_snapshot_guard) = store.try_snapshot_cleanup_guard()? else {
            return Ok(false);
        };
        match fs::symlink_metadata(self.root.join(id.as_str())) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(io(error)),
            Ok(_) => (),
        }
        let path = self.directory(id)?;
        // Selection, pin acquisition and retirement cannot interleave in this owner.
        // Failed deletion remains retired and recover() reports an explicit deficit.
        pins.retired.insert(id.clone());
        match fs::remove_dir_all(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(io(error)),
        }
        Ok(true)
    }
}

pub struct Overlay {
    pub version: u32,
    pub base: Watermark,
    pub through: Watermark,
    pub records: Vec<crate::search_record::SearchRecord>,
    pub lexical_only: bool,
    pub unsatisfied: Option<&'static str>,
    pub bytes: usize,
}
/// Caller supplies a freshly authorized inventory. This bounded lexical overlay
/// never claims semantic freshness; query authorization still runs at handoff.
pub fn overlay(
    base: &Generation,
    fresh: &Inventory,
    max_records: usize,
    max_bytes: usize,
    max_time: Duration,
) -> Result<Overlay> {
    fresh.validate()?;
    if fresh.workspace != base.scope.workspace
        || fresh.watermark < base.canonical_watermark
        || max_records == 0
        || max_records > 128
        || max_bytes == 0
        || max_bytes > 256 * 1024
        || max_time.is_zero()
        || max_time > Duration::from_millis(50)
    {
        return Err(Error::Conflict("overlay boundary"));
    }
    let start = Instant::now();
    let mut records = Vec::new();
    let mut bytes = 0;
    let mut overflow = false;
    for (scanned, row) in fresh.records.iter().enumerate() {
        if scanned >= 1024 || start.elapsed() > max_time {
            overflow = true;
            break;
        }
        if row.watermark <= base.canonical_watermark {
            continue;
        }
        if records.len() >= max_records || bytes + row.text.len() > max_bytes {
            overflow = true;
            break;
        }
        bytes += row.text.len();
        records.push(row.clone());
    }
    Ok(Overlay {
        version: 1,
        base: base.canonical_watermark,
        through: if overflow {
            base.canonical_watermark
        } else {
            fresh.watermark
        },
        records,
        lexical_only: true,
        unsatisfied: Some(if overflow {
            "overlay limit exceeded"
        } else {
            "overlay has no vectors"
        }),
        bytes,
    })
}
