// SPDX-License-Identifier: Apache-2.0
//! Private, synthetic P0 qualification CLI. Uses the retained Codex controllers
//! and a loopback scripted provider. It is not the VCP product executable.
use codex_core::{CodexThread, StartThreadOptions, TurnInputRequest, TurnInputSubmission};
use codex_extension_api::ExtensionRegistryBuilder;
use codex_protocol::{
    ThreadId,
    protocol::{EventMsg, SessionSource, SubAgentSource},
    user_input::UserInput,
};
use core_test_support::{responses, test_codex::test_codex, wait_for_event};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use vcp_lifecycle::{Error, Lifecycle, control::Action};

#[derive(Serialize, Deserialize)]
struct Rollout {
    path: PathBuf,
    root: bool,
    id: ThreadId,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply a disposable fixture directory")?,
    );
    let expected_requests: usize = std::env::args()
        .nth(2)
        .ok_or("supply independently expected request count")?
        .parse()?;
    if expected_requests > 20 {
        return Err("fixture request cap is twenty".into());
    }
    if !directory.is_absolute() {
        return Err("fixture directory must be absolute".into());
    }
    std::fs::create_dir_all(directory.join("workspace"))?;
    std::fs::create_dir_all(directory.join("home"))?;
    let workspace = std::fs::canonicalize(directory.join("workspace"))?;
    let home = std::fs::canonicalize(directory.join("home"))?;
    let identity = workspace.to_string_lossy().into_owned();
    let (host, owner) = Lifecycle::open(
        &directory.join("lifecycle.journal"),
        &identity,
        Duration::from_secs(5),
    )?;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let registry = Arc::new(registry.build());
    let server = responses::start_mock_server().await;
    let observer = if expected_requests == 0 {
        responses::mount_sse_once(&server, responses::sse_completed("unexpected")).await
    } else {
        responses::mount_sse_sequence(
            &server,
            (0..expected_requests)
                .map(|id| responses::sse_completed(&format!("synthetic-{id}")))
                .collect(),
        )
        .await
    };
    let mut builder = test_codex().with_extensions(registry.clone()).with_config({
        let workspace = workspace.clone();
        let host = host.clone();
        move |config| {
            config.codex_home = home.clone().try_into().unwrap();
            config.cwd = workspace.clone().try_into().unwrap();
            config.analytics_enabled = Some(false);
            host.authorize_startup(config.cwd.as_path(), host.root().unwrap())
                .unwrap();
        }
    });
    let pointer = directory.join("rollouts.json");
    let reopening = !host.threads().map_err(debug)?.is_empty();
    let mut rollouts: Vec<Rollout> = if reopening {
        serde_json::from_slice(&std::fs::read(&pointer)?)?
    } else {
        vec![]
    };
    let test = if reopening {
        builder
            .resume(
                &server,
                Arc::new(tempfile::tempdir()?),
                rollouts
                    .iter()
                    .find(|row| row.root)
                    .ok_or("missing root rollout")?
                    .path
                    .clone(),
            )
            .await?
    } else {
        builder.build_with_auto_env(&server).await?
    };
    let root = test.session_configured.thread_id;
    if reopening {
        host.bind_recovered(test.codex.clone()).map_err(debug)?;
    } else {
        rollouts.push(Rollout {
            path: test
                .session_configured
                .rollout_path
                .clone()
                .ok_or("missing rollout")?,
            root: true,
            id: root,
        });
        save_rollouts(&pointer, &rollouts)?;
        host.attach_root(test.codex.clone()).map_err(debug)?;
    }
    let mut restored_children = Vec::new();
    let mut child: Option<Arc<CodexThread>> = None;
    if reopening {
        for rollout in rollouts.iter().filter(|row| !row.root) {
            let config = test.config.clone();
            host.authorize_startup(config.cwd.as_path(), Some(rollout.id))
                .map_err(debug)?;
            let restored = test_codex()
                .with_extensions(registry.clone())
                .with_config(move |value| *value = config.clone())
                .resume(
                    &server,
                    Arc::new(tempfile::tempdir()?),
                    rollout.path.clone(),
                )
                .await?;
            host.bind_recovered(restored.codex.clone()).map_err(debug)?;
            child = Some(restored.codex.clone());
            restored_children.push(restored);
        }
        host.hold(root, &host.inspect(root).map_err(debug)?.revision)
            .map_err(debug)?
            .wait()
            .await
            .map_err(debug)?;
    }
    report(&host, root, observer.requests().len(), "ready")?;
    #[cfg(windows)]
    let mut processes = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let (command_id, command) = line
            .split_once(' ')
            .ok_or("expected command-id and command")?;
        let target = if command.ends_with(" child") {
            child
                .as_ref()
                .ok_or("no child")?
                .session_configured()
                .thread_id
        } else {
            root
        };
        let verb = command.split_whitespace().next().ok_or("missing command")?;
        match verb {
            #[cfg(windows)]
            "/process" => {
                let fixture = std::env::current_exe()?
                    .parent()
                    .ok_or("missing example directory")?
                    .parent()
                    .ok_or("missing target directory")?
                    .join("vcp-process-fixture.exe");
                let tree = directory.join("native-tree");
                std::fs::create_dir(&tree)?;
                let process = host.spawn_process(
                    root,
                    &fixture,
                    &["tree".into(), tree.as_os_str().into()],
                    &workspace,
                    &Default::default(),
                    1024,
                )?;
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !tree.join("child-ready").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await?;
                if process.active_process_count()? != 2 {
                    return Err("fixture tree was not fully owned".into());
                }
                processes.push(process);
            }
            #[cfg(windows)]
            "/reconcile-process" => {
                use std::os::windows::fs::OpenOptionsExt;
                // This observer applies only to the known synthetic tree whose
                // grandchild holds this unique fixture lock throughout its life.
                let lock = directory.join("native-tree/locked");
                let _observed = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(lock)?;
                for work in host
                    .work()
                    .map_err(debug)?
                    .into_iter()
                    .filter(|work| work.receipt.is_none() && work.label == "native-process")
                {
                    host.reconcile(work.id, &host.inspect(root).map_err(debug)?.revision,
                        "synthetic tree observer: exclusive lock released after owner exit; existing fixture artifacts retained").map_err(debug)?;
                }
            }
            "/pause" | "/resume" => {
                let action = if verb == "/pause" {
                    Action::Pause
                } else {
                    Action::Resume
                };
                host.control(
                    command_id,
                    target,
                    &host.inspect(target).map_err(debug)?.revision,
                    action,
                    || {
                        if std::fs::canonicalize(&workspace).ok().as_ref() == Some(&workspace) {
                            Ok(())
                        } else {
                            Err(Error::RevalidationFailed)
                        }
                    },
                )
                .await
                .map_err(debug)?;
            }
            "/child" => {
                if child.is_some() {
                    return Err("child already exists".into());
                }
                host.authorize_startup(test.config.cwd.as_path(), None)
                    .map_err(debug)?;
                let thread = test
                    .thread_manager
                    .start_thread(StartThreadOptions {
                        session_source: Some(SessionSource::SubAgent(
                            SubAgentSource::ThreadSpawn {
                                parent_thread_id: root,
                                depth: 1,
                                agent_path: None,
                                agent_nickname: None,
                                agent_role: None,
                            },
                        )),
                        environments: Some(test.codex.environment_selections().await),
                        ..StartThreadOptions::new(test.config.clone())
                    })
                    .await?
                    .thread;
                rollouts.push(Rollout {
                    path: thread
                        .session_configured()
                        .rollout_path
                        .clone()
                        .ok_or("missing child rollout")?,
                    root: false,
                    id: thread.session_configured().thread_id,
                });
                save_rollouts(&pointer, &rollouts)?;
                host.attach_child(
                    root,
                    &host.inspect(root).map_err(debug)?.revision,
                    thread.clone(),
                )
                .map_err(debug)?;
                child = Some(thread);
            }
            "/turn" => {
                let thread = if target == root {
                    &test.codex
                } else {
                    child.as_ref().unwrap()
                };
                let result = thread
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "synthetic qualification input".into(),
                        text_elements: vec![],
                    }]))
                    .await?;
                if matches!(result, TurnInputSubmission::Started { .. }) {
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        wait_for_event(thread, |event| matches!(event, EventMsg::TurnComplete(_))),
                    )
                    .await?;
                }
            }
            "/status" => {}
            "/crash" => {
                report(&host, target, observer.requests().len(), "crashing")?;
                std::process::exit(77);
            }
            "/quit" => break,
            _ => return Err("unknown private fixture command".into()),
        }
        report(&host, target, observer.requests().len(), command_id)?;
    }
    owner.close().await.map_err(debug)?;
    if let Some(child) = child {
        child.shutdown_and_wait().await?;
    }
    test.codex.shutdown_and_wait().await?;
    if observer.requests().len() != expected_requests {
        return Err("unexpected provider request count".into());
    }
    Ok(())
}

fn save_rollouts(path: &std::path::Path, rollouts: &[Rollout]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(&serde_json::to_vec(rollouts)?)?;
    file.sync_all()
}

fn debug(error: Error) -> std::io::Error {
    std::io::Error::other(format!("{error:?}"))
}
fn report(
    host: &Lifecycle,
    thread: ThreadId,
    requests: usize,
    command: &str,
) -> std::io::Result<()> {
    let view = host.inspect(thread).map_err(debug)?;
    println!(
        "VCP_FIXTURE {}",
        serde_json::json!({"command":command,"thread":thread,"paused":view.local_hold || view.inherited_hold,
        "requests":requests,"work":host.work().map_err(debug)?.len(), "unresolved":view.unresolved_work})
    );
    std::io::stdout().flush()
}
