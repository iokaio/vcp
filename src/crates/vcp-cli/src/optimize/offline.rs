// SPDX-License-Identifier: Apache-2.0
//! Local optimizer commands. No provider configuration or task submission path.
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use vcp_domain::Timestamp;
use vcp_lifecycle::foundation::{
    routing::Request,
    routing_state::{Interview, Question},
};

pub const POLICY_HINT: &str = "Policy preview, apply and rollback require the trusted routing ceilings of an active session; use /optimize preview, /optimize apply or /optimize rollback there. These local commands never submit a model request.";
#[derive(Clone, Debug, Subcommand, Serialize, Deserialize)]
#[command(after_help = POLICY_HINT)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Status,
    Report {
        /// Inclusive Unix milliseconds; omitted means all retained history.
        #[arg(long)]
        from: Option<u64>,
        /// Exclusive Unix milliseconds; defaults to the current time.
        #[arg(long)]
        until: Option<u64>,
    },
    Answer {
        #[arg(value_enum)]
        question: LocalQuestion,
        value: String,
        /// Optional stale-update guard; otherwise use the currently observed revision.
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    Compare {
        baseline: String,
        current: String,
    },
}
#[derive(Clone, Debug, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalQuestion {
    Priority,
    Size,
    Review,
    Restrictions,
}
impl From<&LocalQuestion> for Question {
    fn from(question: &LocalQuestion) -> Self {
        match question {
            LocalQuestion::Priority => Self::Priority,
            LocalQuestion::Size => Self::ExpectedSize,
            LocalQuestion::Review => Self::ReviewPreference,
            LocalQuestion::Restrictions => Self::ModelRestrictions,
        }
    }
}
impl Command {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Report {
                from: Some(from),
                until: Some(until),
            } if from >= until => Err("optimization window must have from < until".into()),
            Self::Answer { value, .. }
                if value.trim().is_empty()
                    || value.len() > 2048
                    || value.chars().any(char::is_control) =>
            {
                Err("optimization answer must be 1..2048 bytes without control characters".into())
            }
            Self::Compare { baseline, current } => {
                vcp_domain::CommandId::parse(baseline).map_err(|e| e.to_string())?;
                vcp_domain::CommandId::parse(current).map_err(|e| e.to_string())?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn request(&self, interview: Option<&Interview>, now: Timestamp) -> Result<Request, String> {
        self.validate()?;
        Ok(match self {
            Self::Status => Request::Status,
            Self::Report { from, until } => {
                let until = until.map(Timestamp::new).unwrap_or(now);
                if from.is_some_and(|from| from >= until.get()) {
                    return Err("optimization window must have from < until".into());
                }
                Request::Report {
                    from: from.map(Timestamp::new),
                    until,
                }
            }
            Self::Compare { baseline, current } => Request::Compare {
                baseline: baseline.clone(),
                current: current.clone(),
            },
            Self::Answer {
                question,
                value,
                expected_revision,
            } => {
                let interview = interview.ok_or("current interview required before answering")?;
                if expected_revision.is_some_and(|expected| expected != interview.revision.get()) {
                    return Err(
                        "optimization interview changed; refresh status before answering".into(),
                    );
                }
                Request::Answer {
                    expected: (!interview.answers.is_empty()).then_some(interview.revision),
                    question: question.into(),
                    value: value.clone(),
                }
            }
        })
    }
}
#[cfg(windows)]
fn now() -> Result<Timestamp, String> {
    Ok(Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    ))
}
fn annotate(mut result: serde_json::Value) -> serde_json::Value {
    result["local_workflow"] =
        serde_json::json!("local analysis; no inference or paid trial submitted");
    result["policy_commands"] = serde_json::json!(POLICY_HINT);
    result
}
/// Store ownership is obtained by the caller. Only current canonical authority
/// is used; profile/provider configuration is neither loaded nor consulted.
pub async fn execute_store(
    store: &mut vcp_store::Store,
    access: &vcp_memory::access::Access,
    command: &Command,
    timestamp: Timestamp,
) -> Result<serde_json::Value, String> {
    let interview = if matches!(command, Command::Answer { .. }) {
        Some(vcp_lifecycle::foundation::routing_state::interview(
            store, access,
        )?)
    } else {
        None
    };
    let request = command.request(interview.as_ref(), timestamp)?;
    Ok(annotate(
        vcp_lifecycle::foundation::routing::execute(store, access, request, None, timestamp)
            .await?,
    ))
}
#[cfg(windows)]
pub fn on_host(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    command: &Command,
) -> Result<serde_json::Value, String> {
    let interview = if matches!(command, Command::Answer { .. }) {
        let status = host.routing_control(Request::Status)?;
        Some(
            serde_json::from_value::<Interview>(status["interview"].clone())
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    Ok(annotate(host.routing_control(
        command.request(interview.as_ref(), now()?)?,
    )?))
}
#[cfg(windows)]
pub async fn execute(
    entry: &crate::settings::WorkspaceEntry,
    workspace: &std::path::Path,
    pipe: &str,
    command: &Command,
) -> Result<serde_json::Value, String> {
    use vcp_store::{contract::Collection, Store};
    command.validate()?;
    match Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[workspace.to_owned()],
    )
    .await
    {
        Ok(mut store) => {
            let result = async {
                let workspace: vcp_domain::workspace::Workspace = store
                    .state()
                    .record(
                        Collection::Workspace,
                        entry.config.workspace.as_str(),
                        &entry.config.workspace,
                    )
                    .and_then(|r| r.decode())
                    .map_err(|e| e.to_string())?;
                let access = vcp_memory::access::Access {
                    workspace: workspace.id,
                    actor: entry.config.actor.clone(),
                    authority: workspace.authority,
                    read: true,
                    write: true,
                    tasks: None,
                };
                execute_store(&mut store, &access, command, now()?).await
            }
            .await;
            store.close().await.map_err(|e| e.to_string())?;
            result
        }
        Err(vcp_store::Error::Conflict("canonical root already has an owner")) => {
            crate::control::request(
                pipe,
                &crate::control::Request::Optimize {
                    workspace: entry.config.workspace.clone(),
                    command: command.clone(),
                },
            )
            .await
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use vcp_domain::Revision;
    #[test]
    fn local_optimizer_is_parsed_without_objective_budget_or_provider() {
        for words in [
            vec!["vcp", "optimize", "status"],
            vec!["vcp", "optimize", "report", "--from", "10", "--until", "20"],
            vec!["vcp", "optimize", "answer", "priority", "spend"],
            vec!["vcp", "optimize", "compare", "baseline", "current"],
        ] {
            let cli = crate::args::Cli::try_parse_from(words).unwrap();
            assert!(matches!(
                cli.command,
                Some(crate::args::Command::Optimize { .. })
            ));
        }
        assert!(crate::args::Cli::try_parse_from(["vcp", "optimize", "apply"]).is_err());
    }
    #[test]
    fn answers_bind_observed_revision_and_reject_unbounded_text() {
        let mut interview = Interview {
            version: 1,
            revision: Revision::ZERO,
            answers: Default::default(),
        };
        let command = Command::Answer {
            question: LocalQuestion::Priority,
            value: "spend".into(),
            expected_revision: None,
        };
        assert!(matches!(
            command.request(Some(&interview), Timestamp::ZERO).unwrap(),
            Request::Answer { expected: None, .. }
        ));
        interview
            .answers
            .insert(Question::Priority, "quality".into());
        assert!(matches!(
            command.request(Some(&interview), Timestamp::ZERO).unwrap(),
            Request::Answer {
                expected: Some(Revision::ZERO),
                ..
            }
        ));
        let stale = Command::Answer {
            question: LocalQuestion::Priority,
            value: "spend".into(),
            expected_revision: Some(1),
        };
        assert!(stale.request(Some(&interview), Timestamp::ZERO).is_err());
        let oversized = Command::Answer {
            question: LocalQuestion::Priority,
            value: "x".repeat(2049),
            expected_revision: None,
        };
        assert!(oversized.validate().is_err());
    }
}
