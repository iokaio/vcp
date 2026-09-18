// SPDX-License-Identifier: Apache-2.0
//! Qualification measurements for this process; not product resource enforcement.
use serde::Serialize;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub private_commit_bytes: u64,
    pub peak_private_commit_bytes: u64,
    pub committed_mapped_address_bytes: u64,
    pub committed_image_address_bytes: u64,
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
pub fn snapshot() -> Result<Snapshot> {
    #[cfg(all(windows, target_pointer_width = "64"))]
    {
        native::snapshot()
    }
    #[cfg(not(all(windows, target_pointer_width = "64")))]
    {
        Err("Native Windows x64 required for memory qualification".into())
    }
}
#[derive(Debug, Serialize)]
pub struct Measurements {
    samples: u64,
    requested_interval_ms: u64,
    maximum_sample_gap_ms: u128,
    peak_resident_bytes: u64,
    peak_private_commit_bytes: u64,
    peak_sampled_mapped_address_bytes: u64,
    peak_sampled_image_address_bytes: u64,
    last: Snapshot,
}
impl Measurements {
    fn first(row: Snapshot) -> Self {
        Self {
            samples: 1,
            requested_interval_ms: 50,
            maximum_sample_gap_ms: 0,
            peak_resident_bytes: row.peak_resident_bytes,
            peak_private_commit_bytes: row.peak_private_commit_bytes,
            peak_sampled_mapped_address_bytes: row.committed_mapped_address_bytes,
            peak_sampled_image_address_bytes: row.committed_image_address_bytes,
            last: row,
        }
    }
    fn observe(&mut self, row: Snapshot, gap: Duration) {
        self.samples += 1;
        self.maximum_sample_gap_ms = self.maximum_sample_gap_ms.max(gap.as_millis());
        self.peak_resident_bytes = self.peak_resident_bytes.max(row.peak_resident_bytes);
        self.peak_private_commit_bytes = self
            .peak_private_commit_bytes
            .max(row.peak_private_commit_bytes);
        self.peak_sampled_mapped_address_bytes = self
            .peak_sampled_mapped_address_bytes
            .max(row.committed_mapped_address_bytes);
        self.peak_sampled_image_address_bytes = self
            .peak_sampled_image_address_bytes
            .max(row.committed_image_address_bytes);
        self.last = row;
    }
}
pub struct Sampler {
    stop: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<Result<Measurements>>>,
}
impl Sampler {
    pub fn start() -> Result<Self> {
        let first = snapshot()?;
        let (stop, receive) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("qualification-memory-sampler".into())
            .spawn(move || {
                let mut observations = Measurements::first(first);
                let mut previous = Instant::now();
                loop {
                    let stopped = match receive.recv_timeout(Duration::from_millis(50)) {
                        Err(mpsc::RecvTimeoutError::Timeout) => false,
                        _ => true,
                    };
                    let row = snapshot()?;
                    let now = Instant::now();
                    observations.observe(row, now.duration_since(previous));
                    previous = now;
                    if stopped {
                        return Ok(observations);
                    }
                }
            })
            .map_err(|_| "Cannot start owned memory sampler")?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
    pub fn finish(mut self) -> Result<Measurements> {
        let _ = self.stop.send(());
        self.worker
            .take()
            .ok_or("Missing owned sampler")?
            .join()
            .map_err(|_| "Memory sampler panicked")?
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(all(test, windows, target_pointer_width = "64"))]
mod tests {
    use super::*;
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(
            file: *mut c_void,
            security: *const c_void,
            protect: u32,
            high: u32,
            low: u32,
            name: *const u16,
        ) -> *mut c_void;
        fn MapViewOfFile(
            mapping: *mut c_void,
            access: u32,
            high: u32,
            low: u32,
            size: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    struct OwnedMapping {
        handle: *mut c_void,
        base: *mut c_void,
    }
    impl Drop for OwnedMapping {
        fn drop(&mut self) {
            // SAFETY: these handles/views were created only by this fixture.
            unsafe {
                if !self.base.is_null() {
                    UnmapViewOfFile(self.base);
                }
                if !self.handle.is_null() {
                    CloseHandle(self.handle);
                }
            }
        }
    }
    #[test]
    fn real_owned_mapping_is_distinct_from_resident_and_lifetime_peak_memory() {
        let sampler = Sampler::start().unwrap();
        let before = snapshot().unwrap();
        let size = 16 * 1024 * 1024;
        // SAFETY: pagefile-backed mapping, no files or external process. Every
        // touched page is inside the successfully mapped view. RAII also frees
        // owned handles if an assertion fails.
        let mut mapping = unsafe {
            OwnedMapping {
                handle: CreateFileMappingW(
                    usize::MAX as *mut c_void,
                    std::ptr::null(),
                    4,
                    0,
                    size as u32,
                    std::ptr::null(),
                ),
                base: std::ptr::null_mut(),
            }
        };
        assert!(!mapping.handle.is_null());
        unsafe {
            mapping.base = MapViewOfFile(mapping.handle, 2, 0, 0, size);
            assert!(!mapping.base.is_null());
            for offset in (0..size).step_by(4096) {
                (mapping.base as *mut u8).add(offset).write_volatile(1);
            }
        }
        let during = snapshot().unwrap();
        assert!(
            during.committed_mapped_address_bytes
                >= before.committed_mapped_address_bytes + size as u64
        );
        assert!(during.resident_bytes > before.resident_bytes);
        let measured = sampler.finish().unwrap();
        assert!(measured.samples >= 2);
        assert!(
            measured.peak_sampled_mapped_address_bytes
                >= before.committed_mapped_address_bytes + size as u64
        );
        drop(mapping);
        let after = snapshot().unwrap();
        assert!(
            after.committed_mapped_address_bytes + size as u64
                <= during.committed_mapped_address_bytes
        );
        assert!(after.peak_resident_bytes >= during.resident_bytes);
    }
}
