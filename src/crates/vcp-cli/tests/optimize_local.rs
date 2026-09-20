// SPDX-License-Identifier: Apache-2.0
#[test]
fn executable_optimizer_help_needs_no_profile_or_large_stack_override() {
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_vcp"))
        .args(["optimize", "--help"])
        .current_dir(temp.path())
        .env_remove("RUST_MIN_STACK")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8_lossy(&output.stderr);
    assert!(help.contains("report") && help.contains("compare"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;
use clap::Parser;
use vcp_cli::{
    args::{Cli, ValidatedCommand},
    optimize::offline::{self, Command, LocalQuestion},
};
use vcp_domain::*;
use vcp_store::{
    contract::{CanonicalStore, Collection},
    BackendKind, Store,
};

#[test]
fn optimize_validates_without_budget_or_profile_and_rejects_unknown_mutations() {
    let workspace = tempfile::tempdir().unwrap();
    let cli = Cli::try_parse_from([
        "vcp",
        "--workspace",
        workspace.path().to_str().unwrap(),
        "--config",
        "does-not-exist.json",
        "optimize",
        "status",
    ])
    .unwrap()
    .validate(None)
    .unwrap();
    assert!(matches!(
        cli.command,
        ValidatedCommand::Optimize(Command::Status)
    ));
    assert!(Cli::try_parse_from(["vcp", "optimize", "apply"]).is_err());
    assert!(Command::Report {
        from: Some(20),
        until: Some(10)
    }
    .validate()
    .is_err());
    assert!(Command::Compare {
        baseline: "../foreign".into(),
        current: "report".into()
    }
    .validate()
    .is_err());
}

#[tokio::test]
async fn local_optimizer_persists_reports_and_answers_without_a_budget_or_provider() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let access = vcp_memory::access::Access {
            workspace: common::workspace().id,
            actor: ActorId::parse("human").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let cutoff = store.state().watermark;
        let status =
            offline::execute_store(&mut store, &access, &Command::Status, Timestamp::new(200))
                .await
                .unwrap();
        assert!(status["policy"].is_null());
        assert!(status["registry"].is_null());
        assert_eq!(store.state().watermark, cutoff);
        let report = offline::execute_store(
            &mut store,
            &access,
            &Command::Report {
                from: None,
                until: Some(200),
            },
            Timestamp::new(200),
        )
        .await
        .unwrap();
        assert_eq!(report["report"]["counts"]["tasks"], 1);
        let report_id = report["report"]["id"].as_str().unwrap().to_owned();
        let result = offline::execute_store(
            &mut store,
            &access,
            &Command::Answer {
                question: LocalQuestion::Priority,
                value: "spend".into(),
                expected_revision: None,
            },
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(result["interview"]["answers"]["priority"], "spend");
        let another = offline::execute_store(
            &mut store,
            &access,
            &Command::Report {
                from: None,
                until: Some(200),
            },
            Timestamp::new(202),
        )
        .await
        .unwrap();
        let compared = offline::execute_store(
            &mut store,
            &access,
            &Command::Compare {
                baseline: report_id,
                current: another["report"]["id"].as_str().unwrap().into(),
            },
            Timestamp::new(203),
        )
        .await
        .unwrap();
        assert_eq!(compared["automatic_action"], false);
        assert_eq!(compared["comparable"], false); // No declared model-attempt cohorts.
        for collection in [
            Collection::Ledger,
            Collection::Attempt,
            Collection::Reservation,
        ] {
            assert!(store
                .state()
                .records
                .values()
                .all(|r| r.collection != collection));
        }
        assert!(!store.state().events.iter().any(|event| matches!(
            event.event.kind,
            vcp_protocol::event::EventKind::AttemptSubmitted
                | vcp_protocol::event::EventKind::ReservationCreated
        )));
        store.close().await.unwrap();
        let mut reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        let status = offline::execute_store(
            &mut reopened,
            &access,
            &Command::Status,
            Timestamp::new(204),
        )
        .await
        .unwrap();
        assert_eq!(status["interview"]["answers"]["priority"], "spend");
        reopened.close().await.unwrap();
    }
}
