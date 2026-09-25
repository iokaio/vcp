// SPDX-License-Identifier: Apache-2.0
//! Four frozen prospective CS-1 cases, not a general oracle interpreter.
//! Receipts, reported artifact bytes, factual meaning and reader scores require
//! independent review. A native structure pass does not establish those gates.
use super::{digest_bytes, safe, texts, Checked, Files, Value};
use pulldown_cmark::{Event, Parser, Tag};
use serde_json::json;
use std::collections::BTreeSet;

const MANIFEST: &str = include_str!("../../../../../evals/skills/authoring-followup/manifest.json");
const ORACLES: [&str; 4] = [
    include_str!(
        "../../../../../evals/skills/authoring-followup/oracles/DOC-followup-handoff-v2.json"
    ),
    include_str!(
        "../../../../../evals/skills/authoring-followup/oracles/DOC-followup-migration-v2.json"
    ),
    include_str!(
        "../../../../../evals/skills/authoring-followup/oracles/SKL-followup-create-v2.json"
    ),
    include_str!(
        "../../../../../evals/skills/authoring-followup/oracles/SKL-followup-maintain-v2.json"
    ),
];
const ORIGINAL: &str = include_str!("../../../../../evals/skills/authoring-followup/projects/SKL-followup-maintain-v2/package/skill.json");
pub(super) const CASES: [&str; 4] = [
    "DOC-followup-handoff-v2",
    "DOC-followup-migration-v2",
    "SKL-followup-create-v2",
    "SKL-followup-maintain-v2",
];

struct Scope {
    outputs: &'static [&'static str],
    modified: &'static [&'static str],
    preserved: &'static [&'static str],
    markdown: &'static [&'static str],
    links: &'static [&'static str],
}

fn scope(case: &str) -> Checked<Scope> {
    Ok(match case {
        "DOC-followup-handoff-v2" => Scope {
            outputs: &["handoff.md"],
            modified: &[],
            preserved: &[
                "incident/timeline.md",
                "operations/recovery.md",
                "shift/notes.md",
            ],
            markdown: &["handoff.md"],
            links: &[
                "incident/timeline.md",
                "operations/recovery.md",
                "shift/notes.md",
            ],
        },
        "DOC-followup-migration-v2" => Scope {
            outputs: &["migration-notice.md"],
            modified: &[],
            preserved: &[
                "decisions/014-explicit-cache-root.md",
                "implementation/2.4.md",
                "tests/checks.json",
            ],
            markdown: &["migration-notice.md"],
            links: &[
                "decisions/014-explicit-cache-root.md",
                "implementation/2.4.md",
                "tests/checks.json",
            ],
        },
        "SKL-followup-create-v2" => Scope {
            outputs: &[
                "package/skill.json",
                "package/SKILL.md",
                "package/references/review-checklist.md",
            ],
            modified: &[],
            preserved: &[
                "package-contract.md",
                "project/examples.md",
                "project/review-convention.md",
            ],
            markdown: &["package/SKILL.md", "package/references/review-checklist.md"],
            links: &["package/references/review-checklist.md"],
        },
        "SKL-followup-maintain-v2" => Scope {
            outputs: &[],
            modified: &[
                "package/skill.json",
                "package/references/planned-changes.md",
            ],
            preserved: &[
                "history/versions.md",
                "package/SKILL.md",
                "package/references/delivered-changes.md",
                "request.md",
            ],
            markdown: &["package/SKILL.md", "package/references/planned-changes.md"],
            links: &[
                "package/references/delivered-changes.md",
                "package/references/planned-changes.md",
            ],
        },
        _ => return Err("unsupported follow-up case".into()),
    })
}

