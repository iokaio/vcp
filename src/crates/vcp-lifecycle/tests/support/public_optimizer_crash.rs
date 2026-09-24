// SPDX-License-Identifier: Apache-2.0
//! Real owner death around the standard public receipt and policy transaction.
use super::*;
use std::{
    io::Write,
    process::{Command as ProcessCommand, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use vcp_lifecycle::foundation::routing_state::public_optimizer as public;
use vcp_store::Barrier;

#[tokio::test]
#[ignore = "launched and killed by public optimizer publication supervisor"]
async fn public_optimizer_crash_child() {
    let input = std::env::var("VCP_PUBLIC_OPTIMIZER_INPUT").unwrap();
    let root = std::env::var("VCP_PUBLIC_OPTIMIZER_ROOT").unwrap();
    let backend: BackendKind = std::env::var("VCP_PUBLIC_OPTIMIZER_BACKEND")
        .unwrap()
        .parse()
        .unwrap();
    let marker = std::env::var("VCP_PUBLIC_OPTIMIZER_MARKER").unwrap();
    let barrier = if std::env::var("VCP_PUBLIC_OPTIMIZER_POINT").unwrap() == "before" {
        Barrier::BeforeCommit
    } else {
        Barrier::AfterCommit
    };
    let (proposal, ceilings, command): (Preview, Policy, public::Command) =
        serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let access = Access {
        workspace: command.workspace.clone(),
        actor: command.actor.clone(),
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
            file.write_all(b"public optimizer atomic boundary").unwrap();
            file.sync_all().unwrap();
            loop {
                std::thread::park();
            }
        }
    }));
    public::apply(
        &mut store,
        &access,
        &command,
        &proposal,
        &ceilings,
        Timestamp::new(4),
        &|| Ok(()),
    )
    .await
    .unwrap();
    panic!("supervisor must terminate at publication barrier");
}

#[tokio::test]
async fn public_optimizer_kill_before_and_after_commit_preserves_receipt_binding_and_policy() {
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
            let session = store
                .state()
                .records
                .values()
                .find(|r| r.collection == Collection::Session)
                .unwrap()
                .decode::<Session>()
                .unwrap()
                .id;
            let command = public::Command {
                workspace: access.workspace.clone(),
                session,
                actor: access.actor.clone(),
                id: CommandId::parse("public-crash-apply").unwrap(),
                digest: vcp_protocol::digest_bytes(b"public crash exact command"),
                expected_revision: Revision::ZERO,
                expected_binding_revision: Revision::ZERO,
            };
            let input = temp.path().join("input.json");
            let marker = temp.path().join("barrier");
            std::fs::write(
                &input,
                serde_json::to_vec(&(&proposal, &ceilings, &command)).unwrap(),
            )
            .unwrap();
            store.close().await.unwrap();
            let mut child = ProcessCommand::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "public_optimizer_crash::public_optimizer_crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VCP_PUBLIC_OPTIMIZER_INPUT", &input)
                .env("VCP_PUBLIC_OPTIMIZER_ROOT", &root)
                .env(
                    "VCP_PUBLIC_OPTIMIZER_BACKEND",
                    if backend == BackendKind::Files {
                        "files"
                    } else {
                        "sqlite"
                    },
                )
                .env("VCP_PUBLIC_OPTIMIZER_MARKER", &marker)
                .env("VCP_PUBLIC_OPTIMIZER_POINT", point)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("public optimizer child exited before {point}: {status}")
                };
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("public optimizer barrier timed out")
                };
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut reopened = Store::open(&root, backend, &[]).await.unwrap();
            let retained = public::replay(&reopened, &access, &command).unwrap();
            assert_eq!(retained.is_some(), point == "after");
            let current = current_policy(&reopened, &access).unwrap().unwrap();
            assert_eq!(current.revision, Revision::new(u64::from(point == "after")));
            assert_eq!(
                current.value.quality_floor_bps,
                if point == "after" { 8000 } else { 7000 }
            );
            let before = reopened.state().clone();
            let accepted = public::apply(
                &mut reopened,
                &access,
                &command,
                &proposal,
                &ceilings,
                Timestamp::new(5),
                &|| Ok(()),
            )
            .await
            .unwrap();
            assert_eq!(
                accepted.receipt,
                reopened
                    .state()
                    .command(&access.workspace, &command.id, &command.digest)
                    .unwrap()
                    .unwrap()
            );
            let public::Outcome::Policy { value } = &accepted.outcome else {
                panic!("policy receipt required")
            };
            assert_eq!(value.published.revision, Revision::new(1));
            assert_eq!(value.published.value.quality_floor_bps, 8000);
            assert!(reopened
                .state()
                .events
                .iter()
                .any(|event| event.watermark == accepted.receipt.watermark
                    && event.event.correlation == command.id
                    && event.event.session == command.session));
            if point == "after" {
                assert_eq!(retained.unwrap(), accepted);
                assert_eq!(reopened.state(), &before);
            }
            reopened.close().await.unwrap();
            let reopened = Store::open(&root, backend, &[]).await.unwrap();
            assert_eq!(
                public::replay(&reopened, &access, &command)
                    .unwrap()
                    .unwrap(),
                accepted
            );
        }
    }
}
