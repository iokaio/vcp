// SPDX-License-Identifier: Apache-2.0
use vcp_audit::{history, session_export};
use vcp_domain::{artifact::*, task::*, verification::Fingerprint, workspace::*, *};
use vcp_engine::{public::PublicError, public_export::*, Access, Engine, HostFacts};
use vcp_protocol::{command::*, methods};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};
use vcp_store::{
    contract::{key, Collection, Record},
    export_contract,
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
fn disclosure() -> ExportDisclosure {
    ExportDisclosure {
        policy: PolicyRevision::ZERO,
        history: true,
        artifacts: true,
    }
}
fn reader() -> history::Access {
    history::Access {
        workspace: access().workspace,
        authority: AuthorityRevision::ZERO,
        read: true,
        tasks: None,
    }
}
fn request(
    command: &str,
    task: Option<&TaskId>,
    capture: methods::CaptureScope,
) -> methods::SessionExport {
    methods::SessionExport {
        scope: methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        },
        mutation: methods::Mutation {
            command_id: id(command),
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: task.map(|t| id(t.as_str())),
        capture,
    }
}
async fn issue(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>, expected: u64) {
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access().workspace,
        session: access().session,
        task,
        caller: access().actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::new(expected),
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(command, &access(), &HostFacts::inspect(Timestamp::new(1)))
        .await
        .unwrap();
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (
    Engine<Store>,
    TaskId,
    TaskId,
    ControllerId,
    vcp_engine::controller::ControllerToken,
) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    issue(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/export-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        0,
    )
    .await;
    let root = TaskId::parse("root").unwrap();
    let other = TaskId::parse("other").unwrap();
    for task in [&root, &other] {
        issue(
            &mut engine,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: format!("private-objective-{task}"),
                    constraints: vec![],
                    acceptance: vec!["inspect".into()],
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
            Some(task.clone()),
            0,
        )
        .await;
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: Scope {
                    workspace: access().workspace,
                    session: access().session,
                    task: task.clone(),
                },
                media_type: "application/octet-stream".into(),
                schema: "synthetic-evidence/1".into(),
                source: "fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![Omission::AuthenticationHeaders],
            })
            .unwrap();
        writer
            .write_chunk(format!("arbitrary-source-bytes-{task}").as_bytes())
            .unwrap();
        let descriptor = writer.finalize().unwrap();
        issue(
            &mut engine,
            Command::AttachArtifact { descriptor },
            Some(task.clone()),
            0,
        )
        .await;
    }
    let connection = ControllerId::new();
    engine
        .acquire_controller(
            &access(),
            &connection,
            CommandId::new(),
            None,
            Timestamp::new(2),
        )
        .await
        .unwrap();
    let token = engine.controller_token(&access(), &connection).unwrap();
    (engine, root, other, connection, token)
}
fn prepare(
    engine: &Engine<Store>,
    request: methods::SessionExport,
    root: &TaskId,
    connection: &ControllerId,
    token: &vcp_engine::controller::ControllerToken,
) -> PreparedPublicExport {
    match engine
        .prepare_public_export(request, &access(), connection, token, &disclosure(), root)
        .unwrap()
    {
        PublicExportAdmission::Ready(value) => value,
        _ => panic!("expected fresh export"),
    }
}
fn read(engine: &Engine<Store>, id: &methods::Id) -> Vec<u8> {
    let mut bytes = Vec::new();
    history::History::read_artifact(
        engine.store(),
        &reader(),
        &ArtifactId::parse(id.as_str()).unwrap(),
        &mut bytes,
    )
    .unwrap();
    bytes
}