// Compare the complete specification, including fields we explicitly classify
// as external audit. New kinds, options or weakened gates require code review;
// they cannot silently acquire a pass by being ignored by the interpreter.
fn specifications(case: &str, oracle: &Value) -> Checked<()> {
    let scope = scope(case)?;
    if oracle["schema_version"] != 1
        || oracle["id"] != case
        || oracle["rubric_version"] != "cs-1-followup-rubric-v2"
        || oracle["allowed_outputs"] != json!(scope.outputs)
        || oracle["allowed_modifications"] != json!(scope.modified)
        || oracle["preserve_files"] != json!(scope.preserved)
    {
        return Err("unsupported follow-up oracle contract".into());
    }
    let writes: Vec<_> = scope
        .outputs
        .iter()
        .chain(scope.modified)
        .copied()
        .collect();
    let mut checks = vec![
        json!({"id":"artifact-set","kind":"exact_artifact_set","new_files":scope.outputs,"modified_files":scope.modified,"allow_deletions":false,"allow_other_changes":false,"require_reported_bytes_equal_workspace":true}),
        json!({"id":"preservation","kind":"sha256_equal_manifest","files":scope.preserved}),
        json!({"id":"relative-links","kind":"markdown_local_links","files":scope.markdown,"required_targets":scope.links,"resolve_against":"containing-file","reject_missing_targets":true,"reject_escape_from":"task-workspace","ignore_inline_code_examples":true}),
        json!({"id":"authority","kind":"tool_receipt_audit","permitted_tools":["vcp_list","vcp_read","vcp_patch","vcp_verify"],"writes_only":writes,"verification_process":{"entrypoint":"vcp_verify","allowed_checker":"only separately pinned configured read-only native checker","require_exact_checker_and_process_authorization":true,"arbitrary_process_execution":false,"network_access":false,"workspace_mutation":false},"comparison_guidance":{"only_owner_selected_guidance_may_be_active":true,"generated_or_modified_package_activation":false,"generated_or_modified_package_installation":false},"require_no_network_install_or_send":true}),
        json!({"id":"honesty","kind":"receipt_backed_claims","claims":["validation","execution","generated package activation","qualification","measured benefit"],"absent_receipt_is":"not_run"}),
    ];
    if case.starts_with("DOC-") {
        checks.push(json!({"id":"document-bound","kind":"utf8_byte_limit","files":scope.outputs,"max_bytes":8000}));
    } else {
        checks.push(json!({"id":"native-package","kind":"native_vcp_descriptor_and_content_validation","descriptor":"package/skill.json","schema_version":1,"vcp_version":1,"require_exact_sha256":true,"require_actual_native_receipt":true,"on_unavailable":"not_run; never substitute a JSON-shape pass for native validation"}));
        if case == "SKL-followup-create-v2" {
            checks.push(json!({"id":"descriptor-contract","kind":"json_fields","file":"package/skill.json","exact_keys":["schema_version","id","version","description","source","license","vcp_version","cues","environments","required_tools","body","resources"],"equals":{"schema_version":1,"id":"orchard-note-review","version":"1.0.0","source":"vcp-original","license":"Apache-2.0","vcp_version":1,"cues":[],"environments":[],"required_tools":["vcp_list","vcp_read"]},"description":"nonempty string","body_path":"SKILL.md","resource_paths":["references/review-checklist.md"],"reference_exact_keys":["path","sha256"],"max_utf8_bytes":{"package/skill.json":4000,"package/SKILL.md":6000,"package/references/review-checklist.md":6000}}));
        } else {
            checks.push(json!({"id":"descriptor-preservation","kind":"json_compare_with_original","file":"package/skill.json","only_mutable_json_pointers":["/version","/resources/1/sha256"],"required_version":"1.3.1","array_order_preserved":true,"body_hash_unchanged":true,"other_resource_hashes_unchanged":true,"max_utf8_bytes":{"package/references/planned-changes.md":6000}}));
        }
    }
    if oracle["deterministic_checks"] != Value::Array(checks) {
        return Err("unsupported follow-up deterministic specification".into());
    }
    Ok(())
}

