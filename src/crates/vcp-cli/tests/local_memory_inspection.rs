// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Governed memory inspection through the compiled observer, without inference.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use vcp_domain::{
    artifact::{ArtifactSpec, Channel, Range},
    ids::*,
    memory::*,
    retention::RetentionMask,
    revision::*,
    task::{Task, TaskState},
    workspace::Workspace,
};
use vcp_memory::repository::propose;
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};

const CAPABILITY: &str = "memory/inspection-state/1";

fn initialize(client: &mut Client, detailed: bool) {
    let mut capabilities = vec!["memory/inspect", "task/read"];
    if detailed {
        capabilities.push(CAPABILITY);
    }
    let response = client.rpc(
        1,
        "initialize",
        json!({"protocol_version":"1.0",
        "client":{"name":"compiled-memory-inspector","version":"1"},
        "capabilities":capabilities,"required_capabilities":capabilities}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        response["result"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!(CAPABILITY)),
        detailed
    );
}

async fn seed(
    fixture: &Fixture,
    visibility: &str,
) -> (ClaimId, ClaimVersionId, Option<ClaimId>, String) {
    let mut store = fixture.reopen().await;
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let origin = store
        .state()
        .events
        .iter()
        .find(|event| {
            event.event.kind == vcp_protocol::event::EventKind::TaskCreated
                && event.event.task.as_ref() == Some(&fixture.config.root_task)
        })
        .unwrap()
        .event
        .id
        .clone();
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: task.scope.clone(),
            media_type: "text/plain".into(),
            schema: "memory-source/1".into(),
            source: "offline compiled memory fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"mod parser; // immutable source\n")
        .unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    artifact.spec.id.as_str(),
                    task.scope.workspace.clone(),
                    Revision::ZERO,
                    &artifact,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let access = vcp_memory::access::Access {
        workspace: task.scope.workspace.clone(),
        actor: fixture.config.actor.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let proposal = Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: task.scope.clone(),
        actor: access.actor.clone(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "compiled-memory/1".into(),
        output_key: "architecture".into(),
        origins: vec![origin],
        subject: "parser".into(),
        predicate: "architecture".into(),
        statement: "Parser isolates syntax; retained-content-marker".into(),
        value: ClaimValue::Architecture {
            decision: "Parser isolates syntax".into(),
            rationale: "Retained module source".into(),
            inference: true,
        },
        applicability: Applicability {
            repository: fixture.config.binding.repository.clone(),
            worktree: fixture.config.binding.worktree.clone(),
            roots: vec![],
            paths: vec!["src/parser.rs".into()],
            symbols: vec![],
            branch: None,
            fingerprint: Some(task.fingerprint.clone()),
            conditions: BTreeMap::new(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![EvidenceRef {
            artifact: artifact.spec.id.clone(),
            sha256: artifact.sha256.clone(),
            range: Some(Range {
                start: ByteCount::new(2),
                end: ByteCount::new(8),
            }),
            source: Some(task.fingerprint.clone()),
            verification: None,
            kind: EvidenceKind::Source,
        }],
        predecessor: None,
        correction_reason: None,
        retention: "workspace".into(),
    };
    let accepted = propose(&mut store, &access, proposal.clone(), Timestamp::new(200))
        .await
        .unwrap();
    assert_eq!(accepted.result.resolution.outcome, Outcome::Accepted);
    let disputed = if visibility == "retained" {
        let mut contrary = proposal.clone();
        contrary.id = ProposalId::new();
        contrary.command = CommandId::new();
        contrary.claim = ClaimId::new();
        contrary.output_key = "contrary-architecture".into();
        contrary.statement = "Parser also owns filesystem operations".into();
        if let ClaimValue::Architecture { decision, .. } = &mut contrary.value {
            *decision = contrary.statement.clone();
        }
        let result = propose(&mut store, &access, contrary.clone(), Timestamp::new(201))
            .await
            .unwrap();
        assert_eq!(result.result.resolution.outcome, Outcome::Disputed);
        Some(contrary.claim)
    } else {
        None
    };
    if visibility != "retained" {
        let mut workspace: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                fixture.config.workspace.as_str(),
                &fixture.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let previous = workspace.revision;
        workspace.revision = previous.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mut mutations = vec![Mutation::Put {
            expected: Some(previous),
            record: Record::typed(
                Collection::Workspace,
                workspace.id.as_str(),
                workspace.id.clone(),
                workspace.revision,
                &workspace,
            )
            .unwrap(),
        }];
        if visibility == "pruned" {
            let mask = RetentionMask {
                schema_version: 1,
                workspace: workspace.id.clone(),
                session: task.scope.session.clone(),
                first: SessionSeq::ZERO,
                last: SessionSeq::ZERO,
                artifacts: vec![artifact.spec.id],
                deletion: workspace.deletion,
                reason: "compiled logical retention".into(),
            };
            mutations.push(Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Tombstone,
                    "compiled-memory-mask",
                    workspace.id.clone(),
                    Revision::ZERO,
                    &mask,
                )
                .unwrap(),
            });
        } else {
            let mut retired = task.clone();
            retired.revision = task.revision.next().unwrap();
            retired.state = TaskState::Cancelled;
            mutations.push(Mutation::Put {
                expected: Some(task.revision),
                record: Record::typed(
                    Collection::Task,
                    retired.scope.task.as_str(),
                    workspace.id.clone(),
                    retired.revision,
                    &retired,
                )
                .unwrap(),
            });
        }
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations,
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        if visibility == "purged" {
            let mut state = store.state().clone();
            for row in state
                .records
                .values_mut()
                .filter(|row| row.workspace == workspace.id)
            {
                row.value = match row.value["document_type"].as_str() {
                    Some("vcp_memory_proposal_v1") => serde_json::to_value(
                        vcp_protocol::redaction::proposal(
                            &row.decode::<ProposalRecord>().unwrap(),
                            workspace.deletion,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                    Some("vcp_memory_version_v1") => serde_json::to_value(
                        vcp_protocol::redaction::version(
                            &row.decode::<Version>().unwrap(),
                            workspace.deletion,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                    Some("vcp_memory_result_v1") => serde_json::to_value(
                        vcp_protocol::redaction::result(
                            &row.decode::<ProposalResult>().unwrap(),
                            workspace.deletion,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                    _ => row.value.clone(),
                };
            }
            for event in state
                .events
                .iter_mut()
                .filter(|event| event.event.workspace == workspace.id)
            {
                *event = vcp_protocol::redaction::event(event, workspace.deletion).unwrap();
            }
            store.rewrite_base(state, &[]).await.unwrap();
        }
    }
    let digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(store.state()).unwrap());
    store.close().await.unwrap();
    (
        proposal.claim,
        accepted.result.version.unwrap(),
        disputed,
        digest,
    )
}

fn request(fixture: &Fixture, claim: &ClaimId) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"claim":claim,"version":null})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_memory_inspection_preserves_governed_states_and_never_mutates_history() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for visibility in ["retained", "pruned", "purged"] {
            let fixture = Fixture::new(backend).await;
            let (claim, version, disputed, digest) = seed(&fixture, visibility).await;
            let mut client = Client::connect(&fixture, "observer");
            initialize(&mut client, true);
            let query = request(&fixture, &claim);
            let response = client.rpc(2, "memory/inspect", query.clone());
            assert!(response.get("error").is_none(), "{response}");
            assert_eq!(response["result"]["kind"], "memory");
            let page = &response["result"]["value"];
            assert_eq!(page["scope"], fixture.scope());
            assert_eq!(page["complete"], true);
            assert_eq!(page["findings"].as_array().unwrap().len(), 1);
            let finding = &page["findings"][0];
            assert_eq!(finding["version"], json!(version));
            assert_eq!(finding["state"]["visibility"], visibility);
            assert_eq!(finding["state"]["current"], true);
            if visibility == "retained" {
                assert_eq!(finding["state"]["applicable"], true);
                assert_eq!(finding["state"]["resolution"]["outcome"], "accepted");
                assert_eq!(finding["evidence"][0]["offset"], "2");
                assert_eq!(finding["evidence"][0]["length"], "6");
                assert_eq!(finding["state"]["evidence"][0]["availability"], "available");
                let contrary = client.rpc(
                    3,
                    "memory/inspect",
                    request(&fixture, disputed.as_ref().unwrap()),
                );
                assert!(contrary.get("error").is_none(), "{contrary}");
                let state = &contrary["result"]["value"]["findings"][0]["state"];
                assert_eq!(state["resolution"]["outcome"], "disputed");
                assert_eq!(state["current"], false);
                assert_eq!(state["resolution"]["conflicts"], json!([version]));
            } else {
                assert_eq!(finding["content"], "");
                assert_eq!(finding["evidence"], json!([]));
                assert_eq!(finding["state"]["applicable"], false);
                assert!(finding["state"]["resolution"].is_null());
                assert!(!response.to_string().contains("retained-content-marker"));
            }
            let mut explicit = query.clone();
            explicit["version"] = json!(version);
            assert_eq!(
                client.rpc(4, "memory/inspect", explicit)["result"],
                response["result"]
            );
            let mut foreign = query;
            foreign["scope"]["session"] = json!("foreign-session");
            assert!(client
                .rpc(5, "memory/inspect", foreign)
                .get("error")
                .is_some());
            assert!(client.finish().await.0.success());
            let mut legacy = Client::connect(&fixture, "observer");
            initialize(&mut legacy, false);
            let denied = legacy.rpc(2, "memory/inspect", request(&fixture, &claim));
            assert_eq!(
                denied["error"]["data"]["details"]["code"],
                "CAPABILITY_UNAVAILABLE"
            );
            assert!(legacy.finish().await.0.success());
            let store = fixture.reopen().await;
            assert_eq!(
                vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(store.state()).unwrap()),
                digest,
                "observer inspection and refused legacy calls must not rewrite canonical memory"
            );
            store.close().await.unwrap();
        }
    }
}
