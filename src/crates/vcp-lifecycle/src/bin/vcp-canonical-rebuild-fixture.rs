// SPDX-License-Identifier: Apache-2.0
//! Read-only synthetic recovery observer; it cannot invoke a retained controller.
use std::{io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("root/backend/workspace/watermark/output required".into());
    }
    let root = PathBuf::from(&args[1]);
    let kind = args[2].parse()?;
    let workspace = vcp_domain::WorkspaceId::parse(args[3].clone())?;
    let watermark = vcp_domain::Watermark::new(args[4].parse()?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let store = runtime.block_on(vcp_store::Store::open(&root, kind, &[]))?;
    let view = vcp_audit::projection::rebuild(store.state(), &workspace, 2, watermark)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[5])?;
    file.write_all(&vcp_protocol::canonical_bytes(&view)?)?;
    file.sync_all()?;
    Ok(())
}
