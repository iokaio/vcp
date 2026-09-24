// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual SDK report commands and receipt recovery; no provider dispatch.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vcp_domain::{task::Task, Timestamp};
use vcp_models::routing as model;
use vcp_store::{contract::Collection, BackendKind};

fn policy() -> model::Policy {
    model::Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: model::Profile::Low,
        allowed_models: BTreeSet::new(),
        allowed_endpoints: BTreeSet::new(),
        allowed_groups: BTreeSet::from([model::Group::Low]),
        quality_floor_bps: 7000,
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
async fn driver(input: Value) {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-ts/tests/native-optimizer-commands.mjs");
    let bytes = serde_json::to_vec(&input).unwrap();
    let output = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(std::env::var_os("VCP_TEST_NODE").expect("qualified Node"))
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
                let out = child.wait_with_output().unwrap();
                panic!(
                    "optimizer deadline: {}",
                    String::from_utf8_lossy(&out.stderr)
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
        "optimizer driver: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "optimizer SDK evidence: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ok"],
        true
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_optimizer_commands_reconcile_without_provider_dispatch() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        println!("optimizer native backend: {backend:?}");
        let server = wiremock::MockServer::start().await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "complete");
        let profile: Value =
            serde_json::from_slice(&std::fs::read(&fixture.profile).unwrap()).unwrap();
        let entry = fixture.seed(backend).await;
        // The seed deliberately stops before profile installation. Publish the
        // fixture policy through the real governed service, without host ceilings.
        let mut seed = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        let workspace: vcp_domain::workspace::Workspace = seed
            .state()
            .record(
                Collection::Workspace,
                entry.config.workspace.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(workspace.trust, vcp_domain::workspace::Trust::Trusted);
        let access = vcp_memory::access::Access {
            workspace: entry.config.workspace.clone(),
            actor: entry.config.actor.clone(),
            authority: workspace.authority,
            read: true,
            write: true,
            tasks: None,
        };
        vcp_lifecycle::foundation::routing_state::initialize_policy(
            &mut seed,
            &access,
            policy(),
            Timestamp::new(1),
        )
        .await
        .unwrap();
        seed.close().await.unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
        driver(json!({"executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,
            "root":entry.config.binding.root,"scope":{"workspace":entry.config.workspace,"session":entry.config.session},"task":entry.config.root_task})).await;
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "optimizer must never dispatch a provider request"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "41\n"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&fixture.profile).unwrap()).unwrap(),
            profile,
            "no profile writes"
        );
        let store = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                entry.config.root_task.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, vcp_domain::task::TaskState::Paused);
        for command in ["optimizer-session-report", "optimizer-workspace-report"] {
            let receipts: Vec<_> = store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == command)
                .collect();
            assert_eq!(receipts.len(), 1);
            assert!(store
                .state()
                .events
                .iter()
                .any(|event| event.event.correlation == receipts[0].command
                    && event.event.session == entry.config.session
                    && event.event.actor == entry.config.actor
                    && event.watermark == receipts[0].watermark));
        }
        assert!(!store
            .state()
            .commands
            .values()
            .any(|r| r.command.as_str() == "optimizer-no-preview"));
        let workspace: vcp_domain::workspace::Workspace = store
            .state()
            .record(
                Collection::Workspace,
                entry.config.workspace.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let access = vcp_memory::access::Access {
            workspace: entry.config.workspace.clone(),
            actor: entry.config.actor.clone(),
            authority: workspace.authority,
            read: true,
            write: false,
            tasks: None,
        };
        let policy = vcp_lifecycle::foundation::routing_state::current_policy(&store, &access)
            .unwrap()
            .unwrap();
        assert_eq!(policy.revision.get(), 0);
        assert_eq!(policy.value.quality_floor_bps, 7000);
        assert!(policy.value.parent.is_none());
        store.close().await.unwrap();
    }
}
