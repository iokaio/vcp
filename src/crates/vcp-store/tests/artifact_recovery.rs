// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "qualification")]
mod common;
use common::*;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use vcp_domain::{artifact::*, ArtifactId, Revision};
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};

const BYTES: &[u8] = b"acknowledged binary output\0\xff preserved after seal failure";

// Assertion failures must not leave the qualification child parked at a barrier.
struct GuardedChild(Child);
impl std::ops::Deref for GuardedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for GuardedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for GuardedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => break,
            }
        }
        eprintln!(
            "artifact supervisor could not confirm termination of PID {}",
            self.0.id()
        );
    }
}

fn durable_json(path: &Path, value: &serde_json::Value) {
    use std::io::Write;
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
}

fn stop(child: &mut Child) -> std::process::ExitStatus {
    child.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "killed artifact child did not exit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn seal_publication_kills_and_storage_full_preserve_acknowledged_prefix() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let backend = if kind == BackendKind::Files {
            "files"
        } else {
            "sqlite"
        };
        for phase in [
            "before_publication",
            "after_publication",
            "after_reply",
            "after_reference",
            "storage_full",
        ] {
            let mut temp = tempfile::tempdir().unwrap();
            temp.disable_cleanup(true);
            println!(
                "retained artifact recovery fixture {backend}/{phase}: {}",
                temp.path().display()
            );
            let root = temp.path().join("canonical");
            let marker = temp.path().join("barrier.json");
            let mut store = Store::open(&root, kind, &[]).await.unwrap();
            let initial_receipt = store.transact(initial()).await.unwrap();
            let before = store.state().clone();
            store.close().await.unwrap();
            let id = ArtifactId::new();
            let started = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut child = GuardedChild(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "artifact_recovery_child", "--nocapture"])
                    .env("VCP_ARTIFACT_CHILD_ROOT", temp.path())
                    .env("VCP_ARTIFACT_CHILD_BACKEND", backend)
                    .env("VCP_ARTIFACT_CHILD_PHASE", phase)
                    .env("VCP_ARTIFACT_CHILD_ID", id.as_str())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("{backend}/{phase}: child exited before barrier: {status}");
                }
                if Instant::now() >= deadline {
                    stop(&mut child);
                    panic!("{backend}/{phase}: 30 second artifact barrier deadline exceeded");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            // Marker is synced before a separate ready file makes it observable.
            let ready = temp.path().join("ready");
            while !ready.exists() {
                if Instant::now() >= deadline {
                    stop(&mut child);
                    panic!("artifact acknowledgement deadline exceeded");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let acknowledgement: serde_json::Value =
                serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
            assert_eq!(acknowledgement["phase"], phase);
            assert_eq!(acknowledgement["chunk_acknowledged"], true);
            assert!(!stop(&mut child).success());
            let mut reopened = Store::open(&root, kind, &[]).await.unwrap();
            assert_eq!(reopened.transact(initial()).await.unwrap(), initial_receipt);
            assert_eq!(reopened.state().events, before.events);
            assert_eq!(reopened.state().commands, before.commands);
            assert_eq!(
                reopened.state().watermark.get(),
                if phase == "after_reference" { 3 } else { 2 }
            );
            for (key, record) in &before.records {
                assert_eq!(reopened.state().records.get(key), Some(record));
            }
            let canonical: ArtifactDescriptor = reopened
                .state()
                .record(Collection::Artifact, id.as_str(), &workspace().id)
                .unwrap()
                .decode()
                .unwrap();
            let durable = reopened.spool().inspect(&id).unwrap();
            let sealed = !matches!(phase, "before_publication" | "storage_full");
            assert_eq!(
                durable.state,
                if sealed {
                    CaptureState::Complete
                } else {
                    CaptureState::Pending
                }
            );
            assert_eq!(
                canonical.state,
                if phase == "after_reference" {
                    CaptureState::Complete
                } else {
                    CaptureState::Pending
                }
            );
            for descriptor in [&canonical, &durable] {
                let mut bytes = Vec::new();
                reopened.spool().read(descriptor, &mut bytes).unwrap();
                assert_eq!(bytes, BYTES);
            }
            // Reconcile only physically sealed data; a failed seal cannot invent a
            // completion. No tool/model dispatch occurs during reconciliation.
            if sealed && phase != "after_reference" {
                reopened
                    .transact(attach(
                        reopened.state(),
                        durable.clone(),
                        Some(Revision::ZERO),
                    ))
                    .await
                    .unwrap();
            }
            // Capacity restoration permits a fresh capture while the failed/pending
            // object's acknowledged prefix stays intact and inspectable.
            let mut retry = reopened.spool().create(spec()).unwrap();
            retry.write_chunk(b"capacity restored").unwrap();
            let retry = retry.finalize().unwrap();
            reopened
                .transact(attach(reopened.state(), retry, None))
                .await
                .unwrap();
            let final_state = reopened.state().clone();
            reopened.close().await.unwrap();
            let verified = Store::open(&root, kind, &[]).await.unwrap();
            assert_eq!(verified.state(), &final_state);
            assert_eq!(verified.spool().inspect(&id).unwrap(), durable);
            verified.close().await.unwrap();
            let receipt = serde_json::json!({
                "backend": backend, "phase": phase, "repeat_seed": "artifact-seal-1",
                "retained_root": temp.path(),
                "role": "store artifact owner", "deadline_unix_ms": started + 30_000,
                "supervisor_confirmed": acknowledgement, "killed": true,
                "reopened": true, "canonical_state": canonical.state,
                "durable_state": durable.state, "retained_bytes": BYTES.len(),
                "initial_receipt_unchanged": true, "external_dispatches": 0,
                "fault": if phase == "storage_full" { "injected StorageFull before seal hard-link, then owner kill" } else { "process kill" }
            });
            durable_json(&temp.path().join("outcome.json"), &receipt);
            if let Some(output) = std::env::var_os("VCP_P802_ARTIFACT_RECEIPTS") {
                let output = std::path::PathBuf::from(output);
                fs::create_dir_all(&output).unwrap();
                durable_json(&output.join(format!("{backend}-{phase}.json")), &receipt);
            }
            println!("artifact recovery receipt: {receipt}");
        }
    }
}

