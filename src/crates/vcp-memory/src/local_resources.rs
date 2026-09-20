// SPDX-License-Identifier: Apache-2.0
//! Host admission for qualified local work, not an operating-system RAM quota.
//! Keep a Permit for the lifetime of its loaded model and private build buffers.
//! Check Controls before every bounded unit; yielding requires returning to the
//! host scheduler, not spinning or merely calling thread::yield_now().
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub declared_ram_bytes: u64,
    pub temporary_disk_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            declared_ram_bytes: 1024 * 1024 * 1024,
            temporary_disk_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Workload {
    pub rows: usize,
    pub source_bytes: usize,
    pub batch: usize,
    pub load_model: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub ram_bytes: u64,
    pub temporary_disk_bytes: u64,
}
impl Workload {
    /// Conservative declared reservation, versioned for the pinned CPU MiniLM
    /// and full-f32 DiskANN provider. Qualification measurements remain separate.
    pub fn estimate(self) -> Result<Estimate> {
        if self.rows == 0
            || self.rows > 1024
            || self.source_bytes > 192 * 1024
            || self.batch == 0
            || self.batch > 16
        {
            return Err("local workload exceeds qualified rows/source/batch limits".into());
        }
        let vectors = self.rows as u64 * 384 * 4;
        Ok(Estimate {
            // Model weights, activations and allocator margin are declared, not
            // asserted as an observed upper bound on the process working set.
            ram_bytes: if self.load_model {
                768 * 1024 * 1024
            } else {
                64 * 1024 * 1024
            } + vectors * 8
                + self.source_bytes as u64 * 4,
            temporary_disk_bytes: 1024 * 1024 + vectors * 8 + self.source_bytes as u64 * 4,
        })
    }
}
pub const ESTIMATE_VERSION: &str = "minilm-cpu384-diskann1024/1";

