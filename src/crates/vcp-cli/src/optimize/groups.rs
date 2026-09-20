// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_lifecycle::foundation::routing_state::Registry;
const PAGE: usize = 8;

pub fn parse(words: &[&str]) -> Result<Command> {
    let (model, offset) = match words {
        [] => (None, 0),
        ["--offset", value] => (None, position(value)?),
        [model] if !model.starts_with('-') => (Some((*model).to_owned()), 0),
        [model, "--offset", value] if !model.starts_with('-') => {
            (Some((*model).to_owned()), position(value)?)
        }
        _ => return Err("Use /groups [exact-model] [--offset <candidate-number>].".into()),
    };
    if model.as_ref().is_some_and(|model| model.len() > 256) {
        return Err("Exact model identity exceeds 256 bytes.".into());
    }
    Ok(Command::Groups { model, offset })
}
fn position(value: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .ok()
        .filter(|position| *position <= 1024)
        .ok_or_else(|| "Group offset must be an integer from 0 to 1024.".into())
}
fn brief(text: &str) -> String {
    crate::terminal::sanitize(text, 160)
}
fn age(observed: Timestamp, now: Timestamp) -> String {
    if observed > now {
        format!("future observation by {} ms", observed.get() - now.get())
    } else {
        format!("age {} ms", now.get() - observed.get())
    }
}
pub(super) fn render(
    value: &Value,
    model: Option<&str>,
    offset: usize,
    now: Timestamp,
) -> Result<String> {
    if value["registry"].is_null() {
        return Ok(CONFIGURE.to_owned());
    }
    let published: Published<Registry> = decode(&value["registry"])?;
    let catalog = &published.value.catalog;
    catalog.validate().map_err(|error| error.to_string())?;
    let entries: Vec<_> = catalog
        .entries
        .iter()
        .filter(|candidate| model.is_none_or(|model| candidate.identity.model == model))
        .collect();
    let mut output=vec![format!("Catalog revision {} ({}) observed {} UTC epoch ms, {}; {} matching candidates. Capability groups Frontier/High/Medium/Low are separate from cost profiles low/med/high. Source artifact: {}.",published.revision.get(),catalog.id,catalog.observed_at.get(),age(catalog.observed_at,now),entries.len(),published.value.raw)];
    for candidate in entries.iter().skip(offset).take(PAGE) {
        output.push(format!(
            "{} @ {}: declared availability {:?}. {}",
            brief(&candidate.identity.model),
            brief(&candidate.identity.endpoint),
            candidate.availability,
            if candidate.reasons.is_empty() {
                "No recorded source rejection.".to_owned()
            } else {
                format!(
                    "Source reasons: {}{}",
                    candidate
                        .reasons
                        .iter()
                        .take(4)
                        .map(|reason| brief(reason))
                        .collect::<Vec<_>>()
                        .join("; "),
                    if candidate.reasons.len() > 4 {
                        "; further reasons retained in registry"
                    } else {
                        ""
                    }
                )
            }
        ));
        match &candidate.snapshot {
            Some(snapshot)=>output.push(format!("Snapshot {}: {}; {} input / {} output capacity; valid until {}. Limits/prices come from this exact endpoint snapshot.",snapshot.id,
                if snapshot.current(now).is_ok(){"current"}else{"stale/not yet effective"},snapshot.max_input.get(),snapshot.max_output.get(),snapshot.valid_until.get())),
            None=>output.push("Snapshot unavailable: capability/price qualification is incomplete; automatic routing cannot select this candidate.".into()),
        }
        for source in candidate.provenance.iter().take(2) {
            output.push(format!(
                "Source {} [{}], observed {}, {}; effective {}. {}",
                brief(&source.source),
                source.sha256,
                source.observed_at.get(),
                age(source.observed_at, now),
                source
                    .effective_at
                    .map(|time| time.get().to_string())
                    .unwrap_or_else(|| "unknown".into()),
                source
                    .limitations
                    .iter()
                    .take(2)
                    .map(|text| brief(text))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if candidate.compatibility.is_empty() {
            output.push("No retained live compatibility observation.".into());
        }
        for observation in candidate.compatibility.iter().take(4) {
            output.push(format!(
                "Compatibility {}: {:?}/{:?}, {}, expires {}; evidence {}.",
                brief(&observation.id),
                observation.kind,
                observation.state,
                age(observation.observed_at, now),
                observation.valid_until.get(),
                observation
                    .provenance
                    .iter()
                    .take(2)
                    .map(|source| brief(&source.source))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let mut shown = 0usize;
        let total: usize = candidate
            .memberships
            .iter()
            .map(|membership| membership.roles.len())
            .sum();
        for membership in &candidate.memberships {
            for role in &membership.roles {
                if shown >= 16 {
                    break;
                }
                shown += 1;
                output.push(format!("{:?} / {:?} / {}: {:?} evidence {}, {} samples, quality {}/10000, p95 {} ms; {}, expires {}; group version {}.",membership.group,role.role,brief(&role.task_class),role.kind,brief(&role.id),role.samples,role.quality_bps,role.latency_p95_ms,age(role.observed_at,now),role.valid_until.get(),brief(&membership.version)));
            }
            if shown >= 16 {
                break;
            }
        }
        if total == 0 {
            output.push("No role/group qualification; research nominations cannot supply missing live evidence.".into());
        }
        if total > shown || candidate.provenance.len() > 2 || candidate.compatibility.len() > 4 {
            output.push(format!("Showing {shown}/{total} role observations and bounded source details. Further observations remain in the canonical registry; /read {} 0 accesses original endpoint metadata.",published.value.raw));
        }
    }
    let next = offset.saturating_add(PAGE);
    if next < entries.len() {
        output.push(format!(
            "Next candidates: /groups {}--offset {next}",
            model
                .map(|model| format!("{} ", crate::terminal::sanitize(model, 1024)))
                .unwrap_or_default()
        ));
    } else if offset >= entries.len() {
        output.push("No candidates at this offset. Use /groups to restart, or an exact catalog model ID to narrow results.".into());
    }
    output.push("Displayed membership is evidence, not a selection promise: current policy, role, quality, exact identity, costs and budget are rechecked before each decision.".into());
    Ok(output.join(" | "))
}
