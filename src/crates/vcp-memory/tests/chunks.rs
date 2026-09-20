// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeSet;
use vcp_domain::{
    artifact::*, ids::*, revision::*, task::Objective, verification::Fingerprint, workspace::*,
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{access::Access, search_record::*, tokenizer::*};
use vcp_protocol::{
    canonical_bytes,
    command::{Command, CommandEnvelope},
    digest_bytes,
};
use vcp_store::{artifact::ArtifactWriter, contract::CanonicalStore, BackendKind, Store};

#[test]
fn exact_utf8_crlf_spans_and_versioned_overlap() {
    let text = "fn HTTPServer() { /* café 日本 */ }\r\n".repeat(12);
    let spec = ChunkerSpec {
        version: 1,
        max_bytes: 64,
        overlap_bytes: 0,
    };
    let ranges = spans(&text, &spec).unwrap();
    let mut restored = String::new();
    for range in &ranges {
        let part = &text[range.start.get() as usize..range.end.get() as usize];
        assert!(part.len() <= 64);
        restored.push_str(part);
    }
    assert_eq!(restored, text);
    let overlapping = ChunkerSpec {
        overlap_bytes: 12,
        ..spec.clone()
    };
    let overlap = spans(&text, &overlapping).unwrap();
    assert!(overlap.windows(2).all(|pair| pair[1].start < pair[0].end));
    assert_ne!(spec.digest().unwrap(), overlapping.digest().unwrap());
    assert!(spans("", &spec).unwrap().is_empty());
    assert!(spans(
        &text,
        &ChunkerSpec {
            max_bytes: 1,
            ..spec
        }
    )
    .is_err());
}

#[test]
fn code_terms_preserve_spelling_and_split_identifiers_without_stemming() {
    for (input, required) in [
        (
            "HTTPServer",
            vec![
                "HTTPServer",
                "httpserver",
                "HTTP",
                "http",
                "Server",
                "server",
            ],
        ),
        ("snake_case", vec!["snake_case", "snake", "case"]),
        (
            "pkg::Type.method",
            vec!["pkg::Type.method", "pkg", "Type", "method"],
        ),
        (
            "src/HTTPServer.rs",
            vec!["src/HTTPServer.rs", "src", "HTTPServer", "rs"],
        ),
        ("utf8Parser", vec!["utf8Parser", "utf", "8", "Parser"]),
        (
            "日本_名前 a X",
            vec!["日本_名前", "日本", "名前", "a", "X", "x"],
        ),
    ] {
        let terms = code_terms(input);
        for term in required {
            assert!(terms.contains(&term.to_string()), "{input}: missing {term}");
        }
    }
    assert!(code_terms("... ::: ___ / ").is_empty());
    assert_eq!(
        prose_terms("Run the test, then RUN the test."),
        vec!["run", "the", "test", "then", "run", "the", "test"]
    );
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
) -> ArtifactDescriptor {
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "application/octet-stream".into(),
            schema: schema.into(),
            source: "chunk-fixture".into(),
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
    descriptor
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
    bytes: &[u8],
) -> (Engine<Store>, SourceBinding) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let request = command(
        &engine,
        None,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/chunk-fixture".into(),
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
    let root = RootId::parse("workspace").unwrap();
    let snapshot = serde_json::json!({"identity":{"workspace":scope.workspace,"root":root,"repository":"repo","worktree":"main","binding":Revision::ZERO},"bounded_scan_complete":true,"files":[{"root":root,"path":"src/server.rs","sha256":digest_bytes(bytes),"bytes":ByteCount::new(bytes.len() as u64)}]});
    let fingerprint = Fingerprint {
        repository: digest_bytes(&canonical_bytes(&snapshot).unwrap()),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    };
    let request = command(
        &engine,
        Some(scope.task.clone()),
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "chunk exact retained source".into(),
                constraints: vec![],
                acceptance: vec!["exact bytes".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint.clone(),
            editing: false,
            required_checks: vec![],
        },
    );
    engine
        .handle(request, &owner(), &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
    let source = capture(&mut engine, &scope, "verification-source/1", bytes).await;
    let manifest = capture(
        &mut engine,
        &scope,
        "verification-plan/1",
        &canonical_bytes(
            &serde_json::json!({"before":snapshot,"source_artifacts":[source.spec.id]}),
        )
        .unwrap(),
    )
    .await;
    (
        engine,
        SourceBinding {
            manifest: manifest.spec.id,
            artifact: source.spec.id,
            root,
            path: "src/server.rs".into(),
            symbols: vec![],
            fingerprint,
        },
    )
}

#[tokio::test]
async fn authorized_native_sources_have_stable_exact_chunks_and_visible_exclusions() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let bytes = "fn HTTPServer() { /* café */ }\r\n".repeat(12);
        let (engine, binding) = fixture(directory.path(), backend, bytes.as_bytes()).await;
        let spec = ChunkerSpec {
            version: 1,
            max_bytes: 64,
            overlap_bytes: 0,
        };
        let first = inventory(
            engine.store(),
            &access(),
            std::slice::from_ref(&binding),
            &spec,
            Limits::default(),
        )
        .unwrap();
        assert!(first.records.len() > 1, "{:?}", first.exclusions);
        first.validate().unwrap();
        for record in &first.records {
            assert_eq!(
                record.text,
                bytes[record.span.start.get() as usize..record.span.end.get() as usize]
            );
            assert_eq!(record.paths, vec!["src/server.rs"]);
            assert_eq!(record.kind, SearchKind::Source);
        }
        let second = inventory(
            engine.store(),
            &access(),
            std::slice::from_ref(&binding),
            &spec,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(first, second);
        let mut corrupted = first.clone();
        corrupted.records[0].text.push('!');
        corrupted.digest = corrupted.calculate_digest().unwrap();
        assert!(corrupted.validate().is_err());
        let mut denied = access();
        denied.tasks = Some(BTreeSet::new());
        let hidden = inventory(
            engine.store(),
            &denied,
            std::slice::from_ref(&binding),
            &spec,
            Limits::default(),
        )
        .unwrap();
        assert!(hidden.records.is_empty());
        assert!(hidden.exclusions.iter().all(|item| item.source.is_none()));
        assert!(!serde_json::to_string(&hidden)
            .unwrap()
            .contains("server.rs"));
        let mut wrong = binding.clone();
        wrong.path = "src/unattested.rs".into();
        assert!(inventory(
            engine.store(),
            &access(),
            &[wrong],
            &spec,
            Limits::default()
        )
        .unwrap()
        .records
        .is_empty());
        let mut wrong = binding.clone();
        wrong.fingerprint.repository = "d".repeat(64);
        assert!(inventory(
            engine.store(),
            &access(),
            &[wrong],
            &spec,
            Limits::default()
        )
        .unwrap()
        .records
        .is_empty());
        let limited = inventory(
            engine.store(),
            &access(),
            &[binding],
            &spec,
            Limits {
                records: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(limited.records.is_empty());
        assert_eq!(limited.exclusions[0].reason, "inventory_limit");
    }
}

#[tokio::test]
async fn invalid_encodings_and_binary_sources_are_explicitly_excluded() {
    for (bytes, reason) in [
        (b"hello\0world".as_slice(), "binary_source"),
        (&[255u8, 254], "unsupported_encoding"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (engine, binding) = fixture(directory.path(), BackendKind::Files, bytes).await;
        let result = inventory(
            engine.store(),
            &access(),
            &[binding],
            &ChunkerSpec::default(),
            Limits::default(),
        )
        .unwrap();
        assert!(result.records.is_empty());
        assert_eq!(result.exclusions[0].reason, reason);
    }
}

#[tokio::test]
async fn corrected_claim_inventory_excludes_old_version_and_preserves_statement_offsets() {
    use vcp_domain::memory::*;
    use vcp_store::contract::Collection;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut engine, binding) = fixture(directory.path(), backend, b"fn parse() {}\n").await;
        let primary_origin = engine
            .store()
            .state()
            .events
            .last()
            .unwrap()
            .event
            .id
            .clone();
        let other_task = TaskId::new();
        let create = command(
            &engine,
            Some(other_task.clone()),
            Command::CreateTask {
                root: other_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "separately scoped observation".into(),
                    constraints: vec![],
                    acceptance: vec!["retained".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: binding.fingerprint.clone(),
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(create, &owner(), &HostFacts::inspect(Timestamp::new(100)))
            .await
            .unwrap();
        let other_origin = engine
            .store()
            .state()
            .events
            .last()
            .unwrap()
            .event
            .id
            .clone();
        let mut store = engine.into_store();
        let artifact: ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                binding.artifact.as_str(),
                &access().workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let proposal = Proposal {
            id: ProposalId::new(),
            command: CommandId::new(),
            claim: ClaimId::new(),
            scope: artifact.spec.scope.clone(),
            actor: access().actor,
            epochs: Epochs {
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                policy: PolicyRevision::ZERO,
            },
            registry_version: REGISTRY_VERSION,
            extractor: "chunk-fixture/1".into(),
            output_key: "architecture".into(),
            origins: vec![primary_origin],
            subject: "parser".into(),
            predicate: "architecture".into(),
            statement: "Parser isolates syntax".into(),
            value: ClaimValue::Architecture {
                decision: "Parser isolates syntax".into(),
                rationale: "Observed source".into(),
                inference: true,
            },
            applicability: Applicability {
                repository: "repo".into(),
                worktree: "main".into(),
                roots: vec![binding.root.clone()],
                paths: vec![binding.path.clone()],
                symbols: vec![],
                branch: None,
                fingerprint: Some(binding.fingerprint.clone()),
                conditions: Default::default(),
                valid_from: None,
                valid_until: None,
            },
            evidence: vec![EvidenceRef {
                artifact: artifact.spec.id,
                sha256: artifact.sha256,
                range: None,
                source: Some(binding.fingerprint),
                verification: None,
                kind: EvidenceKind::Source,
            }],
            predecessor: None,
            correction_reason: None,
            retention: "workspace".into(),
        };
        let result = vcp_memory::repository::propose(
            &mut store,
            &access(),
            proposal.clone(),
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(result.result.resolution.outcome, Outcome::Accepted);
        let before = inventory(
            &store,
            &access(),
            &[],
            &ChunkerSpec::default(),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(before.records.len(), 1);
        assert_eq!(before.records[0].text, proposal.statement);
        let mut correction = proposal.clone();
        correction.id = ProposalId::new();
        correction.command = CommandId::new();
        correction.output_key = "corrected".into();
        correction.origins = vec![other_origin];
        correction.predecessor = result.result.version;
        correction.correction_reason = Some("Observed corrected module boundary".into());
        correction.statement = "Parser isolates both syntax and tokenization".into();
        correction.value = ClaimValue::Architecture {
            decision: correction.statement.clone(),
            rationale: "Observed source correction".into(),
            inference: true,
        };
        let corrected = vcp_memory::repository::propose(
            &mut store,
            &access(),
            correction.clone(),
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(corrected.result.resolution.outcome, Outcome::Accepted);
        let after = inventory(
            &store,
            &access(),
            &[],
            &ChunkerSpec::default(),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(after.records.len(), 1);
        assert_eq!(after.records[0].text, correction.statement);
        assert_ne!(after.records[0].id, before.records[0].id);
        assert!(after
            .exclusions
            .iter()
            .any(|entry| entry.reason == "superseded"));

        // A retention mask must not bypass the current origin-task access fence.
        use vcp_store::contract::{Mutation, Record, Transaction};
        let origin = store
            .state()
            .events
            .iter()
            .find(|event| event.event.id == correction.origins[0])
            .unwrap()
            .clone();
        let mut workspace: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                owner().workspace.as_str(),
                &owner().workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        // Rebind retains task fingerprints/history; those claims cannot become
        // applicable to another repository merely through fresh read authority.
        let copy = store
            .convert(&directory.path().join("rebind-copy"), backend, &[])
            .await
            .unwrap();
        let mut engine = Engine::new(copy).unwrap();
        let mut rebind = command(
            &engine,
            None,
            Command::Rebind {
                binding: Binding {
                    repository: "another-repository".into(),
                    worktree: "another-worktree".into(),
                    ..workspace.binding.clone()
                },
            },
        );
        rebind.expected = workspace.revision;
        engine
            .handle(rebind, &owner(), &HostFacts::inspect(Timestamp::new(300)))
            .await
            .unwrap();
        let mut current = access();
        current.authority = workspace.authority.next().unwrap();
        let rebound = inventory(
            engine.store(),
            &current,
            &[],
            &ChunkerSpec::default(),
            Limits::default(),
        )
        .unwrap();
        assert!(rebound.records.is_empty());
        assert!(rebound
            .exclusions
            .iter()
            .any(|entry| entry.reason == "stale_binding"));
        engine.into_store().close().await.unwrap();

        let previous = workspace.revision;
        workspace.revision = previous.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = vcp_audit::history::RetentionMask {
            schema_version: 1,
            workspace: workspace.id.clone(),
            session: origin.event.session,
            first: origin.sequence,
            last: origin.sequence,
            artifacts: vec![],
            deletion: workspace.deletion,
            reason: "prune separately scoped origin".into(),
        };
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(previous),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Tombstone,
                            "hidden-origin",
                            workspace.id.clone(),
                            Revision::ZERO,
                            &mask,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let mut restricted = access();
        restricted.tasks = Some([proposal.scope.task.clone()].into_iter().collect());
        assert!(matches!(
            vcp_memory::history::query(&store, &restricted, &proposal.claim, None, None),
            Err(vcp_memory::Error::Access)
        ));
        let hidden = inventory(
            &store,
            &restricted,
            &[],
            &ChunkerSpec::default(),
            Limits::default(),
        )
        .unwrap();
        assert!(hidden.records.is_empty());
        assert!(hidden
            .exclusions
            .iter()
            .all(|entry| entry.source.is_none() && entry.reason == "denied"));
    }
}
