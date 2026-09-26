// SPDX-License-Identifier: Apache-2.0
//! Two independently drafted normal document cases. This module checks bytes,
//! parsed Markdown and CSV schema only, never factual coverage or reader scores.
//! Word bounds and reported-byte identity are checked by the companion oracle;
//! tool/claim receipts and semantic quality need independent blinded review.
use super::{digest_bytes, safe, texts, Checked, Files, Value};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde_json::json;
use std::collections::BTreeSet;

const MANIFEST: &str =
    include_str!("../../../../../evals/skills/authoring-qualification/manifest.json");
const ORACLES: [&str; 2] = [
    include_str!("../../../../../evals/skills/authoring-qualification/private-grader/DOC-fresh-format-reference-v1.json"),
    include_str!("../../../../../evals/skills/authoring-qualification/private-grader/DOC-fresh-acceptance-plan-v1.json"),
];
const ORIGINAL: &str = include_str!("../../../../../evals/skills/authoring-qualification/projects/DOC-fresh-format-reference-v1/docs/import-format.md");
pub(super) const CASES: [&str; 2] = [
    "DOC-fresh-format-reference-v1",
    "DOC-fresh-acceptance-plan-v1",
];
const MARKERS: [&str; 2] = ["<!-- FORMAT START -->", "<!-- FORMAT END -->"];

fn scope(case: &str) -> Checked<(&'static str, &'static [&'static str])> {
    match case {
        "DOC-fresh-format-reference-v1" => Ok((
            "docs/import-format.md",
            &[
                "sources/parser-contract.md",
                "sources/examples.md",
                "sources/check-record.md",
            ],
        )),
        "DOC-fresh-acceptance-plan-v1" => Ok((
            "docs/saved-views-acceptance.md",
            &[
                "sources/decision.md",
                "sources/implementation.md",
                "sources/evidence.md",
            ],
        )),
        _ => Err("unknown prospective document case".into()),
    }
}

pub(super) fn contract(case: &str) -> Checked<(Value, Value)> {
    let (target, sources) = scope(case)?;
    let manifest: Value =
        serde_json::from_str(MANIFEST).map_err(|_| "invalid prospective manifest")?;
    if manifest["revision"] != "cs-1-fresh-document-fixtures-v1"
        || manifest["schema_version"] != 1
        || manifest["case_count"] != 2
    {
        return Err("unsupported prospective cohort".into());
    }
    let task = manifest["cases"]
        .as_array()
        .ok_or("prospective cases unavailable")?
        .iter()
        .find(|task| task["id"] == case)
        .ok_or("prospective task missing")?
        .clone();
    let source = ORACLES
        .iter()
        .find(|source| {
            task["expected"]["oracle"]["sha256"] == digest_bytes(source.as_bytes())
                && task["expected"]["oracle"]["bytes"] == source.len() as u64
        })
        .ok_or("prospective oracle identity mismatch")?;
    let oracle: Value = serde_json::from_str(source).map_err(|_| "invalid prospective oracle")?;
    let (outputs, modified) = if case == CASES[0] {
        (Vec::<&str>::new(), vec![target])
    } else {
        (vec![target], Vec::<&str>::new())
    };
    if oracle["schema"] != "cs1-fresh-doc-private-oracle-draft/1"
        || oracle["case_id"] != case
        || oracle["allowed_outputs"] != json!(outputs)
        || oracle["allowed_modifications"] != json!(modified)
        || oracle["preserve_files"] != json!(sources)
        || texts(&task["context"], "tools")?
            != [
                "vcp_list",
                "vcp_read",
                "vcp_search",
                "vcp_patch",
                "vcp_verify",
            ]
    {
        return Err("unsupported prospective scope".into());
    }
    Ok((task, oracle))
}

fn region(text: &str) -> Checked<(&str, &str, &str)> {
    if MARKERS
        .iter()
        .any(|marker| text.matches(marker).count() != 1)
    {
        return Err("exactly one ordered format marker pair required".into());
    }
    let start = text.find(MARKERS[0]).ok_or("format start missing")? + MARKERS[0].len();
    let end = text.find(MARKERS[1]).ok_or("format end missing")?;
    if start > end {
        return Err("format markers reversed".into());
    }
    Ok((&text[..start], &text[start..end], &text[end..]))
}

pub(super) fn preserved(files: &Files, case: &str) -> Checked<()> {
    if case == CASES[0] {
        let bytes = files.get(scope(case)?.0).ok_or("reference target absent")?;
        let content = std::str::from_utf8(bytes).map_err(|_| "reference is not UTF-8")?;
        let before = region(ORIGINAL)?;
        let after = region(content)?;
        if before.0 != after.0 || before.2 != after.2 {
            return Err("outside marked reference region changed".into());
        }
    }
    Ok(())
}

// Resolve relative Markdown file targets, including ../sources from docs/.
// Fragments and raw HTML are outside this structural gate, never fetched.
fn resolve_link(containing: &str, raw: &str) -> Checked<Option<String>> {
    if raw.starts_with("//")
        || raw.split_once(':').is_some_and(|(scheme, _)| {
            scheme.len() > 1
                && !scheme.eq_ignore_ascii_case("file")
                && scheme.as_bytes()[0].is_ascii_alphabetic()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        })
    {
        return Ok(None);
    }
    let target = super::link_target(raw.split('?').next().unwrap_or(raw))?;
    if target.is_empty() {
        return Ok(None);
    }
    if target.starts_with('/') || target.contains(['\\', ':', '?']) {
        return Err("unsafe prospective local link".into());
    }
    let mut parts: Vec<_> = containing.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop().ok_or("local link escapes workspace")?;
            }
            "" => return Err("empty local link component".into()),
            other => parts.push(other),
        }
    }
    let resolved = parts.join("/");
    if !safe(&resolved) {
        return Err("unsafe resolved local link".into());
    }
    Ok(Some(resolved))
}

