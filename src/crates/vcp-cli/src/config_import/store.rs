// SPDX-License-Identifier: Apache-2.0
//! Immutable, local import preferences. The user's profile is only ever read.
//! Publication is a create-only native rename of a flushed complete revision;
//! uncommitted staging files never select configuration after owner loss.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use vcp_extensions::import::normalize::Preferences;
use vcp_repository::{path::HeldPath, Root};

type Result<T> = std::result::Result<T, String>;
const MAX_BYTES: usize = 256 * 1024;
const MAX_REVISIONS: usize = 256;
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Codex,
    Gemini,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Provenance {
    Import {
        format: SourceFormat,
        source_version: String,
        source_sha256: String,
        source_path: String,
        allowed_root: String,
        mapping_version: u32,
    },
    Rollback {
        revision: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub revision: u64,
    pub revision_sha256: String,
    pub base_sha256: String,
    pub preferences: Preferences,
    pub provenance: Option<Provenance>,
    pub selected: BTreeSet<String>,
    pub rollback_from: Option<u64>,
}

impl Snapshot {
    pub fn empty(base_sha256: String) -> Self {
        Self {
            revision: 0,
            revision_sha256: ZERO.into(),
            base_sha256,
            preferences: Preferences::default(),
            provenance: None,
            selected: BTreeSet::new(),
            rollback_from: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    version: u32,
    ordinal: u64,
    previous: String,
    base_sha256: String,
    preferences: Preferences,
    provenance: Provenance,
    selected: BTreeSet<String>,
    rollback_from: Option<u64>,
}

struct Base {
    bytes: Vec<u8>,
    sha256: String,
    state_path: PathBuf,
    _pin: HeldPath,
}

/// Holds the original base file and its ancestors while publishing elsewhere.
/// Reopening after an external base edit is permitted for a refreshed preview;
/// callers must validate preferences against this currently pinned base.
pub struct Store {
    base: Base,
    root: Root,
    _directory: HeldPath,
    _owner: File,
    revisions: Vec<Snapshot>,
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn single_link(file: &File) -> Result<()> {
    let metadata = file
        .metadata()
        .map_err(|_| "import file metadata unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("regular import file required".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("redirected import file rejected".into());
        }
        let mut info = std::mem::MaybeUninit::zeroed();
        // SAFETY: the live owned handle and correctly sized native output remain valid.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err("import file identity unavailable".into());
        }
        if unsafe { info.assume_init() }.nNumberOfLinks != 1 {
            return Err("hard-linked import file rejected".into());
        }
    }
    Ok(())
}

fn base(profile: &Path, workspace: &Path) -> Result<Base> {
    if !profile.is_absolute() {
        return Err("explicit absolute profile path required".into());
    }
    let parent = profile.parent().ok_or("profile parent unavailable")?;
    let name = profile.file_name().ok_or("profile filename unavailable")?;
    // Pin original spelling before canonicalization can hide a junction.
    let root = crate::settings::registry_root(parent)?;
    let pin = root
        .hold(Some(Path::new(name)), false)
        .map_err(|_| "profile is unavailable or redirected")?;
    let canonical = crate::settings::local_path(profile, workspace)?;
    let file = File::open(&canonical).map_err(|_| "profile unavailable")?;
    single_link(&file)?;
    let bytes = root
        .read(Path::new(name), MAX_BYTES as u64)
        .map_err(|_| "profile read rejected")?
        .bytes;
    let mut state_name = name.to_os_string();
    state_name.push(".vcp-imports");
    let state_path = canonical.with_file_name(state_name);
    // The sibling may itself be a repository or synchronized directory even
    // when the profile's parent is legal. Check policy without replacing its
    // spelling: subsequent Root handles must still reject redirected paths.
    crate::settings::local_path(&state_path, workspace)?;
    Ok(Base {
        sha256: vcp_protocol::digest_bytes(&bytes),
        bytes,
        state_path,
        _pin: pin,
    })
}

fn owner(root: &Root, create: bool, exclusive: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(exclusive)
        .create(create)
        .truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1 | 2);
    }
    let file = options
        .open(root.path().join("owner.lock"))
        .map_err(|_| "import revision owner unavailable")?;
    single_link(&file)?;
    if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    }
    .map_err(|_| "configuration import is in use; retry after the current operation")?;
    Ok(file)
}

fn implicit(base: &Base) -> Snapshot {
    Snapshot::empty(base.sha256.clone())
}