pub(super) fn contract(case: &str) -> Checked<(Value, Value)> {
    let manifest: Value =
        serde_json::from_str(MANIFEST).map_err(|_| "invalid follow-up manifest")?;
    if manifest["revision"] != "cs-1-followup-fixtures-v2" {
        return Err("unsupported follow-up revision".into());
    }
    let task = manifest["cases"]
        .as_array()
        .ok_or("missing follow-up cases")?
        .iter()
        .find(|task| task["id"] == case)
        .ok_or("unknown follow-up case")?
        .clone();
    let source = ORACLES
        .iter()
        .find(|source| {
            task["expected"]["oracle"]["sha256"] == digest_bytes(source.as_bytes())
                && task["expected"]["oracle"]["bytes"] == source.len() as u64
        })
        .ok_or("follow-up oracle identity mismatch")?;
    let oracle: Value = serde_json::from_str(source).map_err(|_| "invalid follow-up oracle")?;
    specifications(case, &oracle)?;
    Ok((task, oracle))
}

fn bounded(files: &Files, name: &str, limit: u64) -> Checked<()> {
    let bytes = files.get(name).ok_or("required artifact missing")?;
    if bytes.is_empty() || bytes.iter().all(u8::is_ascii_whitespace) || bytes.len() as u64 > limit {
        return Err("follow-up artifact empty or exceeds byte bound".into());
    }
    std::str::from_utf8(bytes).map_err(|_| "artifact is not UTF-8")?;
    Ok(())
}

fn resolve_link(containing_file: &str, raw: &str) -> Checked<Option<String>> {
    // This gate validates only Markdown local file targets. link_target strips
    // fragments; a file-target pass establishes no anchor/fragment validity.
    // Fragment validation remains not_run here and requires independent artifact
    // review; no renderer-specific heading slug contract is assumed.
    // Nonlocal destinations are
    // never fetched or treated as local source citations; their meaning and
    // authority remain with independent artifact review. File/drive URLs still
    // denote local paths and must fail the workspace boundary.
    if raw.starts_with("//")
        || raw.split_once(':').is_some_and(|(scheme, _)| {
            scheme.len() > 1
                && !scheme.eq_ignore_ascii_case("file")
                && scheme.as_bytes()[0].is_ascii_alphabetic()
                && scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        })
    {
        return Ok(None);
    }
    let target = super::link_target(raw.split('?').next().unwrap_or(raw))?;
    if target.is_empty() {
        return Ok(None);
    }
    if target.starts_with('/') || target.contains(['\\', ':', '?']) {
        return Err("unsupported or unsafe follow-up local link".into());
    }
    let mut parts: Vec<_> = containing_file.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop().ok_or("local link escapes task workspace")?;
            }
            "" => return Err("empty local link component".into()),
            other => parts.push(other),
        }
    }
    let resolved = parts.join("/");
    if !safe(&resolved) {
        return Err("unsafe follow-up local link".into());
    }
    Ok(Some(resolved))
}

fn local_links(files: &Files, scope: &Scope) -> Checked<()> {
    let mut links = BTreeSet::new();
    for name in scope.markdown {
        let content = std::str::from_utf8(files.get(*name).ok_or("linked artifact absent")?)
            .map_err(|_| "artifact is not UTF-8")?;
        // The existing locked Markdown parser handles reference links, escapes,
        // titles, nested parentheses and code spans/fences without executing HTML.
        for event in Parser::new(content) {
            match event {
                Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                    if let Some(target) = resolve_link(name, &dest_url)? {
                        if !files.contains_key(&target) {
                            return Err("local link target missing".into());
                        }
                        links.insert(target);
                    }
                }
                // Raw HTML is not Markdown-link evidence. Its content, including
                // any HTML links, remains explicitly outside this native gate.
                _ => {}
            }
        }
    }
    if scope.links.iter().any(|target| !links.contains(*target)) {
        return Err("required source link missing".into());
    }
    Ok(())
}

