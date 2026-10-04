// SPDX-License-Identifier: Apache-2.0
//! User-local transport pacing, separate from canonical authority and accounting.
//! OS-held slot files survive independent owners and release on process death.
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use vcp_domain::{revision::Revision, RootId, WorkspaceId};
use vcp_repository::{path::HeldPath, Root, RootIdentity};

const ACTIVE: usize = 2;
const SPACING_MS: u64 = 250;
const POLL: Duration = Duration::from_millis(25);
const STATE_LIMIT: u64 = 1024;
pub const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Gate(Arc<Inner>);
struct Inner {
    root: Root,
    path: PathBuf,
    _pin: HeldPath,
}
pub struct Slot {
    gate: Gate,
    _lease: File,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    not_before_ms: u64,
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "provider pacing clock unavailable".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "provider pacing clock overflow".to_owned())
}

impl Gate {
    /// The caller supplies an existing trusted user-local directory, independent
    /// of the selected canonical workspace/data directory.
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let root = Root::open(
            RootIdentity {
                workspace: WorkspaceId::new(),
                root: RootId::new(),
                repository: "provider-pacing".into(),
                worktree: "provider-pacing".into(),
                binding: Revision::ZERO,
            },
            &path,
        )
        .map_err(|_| "provider pacing root unavailable or redirected")?;
        let pin = root
            .hold(None, true)
            .map_err(|_| "provider pacing root cannot be pinned")?;
        Ok(Self(Arc::new(Inner {
            root,
            path,
            _pin: pin,
        })))
    }

    fn open(&self, name: &str) -> Result<(File, bool), String> {
        let _pin = self
            .0
            .root
            .hold(None, true)
            .map_err(|_| "provider pacing root changed")?;
        let path = self.0.path.join(name);
        let options = |create_new| {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(create_new);
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1 | 2);
            options
        };
        let (file, created) = match options(true).open(&path) {
            Ok(file) => (file, true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                options(false)
                    .open(&path)
                    .map_err(|_| "provider pacing file unavailable")?,
                false,
            ),
            Err(_) => return Err("provider pacing file unavailable".into()),
        };
        let metadata = file
            .metadata()
            .map_err(|_| "provider pacing metadata unavailable")?;
        use std::os::windows::fs::MetadataExt;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.file_attributes() & 0x400 != 0
        {
            return Err("provider pacing file redirected".into());
        }
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle;
        let mut information = std::mem::MaybeUninit::zeroed();
        // SAFETY: this owned handle remains live and the native output has the
        // required layout. Reject aliases before either reading or writing.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) }
            == 0
        {
            return Err("provider pacing file identity unavailable".into());
        }
        if unsafe { information.assume_init() }.nNumberOfLinks != 1 {
            return Err("provider pacing hard-linked file rejected".into());
        }
        Ok((file, created))
    }

    fn try_lock(file: &File) -> Result<bool, String> {
        match file.try_lock() {
            Ok(()) => Ok(true),
            Err(TryLockError::WouldBlock) => Ok(false),
            Err(_) => Err("provider pacing lock unavailable".into()),
        }
    }

    fn read(file: &mut File, created: bool) -> Result<State, String> {
        let size = file
            .metadata()
            .map_err(|_| "provider pacing state unavailable")?
            .len();
        if created && size == 0 {
            return Ok(State {
                version: 1,
                not_before_ms: 0,
            });
        }
        if size == 0 || size > STATE_LIMIT {
            return Err("provider pacing state invalid; no request admitted".into());
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| "provider pacing state unreadable")?;
        let mut bytes = Vec::new();
        file.take(STATE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "provider pacing state unreadable")?;
        let state: State = serde_json::from_slice(&bytes)
            .map_err(|_| "provider pacing state invalid; no request admitted")?;
        if state.version != 1 {
            return Err("provider pacing state version unsupported".into());
        }
        Ok(state)
    }

    fn write(file: &mut File, state: &State) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(state).map_err(|_| "provider pacing state encoding failed")?;
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(&bytes))
            .and_then(|_| file.set_len(bytes.len() as u64))
            .and_then(|_| file.sync_data())
            .map_err(|_| "provider pacing state persistence failed".into())
    }

    /// Wait before canonical reservation/submission. No canonical store lock is
    /// held; cancellation and the original absolute deadline remain effective.
    pub async fn acquire<F>(&self, deadline: Instant, current: F) -> Result<Slot, String>
    where
        F: Fn() -> bool,
    {
        loop {
            if !current() {
                return Err("provider pacing cancelled by current owner".into());
            }
            if Instant::now() >= deadline {
                return Err("provider pacing deadline expired before submission".into());
            }
            let (transaction, _) = self.open("transaction.lock")?;
            if Self::try_lock(&transaction)? {
                let (mut state_file, created) = self.open("state.json")?;
                let mut state = Self::read(&mut state_file, created)?;
                let now = now_ms()?;
                if created {
                    Self::write(&mut state_file, &state)?;
                }
                if now >= state.not_before_ms {
                    for index in 0..ACTIVE {
                        let (lease, _) = self.open(&format!("slot-{index}.lock"))?;
                        if Self::try_lock(&lease)? {
                            // Check after filesystem work as well as before it.
                            if !current() || Instant::now() >= deadline {
                                return Err("provider pacing cancelled before submission".into());
                            }
                            state.not_before_ms = now
                                .checked_add(SPACING_MS)
                                .ok_or("provider pacing clock overflow")?;
                            Self::write(&mut state_file, &state)?;
                            return Ok(Slot {
                                gate: self.clone(),
                                _lease: lease,
                            });
                        }
                    }
                }
            }
            drop(transaction);
            tokio::time::sleep(POLL.min(deadline.saturating_duration_since(Instant::now()))).await;
        }
    }
}

