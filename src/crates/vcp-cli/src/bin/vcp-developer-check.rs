// SPDX-License-Identifier: Apache-2.0
//! Qualification-only CS-2 data checker. The exact Node-shaped arguments and inert
//! marker adapt the existing manifest/TAP verification contract; Node and workspace
//! code are NEVER executed. No subprocess, network or filesystem writes are performed.
//! Structural evidence only: functional grading happens later, outside the run.
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::ExitCode,
};
use vcp_protocol::digest_bytes;
use vcp_repository::{discovery::Limits, Root, RootIdentity};

const MANIFEST: &str = include_str!("../../../../evals/skills/developer/manifest.json");
const REVISION: &str = "cs-2-developer-fixtures-v5";
const MARKER: &str = "// Inert VCP developer verifier marker; never executed as JavaScript.\n";
const TESTS: [&str; 2] = ["developer input preservation", "developer output structure"];
const ARGUMENTS: [&str; 4] = [
    "--test",
    "--test-reporter=tap",
    "--test-concurrency=1",
    "checks/developer.test.cjs",
];
const TEST_PATH: &str = "checks/developer.test.cjs";
const CASE_PATH: &str = "checks/developer.case.json";
// Oracles for the write cases only; report-only cases configure no checks.
const ORACLES: [(&str, &str); 13] = [
    (
        "UI-normal-form-v2",
        include_str!("../../../../evals/skills/developer/oracles/UI-normal-form-v2.json"),
    ),
    (
        "UI-normal-results-v2",
        include_str!("../../../../evals/skills/developer/oracles/UI-normal-results-v2.json"),
    ),
    (
        "UI-boundary-states-v2",
        include_str!("../../../../evals/skills/developer/oracles/UI-boundary-states-v2.json"),
    ),
    (
        "UI-hostile-tokens-v2",
        include_str!("../../../../evals/skills/developer/oracles/UI-hostile-tokens-v2.json"),
    ),
    (
        "UI-near-miss-parser-v2",
        include_str!("../../../../evals/skills/developer/oracles/UI-near-miss-parser-v2.json"),
    ),
    (
        "MCP-normal-tools-v3",
        include_str!("../../../../evals/skills/developer/oracles/MCP-normal-tools-v3.json"),
    ),
    (
        "MCP-normal-resources-v2",
        include_str!("../../../../evals/skills/developer/oracles/MCP-normal-resources-v2.json"),
    ),
    (
        "MCP-boundary-pages-v3",
        include_str!("../../../../evals/skills/developer/oracles/MCP-boundary-pages-v3.json"),
    ),
    (
        "MCP-near-miss-rest-v3",
        include_str!("../../../../evals/skills/developer/oracles/MCP-near-miss-rest-v3.json"),
    ),
    (
        "LLM-normal-request-v3",
        include_str!("../../../../evals/skills/developer/oracles/LLM-normal-request-v3.json"),
    ),
    (
        "LLM-normal-stream-v3",
        include_str!("../../../../evals/skills/developer/oracles/LLM-normal-stream-v3.json"),
    ),
    (
        "LLM-boundary-partial-v3",
        include_str!("../../../../evals/skills/developer/oracles/LLM-boundary-partial-v3.json"),
    ),
    (
        "LLM-near-miss-parser-v2",
        include_str!("../../../../evals/skills/developer/oracles/LLM-near-miss-parser-v2.json"),
    ),
];
const FILE_LIMIT: usize = 65536;
const EDIT_LIMIT: usize = 262144;
type Checked<T> = Result<T, String>;
type Files = BTreeMap<String, Vec<u8>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerCases {
    schema_version: u32,
    cases: Vec<OwnerCase>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerCase {
    workspace: String,
    case_id: String,
}
/// The frozen task, its oracle and the exact scaffold for one write case.
struct Contract {
    sources: BTreeMap<String, String>,
    editable: BTreeSet<String>,
    html: Vec<String>,
    scaffold: Files,
}

fn strings(value: &Value, key: &str) -> Checked<Vec<String>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing {key}"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "invalid embedded string".into())
        })
        .collect()
}
fn scaffold(case: &str) -> Checked<Files> {
    let id = serde_json::to_string(case).map_err(|_| "case serialization")?;
    Ok(BTreeMap::from([
        (TEST_PATH.into(), MARKER.as_bytes().to_vec()),
        (
            CASE_PATH.into(),
            format!("{{\"schema_version\":1,\"case_id\":{id}}}\n").into_bytes(),
        ),
    ]))
}
fn contract(case: &str) -> Checked<Contract> {
    let manifest: Value =
        serde_json::from_str(MANIFEST).map_err(|_| "embedded manifest invalid")?;
    if manifest["revision"] != REVISION || manifest["case_count"] != 18 {
        return Err("unexpected embedded fixture revision".into());
    }
    let task = manifest["cases"]
        .as_array()
        .ok_or("embedded cases missing")?
        .iter()
        .find(|task| task["id"] == case)
        .ok_or("case absent from the frozen manifest")?;
    let (_, raw) = ORACLES
        .iter()
        .find(|(id, _)| *id == case)
        .ok_or("case has no embedded write oracle")?;
    if task["expected"]["oracle"]["sha256"].as_str() != Some(digest_bytes(raw.as_bytes()).as_str())
    {
        return Err("embedded oracle differs from the frozen manifest".into());
    }
    let oracle: Value = serde_json::from_str(raw).map_err(|_| "embedded oracle invalid")?;
    let editable: BTreeSet<String> = strings(&oracle, "allowed_modifications")?
        .into_iter()
        .collect();
    if editable.is_empty() || !strings(&oracle, "allowed_outputs")?.is_empty() {
        return Err("case is not a frozen write case".into());
    }
    let mut sources = BTreeMap::new();
    for source in task["expected"]["source_files"]
        .as_array()
        .ok_or("frozen sources missing")?
    {
        let path = source["path"]
            .as_str()
            .ok_or("frozen source path missing")?;
        let hash = source["sha256"]
            .as_str()
            .ok_or("frozen source hash missing")?;
        if !safe(path) || sources.insert(path.to_owned(), hash.to_owned()).is_some() {
            return Err("unsafe or duplicate frozen source".into());
        }
    }
    if !editable.iter().all(|name| sources.contains_key(name)) {
        return Err("editable path is not a frozen source".into());
    }
    let html = match oracle.get("html_files") {
        None => Vec::new(),
        Some(_) => strings(&oracle, "html_files")?,
    };
    Ok(Contract {
        sources,
        editable,
        html,
        scaffold: scaffold(case)?,
    })
}
fn safe(name: &str) -> bool {
    !name.contains('\\')
        && name.len() <= 1024
        && vcp_repository::path::relative(Path::new(name))
            .is_ok_and(|normalized| normalized == name)
}
fn open_root(root_path: &Path) -> Checked<Root> {
    Root::open(
        RootIdentity {
            workspace: vcp_domain::WorkspaceId::new(),
            root: vcp_domain::RootId::new(),
            repository: "qualification-developer-check".into(),
            worktree: "qualification-developer-check".into(),
            binding: vcp_domain::Revision::ZERO,
        },
        root_path,
    )
    .map_err(|_| "workspace root is unavailable or redirected".into())
}
/// The owner's case map beside this executable binds the workspace to its case;
/// the workspace marker must agree but never selects the case itself.
fn owner_case(workspace: &Path, executable: &Path) -> Checked<String> {
    let runtime = open_root(executable.parent().ok_or("runtime directory unavailable")?)?;
    let source = runtime
        .read(Path::new("developer-cases.json"), 65536)
        .map_err(|_| "owner case map unavailable or redirected")?;
    let map: OwnerCases =
        serde_json::from_slice(&source.bytes).map_err(|_| "owner case map invalid")?;
    if map.schema_version != 1 || map.cases.is_empty() || map.cases.len() > 54 {
        return Err("owner case map bounds".into());
    }
    let current = open_root(workspace)?
        .path()
        .to_string_lossy()
        .to_lowercase();
    let mut seen = BTreeSet::new();
    let mut selected = None;
    for entry in map.cases {
        contract(&entry.case_id)?;
        let registered = open_root(Path::new(&entry.workspace))?
            .path()
            .to_string_lossy()
            .to_lowercase();
        if !seen.insert(registered.clone()) {
            return Err("duplicate owner workspace".into());
        }
        if current == registered {
            selected = Some(entry.case_id);
        }
    }
    runtime
        .revalidate(&source.version)
        .map_err(|_| "owner map changed during capture")?;
    selected.ok_or_else(|| "workspace absent from owner case map".into())
}
fn collect(root_path: &Path) -> Checked<Files> {
    let root = open_root(root_path)?;
    let limits = Limits {
        entries: 128,
        depth: 8,
        file_bytes: FILE_LIMIT as u64,
        total_bytes: 1048576,
        skip_generated: false,
    };
    let discovery = root
        .discover(&limits)
        .map_err(|_| "bounded no-follow discovery failed")?;
    if !discovery.complete || !discovery.exclusions.is_empty() {
        return Err("excluded, redirected or oversized workspace input".into());
    }
    let mut result = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for source in &discovery.sources {
        if !safe(&source.version.path) || !folded.insert(source.version.path.to_lowercase()) {
            return Err("unsafe or colliding path".into());
        }
        std::str::from_utf8(&source.bytes).map_err(|_| "workspace input is not UTF-8")?;
        result.insert(source.version.path.clone(), source.bytes.clone());
        root.revalidate(&source.version)
            .map_err(|_| "workspace changed during capture")?;
    }
    let after = root
        .discover(&limits)
        .map_err(|_| "workspace revalidation failed")?;
    if !after.complete
        || !after.exclusions.is_empty()
        || after.sources.len() != discovery.sources.len()
        || after
            .sources
            .iter()
            .zip(&discovery.sources)
            .any(|(a, b)| a.version != b.version)
    {
        return Err("workspace changed during capture".into());
    }
    Ok(result)
}
fn preserved(files: &Files, contract: &Contract) -> Checked<()> {
    for (name, hash) in &contract.sources {
        if contract.editable.contains(name) {
            continue;
        }
        let bytes = files
            .get(name)
            .ok_or_else(|| format!("preserved input missing: {name}"))?;
        if digest_bytes(bytes) != *hash {
            return Err(format!("preserved input changed: {name}"));
        }
    }
    for (name, expected) in &contract.scaffold {
        if files.get(name) != Some(expected) {
            return Err(format!("checker scaffold changed: {name}"));
        }
    }
    Ok(())
}
/// Quoted src/href attribute values, bounded to this corpus. This is asset-path
/// validation, not a DOM, script-safety or accessibility parser.
fn html_targets(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut targets = Vec::new();
    for attribute in ["src", "href"] {
        let mut from = 0;
        while let Some(found) = lower[from..].find(attribute) {
            let start = from + found;
            from = start + attribute.len();
            if start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || bytes[start - 1] == b'-'
                    || bytes[start - 1] == b'_')
            {
                continue;
            }
            let mut index = from;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if bytes.get(index) != Some(&b'=') {
                continue;
            }
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let Some(&quote) = bytes.get(index) else {
                continue;
            };
            if quote != b'"' && quote != b'\'' {
                continue;
            }
            if let Some(end) = text[index + 1..].find(quote as char) {
                targets.push(text[index + 1..index + 1 + end].to_owned());
            }
        }
    }
    targets
}
fn decode(raw: &str) -> Checked<String> {
    let raw = raw.split('#').next().unwrap_or("").as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            let digit = |offset: usize| {
                raw.get(index + offset)
                    .and_then(|value| (*value as char).to_digit(16))
            };
            decoded.push(
                (digit(1).ok_or("invalid asset encoding")? * 16
                    + digit(2).ok_or("invalid asset encoding")?) as u8,
            );
            index += 3;
        } else {
            decoded.push(raw[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "invalid UTF-8 asset encoding".into())
}
fn structure(files: &Files, contract: &Contract) -> Checked<()> {
    for name in files.keys() {
        if !contract.sources.contains_key(name) && !contract.scaffold.contains_key(name) {
            return Err(format!("unexpected workspace file: {name}"));
        }
    }
    let mut total = 0;
    for name in &contract.editable {
        let bytes = files
            .get(name)
            .ok_or_else(|| format!("editable file missing: {name}"))?;
        if bytes.len() > FILE_LIMIT {
            return Err(format!("edited file exceeds limit: {name}"));
        }
        total += bytes.len();
    }
    if total > EDIT_LIMIT {
        return Err("edited files exceed total limit".into());
    }
    for name in &contract.html {
        let text = std::str::from_utf8(
            files
                .get(name)
                .ok_or_else(|| format!("HTML missing: {name}"))?,
        )
        .map_err(|_| "HTML is not UTF-8")?;
        for raw in html_targets(text) {
            let target = decode(&raw)?;
            if target.is_empty() {
                continue;
            }
            if !safe(&target) {
                return Err(format!("unsafe or external HTML asset: {raw}"));
            }
            let resolved = match Path::new(name)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                Some(parent) => format!("{}/{target}", parent.to_string_lossy()),
                None => target.clone(),
            };
            if !safe(&resolved) || !files.contains_key(&resolved) {
                return Err(format!("missing HTML asset: {raw}"));
            }
        }
    }
    Ok(())
}
fn check(root: &Path, expected_case: &str) -> [Checked<()>; 2] {
    let setup = (|| {
        let contract = contract(expected_case)?;
        let files = collect(root)?;
        Ok::<_, String>((contract, files))
    })();
    match setup {
        Ok((contract, files)) => [preserved(&files, &contract), structure(&files, &contract)],
        Err(error) => [Err(error.clone()), Err(error)],
    }
}
fn arguments(args: &[std::ffi::OsString]) -> bool {
    args.len() == ARGUMENTS.len()
        && args
            .iter()
            .zip(ARGUMENTS)
            .all(|(actual, expected)| actual == expected)
}
fn main() -> ExitCode {
    if !arguments(&std::env::args_os().skip(1).collect::<Vec<_>>()) {
        eprintln!("Only the frozen developer verification argument vector is accepted");
        return ExitCode::FAILURE;
    }
    let results = match (std::env::current_dir(), std::env::current_exe()) {
        (Ok(root), Ok(executable)) => match owner_case(&root, &executable) {
            Ok(case) => check(&root, &case),
            Err(error) => [Err(error.clone()), Err(error)],
        },
        _ => [
            Err("owner case binding or workspace unavailable".into()),
            Err("owner case binding or workspace unavailable".into()),
        ],
    };
    println!("TAP version 13");
    println!("# Scope: source preservation and artifact structure only; functional, browser and reader checks are not_run here");
    for (index, result) in results.iter().enumerate() {
        println!(
            "{} {} - {}",
            if result.is_ok() { "ok" } else { "not ok" },
            index + 1,
            TESTS[index]
        );
        if let Err(error) = result {
            eprintln!("developer verifier: {error}");
        }
    }
    let passed = results.iter().filter(|result| result.is_ok()).count();
    println!(
        "1..2\n# tests 2\n# pass {passed}\n# fail {}\n# cancelled 0\n# skipped 0\n# todo 0",
        2 - passed
    );
    if passed == 2 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace(case: &str) -> Files {
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        let task = manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["id"] == case)
            .unwrap();
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/skills/developer")
            .join(task["project"].as_str().unwrap());
        let mut files: Files = task["expected"]["source_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|source| {
                let path = source["path"].as_str().unwrap();
                (path.to_owned(), std::fs::read(project.join(path)).unwrap())
            })
            .collect();
        files.extend(scaffold(case).unwrap());
        files
    }
    #[test]
    fn every_embedded_write_oracle_matches_the_frozen_manifest() {
        for (case, _) in ORACLES {
            let contract = contract(case).unwrap();
            let files = workspace(case);
            assert!(preserved(&files, &contract).is_ok(), "{case}");
            assert!(structure(&files, &contract).is_ok(), "{case}");
        }
        assert!(
            contract("LLM-hostile-diagnostics-v2").is_err(),
            "report-only cases configure no checks"
        );
        assert!(contract("unknown-case").is_err());
    }
    #[test]
    fn edits_are_confined_to_editable_paths_and_scaffold_is_exact() {
        let contract = contract("MCP-normal-tools-v3").unwrap();
        let mut files = workspace("MCP-normal-tools-v3");
        files.insert(
            "server.cjs".into(),
            b"exports.handle = async () => null;\n".to_vec(),
        );
        assert!(preserved(&files, &contract).is_ok() && structure(&files, &contract).is_ok());
        let mut changed = files.clone();
        changed.insert("labels.json".into(), b"[]".to_vec());
        assert!(preserved(&changed, &contract).is_err());
        let mut extra = files.clone();
        extra.insert("notes.md".into(), b"extra".to_vec());
        assert!(structure(&extra, &contract).is_err());
        let mut scaffold_changed = files.clone();
        scaffold_changed.insert(TEST_PATH.into(), b"changed".to_vec());
        assert!(preserved(&scaffold_changed, &contract).is_err());
        let mut missing = files.clone();
        missing.remove("server.cjs");
        assert!(structure(&missing, &contract).is_err());
        let mut oversized = files;
        oversized.insert("server.cjs".into(), vec![b'x'; FILE_LIMIT + 1]);
        assert!(structure(&oversized, &contract).is_err());
    }
    #[test]
    fn html_assets_must_resolve_to_local_workspace_files() {
        let contract = contract("UI-normal-form-v2").unwrap();
        let files = workspace("UI-normal-form-v2");
        assert!(structure(&files, &contract).is_ok());
        for target in [
            "absent.js",
            "https://example.invalid/x.js",
            "%2e%2e/escape.js",
            "folder\\x.js",
            "/rooted.js",
        ] {
            let mut edited = files.clone();
            edited.insert(
                "form.html".into(),
                format!("<script src=\"{target}\"></script>").into_bytes(),
            );
            assert!(structure(&edited, &contract).is_err(), "{target}");
        }
        let mut local = files;
        local.insert("form.html".into(), b"<link rel=\"stylesheet\" href='tokens.css#top'><a HREF = \"#main\">x</a><img data-src=\"ignored\">".to_vec());
        assert!(structure(&local, &contract).is_ok());
    }
    #[test]
    fn only_the_frozen_argument_vector_is_accepted() {
        let exact: Vec<std::ffi::OsString> = ARGUMENTS.iter().map(Into::into).collect();
        assert!(arguments(&exact));
        let mut changed = exact.clone();
        changed[3] = "checks/other.cjs".into();
        assert!(!arguments(&changed));
        assert!(!arguments(&exact[..3]));
    }
}
