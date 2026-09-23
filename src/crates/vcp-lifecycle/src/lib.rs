// SPDX-License-Identifier: Apache-2.0
//! P0 lifecycle qualification host over retained Codex controllers.
//!
//! One instance owns one registered tree, a private checkpoint and dispatch
//! receipts. Production storage, provider accounting and CLI policy remain with
//! their owning implementation tasks.
use codex_core::CodexThread;
use codex_extension_api::TurnStartAdmission;
use codex_protocol::ThreadId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::{oneshot, Notify};
mod journal;
pub use journal::Work;
pub mod control;
pub mod foundation;
pub mod integration;
pub mod ports;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub(crate) mod remote_transport;

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
    pub unresolved_work: usize,
    pub pending_commands: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    PersistenceFailed,
    UnresolvedWork,
    CommandConflict,
    InvalidCommand,
    PendingCommand,
    RevalidationFailed,
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
    // Scoped pause generation; ordinary work revisions and sibling holds do
    // not invalidate this thread's queued callbacks.
    admission_generation: u64,
}

struct State {
    instance: Arc<()>,
    revision: u64,
    attached: bool,
    root: Option<ThreadId>,
    sealing: bool,
    // Generic constructors interrupted before root attachment stay held. Only
    // a new explicit resume constructor with its own identity can attach.
    root_admission_held: bool,
    // Only an explicit, controller-bound resume constructor may use this grant.
    // Generic startup/attachment remain held until that constructor attaches.
    root_startup: Option<Arc<()>>,
    root_hold_error: Option<Error>,
    owner_sealing: bool,
    owner_hold_waiters: Vec<oneshot::Sender<Result<(), Error>>>,
    entries: HashMap<ThreadId, Entry>,
    workspace: String,
    journal: Option<journal::Journal>,
    journal_closed: bool,
    work: Vec<Work>,
    commands: Vec<control::CommandRecord>,
    startups: Vec<(std::path::PathBuf, Option<ThreadId>)>,
    startups_in_flight: usize,
    integration: Option<integration::Ledger>,
}

