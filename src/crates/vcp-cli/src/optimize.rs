// SPDX-License-Identifier: Apache-2.0
//! Local terminal interview and explicit policy previews. Every effect is routed
//! through the canonical host; this module never calls a model or writes a store.
use serde::de::DeserializeOwned;
pub mod offline;
mod selected;
use serde_json::Value;
use vcp_domain::{CommandId, Revision, Timestamp};
use vcp_lifecycle::foundation::{
    routing::Request,
    routing_state::{
        ApplyReceipt, Edit, Interview, OptimizationReport, Preview, Published, Question,
    },
};
use vcp_models::routing::{Policy, Preference, Profile};

pub const HELP: &str = "/optimize [status] | /optimize transitions | /optimize observations | /optimize cycles | /optimize forecasts | /optimize answer priority|size|review|restrictions <answer> | /optimize preview low|med|high --quality-floor <0..10000> | /optimize preview [--profile low|med|high] [--quality-floor N] [--output-tokens N|inherit] [--input-tokens N|inherit] [--retrieval-limits RESULTS TOKENS BYTES|inherit] [--max-transport-retries N|inherit] [--max-quality-switches N|inherit] [--max-total-attempts N|inherit] [--minimum-repeated-failures N|inherit] [--reasoning-effort minimal|low|medium|high|inherit] [--minimum-samples N] [--maximum-evidence-age-ms N] [--models ID,...|none] [--endpoints ID,...|none] [--groups frontier,high,medium,low|none] [--pin MODEL ENDPOINT|--unpin] | /optimize apply | /optimize rollback <target-revision> --expected <current-revision> | /optimize compare <baseline-report> <current-report>";
const CONFIGURE: &str = "Automatic routing is not configured. Add a validated routing catalog, explicit policy and cost estimates to the existing VCP configuration, then reopen this session. Reporting and preference answers remain available.";
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Transitions,
    Observations,
    Cycles,
    Forecasts,
    Compare {
        baseline: String,
        current: String,
    },
    Groups {
        model: Option<String>,
        offset: usize,
    },
    Report,
    Status,
    Answer {
        question: Question,
        value: String,
    },
    Preview {
        profile: Profile,
        quality_floor_bps: u16,
    },
    PreviewSelected {
        selected: Vec<Edit>,
    },
    Apply,
    Rollback {
        target: Revision,
        expected: Revision,
    },
}
mod groups;
pub use groups::parse as parse_groups;
pub fn parse(words: &[&str]) -> Result<Command> {
    match words {
        ["transitions"] => Ok(Command::Transitions),
        ["observations"] => Ok(Command::Observations),
        ["cycles"] => Ok(Command::Cycles),
        ["forecasts"] => Ok(Command::Forecasts),
        [] | ["report"] => Ok(Command::Report),
        ["status"] => Ok(Command::Status),
        ["compare", baseline, current] if baseline.len() <= 128 && current.len() <= 128 => {
            Ok(Command::Compare {
                baseline: baseline.to_string(),
                current: current.to_string(),
            })
        }
        ["answer", question, answer @ ..] if !answer.is_empty() => {
            let question = match *question {
                "priority" => Question::Priority,
                "size" => Question::ExpectedSize,
                "review" => Question::ReviewPreference,
                "restrictions" => Question::ModelRestrictions,
                _ => return Err("Choose priority, size, review or restrictions.".into()),
            };
            let value = answer.join(" ");
            if value.len() > 2048 || value.chars().any(char::is_control) {
                return Err(
                    "Keep an optimization answer within 2048 bytes and omit control characters."
                        .into(),
                );
            }
            Ok(Command::Answer { question, value })
        }
        ["preview", profile, "--quality-floor", floor] if !profile.starts_with("--") => {
            let profile = match *profile {
                "low" => Profile::Low,
                "med" => Profile::Med,
                "high" => Profile::High,
                _ => return Err("Choose low, med or high.".into()),
            };
            let quality_floor_bps = floor.parse::<u16>().ok().filter(|floor| *floor <= 10_000)
                .ok_or("Quality floor must be an explicit integer from 0 to 10000 basis points (10000 = 100%).")?;
            Ok(Command::Preview {
                profile,
                quality_floor_bps,
            })
        }
        ["apply"] => Ok(Command::Apply),
        ["preview", flags @ ..] => Ok(Command::PreviewSelected {
            selected: selected::parse(flags)?,
        }),
        ["rollback", target, "--expected", expected] => {
            let revision = |text: &str| {
                text.parse::<u64>()
                    .map(Revision::new)
                    .map_err(|_| "Policy revision must be an unsigned integer.".to_owned())
            };
            Ok(Command::Rollback {
                target: revision(target)?,
                expected: revision(expected)?,
            })
        }
        _ => Err(format!(
            "Optimization command requires explicit arguments. {HELP}"
        )),
    }
}
fn decode<T: DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid optimization response: {error}"))
}
fn call(service: &mut impl FnMut(Request) -> Result<Value>, request: Request) -> Result<Value> {
    let value = service(request)?;
    Ok(value.get("result").cloned().unwrap_or(value))
}
fn label(profile: Profile) -> &'static str {
    match profile {
        Profile::Low => "low",
        Profile::Med => "med",
        Profile::High => "high",
    }
}
fn order(profile: Profile) -> Vec<Preference> {
    match profile {
        Profile::Low => vec![
            Preference::TotalCost,
            Preference::Latency,
            Preference::Quality,
            Preference::Capability,
        ],
        Profile::Med => vec![
            Preference::Quality,
            Preference::TotalCost,
            Preference::Latency,
            Preference::Capability,
        ],
        Profile::High => vec![
            Preference::Quality,
            Preference::Capability,
            Preference::Latency,
            Preference::TotalCost,
        ],
    }
}
fn order_text(order: &[Preference]) -> String {
    order
        .iter()
        .map(|value| match value {
            Preference::TotalCost => "total task cost",
            Preference::Latency => "latency",
            Preference::Quality => "measured quality",
            Preference::Capability => "capability group",
        })
        .collect::<Vec<_>>()
        .join(" > ")
}
fn policy_text(policy: &Policy) -> String {
    format!(
        "{}; quality floor {}/10000; minimum {} samples; evidence age at most {} ms; output tokens {}; order {}",
        label(policy.profile),
        policy.quality_floor_bps,
        policy.minimum_samples,
        policy.maximum_evidence_age_ms,
        policy.output_tokens.map_or_else(|| "inherited".to_owned(), |value| value.get().to_string()),
        order_text(&policy.ordering)
    )
}
fn money_text(amounts: &std::collections::BTreeMap<String, u64>) -> String {
    if amounts.is_empty() {
        return "none reported".into();
    }
    amounts
        .iter()
        .map(|(currency, micros)| {
            format!(
                "{currency} {}.{:06}",
                micros / 1_000_000,
                micros % 1_000_000
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
fn question_text(question: Option<Question>, report: Option<&OptimizationReport>) -> String {
    let (name, prompt) = match question {
        Some(Question::Priority) => ("priority", "What matters most for this project: lower total cost, faster results or stronger completion quality?"),
        Some(Question::ExpectedSize) => ("size", "What task sizes and complexity do you expect: small edits, multi-file changes or larger investigations?"),
        Some(Question::ReviewPreference) => ("review", "When do you want independent review and verification before accepting work?"),
        Some(Question::ModelRestrictions) => ("restrictions", "Which models or providers must be included or excluded? Say none if you have no additional preference."),
        None => return "No additional questions are needed for this report. Choose /optimize preview low|med|high --quality-floor <0..10000> to review specific changes; no quality threshold is inferred from these answers.".into(),
    };
    let context = report
        .map(|report| {
            if report.counts.uncertain_attempts > 0 && name == "priority" {
                format!(
                    "{} attempts have uncertain cost. ",
                    report.counts.uncertain_attempts
                )
            } else if name == "review" && report.counts.failed > 0 {
                format!(
                    "{} of {} retained tasks failed. ",
                    report.counts.failed, report.counts.tasks
                )
            } else {
                String::new()
            }
        })
        .unwrap_or_default();
    format!("{context}{prompt} Reply with /optimize answer {name} <answer>.")
}
fn next_question(value: &Value, report: Option<&OptimizationReport>) -> Result<Option<Question>> {
    if let Some(report) = report {
        let interview: Interview = decode(&value["interview"])?;
        Ok(vcp_lifecycle::foundation::routing_state::next_question_for_report(&interview, report))
    } else {
        decode(value.get("next_question").unwrap_or(&Value::Null))
    }
}

#[derive(Default)]
pub struct Session {
    report: Option<OptimizationReport>,
    pending: Option<(CommandId, Preview)>,
}
impl Session {
    /// Invalidate a previous selection before parsing a replacement, including
    /// malformed replacements that never reach `execute`.
    pub fn prepare_input(&mut self, line: &str) {
        let mut words = line.split_whitespace();
        if words.next() == Some("/optimize") && words.next() == Some("preview") {
            self.pending = None;
        }
    }
    /// The closure is the only effects boundary, allowing the terminal workflow
    /// to be checked without launching an engine or making provider calls.
    pub fn execute(
        &mut self,
        command: Command,
        now: Timestamp,
        mut service: impl FnMut(Request) -> Result<Value>,
    ) -> Result<String> {
        match command {
            Command::Forecasts => {
                let value = call(
                    &mut service,
                    Request::Forecasts {
                        from: None,
                        until: now,
                    },
                )?;
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
            }
            Command::Cycles => {
                let value = call(
                    &mut service,
                    Request::Cycles {
                        from: None,
                        until: now,
                    },
                )?;
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
            }
            Command::Observations => {
                let value = call(
                    &mut service,
                    Request::Observations {
                        from: None,
                        until: now,
                    },
                )?;
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
            }
            Command::Transitions => {
                let value = call(
                    &mut service,
                    Request::Transitions {
                        from: None,
                        until: now,
                    },
                )?;
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
            }
            Command::Compare { baseline, current } => {
                let value = call(&mut service, Request::Compare { baseline, current })?;
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
            }
            Command::Groups { model, offset } => {
                let value = call(&mut service, Request::Status)?;
                groups::render(&value, model.as_deref(), offset, now)
            }
            Command::Report => {
                let status = call(&mut service, Request::Status)?;
                let value = call(
                    &mut service,
                    Request::Report {
                        from: None,
                        until: now,
                    },
                )?;
                let report: OptimizationReport = decode(&value["report"])?;
                let count = &report.counts;
                let mut message = format!("Optimization report {}: {} tasks; {} completed, {} failed, {} cancelled, {} unfinished; {} attempts, {} retries, {} support attempts. Known spend: {}; uncertain attempts: {}; reserved liability: {}. {}",
                    report.id, count.tasks, count.completed, count.failed, count.cancelled, count.unfinished,
                    count.attempts, count.retries, count.supporting_attempts, money_text(&count.known_spend_micros),
                    count.uncertain_attempts, money_text(&count.reserved_liability_micros), question_text(next_question(&value, Some(&report))?, Some(&report)));
                if !report.uncertainty.is_empty() {
                    message.push_str(&format!(" Coverage: {}", report.uncertainty.join("; ")));
                }
                if let Some(forecast) = value.get("forecast").filter(|value| !value.is_null()) {
                    message.push_str("\nSaved action forecasts (unqualified estimates; observed totals above remain unchanged):\n");
                    message.push_str(
                        &serde_json::to_string_pretty(forecast)
                            .map_err(|error| error.to_string())?,
                    );
                }
                if let Some(compaction) = value.get("compaction").filter(|value| !value.is_null()) {
                    message.push_str(
                        "\nSaved compaction diagnostics (associations, not causal effects):\n",
                    );
                    message.push_str(
                        &serde_json::to_string_pretty(compaction)
                            .map_err(|error| error.to_string())?,
                    );
                }
                if status["policy"].is_null() {
                    message.push_str(&format!(" {CONFIGURE}"));
                }
                self.report = Some(report);
                self.pending = None;
                Ok(message)
            }
            Command::Status => {
                let value = call(&mut service, Request::Status)?;
                let mut message = if value["policy"].is_null() {
                    CONFIGURE.to_owned()
                } else {
                    let published: Published<Policy> = decode(&value["policy"])?;
                    format!(
                        "Routing policy revision {}: {}.",
                        published.revision.get(),
                        policy_text(&published.value)
                    )
                };
                let interview: Interview = decode(&value["interview"])?;
                message.push_str(&format!(
                    " {} saved project answers. {}",
                    interview.answers.len(),
                    question_text(
                        next_question(&value, self.report.as_ref())?,
                        self.report.as_ref()
                    )
                ));
                if let Some((_, preview)) = &self.pending {
                    message.push_str(&format!(" Pending preview against revision {}. /optimize apply applies only that preview.", preview.base.get()));
                }
                Ok(message)
            }
            Command::Answer { question, value } => {
                let status = call(&mut service, Request::Status)?;
                let interview: Interview = decode(&status["interview"])?;
                let expected = (!interview.answers.is_empty()).then_some(interview.revision);
                let restrictions = question == Question::ModelRestrictions;
                let value = call(
                    &mut service,
                    Request::Answer {
                        expected,
                        question,
                        value,
                    },
                )?;
                self.pending = None;
                let mut message = format!(
                    "Project preference saved. {}",
                    question_text(
                        next_question(&value, self.report.as_ref())?,
                        self.report.as_ref()
                    )
                );
                if restrictions {
                    message.push_str(" This answer records a preference. Use /optimize preview --models ID,... --endpoints ID,... to select restrictions within trusted configuration, then /optimize apply.");
                }
                Ok(message)
            }
            Command::Preview {
                profile,
                quality_floor_bps,
            } => self.preview_selected(
                vec![
                    Edit::Profile(profile),
                    Edit::Ordering(order(profile)),
                    Edit::QualityFloorBps(quality_floor_bps),
                ],
                &mut service,
            ),
            Command::PreviewSelected { selected } => self.preview_selected(selected, &mut service),
            Command::Apply => {
                let (command, preview) = self.pending.as_ref().ok_or("No pending preview. Run /optimize and then /optimize preview with the fields you want to change.")?;
                let value = call(
                    &mut service,
                    Request::Apply {
                        command: command.clone(),
                        preview: Box::new(preview.clone()),
                    },
                )?;
                let receipt: ApplyReceipt = decode(&value)?;
                self.pending = None;
                Ok(format!("Applied policy revision {}. Effective: {}. Active requests retain their captured policy; new work revalidates before admission.", receipt.published.revision.get(), policy_text(&receipt.effective)))
            }
            Command::Rollback { target, expected } => {
                let value = call(
                    &mut service,
                    Request::Rollback {
                        command: CommandId::new(),
                        expected,
                        target,
                    },
                )?;
                let receipt: ApplyReceipt = decode(&value)?;
                self.pending = None;
                Ok(format!(
                    "Restored policy values from revision {} as new revision {}. Effective: {}.",
                    target.get(),
                    receipt.published.revision.get(),
                    policy_text(&receipt.effective)
                ))
            }
        }
    }
    fn preview_selected(
        &mut self,
        selected: Vec<Edit>,
        service: &mut impl FnMut(Request) -> Result<Value>,
    ) -> Result<String> {
        // Even a failed replacement invalidates the previous pending selection.
        self.pending = None;
        let report = self
            .report
            .as_ref()
            .ok_or("Run /optimize first to capture a current report.")?;
        let value = call(
            service,
            Request::Preview {
                report: report.id.clone(),
                selected,
            },
        )?;
        let preview: Preview = decode(&value)?;
        let message = format!("Preview against revision {}. Before: {}. Selected: {}. Effective under trusted limits: {}. {} {} Uncertainty: {}. Use /optimize apply to apply these selected changes, or create another preview.",
                    preview.base.get(), policy_text(&preview.prior), policy_text(&preview.persisted),
                    policy_text(&preview.effective), selected::diff(&preview)?, preview.reason, preview.uncertainty.join("; "));
        self.pending = Some((CommandId::new(), preview));
        Ok(message)
    }
}

#[cfg(test)]
mod tests;
