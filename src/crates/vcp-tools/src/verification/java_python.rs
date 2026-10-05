// SPDX-License-Identifier: Apache-2.0
//! Parsers for qualified, verbose Maven Surefire and pytest process output.
use super::Runner;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn results(
    runner: Runner,
    stdout: &str,
    stderr: &str,
) -> Result<BTreeSet<String>, &'static str> {
    match runner {
        Runner::Maven => maven(stdout, stderr),
        Runner::Pytest => pytest(stdout, stderr),
        _ => Err("unsupported Java/Python runner"),
    }
}

fn maven(stdout: &str, stderr: &str) -> Result<BTreeSet<String>, &'static str> {
    let class_summary = regex::Regex::new(r"^Tests run: ([0-9]+), Failures: 0, Errors: 0, Skipped: 0, Time elapsed: [0-9]+(?:\.[0-9]+)? s -- in (\S+)$")
        .map_err(|_| "invalid Maven class parser")?;
    let total_summary =
        regex::Regex::new(r"^Tests run: ([0-9]+), Failures: 0, Errors: 0, Skipped: 0$")
            .map_err(|_| "invalid Maven total parser")?;
    let test_result = regex::Regex::new(r"^(\S(?:.*\S)?) -- Time elapsed: [0-9]+(?:\.[0-9]+)? s$")
        .map_err(|_| "invalid Maven test parser")?;
    let mut classes = BTreeMap::new();
    let mut running = BTreeSet::new();
    let mut passed = BTreeSet::new();
    let mut total = None;
    let mut successful = false;
    for line in stdout.lines().chain(stderr.lines()) {
        if line.starts_with("[ERROR]") || line.contains("BUILD FAILURE") {
            return Err("Maven reports failure");
        }
        let Some(row) = line.strip_prefix("[INFO] ") else {
            continue;
        };
        if let Some(name) = row.strip_prefix("Running ") {
            if name.is_empty() || !running.insert(name.to_owned()) {
                return Err("duplicate or malformed Maven test class");
            }
        } else if row.starts_with("Tests run: ") {
            if let Some(captures) = class_summary.captures(row) {
                let count = captures[1]
                    .parse::<u64>()
                    .map_err(|_| "invalid Maven class count")?;
                if count == 0 || classes.insert(captures[2].to_owned(), count).is_some() {
                    return Err("empty or duplicate Maven class summary");
                }
            } else if let Some(captures) = total_summary.captures(row) {
                let count = captures[1]
                    .parse::<u64>()
                    .map_err(|_| "invalid Maven total count")?;
                if total.replace(count).is_some() {
                    return Err("duplicate Maven total summary");
                }
            } else {
                return Err("Maven summary reports failure, skipped tests or malformed counts");
            }
        } else if row.contains(" -- Time elapsed: ") {
            let captures = test_result.captures(row).ok_or("Maven test did not pass")?;
            let name = &captures[1];
            if name.chars().any(char::is_control) || !passed.insert(name.to_owned()) {
                return Err("duplicate or malformed Maven test result");
            }
        } else if row == "BUILD SUCCESS" {
            if successful {
                return Err("duplicate Maven build result");
            }
            successful = true;
        }
    }
    if !successful
        || passed.is_empty()
        || total != Some(passed.len() as u64)
        || running != classes.keys().cloned().collect()
    {
        return Err("Maven did not prove nonempty complete test execution");
    }
    let mut accounted = BTreeSet::new();
    for (class, count) in classes {
        let prefix = format!("{class}.");
        let methods: Vec<_> = passed
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .collect();
        if methods.len() as u64 != count || methods.iter().any(|name| !accounted.insert(*name)) {
            return Err("Maven class counts do not match named tests");
        }
    }
    if accounted.len() != passed.len() {
        return Err("Maven test results lack class summaries");
    }
    Ok(passed)
}

