// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{check, preserved, scaffold};
use std::path::Path;

fn fixture(case: &str) -> Files {
    let (task, _) = contract(case).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/skills/authoring-followup/projects")
        .join(case);
    let mut files = scaffold(case).unwrap();
    for source in task["expected"]["source_files"].as_array().unwrap() {
        let name = source["path"].as_str().unwrap();
        files.insert(name.into(), std::fs::read(root.join(name)).unwrap());
    }
    files
}

fn descriptor(files: &Files) -> Value {
    serde_json::from_slice(&files["package/skill.json"]).unwrap()
}

fn put_descriptor(files: &mut Files, value: &Value) {
    files.insert(
        "package/skill.json".into(),
        serde_json::to_vec(value).unwrap(),
    );
}

// Intentionally content-free trusted examples. Passing structure must not be
// mistaken for factual correctness, task usefulness or a model-run receipt.
fn artifact(case: &str) -> Files {
    let mut files = fixture(case);
    match case {
        "DOC-followup-handoff-v2" | "DOC-followup-migration-v2" => {
            let scope = scope(case).unwrap();
            let content = scope
                .links
                .iter()
                .map(|target| format!("[Source]({target})\n"))
                .collect::<String>();
            files.insert(scope.outputs[0].into(), content.into_bytes());
        }
        "SKL-followup-create-v2" => {
            let body =
                b"# Trusted structural example\n[Checklist](references/review-checklist.md)\n";
            let resource = b"# Trusted resource\nNo factual or quality score is implied.\n";
            files.insert("package/SKILL.md".into(), body.to_vec());
            files.insert(
                "package/references/review-checklist.md".into(),
                resource.to_vec(),
            );
            put_descriptor(
                &mut files,
                &json!({
                    "schema_version":1,"id":"orchard-note-review","version":"1.0.0",
                    "description":"Trusted structure test only.","source":"vcp-original","license":"Apache-2.0",
                    "vcp_version":1,"cues":[],"environments":[],"required_tools":["vcp_list","vcp_read"],
                    "body":{"path":"SKILL.md","sha256":digest_bytes(body)},
                    "resources":[{"path":"references/review-checklist.md","sha256":digest_bytes(resource)}]
                }),
            );
        }
        "SKL-followup-maintain-v2" => {
            let content = b"# Trusted replacement\nStructural testing only.\n";
            files.insert(
                "package/references/planned-changes.md".into(),
                content.to_vec(),
            );
            let mut updated = descriptor(&files);
            updated["version"] = json!("1.3.1");
            updated["resources"][1]["sha256"] = json!(digest_bytes(content));
            put_descriptor(&mut files, &updated);
        }
        _ => panic!("unknown trusted example"),
    }
    files
}

fn verify(case: &str, files: &Files) -> [Checked<()>; 2] {
    let (task, oracle) = contract(case).unwrap();
    [
        preserved(files, case, &task, &oracle),
        crate::structure(files, case, &oracle),
    ]
}

#[test]
fn four_contracts_and_trusted_structures_resolve_without_semantic_claims() {
    for case in CASES {
        let files = artifact(case);
        let result = verify(case, &files);
        assert!(result.iter().all(Result::is_ok), "{case}: {result:?}");
    }
    assert!(contract("DOC-followup-handoff-v1").is_err());
    assert!(contract("invented").is_err());
}

#[test]
fn unsupported_specifications_are_not_silently_passed() {
    for case in CASES {
        let (_, oracle) = contract(case).unwrap();
        for index in 0..oracle["deterministic_checks"].as_array().unwrap().len() {
            let mut changed = oracle.clone();
            changed["deterministic_checks"][index]["unsupported_option"] = json!(true);
            assert!(specifications(case, &changed).is_err());
        }
        let mut changed = oracle.clone();
        changed["deterministic_checks"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"invented"}));
        assert!(specifications(case, &changed).is_err());
        changed = oracle.clone();
        changed["deterministic_checks"]
            .as_array_mut()
            .unwrap()
            .remove(3);
        assert!(specifications(case, &changed).is_err());
        changed = oracle;
        changed["deterministic_checks"][2]["reject_missing_targets"] = json!(false);
        assert!(specifications(case, &changed).is_err());
    }
}

#[test]
fn every_source_scaffold_output_and_extra_file_is_accounted_for() {
    for case in CASES {
        let original = artifact(case);
        for name in fixture(case).keys() {
            if scope(case).unwrap().modified.contains(&name.as_str()) {
                continue;
            }
            let mut files = original.clone();
            files.get_mut(name).unwrap().push(b' ');
            assert!(verify(case, &files)[0].is_err(), "changed {case}/{name}");
            files.remove(name);
            assert!(verify(case, &files)[0].is_err(), "deleted {case}/{name}");
        }
        for name in scope(case).unwrap().outputs {
            let mut files = original.clone();
            files.remove(*name);
            assert!(verify(case, &files)[1].is_err(), "absent {case}/{name}");
        }
        let mut files = original;
        files.insert("unrequested.md".into(), b"unrequested".to_vec());
        assert!(verify(case, &files)[0].is_err());
    }
}

