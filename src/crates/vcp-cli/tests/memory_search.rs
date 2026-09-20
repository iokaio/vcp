// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use vcp_cli::args::{Cli, ValidatedCommand};

#[test]
fn memory_search_scope_and_bounds_are_validated_before_state_is_opened() {
    let workspace = tempfile::tempdir().unwrap();
    let parse = |extra: &[&str]| {
        let mut args = vec![
            "vcp",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "memory",
            "search",
            "pause_session",
        ];
        args.extend_from_slice(extra);
        Cli::try_parse_from(args)
            .map_err(|e| e.to_string())
            .and_then(|cli| cli.validate(None))
    };
    for extra in [
        vec!["--path", "../outside"],
        vec!["--path", "C:/outside"],
        vec!["--limit", "65"],
        vec!["--tokens", "1"],
        vec!["--root", "../root"],
    ] {
        assert!(parse(&extra).is_err(), "{extra:?}");
    }
    let cli = parse(&[
        "--task",
        "task",
        "--root",
        "root",
        "--path",
        "src/Main.rs",
        "--symbol",
        "pause_session",
        "--minimum-sequence",
        "4",
    ])
    .unwrap();
    let ValidatedCommand::MemorySearch(search) = cli.command else {
        panic!("typed search expected")
    };
    let request = search.request(vcp_domain::WorkspaceId::parse("workspace").unwrap());
    assert_eq!(request.paths.unwrap(), vec!["src/Main.rs"]);
    assert_eq!(request.tasks.unwrap()[0].as_str(), "task");
    assert_eq!(request.minimum_sequence.unwrap().get(), 4);
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
}

#[cfg(windows)]
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;

#[cfg(windows)]
#[tokio::test]
async fn offline_search_declares_missing_generation_without_creating_it_or_mutating_history() {
    use vcp_store::contract::CanonicalStore;
    for backend in [
        vcp_store::BackendKind::Files,
        vcp_store::BackendKind::Sqlite,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = vcp_store::Store::open(temp.path(), backend, &[])
            .await
            .unwrap();
        store.transact(common::initial()).await.unwrap();
        let scope = common::task().scope;
        let request = vcp_memory::retrieval::Request {
            workspace: scope.workspace.clone(),
            tasks: None,
            roots: None,
            paths: None,
            symbols: None,
            text: "pause".into(),
            historical: None,
            minimum_sequence: None,
            timeout_ms: 5000,
            results: 4,
            tokens: 4096,
            bytes: 65536,
        };
        let before = store.state().clone();
        let query = vcp_cli::app::Query::MemorySearch { request };
        let result = vcp_cli::app::query_store(
            &store,
            &scope.workspace,
            &vcp_domain::ActorId::parse("owner").unwrap(),
            &query,
        )
        .unwrap();
        assert!(result["passages"].as_array().unwrap().is_empty());
        assert!(!result["degraded"].as_array().unwrap().is_empty());
        assert_eq!(store.state(), &before);
        assert!(!temp.path().join("search-generations").exists());
        store.close().await.unwrap();
    }
}
