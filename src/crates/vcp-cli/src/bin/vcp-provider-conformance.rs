// SPDX-License-Identifier: Apache-2.0
//! Explicit historical qualification entry point; production uses `vcp setup provider`.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    vcp_cli::provider_setup::legacy_run(std::env::args_os().skip(1).collect()).await
}
