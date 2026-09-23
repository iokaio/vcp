// SPDX-License-Identifier: Apache-2.0
//! P8-01/M9: bounded synthetic controller traces, not provider/effect campaigns.
//! The stream generator has no access to the admission oracle or product state.
//! Reopen is cooperative owner close/drop/open, not process-kill qualification.
use super::*;
use vcp_lifecycle::foundation::CanonicalOwner;
use vcp_protocol::command::{CommandEnvelope, CommandReceipt};
use vcp_store::contract::State;

const SEEDS: [u64; 4] = [1, 0x5eed, 0x8a01, 0xc0ffee];
const GENERATED_STEPS: usize = 48;

#[derive(Clone, Copy, Debug)]
enum Poison {
    None,
    Caller,
    Workspace,
    Controller,
    Epoch,
    Revision,
    Steering,
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Transition(TaskState),
    Stale(TaskState),
    EmptyReason(TaskState),
    Stop(Poison),
    Replay,
    Reopen,
}

/// Fixed xorshift64 stream; invented weights, not a fitted production model.
fn generated(seed: u64) -> Vec<Op> {
    let mut value = seed;
    (0..GENERATED_STEPS)
        .map(|_| {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            let target = [
                TaskState::Running,
                TaskState::WaitingForInput,
                TaskState::Blocked,
                TaskState::Paused,
                TaskState::Pending,
                TaskState::Completed,
            ][((value >> 8) % 6) as usize];
            match value % 10 {
                0..=3 => Op::Transition(target),
                4 => Op::Stale(target),
                5 => Op::EmptyReason(target),
                6 => Op::Stop(
                    [
                        Poison::None,
                        Poison::Caller,
                        Poison::Workspace,
                        Poison::Controller,
                        Poison::Epoch,
                        Poison::Revision,
                        Poison::Steering,
                    ][((value >> 16) % 7) as usize],
                ),
                7 => Op::Stop(Poison::None),
                8 => Op::Replay,
                _ => Op::Reopen,
            }
        })
        .collect()
}

