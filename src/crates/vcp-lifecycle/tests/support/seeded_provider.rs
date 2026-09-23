// SPDX-License-Identifier: Apache-2.0
//! P8-01/M9 synthetic loopback requests through the retained controller.
//! Sequential turns are not autonomous tool loops or packaged/live delegation.
use super::*;
use std::{collections::BTreeSet, sync::Mutex};
use vcp_lifecycle::foundation::CanonicalOwner;
use vcp_store::contract::State;
use wiremock::{matchers::any, Mock, ResponseTemplate};

const SEEDS: [u64; 4] = [1, 0x5eed, 0x8a01, 0xc0ffee];
const TURNS: usize = 16;
const FEE: u64 = 100;
const CAP: u64 = 10_000;
const STOP_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Plan {
    Paid,
    MissingUsage,
    Unauthorized,
    Retry429,
    Retry503,
    Exhausted,
    StopTimer,
}
const PLANS: [Plan; 6] = [
    Plan::Paid,
    Plan::MissingUsage,
    Plan::Unauthorized,
    Plan::Retry429,
    Plan::Retry503,
    Plan::Exhausted,
];

/// Invented schedule generator, independent of accounting/admission assertions.
fn plans(seed: u64) -> Vec<Plan> {
    let mut rows = PLANS.to_vec();
    let mut value = seed;
    while rows.len() < TURNS {
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        rows.push(PLANS[(value % 6) as usize]);
    }
    rows
}

/// Literal independent contract, not inferred from returned attempts or quotes.
fn expected(plan: Plan) -> (usize, u64, u64) {
    match plan {
        Plan::Paid => (1, 100, 0),
        Plan::MissingUsage | Plan::Unauthorized | Plan::StopTimer => (1, 0, 100),
        Plan::Retry429 => (2, 100, 100),
        Plan::Retry503 => (3, 100, 200),
        Plan::Exhausted => (3, 0, 300),
    }
}
fn priced_position(plan: Plan, position: usize) -> bool {
    matches!(
        (plan, position),
        (Plan::Paid, 0) | (Plan::Retry429, 1) | (Plan::Retry503, 2)
    )
}

