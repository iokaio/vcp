// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! First contact with a redirected stdout: diagnostics name the actual
//! conflict and the next step, and scripts still receive exactly one record.
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn vcp(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vcp"))
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        .output()
        .unwrap()
}

fn single_record(stdout: &[u8]) -> Value {
    let text = String::from_utf8(stdout.to_vec()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let record: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["type"], "result");
    record
}

fn text(path: &Path) -> String {
    vcp_cli::settings::display_path(&path.canonicalize().unwrap())
}

#[test]
fn data_inside_workspace_names_both_folders_and_the_next_step() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir(&workspace).unwrap();
    let data = workspace.join("vcp-data");
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        data.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&text(&workspace)), "{stderr}");
    assert!(stderr.contains("vcp-data"), "{stderr}");
    assert!(
        stderr.contains("Next: cd into a project folder"),
        "{stderr}"
    );
    assert!(stderr.contains("--data-dir"), "{stderr}");
    let record = single_record(&output.stdout);
    assert_eq!(record["exit_code"], 2);
    assert_eq!(record["conditions"]["invalid_configuration"], true);
    assert!(!data.exists());
}

#[test]
fn data_inside_a_repository_names_the_repository() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    let repository = temp.path().join("repository");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&repository).unwrap();
    fs::write(repository.join(".git"), "gitdir: elsewhere").unwrap();
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        repository.join("data").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("the Git repository at {}", text(&repository))),
        "{stderr}"
    );
    single_record(&output.stdout);
}

#[test]
fn bare_setup_lists_explicit_steps_without_clap_errors() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir(&workspace).unwrap();
    let outside = temp.path().join("data");
    let inside = workspace.join("data");
    for (format, data_dir) in [(None, &outside), (Some("jsonl"), &outside), (None, &inside)] {
        let mut args = vec![
            "--workspace",
            workspace.to_str().unwrap(),
            "--data-dir",
            data_dir.to_str().unwrap(),
        ];
        if let Some(format) = format {
            args.extend(["--format", format]);
        }
        args.push("setup");
        let output = vcp(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty(), "{output:?}");
        let record = single_record(&output.stdout);
        assert_eq!(record["exit_code"], 2);
        let data = &record["data"];
        assert_eq!(data["status"], "input_required");
        assert_eq!(data["workspace"], text(&workspace));
        if data_dir == &inside {
            let problem = data["workspace_problem"].as_str().unwrap();
            assert!(
                problem.contains("inside the selected workspace"),
                "{problem}"
            );
            assert!(
                problem.contains("Next: cd into a project folder"),
                "{problem}"
            );
        } else {
            assert!(data["workspace_problem"].is_null());
        }
        let commands: Vec<_> = data["next"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["command"].as_str().unwrap())
            .collect();
        assert_eq!(commands[0], "vcp setup provider --help");
        assert_eq!(commands[1], "vcp setup profile --help");
        assert_eq!(commands.len(), 4);
    }
    assert!(!inside.exists() && !outside.exists());
    // Explicit steps keep their required arguments.
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "setup",
        "provider",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--model"));
    assert_eq!(
        single_record(&output.stdout)["conditions"]["invalid_configuration"],
        true
    );
}

#[test]
fn new_workspace_discovery_points_to_setup() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir(&workspace).unwrap();
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        temp.path().join("data").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let record = single_record(&output.stdout);
    assert_eq!(record["data"]["candidates"], serde_json::json!([]));
    assert!(record["data"]["message"]
        .as_str()
        .unwrap()
        .contains("Run `vcp setup`"));
}

#[test]
fn doctor_reports_readiness_without_a_profile_or_key() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    let data = temp.path().join("data");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&data).unwrap();
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        data.to_str().unwrap(),
        "doctor",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let record = single_record(&output.stdout);
    let report = &record["data"];
    assert_eq!(report["ready"], false);
    assert_eq!(report["path_checks_passed"], true);
    let status = |check: &str| {
        report["readiness"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["check"] == check)
            .map(|item| item["status"].as_str().unwrap().to_owned())
    };
    assert_eq!(status("installation").as_deref(), Some("ok"));
    assert_eq!(status("workspace").as_deref(), Some("ok"));
    assert_eq!(status("data_folder").as_deref(), Some("ok"));
    assert_eq!(status("credential").as_deref(), Some("warn"));
    assert_eq!(status("profile").as_deref(), Some("fail"));
    assert_eq!(status("provider_metadata"), None);
}

#[test]
fn setup_check_without_a_selection_points_to_setup() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    let data = temp.path().join("data");
    fs::create_dir(&workspace).unwrap();
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        data.to_str().unwrap(),
        "setup",
        "check",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no profile is selected for this workspace; run `vcp setup`"),
        "{stderr}"
    );
    single_record(&output.stdout);
    // A selection needs --config naming an existing profile.
    let output = vcp(&[
        "--workspace",
        workspace.to_str().unwrap(),
        "--data-dir",
        data.to_str().unwrap(),
        "setup",
        "select",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--config"));
    assert!(!data.exists());
}
