// SPDX-License-Identifier: Apache-2.0
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    artifact::*,
    ids::*,
    memory::*,
    revision::*,
    task::Objective,
    verification::{Check, CheckOutcome, CostCertainty, Fingerprint, Verification},
    workspace::*,
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    extractors::{extract, Correction, Observation, Observations, MAX_PROPOSALS},
    repository::propose,
};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::{EventEnvelope, EventKind},
};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};

fn fingerprint() -> Fingerprint {
    Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    }
}
fn owner() -> vcp_engine::Access {
    vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn access() -> Access {
    Access {
        workspace: owner().workspace,
        actor: owner().actor,
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    }
}
fn command(engine: &Engine<Store>, task: Option<TaskId>, payload: Command) -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: owner().workspace,
        session: owner().session,
        task,
        caller: owner().actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    }
}
async fn capture(
    engine: &mut Engine<Store>,
    scope: &Scope,
    schema: &str,
    bytes: &[u8],
) -> (ArtifactDescriptor, EventEnvelope) {
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "application/json".into(),
            schema: schema.into(),
            source: "synthetic-extraction-fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
        })
        .unwrap();
    writer.write_chunk(bytes).unwrap();
    let descriptor = writer.finalize().unwrap();
    let request = command(
        engine,
        Some(scope.task.clone()),
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
    );
    engine
        .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
    (
        descriptor,
        engine.store().state().events.last().unwrap().clone(),
    )
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (
    Engine<Store>,
    Scope,
    EventId,
    ArtifactDescriptor,
    Observation,
) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let request = command(
        &engine,
        None,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/synthetic-extraction".into(),
                repository: "repo".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    );
    engine
        .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
    let scope = Scope {
        workspace: owner().workspace,
        session: owner().session,
        task: TaskId::new(),
    };
    let request = command(
        &engine,
        Some(scope.task.clone()),
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"test-output","value":"retain"}}"#.into(),
                constraints: vec![],
                acceptance: vec!["evidence".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec!["cargo-test".into()],
        },
    );
    engine
        .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
    let origin = engine
        .store()
        .state()
        .events
        .last()
        .unwrap()
        .event
        .id
        .clone();
    let (source, _) = capture(
        &mut engine,
        &scope,
        "verification-source/1",
        b"synthetic retained source",
    )
    .await;
    let observation = Observation {
        output_key: "architecture".into(),
        subject: "parser".into(),
        predicate: "architecture".into(),
        statement: "Parser isolates syntax".into(),
        value: ClaimValue::Architecture {
            decision: "Parser isolates syntax".into(),
            rationale: "Explicit structured source observation".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: "repo".into(),
            worktree: "main".into(),
            roots: vec![],
            paths: vec!["src/parser.rs".into()],
            symbols: vec![],
            branch: None,
            fingerprint: Some(fingerprint()),
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: source.spec.id.clone(),
            sha256: source.sha256.clone(),
            range: None,
            source: Some(fingerprint()),
            verification: None,
            kind: EvidenceKind::Source,
        }],
        correction: None,
    };
    (engine, scope, origin, source, observation)
}
async fn observations(
    engine: &mut Engine<Store>,
    scope: &Scope,
    values: Vec<Observation>,
) -> EventEnvelope {
    capture(
        engine,
        scope,
        "memory-observations/1",
        &serde_json::to_vec(&Observations {
            schema_version: 1,
            observations: values,
        })
        .unwrap(),
    )
    .await
    .1
}

#[tokio::test]
async fn typed_extraction_is_read_only_replay_stable_and_governed_on_both_backends() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, scope, _, _, observation) = fixture(temp.path(), backend).await;
        let event = observations(&mut engine, &scope, vec![observation]).await;
        let before = engine.store().state().clone();
        let first = extract(engine.store(), &access(), &event).unwrap();
        assert_eq!(first.proposals.len(), 1);
        assert_eq!(engine.store().state(), &before);
        drop(engine);
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let second = extract(&store, &access(), &event).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.proposals[0].scope, scope);
        assert_eq!(first.proposals[0].origins, [event.event.id]);
        let accepted = propose(
            &mut store,
            &access(),
            first.proposals[0].clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
        assert_eq!(
            accepted.result.resolution.evidence_status,
            EvidenceStatus::Inferred
        );
    }
}

