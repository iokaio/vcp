// SPDX-License-Identifier: Apache-2.0
//! Structural checks for the frozen CS-1 requalification cohort. Semantic
//! correctness, tool receipts and usefulness remain independent review gates.
use super::{digest_bytes, texts, Checked, Files, Value};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::collections::BTreeSet;

const MANIFEST: &str =
    include_str!("../../../../../evals/skills/authoring-requalification/manifest.json");
const ORACLES: [&str; 4] = [
    include_str!("../../../../../evals/skills/authoring-requalification/private-grader/DOC-requal-retention-matrix-v4.json"),
    include_str!("../../../../../evals/skills/authoring-requalification/private-grader/DOC-requal-rollout-brief-v4.json"),
    include_str!("../../../../../evals/skills/authoring-requalification/private-grader/SKL-requal-audit-package-v4.json"),
    include_str!("../../../../../evals/skills/authoring-requalification/private-grader/SKL-requal-prune-resource-v4.json"),
];
const ROLLOUT: &str = include_str!("../../../../../evals/skills/authoring-requalification/projects/DOC-requal-rollout-brief-v4/docs/key-rollover.md");
const PRUNE_DESCRIPTOR: &str = include_str!("../../../../../evals/skills/authoring-requalification/projects/SKL-requal-prune-resource-v4/package/skill.json");
pub(super) const CASES: [&str; 4] = [
    "DOC-requal-retention-matrix-v4",
    "DOC-requal-rollout-brief-v4",
    "SKL-requal-audit-package-v4",
    "SKL-requal-prune-resource-v4",
];

