// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeMap;
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{artifact::ArtifactWriter, contract::*, Store};

fn access(config: &Config) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
async fn command(
    engine: &mut Engine<Store>,
    config: &Config,
    payload: Command,
    task: Option<TaskId>,
    expected: Revision,
) {
    let envelope = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task,
        caller: config.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    };
    let mut facts = HostFacts::inspect(Timestamp::new(1000));
    facts.may_execute = true;
    if matches!(
        &envelope.payload,
        Command::Transition {
            next: TaskState::Running,
            ..
        }
    ) {
        let task = engine
            .store()
            .state()
            .record(
                Collection::Task,
                envelope.task.as_ref().unwrap().as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode::<Task>()
            .unwrap();
        if task.state == TaskState::Paused {
            // Explicit synthetic fixture resume only, before inserting liabilities.
            assert!(!engine
                .store()
                .state()
                .records
                .values()
                .any(|r| matches!(r.collection, Collection::Attempt | Collection::Effect)));
            let workspace = engine
                .store()
                .state()
                .record(
                    Collection::Workspace,
                    config.workspace.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode::<vcp_domain::workspace::Workspace>()
                .unwrap();
            assert_eq!(workspace.binding, config.binding);
            facts.resume = Some(vcp_domain::task::ResumeEvidence {
                workspace_current: true,
                policy_current: true,
                budget_current: true,
                effects_reconciled: true,
                owner_current: true,
            });
        }
    }

    let mut caller = access(config);
    if let Ok(row) = engine.store().state().record(
        Collection::Workspace,
        config.workspace.as_str(),
        &config.workspace,
    ) {
        caller.authority = row
            .decode::<vcp_domain::workspace::Workspace>()
            .unwrap()
            .authority;
    }
    engine.handle(envelope, &caller, &facts).await.unwrap();
}
async fn create_task(
    engine: &mut Engine<Store>,
    config: &Config,
    id: &TaskId,
    parent: Option<TaskId>,
) -> Scope {
    command(
        engine,
        config,
        Command::CreateTask {
            root: config.root_task.clone(),
            parent,
            fork_origin: None,
            objective: Objective {
                text: "Preserve encrypted child and accounting history".into(),
                constraints: vec![],
                acceptance: vec!["no lost liabilities".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .await;
    command(
        engine,
        config,
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit synthetic fixture admission".into(),
            verification: None,
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .await;
    Scope {
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: id.clone(),
    }
}
fn actor(config: &Config) -> vcp_budget::Actor {
    vcp_budget::Actor {
        id: config.actor.clone(),
        now: Timestamp::new(1000),
    }
}
async fn captured(
    store: &mut Store,
    scope: &Scope,
    channel: Channel,
    bytes: &[u8],
) -> ArtifactDescriptor {
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "application/json".into(),
            schema: "portable-accounting-fixture/1".into(),
            source: "synthetic retained provider evidence".into(),
            channel,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(bytes).unwrap();
    let descriptor = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    descriptor.spec.id.as_str(),
                    scope.workspace.clone(),
                    Revision::ZERO,
                    &descriptor,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    descriptor
}
async fn attempt(
    store: &mut Store,
    config: &Config,
    scope: &Scope,
    role: RequestRole,
    ceiling: u64,
) -> Attempt {
    let request = captured(
        store,
        scope,
        Channel::RequestBody,
        b"{\"synthetic_request\":true}",
    )
    .await;
    let ledger = vcp_budget::ledger(store.state(), scope).unwrap();
    let mut price = config.price.clone();
    for (category, rate) in &mut price.rates {
        rate.micros = Micros::new(if *category == ChargeCategory::Request {
            ceiling
        } else {
            0
        });
        rate.per_units = Units::new(1);
    }
    let input = vcp_budget::Admission {
        transaction: TransactionId::new(),
        attempt: AttemptId::new(),
        reservation: ReservationId::new(),
        scope: scope.clone(),
        agent: AgentId::new(),
        role,
        request: request.spec.id,
        request_digest: request.sha256,
        quote: vcp_budget::arithmetic::quote(
            price,
            Usage {
                requests: Units::new(1),
                ..Default::default()
            },
            actor(config).now,
        )
        .unwrap(),
        previous: None,
        expected_ledger: ledger.revision,
        policy: ledger.policy,
        steering: SteeringRevision::ZERO,
        draw_protected: false,
        now: actor(config).now,
    };
    let attempt = vcp_budget::reserve(store, input, &actor(config))
        .await
        .unwrap();
    let permit = vcp_budget::submit(store, &attempt.id, scope, attempt.revision, &actor(config))
        .await
        .unwrap();
    assert_eq!(permit.attempt(), &attempt.id);
    // No network request is executed; the durable submitted liability is real.
    attempt
}
async fn observation(
    store: &mut Store,
    config: &Config,
    attempt: &Attempt,
    amount: u64,
    final_usage: bool,
) -> UsageObservation {
    let raw = captured(
        store,
        &attempt.scope,
        Channel::Evidence,
        format!("{{\"synthetic_usage_micros\":{amount}}}").as_bytes(),
    )
    .await;
    UsageObservation {
        id: ObservationId::new(),
        scope: attempt.scope.clone(),
        attempt: attempt.id.clone(),
        provider_request: format!("fixture-{}", attempt.id),
        mode: UsageMode::Cumulative {
            version: Units::new(1),
        },
        amount: Money {
            currency: config.cap.currency.clone(),
            micros: Micros::new(amount),
        },
        final_usage,
        raw: raw.spec.id,
        correction: None,
    }
}

// Shared synthetic U04 seed: real typed accounting, never a provider call.
pub(super) async fn enrich_native(config: &Config) -> TaskId {
    let mut engine = Engine::new(
        Store::open(&config.canonical_root, config.backend, &[])
            .await
            .unwrap(),
    )
    .unwrap();
    let root_task: Task = engine
        .store()
        .state()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    if root_task.state != TaskState::Running {
        command(
            &mut engine,
            config,
            Command::Transition {
                next: TaskState::Running,
                reason: "synthetic U04 accounting admission".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            root_task.revision,
        )
        .await;
    }
    let child_id = TaskId::new();
    let child = create_task(
        &mut engine,
        config,
        &child_id,
        Some(config.root_task.clone()),
    )
    .await;
    let mut store = engine.into_store();
    let root = root_task.scope;
    let cap = Money {
        currency: config.cap.currency.clone(),
        micros: Micros::new(1000),
    };
    if vcp_budget::ledger(store.state(), &root).is_err() {
        vcp_budget::initialize(
            &mut store,
            root.clone(),
            cap.clone(),
            Micros::new(100),
            None,
            &actor(config),
        )
        .await
        .unwrap();
    }
    let ledger = vcp_budget::ledger(store.state(), &root).unwrap();
    vcp_budget::configure(
        &mut store,
        &root,
        ledger.revision,
        cap,
        Micros::new(100),
        BTreeMap::from([(child_id.clone(), Micros::new(200))]),
        &actor(config),
        "synthetic child budget",
    )
    .await
    .unwrap();
    let settled = attempt(&mut store, config, &root, RequestRole::Main, 50).await;
    let final_usage = observation(&mut store, config, &settled, 37, true).await;
    vcp_budget::observe(&mut store, final_usage, &actor(config))
        .await
        .unwrap();
    let pending = attempt(&mut store, config, &child, RequestRole::Child, 80).await;
    let partial = observation(&mut store, config, &pending, 13, false).await;
    vcp_budget::observe(&mut store, partial, &actor(config))
        .await
        .unwrap();
    vcp_budget::hold_uncertain(
        &mut store,
        &pending.id,
        &child,
        &actor(config),
        "synthetic child response outcome remains unknown",
    )
    .await
    .unwrap();
    let ledger = vcp_budget::ledger(store.state(), &root).unwrap();
    assert_eq!(ledger.settled, Micros::new(50));
    assert_eq!(ledger.unresolved, Micros::new(67));
    store.close().await.unwrap();
    child_id
}

#[tokio::test]
async fn encrypted_cross_backend_restore_retains_exact_settlement_uncertain_child_liability_and_graph(
) {
    use vcp_store::{
        keys::{LocalKeys, RecoveryDirectory},
        portable_snapshot::Archive,
        restore_stage::Restore,
        vault_crypto::{Limits, Manifest, Object, PrivateStaging, FORMAT},
        vault_publish::{Checkpoint, LocalTrust},
    };
    for (from, to) in [
        (BackendKind::Files, BackendKind::Sqlite),
        (BackendKind::Sqlite, BackendKind::Files),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let config = config(
            &temp.path().join("source"),
            &workspace_path.canonicalize().unwrap(),
            from,
        );
        let mut engine = Engine::new(
            Store::open(&config.canonical_root, from, &[])
                .await
                .unwrap(),
        )
        .unwrap();
        command(
            &mut engine,
            &config,
            Command::Initialize {
                binding: config.binding.clone(),
            },
            None,
            Revision::ZERO,
        )
        .await;
        let root = create_task(&mut engine, &config, &config.root_task, None).await;
        let child_id = TaskId::new();
        let child = create_task(
            &mut engine,
            &config,
            &child_id,
            Some(config.root_task.clone()),
        )
        .await;
        let mut source = engine.into_store();
        let cap = Money {
            currency: config.cap.currency.clone(),
            micros: Micros::new(1000),
        };
        let ledger = vcp_budget::initialize(
            &mut source,
            root.clone(),
            cap.clone(),
            Micros::new(100),
            None,
            &actor(&config),
        )
        .await
        .unwrap();
        vcp_budget::configure(
            &mut source,
            &root,
            ledger.revision,
            cap,
            Micros::new(100),
            BTreeMap::from([(child_id.clone(), Micros::new(200))]),
            &actor(&config),
            "explicit child budget ceiling",
        )
        .await
        .unwrap();
        let settled = attempt(&mut source, &config, &root, RequestRole::Main, 50).await;
        let final_usage = observation(&mut source, &config, &settled, 37, true).await;
        vcp_budget::observe(&mut source, final_usage.clone(), &actor(&config))
            .await
            .unwrap();
        let pending = attempt(&mut source, &config, &child, RequestRole::Child, 80).await;
        let partial = observation(&mut source, &config, &pending, 13, false).await;
        vcp_budget::observe(&mut source, partial.clone(), &actor(&config))
            .await
            .unwrap();
        vcp_budget::hold_uncertain(
            &mut source,
            &pending.id,
            &child,
            &actor(&config),
            "submitted child response outcome remains unknown",
        )
        .await
        .unwrap();
        let original = source.state().clone();
        let original_ledger = vcp_budget::ledger(&original, &root).unwrap();
        assert_eq!(original_ledger.settled, Micros::new(50));
        assert_eq!(original_ledger.unresolved, Micros::new(67));
        assert_eq!(original_ledger.active, Micros::ZERO);
        let [recovery_path, staging_path, vault_path, restore_path, roots] =
            ["recovery", "stage", "vault", "restore", "roots"].map(|name| temp.path().join(name));
        for path in [
            &recovery_path,
            &staging_path,
            &vault_path,
            &restore_path,
            &roots,
        ] {
            std::fs::create_dir(path).unwrap();
        }
        let forbidden = vec![
            vault_path.clone(),
            config.canonical_root.clone(),
            workspace_path,
        ];
        let recovery = RecoveryDirectory::open(&recovery_path, &forbidden).unwrap();
        let keys = LocalKeys::generate().unwrap();
        let copy = keys.export_recovery(&recovery).unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let trust = LocalTrust::enroll(
            &keys,
            config.workspace.clone(),
            "c".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let archive = Archive::capture(
            &source,
            &source.snapshot().unwrap(),
            &config.workspace,
            &|| false,
        )
        .unwrap();
        let payloads = archive.payloads().unwrap();
        let manifest = Manifest {
            format: FORMAT.into(),
            workspace: config.workspace.clone(),
            lineage: "c".repeat(64),
            sequence: 1,
            deletion: 0,
            parent: None,
            objects: payloads
                .iter()
                .map(|(hash, bytes)| {
                    (
                        hash.clone(),
                        Object {
                            bytes: bytes.len() as u64,
                            sha256: hash.clone(),
                        },
                    )
                })
                .collect(),
        };
        let mut ciphertext = trust
            .encrypt(
                &keys,
                &PrivateStaging::open(&staging_path, &forbidden).unwrap(),
                manifest,
                payloads,
                trust.configuration().revision,
                Limits::default(),
            )
            .unwrap();
        let object = vault_path.join("opaque.age");
        let mut output = std::fs::File::create(&object).unwrap();
        ciphertext.copy_ciphertext(&mut output).unwrap();
        output.sync_all().unwrap();
        drop(output);
        let operation = CommandId::new();
        let mut restore = Restore::begin(
            &restore_path,
            &forbidden,
            operation.clone(),
            &trust,
            ciphertext.sha256().into(),
            ciphertext.bytes(),
        )
        .unwrap();
        restore.acquire(&object, &|| false).unwrap();
        let validated = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .unwrap();
        let imported = restore
            .import(
                &validated,
                &trust,
                to,
                &roots.join(operation.as_str()),
                config.actor.clone(),
                Timestamp::new(2000),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let mut restored = imported.reopen_verified().await.unwrap();
        for row in original.records.values().filter(|row| {
            matches!(
                row.collection,
                Collection::Ledger
                    | Collection::Reservation
                    | Collection::Attempt
                    | Collection::Settlement
            )
        }) {
            assert_eq!(restored.state().records.get(&row.key()), Some(row));
        }
        assert_eq!(
            vcp_budget::ledger(restored.state(), &root).unwrap(),
            original_ledger
        );
        assert_eq!(
            &restored.state().events[..original.events.len()],
            original.events.as_slice()
        );
        for (id, receipt) in &original.transactions {
            assert_eq!(restored.state().transactions.get(id), Some(receipt));
        }
        for scope in [&root, &child] {
            let task: Task = restored
                .state()
                .record(Collection::Task, scope.task.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.scope, *scope);
            assert_eq!(task.root, config.root_task);
            assert_eq!(task.state, TaskState::Paused);
            assert_eq!(
                task.parent,
                if scope == &child {
                    Some(config.root_task.clone())
                } else {
                    None
                }
            );
        }
        let rebound: Workspace = restored
            .state()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(rebound.trust, Trust::Untrusted);
        let watermark = restored.state().watermark;
        // Imported history cannot mint another send capability, and exact usage
        // retry remains idempotent without dropping the child's uncertainty.
        let pending_now: Attempt = restored
            .state()
            .record(Collection::Attempt, pending.id.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(pending_now.phase, ReservationState::ReconciliationPending);
        assert!(vcp_budget::submit(
            &mut restored,
            &pending.id,
            &child,
            pending_now.revision,
            &actor(&config)
        )
        .await
        .is_err());
        vcp_budget::observe(&mut restored, final_usage, &actor(&config))
            .await
            .unwrap();
        vcp_budget::observe(&mut restored, partial, &actor(&config))
            .await
            .unwrap();
        assert_eq!(restored.state().watermark, watermark);
        assert_eq!(
            vcp_budget::ledger(restored.state(), &root).unwrap(),
            original_ledger
        );
        assert_eq!(source.state(), &original);
        restored.close().await.unwrap();
        source.close().await.unwrap();
    }
}
