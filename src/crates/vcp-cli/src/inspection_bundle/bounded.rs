// SPDX-License-Identifier: Apache-2.0
//! Live bounded projections of one pinned canonical cut.
use super::*;

pub async fn collect_store(
    reader: &impl vcp_store::CanonicalHistory,
    access: &Access,
    task: &TaskId,
) -> Result<Value, String> {
    let collection_started = std::time::Instant::now();
    let state = reader.current();
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
        let page = crate::agents_view::page_store(reader, &task_record.scope, now, offset).await?;
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
            let page = inspection::records_store(reader, access, &request)
                .await
                .map_err(|e| e.to_string())?;
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
        let page = history_query::query_store(reader, access, &request)
            .await
            .map_err(|e| e.to_string())?;
        request.cursor = page.next_cursor.clone();
        let value = serde_json::to_value(page).map_err(|e| e.to_string())?;
        budget.page(&value)?;
        history.push(value);
        if request.cursor.is_none() {
            break;
        }
    }
    let diagnostic_index = diagnostics::index(&history)?;
    let retained_diagnostics = lifecycle::retained_store(
        reader,
        &history,
        &task_record.scope,
        MAX_BYTES.saturating_sub(budget.bytes),
    )
    .await?;
    let effective_constraints =
        constraints::project_store(reader, access, &task_record, &workspace).await?;
    let value = json!({"schema_version":1,"kind":"inspection_bundle","source_watermark":state.watermark,
        "task":task_record,"views":views,"history":history,"agents":agents,
        "diagnostics": diagnostic_index,
        "retained_lifecycle_diagnostics": retained_diagnostics,
        "effective_constraints": effective_constraints,
        "collection": {"schema_version":1,"elapsed_micros":collection_started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            "timing_source":"monotonic_instant","pages":budget.pages,"projected_bytes":budget.bytes,
            "observation_scope":"this_inspection_only","analysis_status":"required"}});
    if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err(
            "inspection bundle byte limit exceeded; use paged inspect/history commands".into(),
        );
    }
    if reader.current().watermark != state.watermark {
        return Err("inspection bundle source changed during read".into());
    }
    Ok(value)
}
