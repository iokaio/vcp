// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual SDK history and governed memory queries; no provider or editor is launched.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel, Range},
    ids::*,
    memory::*,
    retention_selector::{Criterion, Selector, Tree},
    revision::*,
    task::Task,
};
use vcp_memory::repository::propose;
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};
async fn driver(input: Value) {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-ts/tests/native-inspector-queries.mjs");
    let bytes = serde_json::to_vec(&input).unwrap();
    assert!(bytes.len() < 256 * 1024);
    let output = tokio::task::spawn_blocking(move || {
        let mut child =
            Command::new(std::env::var_os("VCP_TEST_NODE").expect("qualified native Node path"))
                .arg(script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "bounded SDK driver timeout: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "SDK driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "SDK inspector evidence: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ok"],
        true
    );
}
async fn seed(fixture: &Fixture) -> serde_json::Value {
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
            events: vec![vcp_protocol::event::EventInput {
                id: EventId::new(),
                workspace: task.scope.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()),
                actor: fixture.config.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(9_007_199_254_740_993),
                kind: vcp_protocol::event::EventKind::Diagnostic,
                artifacts: vec![artifact.spec.id.clone()],
                data: json!({"schema_version":1,"diagnostic":"native-inspector-fixture"}),
                metadata: Some(vcp_protocol::event::EventMetadata {
                    agent: None,
                    provider: Some("offline-provider".into()),
                    model: Some("offline-model".into()),
                    paths: vec!["src/parser.rs".into()],
                }),
            }],
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
        origins: vec![origin.clone()],
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
    let mut previous = None;
    // Keep native debug startup within the unchanged SDK bootstrap deadline.
    // The lifecycle fixture independently qualifies the >128-version boundary;
    // this actual SDK fixture exercises five pages of seven versions per store.
    for index in 0..33 {
        let mut next = proposal.clone();
        next.id = ProposalId::new();
        next.command = CommandId::new();
        next.output_key = format!("history-{index}");
        next.predecessor = previous;
        next.correction_reason = next
            .predecessor
            .as_ref()
            .map(|_| "explicit fixture correction".into());
        next.statement = format!("Parser revision {index}");
        if let ClaimValue::Architecture { decision, .. } = &mut next.value {
            *decision = next.statement.clone();
        }
        previous = propose(&mut store, &access, next, Timestamp::new(200 + index))
            .await
            .unwrap()
            .result
            .version;
        assert!(previous.is_some());
    }
    let mut versions = Vec::new();
    let mut after = MemorySeq::ZERO;
    let mut at = None;
    loop {
        let (page, upper, more) =
            vcp_memory::history::window(&store, &access, &proposal.claim, at, after, 32).unwrap();
        at = Some(upper);
        for row in page.versions {
            after = row.memory_seq;
            versions.push(row.id);
        }
        if !more {
            break;
        }
    }
    assert_eq!(versions.len(), 33);
    let history_access = vcp_audit::history::Access {
        workspace: access.workspace.clone(),
        authority: access.authority,
        read: true,
        tasks: None,
    };
    let mut query = vcp_audit::history_query::Query {
        selector: Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Workspace(access.workspace.clone())),
        },
        text: None,
        limit: 7,
        cursor: None,
        artifact: None,
        expand_compacted: false,
    };
    let mut history = Vec::new();
    loop {
        let page = vcp_audit::history_query::query_session(
            store.state(),
            &history_access,
            &query,
            &task.scope.session,
        )
        .unwrap();
        for row in page.rows {
            history.push(json!({"id":row.event.event.id,"session":row.event.event.session,"task":row.event.event.task,"sequence":row.event.sequence.get().to_string(),"timestamp_ms":row.event.event.timestamp.get().to_string(),"visibility":row.visibility,"artifacts":row.artifact_links.iter().map(|a|json!({"id":a.id,"availability":a.availability,"original_bytes":a.original_bytes.map(|v|v.get().to_string())})).collect::<Vec<_>>()}));
        }
        query.cursor = page.next_cursor;
        if query.cursor.is_none() {
            break;
        }
    }
    let input = json!({"executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":fixture.scope(),"task":fixture.config.root_task,"claim":proposal.claim,"artifact":artifact.spec.id,"origin":origin,"versions":versions,"history":history,"watermark":store.state().watermark.get().to_string()});
    store.close().await.unwrap();
    input
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_inspector_queries_match_governed_cli_queries_without_mutation() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let input = seed(&fixture).await;
        let before = fixture.reopen().await;
        let state = serde_json::to_value(before.state()).unwrap();
        before.close().await.unwrap();
        driver(input).await;
        let after = fixture.reopen_within(Duration::from_secs(45)).await;
        assert_eq!(
            serde_json::to_value(after.state()).unwrap(),
            state,
            "observer queries cannot mutate canonical state"
        );
        assert_offline_paused(&after, &fixture.config);
        after.close().await.unwrap();
    }
}