impl State {
    fn admission_current(&self, thread: ThreadId, expected: u64) -> bool {
        self.entries
            .get(&thread)
            .is_some_and(|entry| entry.admission_generation == expected)
    }
    fn checkpoint(&mut self) -> Result<(), Error> {
        if self.journal_closed {
            return Err(Error::OwnerLost);
        }
        if self.journal.is_none() {
            return Ok(());
        }
        let mut threads = Vec::new();
        let mut pending: Vec<_> = self.root.into_iter().collect();
        while let Some(id) = pending.pop() {
            let entry = &self.entries[&id];
            threads.push(journal::Thread {
                id,
                parent: entry.parent,
                held: entry.held,
            });
            let mut children: Vec<_> = self
                .entries
                .iter()
                .filter_map(|(child, entry)| (entry.parent == Some(id)).then_some(*child))
                .collect();
            children.sort_by_key(|id| id.to_string());
            pending.extend(children);
        }
        let checkpoint = journal::Checkpoint {
            format: 2,
            workspace: self.workspace.clone(),
            revision: self.revision,
            threads,
            work: self.work.clone(),
            commands: self.commands.clone(),
            integration: self.integration.clone(),
        };
        if self.journal.as_mut().unwrap().append(&checkpoint).is_err() {
            self.attached = false;
            return Err(Error::PersistenceFailed);
        }
        Ok(())
    }

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
    #[cfg(windows)]
    jobs: Mutex<HashMap<ThreadId, Vec<Arc<codex_utils_pty::JobObject>>>>,
    #[cfg(windows)]
    process_observers: Mutex<HashMap<ThreadId, Vec<Arc<std::sync::atomic::AtomicBool>>>>,
    #[cfg(windows)]
    remote_sockets: Mutex<HashMap<ThreadId, Vec<Weak<remote_transport::SocketControl>>>>,
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
        let _ = self.0.lose_owner();
    }
}
impl OwnerLease {
    pub async fn close(self) -> Result<(), Error> {
        if let Some(waiter) = self.0.lose_owner() {
            waiter.wait().await?;
        }
        let mut state = self.0 .0.state.lock().map_err(|_| Error::Poisoned)?;
        // Interruption has drained every registered controller/process. Any
        // still-unresolved work remains in the final checkpoint. Late callbacks
        // from this owner can no longer acknowledge or write after reacquisition.
        if state.startups_in_flight != 0
            || state
                .entries
                .values()
                .any(|entry| !entry.interrupted || entry.starts != 0)
        {
            return Err(Error::IncompleteInterruption);
        }
        state.journal_closed = true;
        state.journal = None;
        Ok(())
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
    /// A pause invalidates queued callbacks even if the owner resumes before
    /// they wake. Ordinary work/receipt revisions do not invalidate siblings.
    pub(crate) fn admission_generation(&self, id: ThreadId) -> Result<u64, Error> {
        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached {
            return Err(Error::OwnerLost);
        }
        if state.held(id) {
            return Err(Error::Held);
        }
        Ok(state
            .entries
            .get(&id)
            .ok_or(Error::UnknownThread)?
            .admission_generation)
    }
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
                root_admission_held: false,
                root_startup: None,
                root_hold_error: None,
                owner_sealing: false,
                owner_hold_waiters: Vec::new(),
                entries: HashMap::new(),
                workspace: String::new(),
                journal: None,
                journal_closed: false,
                work: Vec::new(),
                commands: Vec::new(),
                startups: Vec::new(),
                startups_in_flight: 0,
                integration: None,
            }),
            changed: Notify::new(),
            runtime: tokio::runtime::Handle::current(),
            deadline,
            #[cfg(windows)]
            jobs: Mutex::new(HashMap::new()),
            #[cfg(windows)]
            process_observers: Mutex::new(HashMap::new()),
            #[cfg(windows)]
            remote_sockets: Mutex::new(HashMap::new()),
        }));
        let owner = OwnerLease(lifecycle.clone());
        (lifecycle, owner)
    }

    /// Acquires exclusive ownership of a private prototype checkpoint. Reopen
    /// always holds the root; it never starts a thread or replays an effect.
    /// The caller must bind recovered controllers and explicitly reconcile them.
    pub fn open(
        path: &std::path::Path,
        workspace: &str,
        deadline: Duration,
    ) -> std::io::Result<(Self, OwnerLease)> {
        if workspace.is_empty() {
            return Err(std::io::Error::other("workspace identity is required"));
        }
        let (journal, recovered) = journal::Journal::open(path, workspace)?;
        let (host, owner) = Self::new(deadline);
        {
            let mut state = host.0.state.lock().unwrap();
            state.workspace = workspace.into();
            state.journal = Some(journal);
            if let Some(checkpoint) = recovered {
                state.revision = checkpoint.revision;
                state.work = checkpoint.work;
                state.commands = checkpoint.commands;
                state.integration = checkpoint.integration;
                for thread in checkpoint.threads {
                    if thread.parent.is_none() {
                        state.root = Some(thread.id);
                    }
                    state.entries.insert(
                        thread.id,
                        Entry {
                            thread: Weak::new(),
                            parent: thread.parent,
                            held: thread.held || thread.parent.is_none(),
                            interrupted: false,
                            interruption_error: None,
                            starts: 0,
                            admission_generation: 0,
                        },
                    );
                }
            }
            state
                .advance()
                .and_then(|_| state.checkpoint())
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        }
        Ok((host, owner))
    }

    pub fn threads(&self) -> Result<Vec<ThreadId>, Error> {
        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        let mut ids: Vec<_> = state.entries.keys().copied().collect();
        ids.sort_by_key(|id| id.to_string());
        Ok(ids)
    }

    pub fn root(&self) -> Result<Option<ThreadId>, Error> {
        Ok(self.0.state.lock().map_err(|_| Error::Poisoned)?.root)
    }

    /// One-use authority supplied by the owning CLI before a retained session
    /// constructor. It is never checkpointed or restored as live authority.
    pub fn authorize_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<ThreadId>,
    ) -> Result<(), Error> {
        let workspace = workspace
            .canonicalize()
            .map_err(|_| Error::RevalidationFailed)?;
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached {
            return Err(Error::OwnerLost);
        }
        if state.sealing {
            return Err(Error::Busy);
        }
        if state.root_admission_held {
            return Err(Error::Held);
        }
        if let Some(id) = resumed {
            if !state.entries.contains_key(&id) {
                return Err(Error::UnknownThread);
            }
        } else if state.root.is_some_and(|id| state.held(id)) {
            return Err(Error::Held);
        }
        state.startups.push((workspace, resumed));
        Ok(())
    }

    pub fn work(&self) -> Result<Vec<Work>, Error> {
        Ok(self
            .0
            .state
            .lock()
            .map_err(|_| Error::Poisoned)?
            .work
            .clone())
    }

    /// Recovery acknowledgement after independent observation. The host must be
    /// paused and quiescent; a receipt does not replay or grant dispatch authority.
    pub fn reconcile(
        &self,
        work_id: u64,
        expected: &Revision,
        evidence: &str,
    ) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        state.check(expected)?;
        let work = state
            .work
            .iter()
            .find(|work| work.id == work_id)
            .ok_or(Error::UnknownThread)?;
        if evidence.trim().is_empty() || work.receipt.is_some() {
            return Err(Error::UnresolvedWork);
        }
        if !state.held(work.thread) {
            return Err(Error::NotHeld);
        }
        if !state.entries[&work.thread].interrupted {
            return Err(Error::IncompleteInterruption);
        }
        state.advance()?;
        state
            .work
            .iter_mut()
            .find(|work| work.id == work_id)
            .unwrap()
            .receipt = Some(evidence.into());
        state.checkpoint()
    }

    /// Bind the retained controller with the exact recovered identity. Binding
    /// grants no start authority, and an existing controller cannot be replaced.
    pub fn bind_recovered(&self, thread: Arc<CodexThread>) -> Result<(), Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached {
            return Err(Error::OwnerLost);
        }
        let id = thread.session_configured().thread_id;
        if !state.held(id) {
            return Err(Error::NotHeld);
        }
        let entry = state.entries.get_mut(&id).ok_or(Error::UnknownThread)?;
        if entry.thread.upgrade().is_some() {
            return Err(Error::DuplicateThread);
        }
        entry.thread = Arc::downgrade(&thread);
        state.advance()?;
        state.checkpoint()
    }

    pub fn attach_root(&self, thread: Arc<CodexThread>) -> Result<ThreadId, Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if !state.attached {
            return Err(Error::OwnerLost);
        }
        if state.sealing || state.root_admission_held {
            return Err(Error::Held);
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
                admission_generation: 0,
            },
        );
        state.checkpoint()?;
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
                admission_generation: 0,
            },
        );
        state.checkpoint()?;
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
            unresolved_work: state
                .work
                .iter()
                .filter(|work| work.receipt.is_none() && state.below(work.thread, id))
                .count(),
            pending_commands: state
                .commands
                .iter()
                .filter(|command| command.result.is_none() && state.below(command.thread, id))
                .count(),
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

    /// Seals/checkpoints synchronously, then owns draining/cancellation even if
    /// the waiter disappears. Unknown effects remain visible and block resume.
    /// Native process confirmation covers jobs registered through this host.
    pub fn hold(&self, id: ThreadId, expected: &Revision) -> Result<HoldWaiter, Error> {
        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        state.check(expected)?;
        self.hold_locked(id, state)
    }

    /// Trusted controller operation: choose and fence the current owner tree
    /// atomically, without retrying a revision observed before work completed.
    pub(crate) fn hold_owner(&self) -> Result<HoldWaiter, Error> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if state.attached && state.sealing && state.owner_sealing {
            let (reply, receiver) = oneshot::channel();
            state.owner_hold_waiters.push(reply);
            return Ok(HoldWaiter(receiver));
        }
        state.check(&state.revision())?;
        if let Some(root) = state.root {
            return self.hold_locked(root, state);
        }
        state.advance()?;
        state.root_admission_held = true;
        state.root_startup = None;
        state.root_hold_error = None;
        state.sealing = true;
        state.owner_sealing = true;
        state.startups.clear();
        let durable = state.checkpoint();
        drop(state);
        self.0.changed.notify_waiters();
        let waiter = HoldWaiter(self.interrupt_owned(Vec::new(), true));
        durable.map(|_| waiter)
    }

    fn hold_locked(
        &self,
        id: ThreadId,
        mut state: std::sync::MutexGuard<'_, State>,
    ) -> Result<HoldWaiter, Error> {
        if !state.entries.contains_key(&id) {
            return Err(Error::UnknownThread);
        }
        state.advance()?;
        state.entries.get_mut(&id).unwrap().held = true;
        state.sealing = true;
        state.owner_sealing = state.root == Some(id);
        state.startups.clear();
        let selected: Vec<_> = state
            .entries
            .keys()
            .copied()
            .filter(|child| state.below(*child, id))
            .collect();
        for child in &selected {
            let entry = state.entries.get_mut(child).unwrap();
            entry.admission_generation = entry
                .admission_generation
                .checked_add(1)
                .ok_or(Error::Overflow)?;
            state.entries.get_mut(child).unwrap().interrupted = false;
            state.entries.get_mut(child).unwrap().interruption_error = None;
        }
        // Seal in memory first. Even if persistence fails, still interrupt every
        // selected controller; failed acknowledgement must not leave work alive.
        let durable = state.checkpoint();
        #[cfg(windows)]
        let remote_wakers = self.close_remote_sockets(&selected);
        drop(state);
        #[cfg(windows)]
        for waker in remote_wakers {
            waker.wake();
        }
        self.0.changed.notify_waiters();
        let waiter = HoldWaiter(self.interrupt_owned(selected, true));
        durable.map(|_| waiter)
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
        if state
            .work
            .iter()
            .any(|work| work.receipt.is_none() && state.below(work.thread, id))
        {
            return Err(Error::UnresolvedWork);
        }
        state.advance()?;
        state.entries.get_mut(&id).unwrap().held = false;
        state.checkpoint()
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
            drop(lifecycle);
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
            let drained = tokio::time::timeout(self.0.deadline, async {
                loop {
                    let changed = self.0.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    let drained = {
                        let state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
                        state.startups_in_flight == 0
                            && selected.iter().all(|id| state.entries[id].starts == 0)
                    };
                    if drained {
                        break;
                    }
                    changed.await;
                }
                Ok::<(), Error>(())
            })
            .await
            .map_err(|_| Error::DrainTimeout)
            .and_then(|result| result);
            #[cfg(windows)]
            let processes_stopped = self.stop_processes(&selected).await;
            #[cfg(not(windows))]
            let processes_stopped = Ok(());
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
            let interrupted = if failed {
                Err(Error::InterruptFailed)
            } else {
                Ok(())
            };
            drained.and(processes_stopped).and(interrupted)
        }
        .await;
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        if state.root.is_none() && state.root_admission_held {
            state.root_hold_error = result.err();
        }
        for id in selected {
            let entry = state.entries.get_mut(&id).unwrap();
            entry.interrupted = result.is_ok();
            entry.interruption_error = result.err();
        }
        if clear_sealing {
            state.sealing = false;
        }
        let result = state.advance().and_then(|_| state.checkpoint()).and(result);
        if clear_sealing && state.owner_sealing {
            state.owner_sealing = false;
            for waiter in state.owner_hold_waiters.drain(..) {
                let _ = waiter.send(result);
            }
        }
        result
    }

    fn lose_owner(&self) -> Option<HoldWaiter> {
        let mut state = match self.0.state.lock() {
            Ok(state) => state,
            Err(error) => {
                drop(error);
                #[cfg(windows)]
                self.close_all_remote_sockets();
                return None;
            }
        };
        if !state.attached {
            drop(state);
            #[cfg(windows)]
            self.close_all_remote_sockets();
            return None;
        }
        state.attached = false;
        state.startups.clear();
        let _ = state.advance();
        let selected: Vec<_> = state.entries.keys().copied().collect();
        let root = state.root;
        for (id, entry) in &mut state.entries {
            // Owner loss adds the root hold; preserve each child's independent
            // hold so deliberately resuming the root can release inherited holds.
            entry.held |= Some(*id) == root;
            entry.interrupted = false;
            entry.interruption_error = None;
        }
        let _ = state.checkpoint();
        #[cfg(windows)]
        let remote_wakers = self.close_remote_sockets(&selected);
        drop(state);
        #[cfg(windows)]
        for waker in remote_wakers {
            waker.wake();
        }
        Some(HoldWaiter(self.interrupt_owned(selected, false)))
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

struct WorkPermit {
    host: Lifecycle,
    id: u64,
    completed: bool,
}

struct StartupPermit(Lifecycle);
impl Drop for StartupPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0 .0.state.lock() {
            state.startups_in_flight -= 1;
        }
        self.0 .0.changed.notify_waiters();
    }
}