#[tokio::test]
async fn both_modes_are_atomic_scoped_truthful_and_replay_without_recapture() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, other, connection, token) = fixture(temp.path(), backend).await;
        for (name, capture) in [
            ("metadata", methods::CaptureScope::VisibleHistory),
            (
                "with-artifacts",
                methods::CaptureScope::VisibleHistoryAndArtifacts,
            ),
        ] {
            let request = request(name, Some(&root), capture.clone());
            let before = engine.store().state().clone();
            let prepared = prepare(&engine, request.clone(), &root, &connection, &token);
            let rendered = session_export::render(
                engine.store(),
                &reader(),
                prepared.sources(),
                prepared.capture(),
            )
            .unwrap();
            let outcome = engine
                .commit_public_export(
                    prepared,
                    &access(),
                    &disclosure(),
                    rendered,
                    Timestamp::new(3),
                )
                .await
                .unwrap();
            assert!(!outcome.replayed);
            assert!(
                !outcome.view.complete,
                "metadata payload omissions must remain explicit"
            );
            assert_eq!(
                engine.store().state().watermark,
                before.watermark.next().unwrap()
            );
            assert_eq!(
                engine.store().state().records.len(),
                before.records.len() + 2
            );
            assert_eq!(engine.store().state().events.len(), before.events.len() + 1);
            for (key, value) in &before.records {
                assert_eq!(engine.store().state().records.get(key), Some(value));
            }
            let bytes = read(&engine, &outcome.view.artifact);
            let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(!String::from_utf8(bytes)
                .unwrap()
                .contains("private-objective"));
            assert_eq!(document["history_profile"], "event-metadata/1");
            assert!(document["history"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["task"] == root.as_str()));
            let artifacts = document["artifacts"].as_array().unwrap();
            if capture == methods::CaptureScope::VisibleHistory {
                assert!(artifacts.is_empty());
            } else {
                assert_eq!(artifacts.len(), 1);
                let content: Vec<u8> =
                    serde_json::from_value(artifacts[0]["bytes"].clone()).unwrap();
                assert_eq!(content, format!("arbitrary-source-bytes-{root}").as_bytes());
                assert_eq!(
                    artifacts[0]["descriptor"]["spec"]["omissions"][0],
                    "authentication_headers"
                );
            }
            let manifest: serde_json::Value =
                serde_json::from_slice(&read(&engine, &outcome.view.visibility_manifest)).unwrap();
            assert_eq!(manifest["secret_sanitization"], false);
            assert_eq!(
                manifest["artifact"]["spec"]["id"],
                outcome.view.artifact.as_str()
            );
            let mut narrow = reader();
            narrow.tasks = Some([other.clone()].into());
            assert!(history::History::read_artifact(
                engine.store(),
                &narrow,
                &ArtifactId::parse(outcome.view.artifact.as_str()).unwrap(),
                Vec::new()
            )
            .is_err());
            let snapshot = engine.store().state().clone();
            let PublicExportAdmission::Replay(replay) = engine
                .prepare_public_export(
                    request.clone(),
                    &access(),
                    &connection,
                    &token,
                    &disclosure(),
                    &root,
                )
                .unwrap()
            else {
                panic!("receipt must replay")
            };
            assert_eq!(replay.receipt, outcome.receipt);
            assert_eq!(replay.view, outcome.view);
            assert_eq!(engine.store().state(), &snapshot);
            let mut changed = request.clone();
            changed.capture = if capture == methods::CaptureScope::VisibleHistory {
                methods::CaptureScope::VisibleHistoryAndArtifacts
            } else {
                methods::CaptureScope::VisibleHistory
            };
            assert!(matches!(
                engine.prepare_public_export(
                    changed,
                    &access(),
                    &connection,
                    &token,
                    &disclosure(),
                    &root
                ),
                Err(PublicError::CommandConflict)
            ));
            let mut observer = access();
            observer.write = false;
            assert!(matches!(
                engine.prepare_public_export(
                    request,
                    &observer,
                    &connection,
                    &token,
                    &disclosure(),
                    &root
                ),
                Err(PublicError::Access)
            ));
        }
    }
}

