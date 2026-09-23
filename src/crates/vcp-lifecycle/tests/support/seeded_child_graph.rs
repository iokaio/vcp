// SPDX-License-Identifier: Apache-2.0
//! P8-01/M9 partial: invented bounded child schedules, loopback scripted provider.
//! Cooperative close/reopen is not a process kill or packaged/live delegation.
//! Four seed-selected scenarios use two sibling permutations, homogeneous read-
//! only/write pairs, and individual settled pause/cancel or in-flight root stop.
//! Provider capture uncertainty must remain fenced; it is not held attachment.
use super::*;
use std::collections::BTreeMap;

const SEEDS: [u64; 4] = [1, 0x5eed, 0x8a01, 0xc0ffee];
#[derive(Clone, Copy, Debug)]
enum Op {
    Assign(usize),
    Overflow,
    Start(usize),
    Send(usize),
    Stop,
    Drain,
    Probe(usize),
    Stale,
    StopRoot,
    Reopen,
    Recover(usize),
}
fn schedule(seed: u64) -> Vec<Op> {
    let a = (seed & 1) as usize;
    let b = 1 - a;
    let mut operations = vec![
        Op::Assign(a),
        Op::Assign(b),
        Op::Overflow,
        Op::Start(b),
        Op::Start(a),
        Op::Send(a),
        Op::Send(b),
        Op::Stop,
        Op::Drain,
        Op::Probe(a),
        Op::Probe(b),
        Op::Stale,
        Op::StopRoot,
        Op::Probe(b),
        Op::Probe(a),
        Op::Reopen,
        Op::Recover(b),
        Op::Recover(a),
        Op::Probe(a),
        Op::Probe(b),
    ];
    // Provider capture cancellation requires reconciliation on reopen. Keep
    // that branch; settled individual stops qualify held native attachment.
    if seed & 2 == 0 {
        operations.swap(7, 8);
    }
    operations
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Charge {
    None,
    Active,
    Settled,
    Unknown,
}
struct Oracle {
    ids: [Option<TaskId>; 2],
    charge: [Charge; 2],
    stopped: [bool; 2],
    root_stopped: bool,
    task_states: [Option<TaskState>; 2],
    wire_digests: [Option<String>; 2],
    writing: bool,
}
impl Oracle {
    fn check(&self, f: &Fixture) {
        self.check_state(&f.host.snapshot().unwrap(), &f.config, &f.binding);
    }
    fn check_state(
        &self,
        state: &vcp_store::contract::State,
        config: &Config,
        binding: &ThreadBinding,
    ) {
        let current = |id: &TaskId| -> Task {
            state
                .record(Collection::Task, id.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap()
        };
        let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
        let sum = |value| {
            self.charge
                .iter()
                .filter(|charge| **charge == value)
                .count() as u64
                * 100
        };
        assert_eq!(
            (
                ledger.active.get(),
                ledger.settled.get(),
                ledger.unresolved.get()
            ),
            (
                sum(Charge::Active),
                sum(Charge::Settled),
                sum(Charge::Unknown)
            )
        );
        let ids = self.ids.iter().flatten().cloned().collect::<BTreeSet<_>>();
        let expected_tasks = ids
            .iter()
            .cloned()
            .chain(std::iter::once(config.root_task.clone()))
            .collect::<BTreeSet<_>>();
        let actual_tasks = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Task)
            .map(|r| r.decode::<Task>().unwrap().scope.task)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual_tasks, expected_tasks,
            "orphan task or refused allocation leaked a task"
        );
        assert_eq!(
            current(&config.root_task).state,
            if self.root_stopped {
                TaskState::Paused
            } else {
                TaskState::Running
            }
        );
        let allocations = ids
            .iter()
            .map(|id| (id.clone(), Micros::new(400)))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(ledger.allocations, allocations);
        let graph = vcp_engine::agents::graph(&state, &binding.scope, &config.root_task)
            .unwrap()
            .unwrap();
        assert_eq!(graph.children.keys().cloned().collect::<BTreeSet<_>>(), ids);
        for id in &ids {
            let task = current(id);
            assert_eq!(task.parent.as_ref(), Some(&config.root_task));
            assert_eq!(task.root, config.root_task);
            assert_eq!(task.scope.workspace, config.workspace);
        }
        let attempts = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Attempt)
            .map(|r| r.decode::<Attempt>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            attempts.len(),
            self.charge.iter().filter(|c| **c != Charge::None).count()
        );
        let reservations = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Reservation)
            .map(|r| r.decode::<Reservation>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reservations.len(), attempts.len());
        for (slot, id) in self.ids.iter().enumerate() {
            if let Some(id) = id {
                let spec = &graph.children[id];
                assert_eq!(spec.parent, config.root_task);
                assert!(spec.dependencies.is_empty());
                assert_eq!(
                    spec.mode,
                    if self.writing {
                        ChildMode::IsolatedWrite
                    } else {
                        ChildMode::ReadOnly
                    }
                );
                let actual_paths = spec
                    .paths
                    .iter()
                    .map(|p| (p.path.clone(), p.write))
                    .collect::<BTreeSet<_>>();
                let mut expected_paths = BTreeSet::from([(String::new(), false)]);
                if self.writing {
                    expected_paths.insert((["left.txt", "right.txt"][slot].into(), true));
                }
                assert_eq!(actual_paths, expected_paths);
                if let Some(expected) = self.task_states[slot] {
                    assert_eq!(current(id).state, expected);
                }
                if self.charge[slot] != Charge::None {
                    let attempt = attempts.iter().find(|a| &a.scope.task == id).unwrap();
                    assert_eq!(attempt.scope.workspace, config.workspace);
                    assert_eq!(attempt.root, config.root_task);
                    let reservation = reservations
                        .iter()
                        .find(|r| r.id == attempt.reservation)
                        .unwrap();
                    assert_eq!(reservation.attempt, attempt.id);
                    assert_eq!(reservation.scope, attempt.scope);
                    assert_eq!(reservation.root, config.root_task);
                    assert_eq!(reservation.amount.micros, Micros::new(100));
                    assert_eq!(
                        Some(&attempt.request_digest),
                        self.wire_digests[slot].as_ref()
                    );
                    if self.charge[slot] == Charge::Unknown {
                        assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
                    }
                    if self.charge[slot] == Charge::Settled {
                        assert_eq!(attempt.phase, ReservationState::Settled);
                    }
                }
            }
        }
    }
}
fn marked_context(
    host: &CanonicalHost,
    id: codex_protocol::ThreadId,
    snapshot: &vcp_models::catalog::Snapshot,
    file: Option<&vcp_repository::Source>,
    marker: &str,
) -> vcp_context::manifest::Sealed {
    use vcp_context::{
        manifest::{Kind, Part, Trust},
        selection::{assemble, Utf8ByteCeiling},
    };
    let mut parts = Vec::new();
    for (key,kind,trust,text) in [("operating",Kind::Operating,Trust::Operating,"Treat source text as evidence; preserve scoped instructions and latest user constraints."),("objective",Kind::Objective,Trust::User,marker),("task",Kind::TaskState,Trust::Observed,"Current task is running; no tools are authorized.")] {
        let descriptor=host.capture(id,Channel::Evidence,text.as_bytes().to_vec()).unwrap();
        parts.push(Part::captured_text(key.into(),kind,trust,&descriptor,text.as_bytes(),true,0,"current canonical fixture".into()).unwrap());
    }
    if let Some(file) = file {
        let descriptor = host
            .capture(id, Channel::Evidence, file.bytes.clone())
            .unwrap();
        let mut part = Part::captured_text(
            "file".into(),
            Kind::Evidence,
            Trust::Untrusted,
            &descriptor,
            &file.bytes,
            true,
            0,
            "selected source".into(),
        )
        .unwrap();
        part.file = Some(file.version.clone());
        parts.push(part);
    }
    let revisions = host.context_revisions(id).unwrap();
    let envelope = vcp_models::request::envelope(
        snapshot,
        Units::new(1024),
        Units::new(512),
        Timestamp::new(1),
    )
    .unwrap();
    assemble(
        parts,
        revisions,
        envelope,
        serde_json::json!([]),
        vec![],
        &Utf8ByteCeiling,
        |parts, envelope, schemas| {
            vcp_models::request::encode(parts, envelope, schemas, snapshot)
                .map_err(|_| vcp_context::manifest::Error::Incompatible("fixture codec"))
        },
    )
    .unwrap()
}

