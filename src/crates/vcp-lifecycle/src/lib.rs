// SPDX-License-Identifier: Apache-2.0
//! P0-03 in-memory host fence over retained Codex controllers.
//!
//! One instance owns one registered tree. It is not the canonical store, an
//! effect/budget gate, startup sandbox, or the product `/pause` command.
use codex_core::CodexThread;
use codex_extension_api::TurnStartAdmission;
use codex_protocol::ThreadId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::{oneshot, Notify};

#[derive(Clone, Debug)]
pub struct Revision {
    instance: Arc<()>,
    value: u64,
}

#[derive(Clone, Debug)]
pub struct View {
    pub revision: Revision,
    pub owner_attached: bool,
    pub local_hold: bool,
    pub inherited_hold: bool,
    pub interrupt_complete: bool,
    pub interruption_error: Option<Error>,
    pub starts_in_flight: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnknownThread,
    DuplicateThread,
    RootAlreadyAttached,
    StaleRevision,
    OwnerLost,
    Held,
    NotHeld,
    Busy,
    IncompleteInterruption,
    DrainTimeout,
    InterruptFailed,
    Overflow,
    Poisoned,
}

struct Entry {
    // The thread's extension registry retains this host; a strong handle here
    // would form a cycle. Acquire owned handles only for an interruption.
    thread: Weak<CodexThread>,
    parent: Option<ThreadId>,
    held: bool,
    interrupted: bool,
    interruption_error: Option<Error>,
    starts: usize,
}

struct State {
    instance: Arc<()>,
    revision: u64,
    attached: bool,
    root: Option<ThreadId>,
    sealing: bool,
    entries: HashMap<ThreadId, Entry>,
}

impl State {
    fn revision(&self) -> Revision {
        Revision {
            instance: self.instance.clone(),
            value: self.revision,
        }
    }

    fn check(&self, expected: &Revision) -> Result<(), Error> {
        if !self.attached {
            return Err(Error::OwnerLost);
        }
        if !Arc::ptr_eq(&self.instance, &expected.instance) || self.revision != expected.value {
            return Err(Error::StaleRevision);
        }
        if self.sealing {
            return Err(Error::Busy);
        }
        Ok(())
    }

    fn advance(&mut self) -> Result<(), Error> {
        self.revision = self.revision.checked_add(1).ok_or_else(|| {
            self.attached = false;
            Error::Overflow
        })?;
        Ok(())
    }

    fn below(&self, mut id: ThreadId, ancestor: ThreadId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            match self.entries.get(&id).and_then(|entry| entry.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    fn held(&self, mut id: ThreadId) -> bool {
        loop {
            let Some(entry) = self.entries.get(&id) else {
                return true;
            };
            if entry.held {
                return true;
            }
            match entry.parent {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }
}

struct Inner {
    state: Mutex<State>,
    changed: Notify,
    runtime: tokio::runtime::Handle,
    deadline: Duration,
}

/// Cloneable host API. The separate owner lease controls owner lifetime.
#[derive(Clone)]
pub struct Lifecycle(Arc<Inner>);

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lifecycle").finish_non_exhaustive()
    }
}

/// Dropping the owning connection seals the whole tree synchronously and
/// schedules retained interruption. It never grants authority on reattachment.
pub struct OwnerLease(Lifecycle);
impl Drop for OwnerLease {
    fn drop(&mut self) {
        self.0.lose_owner();
    }
}

/// Dropping this waiter does not abort the owned interruption operation.
pub struct HoldWaiter(oneshot::Receiver<Result<(), Error>>);
impl HoldWaiter {
    pub async fn wait(self) -> Result<(), Error> {
        self.0.await.unwrap_or(Err(Error::InterruptFailed))
    }
}

struct StartPermit {
    lifecycle: Lifecycle,
    id: ThreadId,
}
impl Drop for StartPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lifecycle.0.state.lock() {
            if let Some(entry) = state.entries.get_mut(&self.id) {
                entry.starts -= 1;
            }
        }
        self.lifecycle.0.changed.notify_waiters();
    }
}