fn validate(revision: &Revision) -> Result<()> {
    if revision.version != 1
        || !hash(&revision.previous)
        || !hash(&revision.base_sha256)
        || revision.ordinal == 0
        || revision.ordinal > MAX_REVISIONS as u64
    {
        return Err("invalid import revision metadata".into());
    }
    revision
        .preferences
        .validate()
        .map_err(|_| "invalid retained import preferences")?;
    if revision.selected.len() > 256
        || revision
            .selected
            .iter()
            .any(|v| v.is_empty() || v.len() > 512 || v.chars().any(char::is_control))
    {
        return Err("invalid retained import selection".into());
    }
    match &revision.provenance {
        Provenance::Import {
            source_version,
            source_sha256,
            source_path,
            allowed_root,
            mapping_version,
            ..
        } => {
            if source_version.is_empty()
                || source_version.len() > 64
                || !source_version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || !hash(source_sha256)
                || [source_path, allowed_root].iter().any(|path| {
                    path.is_empty() || path.len() > 32768 || path.chars().any(char::is_control)
                })
                || *mapping_version != 1
                || revision.rollback_from.is_some()
            {
                return Err("invalid import provenance".into());
            }
        }
        Provenance::Rollback { revision: from } => {
            if *from >= revision.ordinal || revision.rollback_from != Some(*from) {
                return Err("invalid rollback provenance".into());
            }
        }
    }
    Ok(())
}

fn filename(revision: u64) -> String {
    format!("revision-{revision:020}.json")
}

fn load(root: &Root, base: &Base) -> Result<Vec<Snapshot>> {
    let mut names = Vec::new();
    for (index, item) in fs::read_dir(root.path())
        .map_err(|_| "import history unavailable")?
        .enumerate()
    {
        if index >= MAX_REVISIONS * 3 + 8 {
            return Err("import history entry limit exceeded".into());
        }
        let item = item.map_err(|_| "import history entry unavailable")?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "invalid import history filename")?;
        if name == "owner.lock"
            || (name.starts_with("stage-") && name.ends_with(".partial"))
            || matches!(
                name.as_str(),
                "qualification-before_publish.ready" | "qualification-after_publish.ready"
            )
        {
            continue;
        }
        if name.len() != 34
            || !name.starts_with("revision-")
            || !name.ends_with(".json")
            || !name.as_bytes()[9..29].iter().all(u8::is_ascii_digit)
        {
            return Err("unexpected import history entry".into());
        }
        names.push(name);
        if names.len() > MAX_REVISIONS {
            return Err("import revision limit exceeded".into());
        }
    }
    names.sort();
    let mut snapshots = vec![implicit(base)];
    for (index, name) in names.iter().enumerate() {
        let ordinal = index as u64 + 1;
        if *name != filename(ordinal) {
            return Err("import revision chain has a gap".into());
        }
        let _pin = root
            .hold(Some(Path::new(name)), false)
            .map_err(|_| "import revision redirected")?;
        single_link(
            &File::open(root.path().join(name)).map_err(|_| "import revision unavailable")?,
        )?;
        let bytes = root
            .read(Path::new(name), MAX_BYTES as u64)
            .map_err(|_| "import revision read rejected")?
            .bytes;
        let revision: Revision =
            serde_json::from_slice(&bytes).map_err(|_| "invalid import revision")?;
        validate(&revision)?;
        if revision.ordinal != ordinal
            || revision.previous
                != snapshots
                    .last()
                    .ok_or("import chain unavailable")?
                    .revision_sha256
        {
            return Err("import revision chain mismatch".into());
        }
        snapshots.push(Snapshot {
            revision: ordinal,
            revision_sha256: vcp_protocol::digest_bytes(&bytes),
            base_sha256: revision.base_sha256,
            preferences: revision.preferences,
            provenance: Some(revision.provenance),
            selected: revision.selected,
            rollback_from: revision.rollback_from,
        });
    }
    Ok(snapshots)
}

/// Read without creating a state directory or lock. Base and selected history
/// are pinned together; callers can parse returned base bytes without a reread.
fn read_snapshots(profile: &Path, workspace: &Path) -> Result<(Vec<u8>, Vec<Snapshot>)> {
    let base = base(profile, workspace)?;
    match fs::symlink_metadata(&base.state_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let empty = implicit(&base);
            return Ok((base.bytes, vec![empty]));
        }
        Err(_) => return Err("import state unavailable".into()),
        Ok(_) => {}
    }
    let root = crate::settings::registry_root(&base.state_path)?;
    let _directory = root
        .hold(None, true)
        .map_err(|_| "import state redirected")?;
    if !root.path().join("owner.lock").exists() {
        // A process may die after creating the empty directory and before its
        // lock. There is no published configuration in that state.
        if fs::read_dir(root.path())
            .map_err(|_| "import state unavailable")?
            .next()
            .is_none()
        {
            let empty = implicit(&base);
            return Ok((base.bytes, vec![empty]));
        }
        return Err("import state owner missing; inspect before recovery".into());
    }
    let _owner = owner(&root, false, false)?;
    let snapshots = load(&root, &base)?;
    Ok((base.bytes, snapshots))
}

