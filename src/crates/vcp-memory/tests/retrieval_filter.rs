// SPDX-License-Identifier: Apache-2.0
//! Canonical eligibility must constrain the native collector, not its output.
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
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

fn record(name: &str, text: &str) -> SearchRecord {
    SearchRecord {
        id: String::new(),
        scope: Scope {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            task: TaskId::parse("task").unwrap(),
        },
        root: RootId::parse("root").unwrap(),
        paths: vec!["src/allowed.rs".into()],
        symbols: vec!["Allowed::read".into()],
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
    let mut inventory = Inventory {
        workspace: WorkspaceId::parse("workspace").unwrap(),
        watermark: Watermark::new(1),
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        chunker_digest,
        records,
        exclusions: vec![],
        digest: String::new(),
    };
    inventory.digest = inventory.calculate_digest().unwrap();
    inventory.validate().unwrap();
    inventory
}
fn query() -> Query {
    Query {
        workspace: WorkspaceId::parse("workspace").unwrap(),
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        kind: None,
        claim_kind: None,
        status: None,
        text: "needle".into(),
        phrase: false,
        limit: 100,
    }
}
fn id(inventory: &Inventory, name: &str) -> String {
    inventory
        .records
        .iter()
        .find(|record| matches!(&record.source, TextSource::Artifact {id} if id.as_str()==name))
        .unwrap()
        .id
        .clone()
}

#[test]
fn a_hundred_ineligible_high_scores_cannot_starve_a_current_match() {
    let mut records: Vec<_> = (0..105)
        .map(|i| record(&format!("stale-{i}"), "needle needle needle"))
        .collect();
    records.push(record(
        "current",
        &format!("needle {}", "unrelated ".repeat(200)),
    ));
    let inventory = inventory(records);
    let current = id(&inventory, "current");
    let temp = tempfile::tempdir().unwrap();
    let reader = lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let query = query();
    let unfiltered = reader.search(&query).unwrap();
    assert_eq!(unfiltered.len(), 100);
    assert!(
        unfiltered.iter().all(|row| row.id != current),
        "fixture must reproduce post-ranking starvation"
    );
    let allowed = BTreeSet::from([current.clone()]);
    let selected = reader.search_authorized(&query, &allowed).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, current);
    assert_eq!(selected[0].rank, 1);
    assert!(reader
        .search_authorized(&query, &BTreeSet::new())
        .unwrap()
        .is_empty());
    let excessive: BTreeSet<_> = (0..=lexical::MAX_AUTHORIZED_IDS)
        .map(|i| format!("{i:064x}"))
        .collect();
    assert!(reader.search_authorized(&query, &excessive).is_err());
    assert!(reader
        .search_authorized(&query, &BTreeSet::from(["not-a-canonical-id".into()]))
        .is_err());
}

#[test]
fn authorized_membership_does_not_expand_task_root_path_symbol_or_status_scope() {
    let mut records = vec![record("current", "needle")];
    let mut task = record("other-task", "needle needle");
    task.scope.task = TaskId::parse("other-task").unwrap();
    records.push(task);
    let mut root = record("other-root", "needle needle");
    root.root = RootId::parse("other-root").unwrap();
    records.push(root);
    let mut path = record("other-path", "needle needle");
    path.paths = vec!["src/Allowed.rs".into()];
    records.push(path);
    let mut symbol = record("other-symbol", "needle needle");
    symbol.symbols = vec!["Allowed::write".into()];
    records.push(symbol);
    let mut status = record("other-status", "needle needle");
    status.status = Outcome::Disputed;
    records.push(status);
    let inventory = inventory(records);
    let temp = tempfile::tempdir().unwrap();
    let reader = lexical::build(
        temp.path(),
        &inventory,
        Limits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let allowed = inventory.records.iter().map(|r| r.id.clone()).collect();
    let mut query = query();
    query.tasks = Some(vec![TaskId::parse("task").unwrap()]);
    query.roots = Some(vec![RootId::parse("root").unwrap()]);
    query.paths = Some(vec!["src/allowed.rs".into()]);
    query.symbols = Some(vec!["Allowed::read".into()]);
    query.status = Some(Outcome::Accepted);
    let expected = reader.search(&query).unwrap();
    let selected = reader.search_authorized(&query, &allowed).unwrap();
    assert_eq!(selected, expected, "eligibility adds no relevance score");
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, id(&inventory, "current"));
    query.workspace = WorkspaceId::parse("foreign-workspace").unwrap();
    assert!(reader.search_authorized(&query, &allowed).is_err());
}