fn response(plan: Plan, position: usize, turn: usize) -> ResponseTemplate {
    let failure = match (plan, position) {
        (Plan::Unauthorized, _) => Some(401),
        (Plan::Retry429, 0) | (Plan::StopTimer, _) => Some(429),
        (Plan::Retry503, 0 | 1) | (Plan::Exhausted, _) => Some(503),
        _ => None,
    };
    if let Some(status) = failure {
        return ResponseTemplate::new(status)
            .insert_header(
                "retry-after",
                if plan == Plan::StopTimer { "5" } else { "0" },
            )
            .set_body_json(
                serde_json::json!({"error":{"message":"public seeded synthetic provider failure"}}),
            );
    }
    let mut completed = serde_json::json!({"type":"response.completed","response":{
        "id":format!("seeded-{turn}-{position}"),"status":"completed","output":[]
    }});
    if plan != Plan::MissingUsage {
        completed["response"]["usage"] = serde_json::json!({"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001});
    }
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(sse(vec![
            ev_assistant_message("seeded-message", "Public synthetic response."),
            completed,
        ]))
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn attempts(state: &State) -> Result<Vec<Attempt>, String> {
    state
        .records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .map(|row| row.decode().map_err(|error| error.to_string()))
        .collect()
}

#[derive(Clone, Debug)]
struct Wire {
    turn: usize,
    position: usize,
    attempt: Attempt,
    body_digest: String,
}
#[derive(Default)]
struct Journal {
    active: Option<(usize, Plan)>,
    position: usize,
    arrivals: usize,
    resumes: usize,
    wires: Vec<Wire>,
    errors: Vec<String>,
}
impl Journal {
    fn error(&mut self, error: impl Into<String>) {
        if self.errors.len() < 48 {
            self.errors.push(error.into());
        }
    }
}

/// Run on the HTTP responder, before any reply bytes. Failures become ordinary
/// test-thread assertions, never panics hidden in a detached mock-server task.
fn observe(
    host: &CanonicalHost,
    request: &wiremock::Request,
    journal: &Journal,
    turn: usize,
    position: usize,
) -> Result<Wire, String> {
    require(
        request.method.as_str() == "POST" && request.url.path() == "/v1/responses",
        format!(
            "unexpected wire target: {} {}",
            request.method,
            request.url.path()
        ),
    )?;
    let state = host.snapshot()?;
    let all = attempts(&state)?;
    let submitted: Vec<_> = all
        .iter()
        .filter(|attempt| attempt.phase == ReservationState::Submitted)
        .collect();
    require(
        submitted.len() == 1,
        format!("wire arrival has {} submitted attempts", submitted.len()),
    )?;
    let attempt = submitted[0].clone();
    require(
        !journal.wires.iter().any(|prior| {
            prior.attempt.id == attempt.id || prior.attempt.reservation == attempt.reservation
        }),
        "wire reused attempt or reservation identity",
    )?;
    let previous = journal.wires.iter().rev().find(|wire| wire.turn == turn);
    require(
        attempt.previous.as_ref() == previous.map(|wire| &wire.attempt.id),
        "retry predecessor differs from actual wire order",
    )?;
    for prior in journal.wires.iter().filter(|wire| wire.turn == turn) {
        require(
            all.iter().any(|row| {
                row.id == prior.attempt.id && row.phase == ReservationState::ReconciliationPending
            }),
            "retry replaced or settled prior uncertain send",
        )?;
    }
    let intent = attempt
        .send_intent
        .as_ref()
        .ok_or("wire lacks committed send intent")?;
    require(
        state.events.iter().any(|event| {
            &event.event.id == intent
                && event.event.kind == vcp_protocol::event::EventKind::AttemptSubmitted
        }),
        "send intent event is not durable at arrival",
    )?;
    let body_digest = vcp_protocol::digest_bytes(&request.body);
    require(
        attempt.request_digest == body_digest,
        "wire digest differs from admitted attempt",
    )?;
    require(
        host.read_artifact(attempt.request.clone())? == request.body,
        "captured request bytes differ from actual HTTP bytes",
    )?;
    require(
        attempt.quote.amount.micros.get() == FEE,
        "fixture admission deviated from independently fixed 100-micro fee",
    )?;
    Ok(Wire {
        turn,
        position,
        attempt,
        body_digest,
    })
}

struct Fixture {
    config: Config,
    host: Option<CanonicalHost>,
    owner: Option<CanonicalOwner>,
    binding: ThreadBinding,
    test: Option<TestCodex>,
    snapshot: vcp_models::catalog::Snapshot,
    server: wiremock::MockServer,
    journal: Arc<Mutex<Journal>>,
    observer: Arc<Mutex<Option<CanonicalHost>>>,
}

async fn attach(
    host: &CanonicalHost,
    config: &Config,
    binding: &ThreadBinding,
    server: &wiremock::MockServer,
) -> TestCodex {
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = std::path::PathBuf::from(&config.binding.root);
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "public-seeded-loopback-key",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |c| {
            c.cwd = cwd.try_into().unwrap();
            // This fixture checks literal loopback, disables both retained retry
            // layers, telemetry, provider environment credentials and MCP.
            configure_provider_fixture(c);
            starter
                .lifecycle()
                .authorize_startup(c.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(server)
        .await
        .unwrap();
    let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(id, binding.clone()).unwrap();
    test
}

impl Fixture {
    async fn new(temp: &tempfile::TempDir, backend: BackendKind) -> Self {
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let mut config = config(&temp.path().join("canonical"), &workspace, backend);
        config.cap.micros = Micros::new(CAP);
        config.max_transport_retries = 2;
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        host.command(
            Command::SetPolicy {
                policy: vcp_domain::policy::Policy {
                    workspace: config.workspace.clone(),
                    revision: PolicyRevision::ZERO,
                    mode: vcp_domain::policy::Autonomy::Plan,
                    denials: vec![],
                    workspace_roots: BTreeSet::from([
                        RootId::parse(config.workspace.as_str()).unwrap()
                    ]),
                    automatic_effects: BTreeSet::new(),
                    timeout_ceiling_ms: Units::new(30_000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider_with_timeout(snapshot.clone(), raw, Duration::from_secs(20))
            .unwrap();
        let server = start_mock_server().await;
        let journal = Arc::new(Mutex::new(Journal::default()));
        let observer = Arc::new(Mutex::new(Some(host.clone())));
        let seen = journal.clone();
        let inspect = observer.clone();
        Mock::given(any())
            .respond_with(move |request: &wiremock::Request| {
                let mut journal = seen
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                journal.arrivals += 1;
                let Some((turn, plan)) = journal.active else {
                    journal.error("wire arrived outside an explicit logical turn");
                    return ResponseTemplate::new(500);
                };
                let position = journal.position;
                journal.position += 1;
                // Bound retained observations even if a product regression loops.
                if journal.arrivals > 48 || position >= 3 {
                    journal.error("physical wire bound exceeded");
                    return ResponseTemplate::new(500);
                }
                let observed = inspect
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let observation = observed
                    .as_ref()
                    .ok_or_else(|| "wire arrived after owning host release".to_owned())
                    .and_then(|host| observe(host, request, &journal, turn, position));
                match observation {
                    Ok(wire) => journal.wires.push(wire),
                    Err(error) => journal.error(format!("turn={turn} wire={position}: {error}")),
                }
                response(plan, position, turn)
            })
            .mount(&server)
            .await;
        let test = attach(&host, &config, &binding, &server).await;
        Self {
            config,
            host: Some(host),
            owner: Some(owner),
            binding,
            test: Some(test),
            snapshot,
            server,
            journal,
            observer,
        }
    }
    fn host(&self) -> &CanonicalHost {
        self.host.as_ref().unwrap()
    }
    fn test(&self) -> &TestCodex {
        self.test.as_ref().unwrap()
    }
    fn prepare(&self, turn: usize, plan: Plan) -> Result<(), String> {
        let id = self.test().codex.session_configured().thread_id;
        let state = self.host().snapshot()?;
        let root: Task = state
            .record(
                Collection::Task,
                self.binding.scope.task.as_str(),
                &self.binding.scope.workspace,
            )
            .map_err(|error| error.to_string())?
            .decode()
            .map_err(|error| error.to_string())?;
        if root.state == TaskState::Paused {
            // Unknown terminal responses deliberately pause the root. Each
            // generated logical turn is an explicit owner continuation; use
            // current checked resume without clearing prior unknown liability.
            self.host().resume(id, root.revision, root.fingerprint)?;
            self.journal.lock().unwrap().resumes += 1;
        } else {
            require(
                root.state == TaskState::Running,
                format!("unexpected pre-turn root state {:?}", root.state),
            )?;
        }
        let sealed = sealed_provider_context(self.host(), id, &self.snapshot, None);
        self.host()
            .prepare_context(id, sealed, serde_json::json!([]), vec![])?;
        let mut journal = self.journal.lock().unwrap();
        journal.active = Some((turn, plan));
        journal.position = 0;
        Ok(())
    }
    async fn start(&self) -> Result<(), String> {
        self.test()
            .codex
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Run this public synthetic sealed request.".into(),
                text_elements: vec![],
            }]))
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    async fn run_turn(&self, index: usize, plan: Plan) -> Result<(), String> {
        self.prepare(index, plan)?;
        self.start().await?;
        tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_event(&self.test().codex, |event| {
                matches!(event, EventMsg::TurnComplete(_))
            }),
        )
        .await
        .map_err(|_| "logical turn exceeded 30 seconds")?;
        let state = self.host().snapshot()?;
        let root: Task = state
            .record(
                Collection::Task,
                self.binding.scope.task.as_str(),
                &self.binding.scope.workspace,
            )
            .map_err(|error| error.to_string())?
            .decode()
            .map_err(|error| error.to_string())?;
        let expected_state = if matches!(
            plan,
            Plan::MissingUsage | Plan::Unauthorized | Plan::Exhausted
        ) {
            TaskState::Paused
        } else {
            TaskState::Running
        };
        require(
            root.state == expected_state,
            format!(
                "plan {plan:?} terminal root state {:?}, expected {expected_state:?}",
                root.state
            ),
        )?;
        self.journal.lock().unwrap().active = None;
        Ok(())
    }

    /// Independent wire ordering and literal expected fee arithmetic decide the
    /// oracle. Product ledger helpers are only used to read retained values.
    async fn verify(&self, executed: &[Plan]) -> Result<(), String> {
        let (wires, arrivals, errors) = {
            let journal = self.journal.lock().unwrap();
            (
                journal.wires.clone(),
                journal.arrivals,
                journal.errors.clone(),
            )
        };
        require(
            errors.is_empty(),
            format!("responder violations: {errors:?}"),
        )?;
        let counts: Vec<_> = executed.iter().copied().map(expected).collect();
        let expected_resumes = executed
            .iter()
            .take(executed.len().saturating_sub(1))
            .filter(|plan| {
                matches!(
                    plan,
                    Plan::MissingUsage | Plan::Unauthorized | Plan::Exhausted
                )
            })
            .count();
        require(
            self.journal.lock().unwrap().resumes == expected_resumes,
            "deliberate owner resume count differs from independent terminal-disposition table",
        )?;
        let expected_wires: usize = counts.iter().map(|row| row.0).sum();
        require(
            arrivals == expected_wires && wires.len() == expected_wires,
            format!(
                "wire arrivals={arrivals}, joined={}, expected={expected_wires}",
                wires.len()
            ),
        )?;
        let requests = self
            .server
            .received_requests()
            .await
            .ok_or("wire server recording unavailable")?;
        require(
            requests.len() == expected_wires,
            "server request set differs from callback observations",
        )?;
        for (request, wire) in requests.iter().zip(&wires) {
            require(
                vcp_protocol::digest_bytes(&request.body) == wire.body_digest,
                "independent server capture order/body mismatch",
            )?;
        }
        let state = self.host().snapshot()?;
        let rows = attempts(&state)?;
        let reservations: Vec<Reservation> = state
            .records
            .values()
            .filter(|row| row.collection == Collection::Reservation)
            .map(|row| row.decode().map_err(|error| error.to_string()))
            .collect::<Result<_, _>>()?;
        let expected_ids: BTreeSet<_> = wires.iter().map(|wire| &wire.attempt.id).collect();
        let actual_ids: BTreeSet<_> = rows.iter().map(|attempt| &attempt.id).collect();
        require(
            expected_ids.len() == expected_wires
                && expected_ids == actual_ids
                && reservations.len() == expected_wires,
            "attempt/reservation full set differs from actual wire identities",
        )?;
        for (turn, plan) in executed.iter().copied().enumerate() {
            let turn_wires: Vec<_> = wires.iter().filter(|wire| wire.turn == turn).collect();
            require(
                turn_wires.len() == expected(plan).0,
                format!("logical turn {turn} has wrong retry count"),
            )?;
            for (position, wire) in turn_wires.iter().enumerate() {
                require(wire.position == position, "wire ordinal is not contiguous")?;
                let current = rows
                    .iter()
                    .find(|row| row.id == wire.attempt.id)
                    .ok_or("wire attempt disappeared")?;
                let reservation = reservations
                    .iter()
                    .find(|row| row.id == current.reservation)
                    .ok_or("wire reservation disappeared")?;
                let paid = priced_position(plan, position);
                require(
                    current.scope == self.binding.scope
                        && wire.attempt.scope == self.binding.scope
                        && reservation.scope == self.binding.scope
                        && current.root == self.binding.scope.task
                        && wire.attempt.root == self.binding.scope.task
                        && reservation.root == self.binding.scope.task
                        && current.role == RequestRole::Main
                        && wire.attempt.role == RequestRole::Main
                        && reservation.role == RequestRole::Main,
                    "wire accounting escaped root scope or main request role",
                )?;
                let phase = if paid {
                    ReservationState::Settled
                } else {
                    ReservationState::ReconciliationPending
                };
                require(
                    current.phase == phase && reservation.phase == phase,
                    format!("turn {turn} attempt {position} lost final/unknown distinction"),
                )?;
                require(
                    current.charged.get() == if paid { FEE } else { 0 },
                    "attempt charge differs from independent response plan",
                )?;
                require(
                    reservation.charged.get() == if paid { FEE } else { 0 },
                    "reservation charge differs from independent response plan",
                )?;
                require(
                    reservation.liability.get() == if paid { 0 } else { FEE },
                    "unknown liability differs from fixed fee",
                )?;
                require(
                    reservation.amount.micros.get() == FEE && reservation.attempt == current.id,
                    "reservation fee/attempt binding mismatch",
                )?;
                require(
                    current.request_digest == wire.body_digest
                        && current.previous == wire.attempt.previous
                        && current.send_intent == wire.attempt.send_intent,
                    "retained request lineage changed after send",
                )?;
            }
        }
        let ledger =
            vcp_budget::ledger(&state, &self.binding.scope).map_err(|error| error.to_string())?;
        let settled: u64 = counts.iter().map(|row| row.1).sum();
        let unresolved: u64 = counts.iter().map(|row| row.2).sum();
        require(
            ledger.settled.get() == settled
                && ledger.unresolved.get() == unresolved
                && ledger.active.get() == 0
                && ledger.cap.get() == CAP
                && !ledger.overrun,
            format!(
                "ledger differs: settled={}/{} unresolved={}/{} active={} cap={}",
                ledger.settled.get(),
                settled,
                ledger.unresolved.get(),
                unresolved,
                ledger.active.get(),
                ledger.cap.get()
            ),
        )?;
        require(
            !state
                .records
                .values()
                .any(|row| row.collection == Collection::Effect),
            "no-tool provider trace created an effect",
        )
    }
    async fn close_runtime(&mut self) -> Result<(), String> {
        self.owner
            .take()
            .ok_or("owner already closed")?
            .close()
            .await?;
        let test = self.test.take().ok_or("runtime already closed")?;
        test.codex
            .shutdown_and_wait()
            .await
            .map_err(|error| error.to_string())?;
        drop(test);
        self.observer.lock().unwrap().take();
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_provider_bursts_match_independent_wire_and_liability_oracle() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for seed in SEEDS {
            let schedule = plans(seed);
            assert_eq!(
                schedule.iter().copied().collect::<BTreeSet<_>>(),
                PLANS.into_iter().collect()
            );
            eprintln!("seeded-provider begin backend={backend:?} seed={seed:#x}");
            let temp = tempfile::tempdir().unwrap();
            let mut fixture = Fixture::new(&temp, backend).await;
            for (index, plan) in schedule.iter().copied().enumerate() {
                let result = async {
                    fixture.run_turn(index, plan).await?;
                    fixture.verify(&schedule[..=index]).await
                }
                .await;
                if let Err(error) = result {
                    panic!("backend={backend:?} seed={seed:#x} turn={index} first failing executed prefix={:?}: {error}", &schedule[..=index]);
                }
            }
            fixture.close_runtime().await.unwrap();
            fixture.verify(&schedule).await.unwrap_or_else(|error| {
                panic!("backend={backend:?} seed={seed:#x} after shutdown: {error}")
            });
            let wires = fixture.journal.lock().unwrap().arrivals;
            eprintln!("seeded-provider passed backend={backend:?} seed={seed:#x} turns={TURNS} wires={wires}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_provider_retry_pause_reopen_never_replays_unknown_send() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for seed in SEEDS {
            let generated = plans(seed);
            let mut schedule = vec![Plan::Paid, Plan::MissingUsage, generated[6], generated[7]];
            eprintln!(
                "seeded-provider-stop begin backend={backend:?} seed={seed:#x} prefix={schedule:?}"
            );
            let temp = tempfile::tempdir().unwrap();
            let mut fixture = Fixture::new(&temp, backend).await;
            let result: Result<(), String> = async {
                for (index, plan) in schedule.iter().copied().enumerate() {
                    fixture.run_turn(index, plan).await?;
                    fixture.verify(&schedule[..=index]).await?;
                }
                fixture.prepare(schedule.len(), Plan::StopTimer)?;
                fixture.start().await?;
                let predecessor = tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        let wire = fixture
                            .journal
                            .lock()
                            .unwrap()
                            .wires
                            .iter()
                            .find(|wire| wire.turn == schedule.len())
                            .cloned();
                        if let Some(wire) = wire {
                            let state = fixture.host().snapshot()?;
                            for row in state
                                .records
                                .values()
                                .filter(|row| row.collection == Collection::Artifact)
                            {
                                let artifact: ArtifactDescriptor =
                                    row.decode().map_err(|error| error.to_string())?;
                                if artifact.spec.schema == "provider-retry/1" {
                                    let bytes = fixture.host().read_artifact(artifact.spec.id)?;
                                    let retry: serde_json::Value =
                                        serde_json::from_slice(&bytes)
                                            .map_err(|error| error.to_string())?;
                                    if retry["predecessor"] == serde_json::json!(wire.attempt.id) {
                                        return Ok::<_, String>(wire.attempt.id);
                                    }
                                }
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .map_err(|_| "matching wire/retry barrier did not arrive")??;
                let expected_before_stop: usize = schedule
                    .iter()
                    .copied()
                    .map(expected)
                    .map(|row| row.0)
                    .sum::<usize>()
                    + 1;
                require(
                    fixture.journal.lock().unwrap().arrivals == expected_before_stop,
                    "retry delay elapsed before stop barrier; harness schedule invalid",
                )?;
                let state = fixture.host().snapshot()?;
                let task: Task = state
                    .record(
                        Collection::Task,
                        fixture.config.root_task.as_str(),
                        &fixture.config.workspace,
                    )
                    .and_then(|row| row.decode())
                    .map_err(|error| error.to_string())?;
                let command = fixture.host().control_envelope(
                    CommandId::new(),
                    task.scope.task.clone(),
                    task.revision,
                    Command::Transition {
                        next: TaskState::Paused,
                        reason: "seeded explicit retry pause".into(),
                        verification: None,
                    },
                )?;
                let receipt = fixture.host().stop(command.clone())?;
                let after_ack = fixture.journal.lock().unwrap().arrivals;
                require(
                    after_ack == expected_before_stop,
                    "retry reached wire before stop acknowledgement; harness schedule invalid",
                )?;
                let id = fixture.test().codex.session_configured().thread_id;
                tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        if fixture
                            .host()
                            .lifecycle()
                            .inspect(id)
                            .map_err(|error| format!("{error:?}"))?
                            .interrupt_complete
                        {
                            return Ok::<_, String>(());
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .map_err(|_| "pause failed to quiesce retained controller")??;
                let before_replay = fixture.host().snapshot()?;
                require(
                    fixture.host().stop(command.clone())? == receipt,
                    "pause retry changed receipt",
                )?;
                require(
                    fixture.host().snapshot()? == before_replay,
                    "pause replay changed quiescent canonical state",
                )?;
                schedule.push(Plan::StopTimer);
                fixture.verify(&schedule).await?;
                fixture.close_runtime().await?;
                // Cross the original timer window even though shutdown already
                // drained the owning runtime; a hidden retry cannot be overlooked.
                tokio::time::sleep(STOP_DELAY + Duration::from_millis(100)).await;
                require(
                    fixture.journal.lock().unwrap().arrivals == after_ack,
                    "physical send occurred after acknowledged stop",
                )?;
                let closed = fixture.host().snapshot()?;
                drop(fixture.host.take());
                let (host, owner) = CanonicalHost::open(fixture.config.clone())?;
                fixture.host = Some(host);
                fixture.owner = Some(owner);
                let reopened = fixture.host().snapshot()?;
                require(
                    reopened.events.starts_with(&closed.events),
                    "reopen lost acknowledged provider history",
                )?;
                require(
                    closed
                        .commands
                        .iter()
                        .all(|(key, value)| reopened.commands.get(key) == Some(value)),
                    "reopen changed command receipt",
                )?;
                require(
                    fixture.host().stop(command)? == receipt,
                    "reopened pause replay changed receipt",
                )?;
                fixture.verify(&schedule).await?;
                require(
                    attempts(&reopened)?.iter().any(|attempt| {
                        attempt.id == predecessor
                            && attempt.phase == ReservationState::ReconciliationPending
                    }),
                    "reopen lost stopped send liability",
                )?;
                let task: Task = reopened
                    .record(
                        Collection::Task,
                        fixture.config.root_task.as_str(),
                        &fixture.config.workspace,
                    )
                    .and_then(|row| row.decode())
                    .map_err(|error| error.to_string())?;
                require(
                    task.state == TaskState::Paused,
                    "reopen silently resumed root",
                )?;
                // Aborted provider captures are intentionally not among the
                // terminal tool-capture exceptions in worker startup recovery.
                let responses: Vec<ArtifactDescriptor> = reopened
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode().map_err(|error| error.to_string()))
                    .collect::<Result<_, _>>()?;
                require(
                    responses.iter().any(|row| {
                        row.spec.channel == Channel::Response && row.state == CaptureState::Aborted
                    }),
                    "reopened fence lacks retained aborted provider capture",
                )?;
                let startup = codex_extension_api::HostWorkAdmission::admit_startup(
                    fixture.host(),
                    std::path::Path::new(&fixture.config.binding.root),
                    None,
                );
                require(
                    matches!(startup, Err(ref error) if error == "canonical host fenced"),
                    "unreconciled provider capture failed to fence reopened startup",
                )?;
                require(
                    fixture.host().snapshot()? == reopened,
                    "denied startup mutated reopened state",
                )?;
                fixture
                    .owner
                    .take()
                    .ok_or("reopened owner missing")?
                    .close()
                    .await?;
                fixture.verify(&schedule).await
            }
            .await;
            if let Err(error) = result {
                panic!("backend={backend:?} seed={seed:#x} executed prefix={schedule:?} then retry/pause/reopen: {error}");
            }
            eprintln!("seeded-provider-stop passed backend={backend:?} seed={seed:#x} prefix_turns=4 wires={}", fixture.journal.lock().unwrap().arrivals);
        }
    }
}
