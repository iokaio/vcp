// SPDX-License-Identifier: Apache-2.0
//! Opt-in replay benchmark; never open live user state with this helper.
use vcp_store::{BackendKind, Store};

#[tokio::test]
#[ignore = "requires VCP_STORE_BENCHMARK_ROOT pointing to a disposable canonical copy"]
async fn benchmark_retained_history() {
    let root = std::env::var_os("VCP_STORE_BENCHMARK_ROOT")
        .expect("explicit disposable canonical directory required");
    let started = std::time::Instant::now();
    let store = Store::open(std::path::Path::new(&root), BackendKind::Sqlite, &[])
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let state = store.state();
    println!(
        "replay_seconds={:.6} watermark={} records={} events={} state_sha256={}",
        elapsed.as_secs_f64(),
        state.watermark.get(),
        state.records.len(),
        state.events.len(),
        vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(state).unwrap())
    );
    store.close().await.unwrap();
}
