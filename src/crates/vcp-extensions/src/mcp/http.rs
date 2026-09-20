// SPDX-License-Identifier: Apache-2.0
//! Bounded POST-response framing only. Caller owns headers, bytes, clock, capture,
//! transport cancellation and authority. No reconnect, replay or HTTP operations.
//!
//! Transport profile: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>.
//! SSE framing: <https://html.spec.whatwg.org/multipage/server-sent-events.html#parsing-an-event-stream>.
//! This implements a bounded subset, not a complete Streamable HTTP client.
//! It accepts only 200 JSON/SSE request responses, 202 empty acknowledgements,
//! optional UTF-8 charset, and default/message SSE event types. Other successful
//! HTTP statuses and MIME parameters are outside this declared profile. Invalid
//! UTF-8 is rejected rather than decoded with browser replacement characters.
//! `Session` is the HTTP state adapter; the standalone Client remains stdio-only.
//! The host integrating this adapter must preserve the following boundaries:
//! - Actual request-write observation must precede Client::confirm_sent; receiving
//!   HTTP response headers is not proof the entire request was written.
//! - Initialized/control POSTs require 202 with an empty body; do not permit the
//!   next client request merely because confirm_sent advanced its local phase.
//! - Process each yielded frame before reading the next. Pending callback replies
//!   require separately admitted POSTs and acknowledgements before Client::receive
//!   can consume subsequent JSON frames from the original response stream.
//! - A valid reply and a later transport error are separate observations; retain
//!   the reply's evidence. EOF/202/metadata alone are never a tool-result receipt.
//! - Session owns private session IDs and protocol headers. Host owns current
//!   source/credential authority and durable effects. No frame permits replay.
mod session;
pub use session::{ExchangeId, RequestHeaders, Session, SessionError};
use std::{
    fmt,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseTo {
    Request,
    NotificationOrResponse,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub body_bytes: usize,
    pub frame_bytes: usize,
    pub line_bytes: usize,
    pub lines: usize,
    /// All completed blocks, including empty/comment-only blocks.
    pub events: usize,
    pub metadata_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            body_bytes: 8 * 1024 * 1024,
            frame_bytes: 1024 * 1024,
            line_bytes: 1024 * 1024 + 16,
            lines: 16_384,
            events: 1024,
            metadata_bytes: 1024,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self, Error> {
        if self.body_bytes == 0
            || self.body_bytes > 16 * 1024 * 1024
            || self.frame_bytes == 0
            || self.frame_bytes > 1024 * 1024
            || self.frame_bytes > self.body_bytes
            || self.line_bytes == 0
            || self.line_bytes > 1024 * 1024 + 16
            || self.lines == 0
            || self.lines > 65_536
            || self.events == 0
            || self.events > 4096
            || self.metadata_bytes == 0
            || self.metadata_bytes > 4096
        {
            return Err(Error::Limits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid HTTP framing limits")]
    Limits,
    #[error("HTTP status is outside this response profile")]
    Status,
    #[error("HTTP media type is outside this response profile")]
    MediaType,
    #[error("HTTP response exceeds framing bounds")]
    Bounds,
    #[error("HTTP response contains invalid UTF-8")]
    Utf8,
    #[error("HTTP response exceeded its absolute deadline")]
    Deadline,
    #[error("HTTP decoder is closed or clock moved backwards")]
    State,
    #[error("HTTP acknowledgement contains a body")]
    AcknowledgementBody,
    #[error("HTTP JSON response is empty")]
    Empty,
    #[error("SSE event type is outside the MCP message profile")]
    EventType,
}

/// Untrusted metadata, never authorization or an instruction to send anything.
/// Observed IDs persist across blocks; Some("") represents an explicit reset. Retry is
/// decimal text, preserving arbitrarily large values within metadata bounds.
/// This is parser metadata, not a committed Last-Event-ID checkpoint: an ID field
/// can be observed before the block's terminating blank line.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub last_event_id: Option<String>,
    pub retry_milliseconds: Option<String>,
}
impl fmt::Debug for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metadata")
            .field("has_id", &self.last_event_id.is_some())
            .field("has_retry", &self.retry_milliseconds.is_some())
            .finish()
    }
}

/// JSON bytes are only candidates: pass them to the unchanged Client.receive
/// before accepting any protocol meaning. Metadata-only frames must not be fed
/// to that codec. No raw external values appear in Debug.
pub enum Frame {
    Json {
        bytes: Vec<u8>,
        metadata: Option<Metadata>,
    },
    Metadata(Metadata),
}
impl Frame {
    pub fn json_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Json { bytes, .. } => Some(bytes),
            Self::Metadata(_) => None,
        }
    }
}
impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json { bytes, metadata } => f
                .debug_struct("JsonFrame")
                .field("bytes", &bytes.len())
                .field("metadata", metadata)
                .finish(),
            Self::Metadata(metadata) => f.debug_tuple("MetadataFrame").field(metadata).finish(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub body_bytes: usize,
    pub lines: usize,
    pub events: usize,
    pub messages: usize,
}
#[derive(Debug)]
pub struct End {
    /// Only application/json emits its single frame at EOF.
    pub frame: Option<Frame>,
    pub summary: Summary,
    /// Incomplete SSE blocks are discarded, never promoted to JSON messages.
    pub discarded_partial_event: bool,
    /// Observed fields, possibly from the discarded partial block. Never use as
    /// authorization or silently turn this into a resume request.
    pub metadata: Metadata,
}
/// One bounded parsing step. Process the frame before feeding the unconsumed
/// suffix. This preserves already observed replies if later input is malformed.
#[derive(Debug)]
pub struct Step {
    pub consumed: usize,
    pub frame: Option<Frame>,
}
#[derive(Clone, Copy)]
enum Mode {
    Json,
    Sse,
    Acknowledgement,
}
pub struct Decoder {
    mode: Mode,
    limits: Limits,
    deadline: Instant,
    last_now: Instant,
    failed: bool,
    summary: Summary,
    line: Vec<u8>,
    data: Vec<u8>,
    event_type: String,
    metadata: Metadata,
    metadata_changed: bool,
    first_line: bool,
    skip_lf: bool,
    partial_block: bool,
}
impl Decoder {
    /// Caller must reject duplicate Content-Type fields before passing the one
    /// field value here. Header allocation/count limits belong to HTTP transport.
    /// A pure decoder cannot wake itself: host must also time out stalled reads.
    pub fn new(
        response_to: ResponseTo,
        status: u16,
        content_type: Option<&str>,
        limits: Limits,
        now: Instant,
        deadline: Instant,
    ) -> Result<Self, Error> {
        let limits = limits.validate()?;
        if deadline <= now {
            return Err(Error::Deadline);
        }
        if deadline.duration_since(now) > Duration::from_secs(300) {
            return Err(Error::Limits);
        }
        let mode = match response_to {
            ResponseTo::NotificationOrResponse if status == 202 => {
                // Content type has no semantics for the required empty body.
                if content_type.is_some_and(|value| value.len() > 128) {
                    return Err(Error::Bounds);
                }
                Mode::Acknowledgement
            }
            ResponseTo::Request if status == 200 => {
                media_type(content_type.ok_or(Error::MediaType)?)?
            }
            _ => return Err(Error::Status),
        };
        Ok(Self {
            mode,
            limits,
            deadline,
            last_now: now,
            failed: false,
            summary: Summary::default(),
            line: Vec::new(),
            data: Vec::new(),
            event_type: String::new(),
            metadata: Metadata::default(),
            metadata_changed: false,
            first_line: true,
            skip_lf: false,
            partial_block: false,
        })
    }

