// VCP modification: preserve isolated host gates, tool ceilings and atomic model receipts.
// SPDX-License-Identifier: Apache-2.0
// VCP addition: private host dispatch admission and durable receipt boundary.
use codex_protocol::ThreadId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostWorkKind {
    Model,
    Tool,
}

/// Dropping a permit without a receipt leaves its outcome unknown. Completion
/// may reconcile while paused, but cannot authorize new work.
pub trait HostWorkPermit: Send {
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
