// SPDX-License-Identifier: Apache-2.0
//! Stable installed CLI entry point. Editor clients select the resolved native
//! engine, preserving the local bridge's existing parent/child ownership model.
#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("VCP installation: {error}");
        std::process::exit(2);
    }
}

#[cfg(windows)]
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--launcher-version" {
        println!("vcp-launch {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| "launcher location unavailable")?;
    let base = executable
        .parent()
        .ok_or("launcher has no installation directory")?;
    let selected = vcp_cli::installation::select(&base.join("engine"))?;
    if args.len() == 1 && args[0] == "--resolve-installation" {
        println!(
            "{}",
            serde_json::json!({"schema":"vcp-installed-engine/1",
            "executable":selected.executable,"data_directory":selected.data})
        );
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|arg| arg == "local-bridge" || arg == "local-server")
    {
        return Err("editor clients must select the native executable and data_directory returned by --resolve-installation".into());
    }
    // A custom handler is not inherited by the child. The native CLI receives
    // the console event and owns durable pause/termination; this wrapper waits
    // for the real outcome instead of exiting before it.
    unsafe extern "system" fn control(event: u32) -> i32 {
        i32::from(event == 0 || event == 1)
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    // SAFETY: static function pointer with Windows' documented signature.
    if unsafe { SetConsoleCtrlHandler(Some(control), 1) } == 0 {
        return Err("cannot establish console cancellation forwarding".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "launcher runtime unavailable")?;
    let status = runtime.block_on(async {
        let job = codex_utils_pty::JobObject::create_without_breakaway()
            .map_err(|_| "native engine process containment unavailable")?;
        let mut command = tokio::process::Command::new(&selected.executable);
        command.args(args);
        // The child cannot run before containment. Closing this launcher's job
        // handle (including forced termination) stops the engine and descendants.
        let mut child = job.spawn_contained(&mut command).map_err(|_| {
            "contained native engine launch failed; repair or retry after installation completes"
        })?;
        child
            .wait()
            .await
            .map_err(|_| "native engine outcome unavailable")
    })?;
    let code = status
        .code()
        .ok_or("native engine terminated without an exit status")?;
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("VCP installation requires native Windows");
    std::process::exit(2);
}