#[tokio::test]
async fn aggregate_reads_recheck_every_task_and_deny_stale_payload_and_manifest() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, other, connection, token) = fixture(temp.path(), backend).await;
        let request = request(
            "aggregate",
            None,
            methods::CaptureScope::VisibleHistoryAndArtifacts,
        );
        let prepared = prepare(&engine, request.clone(), &root, &connection, &token);
        let rendered = session_export::render(
            engine.store(),
            &reader(),
            prepared.sources(),
            prepared.capture(),
        )
        .unwrap();
        let outcome = engine
            .commit_public_export(
                prepared,
                &access(),
                &disclosure(),
                rendered,
                Timestamp::new(3),
            )
            .await
            .unwrap();
        let mut narrow = reader();
        narrow.tasks = Some([root.clone()].into());
        for id in [&outcome.view.artifact, &outcome.view.visibility_manifest] {
            assert!(history::History::read_artifact(
                engine.store(),
                &narrow,
                &ArtifactId::parse(id.as_str()).unwrap(),
                Vec::new()
            )
            .is_err());
            assert!(!read(&engine, id).is_empty());
        }
        // Change the OTHER task, not the artifact's anchor. Both readers must
        // reject the copied aggregate despite the anchor still being current.
        issue(
            &mut engine,
            Command::ObserveFingerprint {
                fingerprint: Fingerprint {
                    repository: "d".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
            },
            Some(other),
            0,
        )
        .await;
        for id in [&outcome.view.artifact, &outcome.view.visibility_manifest] {
            assert!(history::History::read_artifact(
                engine.store(),
                &reader(),
                &ArtifactId::parse(id.as_str()).unwrap(),
                Vec::new()
            )
            .is_err());
            assert!(engine
                .public_artifact(
                    &access(),
                    &methods::ArtifactRead {
                        scope: request.scope.clone(),
                        task: super_id(&root),
                        artifact: id.clone(),
                        offset: 0.into(),
                        length: 1024
                    }
                )
                .is_err());
        }
        assert!(matches!(
            engine.prepare_public_export(
                request,
                &access(),
                &connection,
                &token,
                &disclosure(),
                &root
            ),
            Err(PublicError::Unavailable)
        ));
    }
}
fn super_id(value: &TaskId) -> methods::Id {
    id(value.as_str())
}

