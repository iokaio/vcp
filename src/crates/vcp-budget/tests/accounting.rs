// SPDX-License-Identifier: Apache-2.0
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;
use std::collections::BTreeMap;
use vcp_budget::{arithmetic::*, *};
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, task::*, workspace::*};
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};
fn currency() -> Currency {
    "USD".to_owned().try_into().unwrap()
}

#[tokio::test]
async fn purged_accounting_preserves_exact_retry_and_new_late_usage() {
    use std::collections::BTreeSet;
    const MARKER: &str = "retained-accounting-narrative-to-purge";
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = setup(temp.path(), kind, 1000, 0).await;
        let scope = common::task().scope;
        let request = capture(&mut store, &scope, b"synthetic request").await;
        let started = start(&mut store, &scope, &request, 50).await;
        let mut original = usage(&mut store, &started, 40, 1, true).await;
        original.correction = Some(Resolution {
            actor: actor().id,
            policy: PolicyRevision::ZERO,
            reason: MARKER.into(),
            remaining_uncertainty: MARKER.into(),
        });
        let prior = observe(&mut store, original.clone(), &actor())
            .await
            .unwrap();
        let accounting = ledger(store.state(), &scope).unwrap();
        let mut task: Task = store
            .state()
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let task_revision = task.revision;
        task.revision = task.revision.next().unwrap();
        task.state = TaskState::Cancelled;
        let mut workspace: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let workspace_revision = workspace.revision;
        workspace.revision = workspace.revision.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(task_revision),
                        record: Record::typed(
                            Collection::Task,
                            scope.task.as_str(),
                            scope.workspace.clone(),
                            task.revision,
                            &task,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: Some(workspace_revision),
                        record: Record::typed(
                            Collection::Workspace,
                            scope.workspace.as_str(),
                            scope.workspace.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let records = BTreeSet::from([
            key(Collection::Attempt, started.id.as_str()),
            key(Collection::Settlement, original.id.as_str()),
        ]);
        let events = store
            .state()
            .events
            .iter()
            .map(|event| event.event.id.clone())
            .collect();
        let candidate = store
            .retention_candidate(&records, &events, &BTreeSet::from([scope.task.clone()]))
            .unwrap();
        store.rewrite_base(candidate, &[]).await.unwrap();
        assert!(
            !String::from_utf8(serde_json::to_vec(store.state()).unwrap())
                .unwrap()
                .contains(MARKER)
        );
        assert_eq!(ledger(store.state(), &scope).unwrap(), accounting);
        let watermark = store.state().watermark;
        let retry = observe(&mut store, original.clone(), &actor())
            .await
            .unwrap();
        assert!(retry.redaction.is_some());
        assert_eq!(
            (retry.total, retry.adjustment),
            (prior.total, prior.adjustment)
        );
        assert_eq!(store.state().watermark, watermark);
        let mut changed = original.clone();
        changed.correction.as_mut().unwrap().reason.push('!');
        assert!(observe(&mut store, changed, &actor()).await.is_err());
        assert_eq!(store.state().watermark, watermark);
        // A later bill is new evidence. Purging historical narrative must not
        // erase a newly discovered liability or prevent actual cost settlement.
        let partial = usage(&mut store, &started, 60, 2, false).await;
        observe(&mut store, partial, &actor()).await.unwrap();
        assert_eq!(
            attempt(store.state(), &started.id, &scope.workspace)
                .unwrap()
                .phase,
            ReservationState::ReconciliationPending
        );
        store.close().await.unwrap();
        let mut store = Store::open(temp.path(), kind, &[]).await.unwrap();
        let mut final_bill = usage(&mut store, &started, 70, 3, true).await;
        final_bill.correction = Some(Resolution {
            actor: actor().id,
            policy: PolicyRevision::ZERO,
            reason: "fresh late accounting explanation".into(),
            remaining_uncertainty: "fresh reconciled uncertainty".into(),
        });
        observe(&mut store, final_bill.clone(), &actor())
            .await
            .unwrap();
        assert_eq!(
            ledger(store.state(), &scope).unwrap().settled,
            Micros::new(70)
        );
        assert!(attempt(store.state(), &started.id, &scope.workspace)
            .unwrap()
            .redaction
            .is_some());
        assert_eq!(
            observe(&mut store, original, &actor()).await.unwrap(),
            retry
        );
        assert!(
            !String::from_utf8(serde_json::to_vec(store.state()).unwrap())
                .unwrap()
                .contains(MARKER)
        );
        let second_records = BTreeSet::from([
            key(Collection::Attempt, started.id.as_str()),
            key(Collection::Settlement, final_bill.id.as_str()),
        ]);
        let events = store
            .state()
            .events
            .iter()
            .filter(|event| event.redaction.is_none())
            .map(|event| event.event.id.clone())
            .collect();
        let candidate = store
            .retention_candidate(
                &second_records,
                &events,
                &BTreeSet::from([scope.task.clone()]),
            )
            .unwrap();
        store.rewrite_base(candidate, &[]).await.unwrap();
        let bytes = String::from_utf8(serde_json::to_vec(store.state()).unwrap()).unwrap();
        assert!(!bytes.contains("fresh late accounting explanation"));
        assert!(!bytes.contains("fresh reconciled uncertainty"));
        assert_eq!(
            ledger(store.state(), &scope).unwrap().settled,
            Micros::new(70)
        );
        assert!(observe(&mut store, final_bill, &actor())
            .await
            .unwrap()
            .redaction
            .is_some());
    }
}

#[tokio::test]
async fn direct_transactions_cannot_spend_protected_funds_or_skip_child_allocations() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = setup(temporary.path(), kind, 1000, 100).await;
        let scope = common::task().scope;
        let request = capture(&mut store, &scope, b"protected request").await;
        let input = admission(&store, &scope, &request, 10);
        let (mut forged, _) = prepare_admission(store.state(), &input, &actor()).unwrap();
        for mutation in &mut forged.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Ledger {
                    let mut root: Ledger = record.decode().unwrap();
                    root.protected = Micros::ZERO;
                    record.value = serde_json::to_value(root).unwrap();
                }
            }
        }
        let before = store.state().clone();
        assert!(store.transact(forged).await.is_err());
        assert_eq!(store.state(), &before);
        let mut child = common::task();
        child.scope.task = TaskId::new();
        child.parent = Some(scope.task.clone());
        child.state = TaskState::Running;
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Task,
                        child.scope.task.to_string(),
                        scope.workspace.clone(),
                        Revision::ZERO,
                        &child,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let current = ledger(store.state(), &scope).unwrap();
        configure(
            &mut store,
            &scope,
            current.revision,
            money(1000),
            Micros::new(100),
            BTreeMap::from([(child.scope.task.clone(), Micros::new(5))]),
            &actor(),
            "small child ceiling",
        )
        .await
        .unwrap();
        let child_request = capture(&mut store, &child.scope, b"child request").await;
        // An untrusted adapter fabricates a snapshot with the ceiling removed.
        // Canonical admission must independently enforce the stored policy.
        let mut snapshot = store.state().clone();
        let root_record = snapshot
            .records
            .get_mut(&key(Collection::Ledger, scope.task.as_str()))
            .unwrap();
        let mut root: Ledger = root_record.decode().unwrap();
        root.allocations.clear();
        root_record.value = serde_json::to_value(root).unwrap();
        let input = admission(&store, &child.scope, &child_request, 10);
        let (mut forged, _) = prepare_admission(&snapshot, &input, &actor()).unwrap();
        for mutation in &mut forged.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Ledger {
                    let mut root: Ledger = record.decode().unwrap();
                    root.allocations
                        .insert(child.scope.task.clone(), Micros::new(5));
                    record.value = serde_json::to_value(root).unwrap();
                }
            }
        }
        let before = store.state().clone();
        assert!(store.transact(forged).await.is_err());
        assert_eq!(store.state(), &before);
    }
}

