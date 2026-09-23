// SPDX-License-Identifier: Apache-2.0
//! Bounded M9 accounting traces. No transport, controller dispatch or crash claim.
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;
use vcp_budget::{arithmetic::quote, *};
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, task::*};
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};

const SEEDS: [u64; 4] = [1, 0x5eed, 0x8a02, 0xc0ffee];
const CAP: u64 = 100_000;
fn actor() -> Actor {
    Actor {
        id: ActorId::parse("seeded-owner").unwrap(),
        now: Timestamp::new(1000),
    }
}
fn money(amount: u64) -> Money {
    Money {
        currency: "USD".to_owned().try_into().unwrap(),
        micros: Micros::new(amount),
    }
}
fn price(amount: u64) -> PriceSnapshot {
    PriceSnapshot {
        id: "a".repeat(64),
        provider: "synthetic-accounting-no-transport".into(),
        model: "none".into(),
        currency: money(0).currency,
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
                        amount
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
async fn setup(path: &std::path::Path, backend: BackendKind) -> Store {
    let mut store = Store::open(path, backend, &[]).await.unwrap();
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
        serde_json::json!({"state":"running","fixture":"seeded accounting; no provider"});
    store.transact(initial).await.unwrap();
    initialize(
        &mut store,
        common::task().scope,
        money(CAP),
        Micros::ZERO,
        None,
        &actor(),
    )
    .await
    .unwrap();
    store
}
async fn capture(store: &mut Store, bytes: &[u8]) -> ArtifactDescriptor {
    let mut spec = common::spec();
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
fn admission(store: &Store, artifact: &ArtifactDescriptor, amount: u64) -> Admission {
    let scope = common::task().scope;
    let current = ledger(store.state(), &scope).unwrap();
    Admission {
        transaction: TransactionId::new(),
        attempt: AttemptId::new(),
        reservation: ReservationId::new(),
        scope,
        agent: AgentId::new(),
        role: RequestRole::Main,
        request: artifact.spec.id.clone(),
        request_digest: artifact.sha256.clone(),
        quote: quote(
            price(amount),
            Usage {
                requests: Units::new(1),
                ..Default::default()
            },
            actor().now,
        )
        .unwrap(),
        previous: None,
        expected_ledger: current.revision,
        policy: current.policy,
        steering: SteeringRevision::ZERO,
        draw_protected: false,
        now: actor().now,
    }
}
async fn usage(
    store: &mut Store,
    id: &AttemptId,
    amount: u64,
    version: u64,
    final_usage: bool,
) -> UsageObservation {
    let raw = capture(
        store,
        format!("invented usage amount={amount}; version={version}").as_bytes(),
    )
    .await;
    UsageObservation {
        id: ObservationId::new(),
        scope: common::task().scope,
        attempt: id.clone(),
        provider_request: format!("invented-{id}"),
        mode: UsageMode::Cumulative {
            version: Units::new(version),
        },
        amount: money(amount),
        final_usage,
        raw: raw.spec.id,
        correction: None,
    }
}

#[derive(Debug)]
struct Episode {
    quote: u64,
    partial: u64,
    final_bill: u64,
    release: bool,
    extra_reopen: bool,
}
fn generated(seed: u64) -> Vec<Episode> {
    let mut random = seed;
    let mut next = || {
        random = random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        random >> 32
    };
    (0..6)
        .map(|index| {
            let quote = 40 + next() % 100;
            let partial = 1 + next() % (quote - 1);
            let final_bill = if index == 5 {
                CAP + 500
            } else {
                quote + 1 + next() % 30
            };
            Episode {
                quote,
                partial,
                final_bill,
                release: index == 0 || (index != 5 && next() % 4 == 0),
                extra_reopen: next() % 2 == 0,
            }
        })
        .collect()
}

// The arithmetic oracle only receives generated amounts and acknowledged
// operation kinds; it never reads a product ledger/reservation to derive totals.
#[derive(Default)]
struct Expected {
    settled: u64,
    active: u64,
    unresolved: u64,
}
impl Expected {
    fn check(&self, store: &Store) {
        let actual = ledger(store.state(), &common::task().scope).unwrap();
        assert_eq!(
            (
                actual.settled.get(),
                actual.active.get(),
                actual.unresolved.get()
            ),
            (self.settled, self.active, self.unresolved)
        );
        assert_eq!(
            actual.overrun,
            self.settled + self.active + self.unresolved > CAP
        );
    }
}
fn attempt_state(store: &Store, id: &AttemptId, phase: ReservationState, charged: u64) {
    let observed = attempt(store.state(), id, &common::workspace().id).unwrap();
    assert_eq!(observed.phase, phase);
    assert_eq!(observed.charged.get(), charged);
}
async fn reopen(
    store: Store,
    path: &std::path::Path,
    backend: BackendKind,
    expected: &Expected,
) -> Store {
    let acknowledged = store.state().clone();
    store.close().await.unwrap();
    let store = Store::open(path, backend, &[]).await.unwrap();
    assert_eq!(
        store.state(),
        &acknowledged,
        "cooperative reopen lost acknowledged state"
    );
    expected.check(&store);
    store
}

#[tokio::test]
async fn seeded_late_usage_traces_preserve_independent_liability_arithmetic() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for seed in SEEDS {
            let episodes = generated(seed);
            eprintln!("M9 accounting backend={backend:?} seed={seed:#x} episodes={episodes:?}");
            let temp = tempfile::tempdir().unwrap();
            let mut store = setup(temp.path(), backend).await;
            let request = capture(&mut store, b"invented request; no transport").await;
            let scope = common::task().scope;
            let mut expected = Expected::default();
            let (mut releases, mut finals, mut denied, mut duplicates, mut stale) = (0, 0, 0, 0, 0);
            for (index, episode) in episodes.iter().enumerate() {
                macro_rules! step { ($name:expr) => { eprintln!("M9 accounting backend={backend:?} seed={seed:#x} prefix_episode={index} operation={}", $name); }; }
                step!("reserve-and-exact-retry");
                let input = admission(&store, &request, episode.quote);
                let started = reserve(&mut store, input.clone(), &actor()).await.unwrap();
                expected.active = episode.quote;
                expected.check(&store);
                attempt_state(&store, &started.id, ReservationState::Created, 0);
                let checkpoint = store.state().clone();
                assert_eq!(
                    reserve(&mut store, input.clone(), &actor()).await.unwrap(),
                    started
                );
                assert_eq!(store.state(), &checkpoint);
                duplicates += 1;
                let mut changed = input;
                changed.request_digest = "c".repeat(64);
                assert!(reserve(&mut store, changed, &actor()).await.is_err());
                assert_eq!(store.state(), &checkpoint);
                denied += 1;

                step!("unknown-before-send-is-denied");
                assert!(hold_uncertain(
                    &mut store,
                    &started.id,
                    &scope,
                    &actor(),
                    "invented missing reply"
                )
                .await
                .is_err());
                assert_eq!(store.state(), &checkpoint);
                denied += 1;
                if episode.release {
                    step!("release-before-send-and-reopen");
                    release_before_send(&mut store, &started.id, &scope, &actor())
                        .await
                        .unwrap();
                    expected.active = 0;
                    expected.check(&store);
                    attempt_state(&store, &started.id, ReservationState::Released, 0);
                    let checkpoint = store.state().clone();
                    release_before_send(&mut store, &started.id, &scope, &actor())
                        .await
                        .unwrap();
                    assert_eq!(store.state(), &checkpoint);
                    duplicates += 1;
                    releases += 1;
                    store = reopen(store, temp.path(), backend, &expected).await;
                    continue;
                }
                step!("submit-and-denied-replay-release");
                let permit = submit(&mut store, &started.id, &scope, started.revision, &actor())
                    .await
                    .unwrap();
                assert_eq!(permit.attempt(), &started.id);
                assert_eq!(permit.request_digest(), &request.sha256);
                expected.check(&store);
                attempt_state(&store, &started.id, ReservationState::Submitted, 0);
                let checkpoint = store.state().clone();
                assert!(
                    submit(&mut store, &started.id, &scope, started.revision, &actor())
                        .await
                        .is_err()
                );
                assert_eq!(store.state(), &checkpoint);
                assert!(
                    release_before_send(&mut store, &started.id, &scope, &actor())
                        .await
                        .is_err()
                );
                assert_eq!(store.state(), &checkpoint);
                denied += 2;
                step!("hold-unknown-and-reopen");
                hold_uncertain(
                    &mut store,
                    &started.id,
                    &scope,
                    &actor(),
                    "invented missing reply; no transport",
                )
                .await
                .unwrap();
                expected.active = 0;
                expected.unresolved = episode.quote;
                expected.check(&store);
                attempt_state(
                    &store,
                    &started.id,
                    ReservationState::ReconciliationPending,
                    0,
                );
                store = reopen(store, temp.path(), backend, &expected).await;

                step!("partial-cumulative-usage");
                let partial = usage(&mut store, &started.id, episode.partial, 1, false).await;
                assert!(
                    observe(&mut store, partial, &actor())
                        .await
                        .unwrap()
                        .applied
                );
                expected.settled += episode.partial;
                expected.unresolved = episode.quote - episode.partial;
                expected.check(&store);
                attempt_state(
                    &store,
                    &started.id,
                    ReservationState::ReconciliationPending,
                    episode.partial,
                );
                if episode.extra_reopen {
                    store = reopen(store, temp.path(), backend, &expected).await;
                }

                step!("late-final-usage-and-exact-retry");
                let final_usage = usage(&mut store, &started.id, episode.final_bill, 2, true).await;
                let settlement = observe(&mut store, final_usage.clone(), &actor())
                    .await
                    .unwrap();
                assert!(settlement.applied);
                expected.settled += episode.final_bill - episode.partial;
                expected.unresolved = 0;
                expected.check(&store);
                attempt_state(
                    &store,
                    &started.id,
                    ReservationState::Settled,
                    episode.final_bill,
                );
                let checkpoint = store.state().clone();
                assert_eq!(
                    observe(&mut store, final_usage.clone(), &actor())
                        .await
                        .unwrap(),
                    settlement
                );
                assert_eq!(store.state(), &checkpoint);
                duplicates += 1;
                let mut conflict = final_usage;
                conflict.amount = money(episode.final_bill + 1);
                assert!(observe(&mut store, conflict, &actor()).await.is_err());
                assert_eq!(store.state(), &checkpoint);
                denied += 1;

                step!("new-id-stale-usage-is-audited-without-double-charge");
                let stale_usage = usage(&mut store, &started.id, episode.partial, 1, true).await;
                let prior = store.state().clone();
                let stale_settlement = observe(&mut store, stale_usage, &actor()).await.unwrap();
                assert!(!stale_settlement.applied);
                assert_eq!(stale_settlement.total.get(), episode.final_bill);
                assert_eq!(stale_settlement.adjustment.get(), 0);
                attempt_state(
                    &store,
                    &started.id,
                    ReservationState::Settled,
                    episode.final_bill,
                );
                assert_eq!(
                    &store.state().events[..prior.events.len()],
                    prior.events.as_slice()
                );
                for (id, receipt) in &prior.commands {
                    assert_eq!(store.state().commands.get(id), Some(receipt));
                }
                expected.check(&store);
                stale += 1;
                finals += 1;
                store = reopen(store, temp.path(), backend, &expected).await;
            }
            assert!(releases >= 1 && finals >= 1 && denied >= 12 && duplicates >= 6 && stale >= 1);
            assert!(expected.settled > CAP);
            let blocked = admission(&store, &request, 1);
            let checkpoint = store.state().clone();
            assert!(reserve(&mut store, blocked, &actor()).await.is_err());
            assert_eq!(
                store.state(),
                &checkpoint,
                "late overrun must deny fresh admission without mutation"
            );
            denied += 1;
            assert_eq!(releases + finals, 6);
            assert_eq!(duplicates, 12);
            assert_eq!(stale, finals);
            assert_eq!(denied, 2 * releases + 5 * finals + 1);
            let operations = 6 * releases + 13 * finals + 1;
            let reopens = releases
                + 2 * finals
                + episodes
                    .iter()
                    .filter(|episode| !episode.release && episode.extra_reopen)
                    .count();
            eprintln!("M9 accounting backend={backend:?} seed={seed:#x} releases={releases} finals={finals} denied={denied} exact_retries={duplicates} audited_stale={stale} budget_api_attempts={operations} cooperative_reopens={reopens}");
            expected.check(&store);
            store.close().await.unwrap();
        }
    }
}