struct State {
    active: bool,
    limits: Limits,
}
/// Share one pool across owners in the process. No hidden wait queue: a failed
/// admission is visible deferred work, to be retried by canonical maintenance.
#[derive(Clone)]
pub struct Admission {
    state: Arc<Mutex<State>>,
}
impl Admission {
    pub fn new(limits: Limits) -> Result<Self> {
        if limits.declared_ram_bytes == 0 || limits.temporary_disk_bytes == 0 {
            return Err("local admission limits must be nonzero".into());
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                active: false,
                limits,
            })),
        })
    }
    pub fn acquire(&self, workload: Workload) -> Result<Permit> {
        let estimate = workload.estimate()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "local admission lock poisoned")?;
        if state.active {
            return Err("local maintenance capacity occupied".into());
        }
        if estimate.ram_bytes > state.limits.declared_ram_bytes
            || estimate.temporary_disk_bytes > state.limits.temporary_disk_bytes
        {
            return Err("local declared resource reservation exceeds admission limit".into());
        }
        state.active = true;
        Ok(Permit {
            state: self.state.clone(),
            estimate,
        })
    }
}
pub struct Permit {
    state: Arc<Mutex<State>>,
    estimate: Estimate,
}
impl Permit {
    pub fn estimate(&self) -> Estimate {
        self.estimate
    }
    /// A sampled overrun stops subsequent work. This is not preemptive allocation
    /// enforcement; intermediate native allocations may exceed the estimate.
    pub fn check_temporary_disk(&self, bytes: u64) -> Result<()> {
        if bytes > self.estimate.temporary_disk_bytes {
            return Err("local temporary disk reservation exceeded".into());
        }
        Ok(())
    }
    /// Check a known serialized component length before opening/writing it.
    /// The host must serialize writes within this permit and sample the owned
    /// directory again after a completed write or recovered interrupted build.
    pub fn check_temporary_write(&self, existing: u64, additional: u64) -> Result<()> {
        self.check_temporary_disk(
            existing
                .checked_add(additional)
                .ok_or("local temporary disk overflow")?,
        )
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Continue,
    Yield,
    Cancelled,
    Deadline,
}
pub struct Controls {
    started: Instant,
    slice_started: Instant,
    total: Duration,
    slice: Duration,
}
impl Controls {
    pub fn new(total: Duration, slice: Duration) -> Result<Self> {
        if total.is_zero()
            || total > Duration::from_secs(600)
            || slice.is_zero()
            || slice > Duration::from_millis(100)
            || slice > total
        {
            return Err("local work duration bounds".into());
        }
        let now = Instant::now();
        Ok(Self {
            started: now,
            slice_started: now,
            total,
            slice,
        })
    }
    pub fn checkpoint(&self, cancelled: bool, interactive_waiting: bool) -> Decision {
        if cancelled {
            Decision::Cancelled
        } else if self.started.elapsed() >= self.total {
            Decision::Deadline
        } else if interactive_waiting || self.slice_started.elapsed() >= self.slice {
            Decision::Yield
        } else {
            Decision::Continue
        }
    }
    /// Only call after the host has actually yielded and readmitted this work.
    pub fn resumed(&mut self) {
        self.slice_started = Instant::now();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub private_commit_bytes: u64,
    pub peak_private_commit_bytes: u64,
    /// Committed mapped address extent, not the number of resident mapped pages.
    pub committed_mapped_address_bytes: u64,
    pub committed_image_address_bytes: u64,
}
#[derive(Debug)]
pub struct Measurements {
    pub samples: u64,
    pub maximum_gap: Duration,
    pub sampled_build_peak_resident_bytes: u64,
    pub sampled_build_peak_private_commit_bytes: u64,
    pub sampled_build_peak_mapped_address_bytes: u64,
    pub sampled_build_peak_temporary_disk_bytes: u64,
    /// OS lifetime high-water marks must not be mislabeled as this build's peak.
    pub process_lifetime_peak_resident_bytes: u64,
    pub process_lifetime_peak_private_commit_bytes: u64,
    last: Instant,
}
impl Measurements {
    pub fn start() -> Self {
        Self {
            samples: 0,
            maximum_gap: Duration::ZERO,
            sampled_build_peak_resident_bytes: 0,
            sampled_build_peak_private_commit_bytes: 0,
            sampled_build_peak_mapped_address_bytes: 0,
            sampled_build_peak_temporary_disk_bytes: 0,
            process_lifetime_peak_resident_bytes: 0,
            process_lifetime_peak_private_commit_bytes: 0,
            last: Instant::now(),
        }
    }
    pub fn observe(&mut self, row: Snapshot, temporary_disk_bytes: u64) {
        self.maximum_gap = self.maximum_gap.max(self.last.elapsed());
        self.last = Instant::now();
        self.samples += 1;
        self.sampled_build_peak_resident_bytes = self
            .sampled_build_peak_resident_bytes
            .max(row.resident_bytes);
        self.sampled_build_peak_private_commit_bytes = self
            .sampled_build_peak_private_commit_bytes
            .max(row.private_commit_bytes);
        self.sampled_build_peak_mapped_address_bytes = self
            .sampled_build_peak_mapped_address_bytes
            .max(row.committed_mapped_address_bytes);
        self.sampled_build_peak_temporary_disk_bytes = self
            .sampled_build_peak_temporary_disk_bytes
            .max(temporary_disk_bytes);
        self.process_lifetime_peak_resident_bytes = self
            .process_lifetime_peak_resident_bytes
            .max(row.peak_resident_bytes);
        self.process_lifetime_peak_private_commit_bytes = self
            .process_lifetime_peak_private_commit_bytes
            .max(row.peak_private_commit_bytes);
    }
    pub fn sample(&mut self, private_directory: &Path) -> Result<()> {
        let row = snapshot()?;
        let disk = temporary_disk_bytes(private_directory)?;
        self.observe(row, disk);
        Ok(())
    }
}
/// Read only an owned private build directory. Reject links/reparse points;
/// callers must also prevent concurrent replacement by untrusted writers.
pub fn temporary_disk_bytes(root: &Path) -> Result<u64> {
    fn visit(path: &Path, depth: usize, count: &mut usize, total: &mut u64) -> Result<()> {
        *count += 1;
        if *count > 4096 || depth > 32 {
            return Err("temporary build inventory limit".into());
        }
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| "temporary build metadata unavailable")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("temporary build reparse point denied".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("temporary build symbolic link denied".into());
        }
        if metadata.is_file() {
            *total = total
                .checked_add(metadata.len())
                .ok_or("temporary build size overflow")?;
        } else if metadata.is_dir() {
            for entry in
                std::fs::read_dir(path).map_err(|_| "temporary build directory unavailable")?
            {
                visit(
                    &entry
                        .map_err(|_| "temporary build entry unavailable")?
                        .path(),
                    depth + 1,
                    count,
                    total,
                )?;
            }
        } else {
            return Err("unsupported temporary build file".into());
        }
        Ok(())
    }
    let mut count = 0;
    let mut bytes = 0;
    visit(root, 0, &mut count, &mut bytes)?;
    Ok(bytes)
}

pub fn snapshot() -> Result<Snapshot> {
    #[cfg(all(windows, target_pointer_width = "64"))]
    {
        native::snapshot()
    }
    #[cfg(not(all(windows, target_pointer_width = "64")))]
    {
        Err("native resource measurement requires Windows x64".into())
    }
}
#[cfg(all(windows, target_pointer_width = "64"))]
mod native {
    use super::{Result, Snapshot};
    use std::{
        ffi::c_void,
        mem::{size_of, zeroed},
    };
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        size: u32,
        faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        commit: usize,
        peak_commit: usize,
        private: usize,
    }
    #[repr(C)]
    struct SystemInfo {
        architecture: u16,
        reserved: u16,
        page_size: u32,
        min: usize,
        max: usize,
        mask: usize,
        cpus: u32,
        kind: u32,
        granularity: u32,
        level: u16,
        revision: u16,
    }
    #[repr(C)]
    struct Region {
        base: usize,
        allocation_base: usize,
        allocation_protect: u32,
        partition: u16,
        size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetSystemInfo(info: *mut SystemInfo);
        fn VirtualQuery(address: *const c_void, region: *mut Region, size: usize) -> usize;
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(process: *mut c_void, counters: *mut Counters, size: u32) -> i32;
    }
    pub(super) fn snapshot() -> Result<Snapshot> {
        // Layouts are Windows x64 only. Each output is a live, correctly sized
        // object; the queried addresses are never dereferenced by Rust. These
        // APIs retain no pointer and inspect only this process.
        unsafe {
            if size_of::<Counters>() != 80
                || size_of::<SystemInfo>() != 48
                || size_of::<Region>() != 48
            {
                return Err("Unsupported native memory layout".into());
            }
            let mut counters = Counters {
                size: size_of::<Counters>() as u32,
                ..Default::default()
            };
            if GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                size_of::<Counters>() as u32,
            ) == 0
            {
                return Err("Process memory counters unavailable".into());
            }
            let mut info: SystemInfo = zeroed();
            GetSystemInfo(&mut info);
            let mut address = info.min;
            let (mut mapped, mut images, mut regions) = (0u64, 0u64, 0);
            while address <= info.max {
                let mut region: Region = zeroed();
                if VirtualQuery(address as *const c_void, &mut region, size_of::<Region>())
                    != size_of::<Region>()
                {
                    return Err("Incomplete process address-space measurement".into());
                }
                let next = region
                    .base
                    .checked_add(region.size)
                    .ok_or("Memory region overflow")?;
                if next <= address {
                    return Err("Non-advancing memory region".into());
                }
                if region.state == 0x1000 {
                    if region.kind == 0x40000 {
                        mapped = mapped
                            .checked_add(region.size as u64)
                            .ok_or("Mapped memory overflow")?;
                    }
                    if region.kind == 0x1000000 {
                        images = images
                            .checked_add(region.size as u64)
                            .ok_or("Image memory overflow")?;
                    }
                }
                regions += 1;
                if regions > 1000000 {
                    return Err("Memory region limit exceeded".into());
                }
                address = next;
            }
            Ok(Snapshot {
                resident_bytes: counters.working_set as u64,
                peak_resident_bytes: counters.peak_working_set as u64,
                private_commit_bytes: counters.private as u64,
                peak_private_commit_bytes: counters.peak_commit as u64,
                committed_mapped_address_bytes: mapped,
                committed_image_address_bytes: images,
            })
        }
    }
}