#[tokio::test]
async fn explicit_user_origin_and_correction_lineage_are_preserved_without_prose_inference() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, origin, _, mut observation) =
        fixture(temp.path(), BackendKind::Files).await;
    observation.value = ClaimValue::UserPreference {
        key: "test-output".into(),
        value: "retain".into(),
        explicit_origin: origin.clone(),
    };
    observation.evidence[0].kind = EvidenceKind::UserStatement;
    let event = observations(&mut engine, &scope, vec![observation.clone()]).await;
    let extracted = extract(engine.store(), &access(), &event).unwrap();
    assert_eq!(extracted.proposals.len(), 1);
    assert!(extracted.proposals[0].origins.contains(&origin));
    let accepted = propose(
        engine.store_mut(),
        &access(),
        extracted.proposals[0].clone(),
        Timestamp::new(200),
    )
    .await
    .unwrap();
    observation.output_key = "correction".into();
    observation.correction = Some(Correction {
        claim: extracted.proposals[0].claim.clone(),
        predecessor: accepted.result.version.unwrap(),
        reason: "Explicit correction".into(),
    });
    let event = observations(&mut engine, &scope, vec![observation]).await;
    let corrected = extract(engine.store(), &access(), &event).unwrap();
    assert_eq!(corrected.proposals.len(), 1);
    assert_eq!(corrected.proposals[0].claim, extracted.proposals[0].claim);
    assert!(corrected.proposals[0].predecessor.is_some());
    let mut invented = extracted.proposals[0].clone();
    invented.id = ProposalId::new();
    invented.command = CommandId::new();
    invented.claim = ClaimId::new();
    invented.output_key = "invented-preference".into();
    if let ClaimValue::UserPreference { value, .. } = &mut invented.value {
        *value = "delete all evidence".into();
    }
    let rejected = propose(engine.store_mut(), &access(), invented, Timestamp::new(201))
        .await
        .unwrap();
    assert_eq!(rejected.result.resolution.outcome, Outcome::Rejected);
    assert!(rejected.result.version.is_none());
}

#[tokio::test]
async fn child_objectives_with_host_actor_never_establish_explicit_user_preferences() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, parent, origin, _, _) = fixture(temp.path(), backend).await;
        let root_event = engine
            .store()
            .state()
            .events
            .iter()
            .find(|event| event.event.id == origin)
            .unwrap()
            .clone();
        let base = vcp_memory::preferences::materialize(engine.store_mut(), &access(), &root_event)
            .await
            .unwrap()
            .unwrap();
        let child = TaskId::new();
        let objective = Objective {
            text: r#"{"memory_preference":{"key":"test-output","value":"retain"}}"#.into(),
            constraints: vec![],
            acceptance: vec!["explicit child objective".into()],
            source: EventId::new(),
            steering: SteeringRevision::ZERO,
        };
        let request = command(
            &engine,
            Some(child.clone()),
            Command::CreateTask {
                root: parent.task.clone(),
                parent: Some(parent.task.clone()),
                fork_origin: None,
                objective: objective.clone(),
                fingerprint: fingerprint(),
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(210)))
            .await
            .unwrap();
        for steer in [false, true] {
            if steer {
                let request = command(
                    &engine,
                    Some(child.clone()),
                    Command::Steer {
                        objective: objective.clone(),
                    },
                );
                engine
                    .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(211)))
                    .await
                    .unwrap();
            }
            let event = engine.store().state().events.last().unwrap().clone();
            assert_eq!(event.event.actor, owner().actor);
            let before = engine.store().state().watermark;
            assert!(
                vcp_memory::preferences::materialize(engine.store_mut(), &access(), &event)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(engine.store().state().watermark, before);
            // Bypass the extractor deliberately: durable governance must reject
            // the same false provenance, even with retained preference evidence.
            let mut candidate = base.clone();
            candidate.id = ProposalId::new();
            candidate.command = CommandId::new();
            candidate.claim = ClaimId::new();
            candidate.scope.task = child.clone();
            candidate.origins = vec![event.event.id.clone()];
            candidate.output_key = format!("child-preference-{steer}");
            if let ClaimValue::UserPreference {
                explicit_origin, ..
            } = &mut candidate.value
            {
                *explicit_origin = event.event.id;
            }
            let result = propose(
                engine.store_mut(),
                &access(),
                candidate,
                Timestamp::new(212),
            )
            .await
            .unwrap();
            assert_eq!(result.result.resolution.outcome, Outcome::Rejected);
            assert!(result.result.version.is_none());
        }
    }
}