#[test]
fn artifact_recovery_child() {
    let Some(root) = std::env::var_os("VCP_ARTIFACT_CHILD_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let phase = std::env::var("VCP_ARTIFACT_CHILD_PHASE").unwrap();
    let barrier = |actual: &str| {
        if actual == phase {
            durable_json(
                &root.join("barrier.json"),
                &serde_json::json!({
                    "phase": actual, "chunk_acknowledged": true,
                    "finalize_acknowledged": matches!(actual, "after_reply" | "after_reference"),
                    "canonical_completion_acknowledged": actual == "after_reference"
                }),
            );
            fs::write(root.join("ready"), b"ready").unwrap();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let backend = if std::env::var("VCP_ARTIFACT_CHILD_BACKEND").unwrap() == "files" { BackendKind::Files } else { BackendKind::Sqlite };
        let mut store = Store::open(&root.join("canonical"), backend, &[]).await.unwrap();
        let mut spec = spec();
        spec.id = ArtifactId::parse(std::env::var("VCP_ARTIFACT_CHILD_ID").unwrap()).unwrap();
        let mut writer = store.spool().create(spec.clone()).unwrap();
        writer.write_chunk(BYTES).unwrap();
        let pending = store.spool().inspect(&spec.id).unwrap();
        store.transact(attach(store.state(), pending, None)).await.unwrap();
        let result = writer.finalize_observed(&|point| {
            if phase == "storage_full" && point == "before_publication" {
                return Err(std::io::Error::from(std::io::ErrorKind::StorageFull).into());
            }
            barrier(point);
            Ok(())
        });
        if phase == "storage_full" {
            assert!(matches!(result, Err(vcp_store::Error::Io(ref error)) if error.kind() == std::io::ErrorKind::StorageFull));
            assert!(writer.write_chunk(b"late").is_err());
            assert!(writer.finalize().is_err());
            barrier("storage_full");
        }
        let descriptor = result.unwrap();
        barrier("after_reply");
        store.transact(attach(store.state(), descriptor, Some(Revision::ZERO))).await.unwrap();
        barrier("after_reference");
        panic!("artifact recovery barrier was not reached");
    });
}
