// SPDX-License-Identifier: Apache-2.0
//! Governed, bounded report projection. Never reads private report rows directly.
use super::*;
use serde::{Deserialize, Serialize};
use vcp_protocol::{canonical_bytes, digest_bytes, routing_inspection::Text};

#[cfg(test)]
mod tests;

fn unavailable() -> RpcError {
    failure(Code::StoreUnavailable)
}
fn text(value: &str) -> Text {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Text {
        text: value[..end].to_owned(),
        truncated: end < value.len(),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    digest: String,
    after: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cohort {
    provider: String,
    model: String,
    catalog: String,
    routing_policy: String,
    authority_policy: vcp_domain::PolicyRevision,
    task_class: String,
    size: String,
    role: String,
}
fn money(values: &BTreeMap<String, u64>) -> RpcResult<Vec<wire::Money>> {
    if values.len() > 32 {
        return Err(unavailable());
    }
    values
        .iter()
        .map(|(currency, value)| {
            if currency.is_empty()
                || currency.len() > 16
                || !currency.bytes().all(|c| c.is_ascii_uppercase())
            {
                return Err(unavailable());
            }
            Ok(wire::Money {
                currency: currency.clone(),
                micros: (*value).into(),
            })
        })
        .collect()
}
fn summary(report: &routing_state::OptimizationReport) -> RpcResult<wire::ReportRow> {
    let c = &report.counts;
    let o = &report.observed;
    let samples = &o.submission_to_final_usage_ms;
    if samples.windows(2).any(|w| w[0] > w[1]) {
        return Err(unavailable());
    }
    // Nearest-rank quantile; no interpolation or zero for absent observations.
    let quantile = |percent: usize| {
        samples
            .get((samples.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
            .map(Into::into)
    };
    Ok(wire::ReportRow::Summary {
        counts: wire::Counts {
            tasks: c.tasks.into(),
            completed: c.completed.into(),
            failed: c.failed.into(),
            cancelled: c.cancelled.into(),
            unfinished: c.unfinished.into(),
            abandoned: c.abandoned.map(Into::into),
            attempts: c.attempts.into(),
            retries: c.retries.into(),
            child_tasks: c.child_tasks.into(),
            supporting_attempts: c.supporting_attempts.into(),
            known_spend: money(&c.known_spend_micros)?,
            uncertain_attempts: c.uncertain_attempts.into(),
            reserved_liability: money(&c.reserved_liability_micros)?,
            pruned_tasks: c.pruned_tasks.into(),
        },
        observed: wire::Observed {
            pruned_events: o.pruned_events.into(),
            pruned_attempts: o.pruned_attempts.into(),
            objective_change_events: o.objective_change_events.into(),
            verification_checks_passed: o.verification_checks_passed.into(),
            verification_checks_failed: o.verification_checks_failed.into(),
            verification_checks_not_run: o.verification_checks_not_run.into(),
            pruned_verifications: o.pruned_verifications.into(),
            latency_samples: (samples.len() as u64).into(),
            submission_to_final_usage_p50_ms: quantile(50),
            submission_to_final_usage_p95_ms: quantile(95),
            attempts_without_complete_latency: o.attempts_without_complete_latency.into(),
        },
    })
}
pub(super) fn read(
    store: &Store,
    access: &Access,
    request: &wire::ReportRead,
    global: bool,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<wire::ReportPage> {
    request.validate().map_err(|_| RpcError::invalid_params())?;
    check()?;
    if request.scope.workspace.as_str() != access.workspace.as_str()
        || request.scope.session.as_str() != access.session.as_str()
    {
        return Err(failure(Code::PolicyDenied));
    }
    let workspace = workspace(store, access)?;
    let governed = memory_access(store, access, global, false, check)?;
    let capture =
        CommandId::parse(request.report.as_str()).map_err(|_| RpcError::invalid_params())?;
    let saved = service::read_report(store, &governed, &access.session, &capture, &|| {
        check().map_err(|_| service::Error::Cancelled)
    })
    .map_err(|error| match error {
        service::Error::Access => failure(Code::PolicyDenied),
        service::Error::Invalid => RpcError::invalid_params(),
        _ => unavailable(),
    })?;
    check()?;
    if !global
        && (saved.coverage == service::Coverage::Workspace || saved.report.source_tasks.is_none())
    {
        return Err(failure(Code::PolicyDenied));
    }
    // Even legacy taskless evidence must belong to the authenticated session.
    if !global {
        let mut events = BTreeMap::new();
        for event in &store.state().events {
            check()?;
            events.insert(&event.event.id, &event.event);
        }
        for event in &saved.report.evidence {
            check()?;
            let event = events.get(event).ok_or_else(unavailable)?;
            if event.session != access.session {
                return Err(failure(Code::PolicyDenied));
            }
        }
    }
    let report = &saved.report;
    let mut query = request.clone();
    query.cursor = None;
    let digest = digest_bytes(
        &canonical_bytes(&(
            &query,
            &access.actor,
            &governed.tasks,
            workspace.authority,
            workspace.deletion,
            workspace.binding.revision,
            &saved.command,
            report,
        ))
        .map_err(|_| unavailable())?,
    );
    let start = match &request.cursor {
        None => 0,
        Some(value) => {
            let cursor: Cursor =
                serde_json::from_str(value).map_err(|_| failure(Code::CursorGap))?;
            if cursor.version != 1 || cursor.digest != digest {
                return Err(failure(Code::CursorGap));
            }
            cursor.after
        }
    };
    let total = match request.section {
        wire::ReportSection::Summary => 1,
        wire::ReportSection::Cohorts => report.cohorts.len(),
        wire::ReportSection::Uncertainty => report.uncertainty.len(),
        wire::ReportSection::Sources => report.tasks.len() + report.evidence.len(),
        wire::ReportSection::Forecast => usize::from(report.forecast.is_some()),
    };
    if start > total || (request.cursor.is_some() && start == total) {
        return Err(failure(Code::CursorGap));
    }
    let mut page = wire::ReportPage {
        scope: request.scope.clone(),
        report: request.report.clone(),
        coverage: match saved.coverage {
            service::Coverage::Session => wire::Coverage::Session,
            service::Coverage::Workspace => wire::Coverage::Workspace,
        },
        window: wire::Window {
            from: report.window.from.map(|v| v.get().into()),
            until: report.window.until.get().into(),
        },
        cutoff: report.cutoff.get().into(),
        watermark: store.state().watermark.get().into(),
        authority_revision: workspace.authority.get().into(),
        deletion_revision: workspace.deletion.get().into(),
        binding_revision: workspace.binding.revision.get().into(),
        cohort_denominator: text(&report.cohort_denominator),
        section: request.section,
        rows: Vec::new(),
        next_cursor: None,
        complete: false,
    };
    let mut next = start;
    while next < total && page.rows.len() < request.limit as usize {
        check()?;
        let row = match request.section {
            wire::ReportSection::Summary => summary(report)?,
            wire::ReportSection::Cohorts => {
                let (key, count) = report.cohorts.iter().nth(next).ok_or_else(unavailable)?;
                let c: Cohort = serde_json::from_str(key).map_err(|_| unavailable())?;
                wire::ReportRow::Cohort {
                    value: wire::Cohort {
                        provider: text(&c.provider),
                        model: text(&c.model),
                        catalog: text(&c.catalog),
                        routing_policy: text(&c.routing_policy),
                        authority_policy: c.authority_policy.get().into(),
                        task_class: text(&c.task_class),
                        size: text(&c.size),
                        role: text(&c.role),
                        count: (*count).into(),
                    },
                }
            }
            wire::ReportSection::Uncertainty => wire::ReportRow::Uncertainty {
                text: text(&report.uncertainty[next]),
            },
            wire::ReportSection::Sources => {
                if next < report.tasks.len() {
                    wire::ReportRow::SourceTask {
                        task: id(report
                            .tasks
                            .iter()
                            .nth(next)
                            .ok_or_else(unavailable)?
                            .as_str())?,
                    }
                } else {
                    wire::ReportRow::SourceEvent {
                        event: id(report.evidence[next - report.tasks.len()].as_str())?,
                    }
                }
            }
            wire::ReportSection::Forecast => {
                let pin = report.forecast.as_ref().ok_or_else(unavailable)?;
                wire::ReportRow::Forecast {
                    artifact: id(pin.artifact.as_str())?,
                    sha256: pin.digest.clone(),
                    source_manifest: id(&pin.source_manifest)?,
                    source_task_count: (pin.source_tasks.len() as u64).into(),
                }
            }
        };
        page.rows.push(row);
        // Reserve ample space for the cursor before admitting this row.
        if serde_json::to_vec(&page).map_err(|_| unavailable())?.len() + 512 > wire::MAX_PAGE_BYTES
        {
            page.rows.pop();
            if page.rows.is_empty() {
                return Err(unavailable());
            }
            break;
        }
        next += 1;
    }
    page.complete = next == total;
    if !page.complete {
        page.next_cursor = Some(
            serde_json::to_string(&Cursor {
                version: 1,
                digest,
                after: next,
            })
            .map_err(|_| unavailable())?,
        );
    }
    if serde_json::to_vec(&page).map_err(|_| unavailable())?.len() > wire::MAX_PAGE_BYTES {
        return Err(unavailable());
    }
    check()?;
    Ok(page)
}
