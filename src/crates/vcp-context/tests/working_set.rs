// SPDX-License-Identifier: Apache-2.0
use serde_json::json;
use vcp_context::{
    manifest::*,
    working_set::{rebuild, Applicability},
};
use vcp_domain::{workspace::Scope, *};
use vcp_repository::FileVersion;

fn revisions() -> Revisions {
    Revisions {
        scope: Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        steering: SteeringRevision::ZERO,
        policy: PolicyRevision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        binding: Revision::ZERO,
        instructions: Revision::ZERO,
        tools: Revision::ZERO,
        skills: Revision::ZERO,
        memory: Revision::ZERO,
        task_state: Revision::ZERO,
    }
}
fn source() -> FileVersion {
    FileVersion {
        root: RootId::new(),
        binding: Revision::ZERO,
        path: "src/file.rs".into(),
        native_identity: "fixture-file".into(),
        sha256: "a".repeat(64),
        bytes: ByteCount::new(1000),
    }
}
fn pair(revisions: &Revisions, source: &FileVersion, id: &str, start: u64, end: u64) -> Vec<Part> {
    [Content::ToolCall { id: id.into(), name: "vcp_read".into(), arguments: json!({"path":source.path}) },
     Content::ToolResult { id: id.into(), output: json!({"result":{"version":source,"complete":false,
         "returned_range":{"start_line":start,"end_line":end},"next_line":end+1,"text":"observed range"}}).to_string() }]
    .into_iter().map(|content| {
        let bytes = content.bytes().unwrap();
        let artifact = ArtifactId::new();
        Part { id: artifact.to_string(), scope: revisions.scope.clone(),
            kind: if matches!(content, Content::ToolCall { .. }) { Kind::ToolCall } else { Kind::ToolResult },
            trust: Trust::Untrusted, artifact, source_hash: vcp_protocol::digest_bytes(&bytes),
            source_length: ByteCount::new(bytes.len() as u64), start: ByteCount::ZERO,
            end: ByteCount::new(bytes.len() as u64), content, mandatory: true, rank: 0,
            reason: "synthetic retained range".into(), applicable_paths: vec![], file: None }
    }).collect()
}

#[test]
fn rebuild_keeps_multiple_ranges_and_revalidates_once_per_file() {
    let revisions = revisions();
    let source = source();
    let mut history = pair(&revisions, &source, "first", 1, 20);
    history.extend(pair(&revisions, &source, "second", 80, 90));
    let mut reads = 0;
    let (index, probes) = rebuild(&history, &revisions, &source.root, |path| {
        assert_eq!(path, "src/file.rs");
        reads += 1;
        Some(source.clone())
    })
    .unwrap();
    assert_eq!(reads, 1);
    assert_eq!(index.entries.len(), 2);
    assert_eq!(index.entries[1].start_line, 80);
    assert_eq!(index.entries[0].artifact, history[1].artifact);
    assert!(index
        .entries
        .iter()
        .all(|entry| entry.applicability == Applicability::Current));
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].observed, Some(source.clone()));
    let (reopened, _) =
        rebuild(&history, &revisions, &source.root, |_| Some(source.clone())).unwrap();
    assert_eq!(
        serde_json::to_value(index).unwrap(),
        serde_json::to_value(reopened).unwrap()
    );
}

#[test]
fn edits_deletion_binding_and_scope_never_reuse_stale_ranges_as_current() {
    let revisions = revisions();
    let old = source();
    let mut changed = old.clone();
    changed.sha256 = "b".repeat(64);
    let mut history = pair(&revisions, &old, "old", 1, 20);
    let (index, probes) =
        rebuild(&history, &revisions, &old.root, |_| Some(changed.clone())).unwrap();
    assert_eq!(index.entries[0].applicability, Applicability::Changed);
    assert_eq!(probes[0].observed, Some(changed.clone()));
    let (missing, probes) = rebuild(&history, &revisions, &old.root, |_| None).unwrap();
    assert_eq!(missing.entries[0].applicability, Applicability::Unavailable);
    assert!(probes.is_empty());
    history.extend(pair(&revisions, &changed, "new", 30, 40));
    let (latest, _) = rebuild(&history, &revisions, &old.root, |_| Some(changed.clone())).unwrap();
    assert_eq!(latest.entries.len(), 1);
    assert_eq!(latest.entries[0].origin_call, "new");
    assert_eq!(latest.entries[0].applicability, Applicability::Current);
    let (rebound, _) = rebuild(&history, &revisions, &RootId::new(), |_| {
        panic!("old root must not be read")
    })
    .unwrap();
    assert!(rebound.entries.is_empty());
    let mut other_task = revisions.clone();
    other_task.scope.task = TaskId::new();
    assert!(rebuild(&history, &other_task, &old.root, |_| Some(changed.clone())).is_err());
    assert!(rebuild(&history[..3], &revisions, &old.root, |_| Some(
        changed.clone()
    ))
    .is_err());
}