#[cfg(feature = "qualification")]
#[tokio::test]
async fn combined_capture_task_reservation_event_and_receipt_survive_process_kills() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        for barrier in ["prepared", "before_commit", "after_commit", "before_reply"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("canonical");
            let store = setup(&root, kind, 1000, 0).await;
            let scope = common::task().scope;
            let mut spec = common::spec();
            spec.channel = Channel::RequestBody;
            let mut writer = store.spool().create(spec).unwrap();
            writer
                .write_chunk(b"full request before admission")
                .unwrap();
            let captured = writer.finalize().unwrap();
            drop(writer);
            let input = admission(&store, &scope, &captured, 100);
            let (mut transaction, _) =
                prepare_captured_admission(store.state(), &input, &captured, &actor()).unwrap();
            let mut task: Task = store
                .state()
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            let previous = task.revision;
            task.revision = task.revision.next().unwrap();
            task.reason = "request durably admitted".into();
            transaction.mutations.push(Mutation::Put {
                expected: Some(previous),
                record: Record::typed(
                    Collection::Task,
                    scope.task.to_string(),
                    scope.workspace.clone(),
                    task.revision,
                    &task,
                )
                .unwrap(),
            });
            transaction.events[0].data["facts"] =
                serde_json::json!([{"collection":"task","value":task}]);
            let command = CommandId::new();
            transaction.events[0].correlation = command.clone();
            transaction.command = Some(ReceiptInput {
                command: command.clone(),
                workspace: scope.workspace.clone(),
                session: scope.session.clone(),
                digest: vcp_protocol::digest_bytes(b"combined fixture command"),
                result: vcp_protocol::command::CommandResult::Accepted {
                    revision: task.revision,
                },
            });
            let original = store.state().clone();
            let transaction_path = temporary.path().join("transaction.json");
            std::fs::write(
                &transaction_path,
                vcp_protocol::canonical_bytes(&transaction).unwrap(),
            )
            .unwrap();
            drop(store);
            let marker = temporary.path().join("reached");
            let mut child = Command::new(env!("CARGO_BIN_EXE_vcp-budget-crash-fixture"))
                .arg(&root)
                .arg(if kind == BackendKind::Sqlite {
                    "sqlite"
                } else {
                    "files"
                })
                .arg(&transaction_path)
                .arg(barrier)
                .arg(&marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("child exited before {barrier}: {status}");
                }
                if start.elapsed() > Duration::from_secs(20) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("accounting barrier timeout");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut store = Store::open(&root, kind, &[]).await.unwrap();
            let committed = matches!(barrier, "after_commit" | "before_reply");
            if !committed {
                assert_eq!(store.state(), &original);
            } else {
                assert_eq!(ledger(store.state(), &scope).unwrap().active.get(), 100);
                assert_eq!(store.state().events.len(), original.events.len() + 1);
                assert_eq!(store.state().commands.len(), original.commands.len() + 1);
                assert_eq!(
                    store
                        .state()
                        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                        .unwrap()
                        .revision,
                    task.revision
                );
                assert!(store
                    .state()
                    .record(
                        Collection::Artifact,
                        captured.spec.id.as_str(),
                        &scope.workspace
                    )
                    .is_ok());
            }
            let receipt = store.transact(transaction.clone()).await.unwrap();
            assert_eq!(store.transact(transaction).await.unwrap(), receipt);
            assert_eq!(ledger(store.state(), &scope).unwrap().active.get(), 100);
            let other = if kind == BackendKind::Sqlite {
                BackendKind::Files
            } else {
                BackendKind::Sqlite
            };
            let converted = store
                .convert(&temporary.path().join("converted"), other, &[])
                .await
                .unwrap();
            assert_eq!(store.state(), converted.state());
            let mut bytes = Vec::new();
            converted.spool().read(&captured, &mut bytes).unwrap();
            assert_eq!(bytes, b"full request before admission");
        }
    }
}
fn money(value: u64) -> Money {
    Money {
        currency: currency(),
        micros: Micros::new(value),
    }
}
fn actor() -> Actor {
    Actor {
        id: ActorId::parse("budget-owner").unwrap(),
        now: Timestamp::new(1000),
    }
}
fn price(cost: u64) -> PriceSnapshot {
    PriceSnapshot {
        id: "a".repeat(64),
        provider: "scripted-loopback".into(),
        model: "offline-fixture".into(),
        currency: currency(),
        capability: "b".repeat(64),
        valid_until: Timestamp::new(u64::MAX),
        rates: [
            ChargeCategory::Input,
            ChargeCategory::Output,
            ChargeCategory::CacheRead,
            ChargeCategory::CacheWrite,
            ChargeCategory::Request,
            ChargeCategory::ProviderTool,
        ]
        .into_iter()
        .map(|kind| {
            (
                kind,
                Rate {
                    micros: Micros::new(if kind == ChargeCategory::Request {
                        cost
                    } else {
                        0
                    }),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    }
}
async fn setup(root: &std::path::Path, kind: BackendKind, cap: u64, protected: u64) -> Store {
    let mut store = Store::open(root, kind, &[]).await.unwrap();
    let mut initial = common::initial();
    for mutation in &mut initial.mutations {
        if let Mutation::Put { record, .. } = mutation {
            if record.collection == Collection::Task {
                let mut task: Task = record.decode().unwrap();
                task.state = TaskState::Running;
                record.value = serde_json::to_value(task).unwrap();
            }
        }
    }
    initial.events[0].data =
        serde_json::json!({"state":"running","fixture":"imported running root"});
    store.transact(initial).await.unwrap();
    initialize(
        &mut store,
        common::task().scope,
        money(cap),
        Micros::new(protected),
        None,
        &actor(),
    )
    .await
    .unwrap();
    store
}
async fn capture(store: &mut Store, scope: &Scope, bytes: &[u8]) -> ArtifactDescriptor {
    let mut spec = common::spec();
    spec.scope = scope.clone();
    spec.channel = Channel::RequestBody;
    let mut writer = store.spool().create(spec).unwrap();
    writer.write_chunk(bytes).unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(common::attach(store.state(), artifact.clone(), None))
        .await
        .unwrap();
    artifact
}
fn admission(store: &Store, scope: &Scope, artifact: &ArtifactDescriptor, cost: u64) -> Admission {
    let root = ledger(store.state(), scope).unwrap();
    Admission {
        transaction: TransactionId::new(),
        attempt: AttemptId::new(),
        reservation: ReservationId::new(),
        scope: scope.clone(),
        agent: AgentId::new(),
        role: RequestRole::Main,
        request: artifact.spec.id.clone(),
        request_digest: artifact.sha256.clone(),
        quote: quote(
            price(cost),
            Usage {
                requests: Units::new(1),
                ..Default::default()
            },
            actor().now,
        )
        .unwrap(),
        previous: None,
        expected_ledger: root.revision,
        policy: root.policy,
        steering: SteeringRevision::ZERO,
        draw_protected: false,
        now: actor().now,
    }
}
async fn usage(
    store: &mut Store,
    attempt: &Attempt,
    amount: u64,
    version: u64,
    final_usage: bool,
) -> UsageObservation {
    let raw = capture(
        store,
        &attempt.scope,
        format!("synthetic provider usage {amount}").as_bytes(),
    )
    .await;
    UsageObservation {
        id: ObservationId::new(),
        scope: attempt.scope.clone(),
        attempt: attempt.id.clone(),
        provider_request: format!("provider-{}", attempt.id),
        mode: UsageMode::Cumulative {
            version: Units::new(version),
        },
        amount: money(amount),
        final_usage,
        raw: raw.spec.id,
        correction: None,
    }
}
async fn start(
    store: &mut Store,
    scope: &Scope,
    artifact: &ArtifactDescriptor,
    cost: u64,
) -> Attempt {
    let input = admission(store, scope, artifact, cost);
    let attempt = reserve(store, input, &actor()).await.unwrap();
    submit(store, &attempt.id, scope, attempt.revision, &actor())
        .await
        .unwrap();
    attempt
}
#[test]
fn rounding_overflow_units_unknown_prices_and_disjoint_subtotals() {
    assert_eq!(
        rate(
            &Rate {
                micros: Micros::new(1),
                per_units: Units::new(3)
            },
            Units::new(1)
        )
        .unwrap()
        .get(),
        1
    );
    for units in 0..1000 {
        let result = rate(
            &Rate {
                micros: Micros::new(17),
                per_units: Units::new(13),
            },
            Units::new(units),
        )
        .unwrap()
        .get();
        assert_eq!(result, (units * 17 + 12) / 13);
    }
    assert!(rate(
        &Rate {
            micros: Micros::new(u64::MAX),
            per_units: Units::new(1)
        },
        Units::new(2)
    )
    .is_err());
    assert!(add(Micros::new(u64::MAX), Micros::new(1)).is_err());
    let mut prices = price(1);
    prices.rates.remove(&ChargeCategory::CacheRead);
    assert!(quote(prices, Usage::default(), actor().now).is_err());
    let usage = Usage {
        input: Units::new(100),
        cache_read: Units::new(40),
        cache_write: Units::new(10),
        output: Units::new(80),
        reasoning: Units::new(30),
        requests: Units::new(1),
        provider_tools: Units::ZERO,
    };
    let charges = usage.disjoint().unwrap();
    assert_eq!(charges[&ChargeCategory::Input].get(), 50);
    assert_eq!(charges[&ChargeCategory::Output].get(), 80);
    assert!(Usage {
        input: Units::new(1),
        ..usage.clone()
    }
    .disjoint()
    .is_err());
    assert!(Usage {
        reasoning: Units::new(81),
        ..usage
    }
    .disjoint()
    .is_err());
    assert_eq!(day(Timestamp::new(86_399_999), 0).unwrap(), 0);
    assert_eq!(day(Timestamp::new(86_400_000), 0).unwrap(), 1);
    assert_eq!(day(Timestamp::ZERO, -60).unwrap(), -1);
    assert_eq!(
        serde_json::to_string(&money(u64::MAX)).unwrap(),
        "{\"currency\":\"USD\",\"micros\":\"18446744073709551615\"}"
    );
}
#[tokio::test]
async fn reserve_submit_lost_reply_unknown_reopen_migration_and_late_overrun() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("root");
        let mut store = setup(&root, kind, 100, 10).await;
        let scope = common::task().scope;
        let request = capture(&mut store, &scope, b"synthetic request").await;
        let input = admission(&store, &scope, &request, 60);
        let first = reserve(&mut store, input.clone(), &actor()).await.unwrap();
        assert_eq!(
            reserve(&mut store, input.clone(), &actor()).await.unwrap(),
            first
        );
        let mut changed = input;
        changed.quote = quote(
            price(61),
            Usage {
                requests: Units::new(1),
                ..Default::default()
            },
            actor().now,
        )
        .unwrap();
        assert!(reserve(&mut store, changed, &actor()).await.is_err());
        let permit = submit(&mut store, &first.id, &scope, Revision::ZERO, &actor())
            .await
            .unwrap();
        assert_eq!(permit.attempt(), &first.id);
        assert_eq!(permit.request_digest(), &request.sha256);
        assert!(permit.receipt().watermark > Watermark::ZERO);
        assert!(
            submit(&mut store, &first.id, &scope, Revision::ZERO, &actor())
                .await
                .is_err()
        );
        assert!(release_before_send(&mut store, &first.id, &scope, &actor())
            .await
            .is_err());
        hold_uncertain(
            &mut store,
            &first.id,
            &scope,
            &actor(),
            "transport disappeared after durable send intent",
        )
        .await
        .unwrap();
        assert_eq!(ledger(store.state(), &scope).unwrap().unresolved.get(), 60);
        drop(store);
        let mut store = Store::open(&root, kind, &[]).await.unwrap();
        let target = temporary.path().join("converted");
        let converted = store
            .convert(
                &target,
                if kind == BackendKind::Sqlite {
                    BackendKind::Files
                } else {
                    BackendKind::Sqlite
                },
                &[],
            )
            .await
            .unwrap();
        assert_eq!(
            ledger(converted.state(), &scope).unwrap().unresolved.get(),
            60
        );
        drop(converted);
        let observation = usage(&mut store, &first, 120, 1, true).await;
        let settlement = observe(&mut store, observation.clone(), &actor())
            .await
            .unwrap();
        assert_eq!(
            observe(&mut store, observation, &actor()).await.unwrap(),
            settlement
        );
        let root = ledger(store.state(), &scope).unwrap();
        assert_eq!(root.settled.get(), 120);
        assert_eq!(root.unresolved.get(), 0);
        assert!(root.overrun);
        let blocked = admission(&store, &scope, &request, 1);
        assert!(reserve(&mut store, blocked, &actor()).await.is_err());
    }
}
#[tokio::test]
async fn root_race_accounts_for_settled_active_unknown_and_protected_money_once() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = setup(temporary.path(), kind, 2_000_000, 100_000).await;
        let scope = common::task().scope;
        let request = capture(&mut store, &scope, b"request").await;
        let billed = start(&mut store, &scope, &request, 400_000).await;
        let observation = usage(&mut store, &billed, 400_000, 1, true).await;
        observe(&mut store, observation, &actor()).await.unwrap();
        let active = admission(&store, &scope, &request, 300_000);
        reserve(&mut store, active, &actor()).await.unwrap();
        let unknown = start(&mut store, &scope, &request, 200_000).await;
        hold_uncertain(&mut store, &unknown.id, &scope, &actor(), "lost stream")
            .await
            .unwrap();
        let root = ledger(store.state(), &scope).unwrap();
        assert_eq!(
            (
                root.settled.get(),
                root.active.get(),
                root.unresolved.get(),
                root.protected.get()
            ),
            (400_000, 300_000, 200_000, 100_000)
        );
        let a = admission(&store, &scope, &request, 650_000);
        let b = admission(&store, &scope, &request, 650_000);
        let (ta, _) = prepare_admission(store.state(), &a, &actor()).unwrap();
        let (tb, _) = prepare_admission(store.state(), &b, &actor()).unwrap();
        let shared = std::sync::Arc::new(tokio::sync::Mutex::new(store));
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let mut jobs = Vec::new();
        for transaction in [ta, tb] {
            let shared = shared.clone();
            let barrier = barrier.clone();
            jobs.push(tokio::spawn(async move {
                barrier.wait().await;
                shared.lock().await.transact(transaction).await.is_ok()
            }));
        }
        let mut admitted = 0;
        for job in jobs {
            admitted += usize::from(job.await.unwrap());
        }
        assert_eq!(admitted, 1);
        let mut store = shared.lock().await;
        let retry = admission(&store, &scope, &request, 650_000);
        assert!(reserve(&mut *store, retry, &actor()).await.is_err());
        assert_eq!(ledger(store.state(), &scope).unwrap().active.get(), 950_000);
    }
}
#[tokio::test]
async fn cumulative_corrections_and_incremental_coverage_cannot_double_charge() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = setup(temporary.path(), BackendKind::Sqlite, 1000, 0).await;
    let scope = common::task().scope;
    let request = capture(&mut store, &scope, b"request").await;
    let first = start(&mut store, &scope, &request, 100).await;
    let one = usage(&mut store, &first, 40, 1, false).await;
    observe(&mut store, one, &actor()).await.unwrap();
    let two = usage(&mut store, &first, 50, 2, true).await;
    observe(&mut store, two, &actor()).await.unwrap();
    let stale = usage(&mut store, &first, 40, 1, true).await;
    assert!(!observe(&mut store, stale, &actor()).await.unwrap().applied);
    assert_eq!(ledger(store.state(), &scope).unwrap().settled.get(), 50);
    let mut correction = usage(&mut store, &first, 30, 3, true).await;
    assert!(observe(&mut store, correction.clone(), &actor())
        .await
        .is_err());
    correction.correction = Some(Resolution {
        actor: actor().id,
        policy: PolicyRevision::ZERO,
        reason: "provider corrected cumulative charge".into(),
        remaining_uncertainty: String::new(),
    });
    assert_eq!(
        observe(&mut store, correction, &actor())
            .await
            .unwrap()
            .direction,
        AdjustmentDirection::Credit
    );
    assert_eq!(ledger(store.state(), &scope).unwrap().settled.get(), 30);
    let second = start(&mut store, &scope, &request, 100).await;
    let mut incremental = usage(&mut store, &second, 20, 1, false).await;
    incremental.mode = UsageMode::Incremental {
        start: Units::ZERO,
        end: Units::new(1),
    };
    observe(&mut store, incremental.clone(), &actor())
        .await
        .unwrap();
    assert!(observe(&mut store, incremental, &actor()).await.is_ok());
    let mut overlap = usage(&mut store, &second, 20, 1, false).await;
    overlap.mode = UsageMode::Incremental {
        start: Units::ZERO,
        end: Units::new(1),
    };
    assert!(observe(&mut store, overlap, &actor()).await.is_err());
    let mut next = usage(&mut store, &second, 30, 2, true).await;
    next.mode = UsageMode::Incremental {
        start: Units::new(1),
        end: Units::new(2),
    };
    observe(&mut store, next, &actor()).await.unwrap();
    assert_eq!(ledger(store.state(), &scope).unwrap().settled.get(), 80);
}
#[tokio::test]
async fn protected_draw_is_not_double_counted_and_policy_reduction_preserves_liability() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = setup(temporary.path(), BackendKind::Files, 100, 100).await;
    let scope = common::task().scope;
    let request = capture(&mut store, &scope, b"verification request").await;
    let mut input = admission(&store, &scope, &request, 80);
    assert!(reserve(&mut store, input.clone(), &actor()).await.is_err());
    input.draw_protected = true;
    assert!(reserve(&mut store, input.clone(), &actor()).await.is_err());
    input.role = RequestRole::Verification;
    let admitted = reserve(&mut store, input, &actor()).await.unwrap();
    let root = ledger(store.state(), &scope).unwrap();
    assert_eq!((root.active.get(), root.protected.get()), (80, 20));
    release_before_send(&mut store, &admitted.id, &scope, &actor())
        .await
        .unwrap();
    let root = ledger(store.state(), &scope).unwrap();
    assert_eq!((root.active.get(), root.protected.get()), (0, 100));
    configure(
        &mut store,
        &scope,
        root.revision,
        money(100),
        Micros::ZERO,
        BTreeMap::new(),
        &actor(),
        "allocate normal work",
    )
    .await
    .unwrap();
    let next = start(&mut store, &scope, &request, 80).await;
    let root = ledger(store.state(), &scope).unwrap();
    configure(
        &mut store,
        &scope,
        root.revision,
        money(50),
        Micros::ZERO,
        BTreeMap::new(),
        &actor(),
        "owner lowered cap",
    )
    .await
    .unwrap();
    hold_uncertain(&mut store, &next.id, &scope, &actor(), "owner closed")
        .await
        .unwrap();
    let root = ledger(store.state(), &scope).unwrap();
    assert_eq!(root.unresolved.get(), 80);
    assert!(root.overrun);
}

