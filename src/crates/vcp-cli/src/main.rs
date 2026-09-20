// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use vcp_cli::{
    args::Cli,
    exit_status::Conditions,
    jsonl::{Jsonl, Payload},
};
use vcp_domain::ids::CommandId;

// Match the retained engine's native bootstrap stack contract (codex-arg0 and
// codex-async-utils). Its configuration/startup paths exceed Windows' default
// main-thread stack even when their async state is boxed.
const RETAINED_STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() -> std::process::ExitCode {
    let result = std::thread::Builder::new()
        .name("vcp-main".into())
        .stack_size(RETAINED_STACK_BYTES)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .thread_stack_size(RETAINED_STACK_BYTES)
                .enable_all()
                .build()
                .map(|runtime| runtime.block_on(Box::pin(run())))
                .map_err(|error| format!("CLI runtime startup failed: {error}"))
        })
        .map_err(|error| format!("CLI thread startup failed: {error}"))
        .and_then(|thread| match thread.join() {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        });
    match result {
        Ok(code) => code,
        Err(error) => configuration_failure(&error),
    }
}

async fn run() -> std::process::ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let help = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            eprint!("{error}");
            if help {
                return std::process::ExitCode::SUCCESS;
            }
            let conditions = Conditions {
                invalid_configuration: true,
                ..Default::default()
            };
            let _ = Jsonl::new(std::io::stdout()).emit(
                &CommandId::new(),
                None,
                Payload::Result {
                    conditions: &conditions,
                    exit_code: 2,
                    receipt: None,
                },
            );
            return 2.into();
        }
    };
    #[cfg(windows)]
    // Keep the retained host/maintenance dispatcher out of the native main
    // future, including for commands that only print help.
    let result = Box::pin(vcp_cli::app::run(cli)).await;
    #[cfg(not(windows))]
    let result: Result<u8, String> = {
        let _ = cli;
        Err("native CLI execution currently requires Windows".into())
    };
    match result {
        Ok(code) => code.into(),
        Err(error) => configuration_failure(&error),
    }
}

fn configuration_failure(error: &str) -> std::process::ExitCode {
    eprintln!("vcp: {error}");
    let conditions = Conditions {
        invalid_configuration: true,
        ..Default::default()
    };
    let _ = Jsonl::new(std::io::stdout()).emit(
        &CommandId::new(),
        None,
        Payload::Result {
            conditions: &conditions,
            exit_code: 2,
            receipt: None,
        },
    );
    2.into()
}