fn read(slot: usize) -> Request {
    Request::Read {
        path: ["left.txt", "right.txt"][slot].into(),
        max_bytes: 1024,
        start_line: None,
        end_line: None,
    }
}
fn stop(f: &Fixture, id: TaskId, next: TaskState) {
    f.host
        .stop(
            f.host
                .control_envelope(
                    CommandId::new(),
                    id.clone(),
                    f.current(&id).revision,
                    Command::Transition {
                        next,
                        reason: "seeded explicit stop".into(),
                        verification: None,
                    },
                )
                .unwrap(),
        )
        .unwrap();
}
async fn reopen(f: Fixture) -> Fixture {
    let Fixture {
        _temp,
        workspace,
        host: old_host,
        owner: old_owner,
        config,
        binding,
        parent: _,
        test: old_test,
        server,
        snapshotter,
        disposable,
        children,
    } = f;
    let before = old_host.snapshot().unwrap();
    old_owner.close().await.unwrap();
    for child in children {
        child.shutdown_and_wait().await.unwrap();
    }
    old_test.codex.shutdown_and_wait().await.unwrap();
    drop(old_test);
    drop(old_host);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let reopened = host.snapshot().unwrap();
    for (id, receipt) in &before.commands {
        assert_eq!(reopened.commands.get(id), Some(receipt));
    }
    for (id, receipt) in &before.transactions {
        assert_eq!(reopened.transactions.get(id), Some(receipt));
    }
    assert_eq!(
        &reopened.events[..before.events.len()],
        before.events.as_slice()
    );
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot, raw).unwrap();
    assert!(host.unfinished_captures().unwrap().is_empty());
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let startup = host.clone();
    let cwd = workspace.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-seeded-child",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = cwd.try_into().unwrap();
            configure_fixture_provider(c);
            startup
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let parent = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(parent, binding.clone()).unwrap();
    let view = host.lifecycle().inspect(parent).unwrap();
    host.lifecycle()
        .hold(parent, &view.revision)
        .unwrap()
        .wait()
        .await
        .unwrap();
    Fixture {
        _temp,
        workspace,
        host,
        owner,
        config,
        binding,
        parent,
        test,
        server,
        snapshotter,
        disposable,
        children: vec![],
    }
}