#[tokio::test]
async fn child_allocations_subdivide_one_root_and_all_roles_require_admission() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = setup(temporary.path(), BackendKind::Sqlite, 1000, 0).await;
    let scope = common::task().scope;
    let mut child = common::task();
    child.scope.task = TaskId::new();
    child.parent = Some(scope.task.clone());
    child.state = TaskState::Running;
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Task,
                    child.scope.task.to_string(),
                    scope.workspace.clone(),
                    Revision::ZERO,
                    &child,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let current = ledger(store.state(), &scope).unwrap();
    configure(
        &mut store,
        &scope,
        current.revision,
        money(1000),
        Micros::ZERO,
        BTreeMap::from([(child.scope.task.clone(), Micros::new(150))]),
        &actor(),
        "child allocation",
    )
    .await
    .unwrap();
    let request = capture(&mut store, &child.scope, b"child request").await;
    let mut first = admission(&store, &child.scope, &request, 100);
    first.role = RequestRole::Child;
    reserve(&mut store, first, &actor()).await.unwrap();
    assert_eq!(ledger(store.state(), &scope).unwrap().active.get(), 100);
    let denied = admission(&store, &child.scope, &request, 100);
    assert!(reserve(&mut store, denied, &actor()).await.is_err());
    assert!(initialize(
        &mut store,
        child.scope.clone(),
        money(99999),
        Micros::ZERO,
        None,
        &actor()
    )
    .await
    .is_err());
    let root_request = capture(&mut store, &scope, b"role request").await;
    for role in [
        RequestRole::Main,
        RequestRole::Helper,
        RequestRole::Compaction,
        RequestRole::Reviewer,
        RequestRole::Optimizer,
        RequestRole::Memory,
        RequestRole::Verification,
    ] {
        let mut input = admission(&store, &scope, &root_request, 100);
        input.role = role;
        reserve(&mut store, input, &actor()).await.unwrap();
    }
    assert_eq!(ledger(store.state(), &scope).unwrap().active.get(), 800);
    let before = ledger(store.state(), &scope).unwrap();
    let local = LocalResources {
        schema_version: 1,
        id: ObservationId::new(),
        scope: child.scope,
        agent: AgentId::new(),
        cpu_millis: Units::new(500),
        peak_ram: ByteCount::new(1024),
        disk: ByteCount::new(4096),
        source: "local-embedding".into(),
    };
    record_local_resources(&mut store, local.clone(), &actor())
        .await
        .unwrap();
    record_local_resources(&mut store, local, &actor())
        .await
        .unwrap();
    assert_eq!(ledger(store.state(), &scope).unwrap(), before);
}