// Bounded fixture CSV: quoted commas/doubled quotes, no embedded line breaks.
// This validates sample data, not the surrounding prose's meaning.
fn csv_row(line: &str) -> Checked<Vec<String>> {
    let mut cells = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else if ch == ',' {
            cells.push(std::mem::take(&mut field));
            closed = false;
        } else if ch == '"' && field.is_empty() && !closed {
            quoted = true;
        } else if ch == '"' || closed {
            return Err("malformed CSV field quoting".into());
        } else {
            field.push(ch);
        }
    }
    if quoted {
        return Err("unclosed CSV quote or multiline field".into());
    }
    cells.push(field);
    Ok(cells)
}

fn csv_example(text: &str) -> Checked<()> {
    // The supplied format permits exactly one UTF-8 BOM at the file start.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().filter(|line| !line.is_empty());
    if lines.next() != Some("item_id,label,quantity,location") {
        return Err("CSV example header mismatch".into());
    }
    let mut ids = BTreeSet::new();
    let (mut count, mut zero, mut blank, mut comma) = (0, false, false, false);
    for line in lines {
        let row = csv_row(line)?;
        if row.len() != 4 {
            return Err("CSV example column count".into());
        }
        let id = &row[0];
        if !(3..=16).contains(&id.len())
            || !id.as_bytes()[0].is_ascii_uppercase()
            || !id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
            || !ids.insert(id.clone())
        {
            return Err("CSV example ID invalid or duplicate".into());
        }
        let label = row[1].trim_matches(' ');
        if label.is_empty() || label.chars().count() > 80 {
            return Err("CSV example label invalid".into());
        }
        if row[2].is_empty() || !row[2].bytes().all(|b| b.is_ascii_digit()) {
            return Err("CSV example quantity is not a decimal integer".into());
        }
        let quantity = row[2]
            .parse::<u32>()
            .map_err(|_| "CSV example quantity exceeds bound")?;
        if quantity > 9999 {
            return Err("CSV example quantity exceeds bound".into());
        }
        if !["", "north", "south", "reserve"].contains(&row[3].as_str()) {
            return Err("CSV example location invalid".into());
        }
        count += 1;
        zero |= quantity == 0;
        blank |= row[3].is_empty();
        comma |= row[1].contains(',');
    }
    if count < 2 || !zero || !blank || !comma {
        return Err(
            "CSV sample must include two valid rows, zero, blank location and quoted comma".into(),
        );
    }
    Ok(())
}

