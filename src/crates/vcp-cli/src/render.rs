// SPDX-License-Identifier: Apache-2.0
//! Plain text for a person at a terminal. Scripts and JSONL consumers keep the
//! versioned records; every stored or untrusted value is escaped for display.
use crate::{settings, terminal::sanitize};
use serde_json::Value;
use std::path::Path;
use vcp_domain::Timestamp;

const FIELD: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Doctor,
    SetupCheck,
    SetupProfile,
    SetupProvider,
}

pub fn human(view: View, value: &Value, now: Timestamp) -> String {
    match view {
        View::Doctor => doctor(value, now),
        View::SetupCheck => setup_check(value, now),
        View::SetupProfile => setup_profile(value),
        View::SetupProvider => setup_provider(value, now),
    }
}

/// Strings verbatim, other values as compact JSON; both escaped and bounded.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => sanitize(text, FIELD),
        Value::Null => "unknown".into(),
        other => sanitize(&other.to_string(), FIELD),
    }
}

/// "valid for 11h 58m" or "expired 2h 3m ago" for a millisecond timestamp.
pub fn validity(valid_until: &Value, now: Timestamp) -> String {
    let Some(until) = valid_until
        .as_str()
        .and_then(|text| text.parse::<u64>().ok())
    else {
        return "validity unknown".into();
    };
    let span = |milliseconds: u64| {
        let minutes = milliseconds / 60_000;
        if minutes >= 60 {
            format!("{}h {}m", minutes / 60, minutes % 60)
        } else {
            format!("{minutes}m")
        }
    };
    if until > now.get() {
        format!("valid for {}", span(until - now.get()))
    } else {
        format!("expired {} ago", span(now.get() - until))
    }
}

/// Exact micros as dollars, without float rounding.
pub fn usd(micros: &Value) -> String {
    let parsed = match micros {
        Value::String(text) => text.parse::<u64>().ok(),
        Value::Number(number) => number.as_u64(),
        _ => None,
    };
    match parsed {
        Some(micros) => format!("${}.{:06}", micros / 1_000_000, micros % 1_000_000),
        None => "unknown".into(),
    }
}

fn path(value: &Value) -> String {
    value
        .as_str()
        .map(|text| sanitize(&settings::display_path(Path::new(text)), FIELD))
        .unwrap_or_else(|| text(value))
}

fn doctor(value: &Value, now: Timestamp) -> String {
    let mut lines = vec![format!(
        "VCP readiness for workspace {}",
        path(&value["workspace"])
    )];
    for item in value["readiness"].as_array().into_iter().flatten() {
        let status = match item["status"].as_str() {
            Some("ok") => "ok  ",
            Some("warn") => "warn",
            _ => "FAIL",
        };
        let mut detail = text(&item["detail"]);
        if item["check"] == "provider_metadata" && !item["valid_until"].is_null() {
            detail = format!("{detail}; {}", validity(&item["valid_until"], now));
        }
        lines.push(format!("  {status}  {:<18} {detail}", text(&item["label"])));
        if !item["next"].is_null() {
            lines.push(format!("        Next: {}", text(&item["next"])));
        }
    }
    let issues = value["issues"].as_array().map_or(0, Vec::len);
    if issues == 0 {
        lines.push("Path checks: passed.".into());
    } else {
        lines.push(format!("Path checks: {issues} issue(s)."));
        for issue in value["issues"].as_array().into_iter().flatten() {
            let kinds = [&issue["kind"], &issue["left"], &issue["right"]]
                .into_iter()
                .filter(|kind| !kind.is_null())
                .map(text)
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "  - {} ({kinds}): {}",
                text(&issue["category"]),
                text(&issue["next_action"])
            ));
        }
    }
    lines.push(if value["ready"] == true {
        "Ready: yes.".into()
    } else {
        "Ready: no; resolve the FAIL items above.".into()
    });
    lines.join("\n")
}

