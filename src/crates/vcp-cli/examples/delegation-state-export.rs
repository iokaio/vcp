// SPDX-License-Identifier: Apache-2.0
//! Qualification-only snapshot after the terminal owner has closed. Opening
//! the canonical host applies ordinary recovery fences; it never resumes work.
use std::{fs, io::Write, path::PathBuf};
use vcp_lifecycle::foundation::CanonicalHost;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let entry_path = PathBuf::from(args.next().ok_or("workspace descriptor required")?);
    let output = PathBuf::from(args.next().ok_or("new snapshot file required")?);
    if args.next().is_some() {
        return Err("only workspace descriptor and new snapshot file accepted".into());
    }
    let entry: vcp_cli::settings::WorkspaceEntry = serde_json::from_slice(&fs::read(entry_path)?)?;
    let (host, owner) = CanonicalHost::open(entry.config)?;
    let state = host.snapshot()?;
    owner.close().await?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    file.write_all(&vcp_protocol::canonical_bytes(&state)?)?;
    file.sync_all()?;
    Ok(())
}
