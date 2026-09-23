// SPDX-License-Identifier: Apache-2.0
//! Explicit local inference commands, separate from read-only memory search.
use clap::Args;
use std::path::PathBuf;

#[derive(Clone, Debug, Args)]
pub struct Build {
    /// Provisioned, digest-verified local model directory. Never downloads assets.
    #[arg(long, value_name = "DIRECTORY")]
    pub assets: PathBuf,
    /// Permit lexical-only publication when local embedding assets fail.
    #[arg(long)]
    pub allow_lexical_only: bool,
}

#[derive(Clone, Debug, Args)]
pub struct Query {
    #[command(flatten)]
    pub search: crate::args::Search,
    /// Provisioned, digest-verified local model directory. Never downloads assets.
    #[arg(long, value_name = "DIRECTORY")]
    pub assets: PathBuf,
}

#[cfg(windows)]
pub async fn execute(
    command: &crate::args::ValidatedCommand,
    entry: &crate::settings::WorkspaceEntry,
) -> Result<(u8, serde_json::Value), String> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use vcp_lifecycle::foundation::local_memory::open_local_memory;

    let (memory, owner) = open_local_memory(entry.config.clone())?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let operation = async {
        match command {
            crate::args::ValidatedCommand::MemoryBuild(build) => {
                memory
                    .build(
                        build.assets.clone(),
                        build.allow_lexical_only,
                        cancelled.clone(),
                    )
                    .await
            }
            crate::args::ValidatedCommand::MemoryQuery(query) => {
                memory
                    .query(
                        query.assets.clone(),
                        query.search.request(entry.config.workspace.clone()),
                        cancelled.clone(),
                    )
                    .await
            }
            _ => Err("unsupported local memory command".into()),
        }
    };
    tokio::pin!(operation);
    let result = tokio::select! {
        result = &mut operation => result,
        signal = tokio::signal::ctrl_c() => {
            cancelled.store(true, Ordering::Release);
            // Native inference stops between bounded units. Keep the owner alive
            // until work drains and resource observations have been retained.
            let result = operation.await;
            match signal {
                Ok(()) => result,
                Err(error) => Err(error.to_string()),
            }
        }
    };
    owner.close().await?;
    result.map(|value| {
        let success = matches!(
            value["status"].as_str(),
            Some("published" | "lexical_only" | "queried")
        );
        (if success { 0 } else { 1 }, value)
    })
}
