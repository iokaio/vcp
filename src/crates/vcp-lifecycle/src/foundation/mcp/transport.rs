// SPDX-License-Identifier: Apache-2.0
//! Private typed MCP wire adapter. The host owns per-call intent and outcome handling.
use crate::foundation::execution::DuplexProcess;
use tokio::time::{timeout_at, Instant};
use vcp_extensions::mcp::client::{Client, Incoming, Outbound};

/// All requests, initialized notifications and callback rejections use the same
/// canonical duplex writer. Only a successfully written notification advances readiness.
pub(super) async fn send(
    process: &mut DuplexProcess,
    client: &mut Client,
    outbound: &Outbound,
    deadline: Instant,
) -> Result<(), String> {
    client
        .validate_send(outbound)
        .map_err(|error| error.to_string())?;
    if Instant::now() >= deadline {
        return Err("MCP request deadline elapsed before send".into());
    }
    timeout_at(deadline, process.write_line(outbound.bytes()))
        .await
        .map_err(|_| "MCP request deadline elapsed during send".to_owned())??;
    client
        .confirm_sent(outbound)
        .map_err(|error| error.to_string())
}

/// Tool payloads additionally carry a host-owned current source/intent fence,
/// executed in the same canonical worker operation as input artifact capture.
pub(super) async fn send_checked<F>(
    process: &mut DuplexProcess,
    client: &mut Client,
    outbound: &Outbound,
    deadline: Instant,
    fence: F,
) -> Result<(), String>
where
    F: FnOnce(
            &mut crate::foundation::worker::Context,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
        + Send
        + 'static,
{
    client
        .validate_send(outbound)
        .map_err(|error| error.to_string())?;
    if Instant::now() >= deadline {
        return Err("MCP request deadline elapsed before send".into());
    }
    timeout_at(
        deadline,
        process.write_line_checked(outbound.bytes(), fence),
    )
    .await
    .map_err(|_| "MCP request deadline elapsed during send".to_owned())??;
    client
        .confirm_sent(outbound)
        .map_err(|error| error.to_string())
}

/// One frame only. The caller bounds control-message counts and reuses the same
/// absolute deadline; incoming notifications never reset the request timeout.
pub(super) async fn receive(
    process: &mut DuplexProcess,
    client: &mut Client,
    deadline: Instant,
) -> Result<Incoming, String> {
    if Instant::now() >= deadline {
        return Err("MCP request deadline elapsed before receive".into());
    }
    let frame = timeout_at(deadline, process.read_line())
        .await
        .map_err(|_| "MCP request deadline elapsed awaiting response".to_owned())??
        .ok_or("MCP peer closed stdout before response")?;
    client.receive(&frame).map_err(|error| error.to_string())
}