pub fn read(profile: &Path, workspace: &Path) -> Result<(Vec<u8>, Option<Snapshot>)> {
    let (bytes, snapshots) = read_snapshots(profile, workspace)?;
    Ok((bytes, snapshots.last().cloned().filter(|s| s.revision != 0)))
}

pub fn read_revision(profile: &Path, workspace: &Path, revision: u64) -> Result<Snapshot> {
    let (_, snapshots) = read_snapshots(profile, workspace)?;
    snapshots
        .get(usize::try_from(revision).map_err(|_| "invalid revision")?)
        .cloned()
        .ok_or("import revision unavailable".into())
}

pub fn load_current(profile: &Path, workspace: &Path) -> Result<Option<Snapshot>> {
    read(profile, workspace).map(|(_, snapshot)| snapshot)
}

impl Store {
    /// Only an explicit apply/rollback opens a writer and may create state.
    pub fn open(profile: &Path, workspace: &Path) -> Result<Self> {
        let base = base(profile, workspace)?;
        match fs::create_dir(&base.state_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("import state directory creation failed".into()),
        }
        let root = crate::settings::registry_root(&base.state_path)?;
        let directory = root
            .hold(None, true)
            .map_err(|_| "import state directory redirected")?;
        let owner = owner(&root, true, true)?;
        let revisions = load(&root, &base)?;
        Ok(Self {
            base,
            root,
            _directory: directory,
            _owner: owner,
            revisions,
        })
    }
    pub fn base_bytes(&self) -> &[u8] {
        &self.base.bytes
    }
    pub fn base_sha256(&self) -> &str {
        &self.base.sha256
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.revisions[self.revisions.len() - 1]
    }
    pub fn revision(&self, revision: u64) -> Result<Snapshot> {
        self.revisions
            .get(usize::try_from(revision).map_err(|_| "invalid revision")?)
            .cloned()
            .ok_or("import revision unavailable".into())
    }
    pub fn publish(
        &mut self,
        expected_revision: u64,
        expected_revision_sha256: &str,
        expected_base_sha256: &str,
        preferences: Preferences,
        provenance: Provenance,
        selected: BTreeSet<String>,
        rollback_from: Option<u64>,
    ) -> Result<Snapshot> {
        // Re-read history under the writer lock as well: noncooperative edits
        // to existing metadata must never be silently incorporated.
        let current = load(&self.root, &self.base)?;
        if current != self.revisions
            || self.snapshot().revision != expected_revision
            || self.snapshot().revision_sha256 != expected_revision_sha256
            || self.base.sha256 != expected_base_sha256
        {
            return Err("configuration changed; refresh the import preview".into());
        }
        let revision = Revision {
            version: 1,
            ordinal: expected_revision
                .checked_add(1)
                .ok_or("import revision overflow")?,
            previous: expected_revision_sha256.into(),
            base_sha256: self.base.sha256.clone(),
            preferences,
            provenance,
            selected,
            rollback_from,
        };
        validate(&revision)?;
        let bytes = vcp_protocol::canonical_bytes(&revision)
            .map_err(|_| "import revision encoding failed")?;
        if bytes.len() > MAX_BYTES {
            return Err("import revision byte limit exceeded".into());
        }
        let stage = self
            .root
            .path()
            .join(format!("stage-{}.partial", vcp_domain::CommandId::new()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            };
            options
                .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut file = options
            .open(&stage)
            .map_err(|_| "import staging creation failed")?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "import staging flush failed")?;
        barrier(&self.root, "before_publish")?;
        rename(&file, &self.root.path().join(filename(revision.ordinal)))?;
        file.sync_all()
            .map_err(|_| "import published; reopen to inspect its durable outcome")?;
        barrier(&self.root, "after_publish")?;
        drop(file);
        let snapshot = Snapshot {
            revision: revision.ordinal,
            revision_sha256: vcp_protocol::digest_bytes(&bytes),
            base_sha256: revision.base_sha256,
            preferences: revision.preferences,
            provenance: Some(revision.provenance),
            selected: revision.selected,
            rollback_from: revision.rollback_from,
        };
        self.revisions.push(snapshot.clone());
        Ok(snapshot)
    }
}