async fn fenced_reopen(
    f: Fixture,
    oracle: &Oracle,
    remaining: &[Op],
    seed: u64,
    backend: BackendKind,
) {
    let Fixture {
        _temp,
        host,
        owner,
        config,
        binding,
        test,
        server,
        children,
        parent,
        ..
    } = f;
    let before = host.snapshot().unwrap();
    owner.close().await.unwrap();
    for child in children {
        child.shutdown_and_wait().await.unwrap();
    }
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    drop(host);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let reopened = host.snapshot().unwrap();
    for (id, receipt) in &before.commands {
        assert_eq!(reopened.commands.get(id), Some(receipt));
    }
    for (id, receipt) in &before.transactions {
        assert_eq!(reopened.transactions.get(id), Some(receipt));
    }
    assert_eq!(
        &reopened.events[..before.events.len()],
        before.events.as_slice()
    );
    oracle.check_state(&reopened, &config, &binding);
    let captures = host.unfinished_captures().unwrap();
    let responses = captures
        .iter()
        .filter(|c| c.spec.channel == Channel::Response)
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert_eq!(
        responses
            .iter()
            .map(|c| c.spec.scope.task.clone())
            .collect::<BTreeSet<_>>(),
        oracle.ids.iter().flatten().cloned().collect()
    );
    assert_eq!(remaining.len(), 4);
    for operation in remaining {
        eprintln!("seeded child seed={seed:#x} backend={backend:?} fenced recovery operation={operation:?}");
        match operation {
            Op::Recover(slot) => {
                let mut child = binding.clone();
                child.scope.task = oracle.ids[*slot].clone().unwrap();
                child.role = RequestRole::Child;
                assert_eq!(
                    host.register(parent, child).unwrap_err(),
                    "canonical capture/admission fenced; reopen required"
                );
            }
            Op::Probe(_) => {
                let admitted = codex_extension_api::HostWorkAdmission::admit_startup(
                    &host,
                    std::path::Path::new(&config.binding.root),
                    None,
                );
                assert!(matches!(admitted,Err(ref error) if error=="canonical host fenced"));
            }
            _ => panic!("unexpected operation after capture-fenced reopen"),
        }
        assert_eq!(
            host.snapshot().unwrap(),
            reopened,
            "fenced admission changed acknowledged state"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        oracle.check_state(&reopened, &config, &binding);
    }
    owner.close().await.unwrap();
    drop(host);
    drop(_temp);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_child_stop_reopen_preserves_graph_and_root_liability() {
    let mut recovered = 0usize;
    let mut capture_blocked = 0usize;
    let mut cancelled_blocked = 0usize;
    let arms = SEEDS.map(|seed| {
        (
            ((seed >> 7) & 1) as usize,
            seed & 2 != 0,
            seed & 0x400 != 0 && seed & 2 == 0,
        )
    });
    assert!(arms.iter().any(|a| a.0 == 0) && arms.iter().any(|a| a.0 == 1));
    assert!(
        arms.iter().any(|a| a.1) && arms.iter().any(|a| a.2) && arms.iter().any(|a| !a.1 && !a.2)
    );
    assert!(
        SEEDS.iter().any(|seed| seed & 0x200 == 0) && SEEDS.iter().any(|seed| seed & 0x200 != 0)
    );
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        'traces: for seed in SEEDS {
            let ops = schedule(seed);
            assert_eq!(ops.len(), 20);
            let selected = ((seed >> 7) & 1) as usize;
            let root_stop = seed & 2 != 0;
            let cancelled = seed & 0x400 != 0 && !root_stop;
            let mut f = Fixture::new(backend, 0).await;
            f.server.reset().await;
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(vec![serde_json::json!({"type":"response.completed","response":{"id":"seeded-child","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})])).set_delay(Duration::from_secs(5))).expect(2).mount(&f.server).await;
            let mut oracle = Oracle {
                ids: [None, None],
                charge: [Charge::None; 2],
                stopped: [false; 2],
                root_stopped: false,
                task_states: [None, None],
                wire_digests: [None, None],
                writing: seed & 0x200 != 0,
            };
            let mut threads = [None, None];
            let mut loops: [Option<Arc<codex_core::CodexThread>>; 2] = [None, None];
            let mut paths = [None, None];
            for (index, op) in ops.iter().enumerate() {
                eprintln!(
                    "seeded child seed={seed:#x} backend={backend:?} prefix={:?}",
                    &ops[..=index]
                );
                match *op {
                    Op::Assign(slot) => {
                        oracle.task_states[slot] = Some(TaskState::Pending);
                        let path = ["left.txt", "right.txt"][slot];
                        let writing = (seed >> 9) & 1 != 0;
                        oracle.ids[slot] = Some(
                            f.host
                                .delegate_child(
                                    f.parent,
                                    DelegationRequest {
                                        role: "seeded isolated sibling".into(),
                                        read_paths: BTreeSet::from([String::new()]),
                                        helper: None,
                                        objective: format!("Observe assigned {path} only"),
                                        acceptance: vec![
                                            "Preserve sibling and parent sources".into()
                                        ],
                                        mode: if writing {
                                            ChildMode::IsolatedWrite
                                        } else {
                                            ChildMode::ReadOnly
                                        },
                                        write_paths: if writing {
                                            BTreeSet::from([path.into()])
                                        } else {
                                            BTreeSet::new()
                                        },
                                        untracked_inputs: ["left.txt", "right.txt", "outside.txt"]
                                            .into_iter()
                                            .map(String::from)
                                            .collect(),
                                        allocation: Micros::new(400),
                                        deadline: Timestamp::new(u64::MAX),
                                        required_checks: vec![],
                                    },
                                    &f.snapshotter,
                                    &f.disposable,
                                )
                                .await
                                .unwrap(),
                        );
                    }
                    Op::Overflow => {
                        let before = f.host.snapshot().unwrap();
                        let result = f
                            .host
                            .delegate_child(
                                f.parent,
                                DelegationRequest {
                                    role: "overflow".into(),
                                    read_paths: BTreeSet::from([String::new()]),
                                    helper: None,
                                    objective: "Must refuse allocation beyond root cap".into(),
                                    acceptance: vec!["No child added".into()],
                                    mode: ChildMode::ReadOnly,
                                    write_paths: BTreeSet::new(),
                                    untracked_inputs: BTreeSet::new(),
                                    allocation: Micros::new(400),
                                    deadline: Timestamp::new(u64::MAX),
                                    required_checks: vec![],
                                },
                                &f.snapshotter,
                                &f.disposable,
                            )
                            .await;
                        assert!(result.is_err());
                        assert_eq!(
                            vcp_budget::ledger(&f.host.snapshot().unwrap(), &f.binding.scope)
                                .unwrap(),
                            vcp_budget::ledger(&before, &f.binding.scope).unwrap()
                        );
                    }
                    Op::Start(slot) => {
                        oracle.task_states[slot] = Some(TaskState::Running);
                        let (thread, path, child) =
                            f.start(oracle.ids[slot].as_ref().unwrap()).await;
                        threads[slot] = Some(thread);
                        paths[slot] = Some(path);
                        loops[slot] = Some(child);
                    }
                    Op::Send(slot) => {
                        let thread = threads[slot].unwrap();
                        let (snapshot, _) = provider_snapshot();
                        let sealed = marked_context(
                            &f.host,
                            thread,
                            &snapshot,
                            None,
                            &format!("seeded-child-{seed:x}-slot-{slot}"),
                        );
                        f.host
                            .prepare_context(thread, sealed, serde_json::json!([]), vec![])
                            .unwrap();
                        loops[slot]
                            .as_ref()
                            .unwrap()
                            .start_or_steer_turn(TurnInputRequest::user_input(vec![
                                UserInput::Text {
                                    text: format!("seeded-child-{seed:x}-slot-{slot}: Observe assigned sources only"),
                                    text_elements: vec![],
                                },
                            ]))
                            .await
                            .unwrap();
                        let sent = oracle.charge.iter().filter(|c| **c != Charge::None).count() + 1;
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while f.server.received_requests().await.unwrap().len() != sent {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .unwrap();
                        oracle.charge[slot] = Charge::Active;
                        let wire = f.server.received_requests().await.unwrap();
                        let marker = format!("seeded-child-{seed:x}-slot-{slot}");
                        let matching = wire
                            .iter()
                            .filter(|r| String::from_utf8_lossy(&r.body).contains(&marker))
                            .collect::<Vec<_>>();
                        assert_eq!(
                            matching.len(),
                            1,
                            "unique task marker missing or duplicated on wire"
                        );
                        oracle.wire_digests[slot] =
                            Some(vcp_protocol::digest_bytes(&matching[0].body));
                    }
                    Op::Stop => {
                        stop(
                            &f,
                            if root_stop {
                                f.config.root_task.clone()
                            } else {
                                oracle.ids[selected].clone().unwrap()
                            },
                            if cancelled {
                                TaskState::Cancelled
                            } else {
                                TaskState::Paused
                            },
                        );
                        for slot in 0..2 {
                            if root_stop || slot == selected {
                                if oracle.charge[slot] == Charge::Active {
                                    wait_for_event_with_timeout(
                                        loops[slot].as_ref().unwrap(),
                                        |e| matches!(e, EventMsg::TurnAborted(_)),
                                        Duration::from_secs(10),
                                    )
                                    .await;
                                    oracle.charge[slot] = Charge::Unknown;
                                }
                                oracle.stopped[slot] = true;
                                // Root stop fences descendants through inherited holds;
                                // only the explicitly addressed task changes durable state.
                                if !root_stop {
                                    oracle.task_states[slot] = Some(if cancelled {
                                        TaskState::Cancelled
                                    } else {
                                        TaskState::Paused
                                    });
                                }
                            }
                        }
                        oracle.root_stopped = root_stop;
                    }
                    Op::Drain => {
                        for slot in 0..2 {
                            if !oracle.stopped[slot] {
                                wait_for_event_with_timeout(
                                    loops[slot].as_ref().unwrap(),
                                    |e| matches!(e, EventMsg::TurnComplete(_)),
                                    Duration::from_secs(15),
                                )
                                .await;
                                oracle.charge[slot] = Charge::Settled;
                            }
                        }
                    }
                    Op::Probe(slot) => {
                        if let Some(thread) = threads[slot] {
                            let result = f.host.prepare_tool(thread, read(slot));
                            if oracle.stopped[slot] || oracle.root_stopped {
                                assert!(result.is_err());
                            } else {
                                assert_eq!(
                                    f.host.dispatch_tool(result.unwrap()).unwrap().result["text"],
                                    "base\n"
                                );
                            }
                        } else {
                            if root_stop {
                                assert_eq!(
                                    f.host.prepare_tool(f.parent, read(slot)).err().unwrap(),
                                    "retained thread has no canonical scope"
                                );
                            } else {
                                assert!(cancelled && slot == selected);
                                assert_eq!(
                                    f.current(oracle.ids[slot].as_ref().unwrap()).state,
                                    TaskState::Cancelled
                                );
                                assert!(f
                                    .host
                                    .prepare_child_start(
                                        f.parent,
                                        oracle.ids[slot].clone().unwrap(),
                                        &f.snapshotter
                                    )
                                    .await
                                    .is_err());
                            }
                        }
                    }
                    Op::Stale => {
                        let before = f.host.snapshot().unwrap();
                        let id = f.config.root_task.clone();
                        let mut envelope = f
                            .host
                            .control_envelope(
                                CommandId::new(),
                                id.clone(),
                                f.current(&id).revision,
                                Command::Transition {
                                    next: TaskState::Paused,
                                    reason: "stale stop".into(),
                                    verification: None,
                                },
                            )
                            .unwrap();
                        envelope.expected = Revision::ZERO;
                        assert!(f.host.stop(envelope).is_err());
                        assert_eq!(f.host.snapshot().unwrap(), before);
                    }
                    Op::StopRoot => {
                        stop(&f, f.config.root_task.clone(), TaskState::Paused);
                        oracle.root_stopped = true;
                    }
                    Op::Reopen => {
                        let before = f.server.received_requests().await.unwrap().len();
                        loops = [None, None];
                        if root_stop {
                            for slot in 0..2 {
                                oracle.task_states[slot] = Some(TaskState::Paused);
                            }
                            for path in paths.iter().flatten().chain(std::iter::once(&f.workspace))
                            {
                                assert_eq!(fs::read(path.join("left.txt")).unwrap(), b"base\n");
                            }
                            fenced_reopen(f, &oracle, &ops[index + 1..], seed, backend).await;
                            capture_blocked += 2;
                            eprintln!("seeded child completed seed={seed:#x} backend={backend:?} operations=20 assignments=2 overallocated_refusals=1 wire_requests=2 cooperative_reopens=1 capture_fenced=true");
                            continue 'traces;
                        }
                        f = reopen(f).await;
                        threads = [None, None];
                        // Fresh owner recovery pauses nonterminal tasks; it does not
                        // resume descendants merely because their parent is attached.
                        for slot in 0..2 {
                            if oracle.task_states[slot] != Some(TaskState::Cancelled) {
                                oracle.task_states[slot] = Some(TaskState::Paused);
                            }
                        }
                        assert_eq!(f.server.received_requests().await.unwrap().len(), before);
                    }
                    Op::Recover(slot) => {
                        let recovery = f
                            .host
                            .prepare_child_recovery(
                                f.parent,
                                oracle.ids[slot].clone().unwrap(),
                                &f.snapshotter,
                            )
                            .await
                            .unwrap();
                        if cancelled && slot == selected {
                            assert!(recovery.ticket.is_none());
                            assert!(recovery.blocked.is_some());
                            cancelled_blocked += 1;
                        } else {
                            let mut ticket = recovery.ticket.unwrap();
                            f.host
                                .authorize_child_recovery_startup(&mut ticket)
                                .unwrap();
                            let mut config = f.test.config.clone();
                            config.cwd = ticket.workspace().to_path_buf().try_into().unwrap();
                            let mut extensions = codex_extension_api::ExtensionDataInit::default();
                            extensions.insert(AllowedTools(vec![]));
                            let child = f
                                .test
                                .thread_manager
                                .start_thread(StartThreadOptions {
                                    thread_extension_init: extensions,
                                    environments: Some(f.test.codex.environment_selections().await),
                                    ..StartThreadOptions::new(config)
                                })
                                .await
                                .unwrap()
                                .thread;
                            let thread = f
                                .host
                                .attach_recovered_child(ticket, child.clone())
                                .await
                                .unwrap();
                            assert!(f.host.lifecycle().inspect(thread).unwrap().local_hold);
                            threads[slot] = Some(thread);
                            loops[slot] = Some(child.clone());
                            f.children.push(child);
                            oracle.stopped[slot] = true;
                            oracle.task_states[slot] = Some(TaskState::Paused);
                            recovered += 1;
                        }
                    }
                }
                oracle.check(&f);
            }
            assert_eq!(f.server.received_requests().await.unwrap().len(), 2);
            for path in paths.iter().flatten().chain(std::iter::once(&f.workspace)) {
                assert_eq!(fs::read(path.join("left.txt")).unwrap(), b"base\n");
            }
            eprintln!("seeded child completed seed={seed:#x} backend={backend:?} operations=20 assignments=2 overallocated_refusals=1 wire_requests=2 cooperative_reopens=1 root_stop={root_stop} child_cancel={cancelled}");
            f.close().await;
        }
    }
    assert_eq!((recovered, capture_blocked, cancelled_blocked), (10, 4, 2));
    eprintln!("seeded child campaign seeds={SEEDS:x?} backends=2 operations=160 assignments=16 wire_requests=16 cooperative_reopens=8 held_child_attachments={recovered} capture_fenced_child_admissions={capture_blocked} cancelled_recovery_refusals={cancelled_blocked}");
}