#[test]
fn utf8_byte_limits_are_not_character_counts() {
    for (case, name, maximum) in [
        (CASES[0], "handoff.md", 7999),
        (CASES[1], "migration-notice.md", 7999),
        (CASES[2], "package/SKILL.md", 6000),
        (CASES[2], "package/references/review-checklist.md", 6000),
        (CASES[3], "package/references/planned-changes.md", 5999),
    ] {
        let mut files = artifact(case);
        let content = files.get_mut(name).unwrap();
        content.resize(maximum - 2, b' ');
        content.extend_from_slice("é".as_bytes());
        if case.starts_with("SKL-") {
            refresh_hashes(&mut files);
        }
        assert!(verify(case, &files).iter().all(Result::is_ok));
        files.get_mut(name).unwrap().push(b' ');
        if case.starts_with("SKL-") {
            refresh_hashes(&mut files);
        }
        assert!(verify(case, &files)[1].is_err());
    }
    let mut files = artifact(CASES[2]);
    files
        .get_mut("package/skill.json")
        .unwrap()
        .resize(4000, b' ');
    assert!(verify(CASES[2], &files).iter().all(Result::is_ok));
    files
        .get_mut("package/skill.json")
        .unwrap()
        .resize(4001, b' ');
    assert!(verify(CASES[2], &files)[1].is_err());
    let mut files = artifact(CASES[0]);
    files.insert("handoff.md".into(), vec![0xff]);
    assert!(verify(CASES[0], &files)[1].is_err());
}

fn refresh_hashes(files: &mut Files) {
    let mut updated = descriptor(files);
    for pointer in ["/body", "/resources/0", "/resources/1"] {
        if let Some(reference) = updated.pointer_mut(pointer) {
            let path = format!("package/{}", reference["path"].as_str().unwrap());
            reference["sha256"] = json!(digest_bytes(&files[&path]));
        }
    }
    put_descriptor(files, &updated);
}

#[test]
fn markdown_links_validate_contained_files_only_not_fragments() {
    let case = CASES[0];
    let scope = scope(case).unwrap();
    let mut files = artifact(case);
    let content = "[Timeline][t] [Recovery](operations/recovery.md \"source\") [Shift](%73hift/notes.md#handoff)\n\n[t]: incident/timeline.md\n\n`[Example](missing.md)`\n\n```md\n[Example](also-missing.md)\n```\n";
    // #handoff does not establish an existing anchor. This positive example
    // proves only that the cited file exists inside the workspace; fragment
    // validity remains not_run until independent artifact review.
    files.insert("handoff.md".into(), content.as_bytes().to_vec());
    assert!(local_links(&files, &scope).is_ok());
    assert_eq!(
        resolve_link("handoff.md", "shift/notes.md#deliberately-missing-anchor").unwrap(),
        Some("shift/notes.md".into())
    );
    for link in [
        "[Missing](absent.md)",
        "[Escape](../outside.md)",
        "[Escape](%2e%2e/outside.md)",
        "![Image](missing.png)",
        "[Absolute local](/outside.md)",
        "[Drive](C:/outside.md)",
        "[File URL](file:///outside.md)",
    ] {
        files.insert(
            "handoff.md".into(),
            format!("{content}\n{link}\n").into_bytes(),
        );
        assert!(local_links(&files, &scope).is_err(), "{link}");
    }
    files.insert("handoff.md".into(), b"`[Timeline](incident/timeline.md)`\n[Recovery](operations/recovery.md) [Shift](shift/notes.md)".to_vec());
    assert!(local_links(&files, &scope).is_err());
    assert_eq!(
        resolve_link("package/SKILL.md", "../project/examples.md").unwrap(),
        Some("project/examples.md".into())
    );
    assert!(resolve_link("package/SKILL.md", "../../outside.md").is_err());
    assert!(resolve_link("package/SKILL.md", "%5coutside.md").is_err());
    files.insert("handoff.md".into(), format!("{content}\n<jobs-file>\n\n[Remote](https://example.invalid/)\n<a href='elsewhere.md'>HTML is external review</a>\n").into_bytes());
    assert!(local_links(&files, &scope).is_ok());
    files.insert(
        "handoff.md".into(),
        b"[Remote](https://example.invalid/)".to_vec(),
    );
    assert!(local_links(&files, &scope).is_err());
}

