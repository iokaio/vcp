// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::HostFacts;
use vcp_domain::{
    artifact::ArtifactSpec,
    policy::*,
    public_diff::Disposition,
    task::Objective,
    verification::Fingerprint,
    workspace::{Binding, Scope},
};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Mutation, Record, Transaction},
    BackendKind,
};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn access() -> Access {
    Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn scope() -> Scope {
    Scope {
        workspace: access().workspace,
        session: access().session,
        task: TaskId::parse("task").unwrap(),
    }
}
fn request() -> methods::DiffRead {
    methods::DiffRead {
        scope: methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        },
        task: id("task"),
        change: id("change"),
        offset: 0.into(),
        length: 65536,
    }
}
async fn command(
    engine: &mut Engine<Store>,
    payload: Command,
    task: Option<TaskId>,
    expected: Revision,
) {
    engine
        .handle(
            CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: access().workspace,
                session: access().session,
                task,
                caller: access().actor,
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected,
                steering: SteeringRevision::ZERO,
                payload,
            },
            &access(),
            &HostFacts::inspect(Timestamp::new(1)),
        )
        .await
        .unwrap();
}
async fn artifact(
    engine: &mut Engine<Store>,
    name: &str,
    schema: &str,
    bytes: &[u8],
    omissions: Vec<vcp_domain::artifact::Omission>,
) -> ArtifactDescriptor {
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::parse(name).unwrap(),
            scope: scope(),
            media_type: "application/json".into(),
            schema: schema.into(),
            source: "fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions,
        })
        .unwrap();
    for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
        writer.write_chunk(chunk).unwrap();
    }
    let descriptor = writer.finalize().unwrap();
    command(
        engine,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
        Some(scope().task),
        Revision::ZERO,
    )
    .await;
    descriptor
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
    variant: u8,
) -> (Engine<Store>, ArtifactDescriptor, Vec<u8>) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    command(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/absent-public-diff-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        Revision::ZERO,
    )
    .await;
    for name in ["task", "other"] {
        command(
            &mut engine,
            Command::CreateTask {
                root: TaskId::parse(name).unwrap(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "retained proposed changes".into(),
                    constraints: vec![],
                    acceptance: vec![],
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
            Some(TaskId::parse(name).unwrap()),
            Revision::ZERO,
        )
        .await;
    }
    let operation = Operation {
        scope: scope(),
        actor: access().actor,
        host: HostId::parse("host").unwrap(),
        binding: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
        tool: "vcp_patch".into(),
        schema: "a".repeat(64),
        arguments: "{}".into(),
        invocation: Invocation::Local,
        resources: vec![Resource {
            root: RootId::parse("root").unwrap(),
            path: "file.txt".into(),
            write: true,
            version: "b".repeat(64),
        }],
        effects: std::collections::BTreeSet::from([EffectClass::Write]),
        required_isolation: Default::default(),
        timeout_ms: Units::new(1000),
        output_bytes: ByteCount::new(4096),
    };
    let prepared = vcp_policy::Prepared::new(operation.clone()).unwrap();
    let before = vec![255, 254, 97, 0, 10, 0];
    let after = vec![b'z'; 80_000];
    let source = serde_json::json!({"schema":"vcp-prepared-tool/2","controller":"private-controller","owner":"7",
        "host_tool_denials":[{"private":"not projected"}],"skills_revision":"0","prepared":{"schema_version":1,"operation":operation,"result":{"private":"not projected"},
        "changes":[{"path":"file.txt","before":{"root":"root","binding":"0","path":"file.txt","native_identity":"private-native-id","sha256":vcp_protocol::digest_bytes(&before),"bytes":before.len().to_string()},
        "before_bytes":before,"after":after,"rename_to":"renamed.txt","probes":[]}]}});
    let source = artifact(
        &mut engine,
        "private-plan",
        "vcp-prepared-tool-v2",
        &vcp_protocol::canonical_bytes(&source).unwrap(),
        if variant == 7 {
            vec![vcp_domain::artifact::Omission::UnobservedTail]
        } else {
            vec![]
        },
    )
    .await;
    let mut document = Document {
        schema: public_diff::SCHEMA.into(),
        scope: scope(),
        change: ToolRunId::parse("change").unwrap(),
        operation_digest: prepared.digest().into(),
        prepared: source.spec.id.clone(),
        disposition: Disposition::Proposed,
        files: vec![FileChange {
            path: "file.txt".into(),
            rename_to: Some("renamed.txt".into()),
            before: Some(Content {
                sha256: vcp_protocol::digest_bytes(&before),
                bytes: before,
            }),
            after: Some(Content {
                sha256: vcp_protocol::digest_bytes(&after),
                bytes: after,
            }),
        }],
    };
    match variant {
        1 => document.operation_digest = "f".repeat(64),
        2 => {
            let content = document.files[0].after.as_mut().unwrap();
            content.bytes[0] = b'x';
            content.sha256 = vcp_protocol::digest_bytes(&content.bytes);
        }
        3 => document.scope.task = TaskId::parse("other").unwrap(),
        4 => document.prepared = ArtifactId::parse("absent").unwrap(),
        _ => (),
    }
    let mut value = serde_json::to_value(document).unwrap();
    if variant == 5 {
        value["private_controller"] = "forbidden".into();
    }
    let bytes = vcp_protocol::canonical_bytes(&value).unwrap();
    let public = artifact(
        &mut engine,
        "public-plan",
        public_diff::SCHEMA,
        &bytes,
        if variant == 8 {
            vec![vcp_domain::artifact::Omission::ExplicitAbort]
        } else {
            vec![]
        },
    )
    .await;
    command(
        &mut engine,
        Command::ProposeEffect {
            id: ToolRunId::parse("change").unwrap(),
            operation_digest: prepared.digest().into(),
        },
        Some(scope().task),
        Revision::ZERO,
    )
    .await;
    let mut changes = vec![source.spec.id.clone()];
    if variant != 6 {
        changes.push(public.spec.id.clone());
    }
    command(
        &mut engine,
        Command::AdvanceEffect {
            id: ToolRunId::parse("change").unwrap(),
            next: EffectState::Validated,
            reason: "native preparation retained".into(),
            execution: None,
            exit_code: None,
            observed_changes: changes,
        },
        Some(scope().task),
        Revision::ZERO,
    )
    .await;
    (engine, public, bytes)
}

#[tokio::test]
async fn diff_ranges_use_original_proposal_link_after_outcome_and_restart_on_both_stores() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, descriptor, bytes) = fixture(temp.path(), backend, 0).await;
        let initial = engine.public_diff(&access(), &request()).unwrap();
        assert_eq!(initial.artifact.as_str(), descriptor.spec.id.as_str());
        assert_eq!(initial.sha256, vcp_protocol::digest_bytes(&bytes));
        assert_eq!(initial.total_bytes.as_str(), bytes.len().to_string());
        assert_eq!(initial.content.len(), 65536);
        assert!(!initial.complete);
        assert!(!String::from_utf8(bytes.clone())
            .unwrap()
            .contains("private-controller"));
        command(
            &mut engine,
            Command::AdvanceEffect {
                id: ToolRunId::parse("change").unwrap(),
                next: EffectState::Cancelled,
                reason: "not applied".into(),
                execution: None,
                exit_code: None,
                observed_changes: vec![ArtifactId::parse("private-plan").unwrap()],
            },
            Some(scope().task),
            Revision::new(1),
        )
        .await;
        assert_eq!(engine.public_diff(&access(), &request()).unwrap(), initial);
        let watermark = engine.store().state().watermark;
        let mut tail = request();
        tail.offset = ((bytes.len() - 10) as u64).into();
        assert!(engine.public_diff(&access(), &tail).unwrap().complete);
        tail.offset = u64::MAX.into();
        assert!(engine.public_diff(&access(), &tail).is_err());
        let mut other = request();
        other.task = id("other");
        assert!(engine.public_diff(&access(), &other).is_err());
        let mut denied = access();
        denied.read = false;
        assert_eq!(
            engine.public_diff(&denied, &request()),
            Err(QueryError::Access)
        );
        assert_eq!(engine.store().state().watermark, watermark);
        engine.into_store().close().await.unwrap();
        let engine = Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert_eq!(engine.public_diff(&access(), &request()).unwrap(), initial);
    }
}
#[tokio::test]
async fn diff_rejects_forged_projection_scope_proof_and_legacy_without_public_capture() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for variant in 1..=8 {
            let temp = tempfile::tempdir().unwrap();
            let (engine, _, _) = fixture(temp.path(), backend, variant).await;
            assert!(
                engine.public_diff(&access(), &request()).is_err(),
                "variant {variant}"
            );
        }
    }
}
#[tokio::test]
async fn diff_respects_logical_artifact_and_original_linkage_retention_before_bytes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for linkage in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (mut engine, descriptor, _) = fixture(temp.path(), backend, 0).await;
            let event = engine.store().state().events.last().unwrap();
            let mask = RetentionMask {
                schema_version: 1,
                workspace: access().workspace,
                session: access().session,
                first: if linkage {
                    event.sequence
                } else {
                    SessionSeq::ZERO
                },
                last: if linkage {
                    event.sequence
                } else {
                    SessionSeq::ZERO
                },
                artifacts: if linkage {
                    vec![]
                } else {
                    vec![descriptor.spec.id]
                },
                deletion: DeletionEpoch::ZERO,
                reason: "logical suppression".into(),
            };
            let tx = Transaction {
                id: TransactionId::new(),
                expected_watermark: engine.store().state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Tombstone,
                        "mask",
                        access().workspace,
                        Revision::ZERO,
                        &mask,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            };
            engine.store_mut().transact(tx).await.unwrap();
            assert!(engine.public_diff(&access(), &request()).is_err());
        }
    }
}
#[tokio::test]
async fn diff_detects_corruption_outside_requested_range() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (engine, descriptor, _) = fixture(temp.path(), backend, 0).await;
        let directory = engine
            .store()
            .spool()
            .root()
            .join(descriptor.spec.id.as_str());
        let mut chunks = std::fs::read_dir(directory)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "chunk"))
            .collect::<Vec<_>>();
        chunks.sort();
        let last = chunks.last().unwrap();
        let mut bytes = std::fs::read(last).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        std::fs::write(last, bytes).unwrap();
        let mut range = request();
        range.length = 1;
        assert!(engine.public_diff(&access(), &range).is_err());
    }
}

#[test]
fn json_string_guard_bounds_parser_input_across_escapes_and_reads() {
    let encoded = br#"{"a":"escaped\\\"quote","b":"ok"}"#;
    let reader = JsonStrings {
        inner: &encoded[..],
        quoted: false,
        escaped: false,
        length: 0,
    };
    let value: serde_json::Value = serde_json::from_reader(reader).unwrap();
    assert_eq!(value["b"], "ok");
    let oversized = format!("\"{}\"", "x".repeat(MAX_STRING_TOKEN + 1));
    let mut reader = JsonStrings {
        inner: oversized.as_bytes(),
        quoted: false,
        escaped: false,
        length: 0,
    };
    let mut byte = [0u8; 1];
    for _ in 0..=MAX_STRING_TOKEN {
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
    }
    assert!(reader.read(&mut byte).is_err());
}