#[tokio::test]
async fn disclosure_limits_and_canonical_provenance_fail_closed() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, _, connection, token) = fixture(temp.path(), backend).await;
        let request = request(
            "provenance",
            Some(&root),
            methods::CaptureScope::VisibleHistoryAndArtifacts,
        );
        let denied = ExportDisclosure {
            policy: PolicyRevision::ZERO,
            history: true,
            artifacts: false,
        };
        let before = engine.store().state().clone();
        assert!(matches!(
            engine.prepare_public_export(
                request.clone(),
                &access(),
                &connection,
                &token,
                &denied,
                &root
            ),
            Err(PublicError::Access)
        ));
        let prepared = prepare(&engine, request.clone(), &root, &connection, &token);
        let mut rendered = session_export::render(
            engine.store(),
            &reader(),
            prepared.sources(),
            prepared.capture(),
        )
        .unwrap();
        rendered.payload = vec![0; export_contract::MAX_BYTES + 1];
        assert!(matches!(
            engine
                .commit_public_export(
                    prepared,
                    &access(),
                    &disclosure(),
                    rendered,
                    Timestamp::new(3)
                )
                .await,
            Err(PublicExportCommitError::Public(
                PublicError::InvalidParameters
            ))
        ));
        assert_eq!(engine.store().state(), &before);
        assert_eq!(engine.store().spool().unfinished().unwrap().len(), 0);
        let prepared = prepare(&engine, request.clone(), &root, &connection, &token);
        let rendered = session_export::render(
            engine.store(),
            &reader(),
            prepared.sources(),
            prepared.capture(),
        )
        .unwrap();
        let outcome = engine
            .commit_public_export(
                prepared,
                &access(),
                &disclosure(),
                rendered,
                Timestamp::new(3),
            )
            .await
            .unwrap();
        assert!(
            matches!(
                engine.prepare_public_export(
                    request,
                    &access(),
                    &connection,
                    &token,
                    &denied,
                    &root
                ),
                Err(PublicError::Access)
            ),
            "replay must not bypass current disclosure"
        );
        let canonical = engine.store().state();
        let descriptor: ArtifactDescriptor = canonical
            .record(
                Collection::Artifact,
                outcome.view.artifact.as_str(),
                &access().workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        export_contract::validate_read(canonical, access().authority, None, &descriptor).unwrap();

        let mut stale = canonical.clone();
        let mut workspace: Workspace = stale
            .record(Collection::Workspace, "workspace", &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        stale.records.insert(
            key(Collection::Workspace, "workspace"),
            Record::typed(
                Collection::Workspace,
                "workspace",
                workspace.id.clone(),
                workspace.revision,
                &workspace,
            )
            .unwrap(),
        );
        assert!(
            export_contract::validate_read(&stale, access().authority, None, &descriptor).is_err()
        );

        let mut policy_changed = canonical.clone();
        // Pure read-guard test: any newly visible authority document changes
        // the policy fingerprint, independently of the artifact's direct task.
        policy_changed.records.insert(
            key(Collection::Access, "new-policy-input"),
            Record::typed(
                Collection::Access,
                "new-policy-input",
                access().workspace,
                Revision::ZERO,
                &serde_json::json!({"schema_version":1,"purpose":"additional authorization input"}),
            )
            .unwrap(),
        );
        assert!(export_contract::validate_read(
            &policy_changed,
            access().authority,
            None,
            &descriptor
        )
        .is_err());
        assert!(export_contract::validate_read(
            canonical,
            access().authority.next().unwrap(),
            None,
            &descriptor
        )
        .is_err());

        let mut malformed = canonical.clone();
        let accepted = malformed
            .events
            .iter_mut()
            .find(|e| e.event.correlation == outcome.receipt.command)
            .unwrap();
        accepted.event.data["session_export"]["sources"]["dependencies"] = serde_json::json!([]);
        assert!(
            export_contract::validate_read(&malformed, access().authority, None, &descriptor)
                .is_err()
        );
        let mut pruned_proof = canonical.clone();
        pruned_proof
            .events
            .retain(|e| e.event.correlation != outcome.receipt.command);
        assert!(export_contract::validate_read(
            &pruned_proof,
            access().authority,
            None,
            &descriptor
        )
        .is_err());
        let mut changed_fact = canonical.clone();
        changed_fact
            .events
            .iter_mut()
            .find(|e| e.event.correlation == outcome.receipt.command)
            .unwrap()
            .event
            .data["facts"] = serde_json::json!([]);
        assert!(export_contract::validate_read(
            &changed_fact,
            access().authority,
            None,
            &descriptor
        )
        .is_err());
        let mut downgrade = canonical.clone();
        let mut ordinary = descriptor.clone();
        ordinary.spec.schema = "ordinary-evidence/1".into();
        downgrade.records.insert(
            key(Collection::Artifact, ordinary.spec.id.as_str()),
            Record::typed(
                Collection::Artifact,
                ordinary.spec.id.as_str(),
                access().workspace,
                Revision::ZERO,
                &ordinary,
            )
            .unwrap(),
        );
        assert!(
            export_contract::validate_read(&downgrade, access().authority, None, &ordinary)
                .is_err(),
            "schema downgrade must not bypass canonical export provenance"
        );
        let mut unknown = descriptor.clone();
        unknown.spec.schema = "vcp-session-export/99".into();
        assert!(
            export_contract::validate_read(canonical, access().authority, None, &unknown).is_err()
        );
    }
}

