// SPDX-License-Identifier: Apache-2.0
//! Labels are frozen separately from history and never consumed by ingestion.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub question: String,
    pub query: String,
    pub allowed: Vec<String>,
    pub relevant: Vec<String>,
    pub stale: Vec<String>,
}
pub fn grade(case: &Case, returned: &[String], supported: bool, succeeded: bool) -> Value {
    let unique: BTreeSet<_> = returned.iter().collect();
    let hits = case
        .relevant
        .iter()
        .filter(|id| unique.contains(id))
        .count();
    let forbidden = returned
        .iter()
        .filter(|id| !case.allowed.contains(id))
        .count();
    let stale = returned.iter().filter(|id| case.stale.contains(id)).count();
    let unsupported = usize::from(!supported);
    let task_pass = succeeded
        && hits == case.relevant.len()
        && forbidden == 0
        && stale == 0
        && unsupported == 0
        && returned.iter().all(|id| case.relevant.contains(id));
    json!({"relevant_denominator":case.relevant.len(),"sourced_hits":hits,
        "returned_denominator":returned.len(),"forbidden_passages":forbidden,
        "stale_passages":stale,"unsupported_passages":unsupported,"task_pass":task_pass})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failures_and_empty_answers_do_not_inflate_recall() {
        let case = Case {
            id: "x".into(),
            question: "x".into(),
            query: "x".into(),
            allowed: vec!["good".into(), "old".into()],
            relevant: vec!["good".into()],
            stale: vec!["old".into()],
        };
        assert_eq!(grade(&case, &[], true, false)["task_pass"], false);
        let result = grade(
            &case,
            &["good".into(), "good".into(), "old".into(), "private".into()],
            true,
            true,
        );
        assert_eq!(result["sourced_hits"], 1);
        assert_eq!(result["forbidden_passages"], 1);
        assert_eq!(result["stale_passages"], 1);
        assert_eq!(result["task_pass"], false);
        assert_eq!(
            grade(&case, &["good".into()], false, true)["task_pass"],
            false
        );
    }
}
