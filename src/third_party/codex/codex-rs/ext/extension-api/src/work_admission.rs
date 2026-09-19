// VCP modification: preserve isolated host gates, tool ceilings and atomic model receipts.
// SPDX-License-Identifier: Apache-2.0
// VCP addition: private host dispatch admission and durable receipt boundary.
use codex_protocol::ThreadId;
use std::sync::Arc;

/// VCP: body bytes only; authentication headers never cross this boundary.
pub type HostResponseCapture = Arc<dyn Fn(&[u8]) -> Result<(), String> + Send + Sync>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostModelPurpose {
    Turn,
    Compaction,
    Memory,
}

/// Provider-independent classification. No credentials or provider response text.
#[derive(Clone, Copy, Debug)]
pub enum HostModelFailure {
    Http(u16),
    Timeout,
    Transport,
    Protocol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostWorkKind {
    Model,
    Tool,
}

/// Dropping a permit without a receipt leaves its outcome unknown. Completion
/// may reconcile while paused, but cannot authorize new work.
pub trait HostWorkPermit: Send {
    /// Qualify a new accounted attempt. The caller waits without a store lock,
    /// checks freshness during the wait, and retains this permit until admission.
    fn retry_delay(
        &mut self,
        _failure: HostModelFailure,
        _retry_after_ms: Option<u64>,
    ) -> Result<Option<std::time::Duration>, String> {
        Ok(None)
    }
    fn retry_current(&self) -> bool {
        false
    }
    fn admit_retry(
        &mut self,
        _body: &mut serde_json::Value,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        Err("host retry is not enabled".into())
    }
    /// VCP: one absolute deadline covers headers and body, including keepalives.
    fn response_deadline(&self) -> Option<std::time::Instant> {
        None
    }
    fn complete_model_response(
        &mut self,
        usage: Option<&codex_protocol::protocol::TokenUsage>,
        _response_id: &str,
    ) -> Result<(), String> {
        self.complete_model(usage)
    }
    fn response_capture(&self) -> Option<HostResponseCapture> {
        None
    }
    /// Non-success HTTP bodies are raw evidence, never successful SSE events.
    fn response_error_capture(&self) -> Option<HostResponseCapture> {
        self.response_capture()
    }
    fn complete(&mut self) -> Result<(), String>;
    /// VCP: retain provider usage with the same durable dispatch receipt.
    fn complete_model(
        &mut self,
        _usage: Option<&codex_protocol::protocol::TokenUsage>,
    ) -> Result<(), String> {
        self.complete()
    }
}

pub trait HostWorkAdmission: std::fmt::Debug + Send + Sync {
    /// VCP: defer tool construction until the complete response is admitted.
    /// A failed/interrupted stream discards pending calls without dispatch.
    fn requires_completed_response(&self) -> bool {
        false
    }
    /// VCP: runs for each actual HTTP attempt after request construction. The
    /// host may insert enforceable output bounds before capturing and admitting
    /// the exact body. A permit must precede any network dispatch.
    fn admit_model(
        &self,
        thread: ThreadId,
        _body: &mut serde_json::Value,
        _purpose: HostModelPurpose,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.admit(thread, HostWorkKind::Model, "responses")
    }
    /// VCP: a host ceiling applies even to tools requested without advertisement.
    fn admit_tool(
        &self,
        thread: ThreadId,
        call_id: &str,
        _name: &crate::ToolName,
    ) -> Result<Box<dyn HostWorkPermit>, String> {
        self.admit(thread, HostWorkKind::Tool, call_id)
    }
    /// Startup requires a separate, one-use host capability. Resuming history
    /// does not confer authority to run model requests or tools.
    fn admit_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<ThreadId>,
    ) -> Result<Box<dyn Send>, String>;
    fn admit(
        &self,
        thread: ThreadId,
        kind: HostWorkKind,
        label: &str,
    ) -> Result<Box<dyn HostWorkPermit>, String>;
}