pub(super) fn structure(files: &Files, case: &str, _oracle: &Value) -> Checked<()> {
    let (target, required) = scope(case)?;
    let bytes = files.get(target).ok_or("prospective artifact missing")?;
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err("prospective artifact byte bound".into());
    }
    let content = std::str::from_utf8(bytes).map_err(|_| "prospective artifact is not UTF-8")?;
    let content = if case == CASES[0] {
        region(content)?.1
    } else {
        content
    };
    let mut links = BTreeSet::new();
    let mut csv = None;
    let mut csv_blocks = Vec::new();
    let (mut in_scenario_table, mut table_rows, mut ordered_list) = (false, 0, false);
    for event in Parser::new_ext(content, Options::ENABLE_TABLES) {
        match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                if let Some(target) = resolve_link(target, &dest_url)? {
                    if !files.contains_key(&target) {
                        return Err("prospective local link target missing".into());
                    }
                    links.insert(target);
                }
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if language.as_ref() == "csv" =>
            {
                csv = Some(String::new())
            }
            Event::Text(text) if csv.is_some() => {
                if let Some(body) = csv.as_mut() {
                    body.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(body) = csv.take() {
                    csv_blocks.push(body);
                }
            }
            Event::Start(Tag::Table(columns)) => in_scenario_table = columns.len() >= 3,
            Event::End(TagEnd::Table) => in_scenario_table = false,
            Event::Start(Tag::TableRow) if in_scenario_table => table_rows += 1,
            Event::Start(Tag::List(Some(_))) => ordered_list = true,
            _ => {}
        }
    }
    if required.iter().any(|source| !links.contains(*source)) {
        return Err("prospective required source citation missing".into());
    }
    if case == CASES[0] {
        if csv_blocks.is_empty() {
            return Err("fenced CSV example missing".into());
        }
        for example in csv_blocks {
            csv_example(&example)?;
        }
    } else if table_rows == 0 || !ordered_list {
        return Err("acceptance plan requires scenario table and ordered validation list".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(case: &str) -> Files {
        let (task, _) = contract(case).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/skills/authoring-qualification/projects")
            .join(case);
        let mut files = super::super::scaffold(case).unwrap();
        for source in task["expected"]["source_files"].as_array().unwrap() {
            let name = source["path"].as_str().unwrap();
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        files
    }
    fn reference(files: &mut Files) {
        let body = "\n[Contract](../sources/parser-contract.md) [Examples](../sources/examples.md) [Checks](../sources/check-record.md)\n\n```csv\nitem_id,label,quantity,location\nKIT-4,\"Cable, blue\",0,reserve\nBOX-2,Storage box,12,\n```\n";
        let (prefix, _, suffix) = region(ORIGINAL).unwrap();
        files.insert(
            scope(CASES[0]).unwrap().0.into(),
            format!("{prefix}{body}{suffix}").into_bytes(),
        );
    }
    #[test]
    fn contracts_and_preservation_are_case_bound() {
        for case in CASES {
            contract(case).unwrap();
        }
        assert!(contract("DOC-invented-v1").is_err());
        let mut files = fixture(CASES[0]);
        reference(&mut files);
        let (task, oracle) = contract(CASES[0]).unwrap();
        assert!(super::super::preserved(&files, CASES[0], &task, &oracle).is_ok());
        assert!(structure(&files, CASES[0], &oracle).is_ok());
        let target = scope(CASES[0]).unwrap().0;
        files
            .get_mut(target)
            .unwrap()
            .extend_from_slice(b"outside edit");
        assert!(super::super::preserved(&files, CASES[0], &task, &oracle).is_err());
        reference(&mut files);
        files.insert("sources/examples.md".into(), b"changed".to_vec());
        assert!(super::super::preserved(&files, CASES[0], &task, &oracle).is_err());
    }
    #[test]
    fn markers_reject_duplication_reversal_and_missing() {
        for value in [
            "",
            "<!-- FORMAT END --><!-- FORMAT START -->",
            "<!-- FORMAT START --><!-- FORMAT START --><!-- FORMAT END -->",
        ] {
            assert!(region(value).is_err());
        }
        let mut files = fixture(CASES[0]);
        reference(&mut files);
        let (_, oracle) = contract(CASES[0]).unwrap();
        files.insert("extra.md".into(), b"unrequested".to_vec());
        let (task, _) = contract(CASES[0]).unwrap();
        assert!(super::super::preserved(&files, CASES[0], &task, &oracle).is_err());
    }
    #[test]
    fn csv_schema_checks_values_not_prose_keywords() {
        let valid = "item_id,label,quantity,location\r\nKIT-4,\"Cable, \"\"blue\"\"\",0,reserve\r\nBOX-2,Storage box,12,\r\n";
        assert!(csv_example(valid).is_ok());
        assert!(csv_example(&format!("\u{feff}{valid}")).is_ok());
        assert!(csv_example(&format!("\n\u{feff}{valid}")).is_err());
        assert!(csv_example(&format!("\u{feff}\u{feff}{valid}")).is_err());
        assert!(csv_example(&valid.replace("Storage box", "Storage\u{feff}box")).is_ok());
        for changed in [
            valid.replace("BOX-2", "KIT-4"),
            valid.replace(",0,", ",1.5,"),
            valid.replace("reserve", "North"),
            valid.replace("KIT-4", "kit-4"),
            valid.replace("Storage box", "   "),
            valid.replace("12,", "10000,"),
        ] {
            assert!(csv_example(&changed).is_err());
        }
        assert!(csv_row("A,\"bad\"tail,0,").is_err());
        assert!(csv_row("A,\"open,0,").is_err());
        for changed in [
            valid.replace(
                "item_id,label,quantity,location",
                "label,item_id,quantity,location",
            ),
            valid.replace(",0,", ",1,"),
            valid.replace("12,\r\n", "12,north\r\n"),
            valid.replace("Cable, ", "Cable "),
            "item_id,label,quantity,location\nKIT-4,\"Cable, blue\",0,\n".into(),
        ] {
            assert!(csv_example(&changed).is_err());
        }
    }
    #[test]
    fn markdown_links_ignore_code_and_reject_escape() {
        assert_eq!(
            resolve_link("docs/a.md", "../sources/evidence.md").unwrap(),
            Some("sources/evidence.md".into())
        );
        assert!(resolve_link("docs/a.md", "../../outside.md").is_err());
        assert!(resolve_link("docs/a.md", "%2e%2e/%2e%2e/outside.md").is_err());
        assert!(resolve_link("docs/a.md", "file:///outside.md").is_err());
        assert!(resolve_link("docs/a.md", "C:/outside.md").is_err());
        let mut files = fixture(CASES[0]);
        reference(&mut files);
        let target = scope(CASES[0]).unwrap().0;
        let text = String::from_utf8(files[target].clone()).unwrap();
        files.insert(
            target.into(),
            text.replace(
                "[Contract](../sources/parser-contract.md)",
                "`[Contract](../sources/parser-contract.md)`",
            )
            .into_bytes(),
        );
        assert!(structure(&files, CASES[0], &contract(CASES[0]).unwrap().1).is_err());
    }
    #[test]
    fn acceptance_structure_never_establishes_semantics() {
        let mut files = fixture(CASES[1]);
        // Deliberately meaningless content: structural pass is not quality.
        let body = "# Plan\n[Decision](../sources/decision.md) [Implementation](../sources/implementation.md) [Evidence](../sources/evidence.md)\n\n| A | B | C |\n|---|---|---|\n| x | y | z |\n\n1. Unreviewed placeholder.\n";
        let target = scope(CASES[1]).unwrap().0;
        files.insert(target.into(), body.as_bytes().to_vec());
        let oracle = contract(CASES[1]).unwrap().1;
        assert!(structure(&files, CASES[1], &oracle).is_ok());
        files.insert(
            target.into(),
            body.replace("1. Unreviewed", "- Unreviewed").into_bytes(),
        );
        assert!(structure(&files, CASES[1], &oracle).is_err());
        files.insert(target.into(), format!("```\n{body}\n```").into_bytes());
        assert!(structure(&files, CASES[1], &oracle).is_err());
        files.insert(
            target.into(),
            body.replace(
                "| A | B | C |\n|---|---|---|\n| x | y | z |",
                "| A | B |\n|---|---|\n| x | y |",
            )
            .into_bytes(),
        );
        assert!(structure(&files, CASES[1], &oracle).is_err());
        files.insert(
            target.into(),
            body.replace("| x | y | z |\n", "").into_bytes(),
        );
        assert!(structure(&files, CASES[1], &oracle).is_err());
    }
    #[test]
    fn reference_structure_accepts_false_prose_but_rejects_missing_mechanics() {
        let mut files = fixture(CASES[0]);
        reference(&mut files);
        let target = scope(CASES[0]).unwrap().0;
        let original = String::from_utf8(files[target].clone()).unwrap();
        let false_prose = original.replace("<!-- FORMAT START -->", "<!-- FORMAT START -->\nBlank location defaults to north. Semicolon files are supported. Production tests passed.\n");
        files.insert(target.into(), false_prose.into_bytes());
        let oracle = contract(CASES[0]).unwrap().1;
        // These false claims MUST fail independent semantic review. Native
        // mechanics intentionally cannot certify or reject their meaning.
        assert!(structure(&files, CASES[0], &oracle).is_ok());
        for changed in [
            original.replace("```csv", "```text"),
            original.replace("../sources/check-record.md", "../sources/missing.md"),
            original.replace("[Checks](../sources/check-record.md)", "Checks"),
            original.replace("../sources/check-record.md", "../../outside.md"),
        ] {
            files.insert(target.into(), changed.into_bytes());
            assert!(structure(&files, CASES[0], &oracle).is_err());
        }
        files.insert(target.into(), vec![b'x'; 65537]);
        assert!(structure(&files, CASES[0], &oracle).is_err());
    }

    #[cfg(windows)]
    fn staged_workspace(case: &str) -> (tempfile::TempDir, Files) {
        let directory = tempfile::tempdir().unwrap();
        let mut files = fixture(case);
        for (name, bytes) in &files {
            let path = directory.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        // Exercise the acceptance plan's initially absent output directory as
        // well as the reference's existing target. This is test staging only;
        // the checker itself must never create either directory or artifact.
        let target = scope(case).unwrap().0;
        if case == CASES[0] {
            assert!(directory.path().join(target).is_file());
            reference(&mut files);
        } else {
            assert!(!directory.path().join("docs").exists());
            // Mechanical fixture, intentionally not a factual/quality answer.
            let body = "# Structural plan fixture\n[Decision](../sources/decision.md) [Implementation](../sources/implementation.md) [Evidence](../sources/evidence.md)\n\n| Preconditions | Action | Expected outcome |\n|---|---|---|\n| Fixture input | Fixture action | Fixture observation |\n\n1. Independent semantic review remains required.\n";
            files.insert(target.into(), body.as_bytes().to_vec());
        }
        let output = directory.path().join(target);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(output, &files[target]).unwrap();
        (directory, files)
    }

    #[test]
    #[cfg(windows)]
    fn prospective_cases_pass_the_owner_bound_read_only_entrypoint() {
        let runtime = tempfile::tempdir().unwrap();
        let executable = runtime.path().join("vcp-authoring-check.exe");
        let workspaces: Vec<_> = CASES.into_iter().map(staged_workspace).collect();
        let assignments: Vec<_> = workspaces
            .iter()
            .zip(CASES)
            .map(|((directory, _), case)| json!({"workspace":directory.path(), "case_id":case}))
            .collect();
        let owner_bytes =
            serde_json::to_vec(&json!({"schema_version":1,"cases":assignments})).unwrap();
        let owner_path = runtime.path().join("authoring-cases.json");
        std::fs::write(&owner_path, &owner_bytes).unwrap();

        for ((directory, expected), case) in workspaces.iter().zip(CASES) {
            assert_eq!(crate::collect(directory.path()).unwrap(), *expected);
            let selected = crate::owner_case(directory.path(), &executable).unwrap();
            assert_eq!(selected, case);
            let result = crate::check(directory.path(), &selected);
            assert!(result.iter().all(Result::is_ok), "{case}: {result:?}");
            // Exact file-set/byte equality covers every source, scaffold file,
            // target and preserved reference prefix/suffix after the check.
            assert_eq!(crate::collect(directory.path()).unwrap(), *expected);
            assert_eq!(std::fs::read(&owner_path).unwrap(), owner_bytes);
        }
        let unlisted = tempfile::tempdir().unwrap();
        assert!(crate::owner_case(unlisted.path(), &executable).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn prospective_real_entrypoint_rejects_source_scaffold_and_scope_mutations() {
        for case in CASES {
            let (directory, expected) = staged_workspace(case);
            let runtime = tempfile::tempdir().unwrap();
            let executable = runtime.path().join("vcp-authoring-check.exe");
            let owner_path = runtime.path().join("authoring-cases.json");
            let owner_bytes = serde_json::to_vec(&json!({
                "schema_version":1,
                "cases":[{"workspace":directory.path(),"case_id":case}]
            }))
            .unwrap();
            std::fs::write(&owner_path, &owner_bytes).unwrap();
            let selected = crate::owner_case(directory.path(), &executable).unwrap();
            for name in [scope(case).unwrap().1[0], "checks/authoring.test.cjs"] {
                let path = directory.path().join(name);
                std::fs::write(&path, b"changed during test setup\n").unwrap();
                let before = crate::collect(directory.path()).unwrap();
                assert!(crate::check(directory.path(), &selected)[0].is_err());
                assert_eq!(crate::collect(directory.path()).unwrap(), before);
                std::fs::write(path, &expected[name]).unwrap();
            }
            let target = scope(case).unwrap().0;
            if case == CASES[0] {
                let mut changed = expected[target].clone();
                changed.extend_from_slice(b"outside marked region\n");
                std::fs::write(directory.path().join(target), changed).unwrap();
            } else {
                std::fs::write(directory.path().join("docs/unrequested.md"), b"extra").unwrap();
            }
            let before = crate::collect(directory.path()).unwrap();
            assert!(crate::check(directory.path(), &selected)[0].is_err());
            assert_eq!(crate::collect(directory.path()).unwrap(), before);
            assert_eq!(std::fs::read(owner_path).unwrap(), owner_bytes);
        }
    }
}
