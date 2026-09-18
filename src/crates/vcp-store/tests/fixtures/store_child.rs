// SPDX-License-Identifier: Apache-2.0
#[path = "../common/mod.rs"]
mod common;
use std::{io::Write, path::PathBuf, sync::Arc};
use vcp_store::{contract::CanonicalStore, BackendKind, Barrier, Store};
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(run());
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(32);
    }
}
async fn run() -> vcp_store::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(vcp_store::Error::Incompatible);
    }
    let root = PathBuf::from(&args[1]);
    let kind: BackendKind = args[2].parse()?;
    let barrier = match args[3].as_str() {
        "prepared" => Barrier::Prepared,
        "before_commit" => Barrier::BeforeCommit,
        "after_commit" => Barrier::AfterCommit,
        "before_reply" => Barrier::BeforeReply,
        "before_validation" => Barrier::BeforeValidation,
        "before_activation" => Barrier::BeforeActivation,
        "after_activation" => Barrier::AfterActivation,
        _ => return Err(vcp_store::Error::Incompatible),
    };
    let marker = PathBuf::from(&args[4]);
    let observer = Arc::new(move |point| {
        if point == barrier {
            let mut file = std::fs::File::create(&marker).unwrap();
            file.write_all(format!("{point:?}").as_bytes()).unwrap();
            file.sync_all().unwrap();
            // The independent supervisor terminates this real process. The child
            // cannot fake successful crash recovery by returning through Drop.
            loop {
                std::thread::park();
            }
        }
    });
    if matches!(
        barrier,
        Barrier::BeforeValidation | Barrier::BeforeActivation | Barrier::AfterActivation
    ) {
        let mut active = vcp_store::migration::ActiveRoot::open(&root, Some(kind), &[]).await?;
        active.store_mut()?.transact(common::initial()).await?;
        active.observe(observer);
        active
            .switch(if kind == BackendKind::Sqlite {
                BackendKind::Files
            } else {
                BackendKind::Sqlite
            })
            .await?;
        return Ok(());
    }
    let mut store = Store::open(&root, kind, &[]).await?;
    store.observe(observer);
    store.transact(common::initial()).await?;
    Ok(())
}