fn setup_check(value: &Value, now: Timestamp) -> String {
    let tools = match &value["canonical_tools"] {
        Value::Array(names) => names.iter().map(text).collect::<Vec<_>>().join(", "),
        other => text(other),
    };
    let key = if value["credential_present"] == true {
        "OPENROUTER_API_KEY is set in this terminal (value not shown)"
    } else {
        "OPENROUTER_API_KEY is not set in this terminal; set it before `vcp run`"
    };
    [
        format!(
            "Ready: profile {} for workspace {}.",
            path(&value["profile"]),
            path(&value["workspace"])
        ),
        format!("  Model metadata  {}", validity(&value["valid_until"], now)),
        format!("  Tools           {tools}"),
        format!(
            "  Checks {}, processes {}; workspace trust {}",
            text(&value["checks"]),
            text(&value["processes"]),
            if value["trust_granted"] == true {
                "granted"
            } else {
                "not granted"
            }
        ),
        format!("  Provider key    {key}"),
        "No model calls were made.".into(),
    ]
    .join("\n")
}

fn setup_profile(value: &Value) -> String {
    let profile = path(&value["profile"]);
    let workspace = path(&value["workspace"]);
    format!(
        "Created profile {profile} for workspace {workspace} (offline; no model calls).\n  Next: vcp --workspace \"{workspace}\" --config \"{profile}\" setup check\n  Note: {}",
        text(&value["checks"])
    )
}

fn setup_provider(value: &Value, now: Timestamp) -> String {
    let snapshot = path(&value["snapshot"]);
    let catalog = path(&value["catalog"]);
    format!(
        "Model endpoint verified ({}). Actual cost {}; at most {} requests, {} retries.\n  Snapshot  {snapshot}\n  Catalog   {catalog}\n  Metadata  {}\n  Next: vcp setup profile --snapshot \"{snapshot}\" --catalog \"{catalog}\" --output <new profile.json> --trust-workspace --budget-usd <amount> --autonomy ask --affected-path <path>",
        text(&value["status"]),
        usd(&value["actual_cost_micros"]),
        text(&value["max_requests"]),
        text(&value["retries"]),
        validity(&value["valid_until"], now),
    )
}

/// One numbered unfinished task for the discovery chooser.
pub fn candidate(number: usize, row: &Value) -> String {
    let objective = row["objective"]
        .as_str()
        .map(|objective| sanitize(objective, 120))
        .unwrap_or_else(|| "(no objective)".into());
    let mut line = format!("{number}. {objective}  [{}]", text(&row["state"]));
    if let Some(reason) = row["reason"].as_str().filter(|reason| !reason.is_empty()) {
        line.push_str(&format!("\n   {}", sanitize(reason, 240)));
    }
    let waiting = row["pending_approvals"].as_array().map_or(0, Vec::len)
        + row["waiting_for_input"].as_array().map_or(0, Vec::len);
    if waiting > 0 {
        line.push_str(&format!("\n   {waiting} question(s) waiting for an answer"));
    }
    line
}

