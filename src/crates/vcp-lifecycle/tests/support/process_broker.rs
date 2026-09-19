// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use vcp_domain::policy::*;
use vcp_tools::process::{Mode, Profile, Request};
fn profile(name: &str, executable: &Path, reduced: bool) -> Profile {
    Profile::new(
        name.into(),
        executable.into(),
        Mode::Direct,
        BTreeMap::from([
            ("SystemRoot".into(), std::env::var("SystemRoot").unwrap()),
            ("CI".into(), "synthetic-public-setting".into()),
        ]),
        BTreeSet::new(),
        reduced,
    )
    .unwrap()
    .with_inputs(vec!["fixture.txt".into()])
    .unwrap()
}
fn request(mode: &str, directory: &Path) -> Request {
    Request {
        profile: "fixture".into(),
        arguments: vec![mode.into(), directory.to_str().unwrap().into()],
        directory: String::new(),
        timeout_ms: 10_000,
        output_bytes: 1024 * 1024,
        input: None,
    }
}
async fn ready(path: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn process_broker_observes_authority_argv_limits_and_native_tree_stop() {
    struct EnvironmentGuard(Option<std::ffi::OsString>);
    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.0 {
                std::env::set_var("VCP_FIXTURE_MUST_NOT_INHERIT", value);
            } else {
                std::env::remove_var("VCP_FIXTURE_MUST_NOT_INHERIT");
            }
        }
    }
    let _environment = EnvironmentGuard(std::env::var_os("VCP_FIXTURE_MUST_NOT_INHERIT"));
    std::env::set_var("VCP_FIXTURE_MUST_NOT_INHERIT", "synthetic-ambient-value");
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace ü");
        let tools = temp.path().join("host tools");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&tools).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let executable = tools.join("fixture.exe");
        let original = fs::read(env!("CARGO_BIN_EXE_vcp-process-fixture")).unwrap();
        fs::write(&executable, &original).unwrap();
        fs::write(workspace.join("fixture.txt"), b"answer = 42\n").unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-process-fixture",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_fixture_provider(c);
                starter
                    .lifecycle()
                    .authorize_startup(c.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(id, binding.clone()).unwrap();
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let mut policy = Policy {
            workspace: config.workspace.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Autonomous,
            denials: vec![],
            workspace_roots: BTreeSet::from([
                RootId::parse(config.workspace.as_str()).unwrap(),
                RootId::parse("exec-fixture").unwrap(),
                RootId::parse("exec-cmd").unwrap(),
                RootId::parse("exec-powershell").unwrap(),
            ]),
            automatic_effects: BTreeSet::from([
                EffectClass::Read,
                EffectClass::Write,
                EffectClass::Execute,
                EffectClass::Network,
                EffectClass::Install,
                EffectClass::Publish,
                EffectClass::Opaque,
            ]),
            timeout_ceiling_ms: Units::new(120_000),
            output_ceiling_bytes: ByteCount::new(8 * 1024 * 1024),
        };
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.configure_process_profile(profile("fixture", &executable, true))
            .unwrap();
        let stale = host
            .prepare_process(id, request("argv", &workspace))
            .unwrap();
        fs::write(workspace.join("fixture.txt"), b"human edit").unwrap();
        assert!(host.dispatch_process(stale).is_err());
        assert!(!workspace.join("argv.json").exists());
        fs::write(workspace.join("fixture.txt"), b"answer = 42\n").unwrap();
        let stale = host
            .prepare_process(id, request("argv", &workspace))
            .unwrap();
        fs::write(&executable, b"changed executable identity").unwrap();
        assert!(host.dispatch_process(stale).is_err());
        assert!(!workspace.join("argv.json").exists());
        fs::write(&executable, &original).unwrap();
        let stale = host
            .prepare_process(id, request("argv", &workspace))
            .unwrap();
        host.configure_process_profile(profile("fixture", &executable, false))
            .unwrap();
        assert!(host.dispatch_process(stale).is_err());
        let restricted = host
            .prepare_process(id, request("argv", &workspace))
            .unwrap();
        assert!(matches!(
            restricted.decision,
            vcp_policy::Decision::Deny { .. }
        ));
        assert!(host.dispatch_process(restricted).is_err());
        assert!(!workspace.join("argv.json").exists());
        host.configure_process_profile(profile("fixture", &executable, true))
            .unwrap();
        let mut argv = request("argv", &workspace);
        let expected = vec![
            "space ü".to_owned(),
            "literal & | > ; %PATH%".into(),
            "quote\"tail\\".into(),
        ];
        argv.arguments.extend(expected.clone());
        let ticket = host.prepare_process(id, argv).unwrap();
        assert!(matches!(
            ticket.decision,
            vcp_policy::Decision::Allow { .. }
        ));
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.state, CaptureState::Complete);
        let actual: serde_json::Value =
            serde_json::from_slice(&fs::read(workspace.join("argv.json")).unwrap()).unwrap();
        assert_eq!(actual["args"], serde_json::json!(expected));
        assert_eq!(actual["ci"], "synthetic-public-setting");
        assert_eq!(actual["inherited"], serde_json::Value::Null);
        policy.revision = PolicyRevision::new(1);
        policy.mode = Autonomy::Plan;
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let denied = host
            .prepare_process(id, request("verify", &workspace))
            .unwrap();
        assert!(matches!(denied.decision, vcp_policy::Decision::Deny { .. }));
        assert!(host.dispatch_process(denied).is_err());
        policy.revision = PolicyRevision::new(2);
        policy.mode = Autonomy::Ask;
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(1),
        )
        .unwrap();
        let ticket = host
            .prepare_process(id, request("verify", &workspace))
            .unwrap();
        assert!(ticket.question.is_some());
        let current: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(current.state, TaskState::WaitingForInput);
        host.command(
            Command::Decide {
                id: ticket.question.clone().unwrap(),
                operation_digest: ticket.digest().into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        assert_eq!(
            host.snapshot()
                .unwrap()
                .record(
                    Collection::Task,
                    binding.scope.task.as_str(),
                    &config.workspace
                )
                .unwrap()
                .decode::<Task>()
                .unwrap()
                .state,
            TaskState::WaitingForInput
        );
        host.resume(id, current.revision, current.fingerprint)
            .unwrap();
        assert_eq!(
            host.dispatch_process(ticket)
                .unwrap()
                .wait()
                .await
                .unwrap()
                .exit_code,
            Some(0)
        );
        let reused = host
            .prepare_process(id, request("verify", &workspace))
            .unwrap();
        assert!(matches!(
            reused.decision,
            vcp_policy::Decision::Allow { grant: Some(_), .. }
        ));
        assert!(reused.question.is_none());
        assert_eq!(
            host.dispatch_process(reused)
                .unwrap()
                .wait()
                .await
                .unwrap()
                .exit_code,
            Some(0)
        );
        policy.revision = PolicyRevision::new(3);
        policy.mode = Autonomy::Autonomous;
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(2),
        )
        .unwrap();
        let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        for (name, executable, mode, script) in [
            (
                "cmd",
                system.join("System32").join("cmd.exe"),
                Mode::Cmd,
                "echo explicit-shell & echo preserved>\"cmd marker.txt\"",
            ),
            (
                "powershell",
                system
                    .join("System32")
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe"),
                Mode::PowerShell,
                "Set-Content -LiteralPath 'powershell ü.txt' -Value 'exact ; &'; Write-Output 'explicit-shell ; & ü'",
            ),
        ] {
            host.configure_process_profile(
                Profile::new(
                    name.into(),
                    executable,
                    mode,
                    BTreeMap::from([("SystemRoot".into(), system.to_str().unwrap().into())]),
                    BTreeSet::new(),
                    true,
                )
                .unwrap(),
            )
            .unwrap();
            let ticket = host
                .prepare_process(
                    id,
                    Request {
                        profile: name.into(),
                        arguments: vec![script.into()],
                        directory: String::new(),
                        timeout_ms: 10_000,
                        output_bytes: 64 * 1024,
                        input: None,
                    },
                )
                .unwrap();
            let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
            assert_eq!(
                result.exit_code,
                Some(0),
                "{name}: {:?}",
                result.stderr_tail
            );
            assert!(String::from_utf8_lossy(&result.stdout_tail).contains("explicit-shell"));
            if name=="cmd"{assert_eq!(fs::read_to_string(workspace.join("cmd marker.txt")).unwrap().trim(),"preserved");}
            if name=="powershell"{assert_eq!(fs::read_to_string(workspace.join("powershell ü.txt")).unwrap().trim(),"exact ; &");}
        }
        for terminal in [false, true] {
            let directory = workspace.join(if terminal {
                "count-terminal"
            } else {
                "count-pipes"
            });
            fs::create_dir(&directory).unwrap();
            let bounded = profile("fixture", &executable, true)
                .with_process_count(2)
                .unwrap();
            let bounded = if terminal {
                bounded.with_terminal(24, 80).unwrap()
            } else {
                bounded
            };
            host.configure_process_profile(bounded).unwrap();
            let ticket = host
                .prepare_process(id, request("process-count", &directory))
                .unwrap();
            let process = host.dispatch_process(ticket).unwrap();
            ready(&directory.join("count-result")).await;
            assert_eq!(
                fs::read_to_string(directory.join("count-result")).unwrap(),
                "blocked"
            );
            assert_eq!(process.active_process_count().unwrap(), 2);
            assert!(!directory.join("excess-marker").exists());
            use std::os::windows::fs::OpenOptionsExt;
            assert!(fs::OpenOptions::new()
                .write(true)
                .share_mode(0)
                .open(directory.join("locked"))
                .is_err());
            fs::write(directory.join("finish"), b"finish").unwrap();
            let result = process.wait().await.unwrap();
            assert_eq!(result.exit_code, Some(0));
            assert!(fs::OpenOptions::new()
                .write(true)
                .share_mode(0)
                .open(directory.join("locked"))
                .is_ok());
            assert!(!directory.join("excess-marker").exists());
        }
        host.configure_process_profile(profile("fixture", &executable, true))
            .unwrap();
        let mut flood = request("flood", &workspace);
        flood.output_bytes = 32768;
        let ticket = host.prepare_process(id, flood).unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(result.reason.as_deref(), Some("output limit exceeded"));
        assert_eq!(result.stdout.state, CaptureState::Aborted);
        assert_eq!(result.stderr.state, CaptureState::Aborted);
        assert!(result.stdout.length.get() + result.stderr.length.get() <= 32768 + 2 * 8192);
        let mut tree = request("tree", &workspace);
        tree.timeout_ms = 1500;
        let ticket = host.prepare_process(id, tree).unwrap();
        let process = host.dispatch_process(ticket).unwrap();
        ready(&workspace.join("child-ready")).await;
        assert!(process.active_process_count().unwrap() >= 2);
        let read = host
            .prepare_tool(
                id,
                vcp_tools::Request::Read {
                    path: "fixture.txt".into(),
                    max_bytes: 100,
                },
            )
            .unwrap();
        assert!(host.dispatch_tool(read).is_err());
        let result = process.wait().await.unwrap();
        assert_eq!(result.reason.as_deref(), Some("process deadline elapsed"));
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                use std::os::windows::fs::OpenOptionsExt;
                if fs::OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(workspace.join("locked"))
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(
            codex_utils_pty::owned_pty_supported(),
            "this native qualification requires owned ConPTY support"
        );
        host.configure_process_profile(
            profile("fixture", &executable, true)
                .with_terminal(24, 100)
                .unwrap(),
        )
        .unwrap();
        let mut terminal = request("terminal", &workspace);
        terminal.input = Some("hello ü ; &\r\n".into());
        let ticket = host.prepare_process(id, terminal).unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "terminal: {:?}; {:?}",
            result.reason,
            String::from_utf8_lossy(&result.stdout_tail)
        );
        let terminal: serde_json::Value =
            serde_json::from_slice(&fs::read(workspace.join("terminal.json")).unwrap()).unwrap();
        assert_eq!(terminal["stdin_terminal"], true);
        assert_eq!(terminal["stdout_terminal"], true);
        assert_eq!(terminal["input"].as_str().unwrap().trim(), "hello ü ; &");
        assert!(String::from_utf8_lossy(&result.stdout_tail).contains("terminal stdout"));
        assert!(String::from_utf8_lossy(&result.stdout_tail).contains("terminal stderr"));
        assert_eq!(result.stderr.length.get(), 0);
        host.configure_process_profile(
            Profile::new(
                "powershell".into(),
                system.join("System32/WindowsPowerShell/v1.0/powershell.exe"),
                Mode::PowerShell,
                BTreeMap::from([("SystemRoot".into(), system.to_str().unwrap().into())]),
                BTreeSet::new(),
                true,
            )
            .unwrap()
            .with_terminal(24, 100)
            .unwrap(),
        )
        .unwrap();
        let ticket = host
            .prepare_process(
                id,
                Request {
                    profile: "powershell".into(),
                    arguments: vec!["Write-Output 'terminal powershell'".into()],
                    directory: String::new(),
                    timeout_ms: 10_000,
                    output_bytes: 65536,
                    input: None,
                },
            )
            .unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "process: {:?}; {:?}",
            result.reason,
            String::from_utf8_lossy(&result.stdout_tail)
        );
        assert!(String::from_utf8_lossy(&result.stdout_tail).contains("terminal powershell"));
        let detached = workspace.join("terminal-descendant");
        fs::create_dir(&detached).unwrap();
        let ticket = host
            .prepare_process(id, request("orphan", &detached))
            .unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(detached.join("child-ready").exists());
        {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .share_mode(0)
                .open(detached.join("locked"))
                .unwrap();
        }
        let mut flood = request("flood", &workspace);
        flood.output_bytes = 32768;
        let ticket = host.prepare_process(id, flood).unwrap();
        let result = host.dispatch_process(ticket).unwrap().wait().await.unwrap();
        assert_eq!(result.reason.as_deref(), Some("output limit exceeded"));
        assert_eq!(result.stdout.state, CaptureState::Aborted);
        assert!(result.stdout.length.get() <= 32768 + 8192);
        let timed = workspace.join("terminal-deadline");
        fs::create_dir(&timed).unwrap();
        let mut timed_request = request("tree", &timed);
        timed_request.timeout_ms = 1500;
        let ticket = host.prepare_process(id, timed_request).unwrap();
        let process = host.dispatch_process(ticket).unwrap();
        ready(&timed.join("child-ready")).await;
        assert!(process.active_process_count().unwrap() >= 2);
        let result = process.wait().await.unwrap();
        assert_eq!(result.reason.as_deref(), Some("process deadline elapsed"));
        host.configure_process_profile(profile("fixture", &executable, true))
            .unwrap();
        policy.revision = PolicyRevision::new(4);
        policy.denials.push(Denial {
            id: "executable-read-denial".into(),
            origin: RuleOrigin::User,
            reason: "synthetic profile restriction".into(),
            effects: BTreeSet::from([EffectClass::Read]),
            tool: Some("vcp_exec".into()),
            roots: BTreeSet::from([RootId::parse("exec-fixture").unwrap()]),
            paths: vec![],
        });
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(3),
        )
        .unwrap();
        // Missing executable would fail filesystem preparation; the trusted denial
        // must be reported first, before the executable is opened or hashed.
        let hidden = tools.join("fixture-hidden.exe");
        fs::rename(&executable, &hidden).unwrap();
        let denied = host.prepare_process(id, request("verify", &workspace));
        assert!(denied
            .err()
            .unwrap()
            .to_string()
            .contains("trusted read denial prevents executable preparation"));
        fs::rename(&hidden, &executable).unwrap();
        policy.revision = PolicyRevision::new(5);
        policy.denials.clear();
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(4),
        )
        .unwrap();
        for (index, effect) in [
            EffectClass::Write,
            EffectClass::Install,
            EffectClass::Publish,
        ]
        .into_iter()
        .enumerate()
        {
            policy.revision = PolicyRevision::new(6 + index as u64);
            policy.denials = vec![Denial {
                id: "opaque-ceiling".into(),
                origin: RuleOrigin::User,
                reason: "synthetic scoped ceiling".into(),
                effects: BTreeSet::from([effect]),
                tool: Some("vcp_exec".into()),
                roots: BTreeSet::from([RootId::parse("unobserved-root").unwrap()]),
                paths: vec!["restricted".into()],
            }];
            host.command(
                Command::SetPolicy {
                    policy: policy.clone(),
                },
                None,
                Revision::new(5 + index as u64),
            )
            .unwrap();
            let marker = workspace.join(format!("denied-{index}"));
            let denied = host.prepare_process(id, request("write", &marker)).unwrap();
            assert!(
                matches!(&denied.decision, vcp_policy::Decision::Deny { origin, .. } if origin == "opaque-ceiling")
            );
            assert!(host.dispatch_process(denied).is_err());
            assert!(!marker.exists());
        }
        policy.revision = PolicyRevision::new(9);
        policy.denials.clear();
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(8),
        )
        .unwrap();
        let closing = workspace.join("closing-child");
        if backend == BackendKind::Sqlite {
            host.configure_process_profile(
                profile("fixture", &executable, true)
                    .with_terminal(24, 80)
                    .unwrap(),
            )
            .unwrap();
        }
        fs::create_dir(&closing).unwrap();
        let ticket = host.prepare_process(id, request("tree", &closing)).unwrap();
        let process = host.dispatch_process(ticket).unwrap();
        ready(&closing.join("child-ready")).await;
        let observed = tokio::spawn(process.wait());
        owner.close().await.unwrap();
        let closed = observed.await.unwrap().unwrap();
        assert_ne!(closed.exit_code, Some(0));
        assert!(host
            .prepare_process(id, request("verify", &workspace))
            .is_err());
    }
}