#[tokio::test]
async fn durable_export_replay_survives_restart_with_explicit_recovery_and_fresh_lease() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, _, connection, token) = fixture(temp.path(), backend).await;
        let request = request(
            "restart-export",
            None,
            methods::CaptureScope::VisibleHistoryAndArtifacts,
        );
        let prepared = prepare(&engine, request.clone(), &root, &connection, &token);
        let rendered = session_export::render(
            engine.store(),
            &reader(),
            prepared.sources(),
            prepared.capture(),
        )
        .unwrap();
        let outcome = engine
            .commit_public_export(
                prepared,
                &access(),
                &disclosure(),
                rendered,
                Timestamp::new(3),
            )
            .await
            .unwrap();
        let original_payload = read(&engine, &outcome.view.artifact);
        drop(engine);
        let mut reopened =
            Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert!(matches!(
            reopened.prepare_public_export(
                request.clone(),
                &access(),
                &connection,
                &token,
                &disclosure(),
                &root
            ),
            Err(PublicError::Access)
        ));
        let replacement = ControllerId::new();
        reopened
            .recover_controller(
                &access(),
                &replacement,
                CommandId::new(),
                Revision::ZERO,
                Revision::new(1),
                Timestamp::new(4),
            )
            .await
            .unwrap();
        reopened
            .acquire_controller(
                &access(),
                &replacement,
                CommandId::new(),
                Some(Revision::new(1)),
                Timestamp::new(5),
            )
            .await
            .unwrap();
        let token = reopened.controller_token(&access(), &replacement).unwrap();
        let before = reopened.store().state().clone();
        let PublicExportAdmission::Replay(replay) = reopened
            .prepare_public_export(
                request,
                &access(),
                &replacement,
                &token,
                &disclosure(),
                &root,
            )
            .unwrap()
        else {
            panic!("durable replay required")
        };
        assert_eq!(replay.receipt, outcome.receipt);
        assert_eq!(replay.view, outcome.view);
        assert_eq!(read(&reopened, &replay.view.artifact), original_payload);
        assert_eq!(reopened.store().state(), &before);
    }
}

#[tokio::test]
async fn large_real_retained_event_rejects_metadata_export_before_capture() {
    use vcp_protocol::event::{EventInput, EventKind};
    use vcp_store::contract::{CanonicalStore, Transaction};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, root, _, connection, token) = fixture(temp.path(), backend).await;
        let scope = Scope {
            workspace: access().workspace,
            session: access().session,
            task: root.clone(),
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: engine.store().state().watermark,
            mutations: vec![],
            events: vec![EventInput {
                id: EventId::new(),
                workspace: scope.workspace.clone(),
                session: scope.session.clone(),
                task: Some(root.clone()),
                actor: access().actor,
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(3),
                kind: EventKind::Diagnostic,
                artifacts: vec![],
                metadata: None,
                data: serde_json::json!({"schema_version":1,"retained_diagnostic":"x".repeat(export_contract::MAX_BYTES+1)}),
            }],
            command: None,
        };
        engine.store_mut().transact(transaction).await.unwrap();
        let watermark = engine.store().state().watermark;
        let records = engine.store().state().records.len();
        assert!(matches!(
            export_contract::Sources::capture(
                engine.store().state(),
                scope,
                Some(root.clone()),
                access().authority
            ),
            Err(vcp_store::Error::Limit(_))
        ));
        assert!(matches!(
            engine.prepare_public_export(
                request(
                    "large-history",
                    Some(&root),
                    methods::CaptureScope::VisibleHistory
                ),
                &access(),
                &connection,
                &token,
                &disclosure(),
                &root
            ),
            Err(PublicError::Unavailable)
        ));
        assert_eq!(engine.store().state().watermark, watermark);
        assert_eq!(engine.store().state().records.len(), records);
        assert!(engine.store().spool().unfinished().unwrap().is_empty());
    }
}