impl Lifecycle {
    /// Call inside the owning Tokio runtime. The deadline bounds start draining;
    /// expiry leaves holds in place and reports failure. Retained interruption
    /// stays owned until complete, so a late stop cannot race readmission.
    pub fn new(deadline: Duration) -> (Self, OwnerLease) {
        let lifecycle = Self(Arc::new(Inner {
            state: Mutex::new(State {
                instance: Arc::new(()),
                revision: 0,
                attached: true,
                root: None,
                sealing: false,
                entries: HashMap::new(),
            }),
            changed: Notify::new(),
            runtime: tokio::runtime::Handle::current(),
            deadline,
        }));
        let owner = OwnerLease(lifecycle.clone());
        (lifecycle, owner)
    }

    pub fn attach_root(&self, thread: Arc<CodexThread>) -> Result<ThreadId, Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached {
            return Err(Error::OwnerLost);
        }
        if state.root.is_some() {
            return Err(Error::RootAlreadyAttached);
        }
        state.advance()?;
        let id = thread.session_configured().thread_id;
        state.root = Some(id);
        state.entries.insert(
            id,
            Entry {
                thread: Arc::downgrade(&thread),
                parent: None,
                held: false,
                interrupted: false,
                interruption_error: None,
                starts: 0,
            },
        );
        Ok(id)
    }

    /// Registration comes from the host, never from model-supplied lineage.
    /// Unknown threads using this admission gate cannot start turns. This does
    /// not fence upstream startup effects or isolated sessions with other gates.
    pub fn attach_child(
        &self,
        parent: ThreadId,
        expected: &Revision,
        thread: Arc<CodexThread>,
    ) -> Result<ThreadId, Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        state.check(expected)?;
        if !state.entries.contains_key(&parent) {
            return Err(Error::UnknownThread);
        }
        if state.held(parent) {
            return Err(Error::Held);
        }
        let id = thread.session_configured().thread_id;
        if state.entries.contains_key(&id) {
            return Err(Error::DuplicateThread);
        }
        state.advance()?;
        state.entries.insert(
            id,
            Entry {
                thread: Arc::downgrade(&thread),
                parent: Some(parent),
                held: false,
                interrupted: false,
                interruption_error: None,
                starts: 0,
            },
        );
        Ok(id)
    }

    pub fn inspect(&self, id: ThreadId) -> Result<View, Error> {
        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        let entry = state.entries.get(&id).ok_or(Error::UnknownThread)?;
        Ok(View {
            revision: state.revision(),
            owner_attached: state.attached,
            local_hold: entry.held,
            inherited_hold: entry.parent.is_some_and(|id| state.held(id)),
            interrupt_complete: entry.interrupted,
            interruption_error: entry.interruption_error,
            starts_in_flight: entry.starts,
        })
    }

    fn admit(&self, id: ThreadId) -> Option<Box<dyn Send>> {
        let mut state = self.0.state.lock().ok()?;
        if !state.attached || state.held(id) {
            return None;
        }
        let entry = state.entries.get_mut(&id)?;
        entry.starts = entry.starts.checked_add(1)?;
        entry.interrupted = false;
        Some(Box::new(StartPermit {
            lifecycle: self.clone(),
            id,
        }))
    }

    /// Seals synchronously, then owns draining/cancellation independently of the
    /// returned waiter. Completion means retained interruption completed, not a
    /// durable pause, drained tool receipts or native process-tree quiescence.
    pub fn hold(&self, id: ThreadId, expected: &Revision) -> Result<HoldWaiter, Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        state.check(expected)?;
        if !state.entries.contains_key(&id) {
            return Err(Error::UnknownThread);
        }
        state.advance()?;
        state.entries.get_mut(&id).unwrap().held = true;
        state.sealing = true;
        let selected: Vec<_> = state
            .entries
            .keys()
            .copied()
            .filter(|child| state.below(*child, id))
            .collect();
        for child in &selected {
            state.entries.get_mut(child).unwrap().interrupted = false;
            state.entries.get_mut(child).unwrap().interruption_error = None;
        }
        drop(state);
        Ok(HoldWaiter(self.interrupt_owned(selected, true)))
    }

    pub fn resume(&self, id: ThreadId, expected: &Revision) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        state.check(expected)?;
        let entry = state.entries.get(&id).ok_or(Error::UnknownThread)?;
        if !entry.held {
            return Err(Error::NotHeld);
        }
        if entry.parent.is_some_and(|parent| state.held(parent)) {
            return Err(Error::Held);
        }
        if state
            .entries
            .iter()
            .any(|(child, entry)| state.below(*child, id) && !entry.interrupted)
        {
            return Err(Error::IncompleteInterruption);
        }
        state.advance()?;
        state.entries.get_mut(&id).unwrap().held = false;
        Ok(())
    }

    fn interrupt_owned(
        &self,
        selected: Vec<ThreadId>,
        clear_sealing: bool,
    ) -> oneshot::Receiver<Result<(), Error>> {
        let lifecycle = self.clone();
        let (reply, waiter) = oneshot::channel();
        drop(self.0.runtime.spawn(async move {
            let result = lifecycle.interrupt_selected(selected, clear_sealing).await;
            let _ = reply.send(result);
        }));
        waiter
    }

    async fn interrupt_selected(
        &self,
        selected: Vec<ThreadId>,
        clear_sealing: bool,
    ) -> Result<(), Error> {
        let result = async {
            tokio::time::timeout(self.0.deadline, async {
                loop {
                    let changed = self.0.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    let drained = {
                        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
                        selected.iter().all(|id| state.entries[id].starts == 0)
                    };
                    if drained {
                        break;
                    }
                    changed.await;
                }
                Ok::<(), Error>(())
            })
            .await
            .map_err(|_| Error::DrainTimeout)??;
            let threads: Vec<_> = {
                let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
                selected
                    .iter()
                    .map(|id| state.entries[id].thread.upgrade())
                    .collect()
            };
            // Submit every interruption before awaiting any one slow thread.
            let mut workers = Vec::new();
            let mut failed = false;
            for thread in threads {
                let Some(thread) = thread else {
                    failed = true;
                    continue;
                };
                workers.push(
                    self.0
                        .runtime
                        .spawn(async move { thread.interrupt_for_host().await }),
                );
            }
            for worker in workers {
                if !matches!(worker.await, Ok(Ok(()))) {
                    failed = true;
                }
            }
            if failed {
                Err(Error::InterruptFailed)
            } else {
                Ok(())
            }
        }
        .await;
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        for id in selected {
            let entry = state.entries.get_mut(&id).unwrap();
            entry.interrupted = result.is_ok();
            entry.interruption_error = result.err();
        }
        if clear_sealing {
            state.sealing = false;
        }
        state.advance()?;
        result
    }

    fn lose_owner(&self) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        if !state.attached {
            return;
        }
        state.attached = false;
        let _ = state.advance();
        let selected: Vec<_> = state.entries.keys().copied().collect();
        for entry in state.entries.values_mut() {
            entry.held = true;
            entry.interrupted = false;
            entry.interruption_error = None;
        }
        drop(state);
        drop(self.interrupt_owned(selected, false));
    }
}

impl TurnStartAdmission for Lifecycle {
    fn admit_turn_start(&self) -> Option<Box<dyn Send>> {
        None
    }
    fn admit_continuation_start(&self) -> Option<Box<dyn Send>> {
        None
    }
    fn admit_turn_start_for_thread(&self, id: ThreadId) -> Option<Box<dyn Send>> {
        self.admit(id)
    }
    fn admit_continuation_start_for_thread(&self, id: ThreadId) -> Option<Box<dyn Send>> {
        self.admit(id)
    }
}
