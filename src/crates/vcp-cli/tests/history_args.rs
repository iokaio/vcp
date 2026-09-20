// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use vcp_cli::{
    args::{Cli, Command},
    history::{Filter, History},
};
use vcp_domain::{
    retention_selector::{Facts, Truth},
    *,
};
#[test]
fn explicit_dates_share_the_same_selector_and_never_guess_local_time() {
    let workspace = WorkspaceId::parse("workspace").unwrap();
    let filter = Filter {
        since: Some("2026-01-01".into()),
        before: Some("2026-01-02".into()),
        utc_offset_minutes: Some(-420),
        ..Default::default()
    };
    let selector = filter.normalized(&workspace).unwrap();
    let timestamp =
        vcp_domain::retention_selector::InstantSpec::parse("2026-01-02T07:00:00Z", None)
            .unwrap()
            .utc;
    let facts = Facts {
        workspace: &workspace,
        timestamp: Some(timestamp),
        roots: None,
        paths: None,
        task: None,
        actor: None,
        agent: None,
        model: None,
        provider: None,
        event: None,
        claim: None,
        task_status: None,
        claim_status: None,
        superseded: None,
    };
    assert_eq!(selector.evaluate(&facts).unwrap(), Truth::NoMatch);
    assert!(Filter {
        utc_offset_minutes: None,
        ..filter
    }
    .normalized(&workspace)
    .is_err());
    let cli = Cli::try_parse_from([
        "vcp",
        "history",
        "prune",
        "--preview",
        "--before",
        "2026-01-02T07:00:00Z",
        "--action",
        "exclude",
    ])
    .unwrap();
    let Some(Command::History {
        command: History::Prune(preview),
    }) = cli.command
    else {
        panic!("wrong command")
    };
    assert!(preview.request(&workspace).is_ok());
    assert!(Cli::try_parse_from(["vcp", "history", "prune"]).is_err());
    assert!(Cli::try_parse_from(["vcp", "retention", "set"]).is_err());
    assert!(Cli::try_parse_from(["vcp", "retention", "set", "--notification-only"]).is_ok());
    let memory =
        Cli::try_parse_from(["vcp", "memory", "inspect", "claim-example", "--limit", "2"]).unwrap();
    let Some(Command::Memory {
        command: vcp_cli::args::Memory::Inspect(inspect),
    }) = memory.command
    else {
        panic!("memory inspect parser")
    };
    assert!(inspect.request().is_ok());
    assert!(Cli::try_parse_from(["vcp", "memory", "prune"]).is_err());
    assert!(vcp_cli::history::terminal_request(
        vec!["memory".into(), "inspect".into(), "claim-example".into()],
        &workspace
    )
    .is_ok());
    let Some(vcp_cli::terminal::Input::Maintenance(words)) =
        vcp_cli::terminal::parse("/history list --limit 2").unwrap()
    else {
        panic!("typed terminal control")
    };
    assert!(vcp_cli::history::terminal_request(words, &workspace).is_ok());
    assert!(vcp_cli::history::terminal_request(
        vec!["prune".into(), "apply".into(), "not-a-preview".into()],
        &workspace
    )
    .is_err());
    assert!(vcp_cli::history::terminal_request(
        vec![
            "history".into(),
            "list".into(),
            "--workspace".into(),
            "foreign".into()
        ],
        &workspace
    )
    .is_err());
}
