// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use vcp_lifecycle::foundation::reconciliation::{
    ReceiptFetch, ReceiptSource, ReconciliationStatus,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

async fn failed(
    temp: &tempfile::TempDir,
    backend: BackendKind,
    identity: bool,
    header_identity: Option<&str>,
) -> (
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    ThreadBinding,
    TestCodex,
    Config,
) {
    failed_response(temp, backend, identity, header_identity, None).await
}

async fn failed_response(
    temp: &tempfile::TempDir,
    backend: BackendKind,
    identity: bool,
    header_identity: Option<&str>,
    interrupted: Option<String>,
) -> (
    CanonicalHost,
    vcp_lifecycle::foundation::CanonicalOwner,
    ThreadBinding,
    TestCodex,
    Config,
) {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let mut settings = config(&temp.path().join("canonical"), &workspace, backend);
    settings.max_transport_retries = 0;
    let (host, owner) = CanonicalHost::open(settings.clone()).unwrap();
    let binding = task(&host, &settings, settings.root_task.clone(), None);
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider_with_timeout(snapshot.clone(), raw, Duration::from_secs(10))
        .unwrap();
    let server = start_mock_server().await;
    let body = if identity {
        serde_json::json!({"id":"gen-rejected", "error":{"code":429, "metadata":{"limit_source":"upstream_provider_shared_pool"}}})
    } else {
        serde_json::json!({"error":{"code":429}})
    };
    let response = match (interrupted, header_identity) {
        (Some(body), _) => ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(body),
        (None, Some(id)) => ResponseTemplate::new(429)
            .set_body_json(body)
            .insert_header("x-generation-id", id),
        (None, None) => ResponseTemplate::new(429).set_body_json(body),
    };
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-receipt-key",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = workspace.try_into().unwrap();
            configure_provider_fixture(c);
            starter
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(id, binding.clone()).unwrap();
    let sealed = sealed_provider_context(&host, id, &snapshot, None);
    host.prepare_context(id, sealed, serde_json::json!([]), vec![])
        .unwrap();
    turn(&test).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let ledger = vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope).unwrap();
    assert_eq!(
        (
            ledger.settled.get(),
            ledger.active.get(),
            ledger.unresolved.get()
        ),
        (0, 0, 100)
    );
    (host, owner, binding, test, settings)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_reconciliation_interrupted_successful_sse_keeps_large_response_identity() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let body = sse(vec![
            serde_json::json!({"type":"response.created","response":{"id":"gen-interrupted"}}),
            ev_message_item_added("interrupted-message", ""),
            ev_output_text_delta(&"x".repeat(90_000)),
        ]);
        let (host, owner, binding, test, settings) =
            failed_response(&temp, backend, false, None, Some(body)).await;
        assert_eq!(
            host.pending_provider_charges().unwrap()[0]
                .expected
                .request_id,
            "gen-interrupted"
        );
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (host, owner) = reopen(settings.clone()).await;
        let pending = host.pending_provider_charges().unwrap().pop().unwrap();
        assert_eq!(pending.expected.request_id, "gen-interrupted");
        host.configure_receipt_source(Arc::new(Unavailable(AtomicUsize::new(0))))
            .unwrap();
        let report = host.reconcile_root_pending().await.unwrap();
        assert!(matches!(
            report.observations.as_slice(),
            [ReconciliationStatus::Unknown { .. }]
        ));
        assert_eq!(
            vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope)
                .unwrap()
                .unresolved
                .get(),
            100
        );
        owner.close().await.unwrap();
        drop(host);
        let (host, owner) = reopen(settings).await;
        host.configure_receipt_source(Arc::new(InterruptedReceipt))
            .unwrap();
        let report = host.reconcile_root_pending().await.unwrap();
        assert!(
            matches!(report.observations.as_slice(), [ReconciliationStatus::Settled { amount, .. }] if amount.get() == 50)
        );
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
        assert_eq!(
            (
                ledger.settled.get(),
                ledger.active.get(),
                ledger.unresolved.get()
            ),
            (50, 0, 0)
        );
        assert_eq!(
            state
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .count(),
            1
        );
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_reconciliation_zero_nonzero_conflict_duplicate_and_reopen_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (cost, micros) in [("0", 0), ("0.00005", 50)] {
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, settings) = failed(&temp, backend, true, None).await;
            let pending = host.pending_provider_charges().unwrap().pop().unwrap();
            for bad in [
                r#"{"data":{"id":"gen-other","total_cost":0}}"#,
                r#"{"data":{"id":"gen-rejected"}}"#,
                r#"{"data":{"id":"gen-rejected","total_cost":-1}}"#,
            ] {
                assert!(host
                    .reconcile_provider_receipt(pending.clone(), bad.as_bytes().to_vec())
                    .is_err());
                assert_eq!(
                    vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope)
                        .unwrap()
                        .unresolved
                        .get(),
                    100
                );
            }
            let raw = format!(r#"{{"data":{{"id":"gen-rejected","model":"gpt-5.1","total_cost":{cost},"finish_reason":"error"}}}}"#).into_bytes();
            for _ in 0..2 {
                assert!(
                    matches!(host.reconcile_provider_receipt(pending.clone(), raw.clone()).unwrap(), ReconciliationStatus::Settled { amount, .. } if amount.get() == micros)
                );
            }
            let state = host.snapshot().unwrap();
            let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
            assert_eq!((ledger.settled.get(), ledger.unresolved.get()), (micros, 0));
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Settlement)
                    .count(),
                1
            );
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Attempt)
                    .count(),
                1,
                "metadata settlement never creates another inference attempt"
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            let (reopened, owner) = reopen(settings).await;
            let ledger = vcp_budget::ledger(&reopened.snapshot().unwrap(), &binding.scope).unwrap();
            assert_eq!((ledger.settled.get(), ledger.unresolved.get()), (micros, 0));
            assert!(reopened.pending_provider_charges().unwrap().is_empty());
            owner.close().await.unwrap();
        }
    }
}

