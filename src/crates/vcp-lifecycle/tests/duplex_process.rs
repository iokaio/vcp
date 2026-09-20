// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use codex_extension_api::ExtensionRegistryBuilder;
use codex_protocol::ThreadId;
use core_test_support::{
    responses,
    test_codex::{test_codex, TestCodex},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use vcp_lifecycle::{
    process::{
        duplex::{Duplex, DuplexLimits},
        Limits,
    },
    Lifecycle, OwnerLease,
};

struct Fixture {
    host: Lifecycle,
    owner: Option<OwnerLease>,
    test: TestCodex,
    thread: ThreadId,
    directory: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let server = responses::start_mock_server().await;
        let (host, owner) = Lifecycle::new(Duration::from_secs(5));
        let mut extensions = ExtensionRegistryBuilder::new();
        extensions.turn_start_admission(Arc::new(host.clone()));
        let test = test_codex()
            .with_extensions(Arc::new(extensions.build()))
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host.attach_root(test.codex.clone()).unwrap();
        Self {
            host,
            owner: Some(owner),
            test,
            thread,
            directory: tempfile::tempdir().unwrap(),
        }
    }
    fn spawn(
        &self,
        mode: &str,
        limits: DuplexLimits,
        resources: Option<Arc<dyn Send + Sync>>,
    ) -> Duplex {
        self.host
            .spawn_duplex_process(
                self.thread,
                Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
                &[mode.into(), self.directory.path().as_os_str().into()],
                self.directory.path(),
                &BTreeMap::from([(
                    OsString::from("VCP_DUPLEX_PUBLIC"),
                    OsString::from("explicit-fixture"),
                )]),
                128,
                None,
                None,
                limits,
                resources,
            )
            .unwrap()
    }
    async fn close(mut self) {
        if let Some(owner) = self.owner.take() {
            owner.close().await.unwrap();
        }
        self.test.codex.shutdown_and_wait().await.unwrap();
    }
}
fn limits() -> DuplexLimits {
    DuplexLimits {
        process: Limits {
            timeout: Duration::from_secs(5),
            output_bytes: 16384,
            process_count: 2,
        },
        frame_bytes: 1024,
        queued_frames: 4,
        input_bytes: 4096,
    }
}
struct Resource(Arc<AtomicBool>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplex_exchanges_bounded_frames_without_inherited_environment_and_retains_resources() {
    let fixture = Fixture::new().await;
    let released = Arc::new(AtomicBool::new(false));
    let mut connection = fixture.spawn(
        "duplex-echo",
        limits(),
        Some(Arc::new(Resource(released.clone()))),
    );
    assert!(connection.write_line(b"not\na single frame").await.is_err());
    for text in ["first", "second"] {
        connection.write_line(text.as_bytes()).await.unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(5), connection.read_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let record: serde_json::Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(record["line"], text);
        assert_eq!(record["public"], "explicit-fixture");
        assert_eq!(record["path_inherited"], false);
        assert_eq!(
            Path::new(record["cwd"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            fixture.directory.path().canonicalize().unwrap()
        );
        assert!(!released.load(Ordering::Acquire));
    }
    connection.close_stdin();
    let outcome = connection.wait().await.unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stop_reason.is_none());
    assert!(outcome.stdout.total > outcome.stdout.bytes.len() as u64);
    assert!(outcome.stderr.total > 0);
    assert!(released.load(Ordering::Acquire));
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_frames_queues_stderr_and_deadlines_stop_owned_processes() {
    let fixture = Fixture::new().await;
    for (mode, expected) in [
        ("duplex-flood", "frame limit"),
        ("duplex-queue", "queue"),
        ("duplex-stderr", "output limit"),
        ("duplex-partial", "inside a frame"),
        ("duplex-silent", "deadline"),
    ] {
        let mut bound = limits();
        if mode == "duplex-silent" {
            bound.process.timeout = Duration::from_millis(100);
        }
        let connection = fixture.spawn(mode, bound, None);
        let outcome = tokio::time::timeout(Duration::from_secs(5), connection.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(
            outcome
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains(expected)),
            "{mode}: {outcome:?}"
        );
        assert!(outcome.stdout.bytes.len() <= 128 && outcome.stderr.bytes.len() <= 128);
    }
    fixture.close().await;
    let fixture = Fixture::new().await;
    let mut connection = fixture.spawn("duplex-flood", limits(), None);
    let error = tokio::time::timeout(Duration::from_secs(5), connection.read_line())
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("frame limit"), "{error}");
    connection.wait().await.unwrap();
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cumulative_input_and_job_descendant_limits_are_observed() {
    let fixture = Fixture::new().await;
    let mut bound = limits();
    bound.input_bytes = 6;
    let mut connection = fixture.spawn("duplex-echo", bound, None);
    connection.write_line(b"one").await.unwrap();
    assert!(connection.read_line().await.unwrap().is_some());
    assert!(connection
        .write_line(b"two")
        .await
        .unwrap_err()
        .to_string()
        .contains("lifetime limit"));
    connection.close_stdin();
    connection.wait().await.unwrap();
    assert_eq!(
        std::fs::read(fixture.directory.path().join("duplex-input")).unwrap(),
        b"one\n"
    );
    fixture.close().await;

    for mode in ["process-count", "tree"] {
        let fixture = Fixture::new().await;
        let connection = fixture.spawn(mode, limits(), None);
        let ready = fixture.directory.path().join(if mode == "process-count" {
            "count-result"
        } else {
            "child-ready"
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(connection.active_process_count().unwrap(), 2);
        assert!(std::fs::OpenOptions::new()
            .write(true)
            .open(fixture.directory.path().join("locked"))
            .is_err());
        if mode == "process-count" {
            assert_eq!(std::fs::read_to_string(ready).unwrap(), "blocked");
            assert!(!fixture.directory.path().join("excess-marker").exists());
        }
        connection.terminate().unwrap();
        connection.wait().await.unwrap();
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(fixture.directory.path().join("locked"))
                .is_ok(),
            "owned descendant lock is released after drain"
        );
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_close_and_drop_drain_without_callers_waiting() {
    for close_owner in [false, true] {
        let mut fixture = Fixture::new().await;
        let released = Arc::new(AtomicBool::new(false));
        let mut connection = fixture.spawn(
            "duplex-silent",
            limits(),
            Some(Arc::new(Resource(released.clone()))),
        );
        if close_owner {
            fixture.owner.take().unwrap().close().await.unwrap();
            assert_eq!(connection.active_process_count().unwrap(), 0);
            assert!(connection.write_line(b"after close").await.is_err());
            assert!(connection.read_line().await.is_err());
            assert!(
                released.load(Ordering::Acquire),
                "owner drains observer before returning"
            );
        } else {
            drop(connection);
            tokio::time::timeout(Duration::from_secs(5), async {
                while !released.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_rejects_further_io_and_cancelled_partial_write_kills_connection() {
    let fixture = Fixture::new().await;
    let mut connection = fixture.spawn("duplex-silent", limits(), None);
    let revision = fixture.host.inspect(fixture.thread).unwrap().revision;
    fixture
        .host
        .hold(fixture.thread, &revision)
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(connection.write_line(b"after pause").await.is_err());
    assert!(connection.read_line().await.is_err());
    assert_eq!(connection.active_process_count().unwrap(), 0);
    fixture.close().await;

    let fixture = Fixture::new().await;
    let mut bound = limits();
    bound.frame_bytes = 1024 * 1024;
    bound.input_bytes = 2 * 1024 * 1024;
    let mut connection = fixture.spawn("duplex-silent", bound, None);
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        connection.write_line(&vec![b'x'; 1024 * 1024])
    )
    .await
    .is_err());
    let outcome = tokio::time::timeout(Duration::from_secs(5), connection.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(outcome
        .stop_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("write interrupted")));
    fixture.close().await;
}