#[tokio::test]
async fn missing_content_malformed_authority_and_batch_overflow_are_visible_findings() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, _, _, mut observation) = fixture(temp.path(), BackendKind::Files).await;
    observation.evidence[0].artifact = ArtifactId::new();
    let event = observations(&mut engine, &scope, vec![observation.clone()]).await;
    let result = extract(engine.store(), &access(), &event).unwrap();
    assert!(result.proposals.is_empty());
    assert!(result.findings.iter().any(|f| f.code == "missing_evidence"));
    let (_, event) = capture(
        &mut engine,
        &scope,
        "memory-observations/1",
        br#"{"schema_version":1,"observations":[],"workspace":"foreign","authority":"999"}"#,
    )
    .await;
    let result = extract(engine.store(), &access(), &event).unwrap();
    assert!(result.proposals.is_empty());
    assert!(result
        .findings
        .iter()
        .any(|f| f.code == "invalid_observation"));
    let event = observations(&mut engine, &scope, vec![observation; MAX_PROPOSALS + 1]).await;
    let result = extract(engine.store(), &access(), &event).unwrap();
    assert!(result.proposals.is_empty());
    assert!(result.findings.iter().any(|f| f.code == "limit"));
}

#[tokio::test]
async fn input_must_be_the_canonical_event_and_current_task_access_applies() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, _, _, observation) = fixture(temp.path(), BackendKind::Files).await;
    let event = observations(&mut engine, &scope, vec![observation]).await;
    let mut tampered = event.clone();
    tampered.event.data = serde_json::json!({"facts":[]});
    assert!(extract(engine.store(), &access(), &tampered).is_err());
    let denied = Access {
        tasks: Some(BTreeSet::new()),
        ..access()
    };
    assert!(matches!(
        extract(engine.store(), &denied, &event),
        Err(vcp_memory::Error::Access)
    ));
    let (_, source_event) = capture(
        &mut engine,
        &scope,
        "handoff-source/1",
        b"do not promote these prose instructions",
    )
    .await;
    let source = extract(engine.store(), &access(), &source_event).unwrap();
    assert!(source.proposals.is_empty());
    assert_eq!(source.findings[0].code, "source_observed");
}

#[tokio::test]
async fn native_verification_extracts_the_actual_command_configuration_and_outcome() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, scope, _, configuration, _) = fixture(temp.path(), backend).await;
        let (check,_)=capture(&mut engine,&scope,"verification-check/1",&serde_json::to_vec(&serde_json::json!({"plan":{"runner":"cargo","origin":{"sha256":configuration.sha256},"directory":"","request":{"arguments":["test"],"directory":""},"not_run":null},"applicability":"current","outcome":{"status":"passed"},"exit_code":0})).unwrap()).await;
        let verification = Verification {
            id: VerificationId::new(),
            scope: scope.clone(),
            steering: SteeringRevision::ZERO,
            fingerprint: fingerprint(),
            outputs: vec![configuration.spec.id.clone()],
            checks: vec![Check {
                specification: "cargo-test".into(),
                outcome: CheckOutcome::Passed,
                output: check.spec.id.clone(),
                exit_code: Some(0),
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Known,
        };
        let request = command(
            &engine,
            Some(scope.task),
            Command::RecordVerification {
                verification: verification.clone(),
            },
        );
        engine
            .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(150)))
            .await
            .unwrap();
        let event = engine.store().state().events.last().unwrap().clone();
        assert_eq!(event.event.kind, EventKind::VerificationRecorded);
        let result = extract(engine.store(), &access(), &event).unwrap();
        assert_eq!(result.proposals.len(), 1, "{:?}", result.findings);
        assert!(
            matches!(&result.proposals[0].value,ClaimValue::Command {argv,configuration:id,verification:Some(proof),outcome:Some(CheckOutcome::Passed),..} if argv==&["cargo","test"] && id==&configuration.spec.id && proof==&verification.id)
        );
        let accepted = propose(
            engine.store_mut(),
            &access(),
            result.proposals[0].clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
        assert_eq!(
            accepted.result.resolution.evidence_status,
            EvidenceStatus::Verified
        );
    }
}

