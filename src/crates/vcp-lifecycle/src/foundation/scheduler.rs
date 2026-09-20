// SPDX-License-Identifier: Apache-2.0
//! Owner-local bounded scheduler. Claims come exclusively from trusted prepared
//! operations, never provider concurrency hints. Conflicting waiters retain FIFO
//! order; independent reads and disjoint effects can bypass a blocked waiter.
use super::*;
use std::collections::VecDeque;
use tokio::sync::Notify;
use vcp_domain::policy::{EffectClass, Operation, Resource};

const MAX_ACTIVE: usize = 8;
const MAX_TASK_ACTIVE: usize = 4;
const MAX_QUEUED: usize = 64;

#[derive(Clone)]
struct Claim {
    task: TaskId,
    resources: Vec<Resource>,
    opaque: bool,
}
impl Claim {
    fn from_operation(operation: &Operation) -> Self {
        Self {
            task: operation.scope.task.clone(),
            resources: operation.resources.clone(),
            opaque: operation.resources.is_empty()
                || operation
                    .effects
                    .iter()
                    .any(|effect| !matches!(effect, EffectClass::Read | EffectClass::Write)),
        }
    }
    fn conflicts(&self, other: &Self) -> bool {
        self.opaque
            || other.opaque
            || self.resources.iter().any(|left| {
                other.resources.iter().any(|right| {
                    left.root == right.root
                        && (left.write || right.write)
                        && overlaps(&left.path, &right.path)
                })
            })
    }
}
fn overlaps(left: &str, right: &str) -> bool {
    // Windows ordinal case folding is not Unicode lowercase. Until native
    // file identity is a scheduler key, conservatively serialize non-ASCII
    // aliases within the root (read/read sharing remains safe).
    if !left.is_ascii() || !right.is_ascii() {
        return true;
    }
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    left.is_empty()
        || right.is_empty()
        || left == right
        || left
            .strip_prefix(&right)
            .is_some_and(|rest| rest.starts_with('/'))
        || right
            .strip_prefix(&left)
            .is_some_and(|rest| rest.starts_with('/'))
}
#[derive(Default)]
struct Queue {
    next: u64,
    active: Vec<(u64, Claim)>,
    waiting: VecDeque<(u64, Claim)>,
}
impl Queue {
    fn eligible(&self, claim: &Claim) -> bool {
        self.active.len() < MAX_ACTIVE
            && self
                .active
                .iter()
                .filter(|(_, c)| c.task == claim.task)
                .count()
                < MAX_TASK_ACTIVE
            && !self.active.iter().any(|(_, c)| c.conflicts(claim))
    }
    fn id(&mut self) -> Result<u64, String> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or("scheduler identity overflow")?;
        Ok(self.next)
    }
}
#[derive(Default)]
pub(super) struct Scheduler {
    queue: Mutex<Queue>,
    changed: Notify,
}
pub(super) struct EffectLease {
    scheduler: Arc<Scheduler>,
    id: u64,
}
// The same guard owns queued and active claims: dropping a suspended future
// removes its waiter, while an executing process keeps its claim until receipt.
impl Drop for EffectLease {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.scheduler.queue.lock() {
            queue.waiting.retain(|(id, _)| *id != self.id);
            queue.active.retain(|(id, _)| *id != self.id);
        }
        self.scheduler.changed.notify_waiters();
    }
}
impl Scheduler {
    pub(super) fn busy(&self) -> bool {
        self.queue
            .lock()
            .map(|q| !q.active.is_empty() || !q.waiting.is_empty())
            .unwrap_or(true)
    }
    pub(super) fn try_acquire(
        self: &Arc<Self>,
        operation: &Operation,
    ) -> Result<EffectLease, String> {
        let claim = Claim::from_operation(operation);
        let mut queue = self.queue.lock().map_err(|_| "scheduler poisoned")?;
        if !queue.eligible(&claim) || queue.waiting.iter().any(|(_, c)| c.conflicts(&claim)) {
            return Err("workspace has an active conflicting operation or scheduler limit".into());
        }
        let id = queue.id()?;
        queue.active.push((id, claim));
        Ok(EffectLease {
            scheduler: self.clone(),
            id,
        })
    }
    async fn acquire(
        self: &Arc<Self>,
        operation: &Operation,
        runtime: &Lifecycle,
        thread: ThreadId,
        generation: u64,
    ) -> Result<EffectLease, String> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(operation.timeout_ms.get());
        self.acquire_until(operation, runtime, thread, generation, deadline)
            .await
    }
    async fn acquire_until(
        self: &Arc<Self>,
        operation: &Operation,
        runtime: &Lifecycle,
        thread: ThreadId,
        generation: u64,
        deadline: tokio::time::Instant,
    ) -> Result<EffectLease, String> {
        let claim = Claim::from_operation(operation);
        let id = {
            let mut queue = self.queue.lock().map_err(|_| "scheduler poisoned")?;
            if queue.waiting.len() >= MAX_QUEUED {
                return Err("scheduler queue limit exceeded".into());
            }
            let id = queue.id()?;
            queue.waiting.push_back((id, claim.clone()));
            id
        };
        let guard = EffectLease {
            scheduler: self.clone(),
            id,
        };
        loop {
            // Enable before inspecting state, avoiding a missed release/hold.
            let released = self.changed.notified();
            let owner_changed = runtime.0.changed.notified();
            tokio::pin!(released, owner_changed);
            released.as_mut().enable();
            owner_changed.as_mut().enable();
            check_generation(runtime, thread, generation)?;
            {
                let mut queue = self.queue.lock().map_err(|_| "scheduler poisoned")?;
                // A release and an elapsed timer may both be ready when a late
                // callback runs. Eligibility cannot outrun its original deadline.
                if tokio::time::Instant::now() >= deadline {
                    return Err("scheduler queue deadline elapsed".into());
                }
                let prior_conflict = queue
                    .waiting
                    .iter()
                    .take_while(|(other, _)| *other != id)
                    .any(|(_, earlier)| earlier.conflicts(&claim));
                let eligible = queue.eligible(&claim) && !prior_conflict;
                if tokio::time::Instant::now() >= deadline {
                    return Err("scheduler queue deadline elapsed".into());
                }
                if eligible {
                    queue.waiting.retain(|(other, _)| *other != id);
                    queue.active.push((id, claim));
                    return Ok(guard);
                }
            }
            tokio::select! {
                _ = released => {},
                _ = owner_changed => {},
                _ = tokio::time::sleep_until(deadline) => return Err("scheduler queue deadline elapsed".into()),
            }
        }
    }
}
pub(super) fn generation(runtime: &Lifecycle, thread: ThreadId) -> Result<u64, String> {
    runtime
        .admission_generation(thread)
        .map_err(|error| format!("scheduler admission: {error:?}"))
}
pub(super) fn check_generation(
    runtime: &Lifecycle,
    thread: ThreadId,
    expected: u64,
) -> Result<(), String> {
    if generation(runtime, thread)? != expected {
        return Err("queued admission was invalidated by pause or steering".into());
    }
    Ok(())
}
impl CanonicalHost {
    #[cfg(windows)]
    pub(super) fn cancel_queued_effect(
        &self,
        binding: ThreadBinding,
        effect: ToolRunId,
        reason: String,
    ) -> Result<(), String> {
        cancel_queued(&self.worker, binding, effect, reason)
    }
    pub(super) async fn schedule(
        &self,
        operation: &Operation,
        thread: ThreadId,
        generation: u64,
    ) -> Result<EffectLease, String> {
        self.scheduler
            .acquire(operation, &self.runtime, thread, generation)
            .await
    }
}
#[cfg(windows)]
fn cancel_queued(
    worker: &worker::Worker,
    binding: ThreadBinding,
    effect: ToolRunId,
    reason: String,
) -> Result<(), String> {
    worker
        .run_cleanup(move |context| {
            use vcp_domain::effect::{Effect, EffectState};
            use vcp_store::contract::Collection;
            let current: Effect = context
                .engine
                .store()
                .state()
                .record(
                    Collection::Effect,
                    effect.as_str(),
                    &binding.scope.workspace,
                )?
                .decode()?;
            if matches!(
                current.state,
                EffectState::Proposed | EffectState::Validated | EffectState::Authorized
            ) {
                context.tool_advance(
                    &binding,
                    &effect,
                    EffectState::Cancelled,
                    None,
                    current.observed_changes,
                    &reason,
                )?;
            }
            Ok(())
        })
        .inspect_err(|_| worker.fence())
}