    /// Returns at most one complete frame and how many input bytes were consumed.
    /// On any error this decoder becomes unusable. Keep input and framed output
    /// in private bounded memory, then parse and sanitize complete capture values
    /// before persistence or display. Redacting individual transport fragments is
    /// insufficient for split secrets or JSON escapes. Framing never authorizes
    /// raw persistence; malformed or incomplete input needs a safe omission path.
    /// Limits count consumed bytes. The adapter must independently bound all
    /// received bytes, including a suffix it chooses not to feed after a reply.
    pub fn push(&mut self, bytes: &[u8], now: Instant) -> Result<Step, Error> {
        let result = self.push_inner(bytes, now);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn check_deadline(&mut self, now: Instant) -> Result<(), Error> {
        if self.failed {
            return Err(Error::State);
        }
        let error = if now < self.last_now {
            Some(Error::State)
        } else if now >= self.deadline {
            Some(Error::Deadline)
        } else {
            None
        };
        if let Some(error) = error {
            self.failed = true;
            return Err(error);
        }
        self.last_now = now;
        Ok(())
    }
    pub fn summary(&self) -> Summary {
        self.summary
    }

    fn push_inner(&mut self, bytes: &[u8], now: Instant) -> Result<Step, Error> {
        self.check_deadline(now)?;
        let mut frame = None;
        match self.mode {
            Mode::Acknowledgement if !bytes.is_empty() => return Err(Error::AcknowledgementBody),
            Mode::Acknowledgement => (),
            Mode::Json => {
                let next = self
                    .summary
                    .body_bytes
                    .checked_add(bytes.len())
                    .ok_or(Error::Bounds)?;
                if next > self.limits.frame_bytes || next > self.limits.body_bytes {
                    return Err(Error::Bounds);
                }
                self.summary.body_bytes = next;
                self.data.extend_from_slice(bytes);
            }
            Mode::Sse => {
                for (index, &byte) in bytes.iter().enumerate() {
                    if self.summary.body_bytes >= self.limits.body_bytes {
                        return Err(Error::Bounds);
                    }
                    self.summary.body_bytes += 1;
                    if self.skip_lf {
                        self.skip_lf = false;
                        if byte == b'\n' {
                            continue;
                        }
                    }
                    if byte == b'\r' || byte == b'\n' {
                        self.process_line(&mut frame)?;
                        self.skip_lf = byte == b'\r';
                        if let Some(frame) = frame.take() {
                            return Ok(Step {
                                consumed: index + 1,
                                frame: Some(frame),
                            });
                        }
                    } else {
                        if self.line.len() >= self.limits.line_bytes {
                            return Err(Error::Bounds);
                        }
                        self.line.push(byte);
                    }
                }
            }
        }
        Ok(Step {
            consumed: bytes.len(),
            frame: None,
        })
    }

    fn process_line(&mut self, frame: &mut Option<Frame>) -> Result<(), Error> {
        if self.summary.lines >= self.limits.lines {
            return Err(Error::Bounds);
        }
        self.summary.lines += 1;
        let line = std::mem::take(&mut self.line);
        let mut text = std::str::from_utf8(&line).map_err(|_| Error::Utf8)?;
        if self.first_line {
            text = text.strip_prefix('\u{feff}').unwrap_or(text);
            self.first_line = false;
        }
        if text.is_empty() {
            return self.finish_event(frame);
        }
        self.partial_block = true;
        if text.starts_with(':') {
            return Ok(());
        }
        let (field, value) = text.split_once(':').unwrap_or((text, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                let size = self
                    .data
                    .len()
                    .checked_add(value.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Error::Bounds)?;
                // One trailing LF is removed at dispatch, so allow that one byte.
                if size > self.limits.frame_bytes + 1 {
                    return Err(Error::Bounds);
                }
                self.data.extend_from_slice(value.as_bytes());
                self.data.push(b'\n');
            }
            "event" => {
                if value.len() > self.limits.metadata_bytes {
                    return Err(Error::Bounds);
                }
                self.event_type = value.into();
            }
            "id" if !value.contains('\0') => {
                if value.len() > self.limits.metadata_bytes {
                    return Err(Error::Bounds);
                }
                self.metadata.last_event_id = Some(value.into());
                self.metadata_changed = true;
            }
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                if value.len() > self.limits.metadata_bytes {
                    return Err(Error::Bounds);
                }
                self.metadata.retry_milliseconds = Some(value.into());
                self.metadata_changed = true;
            }
            _ => (), // SSE requires ignoring comments, unknown fields and invalid retry/id.
        }
        Ok(())
    }

