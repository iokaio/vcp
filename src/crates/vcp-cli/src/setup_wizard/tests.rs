// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::VecDeque;

struct Script {
    answers: VecDeque<String>,
    transcript: String,
}
impl Script {
    fn new(answers: &[&str]) -> Self {
        Self {
            answers: answers.iter().map(|v| v.to_string()).collect(),
            transcript: String::new(),
        }
    }
}
impl Prompter for Script {
    fn say(&mut self, text: &str) {
        self.transcript.push_str(text);
        self.transcript.push('\n');
    }
    fn line(&mut self, prompt: &str) -> Result<Option<String>, String> {
        self.say(prompt);
        Ok(self.answers.pop_front())
    }
    fn secret(&mut self, prompt: &str) -> Result<Option<credential::Secret>, String> {
        self.say(prompt);
        self.answers
            .pop_front()
            .map(credential::Secret::new)
            .transpose()
    }
}

#[test]
fn interactive_rotation_choices_are_explicit_and_survive_revisiting() {
    let mut choices = Default::default();
    let mut prompt = Script::new(&[
        "rotate",
        "main",
        "fixture/one,fixture/two",
        "",
        "fixture/three",
        "",
        "fixture/reserve",
        "0.10",
        "use",
    ]);
    let set = choose_set(&mut prompt, model_preferences::balanced(), &mut choices).unwrap();
    assert_eq!(choices["main"].len(), 3);
    assert_eq!(
        choices["main"][0].models,
        vec!["fixture/one", "fixture/two"]
    );
    assert_eq!(
        choices["main"][2].max_reference_request_cost_usd.as_deref(),
        Some("0.10")
    );
    let original = choices.clone();
    choose_set(&mut Script::new(&["use"]), set, &mut choices).unwrap();
    assert_eq!(choices, original);
    assert!(prompt.transcript.contains("no model calls were made"));
}
#[derive(Default)]
struct Fake {
    calls: usize,
    done: bool,
    fail: bool,
    no_key: bool,
    stored: bool,
    environment: Option<String>,
    prefs: Option<Preferences>,
    project: Option<PathBuf>,
    rename_during_prepare: Option<(PathBuf, PathBuf)>,
}

#[tokio::test]
async fn named_environment_choice_and_changing_existing_source_need_no_secret_entry() {
    for (no_key, answers) in [
        (
            true,
            vec![
                "2",
                "MISSING_NAME",
                "VCP_TEST_ROUTER_KEY",
                "",
                "",
                "no",
                "",
                "yes",
            ],
        ),
        (
            false,
            vec![
                "change",
                "2",
                "VCP_TEST_ROUTER_KEY",
                "",
                "",
                "no",
                "",
                "yes",
            ],
        ),
    ] {
        let mut prompt = Script::new(&answers);
        let mut backend = Fake {
            no_key,
            ..Default::default()
        };
        interview(&mut prompt, &mut backend).await.unwrap();
        assert_eq!(backend.environment.as_deref(), Some("VCP_TEST_ROUTER_KEY"));
        assert!(!backend.stored);
        assert_eq!(backend.calls, 1);
        assert!(!prompt.transcript.contains("API key (hidden"));
    }
}

#[tokio::test]
async fn recoverable_input_mistakes_repeat_only_the_affected_questions() {
    let mut prompt = Script::new(&[
        "",
        "choose",
        "not-a-maker",
        "",
        "choose",
        "openai",
        "general",
        "misspelled",
        "1",
        "",
        "invalid-budget",
        "0",
        "2",
        "typo",
        "no",
        "0.000001",
        "0.01",
        "typo",
        "yes",
    ]);
    let mut backend = Fake::default();
    interview(&mut prompt, &mut backend).await.unwrap();
    assert!(backend.done);
    assert_eq!(backend.calls, 1);
    assert_eq!(backend.prefs.unwrap().budget_usd, "2");
    assert!(prompt.transcript.contains("No sets match"));
    assert!(prompt.transcript.contains("Choose a displayed"));
}

#[tokio::test]
async fn cancel_at_secret_entry_never_saves_a_credential_or_calls_a_model() {
    let mut backend = Fake {
        no_key: true,
        ..Default::default()
    };
    assert!(interview(&mut Script::new(&["1", "cancel"]), &mut backend)
        .await
        .is_err());
    assert!(!backend.stored);
    assert!(!backend.done);
    assert_eq!(backend.calls, 0);
}