/// A cancelled retained future still leaves a durable non-dispatch outcome.
/// Once a native lease is acquired, its execution path owns terminal reporting.
#[cfg(windows)]
pub(super) struct QueuedEffect {
    worker: worker::Worker,
    binding: ThreadBinding,
    effect: Option<ToolRunId>,
}
#[cfg(windows)]
impl QueuedEffect {
    pub(super) fn new(host: &CanonicalHost, binding: &ThreadBinding, effect: &ToolRunId) -> Self {
        Self {
            worker: host.worker.clone(),
            binding: binding.clone(),
            effect: Some(effect.clone()),
        }
    }
    pub(super) fn dispatched(&mut self) {
        self.effect = None;
    }
}
#[cfg(windows)]
impl Drop for QueuedEffect {
    fn drop(&mut self) {
        if let Some(effect) = self.effect.take() {
            let _ = cancel_queued(
                &self.worker,
                self.binding.clone(),
                effect,
                "queued callback cancelled before dispatch".into(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vcp_domain::policy::Invocation;

    fn operation(path: &str, write: bool) -> Operation {
        Operation {
            scope: Scope {
                workspace: WorkspaceId::new(),
                session: SessionId::new(),
                task: TaskId::new(),
            },
            actor: ActorId::new(),
            host: HostId::new(),
            binding: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            steering: SteeringRevision::ZERO,
            policy: PolicyRevision::ZERO,
            tool: "fixture".into(),
            schema: "a".repeat(64),
            arguments: "{}".into(),
            invocation: Invocation::Local,
            resources: vec![Resource {
                root: RootId::parse("workspace").unwrap(),
                path: path.into(),
                write,
                version: "b".repeat(64),
            }],
            effects: BTreeSet::from([if write {
                EffectClass::Write
            } else {
                EffectClass::Read
            }]),
            required_isolation: BTreeSet::new(),
            timeout_ms: Units::new(5_000),
            output_bytes: ByteCount::new(1024),
        }
    }
    fn runtime() -> (Lifecycle, OwnerLease, ThreadId) {
        let (runtime, owner) = Lifecycle::new(Duration::from_secs(2));
        let thread = ThreadId::new();
        let mut state = runtime.0.state.lock().unwrap();
        state.root = Some(thread);
        state.entries.insert(
            thread,
            crate::Entry {
                thread: std::sync::Weak::new(),
                parent: None,
                held: false,
                interrupted: false,
                interruption_error: None,
                starts: 0,
                admission_generation: 0,
            },
        );
        drop(state);
        (runtime, owner, thread)
    }
    #[test]
    fn trusted_claims_share_reads_serialize_overlapping_writes_and_bound_active_work() {
        let scheduler = Arc::new(Scheduler::default());
        let read = operation("Dir/file.txt", false);
        let one = scheduler.try_acquire(&read).unwrap();
        let two = scheduler.try_acquire(&read).unwrap();
        let mut write = operation("dir", true);
        assert!(
            scheduler.try_acquire(&write).is_err(),
            "Windows case/subtree aliases conflict"
        );
        write.resources[0].path = "different.txt".into();
        let three = scheduler.try_acquire(&write).unwrap();
        assert!(scheduler
            .try_acquire(&operation("d\u{131}r/other.txt", true))
            .is_err());
        assert!(scheduler.try_acquire(&operation("", true)).is_err());
        let mut opaque = operation("elsewhere.txt", false);
        opaque.effects.insert(EffectClass::Opaque);
        assert!(scheduler.try_acquire(&opaque).is_err());
        let four = scheduler.try_acquire(&read).unwrap();
        let five = scheduler.try_acquire(&read).unwrap();
        assert!(
            scheduler.try_acquire(&read).is_err(),
            "per-task bound is trusted"
        );
        drop((one, two, three, four, five));
        let mut leases = vec![];
        for _ in 0..MAX_ACTIVE {
            leases.push(
                scheduler
                    .try_acquire(&operation("same.txt", false))
                    .unwrap(),
            );
        }
        assert!(
            scheduler
                .try_acquire(&operation("disjoint.txt", false))
                .is_err(),
            "owner bound applies across tasks"
        );
        drop(leases);
        assert!(!scheduler.busy());
    }
    #[tokio::test]
    async fn queued_conflicts_are_fair_but_independent_results_can_complete_out_of_order() {
        let (runtime, _owner, thread) = runtime();
        let scheduler = Arc::new(Scheduler::default());
        let slow_read = operation("slow.txt", false);
        let live = scheduler.try_acquire(&slow_read).unwrap();
        let write = operation("slow.txt", true);
        let pending = scheduler.acquire(&write, &runtime, thread, 0);
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut pending)
                .await
                .is_err()
        );
        assert!(
            scheduler.try_acquire(&slow_read).is_err(),
            "a later read cannot starve the writer"
        );
        let fast = scheduler
            .acquire(&operation("fast.txt", false), &runtime, thread, 0)
            .await
            .unwrap();
        let fast_id = fast.id;
        drop(fast);
        assert!(scheduler
            .queue
            .lock()
            .unwrap()
            .active
            .iter()
            .all(|(id, _)| *id != fast_id));
        assert_eq!(scheduler.queue.lock().unwrap().active.len(), 1);
        drop(live);
        let written = pending.await.unwrap();
        assert_eq!(scheduler.queue.lock().unwrap().active.len(), 1);
        drop(written);
        assert!(!scheduler.busy());
    }
    #[tokio::test]
    async fn pause_generation_cancels_queued_callbacks_even_after_resume_and_drop_releases_queue() {
        let (runtime, _owner, thread) = runtime();
        let child = ThreadId::new();
        let grandchild = ThreadId::new();
        for (id, parent) in [(child, thread), (grandchild, child)] {
            runtime.0.state.lock().unwrap().entries.insert(
                id,
                crate::Entry {
                    thread: std::sync::Weak::new(),
                    parent: Some(parent),
                    held: false,
                    interrupted: false,
                    interruption_error: None,
                    starts: 0,
                    admission_generation: 0,
                },
            );
        }
        let held = runtime
            .hold(child, &runtime.inspect(child).unwrap().revision)
            .unwrap();
        assert_eq!(
            runtime.admission_generation(thread).unwrap(),
            0,
            "sibling/parent queue is not invalidated by a child hold"
        );
        assert!(runtime.admission_generation(grandchild).is_err());
        // Synthetic entries have no retained controller to interrupt, but the
        // admission fence is established synchronously for exactly this subtree.
        assert_eq!(held.wait().await, Err(crate::Error::InterruptFailed));
        let state = runtime.0.state.lock().unwrap();
        assert_eq!(state.entries[&child].admission_generation, 1);
        assert_eq!(state.entries[&grandchild].admission_generation, 1);
        drop(state);
        let scheduler = Arc::new(Scheduler::default());
        let op = operation("file.txt", true);
        let live = scheduler.try_acquire(&op).unwrap();
        let pending = scheduler.acquire(&op, &runtime, thread, 0);
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut pending)
                .await
                .is_err()
        );
        runtime
            .0
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut(&thread)
            .unwrap()
            .admission_generation += 1;
        runtime.0.changed.notify_waiters();
        #[cfg(windows)]
        {
            let temp = tempfile::tempdir().unwrap();
            let missing = temp.path().join("never-launched.exe");
            let environment = std::collections::BTreeMap::new();
            let error = runtime
                .spawn_bounded_process_with_capture_generation(
                    thread,
                    &missing,
                    &[],
                    temp.path(),
                    &environment,
                    1024,
                    None,
                    None,
                    None,
                    false,
                    None,
                    Some(0),
                )
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "process launch sealed");
            let error = runtime
                .spawn_pty_with_capture_generation(
                    thread,
                    &missing,
                    &[],
                    temp.path(),
                    &environment,
                    codex_utils_pty::TerminalSize { rows: 24, cols: 80 },
                    None,
                    crate::process::Limits {
                        timeout: Duration::from_secs(1),
                        output_bytes: 1024,
                        process_count: 1,
                    },
                    Arc::new(|_| Ok(())),
                    Arc::new(()),
                    Some(0),
                )
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "PTY launch sealed");
            assert!(runtime
                .0
                .state
                .lock()
                .unwrap()
                .work
                .iter()
                .all(|work| work.receipt.is_some()));
        }
        assert!(pending.await.err().unwrap().contains("invalidated"));
        assert!(scheduler.queue.lock().unwrap().waiting.is_empty());
        {
            let cancelled = scheduler.acquire(&op, &runtime, thread, 1);
            tokio::pin!(cancelled);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut cancelled)
                    .await
                    .is_err()
            );
        }
        assert!(scheduler.queue.lock().unwrap().waiting.is_empty());
        drop(live);
        assert!(!scheduler.busy());
    }
    #[tokio::test]
    async fn queued_deadlines_are_bounded_without_releasing_running_claim() {
        let (runtime, _owner, thread) = runtime();
        let scheduler = Arc::new(Scheduler::default());
        let mut op = operation("file.txt", true);
        // A delayed callback sees free resources, but its original deadline has
        // already expired. It must not claim the slot before polling its timer.
        let expired = tokio::time::Instant::now() - Duration::from_millis(1);
        assert!(scheduler
            .acquire_until(&op, &runtime, thread, 0, expired)
            .await
            .err()
            .unwrap()
            .contains("deadline"));
        assert!(!scheduler.busy());
        let live = scheduler.try_acquire(&op).unwrap();
        op.timeout_ms = Units::new(10);
        assert!(scheduler
            .acquire(&op, &runtime, thread, 0)
            .await
            .err()
            .unwrap()
            .contains("deadline"));
        assert!(scheduler.queue.lock().unwrap().waiting.is_empty());
        assert_eq!(scheduler.queue.lock().unwrap().active.len(), 1);
        drop(live);
    }
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_native_start_observation_fences_queue_before_releasing_its_claim() {
        for draining_subtree in [false, true] {
            let (runtime, _owner, thread) = runtime();
            let scheduler = Arc::new(Scheduler::default());
            let op = operation("effect.txt", true);
            let live = scheduler.try_acquire(&op).unwrap();
            let queued = scheduler.acquire(&op, &runtime, thread, 0);
            tokio::pin!(queued);
            assert!(tokio::time::timeout(Duration::from_millis(10), &mut queued)
                .await
                .is_err());
            let temp = tempfile::tempdir().unwrap();
            let marker = temp.path().join("launched.txt");
            let executable =
                PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
            let environment = std::collections::BTreeMap::from([(
                "SystemRoot".into(),
                std::env::var_os("SystemRoot").unwrap(),
            )]);
            let arguments = [
                "/d".into(),
                "/s".into(),
                "/c".into(),
                std::ffi::OsString::from(format!(
                    "echo launched>\"{}\" & for /l %i in (1,0,2) do @rem",
                    marker.display()
                )),
            ];
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> =
                super::super::execution::observe_process_start(&runtime, || {
                    let _process = runtime.spawn_bounded_process_with_capture_generation(
                        thread,
                        &executable,
                        &arguments,
                        temp.path(),
                        &environment,
                        1024,
                        None,
                        None,
                        Some(crate::process::Limits {
                            timeout: Duration::from_secs(5),
                            output_bytes: 2048,
                            process_count: 1,
                        }),
                        true,
                        None,
                        Some(0),
                    )?;
                    let until = std::time::Instant::now() + Duration::from_secs(2);
                    while !marker.exists() {
                        if std::time::Instant::now() >= until {
                            return Err("native fixture did not launch".into());
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    // Model a sibling/subtree drain already holding the coordinator,
                    // while this root can still admit work unless explicitly fenced.
                    runtime.0.state.lock().unwrap().sealing = draining_subtree;
                    Err("injected process-start artifact failure after native launch".into())
                });
            assert!(result.unwrap_err().to_string().contains("artifact failure"));
            assert!(runtime.inspect(thread).unwrap().local_hold);
            assert_eq!(
                runtime.inspect(thread).unwrap().owner_attached,
                !draining_subtree
            );
            assert_eq!(scheduler.queue.lock().unwrap().active.len(), 1);
            drop(live);
            assert!(
                queued.await.is_err(),
                "startup failure must invalidate the queued effect before release"
            );
            runtime.stop_processes(&[thread]).await.unwrap();
            assert!(runtime.0.jobs.lock().unwrap()[&thread]
                .iter()
                .all(|job| job.active_process_count().unwrap() == 0));
            assert!(!scheduler.busy());
        }
    }
}
