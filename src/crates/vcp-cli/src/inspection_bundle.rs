// SPDX-License-Identifier: Apache-2.0
//! Amortize canonical replay across the standard task evidence views. This is a
//! read-only projection of one validated state, never a replay-validation cache.
use serde_json::{json, Map, Value};
use vcp_audit::{
    history::Access,
    history_query,
    inspection::{self, InspectionQuery, View},
};
use vcp_domain::{
    retention_selector::{Criterion, Selector, Tree},
    task::Task,
    TaskId,
};
use vcp_store::contract::{Collection, State};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PAGES: usize = 128;

#[derive(Default)]
struct Budget {
    pages: usize,
    bytes: usize,
}
impl Budget {
    fn page(&mut self, page: &Value) -> Result<(), String> {
        self.pages += 1;
        self.bytes += serde_json::to_vec(page).map_err(|e| e.to_string())?.len();
        if self.pages > MAX_PAGES || self.bytes > MAX_BYTES {
            return Err(
                "inspection bundle limit exceeded; use paged inspect/history commands".into(),
            );
        }
        Ok(())
    }
}

pub fn collect(state: &State, access: &Access, task: &TaskId) -> Result<Value, String> {
    // Match the audit history boundary before exposing task/agent data. The
    // inspector and history queries independently recheck it for every page.
    let workspace: vcp_domain::workspace::Workspace = state
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if !access.read
        || workspace.authority != access.authority
        || access
            .tasks
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(task))
    {
        return Err("inspection bundle task access denied".into());
    }
    let task_record: Task = state
        .record(Collection::Task, task.as_str(), &access.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let mut budget = Budget::default();
    let now = vcp_domain::Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
            .min(u64::MAX as u128) as u64,
    );
    let mut agents = Vec::new();
    let mut offset = 0;
    loop {
        let page = crate::agents_view::page(state, &task_record.scope, now, offset)?;
        if let Some(allowed) = &access.tasks {
            for item in page["items"].as_array().ok_or("invalid agent page")? {
                let id = TaskId::parse(item["task"].as_str().ok_or("invalid agent task")?)
                    .map_err(|e| e.to_string())?;
                if !allowed.contains(&id) {
                    return Err("inspection bundle agent access denied".into());
                }
            }
        }
        budget.page(&page)?;
        let next = page["next_offset"].as_u64();
        agents.push(page);
        let Some(next) = next else {
            break;
        };
        offset = next.try_into().map_err(|_| "agent offset overflow")?;
    }
    let mut views = Map::new();
    for (name, view) in [
        ("costs", View::Costs),
        ("verification", View::Verification),
        ("tools", View::Tools),
        ("routing", View::Routing),
        ("policy", View::Policy),
        ("outputs", View::Outputs),
    ] {
        let mut request = InspectionQuery {
            id: task.to_string(),
            view,
            limit: 128,
            cursor: None,
            range: None,
        };
        let mut pages = Vec::new();
        loop {
            let page = inspection::records(state, access, &request).map_err(|e| e.to_string())?;
            request.cursor = page.next_cursor.clone();
            let value = serde_json::to_value(page).map_err(|e| e.to_string())?;
            budget.page(&value)?;
            pages.push(value);
            if request.cursor.is_none() {
                break;
            }
        }
        views.insert(name.into(), Value::Array(pages));
    }
    let mut request = history_query::Query {
        selector: Selector {
            schema_version: 1,
            tree: Tree::All(vec![
                Tree::Match(Criterion::Workspace(access.workspace.clone())),
                Tree::Match(Criterion::Task(task.clone())),
            ]),
        },
        text: None,
        limit: 128,
        cursor: None,
        artifact: None,
        expand_compacted: false,
    };
    let mut history = Vec::new();
    loop {
        let page = history_query::query(state, access, &request).map_err(|e| e.to_string())?;
        request.cursor = page.next_cursor.clone();
        let value = serde_json::to_value(page).map_err(|e| e.to_string())?;
        budget.page(&value)?;
        history.push(value);
        if request.cursor.is_none() {
            break;
        }
    }
    let value = json!({"schema_version":1,"kind":"inspection_bundle","source_watermark":state.watermark,
        "task":task_record,"views":views,"history":history,"agents":agents});
    if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err(
            "inspection bundle byte limit exceeded; use paged inspect/history commands".into(),
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::{artifact::*, task::*, verification::Fingerprint, workspace::*, *};
    use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
    use vcp_store::contract::Record;

    fn fixture() -> (State, Access, TaskId) {
        let workspace = WorkspaceId::new();
        let session = SessionId::new();
        let task = TaskId::new();
        let access = Access {
            workspace: workspace.clone(),
            authority: AuthorityRevision::ZERO,
            read: true,
            tasks: None,
        };
        let scope = Scope {
            workspace: workspace.clone(),
            session: session.clone(),
            task: task.clone(),
        };
        let mut state = State::default();
        let workspace_record = Record::typed(
            Collection::Workspace,
            workspace.to_string(),
            workspace.clone(),
            Revision::ZERO,
            &Workspace {
                id: workspace.clone(),
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
                trust: Trust::Trusted,
                revision: Revision::ZERO,
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
            },
        )
        .unwrap();
        state
            .records
            .insert(workspace_record.key(), workspace_record);
        let session_record = Record::typed(
            Collection::Session,
            session.to_string(),
            workspace.clone(),
            Revision::ZERO,
            &Session {
                id: session.clone(),
                workspace: workspace.clone(),
                revision: Revision::ZERO,
                configuration: Revision::ZERO,
                fork_origin: None,
                fork_through: None,
            },
        )
        .unwrap();
        state.records.insert(session_record.key(), session_record);
        let task_record = Record::typed(
            Collection::Task,
            task.to_string(),
            workspace.clone(),
            Revision::ZERO,
            &Task {
                scope: scope.clone(),
                root: task.clone(),
                parent: None,
                fork_origin: None,
                revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                objectives: vec![Objective {
                    text: "Retain evidence".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                }],
                state: TaskState::Completed,
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
                cause: EventId::new(),
                reason: "fixture".into(),
                redaction: None,
            },
        )
        .unwrap();
        state.records.insert(task_record.key(), task_record);
        for index in 0..130u64 {
            let descriptor = ArtifactDescriptor {
                spec: ArtifactSpec {
                    id: ArtifactId::new(),
                    scope: scope.clone(),
                    media_type: "text/plain".into(),
                    schema: "fixture/1".into(),
                    source: "fixture".into(),
                    channel: Channel::Stdout,
                    retention: "history".into(),
                    omissions: vec![],
                },
                state: CaptureState::Complete,
                length: ByteCount::ZERO,
                sha256: vcp_protocol::digest_bytes(b""),
                retained: vec![Range {
                    start: ByteCount::ZERO,
                    end: ByteCount::ZERO,
                }],
            };
            let record = Record::typed(
                Collection::Artifact,
                descriptor.spec.id.to_string(),
                workspace.clone(),
                Revision::ZERO,
                &descriptor,
            )
            .unwrap();
            state.records.insert(record.key(), record);
            state.events.push(EventEnvelope {
                redaction: None,
                version: 1,
                sequence: SessionSeq::new(index + 1),
                watermark: Watermark::new(1),
                event: EventInput {
                    id: EventId::new(),
                    workspace: workspace.clone(),
                    session: session.clone(),
                    task: Some(task.clone()),
                    actor: ActorId::new(),
                    causation: None,
                    kind: EventKind::TaskCreated,
                    timestamp: Timestamp::new(index + 1),
                    correlation: CommandId::new(),
                    data: json!({"index": index}),
                    artifacts: vec![],
                    metadata: None,
                },
            });
        }
        state.watermark = Watermark::new(1);
        state.sequences.insert(session, SessionSeq::new(130));
        (state, access, task)
    }

    #[test]
    fn inspection_bundle_matches_paged_queries_without_mutating_state() {
        let (state, access, task) = fixture();
        let before = serde_json::to_value(&state).unwrap();
        let bundle = collect(&state, &access, &task).unwrap();
        assert_eq!(bundle["task"]["scope"]["task"], task.as_str());
        assert_eq!(
            bundle["source_watermark"],
            serde_json::to_value(state.watermark).unwrap()
        );
        assert_eq!(bundle["views"]["outputs"].as_array().unwrap().len(), 2);
        assert_eq!(bundle["history"].as_array().unwrap().len(), 2);
        assert_eq!(bundle["agents"].as_array().unwrap().len(), 1);
        assert!(bundle["agents"][0]["items"].as_array().unwrap().is_empty());
        for (name, view) in [
            ("costs", View::Costs),
            ("verification", View::Verification),
            ("tools", View::Tools),
            ("routing", View::Routing),
            ("policy", View::Policy),
            ("outputs", View::Outputs),
        ] {
            let mut request = InspectionQuery {
                id: task.to_string(),
                view,
                limit: 128,
                cursor: None,
                range: None,
            };
            for expected in bundle["views"][name].as_array().unwrap() {
                let page = inspection::records(&state, &access, &request).unwrap();
                request.cursor = page.next_cursor.clone();
                assert_eq!(*expected, serde_json::to_value(page).unwrap());
            }
            assert!(request.cursor.is_none());
        }
        let mut request = history_query::Query {
            selector: Selector {
                schema_version: 1,
                tree: Tree::All(vec![
                    Tree::Match(Criterion::Workspace(access.workspace.clone())),
                    Tree::Match(Criterion::Task(task.clone())),
                ]),
            },
            text: None,
            limit: 128,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        for expected in bundle["history"].as_array().unwrap() {
            let page = history_query::query(&state, &access, &request).unwrap();
            request.cursor = page.next_cursor.clone();
            assert_eq!(*expected, serde_json::to_value(page).unwrap());
        }
        assert!(request.cursor.is_none());
        assert_eq!(before, serde_json::to_value(&state).unwrap());
    }

    #[test]
    fn inspection_bundle_enforces_read_task_workspace_and_authority_access() {
        let (state, access, task) = fixture();
        let allowed = || Access {
            workspace: access.workspace.clone(),
            authority: access.authority,
            read: access.read,
            tasks: None,
        };
        for denied in [
            Access {
                read: false,
                ..allowed()
            },
            Access {
                tasks: Some(Default::default()),
                ..allowed()
            },
            Access {
                workspace: WorkspaceId::new(),
                ..allowed()
            },
            Access {
                authority: AuthorityRevision::new(1),
                ..allowed()
            },
        ] {
            assert!(collect(&state, &denied, &task).is_err());
        }
        assert!(collect(&state, &access, &TaskId::new()).is_err());
    }

    #[test]
    fn inspection_bundle_cli_is_explicit_and_requires_a_task() {
        use clap::Parser;
        assert!(matches!(
            crate::args::Cli::try_parse_from(["vcp", "inspect-bundle", "task"])
                .unwrap()
                .command,
            Some(crate::args::Command::InspectBundle { .. })
        ));
        assert!(crate::args::Cli::try_parse_from(["vcp", "inspect-bundle"]).is_err());
    }

    #[test]
    fn bundle_bounds_fail_explicitly_instead_of_truncating() {
        let mut budget = Budget::default();
        for _ in 0..MAX_PAGES {
            budget.page(&Value::Null).unwrap();
        }
        assert!(budget
            .page(&Value::Null)
            .unwrap_err()
            .contains("limit exceeded"));
        let mut budget = Budget {
            pages: 0,
            bytes: MAX_BYTES,
        };
        assert!(budget.page(&json!({"evidence":"retained"})).is_err());
    }
}