struct Unavailable(AtomicUsize);

struct InterruptedReceipt;
impl ReceiptSource for InterruptedReceipt {
    fn fetch<'a>(
        &'a self,
        request_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ReceiptFetch, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            assert_eq!(request_id, "gen-interrupted");
            Ok(ReceiptFetch::Found(
                br#"{"data":{"id":"gen-interrupted","model":"gpt-5.1","cancelled":true,"total_cost":0.00005}}"#
                    .to_vec(),
            ))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_reconciliation_header_identity_and_body_conflicts_are_retained() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (body_id, header_id, has_identity) in [
            (false, "gen-header", true),
            (true, "gen-rejected", true),
            (true, "gen-conflict", false),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, _) =
                failed(&temp, backend, body_id, Some(header_id)).await;
            let pending = host.pending_provider_charges().unwrap();
            assert_eq!(pending.len(), usize::from(has_identity));
            if has_identity {
                assert_eq!(pending[0].expected.request_id, header_id);
            }
            assert_eq!(
                vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope)
                    .unwrap()
                    .unresolved
                    .get(),
                100
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

impl ReceiptSource for Unavailable {
    fn fetch<'a>(
        &'a self,
        _: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ReceiptFetch, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ReceiptFetch::Unavailable)
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_reconciliation_missing_identity_and_unavailable_receipts_remain_unknown() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for identity in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (host, owner, binding, test, _) = failed(&temp, backend, identity, None).await;
            let source = Arc::new(Unavailable(AtomicUsize::new(0)));
            host.configure_receipt_source(source.clone()).unwrap();
            let id = test.codex.session_configured().thread_id;
            for _ in 0..2 {
                host.reconcile_pending(id).await.unwrap();
            }
            assert_eq!(
                source.0.load(Ordering::SeqCst),
                usize::from(identity),
                "lookups are throttled and require a generation identity"
            );
            let ledger = vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope).unwrap();
            assert_eq!((ledger.settled.get(), ledger.unresolved.get()), (0, 100));
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_reconciliation_replays_captured_receipt_after_settlement_interruption() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (host, owner, binding, test, settings) = failed(&temp, backend, true, None).await;
        let pending = host.pending_provider_charges().unwrap().pop().unwrap();
        host.qualification_observe_model_dispatch(|point, _, _| {
            if point
                == vcp_lifecycle::foundation::model_dispatch_qualification::Point::BeforeSettlement
            {
                Err("injected interruption after receipt publication".into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert!(host
            .reconcile_provider_receipt(
                pending,
                br#"{"data":{"id":"gen-rejected","total_cost":0,"finish_reason":"error"}}"#
                    .to_vec()
            )
            .is_err());
        assert_eq!(
            vcp_budget::ledger(&host.snapshot().unwrap(), &binding.scope)
                .unwrap()
                .unresolved
                .get(),
            100
        );
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (reopened, owner) = reopen(settings).await;
        // Replaying through a registered reopened owner requires no metadata GET
        // and cannot restart inference. A fresh retained thread attaches paused.
        let server = start_mock_server().await;
        let starter = reopened.clone();
        let root = std::path::PathBuf::from(
            &reopened
                .snapshot()
                .unwrap()
                .record(
                    Collection::Workspace,
                    "canonical-workspace",
                    &binding.scope.workspace,
                )
                .unwrap()
                .decode::<Workspace>()
                .unwrap()
                .binding
                .root,
        );
        let test = test_codex()
            .with_config(move |c| {
                c.cwd = root.try_into().unwrap();
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let id = reopened
            .lifecycle()
            .attach_root(test.codex.clone())
            .unwrap();
        reopened.register(id, binding.clone()).unwrap();
        reopened.reconcile_pending(id).await.unwrap();
        assert_eq!(
            vcp_budget::ledger(&reopened.snapshot().unwrap(), &binding.scope)
                .unwrap()
                .unresolved
                .get(),
            0
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
    }
}

async fn reopen(settings: Config) -> (CanonicalHost, vcp_lifecycle::foundation::CanonicalOwner) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match CanonicalHost::open(settings.clone()) {
            Ok(owner) => return owner,
            Err(error)
                if (error.contains("already held")
                    || error.contains("canonical root already has an owner"))
                    && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("reopen failed after runtime shutdown: {error}"),
        }
    }
}
