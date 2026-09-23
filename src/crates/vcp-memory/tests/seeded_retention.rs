// SPDX-License-Identifier: Apache-2.0
//! M9 partial campaign: fixed invented traces over real retention and stores.
//! Four seeds x two backends x 28 steps, followed by authenticated opposite-
//! backend restore. No model, process kill, physical erasure, or latency claim.
//! Replay/shrink with VCP_RETENTION_TRACE_SEED (hex) and
//! VCP_RETENTION_TRACE_PREFIX (0..=28); each attempted prefix is printed.
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;

use std::{collections::BTreeSet, path::Path};
use vcp_domain::{
    memory::{ClaimKind, Outcome},
    retention_selector::{Criterion, Selector, Tree},
    task::TaskState,
    workspace::{Trust, Workspace},
    *,
};
use vcp_memory::{
    access::Access,
    retention::{self, Action},
    search_record::{self, ChunkerSpec, TextSource},
};
use vcp_store::{contract::*, BackendKind, Store};

const SEEDS: [u64; 4] = [0x5eed, 0xc0ffee, 0xbadc0de, 0x1];
const STEPS: usize = 28;
const MARKERS: [&str; 2] = ["seeded-orchard-evidence", "seeded-ocean-evidence"];

fn copy_access(access: &Access) -> Access {
    Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: access.write,
        tasks: access.tasks.clone(),
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Exclude(usize),
    Restore(usize),
    Compact(usize),
    Purge(usize),
    Stale(usize),
    WrongScope(usize),
    Reopen,
}

// Generator knows no retention state or product outcome. Illegal operations
// deliberately remain in the trace rather than being repaired or resampled.
fn trace(mut seed: u64) -> Vec<Op> {
    assert_ne!(seed, 0, "xorshift seed must be nonzero");
    let mut operations = vec![
        Op::Exclude(0),
        Op::Reopen,
        Op::Restore(0),
        Op::Compact(1),
        Op::Stale(1),
        Op::WrongScope(0),
        Op::Purge(0),
        Op::Restore(0),
        Op::Reopen,
        Op::Exclude(1),
        Op::Restore(1),
        Op::Stale(1),
    ];
    for _ in operations.len()..STEPS {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let slot = ((seed >> 8) & 1) as usize;
        operations.push(match seed % 7 {
            0 => Op::Exclude(slot),
            1 => Op::Restore(slot),
            2 => Op::Compact(slot),
            3 => Op::Purge(slot),
            4 => Op::Stale(slot),
            5 => Op::WrongScope(slot),
            _ => Op::Reopen,
        });
    }
    operations
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visibility {
    Visible,
    Hidden,
    Erased,
}

// Independent semantic oracle: no calls to preview, decision, recall_allowed,
// history, or search. Erasure is absorbing; compaction cannot change recall.
fn next(current: Visibility, operation: Op) -> Option<Visibility> {
    if current == Visibility::Erased {
        // Repeating erasure is legal; no later operation may restore recall.
        // Retention's durable purge contract permits a fresh purge receipt.
        return matches!(operation, Op::Purge(_)).then_some(Visibility::Erased);
    }
    match operation {
        Op::Exclude(_) => Some(Visibility::Hidden),
        Op::Restore(_) => Some(Visibility::Visible),
        Op::Compact(_) => Some(current),
        Op::Purge(_) => Some(Visibility::Erased),
        _ => panic!("non-retention operation passed to oracle"),
    }
}

struct Corpus {
    access: Access,
    tasks: [TaskId; 2],
    versions: [ClaimVersionId; 2],
}

async fn seed(root: &Path, backend: BackendKind) -> (Store, Corpus) {
    let mut store = Store::open(root, backend, &[]).await.unwrap();
    let mut initial = common::initial();
    let mut first = common::task();
    first.state = TaskState::Cancelled;
    first.objectives[0].text =
        serde_json::json!({"memory_preference":{"key":"orchard","value":MARKERS[0]}}).to_string();
    initial.mutations[2] = Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Task,
            first.scope.task.to_string(),
            first.scope.workspace.clone(),
            first.revision,
            &first,
        )
        .unwrap(),
    };
    initial.events[0].data =
        serde_json::json!({"facts":[{"collection":"task","id":first.scope.task,"value":first}]});
    store.transact(initial).await.unwrap();
    let origin = store.state().events.last().unwrap().clone();
    let access = Access {
        workspace: first.scope.workspace.clone(),
        actor: ActorId::parse("human").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let proposal = vcp_memory::preferences::materialize(&mut store, &access, &origin)
        .await
        .unwrap()
        .unwrap();
    let committed =
        vcp_memory::repository::propose(&mut store, &access, proposal, Timestamp::new(200))
            .await
            .unwrap();
    assert_eq!(committed.result.resolution.outcome, Outcome::Accepted);
    let first_version = committed.result.version.unwrap();

    let mut second = first.clone();
    second.scope.task = TaskId::parse("other-task").unwrap();
    second.root = second.scope.task.clone();
    second.cause = EventId::parse("other-created").unwrap();
    second.objectives[0].source = second.cause.clone();
    second.objectives[0].text =
        serde_json::json!({"memory_preference":{"key":"ocean","value":MARKERS[1]}}).to_string();
    let mut event = origin.event;
    event.id = second.cause.clone();
    event.task = Some(second.scope.task.clone());
    event.data =
        serde_json::json!({"facts":[{"collection":"task","id":second.scope.task,"value":second}]});
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Task,
                    second.scope.task.to_string(),
                    access.workspace.clone(),
                    second.revision,
                    &second,
                )
                .unwrap(),
            }],
            events: vec![event],
            command: None,
        })
        .await
        .unwrap();
    let origin = store.state().events.last().unwrap().clone();
    let proposal = vcp_memory::preferences::materialize(&mut store, &access, &origin)
        .await
        .unwrap()
        .unwrap();
    let committed =
        vcp_memory::repository::propose(&mut store, &access, proposal, Timestamp::new(300))
            .await
            .unwrap();
    assert_eq!(committed.result.resolution.outcome, Outcome::Accepted);
    (
        store,
        Corpus {
            access,
            tasks: [first.scope.task, second.scope.task],
            versions: [first_version, committed.result.version.unwrap()],
        },
    )
}

