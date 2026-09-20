// SPDX-License-Identifier: Apache-2.0
use crate::exit_status::Conditions;
use serde::Serialize;
use std::io::{self, Write};
use vcp_domain::{ids::*, workspace::Scope};
use vcp_protocol::{command::CommandReceipt, event::EventEnvelope};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct Envelope<'a> {
    pub schema_version: u32,
    pub correlation: &'a CommandId,
    pub scope: Option<&'a Scope>,
    #[serde(flatten)]
    pub payload: Payload<'a>,
}
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload<'a> {
    Accepted {
        receipt: &'a CommandReceipt,
    },
    Event {
        event: &'a EventEnvelope,
    },
    RequiredInput {
        approval: &'a ApprovalId,
    },
    CursorGap {
        reason: &'a str,
    },
    RetentionNotice {
        data: &'a serde_json::Value,
    },
    #[serde(rename = "result")]
    CommandResult {
        exit_code: u8,
        data: &'a serde_json::Value,
    },
    Result {
        conditions: &'a Conditions,
        exit_code: u8,
        receipt: Option<&'a CommandReceipt>,
    },
}

/// One bounded record at a time. A failed write permanently closes this stream;
/// the owner must handle the returned error as loss of its output consumer.
pub struct Jsonl<W> {
    output: W,
    failed: bool,
    finished: bool,
}
impl<W: Write> Jsonl<W> {
    pub fn new(output: W) -> Self {
        Self {
            output,
            failed: false,
            finished: false,
        }
    }
    pub fn emit(
        &mut self,
        correlation: &CommandId,
        scope: Option<&Scope>,
        payload: Payload<'_>,
    ) -> io::Result<()> {
        if self.failed || self.finished {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "JSONL stream closed",
            ));
        }
        if let Payload::Result {
            conditions,
            exit_code,
            receipt,
        } = &payload
        {
            if *exit_code != conditions.code()
                || ((conditions.completed
                    || conditions.durably_paused
                    || conditions.required_input
                    || conditions.cancelled
                    || conditions.budget_exhausted
                    || conditions.unresolved_effect
                    || conditions.incomplete)
                    && (scope.is_none() || receipt.is_none()))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "result requires consistent conditions and durable references",
                ));
            }
        }
        let final_record = matches!(
            payload,
            Payload::Result { .. } | Payload::CommandResult { .. }
        );
        let result = (|| {
            serde_json::to_writer(
                &mut self.output,
                &Envelope {
                    schema_version: SCHEMA_VERSION,
                    correlation,
                    scope,
                    payload,
                },
            )?;
            self.output.write_all(b"\n")?;
            self.output.flush()
        })();
        if result.is_err() {
            self.failed = true;
        }
        if result.is_ok() && final_record {
            self.finished = true;
        }
        result
    }
}

impl Jsonl<Vec<u8>> {
    pub(crate) fn take_frame(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}
