// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{task::Objective, verification::Fingerprint, workspace::Binding};
use vcp_engine::{Engine, HostFacts};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::BackendKind;
fn wid(text: &str) -> methods::Id {
    text.to_owned().try_into().unwrap()
}
fn policy() -> model::Policy {
    model::Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: model::Profile::Low,
        allowed_models: (0..40).map(|i| format!("model-{i:02}")).collect(),
        allowed_endpoints: BTreeSet::from(["endpoint".into()]),
        allowed_groups: BTreeSet::from([model::Group::Low]),
        quality_floor_bps: 8000,
        minimum_samples: 20,
        maximum_evidence_age_ms: 9007199254740993,
        deny_data_collection: true,
        require_zdr: true,
        ordering: vec![
            model::Preference::TotalCost,
            model::Preference::Latency,
            model::Preference::Quality,
            model::Preference::Capability,
        ],
        pin: None,
        broader_task_class: None,
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .unwrap()
}
#[tokio::test]
async fn catalog_pages_bound_metadata_and_do_not_read_private_source_bytes() {
    use vcp_domain::artifact::{ArtifactSpec, Channel};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut engine, mut access, task) = fixture(directory.path(), backend).await;
        let catalog = model::CatalogRevision::create(
            None,
            Timestamp::new(20),
            None,
            (0..40)
                .map(|i| model::Candidate {
                    identity: model::ModelEndpoint {
                        model: format!("model-{i:02}"),
                        endpoint: "endpoint".into(),
                    },
                    availability: model::State::Unknown,
                    reasons: vec!["\\\"".repeat(if i < 2 { 700 } else { 300 }); 16],
                    provenance: vec![model::Provenance {
                        source: "private-source-url-marker".into(),
                        sha256: "a".repeat(64),
                        observed_at: Timestamp::new(20),
                        effective_at: None,
                        limitations: vec![],
                    }],
                    capabilities: Default::default(),
                    snapshot: None,
                    compatibility: vec![],
                    memberships: vec![],
                })
                .collect(),
        )
        .unwrap();
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: vcp_domain::workspace::Scope {
                    workspace: access.workspace.clone(),
                    session: access.session.clone(),
                    task: task.clone(),
                },
                media_type: "application/json".into(),
                schema: "routing-catalog/1".into(),
                source: "private-source-marker".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer
            .write_chunk(b"private raw credential marker")
            .unwrap();
        let raw = writer.finalize().unwrap();
        drop(writer);
        send(
            &mut engine,
            &access,
            Some(task.clone()),
            Revision::ZERO,
            Command::AttachArtifact {
                descriptor: raw.clone(),
            },
        )
        .await;
        let global = vcp_memory::access::Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: true,
            tasks: None,
        };
        routing_state::publish_registry(
            engine.store_mut(),
            &global,
            None,
            catalog,
            raw.spec.id,
            Timestamp::new(21),
        )
        .await
        .unwrap();
        access.write = false;
        access.bootstrap = false;
        let before = engine.store().state().watermark;
        let mut query = request(&access, &task);
        query.section = wire::Section::Catalog;
        let mut models = Vec::new();
        loop {
            let page = inspect(&engine, &access, &query, None, Timestamp::new(30), &|| {
                Ok(())
            })
            .unwrap();
            let encoded = serde_json::to_string(&page).unwrap();
            assert!(encoded.len() <= wire::MAX_PAGE_BYTES);
            assert!(!encoded.contains("private raw"));
            assert!(!encoded.contains("private-source"));
            assert!(matches!(
                page.registry,
                wire::Registry::Observed {
                    source_availability: wire::SourceAvailability::RetainedMetadataOnly,
                    ..
                }
            ));
            for row in page.rows {
                if let wire::Row::Candidate {
                    model,
                    reasons,
                    reasons_truncated,
                    ..
                } = row
                {
                    models.push(model);
                    assert!(reasons_truncated);
                    assert_eq!(reasons.len(), 8);
                    assert!(reasons.iter().all(|r| r.truncated));
                } else {
                    panic!("candidate expected")
                }
            }
            query.cursor = page.next_cursor;
            if query.cursor.is_none() {
                break;
            }
        }
        assert_eq!(
            models,
            (0..40).map(|i| format!("model-{i:02}")).collect::<Vec<_>>()
        );
        assert_eq!(engine.store().state().watermark, before);
        let published = routing_state::current_registry(engine.store(), &global)
            .unwrap()
            .unwrap();
        let mut workspace: Workspace = engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert!(source(
            engine.store(),
            &access,
            &workspace,
            &BTreeSet::new(),
            &published,
            &|| Ok(())
        )
        .is_err());
        let mut foreign = access.clone();
        foreign.session = SessionId::new();
        assert!(source(
            engine.store(),
            &foreign,
            &workspace,
            &BTreeSet::from([task.clone()]),
            &published,
            &|| Ok(())
        )
        .is_err());
        query.cursor = None;
        let first = inspect(&engine, &access, &query, None, Timestamp::new(31), &|| {
            Ok(())
        })
        .unwrap();
        query.cursor = first.next_cursor;
        let previous = workspace.revision;
        workspace.revision = previous.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = RetentionMask {
            schema_version: 1,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            first: SessionSeq::new(1),
            last: SessionSeq::new(1),
            artifacts: vec![published.value.raw],
            deletion: workspace.deletion,
            reason: "routing source hidden".into(),
        };
        engine
            .store_mut()
            .transact(vcp_store::contract::Transaction {
                id: TransactionId::new(),
                expected_watermark: before,
                mutations: vec![
                    vcp_store::contract::Mutation::Put {
                        expected: Some(previous),
                        record: vcp_store::contract::Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                    vcp_store::contract::Mutation::Put {
                        expected: None,
                        record: vcp_store::contract::Record::typed(
                            Collection::Tombstone,
                            "routing-mask",
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
        assert!(
            inspect(&engine, &access, &query, None, Timestamp::new(32), &|| Ok(
                ()
            ))
            .is_err()
        );
        query.cursor = None;
        let hidden = inspect(&engine, &access, &query, None, Timestamp::new(32), &|| {
            Ok(())
        })
        .unwrap();
        assert!(hidden.rows.is_empty());
        assert!(matches!(
            hidden.registry,
            wire::Registry::Unavailable {
                reason: wire::RegistryReason::SourceUnavailable
            }
        ));
    }
}
fn request(access: &Access, task: &TaskId) -> wire::Request {
    wire::Request {
        scope: methods::Scope {
            workspace: wid(access.workspace.as_str()),
            session: wid(access.session.as_str()),
        },
        task: wid(task.as_str()),
        section: wire::Section::PolicyEntries,
        limit: 32,
        cursor: None,
    }
}
#[tokio::test]
async fn observer_policy_pages_are_readonly_and_fenced_by_host_scope_and_authority() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut engine, mut access, task) = fixture(directory.path(), backend).await;
        let mut query = request(&access, &task);
        let empty = inspect(&engine, &access, &query, None, Timestamp::new(20), &|| {
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            empty.effective,
            wire::Effective::Unavailable {
                reason: wire::EffectiveReason::NotConfigured
            }
        ));
        let global = vcp_memory::access::Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: true,
            tasks: None,
        };
        let persisted = policy();
        routing_state::initialize_policy(
            engine.store_mut(),
            &global,
            persisted.clone(),
            Timestamp::new(20),
        )
        .await
        .unwrap();
        access.write = false;
        access.bootstrap = false;
        let before = engine.store().state().watermark;
        let first = inspect(&engine, &access, &query, None, Timestamp::new(30), &|| {
            Ok(())
        })
        .unwrap();
        assert_eq!(first.rows.len(), 32);
        assert_eq!(
            first
                .persisted
                .as_ref()
                .unwrap()
                .policy
                .maximum_evidence_age_ms
                .as_str(),
            "9007199254740993"
        );
        assert!(matches!(
            first.effective,
            wire::Effective::Unavailable {
                reason: wire::EffectiveReason::HostUnconfigured
            }
        ));
        query.cursor = first.next_cursor;
        let second = inspect(&engine, &access, &query, None, Timestamp::new(31), &|| {
            Ok(())
        })
        .unwrap();
        assert_eq!(second.rows.len(), 10);
        assert!(second.complete);
        assert!(inspect(
            &engine,
            &access,
            &query,
            Some(&persisted),
            Timestamp::new(31),
            &|| Ok(())
        )
        .is_err());
        let mut wrong = query.clone();
        wrong.scope.session = wid(SessionId::new().as_str());
        assert!(
            inspect(&engine, &access, &wrong, None, Timestamp::new(31), &|| Ok(
                ()
            ))
            .is_err()
        );
        let mut denied = access.clone();
        denied.read = false;
        assert!(
            inspect(&engine, &denied, &query, None, Timestamp::new(31), &|| Ok(
                ()
            ))
            .is_err()
        );
        query.cursor = None;
        let effective = inspect(
            &engine,
            &access,
            &query,
            Some(&persisted),
            Timestamp::new(31),
            &|| Ok(()),
        )
        .unwrap();
        assert!(matches!(
            effective.effective,
            wire::Effective::Observed { .. }
        ));
        assert_eq!(engine.store().state().watermark, before);
        assert!(
            inspect(&engine, &access, &query, None, Timestamp::new(31), &|| Err(
                failure(Code::CursorGap)
            ))
            .is_err()
        );
    }
}
async fn send(
    engine: &mut Engine<Store>,
    access: &Access,
    task: Option<TaskId>,
    expected: Revision,
    payload: Command,
) {
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(command, access, &HostFacts::inspect(Timestamp::new(10)))
        .await
        .unwrap();
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Engine<Store>, Access, TaskId) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let access = Access {
        actor: ActorId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    send(
        &mut engine,
        &access,
        None,
        Revision::ZERO,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/history-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    let first = TaskId::new();
    for task in [first.clone(), TaskId::new()] {
        send(
            &mut engine,
            &access,
            Some(task.clone()),
            Revision::ZERO,
            Command::CreateTask {
                root: task,
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "private history needle must never become a wire snippet".into(),
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
        )
        .await;
    }
    (engine, access, first)
}