impl codex_extension_api::HostWorkPermit for WorkPermit {
    fn complete_model(
        &mut self,
        usage: Option<&codex_protocol::protocol::TokenUsage>,
    ) -> Result<(), String> {
        self.finish(Some(usage))
    }
    fn complete(&mut self) -> Result<(), String> {
        self.finish(None)
    }
}
impl WorkPermit {
    fn finish(
        &mut self,
        usage: Option<Option<&codex_protocol::protocol::TokenUsage>>,
    ) -> Result<(), String> {
        if self.completed {
            return Ok(());
        }
        let mut state = self.host.0.state.lock().map_err(|_| "poisoned lifecycle")?;
        if state.journal_closed {
            return Err("owner checkpoint is closed".into());
        }
        if let (Some(ledger), Some(usage)) = (state.integration.as_mut(), usage) {
            if usage.is_some_and(|u| {
                [
                    u.input_tokens,
                    u.output_tokens,
                    u.cached_input_tokens,
                    u.cache_write_input_tokens,
                    u.reasoning_output_tokens,
                    u.total_tokens,
                ]
                .iter()
                .any(|n| *n < 0)
            }) {
                return Err("negative provider usage cannot settle a reservation".into());
            }
            let charge = ledger
                .requests
                .iter_mut()
                .find(|row| row.intent == self.id)
                .ok_or("missing reservation")?;
            charge.usage = usage
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| e.to_string())?;
            charge.settled = true;
        }
        // Receipt ingestion is allowed after owner loss or pause. It is not a
        // state transition capable of dispatching new work.
        state.advance().map_err(|error| format!("{error:?}"))?;
        state
            .work
            .iter_mut()
            .find(|work| work.id == self.id)
            .ok_or("missing intent")?
            .receipt = Some("retained operation completed".into());
        state.checkpoint().map_err(|error| format!("{error:?}"))?;
        self.completed = true;
        Ok(())
    }
}

