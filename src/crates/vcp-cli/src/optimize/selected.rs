// SPDX-License-Identifier: Apache-2.0
//! Explicit field selection for the existing canonical optimizer transaction.
use super::{order, Edit, Preview, Profile, Result};
use std::collections::BTreeSet;
use vcp_domain::Units;
use vcp_models::routing::{Group, ModelEndpoint, Pin};

fn identifier(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
        || value.starts_with("--")
    {
        return Err(
            "Use a nonempty exact identifier of at most 256 bytes without whitespace.".into(),
        );
    }
    Ok(value.to_owned())
}
fn list(value: &str) -> Result<BTreeSet<String>> {
    if value == "none" {
        return Ok(BTreeSet::new());
    }
    let mut values = BTreeSet::new();
    for item in value.split(',') {
        if !values.insert(identifier(item)?) || values.len() > 32 {
            return Err("Use at most 32 distinct identifiers per selected field.".into());
        }
    }
    Ok(values)
}
fn optional_counter(value: &str, minimum: u32, maximum: u32) -> Result<Option<u32>> {
    if value == "inherit" {
        return Ok(None);
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|n| *n >= minimum && *n <= maximum)
        .map(Some)
        .ok_or_else(|| format!("Choose {minimum}..{maximum} or inherit."))
}
pub(super) fn parse(words: &[&str]) -> Result<Vec<Edit>> {
    if words.is_empty() || words.iter().map(|word| word.len()).sum::<usize>() > 8192 {
        return Err("Select at least one policy field; keep the preview within 8192 bytes.".into());
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let flag = words[index];
        let key = if flag == "--unpin" { "--pin" } else { flag };
        if !seen.insert(key) {
            return Err(format!("Duplicate policy field: {key}"));
        }
        index += 1;
        if flag == "--unpin" {
            selected.push(Edit::Pin(None));
            continue;
        }
        let value = words
            .get(index)
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        index += 1;
        let edit = match flag {
            "--retrieval-limits" => Edit::RetrievalLimits(if *value == "inherit" {
                None
            } else {
                let tokens = words
                    .get(index)
                    .ok_or("Retrieval limits require RESULTS TOKENS BYTES or inherit.")?;
                let bytes = words
                    .get(index + 1)
                    .ok_or("Retrieval limits require RESULTS TOKENS BYTES or inherit.")?;
                index += 2;
                let limits = vcp_models::routing::RetrievalLimits {
                    results: value
                        .parse()
                        .map_err(|_| "Invalid retrieval result count.")?,
                    tokens: Units::new(
                        tokens
                            .parse()
                            .map_err(|_| "Invalid retrieval token count.")?,
                    ),
                    bytes: vcp_domain::ByteCount::new(
                        bytes.parse().map_err(|_| "Invalid retrieval byte count.")?,
                    ),
                };
                limits.validate().map_err(|error| error.to_string())?;
                Some(limits)
            }),
            "--input-tokens" => Edit::InputTokens(if *value == "inherit" {
                None
            } else {
                Some(Units::new(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("Input ceiling must be a positive integer or inherit.")?,
                ))
            }),
            "--max-transport-retries" => {
                Edit::EscalationMaxTransportRetries(optional_counter(value, 0, 4)?)
            }
            "--max-quality-switches" => {
                Edit::EscalationMaxQualitySwitches(optional_counter(value, 0, 8)?)
            }
            "--max-total-attempts" => {
                Edit::EscalationMaxTotalAttempts(optional_counter(value, 1, 64)?)
            }
            "--minimum-repeated-failures" => {
                Edit::EscalationMinimumRepeatedFailures(optional_counter(value, 1, 64)?)
            }
            "--reasoning-effort" => Edit::ReasoningEffort(match *value {
                "inherit" => None,
                "minimal" => Some(vcp_models::reasoning::Effort::Minimal),
                "low" => Some(vcp_models::reasoning::Effort::Low),
                "medium" => Some(vcp_models::reasoning::Effort::Medium),
                "high" => Some(vcp_models::reasoning::Effort::High),
                _ => return Err("Choose minimal, low, medium, high or inherit.".into()),
            }),
            "--output-tokens" => Edit::OutputTokens(if *value == "inherit" {
                None
            } else {
                Some(Units::new(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or("Output tokens must be a positive integer or inherit.")?,
                ))
            }),
            "--profile" => {
                let profile = match *value {
                    "low" => Profile::Low,
                    "med" => Profile::Med,
                    "high" => Profile::High,
                    _ => return Err("Choose low, med or high.".into()),
                };
                selected.push(Edit::Profile(profile));
                Edit::Ordering(order(profile))
            }
            "--quality-floor" => Edit::QualityFloorBps(
                value
                    .parse::<u16>()
                    .ok()
                    .filter(|value| *value <= 10_000)
                    .ok_or("Quality floor must be 0..10000 basis points.")?,
            ),
            "--minimum-samples" => Edit::MinimumSamples(
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or("Minimum samples must be a positive integer.")?,
            ),
            "--maximum-evidence-age-ms" => Edit::MaximumEvidenceAgeMs(
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or("Evidence age must be positive milliseconds.")?,
            ),
            "--models" => Edit::AllowedModels(list(value)?),
            "--endpoints" => Edit::AllowedEndpoints(list(value)?),
            "--groups" => Edit::AllowedGroups(
                list(value)?
                    .iter()
                    .map(|value| match value.as_str() {
                        "frontier" => Ok(Group::Frontier),
                        "high" => Ok(Group::High),
                        "medium" => Ok(Group::Medium),
                        "low" => Ok(Group::Low),
                        _ => Err("Choose frontier, high, medium or low capability groups.".into()),
                    })
                    .collect::<Result<_>>()?,
            ),
            "--pin" => {
                let endpoint = words
                    .get(index)
                    .ok_or("Pin requires exact MODEL ENDPOINT identifiers.")?;
                index += 1;
                Edit::Pin(Some(Pin {
                    candidate: ModelEndpoint {
                        model: identifier(value)?,
                        endpoint: identifier(endpoint)?,
                    },
                    fallback_candidates: BTreeSet::new(),
                }))
            }
            _ => return Err(format!("Unsupported selected policy field: {flag}")),
        };
        selected.push(edit);
    }
    Ok(selected)
}

