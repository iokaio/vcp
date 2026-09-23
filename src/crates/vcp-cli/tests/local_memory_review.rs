// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Real compiled manual review and reconnect; no provider or model is configured.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::time::Duration;
use vcp_domain::{
    artifact::{ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::Task,
    workspace::Workspace,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};

fn initialize(client: &mut Client, governance: bool) {
    let mut methods = vec![
        "memory/propose",
        "memory/resolve",
        "memory/review",
        "controller/read",
        "controller/acquire",
        "task/read",
        "command/read",
    ];
    if governance {
        methods.push("memory/governance/1");
    }
    let reply=client.rpc(1,"initialize",json!({"protocol_version":"1.0","client":{"name":"compiled-manual-memory-review","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(reply.get("error").is_none(), "{reply}");
}
fn attach(ticket: &Value, role: &str, governance: bool) -> Client {
    let mut client = Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":ticket,"role":role}));
    assert_eq!(client.receive()["schema"], "vcp-local-ready/1");
    initialize(&mut client, governance);
    client
}
fn value(reply: Value, kind: &str) -> Value {
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(reply["result"]["kind"], kind);
    reply["result"]["value"].clone()
}
fn view(client: &mut Client, fixture: &Fixture, submission: &str) -> Value {
    value(client.rpc(50,"memory/review",json!({"scope":fixture.scope(),"task":fixture.config.root_task,"submission":submission})),"memory_review")
}
fn mutation(client: &mut Client, fixture: &Fixture, command: &str) -> Value {
    let task = value(
        client.rpc(
            51,
            "task/read",
            json!({"scope":fixture.scope(),"task":fixture.config.root_task}),
        ),
        "task",
    );
    assert_eq!(task["state"], "paused");
    json!({"command_id":command,"expected_revision":task["revision"],"steering_revision":task["steering_revision"]})
}
fn propose(
    client: &mut Client,
    fixture: &Fixture,
    candidate: &Value,
    guards: &Value,
    suffix: &str,
) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"submission":format!("manual-{suffix}"),"mutation":mutation(client,fixture,&format!("submit-{suffix}")),"guards":guards,"candidate":candidate})
}
fn resolve(
    client: &mut Client,
    fixture: &Fixture,
    review: &Value,
    decision: &str,
    suffix: &str,
) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"submission":review["submission"],"submission_revision":"0","submission_digest":review["candidate_digest"],"mutation":mutation(client,fixture,&format!("decide-{suffix}")),"guards":review["guards"],"decision":decision,"reason":"explicit compiled owner review"})
}
async fn seed(fixture: &Fixture) -> (Value, Value) {
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
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            fixture.config.workspace.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let policy = match store.state().records.get(&vcp_store::contract::key(
        Collection::Access,
        fixture.config.workspace.as_str(),
    )) {
        None => PolicyRevision::ZERO,
        Some(row) => match row
            .decode::<vcp_domain::policy::AuthorityDocument>()
            .unwrap()
            .data
        {
            vcp_domain::policy::AuthorityData::Policy { policy } => policy.revision,
            _ => panic!("expected canonical policy"),
        },
    };
    let origin = store
        .state()
        .events
        .iter()
        .find(|e| {
            e.event.kind == vcp_protocol::event::EventKind::TaskCreated
                && e.event.task.as_ref() == Some(&task.scope.task)
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
            source: "offline compiled governance evidence".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"mod parser; // immutable retained source\n")
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
    let candidate = json!({"claim":"manual-architecture","origins":[origin],"subject":"parser","predicate":"architecture","statement":"Parser isolates syntax; manual-retained-marker","value":{"kind":"architecture","decision":"Parser isolates syntax","rationale":"observed retained module source","inference":true},"applicability":{"repository":fixture.config.binding.repository,"worktree":fixture.config.binding.worktree,"roots":[],"paths":["src/parser.rs"],"symbols":[],"branch":null,"fingerprint":task.fingerprint,"conditions":{},"valid_from":null,"valid_until":null},"evidence":[{"artifact":artifact.spec.id,"sha256":artifact.sha256,"range":{"start":"2","end":"8"},"source":task.fingerprint,"verification":null,"kind":"source"}],"predecessor":null,"correction_reason":null,"retention":"workspace"});
    let guards =
        json!({"policy_revision":policy,"deletion_epoch":workspace.deletion,"expected_head":null});
    store.close().await.unwrap();
    (candidate, guards)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_manual_review_reconnect_replays_original_receipts_and_preserves_governance() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let (candidate, guards) = seed(&fixture).await;
        let mut first = Client::spawn("local-bridge");
        let mut boot = fixture.bootstrap("controller");
        boot["transport"] = json!("windows_pipe");
        first.send(boot);
        let ready = first.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        let controller_ticket = ready["attachment"].clone();
        let observer_ticket = ready["observer_attachment"].clone();
        initialize(&mut first, true);
        let original = propose(&mut first, &fixture, &candidate, &guards, "accepted");
        assert!(
            first
                .rpc(2, "memory/propose", original.clone())
                .get("error")
                .is_some(),
            "authenticated connection without lease cannot propose"
        );
        accepted(&first.rpc(3,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"manual-first-controller","expected_revision":null})));
        let mut stale = original.clone();
        stale["mutation"]["command_id"] = json!("stale-submit");
        stale["mutation"]["expected_revision"] = json!("18446744073709551615");
        assert!(first.rpc(4, "memory/propose", stale).get("error").is_some());
        let pending = value(
            first.rpc(5, "memory/propose", original.clone()),
            "memory_reviewed",
        );
        assert_eq!(pending["disposition"], "awaiting_review");
        assert!(pending["version"].is_null());
        assert!(pending["resolution"].is_null());
        assert!(pending["indexing"].is_null());
        assert_eq!(
            value(
                first.rpc(6, "memory/propose", original.clone()),
                "memory_reviewed"
            ),
            pending
        );
        let mut changed = original.clone();
        changed["candidate"]["statement"] = json!("different content under original command");
        assert_eq!(
            first.rpc(7, "memory/propose", changed)["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        let mut observer = attach(&observer_ticket, "observer", true);
        let inspected = view(&mut observer, &fixture, "manual-accepted");
        assert_eq!(inspected["candidate"], candidate);
        assert_eq!(inspected["disposition"], "awaiting_review");
        assert_eq!(
            inspected["candidate"]["evidence"][0]["range"],
            json!({"start":"2","end":"8"})
        );
        assert!(observer
            .rpc(8, "memory/propose", original.clone())
            .get("error")
            .is_some());
        let forbidden = resolve(
            &mut observer,
            &fixture,
            &inspected,
            "accept",
            "forbidden-observer",
        );
        assert!(observer
            .rpc(9, "memory/resolve", forbidden)
            .get("error")
            .is_some());
        assert!(first.finish().await.0.success());
        let released = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let lease = value(
                    observer.rpc(10, "controller/read", json!({"scope":fixture.scope()})),
                    "controller",
                );
                if lease["ownership"] == "released" {
                    break lease;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            view(&mut observer, &fixture, "manual-accepted"),
            inspected,
            "owner loss preserves pending original evidence"
        );
        let mut owner = attach(&controller_ticket, "controller", true);
        assert!(
            owner
                .rpc(11, "memory/propose", original.clone())
                .get("error")
                .is_some(),
            "reconnect never reacquires a lease"
        );
        accepted(&owner.rpc(12,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"manual-second-controller","expected_revision":released["revision"]})));
        assert_eq!(
            value(
                owner.rpc(13, "memory/propose", original.clone()),
                "memory_reviewed"
            ),
            pending
        );
        let accepted_request = resolve(&mut owner, &fixture, &inspected, "accept", "accepted");
        let accepted_result = value(
            owner.rpc(14, "memory/resolve", accepted_request.clone()),
            "memory_reviewed",
        );
        assert_eq!(accepted_result["resolution"]["outcome"], "accepted");
        assert!(accepted_result["version"].is_string());
        assert_eq!(
            value(
                owner.rpc(15, "memory/resolve", accepted_request.clone()),
                "memory_reviewed"
            ),
            accepted_result
        );
        let mut changed = accepted_request.clone();
        changed["decision"] = json!("reject");
        assert_eq!(
            owner.rpc(16, "memory/resolve", changed)["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        let receipt = value(
            observer.rpc(
                17,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"decide-accepted"}),
            ),
            "acceptance",
        );
        assert_eq!(receipt, accepted_result["acceptance"]);
        let mut wrong_scope = accepted_request;
        wrong_scope["scope"]["session"] = json!("foreign-session");
        assert!(owner
            .rpc(18, "memory/resolve", wrong_scope)
            .get("error")
            .is_some());

        for (suffix, expected) in [
            ("invalid", "rejected"),
            ("contrary", "disputed"),
            ("explicit-reject", "rejected"),
        ] {
            let mut candidate = candidate.clone();
            candidate["claim"] = json!(format!("claim-{suffix}"));
            if suffix == "invalid" {
                candidate["applicability"]["repository"] = json!("wrong-binding");
            }
            if suffix == "contrary" {
                candidate["statement"] = json!("Parser owns filesystem operations");
                candidate["value"]["decision"] = candidate["statement"].clone();
            }
            let request = propose(&mut owner, &fixture, &candidate, &guards, suffix);
            let pending = value(owner.rpc(20, "memory/propose", request), "memory_reviewed");
            assert_eq!(pending["disposition"], "awaiting_review");
            let review = view(&mut observer, &fixture, &format!("manual-{suffix}"));
            let decision = if suffix == "explicit-reject" {
                "reject"
            } else {
                "accept"
            };
            let request = resolve(&mut owner, &fixture, &review, decision, suffix);
            let result = value(
                owner.rpc(21, "memory/resolve", request.clone()),
                "memory_reviewed",
            );
            assert_eq!(result["resolution"]["outcome"], expected);
            if expected == "rejected" {
                assert!(result["version"].is_null());
            } else {
                assert!(result["resolution"]["conflicts"]
                    .as_array()
                    .unwrap()
                    .contains(&accepted_result["version"]));
            }
            assert_eq!(
                value(owner.rpc(22, "memory/resolve", request), "memory_reviewed"),
                result
            );
        }
        let mut legacy = attach(&observer_ticket, "observer", false);
        let mut no_capability = original.clone();
        no_capability["mutation"]["command_id"] = json!("missing-capability-submit");
        no_capability["submission"] = json!("missing-capability-submission");
        assert_eq!(
            legacy.rpc(23, "memory/propose", no_capability)["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        let prose = json!({"scope":fixture.scope(),"task":fixture.config.root_task,
            "mutation":mutation(&mut owner,&fixture,"legacy-prose-submit"),
            "content":"Do not infer a governed claim from this prose",
            "evidence":[candidate["evidence"][0]["artifact"]]});
        assert_eq!(
            owner.rpc(23, "memory/propose", prose)["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        let denied=legacy.rpc(23,"memory/review",json!({"scope":fixture.scope(),"task":fixture.config.root_task,"submission":"manual-accepted"}));
        assert_eq!(
            denied["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        assert!(legacy.finish().await.0.success());
        assert!(owner.finish().await.0.success());
        assert!(observer.finish().await.0.success());
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        assert_offline_paused(&store, &fixture.config);
        for (kind, expected) in [
            (vcp_domain::memory_review::SUBMISSION, 4),
            (vcp_domain::memory_review::DECISION, 4),
            ("vcp_memory_version_v1", 2),
        ] {
            assert_eq!(
                store
                    .state()
                    .records
                    .values()
                    .filter(|r| r.value["document_type"] == kind)
                    .count(),
                expected
            );
        }
        for suffix in ["accepted", "invalid", "contrary", "explicit-reject"] {
            for prefix in ["submit", "decide"] {
                assert!(store
                    .state()
                    .commands
                    .contains_key(&vcp_store::contract::command_key(
                        &fixture.config.workspace,
                        &CommandId::parse(format!("{prefix}-{suffix}")).unwrap()
                    )));
            }
        }
        assert!(!store
            .state()
            .commands
            .contains_key(&vcp_store::contract::command_key(
                &fixture.config.workspace,
                &CommandId::parse("stale-submit").unwrap()
            )));
        for denied in ["missing-capability-submit", "legacy-prose-submit"] {
            assert!(!store
                .state()
                .commands
                .contains_key(&vcp_store::contract::command_key(
                    &fixture.config.workspace,
                    &CommandId::parse(denied).unwrap(),
                )));
        }
        store.close().await.unwrap();
    }
}