fn pytest(stdout: &str, stderr: &str) -> Result<BTreeSet<String>, &'static str> {
    let collection = regex::Regex::new(r"^(?:collecting \.\.\. )?collected ([0-9]+) items?$")
        .map_err(|_| "invalid pytest collection parser")?;
    let result = regex::Regex::new(
        r"^(\S+::.+?) (PASSED|FAILED|ERROR|SKIPPED|XFAIL|XPASS)(?:\s+\[\s*[0-9]+%\])?$",
    )
    .map_err(|_| "invalid pytest result parser")?;
    let summary = regex::Regex::new(
        r"^=+ ([0-9]+) passed(?:, [0-9]+ warnings?)? in [0-9]+(?:\.[0-9]+)?s(?: \([^)]+\))? =+$",
    )
    .map_err(|_| "invalid pytest summary parser")?;
    let mut collected = None;
    let mut total = None;
    let mut passed = BTreeSet::new();
    for line in stdout.lines().chain(stderr.lines()).map(str::trim_end) {
        if line.starts_with("collected ") || line.starts_with("collecting ... collected ") {
            let captures = collection
                .captures(line)
                .ok_or("pytest collection omitted tests or failed")?;
            let count = captures[1]
                .parse::<u64>()
                .map_err(|_| "invalid pytest collection count")?;
            if collected.replace(count).is_some() {
                return Err("duplicate pytest collection count");
            }
        } else if let Some(captures) = result.captures(line) {
            if &captures[2] != "PASSED"
                || captures[1].chars().any(char::is_control)
                || !passed.insert(captures[1].to_owned())
            {
                return Err("pytest reports failed, skipped or duplicate tests");
            }
        } else if let Some(captures) = summary.captures(line) {
            let count = captures[1]
                .parse::<u64>()
                .map_err(|_| "invalid pytest summary count")?;
            if total.replace(count).is_some() {
                return Err("duplicate pytest summary count");
            }
        } else if line.starts_with('=')
            && line.ends_with('=')
            && [
                " failed",
                " error",
                " skipped",
                " xfailed",
                " xpassed",
                " deselected",
                "no tests ran",
                "interrupted",
            ]
            .iter()
            .any(|status| line.contains(status))
        {
            return Err("pytest summary reports failed or omitted tests");
        }
    }
    if passed.is_empty() || collected != Some(passed.len() as u64) || total != collected {
        return Err("pytest did not prove nonempty complete test execution");
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAVEN: &str = "[INFO] Running example.AcceptanceTest\n[INFO] Tests run: 2, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 0.453 s -- in example.AcceptanceTest\n[INFO] example.AcceptanceTest.first -- Time elapsed: 0.323 s\n[INFO] example.AcceptanceTest.second -- Time elapsed: 0.006 s\n[INFO] Tests run: 2, Failures: 0, Errors: 0, Skipped: 0\n[INFO] BUILD SUCCESS\n";
    const PYTEST: &str = "============================= test session starts =============================\ncollected 2 items\n\ntests/test_cli.py::test_version_flag PASSED                   [ 50%]\ntests/test_cli.py::test_current_input PASSED                  [100%]\n\n============================== 2 passed in 0.01s ==============================\n";

    #[test]
    fn maven_reconciles_named_methods_classes_and_complete_summary() {
        let names = maven(MAVEN, "").unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains("example.AcceptanceTest.first"));
        for malformed in [
            MAVEN.replace(
                "Tests run: 2, Failures: 0, Errors: 0, Skipped: 0\n",
                "Tests run: 1, Failures: 0, Errors: 0, Skipped: 0\n",
            ),
            MAVEN.replace("Time elapsed: 0.453", "Time elapsed: invalid"),
            MAVEN.replace("Skipped: 0", "Skipped: 1"),
            MAVEN.replace("Errors: 0", "Errors: 1"),
            MAVEN.replace("Failures: 0", "Failures: 1"),
            MAVEN.replace(
                "Running example.AcceptanceTest",
                "Running example.OtherTest",
            ),
            MAVEN.replace("example.AcceptanceTest.first", "example.OtherTest.first"),
            MAVEN.replace(
                "example.AcceptanceTest.second",
                "example.AcceptanceTest.first",
            ),
            MAVEN.replace("[INFO] BUILD SUCCESS\n", ""),
            MAVEN.replace("0.323 s", "0.323 s <<< FAILURE!"),
            format!("{MAVEN}[INFO] BUILD SUCCESS\n"),
            format!("{MAVEN}[INFO] Tests run: 2, Failures: 0, Errors: 0, Skipped: 0\n"),
            format!("{MAVEN}[INFO] example.OtherTest.extra -- Time elapsed: 0.001 s\n"),
            "[INFO] Tests run: 0, Failures: 0, Errors: 0, Skipped: 0\n[INFO] BUILD SUCCESS\n"
                .into(),
        ] {
            assert!(maven(&malformed, "").is_err(), "{malformed}");
        }
        assert!(maven(MAVEN, "[ERROR] An additional failure\n").is_err());
        assert!(maven(MAVEN, "[INFO] BUILD FAILURE\n").is_err());
    }

    #[test]
    fn pytest_requires_every_collected_named_test_to_pass_once() {
        let names = pytest(PYTEST, "").unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains("tests/test_cli.py::test_version_flag"));
        assert!(pytest(
            &PYTEST.replace("collected 2 items", "collecting ... collected 2 items"),
            ""
        )
        .is_ok());
        assert!(pytest(&PYTEST.replace("2 passed in", "2 passed, 1 warning in"), "").is_ok());
        for malformed in [
            PYTEST.replace("collected 2 items", "collected 1 item"),
            PYTEST.replace(
                "collected 2 items",
                "collected 3 items / 1 deselected / 2 selected",
            ),
            PYTEST.replace("2 passed in", "1 passed in"),
            PYTEST.replace("PASSED", "SKIPPED"),
            PYTEST.replace("PASSED", "XFAIL"),
            PYTEST.replace("PASSED", "XPASS"),
            PYTEST.replace("PASSED", "FAILED"),
            PYTEST.replace("PASSED", "ERROR"),
            PYTEST.replace("test_current_input", "test_version_flag"),
            format!("{PYTEST}collected 2 items\n"),
            format!("{PYTEST}=== 2 passed in 0.02s ===\n"),
            format!("{PYTEST}=== 2 passed, 1 skipped in 0.02s ===\n"),
            "collected 0 items\n=== no tests ran in 0.01s ===\n".into(),
        ] {
            assert!(pytest(&malformed, "").is_err(), "{malformed}");
        }
        assert!(pytest(PYTEST, "tests/test_cli.py::test_extra SKIPPED [100%]\n").is_err());
    }
}