/// Show every selected field, including values clamped by trusted ceilings.
pub(super) fn diff(preview: &Preview) -> Result<String> {
    let before = serde_json::to_value(&preview.prior).map_err(|e| e.to_string())?;
    let requested = serde_json::to_value(&preview.persisted).map_err(|e| e.to_string())?;
    let effective = serde_json::to_value(&preview.effective).map_err(|e| e.to_string())?;
    let mut lines = Vec::new();
    for edit in &preview.selected {
        let field = match edit {
            Edit::RetrievalLimits(_) => "retrieval_limits",
            Edit::InputTokens(_) => "input_tokens",
            Edit::EscalationMaxTransportRetries(_) => "max_transport_retries",
            Edit::EscalationMaxQualitySwitches(_) => "max_quality_switches",
            Edit::EscalationMaxTotalAttempts(_) => "max_total_attempts",
            Edit::EscalationMinimumRepeatedFailures(_) => "minimum_repeated_failures",
            Edit::ReasoningEffort(_) => "reasoning_effort",
            Edit::OutputTokens(_) => "output_tokens",
            Edit::Profile(_) => "profile",
            Edit::Ordering(_) => "ordering",
            Edit::QualityFloorBps(_) => "quality_floor_bps",
            Edit::MinimumSamples(_) => "minimum_samples",
            Edit::MaximumEvidenceAgeMs(_) => "maximum_evidence_age_ms",
            Edit::AllowedModels(_) => "allowed_models",
            Edit::AllowedEndpoints(_) => "allowed_endpoints",
            Edit::AllowedGroups(_) => "allowed_groups",
            Edit::Pin(_) => "pin",
        };
        let escalation = matches!(
            edit,
            Edit::EscalationMaxTransportRetries(_)
                | Edit::EscalationMaxQualitySwitches(_)
                | Edit::EscalationMaxTotalAttempts(_)
                | Edit::EscalationMinimumRepeatedFailures(_)
        );
        let values = if escalation {
            [
                &before["escalation_limits"][field],
                &requested["escalation_limits"][field],
                &effective["escalation_limits"][field],
            ]
        } else {
            [&before[field], &requested[field], &effective[field]]
        };
        lines.push(format!(
            "{field}: {} -> {}; effective {}",
            values[0], values[1], values[2]
        ));
    }
    Ok(format!("Selected fields: {}.", lines.join("; ")))
}