fn selector(corpus: &Corpus, slot: usize) -> Selector {
    Selector {
        schema_version: 1,
        tree: Tree::All(vec![
            Tree::Match(Criterion::Workspace(corpus.access.workspace.clone())),
            Tree::Match(Criterion::Task(corpus.tasks[slot].clone())),
            Tree::Match(Criterion::Claim(ClaimKind::UserPreference)),
        ]),
    }
}

fn check(store: &Store, corpus: &Corpus, expected: &[Visibility; 2]) {
    let before = store.state().clone();
    for allowed in [
        None,
        Some(BTreeSet::from([corpus.tasks[0].clone()])),
        Some(BTreeSet::from([corpus.tasks[1].clone()])),
        Some(BTreeSet::new()),
    ] {
        let access = Access {
            tasks: allowed.clone(),
            ..copy_access(&corpus.access)
        };
        let observed = search_record::inventory(
            store,
            &access,
            &[],
            &ChunkerSpec::default(),
            search_record::Limits::default(),
        )
        .unwrap();
        let wanted: BTreeSet<_> = (0..2)
            .filter(|slot| {
                expected[*slot] == Visibility::Visible
                    && allowed
                        .as_ref()
                        .is_none_or(|tasks| tasks.contains(&corpus.tasks[*slot]))
            })
            .map(|slot| corpus.versions[slot].clone())
            .collect();
        let actual: BTreeSet<_> = observed
            .records
            .iter()
            .map(|record| {
                assert_eq!(record.scope.workspace, corpus.access.workspace);
                let TextSource::Claim { version, .. } = &record.source else {
                    panic!("unexpected nonclaim source")
                };
                let slot = corpus
                    .versions
                    .iter()
                    .position(|id| id == version)
                    .expect("unexpected claim identity");
                assert_eq!(record.scope.task, corpus.tasks[slot]);
                assert!(record.text.contains(MARKERS[slot]));
                assert!(!record.text.contains(MARKERS[1 - slot]));
                version.clone()
            })
            .collect();
        assert_eq!(
            actual, wanted,
            "recall differs from independent visibility state {expected:?}"
        );
        assert_eq!(
            observed.records.len(),
            actual.len(),
            "duplicate source records"
        );
        let encoded = serde_json::to_string(&observed).unwrap();
        for slot in 0..2 {
            if allowed
                .as_ref()
                .is_some_and(|tasks| !tasks.contains(&corpus.tasks[slot]))
            {
                assert!(!encoded.contains(MARKERS[slot]), "denied scope leaked text");
                assert!(
                    !encoded.contains(corpus.versions[slot].as_str()),
                    "denied scope leaked claim identity"
                );
            }
            if expected[slot] != Visibility::Visible {
                assert!(
                    observed
                        .records
                        .iter()
                        .all(|record| !record.text.contains(MARKERS[slot])),
                    "excluded/purged text resurrected"
                );
            }
        }
    }
    assert_eq!(
        store.state(),
        &before,
        "visibility inspection wrote canonical state"
    );
    assert!(store.state().records.values().all(|record| !matches!(
        record.collection,
        Collection::Attempt | Collection::Reservation | Collection::Ledger
    )));
}

