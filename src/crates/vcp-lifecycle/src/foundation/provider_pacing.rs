// SPDX-License-Identifier: Apache-2.0
//! User-local transport pacing, separate from canonical authority and accounting.
//! OS-held slot files survive independent owners and release on process death.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use vcp_domain::{revision::Revision, RootId, WorkspaceId};
use vcp_models::routing::ModelEndpoint;
use vcp_repository::{path::HeldPath, Root, RootIdentity};

const ACTIVE: usize = 2;
const SPACING_MS: u64 = 250;
const POLL: Duration = Duration::from_millis(25);
const STATE_LIMIT: u64 = 1024;
const ROTATION_STATE_LIMIT: u64 = 512 * 1024;
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
    route: Option<ModelEndpoint>,
    _route_lease: Option<File>,
    sample: Option<u64>,
    pub(super) deadline: Instant,
    selection: Option<Selection>,
    admitted: std::sync::atomic::AtomicBool,
}
#[derive(Clone, Serialize)]
pub(super) struct Selection {
    pub policy_key: String,
    pub tier: usize,
    pub sequence: u64,
    pub selected: ModelEndpoint,
    pub estimated_tokens: u64,
    pub queued_ms: u64,
    pub decision_reason: String,
    pub recovery_probe: bool,
    pub remaining_deadline_ms: u64,
    pub local_token_target: u64,
    pub local_window_load: u64,
    pub pressure_kind: &'static str,
}
#[derive(Clone)]
pub(super) struct Routes {
    pub policy_key: String,
    pub sets: Vec<Vec<ModelEndpoint>>,
    /// Local pressure estimate, not a tokenizer guarantee or monetary bound.
    pub estimated_tokens: u64,
    pub waiter_id: String,
    pub queued_at_ms: u64,
    pub deadline: Option<Instant>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Health {
    until_ms: u64,
    failures: u32,
    target: u64,
    samples: Vec<Load>,
    #[serde(default)]
    last_used_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Load {
    id: u64,
    at_ms: u64,
    tokens: u64,
    #[serde(default)]
    usage: Option<vcp_domain::accounting::Usage>,
    #[serde(default)]
    queued_ms: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RotationState {
    version: u32,
    sequence: u64,
    account_until_ms: u64,
    next_start_ms: u64,
    positions: BTreeMap<String, u64>,
    routes: BTreeMap<String, Health>,
    #[serde(default)]
    position_updates: BTreeMap<String, u64>,
    #[serde(default)]
    waiters: BTreeMap<String, Waiter>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Waiter {
    queued_at_ms: u64,
    expires_ms: u64,
    reserve_route: String,
    #[serde(default)]
    estimated_tokens: u64,
}
fn route_key(route: &ModelEndpoint) -> String {
    let mut digest = Sha256::new();
    digest.update(route.model.as_bytes());
    digest.update([0]);
    digest.update(route.endpoint.as_bytes());
    format!("{:x}", digest.finalize())
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

    fn rotation_state(&self) -> Result<(File, RotationState), String> {
        let (mut file, created) = self.open("rotation-state.json")?;
        let size = file
            .metadata()
            .map_err(|_| "rotation state unavailable")?
            .len();
        if created && size == 0 {
            let state = RotationState {
                version: 1,
                ..Default::default()
            };
            Self::write_rotation(&mut file, &state)?;
            return Ok((file, state));
        }
        if size == 0 || size > ROTATION_STATE_LIMIT {
            return Err("rotation state exceeds bounds".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(ROTATION_STATE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "rotation state unreadable")?;
        let value =
            vcp_protocol::persisted_json::parse(&bytes).map_err(|_| "rotation state invalid")?;
        let state: RotationState =
            serde_json::from_value(value).map_err(|_| "rotation state invalid")?;
        if state.version != 1
            || state.routes.len() > 256
            || state.positions.len() > 512
            || state.position_updates.len() > 512
            || state.waiters.len() > 32
            || state.waiters.iter().any(|(key, waiter)| {
                key.len() > 128
                    || waiter.reserve_route.len() != 64
                    || !waiter
                        .reserve_route
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
            })
            || state.routes.iter().any(|(key, health)| {
                key.len() != 64
                    || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || health.samples.len() > 128
                    || health.failures > 16
                    || (health.target != 0 && !(32_000..=500_000).contains(&health.target))
                    || health
                        .samples
                        .iter()
                        .any(|sample| sample.id > state.sequence)
            })
            || state.positions.keys().any(|key| key.len() > 256)
        {
            return Err("rotation state version or bounds invalid".into());
        }
        Ok((file, state))
    }
    fn write_rotation(file: &mut File, state: &RotationState) -> Result<(), String> {
        if state.routes.len() > 256
            || state.positions.len() > 512
            || state.waiters.len() > 32
            || state.position_updates.len() > 512
        {
            return Err("rotation state capacity exhausted".into());
        }
        let bytes = serde_json::to_vec(state).map_err(|_| "rotation state encoding failed")?;
        if bytes.len() as u64 > ROTATION_STATE_LIMIT {
            return Err("rotation state exceeds bounds".into());
        }
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(&bytes))
            .and_then(|_| file.set_len(bytes.len() as u64))
            .and_then(|_| file.sync_data())
            .map_err(|_| "rotation state persistence failed".into())
    }
    fn clear_waiter(&self, id: &str) -> Result<(), String> {
        let (transaction, _) = self.open("transaction.lock")?;
        if Self::try_lock(&transaction)? {
            let (mut file, mut state) = self.rotation_state()?;
            state.waiters.remove(id);
            Self::write_rotation(&mut file, &state)?;
        }
        Ok(())
    }
    fn ready_waiter(&self, state: &RotationState, now: u64) -> Result<Option<String>, String> {
        let mut aged: Vec<_> = state
            .waiters
            .iter()
            .filter(|(_, waiter)| {
                now < waiter.expires_ms
                    && now.saturating_sub(waiter.queued_at_ms) >= 1_000
                    && state
                        .routes
                        .get(&waiter.reserve_route)
                        .is_none_or(|health| {
                            let target = if health.target == 0 {
                                500_000
                            } else {
                                health.target
                            };
                            let load = health
                                .samples
                                .iter()
                                .filter(|sample| now.saturating_sub(sample.at_ms) < 60_000)
                                .fold(0u64, |sum, sample| sum.saturating_add(sample.tokens));
                            now >= health.until_ms
                                && health.samples.len() < 128
                                && (load == 0
                                    || load.saturating_add(waiter.estimated_tokens) <= target)
                        })
            })
            .collect();
        aged.sort_by_key(|(id, waiter)| (waiter.queued_at_ms, *id));
        for (id, waiter) in aged {
            let (lease, _) = self.open(&format!("route-{}.lock", waiter.reserve_route))?;
            if Self::try_lock(&lease)? {
                return Ok(Some(id.clone()));
            }
        }
        Ok(None)
    }

    /// Select and hold ready capacity before canonical reservation. Recovery
    /// route leases permit one probe and release automatically on process death.
    pub(super) async fn acquire_routes<F>(
        &self,
        routes: &Routes,
        deadline: Instant,
        current: F,
    ) -> Result<Slot, String>
    where
        F: Fn() -> bool,
    {
        if routes.policy_key.len() > 128
            || routes.waiter_id.len() > 128
            || routes.sets.len() > 3
            || routes.sets.iter().map(Vec::len).sum::<usize>() > 96
            || routes.sets.iter().all(Vec::is_empty)
        {
            return Err("rotation has no eligible approved route".into());
        }
        let queued = Instant::now();
        loop {
            if !current() {
                let _ = self.clear_waiter(&routes.waiter_id);
                return Err("provider rotation cancelled by current owner".into());
            }
            if Instant::now() >= deadline {
                if routes
                    .deadline
                    .is_none_or(|original| Instant::now() >= original)
                {
                    let _ = self.clear_waiter(&routes.waiter_id);
                }
                return Err("provider rotation deadline expired before submission".into());
            }
            let (transaction, _) = self.open("transaction.lock")?;
            if Self::try_lock(&transaction)? {
                let (mut file, mut state) = self.rotation_state()?;
                let now = now_ms()?;
                // Remove expired local pressure observations.
                for health in state.routes.values_mut() {
                    health
                        .samples
                        .retain(|sample| now.saturating_sub(sample.at_ms) < 60_000);
                }
                let keys: Vec<_> = routes.sets.iter().flatten().map(route_key).collect();
                state.routes.retain(|key, health| {
                    keys.contains(key)
                        || !health.samples.is_empty()
                        || now < health.until_ms
                        || now.saturating_sub(health.last_used_ms) < 3_600_000
                });
                state.positions.retain(|key, _| {
                    key.starts_with(&routes.policy_key)
                        || now.saturating_sub(state.position_updates.get(key).copied().unwrap_or(0))
                            < 3_600_000
                });
                state
                    .position_updates
                    .retain(|key, _| state.positions.contains_key(key));
                state.waiters.retain(|_, waiter| now < waiter.expires_ms);
                let until = now.saturating_add(
                    routes
                        .deadline
                        .unwrap_or(deadline)
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(10_000) as u64,
                );
                let touch = state
                    .waiters
                    .get(&routes.waiter_id)
                    .is_none_or(|waiter| waiter.expires_ms < until.saturating_sub(5_000));
                if touch {
                    if !state.waiters.contains_key(&routes.waiter_id) && state.waiters.len() >= 32 {
                        return Err("rotation wait queue capacity exhausted".into());
                    }
                    state.waiters.insert(
                        routes.waiter_id.clone(),
                        Waiter {
                            queued_at_ms: routes.queued_at_ms,
                            expires_ms: until,
                            reserve_route: keys.first().cloned().ok_or("rotation routes empty")?,
                            estimated_tokens: routes.estimated_tokens,
                        },
                    );
                    Self::write_rotation(&mut file, &state)?;
                }
                let priority = self.ready_waiter(&state, now)?;
                if now >= state.account_until_ms
                    && now >= state.next_start_ms
                    && priority.is_none_or(|id| id == routes.waiter_id)
                {
                    for (tier, members) in routes.sets.iter().enumerate() {
                        let mut ready = Vec::new();
                        for member in members {
                            let key = route_key(member);
                            let reserved = state
                                .waiters
                                .iter()
                                .filter(|(_, waiter)| {
                                    waiter.reserve_route == key
                                        && now.saturating_sub(waiter.queued_at_ms) >= 1_000
                                })
                                .min_by_key(|(id, waiter)| (waiter.queued_at_ms, *id))
                                .is_some_and(|(id, _)| id != &routes.waiter_id);
                            let health = state.routes.entry(key).or_default();
                            let target = if health.target == 0 {
                                500_000
                            } else {
                                health.target
                            };
                            let load = health
                                .samples
                                .iter()
                                .fold(0u64, |sum, value| sum.saturating_add(value.tokens));
                            // An oversized request gets a turn when the window
                            // drains rather than becoming permanently ineligible.
                            if !reserved
                                && now >= health.until_ms
                                && health.samples.len() < 128
                                && (health.samples.is_empty()
                                    || load.saturating_add(routes.estimated_tokens) <= target)
                            {
                                ready.push(member.clone());
                            }
                        }
                        let position_key = format!("{}:{tier}", routes.policy_key);
                        let position = state.positions.get(&position_key).copied().unwrap_or(0);
                        while !ready.is_empty() {
                            let probe = vcp_models::rotation::next(&ready, position, 0)
                                .ok_or("rotation selection empty")?;
                            let endpoint_key = format!(
                                "{}:{tier}:{}",
                                routes.policy_key,
                                format!("{:x}", Sha256::digest(probe.model.as_bytes()))
                            );
                            let endpoint_position =
                                state.positions.get(&endpoint_key).copied().unwrap_or(0);
                            let selected =
                                vcp_models::rotation::next(&ready, position, endpoint_position)
                                    .ok_or("rotation selection empty")?;
                            let key = route_key(&selected);
                            let (route_lease, _) = self.open(&format!("route-{key}.lock"))?;
                            // Serialize transports on a route as well as probes.
                            if !Self::try_lock(&route_lease)? {
                                ready.retain(|member| member != &selected);
                                continue;
                            }
                            for index in 0..ACTIVE {
                                let (lease, _) = self.open(&format!("slot-{index}.lock"))?;
                                if !Self::try_lock(&lease)? {
                                    continue;
                                }
                                if !current() || Instant::now() >= deadline {
                                    return Err(
                                        "provider rotation cancelled before submission".into()
                                    );
                                }
                                if state.routes.len() > 256 || state.positions.len() + 2 > 512 {
                                    return Err("rotation shared state capacity exhausted".into());
                                }
                                state.sequence = state
                                    .sequence
                                    .checked_add(1)
                                    .ok_or("rotation sequence exhausted")?;
                                state.next_start_ms = now.saturating_add(SPACING_MS);
                                state
                                    .positions
                                    .insert(position_key.clone(), position.wrapping_add(1));
                                state.positions.insert(
                                    endpoint_key.clone(),
                                    endpoint_position.wrapping_add(1),
                                );
                                state.position_updates.insert(position_key.clone(), now);
                                state.position_updates.insert(endpoint_key, now);
                                state.waiters.remove(&routes.waiter_id);
                                let health = state
                                    .routes
                                    .get_mut(&key)
                                    .ok_or("rotation health missing")?;
                                let recovery_probe = health.failures > 0;
                                let local_token_target = if health.target == 0 {
                                    500_000
                                } else {
                                    health.target
                                };
                                let local_window_load =
                                    health.samples.iter().fold(0u64, |total, sample| {
                                        total.saturating_add(sample.tokens)
                                    });
                                health.last_used_ms = now;
                                state
                                    .routes
                                    .get_mut(&key)
                                    .ok_or("rotation health missing")?
                                    .samples
                                    .push(Load {
                                        id: state.sequence,
                                        at_ms: now,
                                        tokens: routes.estimated_tokens,
                                        usage: None,
                                        queued_ms: queued
                                            .elapsed()
                                            .as_millis()
                                            .min(u128::from(u64::MAX))
                                            as u64,
                                    });
                                Self::write_rotation(&mut file, &state)?;
                                let selection = Selection {
                                    policy_key: routes.policy_key.clone(),
                                    tier,
                                    sequence: state.sequence,
                                    selected: selected.clone(),
                                    estimated_tokens: routes.estimated_tokens,
                                    queued_ms: queued
                                        .elapsed()
                                        .as_millis()
                                        .min(u128::from(u64::MAX))
                                        as u64,
                                    decision_reason: if tier == 0 {
                                        "highest-priority set with ready capacity"
                                    } else {
                                        "earlier sets have no eligible ready capacity"
                                    }
                                    .into(),
                                    recovery_probe,
                                    remaining_deadline_ms: routes
                                        .deadline
                                        .unwrap_or(deadline)
                                        .saturating_duration_since(Instant::now())
                                        .as_millis()
                                        .min(u128::from(u64::MAX))
                                        as u64,
                                    local_token_target,
                                    local_window_load,
                                    pressure_kind: "local estimate; provider token limit unknown",
                                };
                                return Ok(Slot {
                                    gate: self.clone(),
                                    _lease: lease,
                                    route: Some(selected),
                                    _route_lease: Some(route_lease),
                                    sample: Some(state.sequence),
                                    deadline,
                                    selection: Some(selection),
                                    admitted: std::sync::atomic::AtomicBool::new(false),
                                });
                            }
                            break;
                        }
                        if !ready.is_empty() {
                            break;
                        } // Overall transport capacity full.
                    }
                }
            }
            drop(transaction);
            tokio::time::sleep(POLL.min(deadline.saturating_duration_since(Instant::now()))).await;
        }
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
                let (mut rotation_file, mut rotation) = self.rotation_state()?;
                let now = now_ms()?;
                if created {
                    Self::write(&mut state_file, &state)?;
                }
                if self.ready_waiter(&rotation, now)?.is_none()
                    && now >= state.not_before_ms
                    && now >= rotation.account_until_ms
                    && now >= rotation.next_start_ms
                {
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
                            rotation.next_start_ms = state.not_before_ms;
                            Self::write_rotation(&mut rotation_file, &rotation)?;
                            Self::write(&mut state_file, &state)?;
                            return Ok(Slot {
                                gate: self.clone(),
                                _lease: lease,
                                route: None,
                                _route_lease: None,
                                sample: None,
                                deadline,
                                selection: None,
                                admitted: std::sync::atomic::AtomicBool::new(false),
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
    pub(super) fn bind_failed_route(&mut self, route: ModelEndpoint) -> Result<(), String> {
        route
            .validate()
            .map_err(|_| "provider pacing admitted identity invalid")?;
        if self
            .route
            .as_ref()
            .is_some_and(|selected| selected != &route)
        {
            return Err("provider pacing route differs from exact admitted quote".into());
        }
        self.route = Some(route);
        self.update_rotation(|state, key, now| {
            state.routes.entry(key.to_owned()).or_default().last_used_ms = now;
        })
    }
    #[cfg(test)]
    pub(super) fn selected(&self) -> Option<&ModelEndpoint> {
        self.route.as_ref()
    }
    pub(super) fn selection(&self) -> Option<Selection> {
        self.selection.clone()
    }
    pub(super) fn queued_for(&mut self, duration: Duration) -> Result<(), String> {
        let queued_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        if let Some(selection) = &mut self.selection {
            selection.queued_ms = queued_ms;
        }
        self.update_rotation(|state, key, _| {
            if let Some(health) = state.routes.get_mut(key) {
                if let Some(sample) = health
                    .samples
                    .iter_mut()
                    .find(|sample| Some(sample.id) == self.sample)
                {
                    sample.queued_ms = queued_ms;
                }
            }
        })
    }
    pub(super) fn mark_admitted(&self) {
        self.admitted
            .store(true, std::sync::atomic::Ordering::Release);
    }
    fn update_rotation<F>(&self, update: F) -> Result<(), String>
    where
        F: FnOnce(&mut RotationState, &str, u64),
    {
        let (transaction, _) = self.gate.open("transaction.lock")?;
        let deadline = Instant::now() + Duration::from_secs(1);
        while !Gate::try_lock(&transaction)? {
            if Instant::now() >= deadline {
                return Err("rotation health update lock timed out".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let (mut file, mut state) = self.gate.rotation_state()?;
        let key = self.route.as_ref().map(route_key).unwrap_or_default();
        update(&mut state, &key, now_ms()?);
        Gate::write_rotation(&mut file, &state)
    }
    pub(super) fn failed(
        &self,
        failure: vcp_models::retry::Failure,
        source: Option<vcp_models::retry::LimitSource>,
        delay: Duration,
    ) -> Result<(), String> {
        self.mark_admitted();
        if self.selection.is_none() && failure == vcp_models::retry::Failure::RateLimit {
            // Fixed-provider requests retain their pacing contract and publish
            // the diagnosed scope to processes using rotation.
            self.cooldown(delay)?;
        }
        self.update_rotation(|state, key, now| match source {
            Some(vcp_models::retry::LimitSource::OpenrouterInFlightBudget) => {
                state.account_until_ms = state
                    .account_until_ms
                    .max(now.saturating_add(delay.as_millis().min(u128::from(u64::MAX)) as u64));
            }
            None if failure == vcp_models::retry::Failure::RateLimit => {
                state.account_until_ms = state
                    .account_until_ms
                    .max(now.saturating_add(delay.as_millis().min(u128::from(u64::MAX)) as u64));
            }
            Some(
                vcp_models::retry::LimitSource::OpenrouterKeyLimit
                | vcp_models::retry::LimitSource::OpenrouterCredits,
            ) => {}
            _ if matches!(
                failure,
                vcp_models::retry::Failure::RateLimit | vcp_models::retry::Failure::Transient
            ) =>
            {
                if let Some(health) = state.routes.get_mut(key) {
                    health.failures = health.failures.saturating_add(1).min(16);
                    let minimum =
                        5_000u64.saturating_mul(1 << health.failures.saturating_sub(1).min(4));
                    health.until_ms = health.until_ms.max(now.saturating_add(
                        minimum.max(delay.as_millis().min(u128::from(u64::MAX)) as u64),
                    ));
                    health.target = (if health.target == 0 {
                        500_000
                    } else {
                        health.target
                    } / 2)
                        .max(32_000);
                }
            }
            _ => {}
        })
    }
    pub(super) fn succeeded(
        &self,
        usage: Option<&vcp_domain::accounting::Usage>,
    ) -> Result<(), String> {
        self.mark_admitted();
        self.update_rotation(|state, key, _| {
            if let Some(health) = state.routes.get_mut(key) {
                health.until_ms = 0;
                health.failures = 0;
                if let (Some(id), Some(usage)) = (self.sample, usage) {
                    if let Some(sample) = health.samples.iter_mut().find(|sample| sample.id == id) {
                        sample.tokens = usage.input.get().saturating_add(usage.output.get());
                        sample.usage = Some(usage.clone());
                    }
                }
                health.target = (if health.target == 0 {
                    500_000
                } else {
                    health.target
                })
                .saturating_add(8_000)
                .min(500_000);
            }
        })
    }
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
impl Drop for Slot {
    fn drop(&mut self) {
        if !self.admitted.load(std::sync::atomic::Ordering::Acquire) {
            // Failed admission and queued cancellation consume no provider load.
            // A failed cleanup conservatively expires through the bounded window.
            let _ = self.update_rotation(|state, key, _| {
                if let Some(health) = state.routes.get_mut(key) {
                    health
                        .samples
                        .retain(|sample| Some(sample.id) != self.sample);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gate(temp: &tempfile::TempDir) -> Gate {
        Gate::new(temp.path().to_path_buf()).unwrap()
    }
    fn routes(sets: &[&[(&str, &str)]], load: u64) -> Routes {
        Routes {
            policy_key: "test-pool".into(),
            estimated_tokens: load,
            waiter_id: WorkspaceId::new().to_string(),
            queued_at_ms: now_ms().unwrap(),
            deadline: None,
            sets: sets
                .iter()
                .map(|set| {
                    set.iter()
                        .map(|(model, endpoint)| ModelEndpoint {
                            model: (*model).into(),
                            endpoint: (*endpoint).into(),
                        })
                        .collect()
                })
                .collect(),
        }
    }
    #[tokio::test]
    async fn independent_process_state_rotates_models_before_endpoints() {
        let temp = tempfile::tempdir().unwrap();
        let one = gate(&temp);
        let two = gate(&temp);
        let request = routes(&[&[("a", "one"), ("a", "two"), ("b", "one")]], 1);
        let mut observed = Vec::new();
        for gate in [&one, &two, &one, &two] {
            let slot = gate
                .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
                .await
                .unwrap();
            observed.push(slot.selected().unwrap().clone());
            slot.succeeded(None).unwrap();
        }
        // Expected model order a,b,a,b; endpoint a alternates one,two.
        assert_eq!(
            observed
                .iter()
                .map(|value| value.model.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "a", "b"]
        );
        assert_eq!(observed[2].endpoint, "two");
    }
    #[tokio::test]
    async fn scoped_failure_uses_ready_tier_and_next_step_returns_to_primary() {
        let temp = tempfile::tempdir().unwrap();
        let one = gate(&temp);
        let two = gate(&temp);
        let request = routes(&[&[("a", "one")], &[("b", "one")]], 1);
        let first = one
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        first
            .failed(
                vcp_models::retry::Failure::RateLimit,
                Some(vcp_models::retry::LimitSource::UpstreamProviderSharedPool),
                Duration::from_secs(90),
            )
            .unwrap();
        drop(first);
        let second = two
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(second.selected().unwrap().model, "b");
        second.succeeded(None).unwrap();
        drop(second);
        let (mut file, mut state) = one.rotation_state().unwrap();
        state
            .routes
            .get_mut(&route_key(&request.sets[0][0]))
            .unwrap()
            .until_ms = 0;
        Gate::write_rotation(&mut file, &state).unwrap();
        let probe = one
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(probe.selected().unwrap().model, "a");
        // A second independent owner cannot own the same recovery probe.
        let single = routes(&[&[("a", "one")]], 1);
        assert!(two
            .acquire_routes(&single, Instant::now() + Duration::from_millis(300), || {
                true
            })
            .await
            .is_err());
        probe.succeeded(None).unwrap();
    }
    #[tokio::test]
    async fn token_pressure_skips_loaded_route_and_actual_usage_releases_headroom() {
        let temp = tempfile::tempdir().unwrap();
        let one = gate(&temp);
        let request = routes(&[&[("a", "one"), ("b", "one")]], 600_000);
        let first = one
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(first.selected().unwrap().model, "a");
        first
            .succeeded(Some(&vcp_domain::accounting::Usage {
                input: vcp_domain::Units::new(10),
                output: vcp_domain::Units::new(5),
                ..Default::default()
            }))
            .unwrap();
        drop(first);
        let second = one
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(second.selected().unwrap().model, "b");
        second.mark_admitted();
        drop(second);
        let tiny = routes(&[&[("a", "one"), ("b", "one")]], 20);
        let third = one
            .acquire_routes(&tiny, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(third.selected().unwrap().model, "a");
    }
    #[tokio::test]
    async fn account_limit_and_unknown_rate_limit_do_not_rotate_around_account_wait() {
        for source in [
            None,
            Some(vcp_models::retry::LimitSource::OpenrouterInFlightBudget),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let one = gate(&temp);
            let request = routes(&[&[("a", "one"), ("b", "one")]], 1);
            let first = one
                .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
                .await
                .unwrap();
            first
                .failed(
                    vcp_models::retry::Failure::RateLimit,
                    source,
                    Duration::from_secs(5),
                )
                .unwrap();
            drop(first);
            assert!(one
                .acquire_routes(
                    &request,
                    Instant::now() + Duration::from_millis(100),
                    || true
                )
                .await
                .is_err());
        }
    }
    #[tokio::test]
    async fn aged_large_waiter_keeps_priority_across_sweeps_and_idle_state_is_retired() {
        let temp = tempfile::tempdir().unwrap();
        let gate = gate(&temp);
        let mut large = routes(&[&[("a", "one")]], 400_000);
        large.queued_at_ms = now_ms().unwrap().saturating_sub(2_000);
        large.deadline = Some(Instant::now() + Duration::from_secs(5));
        let (mut file, mut state) = gate.rotation_state().unwrap();
        state.sequence = 1;
        state.routes.insert(
            route_key(&large.sets[0][0]),
            Health {
                samples: vec![Load {
                    id: 1,
                    at_ms: now_ms().unwrap(),
                    tokens: 400_000,
                    usage: None,
                    queued_ms: 0,
                }],
                ..Default::default()
            },
        );
        // Previous preference pools are derived scheduling state, not authority.
        for index in 0..256 {
            state.positions.insert(format!("old-pool-{index}"), 5);
            state
                .position_updates
                .insert(format!("old-pool-{index}"), 0);
        }
        Gate::write_rotation(&mut file, &state).unwrap();
        assert!(gate
            .acquire_routes(&large, Instant::now() + Duration::from_millis(80), || true)
            .await
            .is_err());
        let (_, state) = gate.rotation_state().unwrap();
        assert!(state.waiters.contains_key(&large.waiter_id));
        assert!(state.positions.is_empty());
        let tiny = routes(&[&[("a", "one")]], 1);
        assert!(
            gate.acquire_routes(&tiny, Instant::now() + Duration::from_millis(80), || true)
                .await
                .is_err(),
            "young small traffic must not continually refill an aged waiter's reserved route"
        );
        let (mut file, mut state) = gate.rotation_state().unwrap();
        state
            .routes
            .get_mut(&route_key(&large.sets[0][0]))
            .unwrap()
            .samples[0]
            .at_ms = now_ms().unwrap().saturating_sub(60_000);
        Gate::write_rotation(&mut file, &state).unwrap();
        let slot = gate
            .acquire_routes(&large, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(slot.selection().unwrap().local_window_load, 0);
        assert_eq!(slot.selected().unwrap(), &large.sets[0][0]);
    }
    #[tokio::test]
    async fn fixed_owner_failure_cools_exact_rotating_route_without_blocking_other_models() {
        let temp = tempfile::tempdir().unwrap();
        let fixed = gate(&temp);
        let rotated = gate(&temp);
        let mut slot = fixed
            .acquire(Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        let failed = ModelEndpoint {
            model: "fixture/a".into(),
            endpoint: "fixture/one".into(),
        };
        slot.bind_failed_route(failed.clone()).unwrap();
        slot.failed(
            vcp_models::retry::Failure::RateLimit,
            Some(vcp_models::retry::LimitSource::UpstreamProviderSharedPool),
            Duration::from_secs(30),
        )
        .unwrap();
        drop(slot);
        let request = routes(
            &[&[("fixture/a", "fixture/one"), ("fixture/b", "fixture/one")]],
            1,
        );
        let healthy = rotated
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(healthy.selected().unwrap().model, "fixture/b");
        let (_, state) = rotated.rotation_state().unwrap();
        assert!(
            state.routes[&route_key(&failed)].until_ms >= now_ms().unwrap().saturating_add(28_000)
        );
        assert_eq!(state.account_until_ms, 0);
    }
    #[tokio::test]
    async fn ready_aged_waiter_gets_global_capacity_before_younger_other_route_traffic() {
        let temp = tempfile::tempdir().unwrap();
        let gate = gate(&temp);
        let mut old = routes(&[&[("a", "one")]], 1);
        old.queued_at_ms = now_ms().unwrap().saturating_sub(2_000);
        old.deadline = Some(Instant::now() + Duration::from_secs(5));
        let (mut file, mut state) = gate.rotation_state().unwrap();
        state.waiters.insert(
            old.waiter_id.clone(),
            Waiter {
                queued_at_ms: old.queued_at_ms,
                expires_ms: now_ms().unwrap() + 5_000,
                reserve_route: route_key(&old.sets[0][0]),
                estimated_tokens: 1,
            },
        );
        Gate::write_rotation(&mut file, &state).unwrap();
        let young = routes(&[&[("b", "one")]], 1);
        assert!(gate
            .acquire_routes(&young, Instant::now() + Duration::from_millis(80), || true)
            .await
            .is_err());
        assert!(
            gate.acquire(Instant::now() + Duration::from_millis(80), || true)
                .await
                .is_err(),
            "fixed-provider traffic also yields a free global slot to an aged ready waiter"
        );
        let oldest = gate
            .acquire_routes(&old, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        let following = gate
            .acquire_routes(&young, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert_eq!(oldest.selected().unwrap().model, "a");
        assert_eq!(following.selected().unwrap().model, "b");
        drop((oldest, following));
    }
    struct RotationProcess(std::process::Child);
    impl Drop for RotationProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn rotation_child(root: &std::path::Path, index: usize, hold: bool) -> RotationProcess {
        RotationProcess(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "foundation::provider_pacing::tests::subprocess_rotation_child",
                    "--nocapture",
                ])
                .env("VCP_PROVIDER_ROTATION_TEST_ROOT", root)
                .env("VCP_PROVIDER_ROTATION_TEST_INDEX", index.to_string())
                .env(
                    "VCP_PROVIDER_ROTATION_TEST_HOLD",
                    if hold { "yes" } else { "no" },
                )
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    #[test]
    fn subprocess_rotation_child() {
        let Some(path) = std::env::var_os("VCP_PROVIDER_ROTATION_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(path);
        let gate = Gate::new(root.clone()).unwrap();
        let index = std::env::var("VCP_PROVIDER_ROTATION_TEST_INDEX")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let hold = std::env::var("VCP_PROVIDER_ROTATION_TEST_HOLD").unwrap() == "yes";
        let request = if hold {
            routes(&[&[("a", "one")]], 1)
        } else {
            routes(&[&[("a", "one"), ("a", "two"), ("b", "one")]], 1)
        };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let slot = runtime
            .block_on(gate.acquire_routes(
                &request,
                Instant::now() + Duration::from_secs(10),
                || true,
            ))
            .unwrap();
        slot.mark_admitted();
        std::fs::write(
            root.join(format!("rotation-{index}.json")),
            serde_json::to_vec(&slot.selection()).unwrap(),
        )
        .unwrap();
        if hold {
            loop {
                std::thread::park();
            }
        }
        slot.succeeded(None).unwrap();
    }
    #[tokio::test]
    async fn actual_processes_share_rotation_and_crashed_recovery_probe_releases_lease() {
        let temp = tempfile::tempdir().unwrap();
        let mut selected = Vec::new();
        for index in 0..4 {
            let mut child = rotation_child(temp.path(), index, false);
            let deadline = Instant::now() + Duration::from_secs(15);
            while child.0.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline, "rotation child did not finish");
                tokio::time::sleep(POLL).await;
            }
            let value: serde_json::Value = serde_json::from_slice(
                &std::fs::read(temp.path().join(format!("rotation-{index}.json"))).unwrap(),
            )
            .unwrap();
            selected.push((
                value["selected"]["model"].as_str().unwrap().to_owned(),
                value["selected"]["endpoint"].as_str().unwrap().to_owned(),
            ));
        }
        assert_eq!(
            selected,
            [
                ("a".into(), "one".into()),
                ("b".into(), "one".into()),
                ("a".into(), "two".into()),
                ("b".into(), "one".into())
            ]
        );
        let gate = gate(&temp);
        let request = routes(&[&[("a", "one")]], 1);
        let first = gate
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        first
            .failed(
                vcp_models::retry::Failure::RateLimit,
                Some(vcp_models::retry::LimitSource::UpstreamProviderSharedPool),
                Duration::from_secs(5),
            )
            .unwrap();
        drop(first);
        let (mut file, mut state) = gate.rotation_state().unwrap();
        state
            .routes
            .get_mut(&route_key(&request.sets[0][0]))
            .unwrap()
            .until_ms = 0;
        Gate::write_rotation(&mut file, &state).unwrap();
        let mut child = rotation_child(temp.path(), 4, true);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !temp.path().join("rotation-4.json").exists() {
            assert!(Instant::now() < deadline && child.0.try_wait().unwrap().is_none());
            tokio::time::sleep(POLL).await;
        }
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(temp.path().join("rotation-4.json")).unwrap())
                .unwrap();
        assert_eq!(value["recovery_probe"], true);
        assert!(gate
            .acquire_routes(
                &request,
                Instant::now() + Duration::from_millis(100),
                || true
            )
            .await
            .is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let recovered = gate
            .acquire_routes(&request, Instant::now() + Duration::from_secs(2), || true)
            .await
            .unwrap();
        assert!(recovered.selection().unwrap().recovery_probe);
        recovered.succeeded(None).unwrap();
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