#[cfg(windows)]
fn rename(file: &File, target: &Path) -> Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
    };
    let name: Vec<u16> = target.as_os_str().encode_wide().collect();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let length = (offset + (name.len() + 1) * 2).max(std::mem::size_of::<FILE_RENAME_INFO>());
    let mut storage = vec![0usize; length.div_ceil(std::mem::size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: aligned, zero-initialized native buffer includes the terminating
    // UTF-16 unit. The source is our exclusive DELETE-capable staging handle;
    // the destination ancestry is pinned and replacement is explicitly false.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast(),
            name.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            length as u32,
        ) == 0
        {
            return Err(
                "import publication failed; inspect the original revision before retry".into(),
            );
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn rename(_file: &File, _target: &Path) -> Result<()> {
    Err("native Windows import publication required".into())
}

fn barrier(root: &Root, name: &str) -> Result<()> {
    #[cfg(feature = "qualification")]
    if std::env::var("VCP_IMPORT_BARRIER").ok().as_deref() == Some(name) {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.path().join(format!("qualification-{name}.ready")))
            .map_err(|_| "import qualification barrier unavailable")?;
        file.write_all(b"ready")
            .and_then(|()| file.sync_all())
            .map_err(|_| "import qualification barrier flush failed")?;
        loop {
            std::thread::park_timeout(std::time::Duration::from_millis(50));
        }
    }
    let _ = (root, name);
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use vcp_extensions::import::normalize::Restriction;

    struct Fixture {
        _temporary: tempfile::TempDir,
        profile: PathBuf,
        workspace: PathBuf,
    }

    fn fixture() -> Fixture {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let workspace = root.join("workspace");
        let private = root.join("private");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&private).unwrap();
        let profile = private.join("profile.json");
        fs::write(&profile, br#"{"token":"synthetic-never-store-secret"}"#).unwrap();
        Fixture {
            _temporary: temporary,
            profile,
            workspace,
        }
    }

    fn preferences() -> Preferences {
        let mut value = Preferences::default();
        value.servers.insert(
            "docs".into(),
            Restriction {
                allowed_tools: Some(BTreeSet::from(["read".into()])),
                timeout_ms: Some(1000),
            },
        );
        value
    }

    fn publish(store: &mut Store) -> Snapshot {
        let expected = store.snapshot().clone();
        let base = store.base_sha256().to_owned();
        store
            .publish(
                expected.revision,
                &expected.revision_sha256,
                &base,
                preferences(),
                Provenance::Import {
                    format: SourceFormat::Codex,
                    source_version: "test-v1".into(),
                    source_sha256: vcp_protocol::digest_bytes(b"synthetic input"),
                    source_path: "C:/selected/config.toml".into(),
                    allowed_root: "C:/selected".into(),
                    mapping_version: 1,
                },
                BTreeSet::from([
                    "servers.docs.allowed_tools".into(),
                    "servers.docs.timeout_ms".into(),
                ]),
                None,
            )
            .unwrap()
    }

    #[test]
    fn preview_is_read_only_and_publication_preserves_base_and_prior_revisions() {
        let fixture = fixture();
        let original = fs::read(&fixture.profile).unwrap();
        let state = fixture.profile.with_file_name("profile.json.vcp-imports");
        let (bytes, before) = read(&fixture.profile, &fixture.workspace).unwrap();
        assert_eq!(bytes, original);
        assert!(before.is_none());
        assert!(!state.exists());
        let empty = read_revision(&fixture.profile, &fixture.workspace, 0).unwrap();
        assert!(!state.exists());
        let mut store = Store::open(&fixture.profile, &fixture.workspace).unwrap();
        assert_eq!(store.snapshot(), &empty);
        let applied = publish(&mut store);
        let base = store.base_sha256().to_owned();
        let rollback = store
            .publish(
                applied.revision,
                &applied.revision_sha256,
                &base,
                Preferences::default(),
                Provenance::Rollback { revision: 0 },
                BTreeSet::new(),
                Some(0),
            )
            .unwrap();
        assert_eq!(rollback.revision, 2);
        drop(store);
        assert_eq!(
            read_revision(&fixture.profile, &fixture.workspace, 1).unwrap(),
            applied
        );
        assert_eq!(
            load_current(&fixture.profile, &fixture.workspace).unwrap(),
            Some(rollback)
        );
        assert_eq!(fs::read(&fixture.profile).unwrap(), original);
        for entry in fs::read_dir(state).unwrap() {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-never-store-secret"));
        }
    }

    #[test]
    fn first_directory_without_owner_selects_original_without_mutating_read() {
        let fixture = fixture();
        let state = fixture.profile.with_file_name("profile.json.vcp-imports");
        fs::create_dir(&state).unwrap();
        let (bytes, snapshot) = read(&fixture.profile, &fixture.workspace).unwrap();
        assert_eq!(bytes, fs::read(&fixture.profile).unwrap());
        assert!(snapshot.is_none());
        assert_eq!(fs::read_dir(&state).unwrap().count(), 0);
        assert_eq!(
            read_revision(&fixture.profile, &fixture.workspace, 0).unwrap(),
            Snapshot::empty(vcp_protocol::digest_bytes(&bytes))
        );
        // No legitimate staged write precedes creation of the owner lock.
        // Unexplained contents must not be mistaken for a fresh import state.
        fs::write(state.join("stage-unowned.partial"), b"unowned").unwrap();
        assert!(read(&fixture.profile, &fixture.workspace).is_err());
        assert!(!state.join("owner.lock").exists());
    }

    #[test]
    fn history_directory_inside_a_repository_is_rejected_without_mutation() {
        let fixture = fixture();
        let state = fixture.profile.with_file_name("profile.json.vcp-imports");
        fs::create_dir(&state).unwrap();
        fs::create_dir(state.join(".git")).unwrap();
        let original = fs::read(&fixture.profile).unwrap();
        assert!(read(&fixture.profile, &fixture.workspace).is_err());
        assert!(Store::open(&fixture.profile, &fixture.workspace).is_err());
        assert_eq!(fs::read_dir(&state).unwrap().count(), 1);
        assert!(!state.join("owner.lock").exists());
        assert_eq!(fs::read(&fixture.profile).unwrap(), original);
    }

    #[test]
    fn writer_and_profile_are_pinned_and_stale_compare_exchange_never_publishes() {
        let fixture = fixture();
        let mut store = Store::open(&fixture.profile, &fixture.workspace).unwrap();
        assert!(Store::open(&fixture.profile, &fixture.workspace).is_err());
        assert!(read(&fixture.profile, &fixture.workspace).is_err());
        assert!(fs::write(&fixture.profile, b"external changed bytes").is_err());
        let base = store.base_sha256().to_owned();
        let prior = store.snapshot().clone();
        for (revision, digest, base_hash) in [
            (1, ZERO, base.as_str()),
            (0, "wrong", base.as_str()),
            (0, ZERO, "wrong"),
        ] {
            assert!(store
                .publish(
                    revision,
                    digest,
                    base_hash,
                    Preferences::default(),
                    Provenance::Rollback { revision: 0 },
                    BTreeSet::new(),
                    Some(0)
                )
                .is_err());
            assert_eq!(store.snapshot(), &prior);
        }
        let applied = publish(&mut store);
        drop(store);
        fs::write(&fixture.profile, b"external changed bytes").unwrap();
        let (current, selected) = read(&fixture.profile, &fixture.workspace).unwrap();
        assert_eq!(selected.as_ref(), Some(&applied));
        assert_ne!(vcp_protocol::digest_bytes(&current), applied.base_sha256);
        let store = Store::open(&fixture.profile, &fixture.workspace).unwrap();
        assert_eq!(store.base_bytes(), current);
        assert_ne!(store.base_sha256(), store.snapshot().base_sha256);
    }

    #[test]
    fn hardlinks_and_broken_chains_fail_closed_and_staging_never_selects() {
        let fixture = fixture();
        let alias = fixture.workspace.join("profile-alias.json");
        fs::hard_link(&fixture.profile, &alias).unwrap();
        assert!(read(&fixture.profile, &fixture.workspace).is_err());
        fs::remove_file(alias).unwrap();
        let mut store = Store::open(&fixture.profile, &fixture.workspace).unwrap();
        publish(&mut store);
        let state = store.root.path().to_owned();
        drop(store);
        fs::write(
            state.join("stage-uncommitted.partial"),
            b"partial and untrusted",
        )
        .unwrap();
        assert_eq!(
            load_current(&fixture.profile, &fixture.workspace)
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        let committed = state.join(filename(1));
        let alias = fixture.workspace.join("revision-alias.json");
        fs::hard_link(&committed, &alias).unwrap();
        assert!(load_current(&fixture.profile, &fixture.workspace).is_err());
        fs::remove_file(alias).unwrap();
        fs::rename(&committed, state.join(filename(2))).unwrap();
        assert!(load_current(&fixture.profile, &fixture.workspace).is_err());
    }
}