impl codex_extension_api::HostWorkAdmission for Lifecycle {
    fn admit_tool(
        &self,
        thread: ThreadId,
        call_id: &str,
        name: &codex_extension_api::ToolName,
    ) -> Result<Box<dyn codex_extension_api::HostWorkPermit>, String> {
        {
            let state = self.0.state.lock().map_err(|_| "poisoned lifecycle")?;
            if state.integration.is_some()
                && (!name.is_default_namespace() || name.name != "vcp_workspace")
            {
                return Err("tool exceeds the immutable P0 host ceiling".into());
            }
        }
        codex_extension_api::HostWorkAdmission::admit(
            self,
            thread,
            codex_extension_api::HostWorkKind::Tool,
            call_id,
        )
    }
    fn admit_startup(
        &self,
        workspace: &std::path::Path,
        resumed: Option<ThreadId>,
    ) -> Result<Box<dyn Send>, String> {
        let workspace = workspace
            .canonicalize()
            .map_err(|_| "unavailable startup workspace")?;
        let mut state = self.0.state.lock().map_err(|_| "poisoned lifecycle")?;
        if !state.attached || state.sealing || state.root_admission_held {
            return Err("startup owner unavailable".into());
        }
        let index = state
            .startups
            .iter()
            .position(|grant| grant == &(workspace.clone(), resumed))
            .ok_or("missing explicit startup authority")?;
        state.startups.remove(index);
        state.startups_in_flight = state
            .startups_in_flight
            .checked_add(1)
            .ok_or("startup overflow")?;
        Ok(Box::new(StartupPermit(self.clone())))
    }

