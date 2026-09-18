// VCP modification: gate delegated turn starts through host continuation admission.
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
}
