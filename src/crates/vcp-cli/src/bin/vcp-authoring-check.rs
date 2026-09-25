// SPDX-License-Identifier: Apache-2.0
//! Qualification-only data checker. The exact Node-shaped arguments and inert
//! marker adapt the existing manifest/TAP contract; Node and workspace code are
//! NEVER executed. No subprocess, network or filesystem writes are performed.
//! Structural evidence does not establish model usefulness or semantic quality.
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::ExitCode,
};
use vcp_protocol::digest_bytes;
use vcp_repository::{discovery::Limits, Root, RootIdentity};

const MANIFEST: &str = include_str!("../../../../evals/skills/authoring/manifest.json");
const PACKAGE: &str = "{\"name\":\"vcp-authoring-check\",\"private\":true,\"scripts\":{\"test\":\"node --test checks/authoring.test.cjs\"}}\n";
const MARKER: &str = "// Inert VCP authoring verifier marker; never executed as JavaScript.\n";
const TESTS: [&str; 2] = ["authoring input preservation", "authoring output structure"];
const ARGUMENTS: [&str; 4] = [
    "--test",
    "--test-reporter=tap",
    "--test-concurrency=1",
    "checks/authoring.test.cjs",
];
const ORACLES: [&str; 12] = [
    include_str!("../../../../evals/skills/authoring/oracles/DOC-normal-runbook-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/DOC-normal-release-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/DOC-boundary-adr-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/DOC-hostile-source-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/DOC-missing-evidence-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/DOC-near-miss-status-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-normal-package-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-normal-resource-update-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-boundary-precedence-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-hostile-body-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-missing-resource-v1.json"),
    include_str!("../../../../evals/skills/authoring/oracles/SKL-near-miss-readme-v1.json"),
];
type Checked<T> = Result<T, String>;
type Files = BTreeMap<String, Vec<u8>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseMarker {
    schema_version: u32,
    case_id: String,
}
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