/// Independent fixture contract: no verification or resume evidence is supplied.
/// Do not call Task::transition, can_dispatch or another product validator here.
fn admitted(from: TaskState, to: TaskState) -> bool {
    use TaskState::*;
    matches!(
        (from, to),
        (Running, WaitingForInput | Blocked | Paused | Cancelled)
            | (WaitingForInput, Blocked | Paused | Cancelled)
            | (Blocked, WaitingForInput | Paused | Cancelled)
            | (Paused, WaitingForInput | Blocked | Paused | Cancelled)
    )
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn root_task(state: &State, config: &Config) -> Result<Task, String> {
    state
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .and_then(|record| record.decode())
        .map_err(|error| error.to_string())
}
fn transition(next: TaskState, reason: String) -> Command {
    Command::Transition {
        next,
        reason,
        verification: None,
    }
}

#[derive(Default, Debug)]
struct Coverage {
    accepted: usize,
    denied: usize,
    replayed: usize,
    reopened: usize,
}

struct Trace {
    host: Option<CanonicalHost>,
    owner: Option<CanonicalOwner>,
    config: Config,
    original: Task,
    expected_state: TaskState,
    expected_revision: u64,
    replay: Option<(CommandEnvelope, CommandReceipt)>,
    coverage: Coverage,
}

impl Trace {
    fn host(&self) -> &CanonicalHost {
        self.host.as_ref().unwrap()
    }

    fn invariant(&self, state: &State) -> Result<(), String> {
        let task = root_task(state, &self.config)?;
        require(
            task.state == self.expected_state,
            format!("state {:?}, expected {:?}", task.state, self.expected_state),
        )?;
        require(
            task.revision.get() == self.expected_revision,
            format!(
                "revision {}, expected {}",
                task.revision.get(),
                self.expected_revision
            ),
        )?;
        // Everything outside the four fields owned by a task transition is
        // immutable in this trace, including scope, objective and steering.
        let mut unchanged = task.clone();
        unchanged.state = self.original.state;
        unchanged.revision = self.original.revision;
        unchanged.cause = self.original.cause.clone();
        unchanged.reason = self.original.reason.clone();
        require(
            unchanged == self.original,
            "transition changed an unrelated task field",
        )?;
        require(
            state
                .events
                .iter()
                .any(|event| event.event.id == task.cause),
            "task cause has no retained event",
        )?;
        require(
            state
                .records
                .values()
                .filter(|record| record.collection == Collection::Task)
                .count()
                == 1,
            "unexpected task creation",
        )?;
        // No executable thread, tool, provider or budget is requested. These
        // zero-effect assertions are not unknown/late accounting qualification.
        require(
            !state.records.values().any(|record| {
                matches!(
                    record.collection,
                    Collection::Attempt
                        | Collection::Reservation
                        | Collection::Ledger
                        | Collection::Settlement
                        | Collection::Effect
                        | Collection::Turn
                )
            }),
            "controller-only trace created execution/accounting state",
        )?;
        require(
            !state.events.iter().any(|event| {
                matches!(
                    event.event.kind,
                    vcp_protocol::event::EventKind::AttemptSubmitted
                        | vcp_protocol::event::EventKind::ReservationCreated
                        | vcp_protocol::event::EventKind::UsageReconciled
                        | vcp_protocol::event::EventKind::LiabilityRetained
                        | vcp_protocol::event::EventKind::EffectTransition
                )
            }),
            "controller-only trace emitted execution/accounting event",
        )
    }

    fn acknowledged_prefix(before: &State, after: &State) -> Result<(), String> {
        require(
            after.events.starts_with(&before.events),
            "acknowledged event prefix changed or disappeared",
        )?;
        require(
            before
                .commands
                .iter()
                .all(|(key, receipt)| after.commands.get(key) == Some(receipt)),
            "acknowledged command receipt changed or disappeared",
        )?;
        require(
            before
                .transactions
                .iter()
                .all(|(key, receipt)| after.transactions.get(key) == Some(receipt)),
            "acknowledged transaction receipt changed or disappeared",
        )
    }

    fn envelope(&self, reason: String) -> Result<CommandEnvelope, String> {
        self.host().control_envelope(
            CommandId::new(),
            self.config.root_task.clone(),
            Revision::new(self.expected_revision),
            transition(TaskState::Paused, reason),
        )
    }

    async fn apply(&mut self, op: Op, index: usize) -> Result<(), String> {
        let before = self.host().snapshot()?;
        let reason = format!("seeded controller step {index}");
        match op {
            Op::Transition(next) | Op::Stale(next) | Op::EmptyReason(next) => {
                let valid = matches!(op, Op::Transition(_)) && admitted(self.expected_state, next);
                let expected = if matches!(op, Op::Stale(_)) {
                    self.expected_revision - 1
                } else {
                    self.expected_revision
                };
                let reason = if matches!(op, Op::EmptyReason(_)) {
                    String::new()
                } else {
                    reason
                };
                let result = self.host().command(
                    transition(next, reason),
                    Some(self.config.root_task.clone()),
                    Revision::new(expected),
                );
                self.transition_result(&before, next, valid, result)?;
            }
            Op::Stop(poison) => {
                let mut command = self.envelope(reason)?;
                match poison {
                    Poison::None => {}
                    Poison::Caller => {
                        command.caller = ActorId::parse("foreign-trace-caller").unwrap()
                    }
                    Poison::Workspace => {
                        command.workspace = WorkspaceId::parse("foreign-trace-workspace").unwrap()
                    }
                    Poison::Controller => {
                        command.controller =
                            ControllerId::parse("foreign-trace-controller").unwrap()
                    }
                    Poison::Epoch => {
                        command.owner_epoch = OwnerEpoch::new(command.owner_epoch.get() + 1)
                    }
                    Poison::Revision => {
                        command.expected = Revision::new(self.expected_revision - 1)
                    }
                    Poison::Steering => {
                        command.steering = SteeringRevision::new(command.steering.get() + 1)
                    }
                }
                let valid = matches!(poison, Poison::None)
                    && admitted(self.expected_state, TaskState::Paused)
                    && self.expected_state != TaskState::Running;
                let result = self.host().stop(command.clone());
                if valid {
                    self.replay = result
                        .as_ref()
                        .ok()
                        .cloned()
                        .map(|receipt| (command, receipt));
                }
                self.transition_result(&before, TaskState::Paused, valid, result)?;
            }
            Op::Replay => {
                let (command, receipt) = self
                    .replay
                    .as_ref()
                    .ok_or("missing acknowledged replay fixture")?;
                let replay = self.host().stop(command.clone())?;
                require(
                    replay == *receipt,
                    "replay returned a different durable receipt",
                )?;
                require(
                    self.host().snapshot()? == before,
                    "acknowledged replay changed canonical state",
                )?;
                self.coverage.replayed += 1;
            }
            Op::Reopen => {
                // This ID was never acknowledged. It must not acquire authority
                // merely because the same workspace is reopened by a new owner.
                let old_owner_command = self.envelope(reason)?;
                self.owner.take().ok_or("missing owner")?.close().await?;
                if self.expected_state == TaskState::Running {
                    self.expected_state = TaskState::Paused;
                    self.expected_revision += 1;
                }
                let closed = self.host().snapshot()?;
                self.invariant(&closed)?;
                Self::acknowledged_prefix(&before, &closed)?;
                let denied = self
                    .host()
                    .stop(old_owner_command.clone())
                    .err()
                    .ok_or("closed owner accepted a command")?;
                require(
                    denied.contains("stale or closed control owner"),
                    format!("unexpected closed-owner denial: {denied}"),
                )?;
                require(
                    self.host().snapshot()? == closed,
                    "closed-owner denial changed state",
                )?;
                drop(self.host.take());
                let (host, owner) = CanonicalHost::open(self.config.clone())?;
                self.host = Some(host);
                self.owner = Some(owner);
                let reopened = self.host().snapshot()?;
                self.invariant(&reopened)?;
                Self::acknowledged_prefix(&closed, &reopened)?;
                let denied = self
                    .host()
                    .stop(old_owner_command)
                    .err()
                    .ok_or("reopen accepted old owner authority")?;
                require(
                    denied.contains("stale or closed control owner"),
                    format!("unexpected stale-owner denial: {denied}"),
                )?;
                require(
                    self.host().snapshot()? == reopened,
                    "stale-owner denial changed state",
                )?;
                self.coverage.reopened += 1;
            }
        }
        let after = self.host().snapshot()?;
        Self::acknowledged_prefix(&before, &after)?;
        self.invariant(&after)
    }

    fn transition_result(
        &mut self,
        before: &State,
        next: TaskState,
        valid: bool,
        result: Result<CommandReceipt, String>,
    ) -> Result<(), String> {
        require(
            result.is_ok() == valid,
            format!("admission expected {valid}; result {result:?}"),
        )?;
        let after = self.host().snapshot()?;
        if valid {
            self.coverage.accepted += 1;
            if self.expected_state != next {
                self.expected_revision += 1;
                self.expected_state = next;
            } else {
                require(
                    root_task(before, &self.config)? == root_task(&after, &self.config)?,
                    "repeated pause changed task",
                )?;
            }
            require(
                after.watermark > before.watermark,
                "accepted command lacks durable acknowledgement",
            )?;
        } else {
            self.coverage.denied += 1;
            require(after == *before, "denied command changed canonical state")?;
        }
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_controller_traces_preserve_independent_state_and_reopen_invariants() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for seed in SEEDS {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            task(&host, &config, config.root_task.clone(), None);
            let original = root_task(&host.snapshot().unwrap(), &config).unwrap();
            assert_eq!(original.state, TaskState::Running);
            assert_eq!(original.revision, Revision::new(1));
            let mut trace = Trace {
                host: Some(host),
                owner: Some(owner),
                config,
                original,
                expected_state: TaskState::Running,
                expected_revision: 1,
                replay: None,
                coverage: Coverage::default(),
            };
            let mut stream = vec![
                Op::Reopen,
                Op::Stop(Poison::None),
                Op::Replay,
                Op::Transition(TaskState::WaitingForInput),
                Op::Transition(TaskState::Blocked),
                Op::Transition(TaskState::Paused),
                Op::Transition(TaskState::Running),
                Op::Transition(TaskState::Completed),
                Op::Transition(TaskState::Pending),
                Op::Stale(TaskState::WaitingForInput),
                Op::EmptyReason(TaskState::WaitingForInput),
            ];
            stream.extend(
                [
                    Poison::Caller,
                    Poison::Workspace,
                    Poison::Controller,
                    Poison::Epoch,
                    Poison::Revision,
                    Poison::Steering,
                ]
                .map(Op::Stop),
            );
            stream.push(Op::Reopen);
            stream.extend(generated(seed));
            stream.push(Op::Transition(TaskState::Cancelled));
            stream.extend(
                [
                    TaskState::Pending,
                    TaskState::Running,
                    TaskState::WaitingForInput,
                    TaskState::Blocked,
                    TaskState::Paused,
                    TaskState::Completed,
                    TaskState::Failed,
                    TaskState::Cancelled,
                ]
                .map(Op::Transition),
            );
            stream.extend([Op::Stop(Poison::None), Op::Replay, Op::Reopen]);
            for (index, op) in stream.iter().copied().enumerate() {
                if let Err(error) = trace.apply(op, index).await {
                    // Stop at the earliest failing operation. This exact bounded
                    // prefix reproduces the first failure without later actions.
                    panic!("backend={backend:?} seed={seed:#x} step={index} error={error}; shortest executed failing prefix={:?}", &stream[..=index]);
                }
            }
            assert!(
                trace.coverage.accepted >= 5
                    && trace.coverage.denied >= 20
                    && trace.coverage.replayed >= 2
                    && trace.coverage.reopened >= 3,
                "mandatory coverage missing: backend={backend:?} seed={seed:#x} {:?}",
                trace.coverage
            );
            eprintln!("seeded-controller backend={backend:?} seed={seed:#x} generated={GENERATED_STEPS} total={} {:?}", stream.len(), trace.coverage);
            trace.owner.take().unwrap().close().await.unwrap();
        }
    }
}
