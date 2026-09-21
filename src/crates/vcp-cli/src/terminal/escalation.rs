// SPDX-License-Identifier: Apache-2.0
//! Explicit current-owner declarations; task revisions come from the owner snapshot.
use vcp_domain::{task::Task, ArtifactId, CommandId};
use vcp_lifecycle::foundation::{
    routing::Request,
    routing_state::declarations::{Input, Kind},
};

pub const HELP: &str = "/escalate status | /escalate complexity --evidence ARTIFACT,... | /escalate capability NAME --evidence ARTIFACT,...";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Status,
    Declare {
        declaration: Kind,
        evidence: Vec<ArtifactId>,
    },
}
pub fn parse(words: &[&str]) -> Result<Command, String> {
    let (declaration, text) = match words {
        ["status"] => return Ok(Command::Status),
        ["complexity", "--evidence", evidence] => (Kind::DeclaredComplexity, *evidence),
        ["capability", capability, "--evidence", evidence]
            if !capability.is_empty()
                && capability.len() <= 128
                && capability
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') =>
        {
            (
                Kind::UnsupportedCapability {
                    capability: (*capability).into(),
                },
                *evidence,
            )
        }
        _ => return Err(HELP.into()),
    };
    let evidence = text
        .split(',')
        .map(ArtifactId::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if evidence.is_empty()
        || evidence.len() > 63
        || evidence
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != evidence.len()
    {
        return Err("Choose 1..63 distinct retained artifact IDs from this task.".into());
    }
    Ok(Command::Declare {
        declaration,
        evidence,
    })
}
pub fn execute(
    command: Command,
    task: &Task,
    mut service: impl FnMut(Request) -> Result<serde_json::Value, String>,
) -> Result<String, String> {
    let declaring = matches!(command, Command::Declare { .. });
    let request = match command {
        Command::Status => Request::EscalationDeclarations {
            task: task.scope.task.clone(),
        },
        Command::Declare {
            declaration,
            evidence,
        } => Request::DeclareEscalation {
            declaration: Input {
                command: CommandId::new(),
                task: task.scope.task.clone(),
                expected_revision: task.revision,
                steering: task.steering,
                declaration,
                evidence,
            },
        },
    };
    let value = service(request)?;
    Ok(format!(
        "{}\n{}",
        if declaring {
            "Declaration recorded for the next scheduling boundary. Normal eligibility, strict pins, budget and handoff checks still apply. This does not resume paused work."
        } else {
            "A consumed declaration records a scheduling attempt, not a successful handoff; inspect its admission evidence before declaring again."
        },
        serde_json::to_string(&value).map_err(|e| e.to_string())?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declarations_require_explicit_bounded_evidence_and_capability() {
        assert_eq!(parse(&["status"]), Ok(Command::Status));
        assert!(matches!(
            parse(&["complexity", "--evidence", "source-a,source-b"]),
            Ok(Command::Declare {
                declaration: Kind::DeclaredComplexity,
                ..
            })
        ));
        assert!(matches!(
            parse(&[
                "capability",
                "responses_text_tools",
                "--evidence",
                "source-a"
            ]),
            Ok(Command::Declare {
                declaration: Kind::UnsupportedCapability { .. },
                ..
            })
        ));
        for words in [
            vec!["complexity"],
            vec!["complexity", "--evidence", "a,a"],
            vec!["complexity", "--evidence", "a,"],
            vec!["capability", "untrusted key", "--evidence", "a"],
            vec!["status", "resume"],
        ] {
            assert!(parse(&words).is_err(), "{words:?}");
        }
    }
}
