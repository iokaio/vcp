// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
use std::{collections::HashMap, path::Path, process::Command, time::Duration};

async fn terminal_launch(
    arguments: &[String],
    directory: &Path,
    account_home: &Path,
    cancel_interview: bool,
) -> (i32, String) {
    use codex_utils_pty::{spawn_pty_process, TerminalSize};
    let mut environment: HashMap<String, String> = std::env::vars().collect();
    environment.insert("LOCALAPPDATA".into(), account_home.to_str().unwrap().into());
    environment.insert("NO_COLOR".into(), "1".into());
    // Supplying a synthetic environment credential also keeps the fixture from
    // reading the real user's optional Credential Manager entry.
    environment.insert(
        "OPENROUTER_API_KEY".into(),
        "synthetic-first-contact".into(),
    );
    let mut child = spawn_pty_process(
        env!("CARGO_BIN_EXE_vcp"),
        arguments,
        directory,
        &environment,
        &None,
        TerminalSize {
            rows: 40,
            cols: 120,
        },
        &[],
    )
    .await
    .unwrap();
    let writer = child.session.writer_sender();
    let (display, mut observed) = tokio::sync::watch::channel(String::new());
    let output = tokio::spawn(async move {
        let mut captured = Vec::new();
        while let Some(chunk) = child.stdout_rx.recv().await {
            assert!(captured.len() + chunk.len() <= 256 * 1024);
            captured.extend(chunk);
            display.send_replace(String::from_utf8_lossy(&captured).into_owned());
        }
        String::from_utf8_lossy(&captured).into_owned()
    });
    let exercise = async {
        if cancel_interview {
            while !observed.borrow().contains("Keep this credential [Enter]") {
                observed
                    .changed()
                    .await
                    .expect("interview exited before credential prompt");
            }
            writer.send(b"\r".to_vec()).await.unwrap();
            while !observed.borrow().contains("Use this set [Enter]") {
                observed
                    .changed()
                    .await
                    .expect("interview exited before model-set prompt");
            }
            writer.send(b"\r".to_vec()).await.unwrap();
            while !observed.borrow().contains("Use this set [Enter]") {
                observed
                    .changed()
                    .await
                    .expect("interview exited before model choices");
            }
            // The interview has not reached metadata refresh or test admission.
            writer.send(b"cancel\r".to_vec()).await.unwrap();
        }
        (&mut child.exit_rx).await.unwrap()
    };
    let outcome = tokio::time::timeout(Duration::from_secs(20), exercise).await;
    child.session.terminate();
    let captured = tokio::time::timeout(Duration::from_secs(5), output)
        .await
        .unwrap()
        .unwrap();
    assert!(
        outcome.is_ok(),
        "first-contact console timed out: {captured}"
    );
    assert!(
        !captured.contains("synthetic-first-contact"),
        "credential appeared in output"
    );
    (outcome.unwrap(), captured)
}

#[test]
fn explicit_help_and_redirected_bare_launch_are_identical_without_setup() {
    let temp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_vcp"))
            .args(args)
            .current_dir(temp.path())
            .env_remove("LOCALAPPDATA")
            .env_remove("OPENROUTER_API_KEY")
            .output()
            .unwrap()
    };
    let explicit = run(&["--help"]);
    let bare = run(&[]);
    assert!(explicit.status.success());
    assert!(bare.status.success());
    assert_eq!(bare.stdout, explicit.stdout);
    assert_eq!(bare.stderr, explicit.stderr);
    let help = String::from_utf8(explicit.stdout).unwrap();
    assert!(help.contains("setup"));
    assert!(help.contains("models"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(run(&["--workspace", "missing-project", "setup", "--help"])
        .status
        .success());
    let noninteractive = run(&["--non-interactive", "setup"]);
    assert!(!noninteractive.status.success());
    assert!(
        String::from_utf8_lossy(&noninteractive.stderr).contains("interactive Windows terminal")
    );
}

#[test]
fn help_does_not_parse_or_repair_corrupt_account_state() {
    let temp = tempfile::tempdir().unwrap();
    let account = temp.path().join("VCP/account");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join("setup-complete.json"), b"invalid sentinel").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_vcp"))
        .arg("--help")
        .env("LOCALAPPDATA", temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        std::fs::read(account.join("setup-complete.json")).unwrap(),
        b"invalid sentinel"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_first_run_resumes_after_cancel_and_completion_follows_windows_user() {
    let temp = tempfile::tempdir().unwrap();
    let account_home = temp.path().join("user-account");
    let first_directory = temp.path().join("first-directory");
    let second_directory = temp.path().join("second-directory");
    for directory in [&account_home, &first_directory, &second_directory] {
        std::fs::create_dir(directory).unwrap();
    }
    let account = account_home.join("VCP/account");
    for directory in [&first_directory, &second_directory] {
        let (status, output) = terminal_launch(&[], directory, &account_home, true).await;
        assert_eq!(status, 2, "{output}");
        assert!(output.contains("VCP setup"), "{output}");
        assert!(output.contains("Project selection is optional"), "{output}");
        assert!(output.contains("Balanced quality and cost"), "{output}");
        assert!(output.contains("setup cancelled"), "{output}");
        assert!(
            !output.contains("schema_version"),
            "interactive cancellation should be readable: {output}"
        );
        assert!(!output.contains("Connection test:"));
        assert!(!account.join("setup-complete.json").exists());
        assert!(!account.join("connection-pending.json").exists());
    }
    // Seed the success boundary without making a paid provider call. The
    // separate connection tests exercise accounted success and failure.
    let completion = serde_json::json!({"version":1,"connection":{
        "status":"connected","response":"synthetic completed response",
        "model":"fixture/model","reported_cost_micros":0
    }});
    vcp_cli::model_preferences::save_record(&account, "setup-complete.json", &completion).unwrap();
    let alternate_data = temp.path().join("unused-alternate-data");
    for (directory, arguments) in [
        (&first_directory, vec![]),
        (
            &second_directory,
            vec!["--data-dir".into(), alternate_data.to_str().unwrap().into()],
        ),
    ] {
        let (status, output) = terminal_launch(&arguments, directory, &account_home, false).await;
        assert_eq!(status, 0, "{output}");
        assert!(output.contains("Usage: vcp"), "{output}");
        assert!(
            output.contains("setup") && output.contains("models"),
            "{output}"
        );
        assert!(
            !output.contains("VCP setup —"),
            "interview repeated: {output}"
        );
        assert!(
            !output.contains("Use this set [Enter]"),
            "interview repeated: {output}"
        );
    }
    assert!(!alternate_data.exists());
    assert_eq!(std::fs::read_dir(first_directory).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(second_directory).unwrap().count(), 0);
    assert_eq!(
        vcp_cli::model_preferences::read_record(&account, "setup-complete.json").unwrap(),
        Some(completion)
    );
}
