// SPDX-License-Identifier: Apache-2.0
//! Disposable native console owner used only by the recovery acceptance test.
#[cfg(windows)]
#[path = "../../tests/support/recovery_config.rs"]
mod recovery_config;
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use vcp_domain::{effect::EffectState, ids::*, revision::*, task::*};
    use vcp_lifecycle::foundation::CanonicalHost;
    use vcp_protocol::command::Command;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> isize;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(window: isize, command: i32) -> i32;
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("fixture root and backend required".into());
    }
    let root = std::path::PathBuf::from(&args[0]);
    if args[1] == "child" {
        use std::os::windows::fs::OpenOptionsExt;
        let _lock = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0)
            .open(root.join("child-lock"))?;
        std::fs::write(root.join("child-ready"), b"ready")?;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
    let backend = if args[1] == "files" {
        vcp_store::BackendKind::Files
    } else {
        vcp_store::BackendKind::Sqlite
    };
    let workspace = root.join("workspace");
    std::fs::create_dir(&workspace)?;
    let config = recovery_config::config(&root.join("canonical"), &workspace, backend);
    let (host, _owner) = CanonicalHost::open(config.clone())?;
    let _console = host.install_console_close_handler()?;
    let window = unsafe { GetConsoleWindow() };
    if window == 0 {
        return Err("fixture requires a real Windows console".into());
    }
    unsafe {
        ShowWindow(window, 0);
    }
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "console recovery".into(),
                constraints: vec![],
                acceptance: vec!["no replay".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: vcp_domain::verification::Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )?;
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "native console started".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )?;
    let effect = ToolRunId::new();
    let execution = ExecutionId::new();
    host.command(
        Command::ProposeEffect {
            id: effect.clone(),
            operation_digest: "a".repeat(64),
        },
        Some(config.root_task.clone()),
        Revision::new(1),
    )?;
    for (index, next) in [
        EffectState::Validated,
        EffectState::Authorized,
        EffectState::DispatchRecorded,
        EffectState::Running,
    ]
    .into_iter()
    .enumerate()
    {
        host.command(
            Command::AdvanceEffect {
                id: effect.clone(),
                next,
                reason: "durable non-idempotent marker fixture".into(),
                execution: Some(execution.clone()),
                exit_code: None,
                observed_changes: vec![],
            },
            Some(config.root_task.clone()),
            Revision::new(index as u64),
        )?;
    }
    use std::io::Write;
    let mut marker = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(workspace.join("non-idempotent-marker"))?;
    marker.write_all(b"effect\n")?;
    marker.sync_all()?;
    drop(marker);
    // Use the same native containment primitive as the host. Both graceful
    // console closure and a hard kill must release the entire owned job.
    let _job = codex_utils_pty::JobObject::create_without_breakaway()?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .arg(&root)
        .arg("child")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _child = _job.spawn_contained(&mut command)?;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !root.join("child-ready").exists() {
        if std::time::Instant::now() > until {
            return Err("contained child did not start".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    std::fs::write(root.join("ready"), window.to_string())?;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("native Windows console required");
    std::process::exit(3);
}