#[cfg(windows)]
#[test]
fn interactive_model_changes_are_scoped_and_declines_preserve_preferences() {
    let temporary = tempfile::tempdir().unwrap();
    let account = temporary.path().join("account");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let original = Preferences::default();
    model_preferences::save(&account, &original).unwrap();
    models_interview(
        &mut Script::new(&["account", "choose", "openai", "general", "openai", "", "no"]),
        &account,
        &project,
    )
    .unwrap();
    assert_eq!(
        model_preferences::read(&account).unwrap(),
        Some(original.clone())
    );
    models_interview(
        &mut Script::new(&[
            "project", "choose", "openai", "general", "openai", "", "yes",
        ]),
        &account,
        &project,
    )
    .unwrap();
    assert_eq!(model_preferences::read(&account).unwrap(), Some(original));
    assert_eq!(
        model_preferences::effective(&account, &project)
            .unwrap()
            .set
            .id,
        "openai"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn completed_setup_stays_successful_when_pending_pointer_cannot_be_archived() {
    use std::os::windows::fs::OpenOptionsExt;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("account");
    model_preferences::ensure_root(&root).unwrap();
    std::fs::write(root.join(PENDING), b"{}").unwrap();
    let _blocked_archive = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(root.join(PENDING))
        .unwrap();
    let mut backend = Native {
        root: root.clone(),
        prepared: None,
    };
    let response = json!({"status":"connected","response":"synthetic greeting","model":"fixture/model","reported_cost_micros":1});
    backend
        .finish(
            &Preferences::default(),
            None,
            Path::new("README.md"),
            &response,
        )
        .await
        .unwrap();
    assert!(completed(&root).unwrap());
    assert!(root.join(PENDING).exists());
    assert!(model_preferences::read(&root).unwrap().is_some());
}
impl Backend for Fake {
    fn preferences(&self) -> Result<Option<Preferences>, String> {
        Ok(self.prefs.clone())
    }
    fn has_key(&self) -> Result<bool, String> {
        Ok(!self.no_key)
    }
    fn set_key(&mut self, key: credential::Secret, store: bool) -> Result<(), String> {
        assert_eq!(key.expose(), "synthetic-secret");
        self.stored = store;
        Ok(())
    }
    fn environment_name(&self) -> Result<String, String> {
        Ok("OPENROUTER_API_KEY".into())
    }
    fn use_environment(&mut self, name: &str) -> Result<(), String> {
        if name != "VCP_TEST_ROUTER_KEY" {
            return Err("environment variable unavailable".into());
        }
        self.environment = Some(name.into());
        Ok(())
    }
    async fn prepare(&mut self, prefs: &Preferences) -> Result<(String, u64), String> {
        if let Some((original, replacement)) = &self.rename_during_prepare {
            assert!(
                std::fs::rename(original, replacement).is_err(),
                "trusted project root must stay pinned through setup"
            );
        }
        Ok((prefs.set.roles["main"][0].clone(), 1000))
    }
    async fn test(&mut self, _: &str) -> Result<Value, String> {
        self.calls += 1;
        if self.fail {
            return Err("synthetic connection failure".into());
        }
        Ok(
            json!({"status":"connected","response":"Hello\u{001b}[31m","model":"fixture/model","reported_cost_micros":12}),
        )
    }
    async fn finish(
        &mut self,
        prefs: &Preferences,
        project: Option<&Path>,
        _: &Path,
        _: &Value,
    ) -> Result<(), String> {
        self.prefs = Some(prefs.clone());
        self.project = project.map(Path::to_path_buf);
        self.done = true;
        Ok(())
    }
}

#[tokio::test]
async fn balanced_setup_completes_with_no_project_and_one_reported_test() {
    let mut prompt = Script::new(&["", "", "", "no", "", "yes"]);
    let mut backend = Fake::default();
    interview(&mut prompt, &mut backend).await.unwrap();
    assert!(backend.done);
    assert_eq!(backend.calls, 1);
    assert!(backend.project.is_none());
    assert_eq!(backend.prefs.unwrap().set.id, "balanced");
    assert!(prompt.transcript.contains("Reported cost: $0.000012"));
    assert!(!prompt.transcript.contains('\u{001b}'));
    assert!(prompt.transcript.contains("does not test every role"));
}

#[tokio::test]
async fn interruption_decline_and_insufficient_budget_never_send_a_prompt() {
    let complete = ["", "", "", "no", "", "yes"];
    for end in 0..complete.len() {
        let mut backend = Fake::default();
        assert!(interview(&mut Script::new(&complete[..end]), &mut backend)
            .await
            .is_err());
        assert_eq!(backend.calls, 0);
        assert!(!backend.done);
    }
    for answers in [
        ["", "", "", "no", "0.000999", "yes"],
        ["", "", "", "no", "26", "yes"],
        ["", "", "", "no", "", "no"],
    ] {
        let mut backend = Fake::default();
        assert!(interview(&mut Script::new(&answers), &mut backend)
            .await
            .is_err());
        assert_eq!(backend.calls, 0);
        assert!(!backend.done);
    }
}

#[tokio::test]
async fn failed_test_never_completes_setup_or_replaces_preferences() {
    let original = Preferences::default();
    let mut backend = Fake {
        fail: true,
        prefs: Some(original.clone()),
        ..Default::default()
    };
    assert!(interview(
        &mut Script::new(&["", "", "2", "no", "", "yes"]),
        &mut backend
    )
    .await
    .is_err());
    assert_eq!(backend.calls, 1);
    assert!(!backend.done);
    assert_eq!(backend.prefs, Some(original));
}

#[tokio::test]
async fn key_is_hidden_and_storage_is_opt_in() {
    for choice in ["yes", "no"] {
        let mut prompt = Script::new(&["1", "synthetic-secret", choice, "", "", "no", "", "yes"]);
        let mut backend = Fake {
            no_key: true,
            ..Default::default()
        };
        let result = interview(&mut prompt, &mut backend).await;
        assert_eq!(result.is_ok(), choice == "yes");
        assert_eq!(backend.stored, choice == "yes");
        assert!(!prompt.transcript.contains("synthetic-secret"));
    }
}

#[tokio::test]
async fn browsing_and_customization_change_saved_role_assignments() {
    let mut prompt = Script::new(&[
        "",
        "choose",
        "",
        "systems",
        "systems",
        "customize",
        "fixture/child",
        "",
        "",
        "fixture/main",
        "",
        "",
        "",
        "",
        "use",
        "",
        "no",
        "",
        "yes",
    ]);
    let mut backend = Fake::default();
    interview(&mut prompt, &mut backend).await.unwrap();
    let selected = backend.prefs.unwrap();
    assert_eq!(selected.set.id, "custom");
    assert_eq!(selected.set.roles["main"], vec!["fixture/main"]);
    assert_eq!(selected.set.roles["child"], vec!["fixture/child"]);
}

#[tokio::test]
async fn optional_project_requires_explicit_trust_before_any_test() {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().to_str().unwrap();
    let mut accepted = Fake::default();
    interview(
        &mut Script::new(&["", "", "", "yes", folder, "yes", "README.md", "", "yes"]),
        &mut accepted,
    )
    .await
    .unwrap();
    assert_eq!(
        accepted.project,
        Some(temporary.path().canonicalize().unwrap())
    );
    assert_eq!(accepted.calls, 1);
    assert!(accepted.done);

    let mut denied = Fake::default();
    assert!(interview(
        &mut Script::new(&["", "", "", "yes", folder, "no"]),
        &mut denied
    )
    .await
    .is_err());
    assert_eq!(denied.calls, 0);
    assert!(!denied.done);
}

#[tokio::test]
async fn invalid_project_task_paths_fail_before_connection_dispatch() {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().to_str().unwrap();
    for path in ["../outside.txt", folder] {
        let mut backend = Fake::default();
        assert!(interview(
            &mut Script::new(&["", "", "", "yes", folder, "yes", path, "", "yes"]),
            &mut backend
        )
        .await
        .is_err());
        assert_eq!(backend.calls, 0);
        assert!(!backend.done);
    }
}

#[cfg(windows)]
#[tokio::test]
async fn project_identity_stays_pinned_after_trust_while_setup_awaits() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let mut backend = Fake {
        rename_during_prepare: Some((project.clone(), temporary.path().join("moved"))),
        ..Default::default()
    };
    interview(
        &mut Script::new(&[
            "",
            "",
            "",
            "yes",
            project.to_str().unwrap(),
            "yes",
            "README.md",
            "",
            "yes",
        ]),
        &mut backend,
    )
    .await
    .unwrap();
    assert!(backend.done);
    // The guard is released after the configured setup finishes.
    std::fs::rename(&project, temporary.path().join("moved")).unwrap();
}

#[cfg(windows)]
#[test]
fn account_setup_lease_serializes_interviews_and_releases_after_interruption() {
    let temporary = tempfile::tempdir().unwrap();
    let first = setup_lease(temporary.path()).unwrap();
    assert!(setup_lease(temporary.path()).is_err());
    drop(first);
    assert!(setup_lease(temporary.path()).is_ok());
}

#[cfg(windows)]
#[test]
fn completion_is_per_account_and_absent_for_interrupted_setup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("account");
    assert!(!completed(&root).unwrap());
    model_preferences::save(&root, &Preferences::default()).unwrap();
    assert!(!completed(&root).unwrap());
    model_preferences::save_record(
        &root,
        COMPLETE,
        &json!({"version":1,"connection":{"status":"connected"}}),
    )
    .unwrap();
    assert!(completed(&root).unwrap());
    assert!(!completed(&temp.path().join("another-user")).unwrap());
}