    fn finish_event(&mut self, frame: &mut Option<Frame>) -> Result<(), Error> {
        if self.summary.events >= self.limits.events {
            return Err(Error::Bounds);
        }
        self.summary.events += 1;
        self.partial_block = false;
        let had_data_field = !self.data.is_empty();
        if had_data_field {
            self.data.pop();
        }
        if had_data_field && !self.event_type.is_empty() && self.event_type != "message" {
            return Err(Error::EventType);
        }
        self.event_type.clear();
        if !self.data.is_empty() {
            self.summary.messages += 1;
            *frame = Some(Frame::Json {
                bytes: std::mem::take(&mut self.data),
                metadata: Some(self.metadata.clone()),
            });
        } else if had_data_field || self.metadata_changed {
            // The 2025-11-25 empty-data priming event is metadata, not invalid JSON.
            *frame = Some(Frame::Metadata(self.metadata.clone()));
        }
        self.metadata_changed = false;
        Ok(())
    }

    /// EOF is framing completion only. It never certifies a matching JSON-RPC
    /// response, successful call, or absent remote effect. Host must consult Client.
    pub fn finish(mut self, now: Instant) -> Result<End, Error> {
        self.check_deadline(now)?;
        let mut frame = None;
        let mut discarded_partial_event = false;
        match self.mode {
            Mode::Acknowledgement => (),
            Mode::Json => {
                std::str::from_utf8(&self.data).map_err(|_| Error::Utf8)?;
                if self.data.iter().all(u8::is_ascii_whitespace) {
                    return Err(Error::Empty);
                }
                self.summary.messages = 1;
                frame = Some(Frame::Json {
                    bytes: self.data,
                    metadata: None,
                });
            }
            Mode::Sse => {
                // No terminal blank line means no dispatch, including a final
                // complete JSON fragment. Validate but do not interpret the tail.
                std::str::from_utf8(&self.line).map_err(|_| Error::Utf8)?;
                discarded_partial_event = self.partial_block || !self.line.is_empty();
            }
        }
        Ok(End {
            frame,
            summary: self.summary,
            discarded_partial_event,
            metadata: self.metadata,
        })
    }
}

fn media_type(value: &str) -> Result<Mode, Error> {
    if value.len() > 128 || !value.is_ascii() || value.contains(['\r', '\n']) {
        return Err(Error::MediaType);
    }
    let mut parts = value.split(';');
    let base = parts.next().ok_or(Error::MediaType)?.trim();
    let mode = if base.eq_ignore_ascii_case("application/json") {
        Mode::Json
    } else if base.eq_ignore_ascii_case("text/event-stream") {
        Mode::Sse
    } else {
        return Err(Error::MediaType);
    };
    // Narrow declared parameter profile: absent or exactly one UTF-8 charset.
    if let Some(parameter) = parts.next() {
        let (name, value) = parameter.trim().split_once('=').ok_or(Error::MediaType)?;
        let value = value.trim();
        if !name.trim().eq_ignore_ascii_case("charset")
            || !(value.eq_ignore_ascii_case("utf-8") || value.eq_ignore_ascii_case("\"utf-8\""))
            || parts.next().is_some()
        {
            return Err(Error::MediaType);
        }
    }
    Ok(mode)
}