#[tokio::test]
async fn daily_scope_is_explicit_and_midnight_does_not_erase_open_liabilities() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = Store::open(temporary.path(), BackendKind::Files, &[])
        .await
        .unwrap();
    let mut initial = common::initial();
    for mutation in &mut initial.mutations {
        if let Mutation::Put { record, .. } = mutation {
            if record.collection == Collection::Task {
                let mut task: Task = record.decode().unwrap();
                task.state = TaskState::Running;
                record.value = serde_json::to_value(task).unwrap();
            }
        }
    }
    store.transact(initial).await.unwrap();
    let scope = common::task().scope;
    initialize(
        &mut store,
        scope.clone(),
        money(1000),
        Micros::ZERO,
        Some(DailyPolicy {
            cap: Micros::new(100),
            utc_offset_minutes: 0,
            scope: "local_root".into(),
        }),
        &actor(),
    )
    .await
    .unwrap();
    let request = capture(&mut store, &scope, b"daily request").await;
    let first = start(&mut store, &scope, &request, 60).await;
    hold_uncertain(
        &mut store,
        &first.id,
        &scope,
        &actor(),
        "yesterday's lost stream",
    )
    .await
    .unwrap();
    let mut tomorrow = admission(&store, &scope, &request, 60);
    tomorrow.now = Timestamp::new(86_400_001);
    let tomorrow_actor = Actor {
        id: actor().id,
        now: tomorrow.now,
    };
    assert!(reserve(&mut store, tomorrow, &tomorrow_actor)
        .await
        .is_err());
    assert_eq!(ledger(store.state(), &scope).unwrap().unresolved.get(), 60);
}

