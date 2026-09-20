// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use vcp_store::Barrier;

#[tokio::test]
#[ignore = "launched and terminated by routing publication supervisor"]
async fn routing_policy_crash_child() {
    let input = std::env::var("VCP_ROUTING_CRASH_INPUT").unwrap();
    let root = std::env::var("VCP_ROUTING_CRASH_ROOT").unwrap();
    let backend: BackendKind = std::env::var("VCP_ROUTING_CRASH_BACKEND")
        .unwrap()
        .parse()
        .unwrap();
    let marker = std::env::var("VCP_ROUTING_CRASH_MARKER").unwrap();
    let barrier = if std::env::var("VCP_ROUTING_CRASH_POINT").unwrap() == "before" {
        Barrier::BeforeCommit
    } else {
        Barrier::AfterCommit
    };
    let (proposal, ceilings, workspace, actor): (Preview, Policy, WorkspaceId, ActorId) =
        serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let access = Access {
        workspace,
        actor,
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let mut store = Store::open(std::path::Path::new(&root), backend, &[])
        .await
        .unwrap();
    store.observe(Arc::new(move |point| {
        if point == barrier {
            let mut file = std::fs::File::create(&marker).unwrap();
            file.write_all(b"publication barrier").unwrap();
            file.sync_all().unwrap();
            loop {
                std::thread::park();
            }
        }
    }));
    apply(
        &mut store,
        &access,
        CommandId::parse("crash-apply").unwrap(),
        &proposal,
        &ceilings,
        Timestamp::new(4),
    )
    .await
    .unwrap();
    panic!("supervisor must terminate at publication barrier");
}

#[tokio::test]
async fn process_kill_before_and_after_publication_reopens_exact_policy_and_command_receipt() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for point in ["before", "after"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("store");
            let (mut store, access) = setup(&root, backend).await;
            let ceilings = policy();
            initialize_policy(&mut store, &access, ceilings.clone(), Timestamp::new(2))
                .await
                .unwrap();
            let report = save_report(
                &mut store,
                &access,
                HistoryWindow {
                    from: None,
                    until: Timestamp::new(3),
                },
                Timestamp::new(3),
            )
            .await
            .unwrap();
            let proposal = preview(
                &store,
                &access,
                &report.id,
                vec![Edit::QualityFloorBps(8000)],
                &ceilings,
            )
            .unwrap();
            let input = temp.path().join("input.json");
            let marker = temp.path().join("barrier");
            std::fs::write(
                &input,
                serde_json::to_vec(&(&proposal, &ceilings, &access.workspace, &access.actor))
                    .unwrap(),
            )
            .unwrap();
            store.close().await.unwrap();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "routing_crash::routing_policy_crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VCP_ROUTING_CRASH_INPUT", &input)
                .env("VCP_ROUTING_CRASH_ROOT", &root)
                .env(
                    "VCP_ROUTING_CRASH_BACKEND",
                    if backend == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .env("VCP_ROUTING_CRASH_MARKER", &marker)
                .env("VCP_ROUTING_CRASH_POINT", point)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("routing child exited before {point}: {status}");
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("routing publication barrier timed out");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut reopened = Store::open(&root, backend, &[]).await.unwrap();
            let current = current_policy(&reopened, &access).unwrap().unwrap();
            assert_eq!(current.revision, Revision::new(u64::from(point == "after")));
            assert_eq!(
                current.value.quality_floor_bps,
                if point == "after" { 8000 } else { 7000 }
            );
            let watermark = reopened.state().watermark;
            let result = apply(
                &mut reopened,
                &access,
                CommandId::parse("crash-apply").unwrap(),
                &proposal,
                &ceilings,
                Timestamp::new(5),
            )
            .await
            .unwrap();
            assert_eq!(result.published.revision, Revision::new(1));
            if point == "after" {
                assert_eq!(reopened.state().watermark, watermark);
            }
            reopened.close().await.unwrap();
        }
    }
}