#[test]
fn native_descriptor_validation_rejects_valid_json_with_invalid_metadata() {
    let case = CASES[2];
    for (pointer, bad) in [
        ("/description", json!("description\nwith control")),
        ("/description", json!(" ")),
        ("/id", json!("INVALID ID")),
        ("/schema_version", json!(2)),
        ("/body/path", json!("../SKILL.md")),
        ("/body/sha256", json!("abcd")),
    ] {
        let mut files = artifact(case);
        let mut updated = descriptor(&files);
        *updated.pointer_mut(pointer).unwrap() = bad;
        put_descriptor(&mut files, &updated);
        assert!(
            native_package(&files).is_err(),
            "native validation of {pointer}"
        );
    }
    let mut files = artifact(case);
    let mut updated = descriptor(&files);
    updated["unexpected"] = json!(true);
    put_descriptor(&mut files, &updated);
    assert!(native_package(&files).is_err());
    updated.as_object_mut().unwrap().remove("unexpected");
    updated["resources"][0] = updated["body"].clone();
    put_descriptor(&mut files, &updated);
    assert!(native_package(&files).is_err());
}

#[test]
fn native_hashes_bind_exact_body_and_each_resource() {
    for case in [CASES[2], CASES[3]] {
        let original = artifact(case);
        let descriptor = descriptor(&original);
        for reference in
            std::iter::once(&descriptor["body"]).chain(descriptor["resources"].as_array().unwrap())
        {
            let path = format!("package/{}", reference["path"].as_str().unwrap());
            let mut files = original.clone();
            files.get_mut(&path).unwrap().push(b'\n');
            assert!(native_package(&files).is_err(), "stale hash {path}");
            files.remove(&path);
            assert!(native_package(&files).is_err(), "absent content {path}");
        }
    }
}

#[test]
fn maintenance_all_metadata_except_two_pointers_is_preserved() {
    let case = CASES[3];
    for (pointer, bad) in [
        ("/id", json!("different-id")),
        ("/description", json!("Different description")),
        ("/source", json!("different-source")),
        ("/license", json!("MIT")),
        ("/cues", json!(["new cue"])),
        ("/environments", json!(["windows"])),
        (
            "/required_tools",
            json!(["vcp_list", "vcp_read", "vcp_patch"]),
        ),
        ("/version", json!("1.3.2")),
    ] {
        let mut files = artifact(case);
        let mut updated = descriptor(&files);
        *updated.pointer_mut(pointer).unwrap() = bad;
        put_descriptor(&mut files, &updated);
        assert!(
            native_package(&files).is_ok(),
            "valid native shape: {pointer}"
        );
        assert!(
            verify(case, &files)[1].is_err(),
            "preserved pointer: {pointer}"
        );
    }
    let mut files = artifact(case);
    let mut updated = descriptor(&files);
    updated["resources"].as_array_mut().unwrap().swap(0, 1);
    put_descriptor(&mut files, &updated);
    assert!(native_package(&files).is_ok());
    assert!(verify(case, &files)[1].is_err());
    for name in [
        "package/SKILL.md",
        "package/references/delivered-changes.md",
    ] {
        let mut files = artifact(case);
        files.get_mut(name).unwrap().push(b'\n');
        refresh_hashes(&mut files);
        assert!(native_package(&files).is_ok());
        assert!(verify(case, &files).iter().all(Result::is_err));
    }
}

#[test]
fn exact_modification_set_rejects_version_only_maintenance() {
    let case = CASES[3];
    let mut files = fixture(case);
    let mut updated = descriptor(&files);
    updated["version"] = json!("1.3.1");
    put_descriptor(&mut files, &updated);
    assert!(native_package(&files).is_ok());
    assert!(verify(case, &files)[1].is_err());
}

#[test]
#[cfg(windows)]
fn frozen_followup_cases_pass_the_real_read_only_entrypoint() {
    for case in CASES {
        let directory = tempfile::tempdir().unwrap();
        for (name, bytes) in artifact(case) {
            let path = directory.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let result = check(directory.path(), case);
        assert!(result.iter().all(Result::is_ok), "{case}: {result:?}");
    }
}

#[test]
#[cfg(windows)]
fn owner_map_supports_54_unique_workspaces_and_rejects_55() {
    let runtime = tempfile::tempdir().unwrap();
    let workspaces: Vec<_> = (0..54).map(|_| tempfile::tempdir().unwrap()).collect();
    let executable = runtime.path().join("vcp-authoring-check.exe");
    let mut cases: Vec<_> = workspaces
        .iter()
        .map(|root| json!({"workspace":root.path(), "case_id":CASES[0]}))
        .collect();
    let path = runtime.path().join("authoring-cases.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"schema_version":1,"cases":cases})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::owner_case(workspaces[53].path(), &executable).unwrap(),
        CASES[0]
    );
    cases.push(cases[0].clone());
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"schema_version":1,"cases":cases})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::owner_case(workspaces[0].path(), &executable).unwrap_err(),
        "owner case map bounds"
    );
}
