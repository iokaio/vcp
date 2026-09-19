// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use vcp_cli::{
    args::Cli,
    exit_status::Conditions,
    jsonl::{Jsonl, Payload},
};
use vcp_domain::ids::CommandId;

#[tokio::main]
async fn main() -> std::process::ExitCode {
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
    let result = vcp_cli::app::run(cli).await;
    #[cfg(not(windows))]
    let result: Result<u8, String> = {
        let _ = cli;
        Err("native CLI execution currently requires Windows".into())
    };
    match result {
        Ok(code) => code.into(),
        Err(error) => {
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
    }
}
