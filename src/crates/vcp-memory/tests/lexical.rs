// SPDX-License-Identifier: Apache-2.0
use std::sync::atomic::AtomicBool;
use vcp_domain::{
    artifact::Range,
    memory::{EvidenceStatus, Outcome},
    workspace::Scope,
    *,
};
use vcp_memory::{
    lexical::{self, Limits, Query},
    search_record::{ChunkerSpec, Inventory, SearchKind, SearchRecord, TextSource},
};

fn record(name: &str, task: &str, path: &str, symbol: &str, text: &str) -> SearchRecord {
    SearchRecord {
        id: String::new(),
        scope: Scope {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            task: TaskId::parse(task).unwrap(),
        },
        root: RootId::parse("root").unwrap(),
        paths: vec![path.into()],
        symbols: vec![symbol.into()],
        kind: SearchKind::Source,
        claim_kind: None,
        source: TextSource::Artifact {
            id: ArtifactId::parse(name).unwrap(),
        },
        source_digest: vcp_protocol::digest_bytes(text.as_bytes()),
        span: Range {
            start: ByteCount::ZERO,
            end: ByteCount::new(text.len() as u64),
        },
        applicability: None,
        status: Outcome::Accepted,
        evidence_status: EvidenceStatus::Observed,
        memory_seq: MemorySeq::ZERO,
        watermark: Watermark::new(1),
        text: text.into(),
    }
}
fn inventory(mut records: Vec<SearchRecord>) -> Inventory {
    let chunker_digest = ChunkerSpec::default().digest().unwrap();
    for record in &mut records {
        record.id = record.calculate_id(&chunker_digest).unwrap();
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    let mut result = Inventory {
        workspace: WorkspaceId::parse("workspace").unwrap(),
        watermark: Watermark::new(1),
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        chunker_digest,
        records,
        exclusions: vec![],
        digest: String::new(),
    };
    result.digest = result.calculate_digest().unwrap();
    result.validate().unwrap();
    result
}
fn query(text: &str) -> Query {
    Query {
        workspace: WorkspaceId::parse("workspace").unwrap(),
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        kind: None,
        claim_kind: None,
        status: None,
        text: text.into(),
        phrase: false,
        limit: 10,
    }
}

#[test]
fn exact_code_scope_and_phrases_are_filtered_before_rank_limits() {
    let records = vec![
        record(
            "source-a",
            "task-a",
            "src/Parser.rs",
            "Parser::readHTTP",
            "read write parser snake_case x",
        ),
        record(
            "source-b",
            "task-b",
            "src/parser.rs",
            "parser::read_http",
            "read intervening write parser snake_case x",
        ),
        record(
            "source-c",
            "task-c",
            "src/日本.rs",
            "短い",
            "unicode path identifier",
        ),
    ];
    let inventory = inventory(records);
    let temp = tempfile::tempdir().unwrap();
    let reader = lexical::build(
        temp.path(),
        &inventory,
        Limits {
            batch_documents: 1,
            ..Limits::default()
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let id = |name| {
        inventory
            .records
            .iter()
            .find(|r| r.scope.task.as_str() == name)
            .unwrap()
            .id
            .clone()
    };
    let mut q = query("parser");
    q.tasks = Some(vec![TaskId::parse("task-b").unwrap()]);
    q.limit = 1;
    assert_eq!(reader.search(&q).unwrap()[0].id, id("task-b"));
    q.tasks = Some(vec![]);
    assert!(reader.search(&q).unwrap().is_empty());
    let mut q = query("");
    q.paths = Some(vec!["src/Parser.rs".into()]);
    assert_eq!(
        reader
            .search(&q)
            .unwrap()
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        vec![id("task-a")]
    );
    q.paths = Some(vec!["src/PARSER.rs".into()]);
    assert!(reader.search(&q).unwrap().is_empty());
    q.paths = Some(vec!["src/日本.rs".into()]);
    assert_eq!(reader.search(&q).unwrap()[0].id, id("task-c"));
    let mut q = query("");
    q.symbols = Some(vec!["Parser::readHTTP".into()]);
    assert_eq!(reader.search(&q).unwrap()[0].id, id("task-a"));
    for text in ["snake", "case", "x", "HTTP", "readHTTP"] {
        assert!(
            reader
                .search(&query(text))
                .unwrap()
                .iter()
                .any(|hit| hit.id == id("task-a")),
            "{text}"
        );
    }
    let mut q = query("read write");
    q.phrase = true;
    assert_eq!(
        reader
            .search(&q)
            .unwrap()
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        vec![id("task-a")]
    );
    let mut q = query("parser");
    q.workspace = WorkspaceId::parse("foreign").unwrap();
    assert!(reader.search(&q).is_err());
    let encoded = serde_json::to_value(reader.search(&query("parser")).unwrap()).unwrap();
    assert!(encoded.as_array().unwrap().iter().all(|candidate| candidate
        .as_object()
        .unwrap()
        .keys()
        .all(|key| ["id", "rank", "score"].contains(&key.as_str()))));
}

#[test]
fn replacement_deletes_old_versions_while_pinned_readers_keep_stable_ids() {
    let old = inventory(vec![record(
        "old",
        "task",
        "old.rs",
        "old_name",
        "old unique version",
    )]);
    let new = inventory(vec![record(
        "new",
        "task",
        "new.rs",
        "new_name",
        "new unique version",
    )]);
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let old_reader = lexical::build(
        first.path(),
        &old,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let new_reader = lexical::build(
        second.path(),
        &new,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        old_reader.search(&query("old_name")).unwrap()[0].id,
        old.records[0].id
    );
    let mut deleted = query("");
    deleted.symbols = Some(vec!["old_name".into()]);
    assert!(new_reader.search(&deleted).unwrap().is_empty());
    assert_eq!(
        new_reader.search(&query("new_name")).unwrap()[0].id,
        new.records[0].id
    );
    drop(new_reader);
    let reopened = lexical::open(second.path(), &new, Limits::default()).unwrap();
    assert_eq!(
        reopened.search(&query("new_name")).unwrap()[0].id,
        new.records[0].id
    );
    assert!(lexical::open(second.path(), &old, Limits::default()).is_err());
}

#[test]
fn cancellation_limits_and_corrupt_components_never_yield_readers() {
    let inventory = inventory(vec![record(
        "one",
        "task",
        "one.rs",
        "one",
        "retained content",
    )]);
    let temp = tempfile::tempdir().unwrap();
    assert!(lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(lexical::build(
        temp.path(),
        &inventory,
        Limits {
            source_bytes: 1,
            ..Limits::default()
        },
        &AtomicBool::new(false)
    )
    .is_err());
    let reader = lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(reader.len(), 1);
    drop(reader);
    assert!(lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(false)
    )
    .is_err());
    let metadata = temp.path().join("meta.json");
    let original = std::fs::read(&metadata).unwrap();
    std::fs::write(&metadata, b"{}").unwrap();
    assert!(lexical::open(temp.path(), &inventory, Limits::default()).is_err());
    std::fs::write(&metadata, original).unwrap();
    let manifest = temp.path().join("vcp-lexical.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    value["tokenizer"] = serde_json::json!("foreign-version");
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(lexical::open(temp.path(), &inventory, Limits::default()).is_err());
}

#[test]
fn cancellation_during_private_build_never_writes_readiness_manifest() {
    let inventory = inventory(
        (0..256)
            .map(|n| {
                record(
                    &format!("source-{n}"),
                    "task",
                    "src.rs",
                    "symbol",
                    "bounded cancellation fixture",
                )
            })
            .collect(),
    );
    let temp = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let flag = &cancel;
        let path = temp.path();
        scope.spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !path.join("meta.json").exists() && std::time::Instant::now() < deadline {
                std::thread::yield_now();
            }
            flag.store(true, std::sync::atomic::Ordering::Release);
        });
        assert!(lexical::build(
            temp.path(),
            &inventory,
            Limits {
                batch_documents: 1,
                ..Limits::default()
            },
            &cancel
        )
        .is_err());
    });
    assert!(!temp.path().join("vcp-lexical.json").exists());
    assert!(lexical::open(temp.path(), &inventory, Limits::default()).is_err());
}

#[test]
fn empty_inventory_is_a_valid_empty_component() {
    let inventory = inventory(vec![]);
    let temp = tempfile::tempdir().unwrap();
    let reader = lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(reader.is_empty());
    assert!(reader.search(&query("anything")).unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn windows_junction_cannot_redirect_private_component_storage() {
    let owned = tempfile::tempdir().unwrap();
    let target = owned.path().join("target");
    let link = owned.path().join("redirect");
    std::fs::create_dir(&target).unwrap();
    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile", "-NonInteractive", "-Command",
            "New-Item -ItemType Junction -Path $env:VCP_LEXICAL_LINK -Target $env:VCP_LEXICAL_TARGET -ErrorAction Stop | Out-Null",
        ])
        .env("VCP_LEXICAL_LINK", &link)
        .env("VCP_LEXICAL_TARGET", &target)
        .status().unwrap();
    assert!(status.success());
    let inventory = inventory(vec![]);
    assert!(lexical::build(
        &link,
        &inventory,
        Limits::default(),
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(std::fs::read_dir(&target).unwrap().next().is_none());
    let reader = lexical::build(
        &target,
        &inventory,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(lexical::open(&link, &inventory, Limits::default()).is_err());
    drop(reader);
    std::fs::remove_dir(&link).unwrap();
}
