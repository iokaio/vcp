// SPDX-License-Identifier: Apache-2.0
//! Bounded structured input for an already authenticated owning process.
use std::io::{self, BufRead};
use tokio::sync::mpsc;
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_protocol::{
    command::{CommandEnvelope, CommandReceipt},
    version::MAX_COMMAND_BYTES,
};

/// Reads exactly one newline-terminated command, without allocating for an
/// unbounded line. EOF between records is clean; EOF inside a record is an
/// error, so a truncated control write can never execute.
pub fn read_command(reader: &mut impl BufRead) -> io::Result<Option<CommandEnvelope>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated control command",
                ))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if frame.len().saturating_add(count) > MAX_COMMAND_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "control command exceeds input limit",
            ));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            return CommandEnvelope::parse_jsonl(&frame).map(Some).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid structured control command",
                )
            });
        }
    }
}

pub struct ControlReply {
    pub command: vcp_domain::ids::CommandId,
    pub result: Result<CommandReceipt, String>,
}

/// The reader owns no engine or lifecycle capability. Its bounded mailbox and
/// the handler are separate from the output writer, so a blocked stdout cannot
/// prevent a delivered pause from reaching the owner. This is a private input
/// adapter, not an attach server: callers must inherit the owner's input handle.
pub struct ControlInput {
    receiver: mpsc::Receiver<io::Result<CommandEnvelope>>,
}

impl ControlInput {
    pub fn new<R: BufRead + Send + 'static>(mut reader: R) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(1);
        std::thread::Builder::new()
            .name("vcp-cli-input".into())
            .spawn(move || loop {
                let result = match read_command(&mut reader) {
                    Ok(Some(command)) => Ok(command),
                    Ok(None) => break,
                    Err(error) => Err(error),
                };
                let failed = result.is_err();
                if sender.blocking_send(result).is_err() || failed {
                    break;
                }
            })?;
        Ok(Self { receiver })
    }

    /// Scope, owner epoch, revision, and idempotency are checked by the same
    /// canonical stop path used by in-process controls. Other mutations are
    /// rejected there before touching the retained runtime.
    pub async fn next(&mut self, host: &CanonicalHost) -> io::Result<Option<ControlReply>> {
        match self.receiver.recv().await {
            Some(Ok(command)) => Ok(Some(ControlReply {
                command: command.id.clone(),
                result: host.stop(command),
            })),
            Some(Err(error)) => Err(error),
            None => Ok(None),
        }
    }
}
