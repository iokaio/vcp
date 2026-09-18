// SPDX-License-Identifier: Apache-2.0
use std::{io::Write, path::PathBuf, sync::Arc};
use vcp_domain::{Watermark, WorkspaceId};
use vcp_store::{BackendKind, Barrier, Store};
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run()) {
        eprintln!("{error}");
        std::process::exit(32);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 && args.len() != 7 {
        return Err(
            "root/backend/workspace/watermark-or-activate/output/[barrier] required".into(),
        );
    }
    let root = PathBuf::from(&args[1]);
    let kind: BackendKind = args[2].parse()?;
    let workspace = WorkspaceId::parse(args[3].clone())?;
    let output = PathBuf::from(&args[5]);
    let mut store = Store::open(&root, kind, &[]).await?;
    let view = if args[4] == "activate" {
        if args.len() != 7 {
            return Err("activation barrier required".into());
        }
        let barrier = match args[6].as_str() {
            "before_commit" => Barrier::BeforeCommit,
            "after_commit" => Barrier::AfterCommit,
            _ => return Err("unknown barrier".into()),
        };
        store.observe(Arc::new(move |point| {
            if point == barrier {
                let mut file = std::fs::File::create(&output).unwrap();
                file.write_all(b"projection barrier reached").unwrap();
                file.sync_all().unwrap();
                loop {
                    std::thread::park();
                }
            }
        }));
        vcp_audit::projection::publish(&mut store, &workspace, 2).await?
    } else {
        let watermark = Watermark::new(args[4].parse()?);
        let view = vcp_audit::projection::rebuild(store.state(), &workspace, 1, watermark)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&vcp_protocol::canonical_bytes(&view)?)?;
        file.sync_all()?;
        view
    };
    println!("{}", view.semantic_digest()?);
    Ok(())
}
