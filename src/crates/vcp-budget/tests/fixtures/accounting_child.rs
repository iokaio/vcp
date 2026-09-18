// SPDX-License-Identifier: Apache-2.0
use std::{io::Write, path::PathBuf, sync::Arc};
use vcp_store::{
    contract::{CanonicalStore, Transaction},
    BackendKind, Barrier, Store,
};
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
    if args.len() != 6 {
        return Err("root/backend/transaction/barrier/marker required".into());
    }
    let root = PathBuf::from(&args[1]);
    let kind: BackendKind = args[2].parse()?;
    let transaction: Transaction = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let barrier = match args[4].as_str() {
        "prepared" => Barrier::Prepared,
        "before_commit" => Barrier::BeforeCommit,
        "after_commit" => Barrier::AfterCommit,
        "before_reply" => Barrier::BeforeReply,
        _ => return Err("unknown barrier".into()),
    };
    let marker = PathBuf::from(&args[5]);
    let mut store = Store::open(&root, kind, &[]).await?;
    store.observe(Arc::new(move |point| {
        if point == barrier {
            let mut file = std::fs::File::create(&marker).unwrap();
            file.write_all(b"accounting barrier reached").unwrap();
            file.sync_all().unwrap();
            loop {
                std::thread::park();
            }
        }
    }));
    store.transact(transaction).await?;
    Ok(())
}
