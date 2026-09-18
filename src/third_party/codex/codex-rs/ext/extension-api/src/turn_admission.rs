// VCP modification: thread-scoped host admission for ordinary and delegated starts.
//! Lets Core turn-input submissions participate in host admission control.

/// A host-provided gate checked before Core starts a turn-input submission.
///
/// Implementations return a permit for work admitted before shutdown and
/// Core retains it through submission. `None` skips the start without consuming
/// pending input. Steering an existing turn does not acquire a new permit.
/// Memory-only mailbox wakeups and parent-delegated subagent input use the
/// separate continuation hook, whose default lets delegated work finish during
/// shutdown drain. Automatic starts use ordinary admission.
pub trait TurnStartAdmission: std::fmt::Debug + Send + Sync {
    fn admit_turn_start(&self) -> Option<Box<dyn Send>>;

    /// Acquires admission for delegated input or a memory-only mailbox wakeup.
    ///
    /// The default preserves shutdown draining: already delegated work may finish.
    /// A host implementing pause must override this and seal the same admission
    /// boundary as ordinary turn starts. Core retains the permit through start
    /// publication and does not consume pending mailbox input when it is denied.
    /// This hook alone does not cancel active work or establish a durable pause.
    fn admit_continuation_start(&self) -> Option<Box<dyn Send>> {
        Some(Box::new(()))
    }

    /// Checks ordinary admission with the controller-owned thread identity.
    /// A host must resolve this locator against its own workspace/owner/task
    /// binding; the identifier alone is not authority. The default preserves
    /// existing hosts' global admission behavior.
    fn admit_turn_start_for_thread(
        &self,
        _thread_id: codex_protocol::ThreadId,
    ) -> Option<Box<dyn Send>> {
        self.admit_turn_start()
    }

    /// Checks delegated admission with the controller-owned thread identity.
    /// Task-specific hosts should override both thread-aware methods and share
    /// the same fence for ordinary and continuation work.
    fn admit_continuation_start_for_thread(
        &self,
        _thread_id: codex_protocol::ThreadId,
    ) -> Option<Box<dyn Send>> {
        self.admit_continuation_start()
    }
}