impl Slot {
    /// Publish a shared minimum retry time before releasing this transport slot.
    /// The full qualified hint is retained; admission's own deadline bounds waits.
    pub fn cooldown(&self, delay: Duration) -> Result<(), String> {
        let delay = delay.max(RATE_LIMIT_COOLDOWN);
        // An otherwise qualified hint beyond the representable clock horizon
        // must still block other owners rather than discard the shared barrier.
        let delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
        let until = now_ms()?.saturating_add(delay_ms);
        let (transaction, _) = self.gate.open("transaction.lock")?;
        // Only a short state transaction; async waiters use try_lock and never
        // retain this lock while waiting for capacity or cooldown.
        let lock_deadline = Instant::now() + Duration::from_secs(1);
        while !Gate::try_lock(&transaction)? {
            if Instant::now() >= lock_deadline {
                return Err("provider pacing cooldown lock timed out".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let (mut file, created) = self.gate.open("state.json")?;
        let mut state = Gate::read(&mut file, created)?;
        state.not_before_ms = state.not_before_ms.max(until);
        Gate::write(&mut file, &state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gate(temp: &tempfile::TempDir) -> Gate {
        Gate::new(temp.path().to_path_buf()).unwrap()
    }

    #[tokio::test]
    async fn independent_gates_share_two_slots_and_queued_cancellation_does_not_acquire_capacity() {
        let temp = tempfile::tempdir().unwrap();
        let first = gate(&temp);
        let second = gate(&temp);
        let deadline = Instant::now() + Duration::from_secs(3);
        let one = first.acquire(deadline, || true).await.unwrap();
        let two = second.acquire(deadline, || true).await.unwrap();
        let began = Instant::now();
        let error = first
            .acquire(deadline, || began.elapsed() < Duration::from_millis(80))
            .await
            .err()
            .unwrap();
        assert!(error.contains("cancelled"));
        drop(one);
        let three = first.acquire(deadline, || true).await.unwrap();
        drop((two, three));
    }

    #[tokio::test]
    async fn four_independent_owners_complete_with_bounded_overlap() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let temp = tempfile::tempdir().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut owners = Vec::new();
        for _ in 0..4 {
            let gate = gate(&temp);
            let active = active.clone();
            let maximum = maximum.clone();
            owners.push(tokio::spawn(async move {
                let _slot = gate
                    .acquire(Instant::now() + Duration::from_secs(8), || true)
                    .await
                    .unwrap();
                let observed = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(observed, Ordering::SeqCst);
                assert!(observed <= 2, "more than two owners entered transport");
                tokio::time::sleep(Duration::from_millis(700)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for owner in owners {
            owner.await.unwrap();
        }
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(
            maximum.load(Ordering::SeqCst),
            2,
            "owners were unnecessarily serialized"
        );
    }

    #[tokio::test]
    async fn shared_cooldown_honors_long_hint_and_deadline_without_early_admission() {
        let temp = tempfile::tempdir().unwrap();
        let first = gate(&temp);
        let slot = first
            .acquire(Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        slot.cooldown(Duration::from_secs(12)).unwrap();
        slot.cooldown(Duration::from_secs(1)).unwrap();
        drop(slot);
        let bytes = std::fs::read(temp.path().join("state.json")).unwrap();
        let state: State = serde_json::from_slice(&bytes).unwrap();
        assert!(state.not_before_ms >= now_ms().unwrap() + 11_000);
        let error = gate(&temp)
            .acquire(Instant::now() + Duration::from_millis(80), || true)
            .await
            .err()
            .unwrap();
        assert!(error.contains("deadline"));
    }

    #[tokio::test]
    async fn corrupt_state_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("state.json"), b"{}").unwrap();
        assert!(gate(&temp)
            .acquire(Instant::now() + Duration::from_secs(1), || true)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn huge_qualified_hint_keeps_shared_gate_closed_without_overflow() {
        let temp = tempfile::tempdir().unwrap();
        let first = gate(&temp);
        let slot = first
            .acquire(Instant::now() + Duration::from_secs(1), || true)
            .await
            .unwrap();
        slot.cooldown(Duration::from_millis(u64::MAX - 1)).unwrap();
        drop(slot);
        let state: State =
            serde_json::from_slice(&std::fs::read(temp.path().join("state.json")).unwrap())
                .unwrap();
        assert_eq!(state.not_before_ms, u64::MAX);
        assert!(gate(&temp)
            .acquire(Instant::now() + Duration::from_millis(30), || true)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn hard_linked_state_fails_closed_without_changing_external_file() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("gate");
        std::fs::create_dir(&directory).unwrap();
        let external = temp.path().join("external.json");
        let original = b"{\"version\":1,\"not_before_ms\":0}";
        std::fs::write(&external, original).unwrap();
        std::fs::hard_link(&external, directory.join("state.json")).unwrap();
        let gate = Gate::new(directory).unwrap();
        assert!(gate
            .acquire(Instant::now() + Duration::from_secs(1), || true)
            .await
            .is_err());
        assert_eq!(std::fs::read(external).unwrap(), original);
    }

    #[test]
    fn file_as_root_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file");
        std::fs::write(&path, b"x").unwrap();
        assert!(Gate::new(path).is_err());
    }

    // Executed only by the subprocess regression below. OS leases, rather than
    // an in-process mutex, must release when an owner terminates unexpectedly.
    #[test]
    fn subprocess_lease_child() {
        let Some(path) = std::env::var_os("VCP_PROVIDER_PACING_TEST_ROOT") else {
            return;
        };
        let gate = Gate::new(PathBuf::from(&path)).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _slot = runtime
            .block_on(gate.acquire(Instant::now() + Duration::from_secs(10), || true))
            .unwrap();
        if let Ok(index) = std::env::var("VCP_PROVIDER_PACING_TEST_INDEX") {
            let root = PathBuf::from(path);
            std::fs::write(
                root.join(format!("{index}.start")),
                now_ms().unwrap().to_string(),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(700));
            std::fs::write(
                root.join(format!("{index}.end")),
                now_ms().unwrap().to_string(),
            )
            .unwrap();
            return;
        }
        std::fs::write(PathBuf::from(path).join("child-ready"), b"ready").unwrap();
        loop {
            std::thread::park();
        }
    }

    #[tokio::test]
    async fn independent_process_crash_releases_capacity() {
        struct Process(std::process::Child);
        impl Drop for Process {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut child = Process(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "foundation::provider_pacing::tests::subprocess_lease_child",
                    "--nocapture",
                ])
                .env("VCP_PROVIDER_PACING_TEST_ROOT", temp.path())
                .env_remove("VCP_PROVIDER_PACING_TEST_INDEX")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !temp.path().join("child-ready").exists() {
            assert!(
                Instant::now() < deadline,
                "child did not acquire its independent lease"
            );
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child terminated before lease acquisition"
            );
            tokio::time::sleep(POLL).await;
        }
        let gate = gate(&temp);
        let second = gate.acquire(deadline, || true).await.unwrap();
        assert!(gate
            .acquire(Instant::now() + Duration::from_millis(80), || true)
            .await
            .is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let replacement = gate.acquire(deadline, || true).await.unwrap();
        drop((second, replacement));
    }

    #[tokio::test]
    async fn four_processes_complete_with_two_overlapping_transport_slots() {
        struct Process(std::process::Child);
        impl Drop for Process {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut children = Vec::new();
        for index in 0..4 {
            children.push(Process(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "foundation::provider_pacing::tests::subprocess_lease_child",
                        "--nocapture",
                    ])
                    .env("VCP_PROVIDER_PACING_TEST_ROOT", temp.path())
                    .env("VCP_PROVIDER_PACING_TEST_INDEX", index.to_string())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let mut all_done = true;
            for child in &mut children {
                match child.0.try_wait().unwrap() {
                    Some(status) => assert!(status.success(), "independent owner failed"),
                    None => all_done = false,
                }
            }
            if all_done {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "independent owners did not complete"
            );
            tokio::time::sleep(POLL).await;
        }
        // Independent child clocks report their actual transport intervals.
        // End events sort before starts that have the same timestamp.
        let mut events = Vec::new();
        for index in 0..4 {
            let start: u64 = std::fs::read_to_string(temp.path().join(format!("{index}.start")))
                .unwrap()
                .parse()
                .unwrap();
            let end: u64 = std::fs::read_to_string(temp.path().join(format!("{index}.end")))
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                end >= start + 600,
                "child did not hold its transport interval"
            );
            events.push((start, 1_i32));
            events.push((end, -1_i32));
        }
        events.sort_unstable();
        let mut active = 0;
        let mut maximum = 0;
        for (_, change) in events {
            active += change;
            assert!(
                (0..=2).contains(&active),
                "cross-process transport bound violated"
            );
            maximum = maximum.max(active);
        }
        assert_eq!(active, 0);
        assert_eq!(
            maximum, 2,
            "independent processes were unnecessarily serialized"
        );
    }

    #[test]
    fn redirected_root_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        let redirected = temp.path().join("redirected");
        std::fs::create_dir(&destination).unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&redirected)
            .arg(&destination)
            .output()
            .unwrap();
        assert!(result.status.success(), "junction fixture creation failed");
        assert!(Gate::new(redirected).is_err());
    }
}