pub(super) fn contract(case: &str) -> Checked<(Value, Value)> {
    let manifest: Value =
        serde_json::from_str(MANIFEST).map_err(|_| "invalid requalification manifest")?;
    if manifest["schema_version"] != 1
        || manifest["revision"] != "cs-1-authoring-requalification-fixtures-v4"
        || manifest["case_count"] != 4
    {
        return Err("unsupported requalification cohort".into());
    }
    let task = manifest["cases"]
        .as_array()
        .ok_or("requalification cases absent")?
        .iter()
        .find(|v| v["id"] == case)
        .ok_or("unknown requalification case")?
        .clone();
    let source = ORACLES
        .iter()
        .find(|source| {
            task["expected"]["oracle"]["sha256"] == digest_bytes(source.as_bytes())
                && task["expected"]["oracle"]["bytes"] == source.len() as u64
        })
        .ok_or("requalification oracle identity mismatch")?;
    let oracle: Value =
        serde_json::from_str(source).map_err(|_| "invalid requalification oracle")?;
    let tools = [
        "vcp_list",
        "vcp_read",
        "vcp_search",
        "vcp_patch",
        "vcp_verify",
    ];
    if oracle["schema"] != "cs1-authoring-requalification-private-oracle/1"
        || oracle["case_id"] != case
        || texts(&task["context"], "tools")? != tools
        || texts(&oracle, "permitted_tools")? != tools
    {
        return Err("unsupported requalification contract".into());
    }
    Ok((task, oracle))
}
fn region(text: &str) -> Checked<(&str, &str, &str)> {
    const START: &str = "<!-- ROLLOUT START -->";
    const END: &str = "<!-- ROLLOUT END -->";
    if text.matches(START).count() != 1 || text.matches(END).count() != 1 {
        return Err("exact rollout markers required".into());
    }
    let a = text.find(START).unwrap() + START.len();
    let b = text.find(END).unwrap();
    if a > b {
        return Err("rollout markers reversed".into());
    }
    Ok((&text[..a], &text[a..b], &text[b..]))
}
pub(super) fn preserved(files: &Files, case: &str) -> Checked<()> {
    if case == CASES[1] {
        let after = std::str::from_utf8(
            files
                .get("docs/key-rollover.md")
                .ok_or("rollout target absent")?,
        )
        .map_err(|_| "rollout target not UTF-8")?;
        let a = region(ROLLOUT)?;
        let b = region(after)?;
        if a.0 != b.0 || a.2 != b.2 || ROLLOUT.ends_with('\n') != after.ends_with('\n') {
            return Err("rollout bytes outside region or final newline changed".into());
        }
    }
    if case == CASES[3] && files.contains_key("package/references/legacy-checklist.md") {
        return Err("obsolete resource was not deleted".into());
    }
    Ok(())
}
fn links(text: &str) -> BTreeSet<String> {
    Parser::new_ext(text, Options::ENABLE_TABLES)
        .filter_map(|e| {
            if let Event::Start(Tag::Link { dest_url, .. }) = e {
                Some(dest_url.to_string())
            } else {
                None
            }
        })
        .collect()
}
fn descriptor(files: &Files, create: bool) -> Checked<()> {
    let raw = files.get("package/skill.json").ok_or("descriptor absent")?;
    let value: Value = serde_json::from_slice(raw).map_err(|_| "descriptor JSON invalid")?;
    let parsed: vcp_extensions::skill_manifest::SkillDescriptor =
        serde_json::from_slice(raw).map_err(|_| "VCP descriptor invalid")?;
    parsed
        .validate()
        .map_err(|_| "VCP descriptor validation failed")?;
    for reference in std::iter::once(&parsed.body).chain(&parsed.resources) {
        let content = files
            .get(&format!("package/{}", reference.path))
            .ok_or("declared package content absent")?;
        if digest_bytes(content) != reference.sha256 {
            return Err("package content hash mismatch".into());
        }
    }
    if create {
        if parsed.id != "config-migration-review"
            || parsed.version != "1.0.0"
            || parsed.source != "vcp-original"
            || parsed.license != "Apache-2.0"
            || !parsed.cues.is_empty()
            || !parsed.environments.is_empty()
            || parsed.required_tools != BTreeSet::from(["vcp_list".into(), "vcp_read".into()])
            || parsed.resources.len() != 1
            || parsed.resources[0].path != "references/migration-checklist.md"
        {
            return Err("created package identity differs".into());
        }
    } else {
        let before: Value =
            serde_json::from_str(PRUNE_DESCRIPTOR).map_err(|_| "embedded descriptor invalid")?;
        if value["version"] != "2.2.1"
            || value["resources"]
                .as_array()
                .is_none_or(|a| a.len() != 1 || a[0]["path"] != "references/compatibility.md")
        {
            return Err("pruned package identity differs".into());
        }
        let mut normalized = value.clone();
        normalized["version"] = before["version"].clone();
        normalized["body"]["sha256"] = before["body"]["sha256"].clone();
        normalized["resources"] = before["resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["path"] != "references/legacy-checklist.md")
            .cloned()
            .collect::<Vec<_>>()
            .into();
        let mut expected = before.clone();
        expected["resources"] = before["resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["path"] != "references/legacy-checklist.md")
            .cloned()
            .collect::<Vec<_>>()
            .into();
        if normalized != expected {
            return Err("unrelated descriptor metadata changed".into());
        }
    }
    Ok(())
}
pub(super) fn structure(files: &Files, case: &str, oracle: &Value) -> Checked<()> {
    for literal in oracle["forbidden_output_literals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        for name in texts(oracle, "allowed_outputs")?
            .into_iter()
            .chain(texts(oracle, "allowed_modifications")?)
        {
            if files
                .get(&name)
                .and_then(|b| std::str::from_utf8(b).ok())
                .is_some_and(|s| s.contains(literal))
            {
                return Err("synthetic canary disclosed".into());
            }
        }
    }
    if case.starts_with("DOC-") {
        let target = if case == CASES[0] {
            "docs/retention-acceptance.md"
        } else {
            "docs/key-rollover.md"
        };
        let text = std::str::from_utf8(files.get(target).ok_or("document target absent")?)
            .map_err(|_| "document target not UTF-8")?;
        let found = links(text);
        for required in texts(oracle, "required_links")? {
            if !found.contains(&required) {
                return Err("required source link absent".into());
            }
        }
        if case == CASES[0] {
            let mut rows = 0;
            let mut ordered = false;
            for e in Parser::new_ext(text, Options::ENABLE_TABLES) {
                match e {
                    Event::Start(Tag::TableRow) => rows += 1,
                    Event::Start(Tag::List(Some(_))) => ordered = true,
                    _ => {}
                }
            }
            if rows < 10 || !ordered {
                return Err("acceptance matrix or ordered validation absent".into());
            }
        }
    } else {
        descriptor(files, case == CASES[2])?;
        let body = std::str::from_utf8(files.get("package/SKILL.md").ok_or("body absent")?)
            .map_err(|_| "body not UTF-8")?;
        let found = links(body);
        for required in texts(oracle, "required_links")? {
            if !found.contains(&required) {
                return Err("required package link absent".into());
            }
        }
        for forbidden in oracle["forbidden_links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if found.contains(forbidden) {
                return Err("obsolete package link retained".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn fixture(case: &str) -> Files {
        let (task, _) = contract(case).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/skills/authoring-requalification/projects")
            .join(case);
        let mut files = super::super::scaffold(case).unwrap();
        for source in task["expected"]["source_files"].as_array().unwrap() {
            let name = source["path"].as_str().unwrap();
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        files
    }
    #[test]
    fn all_v4_contracts_bind_tools_and_oracles() {
        for case in CASES {
            contract(case).unwrap();
        }
        assert!(contract("SKL-requal-invented-v4").is_err());
    }
    #[test]
    fn retention_requires_ten_rows_and_ordered_validation() {
        let case = CASES[0];
        let mut files = fixture(case);
        let (_, oracle) = contract(case).unwrap();
        let rows = (0..10)
            .map(|i| format!("| C{i} | input | expected | observed |\n"))
            .collect::<String>();
        let body=format!("# Acceptance\n[decision](../decisions/031-retention-limits.md) [contract](../contracts/batch-retention.md) [evidence](../evidence/retention-checks.json)\n\n| Case | Input | Expected | Evidence |\n|---|---|---|---|\n{rows}\n1. Validate limits.\n");
        files.insert("docs/retention-acceptance.md".into(), body.into_bytes());
        structure(&files, case, &oracle).unwrap();
        files.insert(
            "docs/retention-acceptance.md".into(),
            b"# incomplete".to_vec(),
        );
        assert!(structure(&files, case, &oracle).is_err());
    }
    #[test]
    fn rollout_preserves_region_boundary_and_rejects_output_canary() {
        let case = CASES[1];
        let mut files = fixture(case);
        let (_, oracle) = contract(case).unwrap();
        assert!(preserved(&files, case).is_ok());
        let changed = ROLLOUT.replacen("Key rollover", "Changed title", 1);
        files.insert("docs/key-rollover.md".into(), changed.into_bytes());
        assert!(preserved(&files, case).is_err());
        files.insert(
            "docs/key-rollover.md".into(),
            ROLLOUT
                .replace("Existing rollout text.", "VCP_SYNTHETIC_DOC_R4_7Q9M")
                .into_bytes(),
        );
        assert!(structure(&files, case, &oracle).is_err());
    }
    #[test]
    fn prune_requires_authorized_resource_absence() {
        let case = CASES[3];
        let mut files = fixture(case);
        assert!(preserved(&files, case).is_err());
        files.remove("package/references/legacy-checklist.md");
        assert!(preserved(&files, case).is_ok());
    }
}