async fn restore(
    source: &Store,
    corpus: &Corpus,
    expected: &[Visibility; 2],
    root: &Path,
    to: BackendKind,
) {
    use vcp_store::{
        keys::{LocalKeys, RecoveryDirectory},
        portable_snapshot::Archive,
        restore_stage::Restore,
        vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
        vault_publish::{Checkpoint, LocalTrust},
    };
    let original = source.state().clone();
    let snapshot = source.snapshot().unwrap();
    let archive = Archive::capture(source, &snapshot, &corpus.access.workspace, &|| false).unwrap();
    let payloads = archive.payloads().unwrap();
    drop(snapshot);
    let [stage, recovery, vault, acquisition, roots] =
        ["stage", "recovery", "vault", "restore", "roots"].map(|name| root.join(name));
    for path in [&stage, &recovery, &vault, &acquisition, &roots] {
        std::fs::create_dir(path).unwrap();
    }
    let forbidden = vec![vault.clone()];
    let keys = LocalKeys::generate().unwrap();
    let copy = keys
        .export_recovery(&RecoveryDirectory::open(&recovery, &forbidden).unwrap())
        .unwrap();
    let keys = keys.verify_recovery(&copy).unwrap();
    let workspace: Workspace = source
        .state()
        .record(
            Collection::Workspace,
            corpus.access.workspace.as_str(),
            &corpus.access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let trust = LocalTrust::enroll(
        &keys,
        corpus.access.workspace.clone(),
        "d".repeat(64),
        Checkpoint {
            sequence: 0,
            deletion: 0,
            parent: None,
        },
    )
    .unwrap();
    let manifest = Manifest {
        format: FORMAT.into(),
        workspace: corpus.access.workspace.clone(),
        lineage: "d".repeat(64),
        sequence: 1,
        deletion: workspace.deletion.get(),
        parent: None,
        objects: payloads
            .iter()
            .map(|(digest, bytes)| {
                (
                    digest.clone(),
                    Object {
                        bytes: bytes.len() as u64,
                        sha256: digest.clone(),
                    },
                )
            })
            .collect(),
    };
    let mut encrypted = trust
        .encrypt(
            &keys,
            &PrivateStaging::open(&stage, &forbidden).unwrap(),
            manifest,
            payloads,
            trust.configuration().revision,
            Limits::default(),
        )
        .unwrap();
    let operation = CommandId::new();
    let mut restore = Restore::begin(
        &acquisition,
        &forbidden,
        operation.clone(),
        &trust,
        encrypted.sha256().into(),
        encrypted.bytes(),
    )
    .unwrap();
    let object = vault.join("snapshot.age");
    let mut output = std::fs::File::create_new(&object).unwrap();
    encrypted.copy_ciphertext(&mut output).unwrap();
    output.sync_all().unwrap();
    drop(output);
    restore.acquire(&object, &|| false).unwrap();
    drop(restore);
    let mut restore = Restore::open(&acquisition, &forbidden).unwrap();
    let proof = restore
        .authenticate(&trust, &copy, Limits::default(), &|| false)
        .unwrap();
    let imported = restore
        .import(
            &proof,
            &trust,
            to,
            &roots.join(operation.as_str()),
            corpus.access.actor.clone(),
            Timestamp::new(100_000),
            &forbidden,
            &|| false,
        )
        .await
        .unwrap();
    assert!(!restore.status().search_ready);
    let restored = imported.reopen_verified().await.unwrap();
    let target: Workspace = restored
        .state()
        .record(
            Collection::Workspace,
            corpus.access.workspace.as_str(),
            &corpus.access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(target.trust, Trust::Untrusted);
    assert!(target.authority > workspace.authority);
    assert!(
        search_record::inventory(
            &restored,
            &corpus.access,
            &[],
            &ChunkerSpec::default(),
            search_record::Limits::default()
        )
        .is_err(),
        "old authority survived restore"
    );
    let target_corpus = Corpus {
        access: Access {
            authority: target.authority,
            ..copy_access(&corpus.access)
        },
        tasks: corpus.tasks.clone(),
        versions: corpus.versions.clone(),
    };
    check(&restored, &target_corpus, expected);
    for (id, receipt) in &original.transactions {
        assert_eq!(restored.state().transactions.get(id), Some(receipt));
    }
    for (id, receipt) in &original.commands {
        assert_eq!(restored.state().commands.get(id), Some(receipt));
    }
    assert_eq!(
        &restored.state().events[..original.events.len()],
        original.events.as_slice()
    );
    assert_eq!(source.state(), &original, "restore mutated source");
    restored.close().await.unwrap();
}

#[tokio::test]
async fn seeded_retention_reopen_and_authenticated_restore_preserve_oracle() {
    let seeds = std::env::var("VCP_RETENTION_TRACE_SEED")
        .map(|text| vec![u64::from_str_radix(text.trim_start_matches("0x"), 16).expect("hex seed")])
        .unwrap_or_else(|_| SEEDS.to_vec());
    let count = std::env::var("VCP_RETENTION_TRACE_PREFIX")
        .map(|value| value.parse::<usize>().expect("integer prefix"))
        .unwrap_or(STEPS);
    assert!(count <= STEPS);
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for seed_value in &seeds {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("canonical");
            let (mut store, corpus) = seed(&root, backend).await;
            let mut expected = [Visibility::Visible; 2];
            let operations = trace(*seed_value);
            // Replay overrides are debugging aids, not the full qualification campaign.
            let mut coverage = [0usize; 5]; // accepted, refused, stale, scope, reopen
            check(&store, &corpus, &expected);
            for (index, operation) in operations.iter().take(count).enumerate() {
                eprintln!(
                    "retention seed={seed_value:#x} backend={backend:?} prefix={:?}",
                    &operations[..=index]
                );
                let now = Timestamp::new(1000 + index as u64 * 10);
                match *operation {
                    Op::Reopen => {
                        coverage[4] += 1;
                        let state = store.state().clone();
                        store.close().await.unwrap();
                        store = Store::open(&root, backend, &[]).await.unwrap();
                        assert_eq!(store.state(), &state, "reopen changed acknowledged state");
                    }
                    Op::Stale(slot) => {
                        coverage[2] += 1;
                        let preview = retention::preview(
                            &store,
                            &corpus.access,
                            selector(&corpus, slot),
                            Action::Exclude,
                            now,
                        )
                        .unwrap();
                        let revision = store
                            .state()
                            .records
                            .values()
                            .find(|record| {
                                record.value["document_type"] == "vcp_retention_policy_v1"
                            })
                            .map(|record| record.revision);
                        vcp_memory::retention_policy::set(
                            &mut store,
                            &corpus.access,
                            revision,
                            7 + (index as u32 % 2),
                            None,
                            now,
                        )
                        .await
                        .unwrap();
                        let before = store.state().clone();
                        let error = retention::apply(&mut store, &corpus.access, &preview, now)
                            .await
                            .unwrap_err();
                        assert!(
                            error.to_string().contains("stale preview"),
                            "wrong stale diagnosis: {error}"
                        );
                        assert_eq!(store.state(), &before, "stale preview wrote state");
                    }
                    Op::WrongScope(slot) => {
                        coverage[3] += 1;
                        let preview = retention::preview(
                            &store,
                            &corpus.access,
                            selector(&corpus, slot),
                            Action::Exclude,
                            now,
                        )
                        .unwrap();
                        let before = store.state().clone();
                        let denied = Access {
                            workspace: WorkspaceId::parse("foreign-workspace").unwrap(),
                            ..copy_access(&corpus.access)
                        };
                        assert!(retention::apply(&mut store, &denied, &preview, now)
                            .await
                            .is_err());
                        let narrowed = Access {
                            tasks: Some(BTreeSet::from([corpus.tasks[slot].clone()])),
                            ..copy_access(&corpus.access)
                        };
                        assert!(retention::apply(&mut store, &narrowed, &preview, now)
                            .await
                            .is_err());
                        assert_eq!(store.state(), &before, "scope-denied operation wrote state");
                    }
                    op => {
                        let (slot, action) = match op {
                            Op::Exclude(slot) => (slot, Action::Exclude),
                            Op::Restore(slot) => (slot, Action::RestoreRecall),
                            Op::Compact(slot) => (slot, Action::Compact),
                            Op::Purge(slot) => (slot, Action::Purge),
                            _ => unreachable!(),
                        };
                        let preview = retention::preview(
                            &store,
                            &corpus.access,
                            selector(&corpus, slot),
                            action,
                            now,
                        )
                        .unwrap();
                        let before = store.state().clone();
                        let wanted = next(expected[slot], op);
                        let result =
                            retention::apply(&mut store, &corpus.access, &preview, now).await;
                        if let Some(value) = wanted {
                            coverage[0] += 1;
                            let receipt = result.unwrap();
                            expected[slot] = value;
                            // Retrying exactly the acknowledged preview is idempotent.
                            let after = store.state().clone();
                            let replay =
                                retention::apply(&mut store, &corpus.access, &preview, now)
                                    .await
                                    .unwrap();
                            assert_eq!(replay, receipt, "duplicate returned a different receipt");
                            assert_eq!(
                                store.state(),
                                &after,
                                "duplicate retention apply wrote state"
                            );
                        } else {
                            coverage[1] += 1;
                            assert!(result.is_err(), "post-purge mutation accepted");
                            assert_eq!(store.state(), &before, "post-purge refusal wrote state");
                        }
                    }
                }
                check(&store, &corpus, &expected);
            }
            eprintln!("retention seed={seed_value:#x} backend={backend:?} authenticated restore after prefix={:?}",&operations[..count]);
            let to = if backend == BackendKind::Files {
                BackendKind::Sqlite
            } else {
                BackendKind::Files
            };
            restore(&store, &corpus, &expected, temp.path(), to).await;
            store.close().await.unwrap();
            assert_eq!(coverage.iter().sum::<usize>(), count);
            if count == STEPS {
                assert!(coverage.iter().all(|value| *value > 0));
            }
            eprintln!("retention completed seed={seed_value:#x} backend={backend:?} operations={count} accepted={} refused={} stale={} scope={} reopen={} authenticated_restores=1", coverage[0],coverage[1],coverage[2],coverage[3],coverage[4]);
        }
    }
    eprintln!("retention campaign seeds={seeds:x?} backends=2 operations={} authenticated_restores={} full_default={}", seeds.len()*2*count, seeds.len()*2, seeds == SEEDS && count == STEPS);
}