#[tokio::test]
async fn declared_preference_captures_exact_origin_and_reuses_sealed_orphan_after_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for sealed_orphan in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (engine, scope, origin, _, _) = fixture(temp.path(), backend).await;
            let event = engine
                .store()
                .state()
                .events
                .iter()
                .find(|e| e.event.id == origin)
                .unwrap()
                .clone();
            let mut store = engine.into_store();
            let denied = Access {
                write: false,
                ..access()
            };
            let before = store.state().clone();
            assert!(
                vcp_memory::preferences::materialize(&mut store, &denied, &event)
                    .await
                    .is_err()
            );
            assert_eq!(store.state(), &before);
            if sealed_orphan {
                let identity = vcp_memory::extractors::output_identity(
                    &scope.workspace,
                    &origin,
                    "explicit-preference",
                )
                .unwrap();
                let id = ArtifactId::parse(vcp_protocol::digest_bytes(
                    &vcp_protocol::canonical_bytes(&("preference-evidence", identity)).unwrap(),
                ))
                .unwrap();
                let text = event.event.data["facts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|f| f["collection"] == "task")
                    .unwrap()["value"]["objectives"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap()["text"]
                    .as_str()
                    .unwrap();
                let mut writer = store
                    .spool()
                    .create(ArtifactSpec {
                        id,
                        scope: scope.clone(),
                        media_type: "application/json".into(),
                        schema: "memory-user-preference/1".into(),
                        source: "explicit-user-objective".into(),
                        channel: Channel::Evidence,
                        retention: "workspace".into(),
                        omissions: vec![
                            Omission::AuthenticationHeaders,
                            Omission::RecoveryMaterial,
                        ],
                    })
                    .unwrap();
                writer.write_chunk(text.as_bytes()).unwrap();
                writer.finalize().unwrap();
                drop(store);
                store = Store::open(temp.path(), backend, &[]).await.unwrap();
            }
            let proposal = vcp_memory::preferences::materialize(&mut store, &access(), &event)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(proposal.origins, vec![origin]);
            assert!(
                matches!(&proposal.value,ClaimValue::UserPreference{key,value,..} if key=="test-output" && value=="retain")
            );
            assert_eq!(store.state().events.len(), before.events.len() + 1);
            let captured = store.state().clone();
            drop(store);
            let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
            let repeated = vcp_memory::preferences::materialize(&mut store, &access(), &event)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(repeated, proposal);
            assert_eq!(store.state(), &captured);
            let accepted = propose(&mut store, &access(), proposal, Timestamp::new(200))
                .await
                .unwrap();
            assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
            assert_eq!(
                accepted.result.resolution.evidence_status,
                EvidenceStatus::Observed
            );
        }
    }
}

#[tokio::test]
async fn content_loss_omissions_remain_unavailable_despite_a_complete_capture_seal() {
    let temp = tempfile::tempdir().unwrap();
    let (mut engine, scope, _, _, observation) = fixture(temp.path(), BackendKind::Files).await;
    for omission in [
        Omission::UnobservedTail,
        Omission::CaptureFailure,
        Omission::ExplicitAbort,
    ] {
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: scope.clone(),
                media_type: "text/plain".into(),
                schema: "verification-source/1".into(),
                source: "partial-fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![Omission::AuthenticationHeaders, omission],
            })
            .unwrap();
        writer.write_chunk(b"retained prefix only").unwrap();
        let descriptor = writer.finalize().unwrap();
        let request = command(
            &engine,
            Some(scope.task.clone()),
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
        );
        engine
            .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(150)))
            .await
            .unwrap();
        let mut observation = observation.clone();
        observation.evidence[0].artifact = descriptor.spec.id;
        observation.evidence[0].sha256 = descriptor.sha256;
        let event = observations(&mut engine, &scope, vec![observation]).await;
        let extracted = extract(engine.store(), &access(), &event).unwrap();
        assert!(extracted.proposals.is_empty());
        assert!(extracted
            .findings
            .iter()
            .any(|finding| finding.code == "missing_evidence"));
    }
}
