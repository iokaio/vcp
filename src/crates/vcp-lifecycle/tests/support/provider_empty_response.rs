// SPDX-License-Identifier: Apache-2.0
//! Real HTTP disconnects retain billing uncertainty without replaying partial output.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Copy, Debug)]
enum Failure {
    EmptyEof,
    EmptyDisconnect,
    PartialDisconnect,
}

struct Provider {
    endpoint: String,
    calls: Arc<AtomicUsize>,
    worker: tokio::task::JoinHandle<()>,
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl Provider {
    async fn start(failure: Failure, fail_forever: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let worker = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                // Consume the complete submitted request before closing its response.
                // This is a real HTTP 200 body-decoding error, not a fake status code.
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0_u8; 8192];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert_ne!(count, 0, "request closed before complete submission");
                    request.extend_from_slice(&chunk[..count]);
                    assert!(request.len() < 4 * 1024 * 1024);
                    if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&request[..end]).unwrap();
                        let length: usize = headers
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .unwrap()
                            .1
                            .trim()
                            .parse()
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let failing = index == 0 || fail_forever;
                let body = if !failing {
                    sse(vec![
                        ev_assistant_message("recovered", "The source remains unchanged."),
                        serde_json::json!({"type":"response.completed","response":{
                            "id":"recovered-response","status":"completed","output":[],
                            "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}
                        }}),
                    ])
                } else if matches!(failure, Failure::PartialDisconnect) {
                    sse(vec![ev_assistant_message(
                        "partial",
                        "Retained partial output.",
                    )])
                } else {
                    String::new()
                };
                let advertised = body.len()
                    + usize::from(failing && !matches!(failure, Failure::EmptyEof)) * 256;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {advertised}\r\nConnection: close\r\n\r\n{body}"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        });
        Self {
            endpoint,
            calls,
            worker,
        }
    }
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_provider_disconnect_recovers_with_distinct_liability_and_verified_completion() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for failure in [Failure::EmptyEof, Failure::EmptyDisconnect] {
            let provider = Provider::start(failure, false).await;
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, _) =
                super::provider_retries::setup_with_fixture_options(
                    &temp,
                    backend,
                    Duration::from_secs(15),
                    1000,
                    true,
                    1,
                    None,
                    true,
                    Some(provider.endpoint.clone()),
                    true,
                )
                .await;
            turn(&test).await;
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                2,
                "{backend:?}/{failure:?}"
            );
            let state = host.snapshot().unwrap();
            let attempts = attempts(&state);
            assert_eq!(attempts.len(), 2);
            let prior = attempts
                .iter()
                .find(|attempt| attempt.previous.is_none())
                .unwrap();
            let successor = attempts
                .iter()
                .find(|attempt| attempt.previous.is_some())
                .unwrap();
            assert_eq!(successor.previous.as_ref(), Some(&prior.id));
            assert_ne!(prior.id, successor.id);
            assert_ne!(prior.reservation, successor.reservation);
            assert_eq!(prior.phase, ReservationState::ReconciliationPending);
            assert_eq!(prior.charged, Micros::ZERO);
            assert_eq!(successor.phase, ReservationState::Settled);
            let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
            assert!(ledger.unresolved.known().unwrap().get() > 0);
            assert_eq!(ledger.settled.get(), 100);
            assert_eq!(ledger.active.known().unwrap().get(), 0);
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Effect)
                    .count(),
                0
            );
            assert_eq!(
                std::fs::read_to_string(temp.path().join("workspace/evidence.txt")).unwrap(),
                "current source"
            );
            let thread = test.codex.session_configured().thread_id;
            let verified = host.verify_for_completion(thread).await.unwrap();
            assert!(
                verified.outstanding_issues.is_empty(),
                "{:?}",
                verified.outstanding_issues
            );
            assert!(!host.completed_financial_uncertainty(prior.clone()).unwrap());
            assert!(matches!(
                host.try_complete_verified(thread, verified.id).unwrap(),
                vcp_lifecycle::foundation::verification::CompletionAttempt::Completed(_)
            ));
            assert!(host.completed_financial_uncertainty(prior.clone()).unwrap());
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                2,
                "verification cannot send inference"
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            let workspace = temp.path().join("workspace").canonicalize().unwrap();
            let mut config = config(&temp.path().join("canonical"), &workspace, backend);
            config.cap.micros = vcp_domain::Limit::Unbounded;
            config.max_transport_retries = 1;
            let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
            let reopened_state = reopened.snapshot().unwrap();
            let retained: Attempt = reopened_state
                .record(
                    Collection::Attempt,
                    prior.id.as_str(),
                    &binding.scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(&retained, prior);
            assert!(reopened
                .completed_financial_uncertainty(retained.clone())
                .unwrap());
            assert_eq!(
                vcp_budget::ledger(&reopened_state, &binding.scope).unwrap(),
                ledger
            );
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                2,
                "reopen cannot resend"
            );

            // The classifier requires one exact immutable proof, not any vaguely
            // matching evidence. Even identical duplicate proof records fail closed.
            use vcp_store::artifact::ArtifactWriter;
            let proof = reopened_state
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
                .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                .find(|artifact| artifact.spec.schema == "provider-empty-response-retry/1")
                .unwrap();
            let bytes = reopened.read_artifact(proof.spec.id.clone()).unwrap();
            let spool = vcp_store::artifact::Spool::open(
                &config.canonical_root.join("spool"),
                &[workspace],
                config.artifact_limit.get(),
            )
            .unwrap();
            let mut spec = proof.spec;
            spec.id = ArtifactId::new();
            let mut writer = spool.create(spec).unwrap();
            writer.write_chunk(&bytes).unwrap();
            let duplicate = writer.finalize().unwrap();
            drop(writer);
            reopened
                .command(
                    Command::AttachArtifact {
                        descriptor: duplicate,
                    },
                    Some(config.root_task.clone()),
                    Revision::ZERO,
                )
                .unwrap();
            assert!(!reopened.completed_financial_uncertainty(retained).unwrap());
            owner.close().await.unwrap();
        }
    }
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_provider_disconnect_obeys_retry_cap_and_partial_response_is_never_replayed() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for (failure, retries, expected) in [
            (Failure::EmptyDisconnect, 0, 1),
            (Failure::EmptyDisconnect, 1, 2),
            (Failure::PartialDisconnect, 2, 1),
        ] {
            let provider = Provider::start(failure, true).await;
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, _) =
                super::provider_retries::setup_with_fixture_options(
                    &temp,
                    backend,
                    Duration::from_secs(15),
                    1000,
                    true,
                    retries,
                    None,
                    true,
                    Some(provider.endpoint.clone()),
                    true,
                )
                .await;
            turn(&test).await;
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                expected,
                "{backend:?}/{failure:?}/{retries}"
            );
            let state = host.snapshot().unwrap();
            let attempts = attempts(&state);
            assert_eq!(attempts.len(), expected);
            let stopped: Task = state
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &binding.scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(stopped.state, TaskState::Paused);
            if !matches!(failure, Failure::PartialDisconnect) {
                assert!(
                    stopped.reason.contains("provider temporarily unavailable"),
                    "{}",
                    stopped.reason
                );
            }
            for attempt in attempts {
                assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
                assert!(!host.completed_financial_uncertainty(attempt).unwrap());
            }
            let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
            assert!(ledger.unresolved.known().unwrap().get() > 0);
            assert_eq!(ledger.settled.get(), 0);
            assert_eq!(ledger.active.known().unwrap().get(), 0);
            assert!(!state
                .records
                .values()
                .any(|row| matches!(row.collection, Collection::Effect | Collection::Settlement)));
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

fn attempts(state: &vcp_store::contract::State) -> Vec<Attempt> {
    state
        .records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .map(|row| row.decode().unwrap())
        .collect()
}