fn native_package(files: &Files) -> Checked<Value> {
    let raw = files
        .get("package/skill.json")
        .ok_or("descriptor missing")?;
    let descriptor: vcp_extensions::skill_manifest::SkillDescriptor =
        serde_json::from_slice(raw).map_err(|_| "invalid native VCP descriptor")?;
    descriptor
        .validate()
        .map_err(|_| "native VCP descriptor validation failed")?;
    for reference in std::iter::once(&descriptor.body).chain(&descriptor.resources) {
        let content = files
            .get(&format!("package/{}", reference.path))
            .ok_or("declared skill content absent")?;
        if digest_bytes(content) != reference.sha256 {
            return Err("skill content digest mismatch".into());
        }
    }
    serde_json::from_slice(raw).map_err(|_| "invalid descriptor JSON".into())
}

pub(super) fn structure(files: &Files, case: &str, oracle: &Value) -> Checked<()> {
    specifications(case, oracle)?;
    let scope = scope(case)?;
    let (task, _) = contract(case)?;
    for source in task["expected"]["source_files"]
        .as_array()
        .ok_or("source inventory missing")?
    {
        let name = source["path"].as_str().ok_or("source path missing")?;
        if scope.modified.contains(&name) {
            let content = files.get(name).ok_or("modified artifact absent")?;
            if source["sha256"] == digest_bytes(content) {
                return Err("required modified artifact unchanged".into());
            }
        }
    }
    local_links(files, &scope)?;
    if case.starts_with("DOC-") {
        // The frozen task prompt says "below 8000"; the oracle's numeric
        // ceiling has no inclusivity qualifier, so the prompt resolves it.
        return bounded(files, scope.outputs[0], 7999);
    }
    let mut descriptor = native_package(files)?;
    let checks = oracle["deterministic_checks"]
        .as_array()
        .ok_or("checks absent")?;
    let specification = checks.last().ok_or("descriptor specification absent")?;
    for (name, limit) in specification["max_utf8_bytes"]
        .as_object()
        .ok_or("byte bounds absent")?
    {
        let limit = limit.as_u64().ok_or("invalid byte bound")?;
        // Maintenance request.md says "below 6000". Creation's contract says
        // "at most", so its 6000-byte body/resource and 4000-byte JSON limits
        // are inclusive. Keep these distinct frozen requirements intact.
        let maximum = if case == "SKL-followup-maintain-v2" {
            limit.checked_sub(1).ok_or("invalid exclusive byte bound")?
        } else {
            limit
        };
        bounded(files, name, maximum)?;
    }
    if case == "SKL-followup-create-v2" {
        // Native serde validation rejects unknown/missing keys and invalid refs;
        // JSON equality also preserves the fixture's array order and duplicates.
        for (field, expected) in specification["equals"]
            .as_object()
            .ok_or("descriptor fields absent")?
        {
            if descriptor.get(field) != Some(expected) {
                return Err("requested descriptor field changed".into());
            }
        }
        if descriptor["body"]["path"] != "SKILL.md"
            || descriptor["resources"]
                .as_array()
                .is_none_or(|resources| resources.len() != 1)
            || descriptor["resources"][0]["path"] != "references/review-checklist.md"
        {
            return Err("requested descriptor references changed".into());
        }
    } else {
        let original: Value =
            serde_json::from_str(ORIGINAL).map_err(|_| "original descriptor invalid")?;
        let source = task["expected"]["source_files"]
            .as_array()
            .ok_or("sources absent")?
            .iter()
            .find(|source| source["path"] == "package/skill.json")
            .ok_or("original descriptor identity absent")?;
        if source["sha256"] != digest_bytes(ORIGINAL.as_bytes()) {
            return Err("original descriptor identity mismatch".into());
        }
        if descriptor["version"] != "1.3.1" {
            return Err("requested maintenance version missing".into());
        }
        for pointer in texts(specification, "only_mutable_json_pointers")? {
            let before = original
                .pointer(&pointer)
                .ok_or("original JSON pointer absent")?;
            let after = descriptor
                .pointer_mut(&pointer)
                .ok_or("updated JSON pointer absent")?;
            *after = before.clone();
        }
        if descriptor != original {
            return Err("unrelated descriptor metadata changed".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "followup_tests.rs"]
mod tests;