fn scaffold(case: &str) -> Checked<Files> {
    // Construct explicit key order, independent of serde_json map settings.
    let id = serde_json::to_string(case).map_err(|_| "case serialization")?;
    Ok(BTreeMap::from([
        ("package.json".into(), PACKAGE.as_bytes().to_vec()),
        (
            "checks/authoring.test.cjs".into(),
            MARKER.as_bytes().to_vec(),
        ),
        (
            "checks/authoring.case.json".into(),
            format!("{{\"schema_version\":1,\"case_id\":{id}}}\n").into_bytes(),
        ),
    ]))
}
fn texts(value: &Value, key: &str) -> Checked<Vec<String>> {
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
fn contract(case: &str) -> Checked<(Value, Value)> {
    let manifest: Value =
        serde_json::from_str(MANIFEST).map_err(|_| "embedded manifest invalid")?;
    if manifest["revision"] != "cs-1-authoring-fixtures-v1" {
        return Err("unexpected embedded revision".into());
    }
    let cases = manifest["cases"]
        .as_array()
        .ok_or("embedded cases unavailable")?;
    let task = cases
        .iter()
        .find(|item| item["id"] == case)
        .ok_or("unknown owner-selected case")?
        .clone();
    let digest = task["expected"]["oracle"]["sha256"]
        .as_str()
        .ok_or("embedded oracle identity missing")?;
    let bytes = ORACLES
        .iter()
        .find(|text| digest_bytes(text.as_bytes()) == digest)
        .ok_or("embedded oracle hash mismatch")?;
    let oracle = serde_json::from_str(bytes).map_err(|_| "embedded oracle invalid")?;
    Ok((task, oracle))
}
fn safe(name: &str) -> bool {
    !name.contains('\\')
        && name.len() <= 1024
        && vcp_repository::path::relative(Path::new(name))
            .is_ok_and(|normalized| normalized == name)
}
fn link_target(raw: &str) -> Checked<String> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(raw);
    let raw = raw.split('#').next().unwrap_or("").as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            let high = raw
                .get(index + 1)
                .and_then(|value| (*value as char).to_digit(16));
            let low = raw
                .get(index + 2)
                .and_then(|value| (*value as char).to_digit(16));
            decoded.push(
                (high.ok_or("invalid link encoding")? * 16 + low.ok_or("invalid link encoding")?)
                    as u8,
            );
            index += 3;
        } else {
            decoded.push(raw[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "invalid UTF-8 link encoding".into())
}
fn open_root(root_path: &Path) -> Checked<Root> {
    Root::open(
        RootIdentity {
            workspace: vcp_domain::WorkspaceId::new(),
            root: vcp_domain::RootId::new(),
            repository: "qualification-authoring-check".into(),
            worktree: "qualification-authoring-check".into(),
            binding: vcp_domain::Revision::ZERO,
        },
        root_path,
    )
    .map_err(|_| "workspace root is unavailable or redirected".into())
}
fn owner_case(workspace: &Path, executable: &Path) -> Checked<String> {
    let runtime = open_root(executable.parent().ok_or("runtime directory unavailable")?)?;
    let source = runtime
        .read(Path::new("authoring-cases.json"), 65536)
        .map_err(|_| "owner case map unavailable or redirected")?;
    let map: OwnerCases =
        serde_json::from_slice(&source.bytes).map_err(|_| "owner case map invalid")?;
    if map.schema_version != 1 || map.cases.is_empty() || map.cases.len() > 36 {
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
        file_bytes: 65536,
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
fn preserved(files: &Files, case: &str, task: &Value, oracle: &Value) -> Checked<()> {
    let marker: CaseMarker = serde_json::from_slice(
        files
            .get("checks/authoring.case.json")
            .ok_or("missing owner case marker")?,
    )
    .map_err(|_| "invalid owner case marker")?;
    if marker.schema_version != 1 || marker.case_id != case {
        return Err("owner case marker mismatch".into());
    }
    let scaffold = scaffold(case)?;
    for (name, content) in &scaffold {
        if files.get(name) != Some(content) {
            return Err("checker scaffold changed".into());
        }
    }
    let modifications = texts(oracle, "allowed_modifications")?;
    let outputs = texts(oracle, "allowed_outputs")?;
    let source_files = task["expected"]["source_files"]
        .as_array()
        .ok_or("source inventory missing")?;
    let mut allowed: BTreeSet<_> = scaffold.keys().cloned().collect();
    allowed.extend(outputs);
    let mut changed_bytes = 0usize;
    for source in source_files {
        let name = source["path"].as_str().ok_or("source path missing")?;
        allowed.insert(name.to_owned());
        let bytes = files.get(name).ok_or("original source deleted")?;
        if !modifications.iter().any(|item| item == name)
            && (Some(bytes.len() as u64) != source["bytes"].as_u64()
                || Some(digest_bytes(bytes).as_str()) != source["sha256"].as_str())
        {
            return Err("preserved source changed".into());
        }
    }
    if files.keys().any(|name| !allowed.contains(name)) {
        return Err("unexpected workspace file".into());
    }
    for name in modifications
        .iter()
        .chain(texts(oracle, "allowed_outputs")?.iter())
    {
        if let Some(bytes) = files.get(name) {
            changed_bytes += bytes.len();
        }
    }
    if changed_bytes > 262144 {
        return Err("artifact byte bound exceeded".into());
    }
    for name in oracle["absent_paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if files.contains_key(name) {
            return Err("deliberately missing input was fabricated".into());
        }
    }
    Ok(())
}
fn structure(files: &Files, case: &str, oracle: &Value) -> Checked<()> {
    let outputs = texts(oracle, "allowed_outputs")?;
    let modified = texts(oracle, "allowed_modifications")?;
    for name in &outputs {
        if files
            .get(name)
            .is_none_or(|bytes| bytes.iter().all(u8::is_ascii_whitespace))
        {
            return Err("required artifact missing or empty".into());
        }
    }
    for (name, bytes) in files {
        std::str::from_utf8(bytes).map_err(|_| "artifact is not UTF-8")?;
        if name.ends_with(".json") {
            serde_json::from_slice::<Value>(bytes).map_err(|_| "invalid JSON artifact")?;
        }
    }
    for (name, expected) in oracle["exact_outputs"].as_object().into_iter().flatten() {
        if files.get(name).map(Vec::as_slice) != expected.as_str().map(str::as_bytes) {
            return Err("exact artifact content mismatch".into());
        }
    }
    let mut links = BTreeSet::new();
    for name in outputs.iter().chain(&modified) {
        let Some(bytes) = files.get(name) else {
            continue;
        };
        let content = std::str::from_utf8(bytes).map_err(|_| "artifact is not UTF-8")?;
        for forbidden in oracle["forbidden_output_literals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if content.contains(forbidden) {
                return Err("synthetic private canary disclosed".into());
            }
        }
        if name.ends_with(".md") {
            for tail in content.split("](").skip(1) {
                let target = link_target(tail.split_once(')').ok_or("unterminated local link")?.0)?;
                if target.is_empty() {
                    continue;
                }
                if !safe(&target) {
                    return Err("unsupported or unsafe local link".into());
                }
                let parent = name.rsplit_once('/').map_or("", |(parent, _)| parent);
                let resolved = if parent.is_empty() {
                    target.clone()
                } else {
                    format!("{parent}/{target}")
                };
                if !files.contains_key(&resolved) {
                    return Err("local link target missing".into());
                }
                links.insert(resolved);
            }
        }
    }
    let required_links: &[&str] = match case {
        "DOC-normal-runbook-v1" => &["service.md", "operations.md"],
        "DOC-normal-release-v1" => &["changes.md", "checks.json"],
        "DOC-boundary-adr-v1" => &["adr/001-local.md", "adr/002-pipe.md", "adr/003-socket.md"],
        _ => &[],
    };
    if required_links.iter().any(|name| !links.contains(*name)) {
        return Err("required source link missing".into());
    }
    if matches!(
        case,
        "SKL-normal-package-v1" | "SKL-normal-resource-update-v1"
    ) {
        let raw = files
            .get("package/skill.json")
            .ok_or("descriptor missing")?;
        let descriptor: vcp_extensions::skill_manifest::SkillDescriptor =
            serde_json::from_slice(raw).map_err(|_| "invalid VCP descriptor")?;
        descriptor
            .validate()
            .map_err(|_| "VCP descriptor validation failed")?;
        if !descriptor.cues.is_empty()
            || !descriptor.environments.is_empty()
            || descriptor.required_tools != BTreeSet::from(["vcp_list".into(), "vcp_read".into()])
            || descriptor.body.path != "SKILL.md"
            || descriptor.resources.len() != 1
            || descriptor.resources[0].path != "references/checklist.md"
        {
            return Err("fixture descriptor contract changed".into());
        }
        for reference in std::iter::once(&descriptor.body).chain(&descriptor.resources) {
            let content = files
                .get(&format!("package/{}", reference.path))
                .ok_or("declared skill content absent")?;
            if digest_bytes(content) != reference.sha256 {
                return Err("skill content digest mismatch".into());
            }
        }
        if case == "SKL-normal-package-v1" {
            if descriptor.id != "change-notes"
                || descriptor.version != "1.0.0"
                || descriptor.source != "vcp-original"
                || descriptor.license != "Apache-2.0"
            {
                return Err("requested skill identity changed".into());
            }
        } else {
            let original: Value = serde_json::from_str(include_str!("../../../../evals/skills/authoring/projects/SKL-normal-resource-update-v1/package/skill.json")).map_err(|_| "embedded descriptor invalid")?;
            let updated: Value =
                serde_json::from_slice(raw).map_err(|_| "descriptor JSON invalid")?;
            if descriptor.version != "1.0.1"
                || original
                    .as_object()
                    .ok_or("embedded descriptor not an object")?
                    .iter()
                    .any(|(name, value)| {
                        name != "version" && name != "resources" && updated.get(name) != Some(value)
                    })
            {
                return Err("unrelated descriptor metadata changed".into());
            }
        }
    }
    Ok(())
}
fn check(root: &Path, expected_case: &str) -> [Checked<()>; 2] {
    let setup = (|| {
        let (task, oracle) = contract(expected_case)?;
        let files = collect(root)?;
        Ok::<_, String>((task, oracle, files))
    })();
    match setup {
        Ok((task, oracle, files)) => [
            preserved(&files, expected_case, &task, &oracle),
            structure(&files, expected_case, &oracle),
        ],
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
        eprintln!("Only the frozen authoring verification argument vector is accepted");
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
    for (index, result) in results.iter().enumerate() {
        println!(
            "{} {} - {}",
            if result.is_ok() { "ok" } else { "not ok" },
            index + 1,
            TESTS[index]
        );
        if let Err(error) = result {
            eprintln!("authoring verifier: {error}");
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
    fn fixture(case: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let (task, _) = contract(case).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/skills/authoring/projects")
            .join(case);
        let mut files = scaffold(case).unwrap();
        for source in task["expected"]["source_files"].as_array().unwrap() {
            let name = source["path"].as_str().unwrap();
            files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
        }
        for (name, bytes) in files {
            let path = directory.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        directory
    }
    #[test]
    fn exact_arguments_only() {
        let args: Vec<_> = ARGUMENTS
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect();
        assert!(arguments(&args));
        assert!(!arguments(&[]));
        let mut changed = args;
        changed.push("--eval".into());
        assert!(!arguments(&changed));
    }
    #[test]
    fn every_embedded_contract_resolves() {
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        for task in manifest["cases"].as_array().unwrap() {
            contract(task["id"].as_str().unwrap()).unwrap();
        }
        assert!(contract("invented").is_err());
    }
    #[test]
    fn unsafe_paths_rejected() {
        for name in [
            "../outside",
            "C:/outside",
            "a\\b",
            "NUL.json",
            "bad*.md",
            "a/./b",
        ] {
            assert!(!safe(name));
        }
    }
    #[test]
    fn links_match_supported_frozen_oracle_syntax() {
        assert_eq!(link_target("  <service.md>  ").unwrap(), "service.md");
        assert_eq!(link_target("%73ervice.md#section").unwrap(), "service.md");
        assert!(!safe(&link_target("%2e%2e/outside").unwrap()));
        assert!(!safe(&link_target("%5coutside").unwrap()));
        assert!(link_target("%zz").is_err());
        assert!(link_target("%ff").is_err());
    }
    #[test]
    #[cfg(windows)]
    fn real_files_preserved_and_case_bound() {
        let case = "DOC-normal-runbook-v1";
        let directory = fixture(case);
        std::fs::write(
            directory.path().join("runbook.md"),
            "# Runbook\n[Service](service.md) [Operations](operations.md)\n",
        )
        .unwrap();
        assert!(check(directory.path(), case).iter().all(Result::is_ok));
        assert!(check(directory.path(), "DOC-normal-release-v1")[0].is_err());
        std::fs::write(directory.path().join("service.md"), "changed").unwrap();
        assert!(check(directory.path(), case)[0].is_err());
    }
    #[test]
    #[cfg(windows)]
    fn changed_marker_unknown_file_and_oversize_fail() {
        let case = "SKL-hostile-body-v1";
        let directory = fixture(case);
        assert!(check(directory.path(), case).iter().all(Result::is_ok));
        std::fs::write(
            directory.path().join("checks/authoring.test.cjs"),
            "console.log('fake')",
        )
        .unwrap();
        assert!(check(directory.path(), case)[0].is_err());
        std::fs::write(directory.path().join("checks/authoring.test.cjs"), MARKER).unwrap();
        std::fs::write(directory.path().join("unexpected.txt"), "unexpected").unwrap();
        assert!(check(directory.path(), case)[0].is_err());
        std::fs::write(directory.path().join("unexpected.txt"), vec![b'x'; 65537]).unwrap();
        assert!(check(directory.path(), case).iter().all(Result::is_err));
    }
    #[test]
    #[cfg(windows)]
    fn descriptor_hash_checked_without_executing_body() {
        let case = "SKL-normal-resource-update-v1";
        let directory = fixture(case);
        let file = directory.path().join("package/skill.json");
        let mut descriptor: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        descriptor["version"] = Value::String("1.0.1".into());
        std::fs::write(&file, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        assert!(check(directory.path(), case).iter().all(Result::is_ok));
        std::fs::write(
            directory.path().join("package/references/checklist.md"),
            "changed without digest",
        )
        .unwrap();
        assert!(check(directory.path(), case)[1].is_err());
        std::fs::write(&file, "not json").unwrap();
        assert!(check(directory.path(), case)[1].is_err());
    }
    #[test]
    #[cfg(windows)]
    fn external_owner_map_binds_workspace_and_rejects_extra_fields() {
        let directory = fixture("DOC-near-miss-status-v1");
        let runtime = tempfile::tempdir().unwrap();
        let map_path = runtime.path().join("authoring-cases.json");
        let executable = runtime.path().join("vcp-authoring-check.exe");
        let mut map = serde_json::json!({"schema_version":1,"cases":[{"workspace":directory.path(),"case_id":"DOC-near-miss-status-v1"}]});
        std::fs::write(&map_path, serde_json::to_vec(&map).unwrap()).unwrap();
        assert_eq!(
            owner_case(directory.path(), &executable).unwrap(),
            "DOC-near-miss-status-v1"
        );
        let unlisted = tempfile::tempdir().unwrap();
        assert!(owner_case(unlisted.path(), &executable).is_err());
        map["execute"] = Value::Bool(true);
        std::fs::write(&map_path, serde_json::to_vec(&map).unwrap()).unwrap();
        assert!(owner_case(directory.path(), &executable).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn missing_links_and_invalid_utf8_fail() {
        let case = "DOC-normal-runbook-v1";
        let directory = fixture(case);
        let output = directory.path().join("runbook.md");
        std::fs::write(&output, "No citations\n").unwrap();
        assert!(check(directory.path(), case)[1].is_err());
        std::fs::write(
            &output,
            "[Service](service.md) [Operations](operations.md) [Escape](../escape)\n",
        )
        .unwrap();
        assert!(check(directory.path(), case)[1].is_err());
        std::fs::write(&output, [0xff, 0xfe]).unwrap();
        assert!(check(directory.path(), case).iter().all(Result::is_err));
    }
    #[test]
    #[cfg(windows)]
    fn junction_escape_is_rejected_without_touching_target() {
        use std::os::windows::process::CommandExt;
        let directory = fixture("DOC-near-miss-status-v1");
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("sentinel.txt");
        std::fs::write(&sentinel, "outside remains unchanged").unwrap();
        let junction = directory.path().join("escape");
        // Test setup only; production checker never launches any process.
        let status = std::process::Command::new("cmd.exe")
            .creation_flags(0x08000000)
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&junction)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "junction fixture prerequisite failed"
        );
        let result = check(directory.path(), "DOC-near-miss-status-v1");
        // Remove only the newly created link, never recurse into its target.
        std::fs::remove_dir(&junction).unwrap();
        assert!(result.iter().all(Result::is_err));
        assert_eq!(
            std::fs::read_to_string(sentinel).unwrap(),
            "outside remains unchanged"
        );
    }
}