#[tokio::test]
async fn uncertain_retry_explicit_resolution_and_currency_conflicts_keep_provenance() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = setup(temporary.path(), BackendKind::Files, 500, 0).await;
    let scope = common::task().scope;
    let request = capture(&mut store, &scope, b"retry request").await;
    let first = start(&mut store, &scope, &request, 100).await;
    hold_uncertain(&mut store, &first.id, &scope, &actor(), "response missing")
        .await
        .unwrap();
    let mut retry = admission(&store, &scope, &request, 100);
    retry.previous = Some(first.id.clone());
    let second = reserve(&mut store, retry, &actor()).await.unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.previous, Some(first.id.clone()));
    assert_eq!(
        (
            ledger(store.state(), &scope).unwrap().unresolved.get(),
            ledger(store.state(), &scope).unwrap().active.get()
        ),
        (100, 100)
    );
    let mut resolution = usage(&mut store, &first, 80, 1, true).await;
    resolution.correction = Some(Resolution {
        actor: actor().id,
        policy: PolicyRevision::ZERO,
        reason: "owner accepts conservative estimated charge".into(),
        remaining_uncertainty: "provider invoice unavailable".into(),
    });
    observe(&mut store, resolution, &actor()).await.unwrap();
    let resolved = attempt(store.state(), &first.id, &scope.workspace).unwrap();
    assert_eq!(resolved.phase, ReservationState::ExplicitlyResolved);
    assert!(resolved.uncertain.is_some());
    assert_eq!(ledger(store.state(), &scope).unwrap().settled.get(), 80);
    let mut wrong = usage(&mut store, &first, 90, 2, true).await;
    wrong.amount.currency = "EUR".to_owned().try_into().unwrap();
    assert!(observe(&mut store, wrong, &actor()).await.is_err());
    let actual = usage(&mut store, &first, 90, 3, true).await;
    observe(&mut store, actual, &actor()).await.unwrap();
    let observed = attempt(store.state(), &first.id, &scope.workspace).unwrap();
    assert_eq!(observed.phase, ReservationState::Settled);
    assert!(observed.uncertain.is_none());
    assert_eq!(ledger(store.state(), &scope).unwrap().settled.get(), 90);
}
