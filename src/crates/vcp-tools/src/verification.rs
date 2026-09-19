// SPDX-License-Identifier: Apache-2.0
//! Check discovery is data, never permission to execute or proof of success.
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;
use vcp_domain::verification::CheckOutcome;
use vcp_repository::{observation::Observation, FileVersion};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runner {
    Node,
    Cargo,
}
/// Explicit owner acceptance, not model-supplied assertions about coverage.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub manifest: String,
    pub runner: Runner,
    pub profile: String,
    pub expected_tests: Vec<String>,
    pub rationale: String,
}
impl Requirement {
    pub fn validate(&self) -> Result<()> {
        if vcp_repository::path::relative(Path::new(&self.manifest))? != self.manifest {
            return Err(Error::Invalid("normalized manifest path required"));
        }
        if self.profile.is_empty()
            || self.profile.len() > 64
            || self.expected_tests.is_empty()
            || self.expected_tests.len() > 256
            || self
                .expected_tests
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 1024 || s.contains(['\r', '\n', '\0']))
            || self.expected_tests.iter().collect::<BTreeSet<_>>().len()
                != self.expected_tests.len()
            || self.rationale.trim().is_empty()
            || self.rationale.len() > 4096
        {
            return Err(Error::Invalid("bounded verification acceptance required"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub specification: String,
    pub origin: Option<FileVersion>,
    pub directory: String,
    pub runner: Runner,
    pub request: process::Request,
    pub expected_tests: Vec<String>,
    pub rationale: String,
    pub not_run: Option<String>,
}
/// Discover only qualified command forms. Shell syntax and pre/post hooks are
/// not silently discarded. Unsupported configurations remain visible as not run.
pub fn discover(observation: &Observation, requirements: &[Requirement]) -> Result<Vec<Plan>> {
    if requirements.len() > 32 {
        return Err(Error::Invalid("verification plan count"));
    }
    let mut seen = BTreeSet::new();
    requirements.iter().map(|requirement| {
        requirement.validate()?;
        if !seen.insert(requirement.manifest.to_lowercase()) {
            return Err(Error::Invalid("duplicate verification manifest"));
        }
        let directory = requirement.manifest.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        let source = observation.sources.iter().find(|s| s.version.path == requirement.manifest);
        let mut plan = Plan {
            specification: format!("{}#test", requirement.manifest),
            origin: source.map(|s| s.version.clone()),
            directory: directory.clone(), runner: requirement.runner,
            request: process::Request { profile: requirement.profile.clone(), arguments: vec![], directory,
                timeout_ms: 120_000, output_bytes: 1024 * 1024, input: None },
            expected_tests: requirement.expected_tests.clone(), rationale: requirement.rationale.clone(), not_run: None,
        };
        let discovered: std::result::Result<Vec<String>, String> = (|| {
            if !observation.manifest.bounded_scan_complete { return Err("workspace scan is incomplete".into()); }
            let source = source.ok_or("required project manifest is unavailable or excluded")?;
            match requirement.runner {
                Runner::Node => {
                    if requirement.manifest.rsplit('/').next() != Some("package.json") { return Err("Node check requires package.json".into()); }
                    let value: Value = serde_json::from_slice(&source.bytes).map_err(|_| "invalid package.json")?;
                    let scripts = value.get("scripts").and_then(Value::as_object).ok_or("package.json has no scripts")?;
                    if scripts.contains_key("pretest") || scripts.contains_key("posttest") { return Err("npm pretest/posttest hooks require a separate qualified plan".into()); }
                    let script = scripts.get("test").and_then(Value::as_str).ok_or("package.json has no test script")?;
                    let words: Vec<_> = script.split_ascii_whitespace().collect();
                    if words.len() < 3 || words[..2] != ["node", "--test"] || words.len() > 66 {
                        return Err("supported test script is node --test followed by explicit relative test files".into());
                    }
                    let mut targets = BTreeSet::new();
                    for target in &words[2..] {
                        if !target.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
                            || target.starts_with('-') || !targets.insert(target.to_lowercase()) {
                            return Err("test targets must be distinct literal relative paths".into());
                        }
                        vcp_repository::path::relative(Path::new(target)).map_err(|_| "test target escapes project directory")?;
                        let path = if plan.directory.is_empty() { target.to_string() } else { format!("{}/{target}", plan.directory) };
                        if !observation.sources.iter().any(|s| s.version.path == path) {
                            return Err(format!("test target is unavailable or excluded: {path}"));
                        }
                    }
                    Ok([vec!["--test".into(), "--test-reporter=tap".into(), "--test-concurrency=1".into()], words[2..].iter().map(|s| s.to_string()).collect()].concat())
                }
                Runner::Cargo => {
                    if requirement.manifest.rsplit('/').next() != Some("Cargo.toml") { return Err("Cargo check requires Cargo.toml".into()); }
                    // Cargo, not a second partial TOML parser, validates the manifest.
                    Ok(["test", "--locked", "--offline", "--manifest-path", "Cargo.toml", "--all-targets", "--", "--test-threads=1"].into_iter().map(str::to_owned).collect())
                }
            }
        })();
        match discovered { Ok(arguments) => plan.request.arguments = arguments, Err(reason) => plan.not_run = Some(reason) }
        Ok(plan)
    }).collect()
}

/// Runner output is an observation from untrusted project execution. It is
/// useful only together with the exact native process and input receipts.
pub fn evaluate(
    plan: &Plan,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    reason: Option<&str>,
) -> CheckOutcome {
    let fail = |reason: &str| CheckOutcome::Failed {
        reason: reason.into(),
    };
    if let Some(reason) = &plan.not_run {
        return CheckOutcome::NotRun {
            reason: reason.clone(),
        };
    }
    if let Some(reason) = reason {
        return fail(reason);
    }
    if exit_code != Some(0) {
        return fail("check did not exit successfully");
    }
    if stdout.len().saturating_add(stderr.len()) > 1024 * 1024 {
        return fail("check output exceeds parser bound");
    }
    let Ok(stdout) = std::str::from_utf8(stdout) else {
        return fail("check stdout is not UTF-8");
    };
    let Ok(stderr) = std::str::from_utf8(stderr) else {
        return fail("check stderr is not UTF-8");
    };
    let mut passed = BTreeSet::new();
    match plan.runner {
        Runner::Node => {
            let mut totals = BTreeMap::new();
            let mut plan_count = None;
            for line in stdout.lines() {
                if line.starts_with("not ok ") || line.starts_with("Bail out!") {
                    return fail("TAP reports failure");
                }
                if let Some(row) = line.strip_prefix("ok ") {
                    let Some((number, name)) = row.split_once(" - ") else {
                        return fail("invalid TAP result");
                    };
                    if number.parse::<u64>().ok() != Some(passed.len() as u64 + 1)
                        || name.contains(" #")
                        || !passed.insert(name.to_owned())
                    {
                        return fail("skipped, duplicate or malformed TAP result");
                    }
                }
                if let Some(row) = line.strip_prefix("1..") {
                    if plan_count.is_some() {
                        return fail("multiple top-level TAP plans");
                    }
                    plan_count = row.parse::<u64>().ok();
                    if plan_count.is_none() {
                        return fail("invalid TAP plan");
                    }
                }
                for key in ["tests", "pass", "fail", "cancelled", "skipped", "todo"] {
                    if let Some(value) = line.strip_prefix(&format!("# {key} ")) {
                        let Ok(value) = value.parse::<u64>() else {
                            return fail("invalid TAP summary");
                        };
                        if totals.insert(key, value).is_some() {
                            return fail("duplicate TAP summary");
                        }
                    }
                }
            }
            let count = passed.len() as u64;
            if count == 0
                || plan_count != Some(count)
                || totals.get("tests") != Some(&count)
                || totals.get("pass") != Some(&count)
                || ["fail", "cancelled", "skipped", "todo"]
                    .iter()
                    .any(|key| totals.get(key) != Some(&0))
            {
                return fail("TAP did not prove a nonempty, complete, unskipped check");
            }
            // This bounded parser intentionally rejects nested suites until their
            // parent/child accounting is qualified, instead of guessing coverage.
        }
        Runner::Cargo => {
            let mut total = 0u64;
            let mut summaries = 0;
            for line in stdout.lines().chain(stderr.lines()) {
                if let Some(row) = line
                    .strip_prefix("test ")
                    .and_then(|s| s.strip_suffix(" ... ok"))
                {
                    if !passed.insert(row.to_owned()) {
                        return fail("ambiguous duplicate Rust test name");
                    }
                }
                if let Some(row) = line.strip_prefix("test result: ") {
                    let fields: Vec<_> = row.split_whitespace().collect();
                    if fields.len() < 13
                        || fields[0] != "ok."
                        || fields[2] != "passed;"
                        || fields[3] != "0"
                        || fields[4] != "failed;"
                        || fields[5] != "0"
                        || fields[6] != "ignored;"
                        || fields[7] != "0"
                        || fields[8] != "measured;"
                        || fields[9] != "0"
                        || fields[10..13] != ["filtered", "out;", "finished"]
                    {
                        return fail("Rust test summary reports failure or omitted tests");
                    }
                    let Ok(count) = fields[1].parse::<u64>() else {
                        return fail("invalid Rust test count");
                    };
                    let Some(next) = total.checked_add(count) else {
                        return fail("Rust count overflow");
                    };
                    total = next;
                    summaries += 1;
                }
            }
            if summaries == 0 || total == 0 || total != passed.len() as u64 {
                return fail("Rust runner did not prove nonempty test execution");
            }
        }
    }
    if !plan.expected_tests.iter().all(|name| passed.contains(name)) {
        return fail("expected acceptance tests were not observed");
    }
    CheckOutcome::Passed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(runner: Runner) -> Plan {
        Plan {
            specification: "project#test".into(),
            origin: None,
            directory: ".".into(),
            runner,
            request: process::Request {
                profile: "test".into(),
                arguments: vec![],
                directory: ".".into(),
                timeout_ms: 1000,
                output_bytes: 1024,
                input: None,
            },
            expected_tests: vec!["seeded_acceptance".into()],
            rationale: "fixture acceptance".into(),
            not_run: None,
        }
    }
    #[test]
    fn verification_requires_expected_tests_and_complete_runner_results() {
        let tap = b"TAP version 13\nok 1 - seeded_acceptance\n1..1\n# tests 1\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n";
        let node = plan(Runner::Node);
        assert_eq!(
            evaluate(&node, Some(0), tap, b"", None),
            CheckOutcome::Passed
        );
        for output in [b"".as_slice(), b"1..0\n# tests 0\n", b"ok 1 - unrelated\n1..1\n# tests 1\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n"] {
            assert!(matches!(evaluate(&node, Some(0), output, b"", None), CheckOutcome::Failed { .. }));
        }
        assert!(matches!(
            evaluate(&node, Some(1), tap, b"", None),
            CheckOutcome::Failed { .. }
        ));
        assert!(matches!(
            evaluate(&node, Some(0), tap, b"", Some("timeout")),
            CheckOutcome::Failed { .. }
        ));
        for suffix in ["# pass 1\n", "not ok 2 - failure\n", "1..1\n"] {
            assert!(matches!(
                evaluate(
                    &node,
                    Some(0),
                    &[tap.as_slice(), suffix.as_bytes()].concat(),
                    b"",
                    None
                ),
                CheckOutcome::Failed { .. }
            ));
        }
        let cargo = plan(Runner::Cargo);
        let output = b"running 1 test\ntest seeded_acceptance ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        assert_eq!(
            evaluate(&cargo, Some(0), output, b"", None),
            CheckOutcome::Passed
        );
        assert!(matches!(evaluate(&cargo, Some(0), b"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n", b"", None), CheckOutcome::Failed { .. }));
        let filtered = String::from_utf8(output.to_vec())
            .unwrap()
            .replace("0 filtered", "1 filtered");
        assert!(matches!(
            evaluate(&cargo, Some(0), filtered.as_bytes(), b"", None),
            CheckOutcome::Failed { .. }
        ));
    }
}