/// Name the actual placement conflict for the private data folder and the
/// next step; the home folder is the common accidental workspace.
pub fn data_placement(error: String, data: &Path, workspace: &Path) -> String {
    let Ok(Some(conflict)) = settings::local_path_conflict(data, workspace) else {
        return error;
    };
    let data = settings::display_path(&std::path::absolute(data).unwrap_or(data.to_path_buf()));
    let elsewhere = "To keep VCP data elsewhere, pass --data-dir <folder outside projects, Git repositories and OneDrive>.";
    match conflict {
        settings::PlacementConflict::Workspace(root) => {
            let home = std::env::var_os("USERPROFILE")
                .and_then(|home| std::path::PathBuf::from(home).canonicalize().ok())
                .is_some_and(|home| {
                    settings::within(&home, &root) && settings::within(&root, &home)
                });
            let reason = if home {
                format!(
                    "this is your home folder ({}), which cannot be a VCP workspace because VCP's private data folder {data} is inside it",
                    settings::display_path(&root)
                )
            } else {
                format!(
                    "VCP's private data folder {data} is inside the selected workspace {}",
                    settings::display_path(&root)
                )
            };
            format!("{reason}.\n  Next: cd into a project folder and run `vcp setup`, or pass --workspace <project folder>.\n  {elsewhere}")
        }
        conflict => {
            format!("VCP's private data folder {data} is inside {conflict}.\n  {elsewhere}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_759_400_000_000;

    #[test]
    fn validity_and_money_are_exact() {
        let now = Timestamp::new(NOW);
        let later = (NOW + (11 * 60 + 58) * 60_000).to_string();
        assert_eq!(validity(&json!(later), now), "valid for 11h 58m");
        let earlier = (NOW - 5 * 60_000).to_string();
        assert_eq!(validity(&json!(earlier), now), "expired 5m ago");
        assert_eq!(validity(&json!(null), now), "validity unknown");
        assert_eq!(usd(&json!("2232")), "$0.002232");
        assert_eq!(usd(&json!(6_397_576)), "$6.397576");
        assert_eq!(usd(&json!("not money")), "unknown");
    }

    #[test]
    fn doctor_lists_each_check_and_next_step() {
        let value = json!({
            "workspace": r"\\?\D:\code\project",
            "ready": false,
            "readiness": [
                {"check":"installation","label":"Installation","status":"ok","detail":"portable engine 0.2.0","next":null},
                {"check":"profile","label":"Profile","status":"fail","detail":"no profile found","next":"run `vcp setup`"},
                {"check":"provider_metadata","label":"Model metadata","status":"ok","detail":"qwen/qwen3.8-max-0902 @ alibaba","valid_until":(NOW + 90 * 60_000).to_string(),"next":null}
            ],
            "issues": [{"category":"path_overlap","left":"workspace","right":"local_data","next_action":"select separate private, workspace, and vault directories"}]
        });
        let shown = human(View::Doctor, &value, Timestamp::new(NOW));
        assert!(
            shown.starts_with(r"VCP readiness for workspace D:\code\project"),
            "{shown}"
        );
        assert!(shown.contains("  ok    Installation"), "{shown}");
        assert!(shown.contains("  FAIL  Profile"), "{shown}");
        assert!(shown.contains("        Next: run `vcp setup`"), "{shown}");
        assert!(shown.contains("valid for 1h 30m"), "{shown}");
        assert!(
            shown.contains("path_overlap (workspace, local_data)"),
            "{shown}"
        );
        assert!(
            shown.ends_with("Ready: no; resolve the FAIL items above."),
            "{shown}"
        );
    }

    #[test]
    fn stored_text_cannot_inject_terminal_controls() {
        let row = json!({
            "objective": "fix\u{1b}[2J build\u{202e}gnp.exe",
            "state": "paused",
            "reason": "line\nbreak",
            "pending_approvals": [{}],
            "waiting_for_input": []
        });
        let shown = candidate(1, &row);
        assert!(
            !shown.contains('\u{1b}') && !shown.contains('\u{202e}'),
            "{shown}"
        );
        assert!(
            shown.contains(r"\u{1b}") && shown.contains(r"\u{202e}"),
            "{shown}"
        );
        assert!(shown.starts_with("1. fix"), "{shown}");
        assert!(shown.contains(r"line\u{a}break"), "{shown}");
        assert!(
            shown.ends_with("1 question(s) waiting for an answer"),
            "{shown}"
        );
        let check = json!({"profile":"p\u{7}.json","workspace":"w","valid_until":null,
            "canonical_tools":["vcp_read","vcp\u{1b}"],"checks":0,"processes":0,
            "trust_granted":true,"credential_present":false});
        let shown = human(View::SetupCheck, &check, Timestamp::new(NOW));
        assert!(
            !shown.contains('\u{7}') && !shown.contains('\u{1b}'),
            "{shown}"
        );
        assert!(shown.contains("not set in this terminal"), "{shown}");
    }

    #[test]
    fn setup_results_end_with_the_next_command() {
        let provider = json!({"status":"qualified","snapshot":r"C:\data\gen\qualified\snapshot.json",
            "catalog":r"C:\data\gen\endpoints.json","valid_until":(NOW + 12 * 3_600_000).to_string(),
            "actual_cost_micros":"2232","max_requests":2,"retries":0});
        let shown = human(View::SetupProvider, &provider, Timestamp::new(NOW));
        assert!(shown.contains("Actual cost $0.002232"), "{shown}");
        assert!(shown.contains("valid for 12h 0m"), "{shown}");
        assert!(
            shown.contains(r#"--snapshot "C:\data\gen\qualified\snapshot.json""#),
            "{shown}"
        );
        let profile = json!({"status":"created","profile":r"C:\data\p.json","workspace":r"\\?\D:\w","checks":"source integrity only"});
        let shown = human(View::SetupProfile, &profile, Timestamp::new(NOW));
        assert!(
            shown.contains(r#"Next: vcp --workspace "D:\w" --config "C:\data\p.json" setup check"#),
            "{shown}"
        );
    }
}