    fn admit(
        &self,
        thread: ThreadId,
        kind: codex_extension_api::HostWorkKind,
        label: &str,
    ) -> Result<Box<dyn codex_extension_api::HostWorkPermit>, String> {
        let mut state = self.0.state.lock().map_err(|_| "poisoned lifecycle")?;
        if !state.attached || state.held(thread) {
            return Err("host dispatch is paused or unowned".into());
        }
        if kind == codex_extension_api::HostWorkKind::Model {
            if let Some(ledger) = &state.integration {
                ledger.check_admission()?;
            }
        }
        state.advance().map_err(|error| format!("{error:?}"))?;
        let id = state.revision;
        if kind == codex_extension_api::HostWorkKind::Model {
            if let Some(ledger) = state.integration.as_mut() {
                ledger.requests.push(integration::Charge {
                    intent: id,
                    thread,
                    units: ledger.per_request,
                    settled: false,
                    usage: None,
                });
            }
        }
        state.work.push(Work {
            id,
            thread,
            kind: format!("{kind:?}"),
            label: label.into(),
            receipt: None,
        });
        state.checkpoint().map_err(|error| format!("{error:?}"))?;
        Ok(Box::new(WorkPermit {
            host: self.clone(),
            id,
            completed: false,
        }))
    }
}
