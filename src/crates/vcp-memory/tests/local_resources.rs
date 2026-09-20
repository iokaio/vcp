// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use vcp_memory::local_resources::*;

fn workload() -> Workload {
    Workload {
        rows: 1024,
        source_bytes: 192 * 1024,
        batch: 16,
        load_model: true,
    }
}

#[test]
fn admission_is_bounded_shared_and_released_after_failure() {
    let pool = Admission::new(Limits::default()).unwrap();
    let shared = pool.clone();
    let lease = pool.acquire(workload()).unwrap();
    assert!(shared.acquire(workload()).is_err());
    assert!(lease.estimate().ram_bytes > 768 * 1024 * 1024);
    assert!(lease
        .check_temporary_disk(lease.estimate().temporary_disk_bytes + 1)
        .is_err());
    assert!(lease.check_temporary_disk(0).is_ok());
    assert!(lease.check_temporary_write(u64::MAX, 1).is_err());
    assert!(lease
        .check_temporary_write(lease.estimate().temporary_disk_bytes, 1)
        .is_err());
    drop(lease);
    assert!(shared.acquire(workload()).is_ok());
    assert!(pool
        .acquire(Workload {
            rows: 1025,
            ..workload()
        })
        .is_err());
    assert!(pool
        .acquire(Workload {
            batch: 17,
            ..workload()
        })
        .is_err());
    assert!(pool
        .acquire(Workload {
            source_bytes: 192 * 1024 + 1,
            ..workload()
        })
        .is_err());
    let tiny = Admission::new(Limits {
        declared_ram_bytes: 1,
        temporary_disk_bytes: 1,
    })
    .unwrap();
    assert!(tiny.acquire(workload()).is_err());
    assert!(!ESTIMATE_VERSION.is_empty());
}

#[test]
fn cancellation_and_interactive_work_take_priority_and_deadline_does_not_reset() {
    let mut control = Controls::new(Duration::from_millis(30), Duration::from_millis(10)).unwrap();
    assert_eq!(control.checkpoint(false, false), Decision::Continue);
    assert_eq!(control.checkpoint(false, true), Decision::Yield);
    assert_eq!(control.checkpoint(true, true), Decision::Cancelled);
    std::thread::sleep(Duration::from_millis(12));
    assert_eq!(control.checkpoint(false, false), Decision::Yield);
    control.resumed();
    std::thread::sleep(Duration::from_millis(30));
    control.resumed();
    assert_eq!(control.checkpoint(false, false), Decision::Deadline);
    assert!(Controls::new(Duration::from_secs(1), Duration::from_millis(101)).is_err());
}

#[test]
fn build_samples_do_not_mislabel_lifetime_peak_as_build_peak() {
    let mut observations = Measurements::start();
    let first = Snapshot {
        resident_bytes: 10,
        peak_resident_bytes: 100,
        private_commit_bytes: 20,
        peak_private_commit_bytes: 200,
        committed_mapped_address_bytes: 30,
        committed_image_address_bytes: 40,
    };
    observations.observe(first, 500);
    observations.observe(
        Snapshot {
            resident_bytes: 15,
            committed_mapped_address_bytes: 25,
            ..first
        },
        400,
    );
    assert_eq!(observations.samples, 2);
    assert_eq!(observations.sampled_build_peak_resident_bytes, 15);
    assert_eq!(observations.process_lifetime_peak_resident_bytes, 100);
    assert_eq!(observations.sampled_build_peak_mapped_address_bytes, 30);
    assert_eq!(observations.sampled_build_peak_temporary_disk_bytes, 500);
}

#[test]
fn temporary_disk_measurement_is_owned_bounded_and_independent() {
    let root = std::env::temp_dir().join(format!("vcp-local-resources-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("nested")).unwrap();
    std::fs::write(root.join("model"), [0; 17]).unwrap();
    std::fs::write(root.join("nested/vector"), [0; 31]).unwrap();
    assert_eq!(temporary_disk_bytes(&root).unwrap(), 48);
    #[cfg(all(windows, target_pointer_width = "64"))]
    {
        let actual = snapshot().unwrap();
        assert!(actual.resident_bytes > 0);
        assert!(actual.peak_resident_bytes >= actual.resident_bytes);
        assert!(actual.committed_image_address_bytes > 0);
        let mut measured = Measurements::start();
        measured.sample(&root).unwrap();
        assert_eq!(measured.sampled_build_peak_temporary_disk_bytes, 48);
    }
    // Delete only the exact fixture files we created; no recursive cleanup.
    std::fs::remove_file(root.join("model")).unwrap();
    std::fs::remove_file(root.join("nested/vector")).unwrap();
    std::fs::remove_dir(root.join("nested")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
