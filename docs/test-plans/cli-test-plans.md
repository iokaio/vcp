# VCP CLI practical scenario test plans

Status: **scenario execution not yet qualified.** Static review and offline harness
checks do not establish that the paid scenarios pass. Record the CLI version, resolved
dependencies, parameter values, scorecard and command logs for each actual run; keep dry
runs, calibration runs and measured runs distinct.

This plan defines four long-running, realistic scenarios. Each one drives the `vcp` CLI
through several turns of real development work on a fresh project. A PowerShell harness
then assesses every turn deterministically from logs, VCP's canonical evidence and
independent builds and tests. Each scenario leaves a project folder with its source,
per-turn git history, compiled assets and a scorecard.

| Scenario | Project | Stack | Script |
|---|---|---|---|
| A | TaskBoard: team task board | TypeScript, Node.js (Express 5), Vue 3 + Vite | [scenario-a-vue-taskboard.ps1](scenario-a-vue-taskboard.ps1) |
| B | Contoso Inventory | ASP.NET Core Razor Pages + REST API in C#, EF Core, SQL Server LocalDB | [scenario-b-aspnet-inventory.ps1](scenario-b-aspnet-inventory.ps1) |
| C | ledger-cli: personal finance CLI | Java 21, Maven, picocli, Jackson | [scenario-c-java-ledger-cli.ps1](scenario-c-java-ledger-cli.ps1) |
| D | textlab: support-ticket text analysis | Python 3.11+, scikit-learn, pytest | [scenario-d-python-textlab.ps1](scenario-d-python-textlab.ps1) |

All scripts share [VcpScenarioHarness.psm1](VcpScenarioHarness.psm1). Each script runs in its
own console and keeps its own run root, VCP data directory, profiles, ports and logs. Any
two scenarios, or all four, can run at the same time.

---

## 1. What the scenarios assess

Each scenario measures how VCP performs as a coding agent over a realistic multi-turn
engagement:

1. **Functional delivery per turn.** Did the requested feature work, as judged by
   harness-owned acceptance checks rather than the agent's own claims?
2. **First-pass quality versus repair.** When a turn fails, the harness feeds the exact
   failures back as up to `-MaxRepairTurns` repair turns (default one). Both the first-pass rate and the final state are
   reported.
3. **Regression resistance.** The stage gate lists specify which earlier contracts are
   replayed; the final verification repeats the implemented functional gate suites.
4. **Following constraints.** Protected test and data files must stay byte-identical. Plan
   autonomy must not change the workspace.
5. **CLI lifecycle behavior.** This covers structured JSONL output, exit codes, guardrail
   rejection before execution, and deadline pause followed by `resume` / `resume --last` /
   `sessions resume` / `sessions fork`. It also covers inspection views, history and cost
   accounting.
6. **Operational cost and time.** Settled spend comes from VCP's canonical ledger. Wall time
   and request/tool counts are recorded per stage.

### 1.1 How VCP turns work in these plans

VCP has no "send a follow-up message to a finished task" command. Each turn is therefore a
new root task started with `vcp run --file <prompt>` in the same workspace and the same data
directory. Continuity comes from the repository state, the durable workspace history, and
task prompts that build on the earlier contracts, as with a real developer issuing successive
tickets. Continuation of a single task is exercised separately:

- a turn runs under a short-deadline profile so it pauses durably (exit 8);
- the harness then continues it with `resume --last` (A), `resume <task>` (B),
  `sessions resume <session>` (C), or `resume <task> --expected-revision <rev>` taken from
  `workspace discover` (D);
- D also forks a completed review with `sessions fork <session> --through-turn <turn>`.

---

## 2. Prerequisites

### 2.1 Machine and tools

| Requirement | A | B | C | D |
|---|---|---|---|---|
| Windows 11 x64, PowerShell 7.4+ (`pwsh`) | yes | yes | yes | yes |
| VCP installed: `%LOCALAPPDATA%\Programs\VCP\vcp.exe`, or pass `-Vcp` / set `VCP_EXE` | yes | yes | yes | yes |
| Node.js 22.18+ (24 LTS recommended) with bundled npm | yes | | | |
| .NET SDK 8+ (the selected SDK determines the target major) and SQL Server LocalDB (or a credential-free `-SqlConnectionString`) | | yes | | |
| `sqlcmd` (optional, enables one advisory database gate) | | opt | | |
| JDK 21+ (`JAVA_HOME` or PATH) and Apache Maven 3.9+ (`mvn.cmd` on PATH) | | | yes | |
| Python 3.11+ (py launcher or python.exe, not the Store alias) | | | | yes |
| git (optional; enables one checkpoint commit per stage) | opt | opt | opt | opt |
| Internet access to npm, NuGet, Maven Central and PyPI (seeding, package restores, `npm ci`) | yes | yes | yes | yes |

### 2.2 Automatic provider preparation

Start `run-cli-scenarios.ps1`; no manual VCP commands or project scaffolding are needed.
The launcher reuses a valid provider generation when available. Otherwise it runs
`vcp setup provider` itself, using the installed account's primary model and matching
endpoint from completed account setup where available. Missing model/endpoint choices
are requested inside the launcher. The scenario then creates its project, restores
dependencies, creates execution profiles and runs the checks.

Qualification makes up to two paid requests. The launcher asks for a **separate setup
budget** (Enter authorizes 3 USD), or accepts `-SetupBudgetUsd` up to 25 USD. This is
additional to `-MaxScenarioUsd`, including when preparing a new generation for DryRun.
An existing valid generation requires no qualification spend. The request-price ceiling
defaults to 0.001 USD and can be set with `-RequestPriceLimitUsd`.

The resulting directory contains `qualified\snapshot.json` and `endpoints.json`.
**Snapshots are valid for 12 hours**; scenarios require five hours remaining. Missing or
expired supplied generations are preserved and replaced with a new generation under
the invocation's `setup` directory. If only generation receipts are delayed, the launcher
tries `setup provider-complete` once without repeating inference. Other failures stop
before the scenario and retain their logs. See [beta onboarding](../usage/beta-onboarding.md)
for the underlying CLI contracts.

For advanced use, the equivalent manual command remains:

```powershell
& $vcp --workspace C:\vcp-scenarios setup provider --model <org/model> --endpoint <tag> `
  --request-price-limit <usd> --budget-usd <probe-cap> --output C:\vcp-private\provider-20261002
```

Choose an endpoint that supports tools and has an output limit large enough for code
generation. `-OutputTokens` defaults to 8192 and is clamped to the endpoint's `max_output`.

### 2.3 Credential (per console)

The launcher checks the installed credential selection and prompts with masked input
only when its environment credential is absent. It restores the environment afterward.
VCP's automated JSONL commands cannot use a stored Windows key; the launcher respects
that boundary. Credentials are excluded from project build/test/server processes.

When invoking an individual scenario directly with the default credential selection:

```powershell
$secret = Read-Host 'OpenRouter API key' -AsSecureString
$env:OPENROUTER_API_KEY = [pscredential]::new('k', $secret).GetNetworkCredential().Password
$secret = $null
```

The harness never prints, logs or writes the key. Remove it afterwards with
`Remove-Item Env:\OPENROUTER_API_KEY`.

### 2.4 Run root

The default run root is `%SystemDrive%\vcp-scenarios`. Change it with `-RunRoot`. The harness
refuses any run root inside a git repository, a OneDrive root or a network path, because VCP
rejects data directories and profiles in those locations.

Scenario B's optional `-SqlConnectionString` must use integrated authentication and no
embedded password. The script creates a distinct database name for the run; connection
settings are written into the generated project, exposed to VCP, checkpointed and included
in published assets. Password-bearing connection strings are rejected before seeding.

---

## 3. Harness design (shared by all scenarios)

### 3.1 Layout of one scenario run

```
<RunRoot>\<scenario>\<yyyyMMdd-HHmmss-xxxxxx>\
  workspace\     the produced project; git history has one commit per stage; artifacts\ holds compiled assets
  vcp-data\      VCP --data-dir for this run only (canonical store, task model selections)
  profiles\      base profile from 'vcp setup profile' plus the composed scenario profiles
  hidden\        fixtures deliberately kept outside the workspace (D: holdout, edge cases)
  env\           harness-managed environments (D: Python venv)
  tmp\           TEMP/TMP for VCP-launched processes and scratch data for gates
  logs\
    console-transcript.log   full console transcript
    progress.log             concise timeline of stages, events and gate outcomes
    vcp-commands.log         replayable PowerShell command lines, start/end status, task/scope and result paths
    vcp-commands.jsonl       exact executable/argv and completion records for automated inspection
    tool-commands.log        every toolchain command (npm, dotnet, mvn, java, python, git)
    <stage>\prompt.md                       the exact task text given to VCP
    <stage>\vcp\NN-<label>.stdout.jsonl     raw JSONL from each vcp command
    <stage>\vcp\NN-<label>.stderr.txt
    <stage>\tasks-status.json, tasks-agents.json, history.json
    <stage>\inspect-{costs,verification,tools,routing,policy,outputs}.json
    <stage>\final-message.md                final assistant text (best effort, see 3.5)
    <stage>\workspace-diff.json             files added/modified/removed by the stage
    <stage>\tools\NN-<label>.{out,err}.log   build/test/CLI output used by the gates
    <stage>\servers\*.log                   logs of the app servers started by gates
  results\
    scorecard.json           machine-readable result (schema vcp-practical-scenario/1)
    summary.md               human-readable result
```

### 3.2 Stage sequence (identical shape in every scenario)

| Stage | Spend | Purpose |
|---|---|---|
| P0-preflight | none | `--version`, `doctor`, `setup credential status`, `setup profile` (real CLI path for this workspace), `skills list`, `models` |
| B0-baseline | none (VCP) | Seed a realistic starting scaffold, restore dependencies and prove it builds and tests before any spend. A failure here stops the run. |
| P1-profiles | none | Compose the scenario profiles (3.3) and validate each with `vcp setup check` |
| G0-guardrail | none | A `vcp run` that must be rejected before execution with exit 2 and no accepted task |
| T1-T5 | paid | Feature turns, each followed by the evidence sweep (3.4), deterministic gates (3.6) and at most `-MaxRepairTurns` repair turns |
| T5 continuation | paid | T5 runs under a short-deadline profile (`-ShortDeadlineSeconds`, default 150 s). On exit 8 the scenario-specific continuation command resumes it with the full profile. |
| T6-review | paid, small (cap 2.00 USD) | A review in `--autonomy plan` with a read-only profile. The workspace must stay byte-identical, and the answer ends with a JSON findings block. |
| T7-fork (D only) | paid | `sessions fork` of the review session, which tests consistency and immutability |
| FINAL | none (VCP) | Independent build and functional regression gates for the final project, packaging of compiled assets; lifecycle actions are not repeated |
| P9-evidence | none | Read-only CLI sweep: sessions, discover, history, retention, optimize, memory |

`-SkipPaidStages` performs a **zero-spend dry run**: P0, B0, P1 and G0 only. It needs no
credential. Run it first on every machine to validate toolchains, seeding, profiles and the
guardrail. It still requires a valid qualified generation and may download dependencies;
it does not run provider qualification or prove that a feature turn succeeds.

### 3.3 Execution profiles

Each scenario composes its own profiles with `New-ScenarioProfile`. The qualified provider
snapshot is spliced in **verbatim** from `qualified\snapshot.json`, so no prices, timestamps
or hashes are re-encoded. Everything else is explicit owner configuration:

```json
{
  "version": 1,
  "workspace": "<run>\\workspace",
  "trust_workspace": true,
  "sync_roots": [],
  "maximum_autonomy": "autonomous",
  "automatic_effects": ["read", "write", "execute", "network", "install", "opaque"],
  "budget_usd": "3.00",
  "provider": { "...": "verbatim qualified snapshot" },
  "catalog": "<generation>\\endpoints.json",
  "affected_paths": ["README.md", "src", "tests", "..."],
  "canonical_tools": ["vcp_read", "vcp_list", "vcp_search", "vcp_patch", "vcp_exec", "vcp_verify"],
  "max_requests": 96,
  "output_tokens": "8192",
  "provider_timeout_seconds": 300,
  "max_transport_retries": 2,
  "deadline_seconds": 1800,
  "processes": [{
    "name": "node", "executable": "C:\\Program Files\\nodejs\\node.exe",
    "environment": { "SYSTEMROOT": "...", "WINDIR": "...", "PATH": "<tool dir>;System32",
                     "PATHEXT": ".COM;.EXE;.BAT;.CMD", "TEMP": "<run>\\tmp", "TMP": "<run>\\tmp", "CI": "true" },
    "required_isolation": [], "reduced_isolation": true, "inputs": [], "max_timeout_ms": 900000
  }],
  "checks": []
}
```

Design notes, each based on the current source:

- **Unattended execution.** Any effect outside the automatic set would stop a non-interactive
  run with exit 4. Feature turns therefore use `--autonomy autonomous` under
  `maximum_autonomy: autonomous`, with `automatic_effects` covering execute, network and
  install for builds and package restores. `publish` is deliberately excluded. Policy
  evaluation also requires every resource to be inside the workspace roots.
- **Process profiles** are `.exe` only, in `Direct` mode with no shell, and use a filtered
  public environment. `npm`, `mvn` and the `dotnet-ef` tool are reached through `node.exe`,
  `java.exe` and `dotnet.exe`; each task prompt gives the exact argument forms. The
  environment allowlist (`SYSTEMROOT`, `PATH`, `TEMP`, ...) has no `USERPROFILE`, `APPDATA`
  or `JAVA_HOME`. Tools are expected to resolve user folders through Windows APIs, and the
  first run will confirm this (section 8).
- **`reduced_isolation: true` with no required isolation** mirrors the repository's own
  execution fixtures. Toolchains need to read SDK, cache and package locations outside the
  workspace. This is an explicit owner choice for a test machine; review it before reusing
  these profiles anywhere else.
- **Checks.** The VCP verification runner supports `node` (`node --test <files>`) and `cargo`
  only. Scenario A adds a per-turn `node` check with the cumulative expected test names.
  Scenarios B, C and D use `checks: []` and rely on the harness gates.
- **The read-only review profile** has `maximum_autonomy: plan`, `automatic_effects: ["read"]`
  and no processes.

### 3.4 Commands run by every paid stage

`$G` below means `--format jsonl --non-interactive --workspace <ws> --data-dir <data>`.
Every stage that starts or continues a task (run, repair, resume, fork) is followed by this
**evidence sweep**. The sweep makes no inference calls:

```text
vcp $G tasks status <task>
vcp $G tasks agents <task>
vcp $G inspect <task> --view costs        --limit 128 [--cursor '<next_cursor JSON>']   (all pages, max 64)
vcp $G inspect <task> --view verification --limit 128 [--cursor ...]
vcp $G inspect <task> --view tools        --limit 128 [--cursor ...]
vcp $G inspect <task> --view routing      --limit 128 [--cursor ...]
vcp $G inspect <task> --view policy       --limit 128 [--cursor ...]
vcp $G inspect <task> --view outputs      --limit 128 [--cursor ...]
vcp $G inspect <last-response-artifact> --view outputs --offset <n> --length 65536   (repeated to the artifact length)
vcp $G history list --task <task> --limit 128
```

The **repair turn** for any stage `<S>` is
`vcp $G --config <profile of S> run --file logs\<S>-repair1\prompt.md --budget-usd <cap> --autonomy autonomous`.
Its prompt lists the failed gate IDs with their exact failure details, followed by the evidence sweep.

### 3.5 What is read from VCP output

- **JSONL frames.** Stdout carries `accepted` (with `scope.{workspace,session,task}`),
  `event`, `required_input`, `cursor_gap`, `retention_notice` and a final `result` (with
  `conditions` and `exit_code`). The harness records the first `accepted` frame, the last
  `result` frame, the count of each event kind and any invalid lines.
- **Exit codes:** 0 completed, 1 internal or output failure, 2 invalid configuration, 3
  incomplete, 4 required input, 5 budget exhausted, 6 cancelled, 7 unresolved effect, 8
  durably paused. Feature turns accept only 0. Repair and resume turns accept 0 or 3,
  short-deadline turns accept 0, 3 or 8, and plan reviews/forks accept 0, 3 or 4. These
  allowances never substitute for the stage's functional gates. JSONL acceptance and final
  result framing are required independently of the process exit code.
- **Cost** comes from `settled` micros in the canonical task `ledger` records in
  `inspect --view costs`, counting a resumed task's cumulative total only once. If cost evidence is incomplete, the budget guard assumes the full
  per-turn cap was spent and the scorecard sets `spend_evidence_complete: false`.
- **Final message.** The final assistant text is not part of the JSONL stream. The harness
  uses chronological artifact events to select a captured response, reads it through
  `inspect --view outputs` byte ranges and extracts `output_text` from its
  `response.completed` SSE event. Inspection page order is canonical key order, not
  response chronology. Missing ordering or capture evidence leaves the message unavailable;
  this is best effort and is used only by advisory gates.
- **Completed turn IDs** for `sessions fork` come from `event.event.data.facts[]` entries with
  `collection == "turn"` and `value.state == "completed"`.

### 3.6 Gates and scoring

A gate is a deterministic check with a stable ID, a stage, and an outcome of
`pass`/`fail`/`skip`. **Required** gates decide the verdict. **Advisory** gates are reported
only. They are used where the expected behavior is informative but not essential, or where
evidence capture is best effort.

| Gate family | Technique |
|---|---|
| Build/compile | Exit code of the real toolchain (`npm run build`, `dotnet build/publish`, `mvn verify`, `pip wheel`, `compileall`) |
| Tests | Fresh JUnit/TRX/TAP reports parsed for passed counts, failures and **named passing** required tests; skipped tests do not satisfy a minimum |
| Contracts | The harness starts the produced app and makes HTTP requests, or runs the produced CLI, then compares status codes, bodies and ordering exactly against the contract in the prompt |
| Expected values | Harness-computed expected outputs (C: report totals from a seeded fixture; D: confusion matrix, accuracy and macro-F1 recomputed from ordered evaluation predictions and compared with the application's reported metrics) |
| Determinism | Two identical training runs must produce byte-identical outputs (D) |
| Protection | SHA-256 of protected tests and data files before and after each turn |
| Immutability | Workspace file hashes before and after plan-mode and fork stages, including generated outputs; ordinary stage diffs exclude build outputs and dependencies |
| Lifecycle | Accepted exit codes, same task after resume, new session after fork, guardrail rejected before acceptance |

Scorecard metrics (`results\scorecard.json`):

- `verdict`: `pass` when every required gate passes in its **latest** attempt (a repair stage
  supersedes the original stage's result for the same gate ID), no paid stage was skipped
  for budget, and no fatal error occurred. `dry-run-pass` covers only the zero-spend
  preflight path; `incomplete` means the budget ceiling prevented requested stages;
- `gate_pass_rate`: latest required gates passed divided by the total;
- `first_pass_rate`: the share of feature turns whose required gates all passed **before**
  any repair;
- `repair_turns`, `exit_code_distribution`, `spend_usd`, `wall_minutes`;
- per stage: exit code, conditions, task and session IDs, cost, attempts, tool items, files
  changed, event counts and duration;
- the produced `assets`, each with its size and SHA-256.

Successful full runs and successful dry runs exit 0; failures and incomplete runs exit 1.
Read the verdict as well as the process exit code: a dry run is not functional delivery
evidence, and an advisory continuation skip is not a successful resume test.

### 3.7 Budget controls

- `-TurnBudgetUsd` (default 3.00) is passed to every run as `--budget-usd`. VCP enforces it
  per task. Reviews use the smaller of this value and 2.00 USD. Resume retains the
  original task's durable cap; it does not grant a fresh task budget.
- `-MaxScenarioUsd` (default 30) is enforced by the harness. A stage that could exceed the
  ceiling is skipped and recorded. New tasks reserve their full cap; a resume reserves
  only the original task cap not already accounted for. A fork reserves the selected
  profile's `budget_usd`, which can differ from the source review's smaller run override.
- Concurrent scenarios do not share a ceiling. The worst-case total is the sum of their
  `-MaxScenarioUsd` values.

Budget parameters accept at most two decimal places so the admission guard and rendered
CLI dollar values agree; sub-cent values are rejected before paid execution.

Provider qualification is a separate paid operation and is outside these ceilings.
Settled ledger totals are cumulative per task: a resume contributes only newly observed
spend. Missing, truncated or unresolved cost evidence causes conservative cap accounting
until a later complete ledger for the same task reconciles it. Stage snapshots are advisory
because intentional pauses can leave unsettled liabilities; final accounting requires every
task's latest evidence to be complete and both task and scenario caps to be respected.
An execution without task scope leaves unresolved accounting even if another task settles.
While accounting remains unknown, `spend_evidence_complete` is false and conservative
estimates must not be presented as measured spend.

---

## 4. Scenario A - TaskBoard (TypeScript, Node, Vue)

### 4.1 Summary

A small team builds a task board: a REST API in TypeScript on Express 5, run directly by
Node's built-in type stripping, with JSON-file persistence, plus a Vue 3 single-page app
built with Vite. Over five feature turns VCP builds the API, the UI, a cross-stack feature,
fixes protected regression tests, and makes the app deployable as one Node process that
also serves the built client. The test focuses on API contract precision, coordinating
front-end and back-end changes, keeping both the `node --test` and Vitest suites green, and
production packaging.

**Seed (B0).** The harness writes the scaffold that `npm create vue` would produce, adapted
for the API: `package.json` (express, vue, vite, @vitejs/plugin-vue, vitest, vue-tsc,
typescript, @vue/test-utils, jsdom), Vite, Vitest and TS configs, `server/app.ts` (only
`/api/health`), `tests/health.test.ts`, `src/App.vue`, `src/App.spec.ts` and a README. It
then runs `npm install` and checks the baseline typecheck, tests and build.

**API port.** Gates start the produced server on `-ApiPort` (default 41731).

### 4.2 Turns and gates

| Stage | Task given to VCP | Required gates (beyond `vcp-exit`) |
|---|---|---|
| T1-api | Task CRUD API with exact contract: `{items,total}` list with status/q filters, 201/204/400/404 semantics, `VALIDATION_ERROR`/`NOT_FOUND`/`INVALID_JSON` error codes, atomic JSON-file persistence via `createApp({dataFile})`/`TASKBOARD_DATA`, five named API tests | typecheck; `node --test` incl. 6 named tests; 12 HTTP contract checks: health, create defaults, title/status/impossible-date validation, malformed JSON, list, PATCH refreshes `updatedAt`, filters, 404, **persistence across a restart**, delete |
| T2-ui | Vue board: typed API client, composable, three columns, create form with validation, search, move/delete; required `data-testid`s; at least 3 new Vitest tests | typecheck; Vitest >= 4 tests, 0 failures (JUnit); `vite build` emits `dist/client`; bundle contains `column-todo/doing/done`, `task-card`, `task-form`, `search-input`; API tests still pass |
| T3-labels | Labels (`^[a-z0-9-]{1,20}$`, max 5, unique), `label=` filter, `sort=createdAt|priority|dueDate` with defined tie-breaks, UI chips and sort selector | 2 more named tests; label validation (uppercase, duplicate, >5 rejected); sort orders asserted exactly on a 4-task fixture (`BDCA`, `DCAB`, `ABCD`), `label=ui` gives `BD`, invalid sort gives 400; bundle has `label-chip`, `sort-select`; **T1 contract replayed** |
| T4-regressions | Harness adds the protected `tests/regressions.test.ts` (title trimming, whitespace-only title, `UNKNOWN_FIELD`, `READ_ONLY_FIELD`) and lists it in `npm test` | 4 regression tests pass; typecheck, build and UI tests pass; protected files unchanged; T1 and T3 contracts replayed |
| T5-production (short deadline) | Serve `dist/client` from the API process with SPA fallback; JSON 404 for unknown `/api`; `GET /api/stats` with overdue; stats bar; README production instructions | resume continues the same task (if paused); `GET /` and `/board` serve index.html; hashed asset served as JavaScript; `/api/nope` gives a JSON 404; stats exactly `{total:4, todo:2, doing:1, done:1, overdue:1}`; protected files unchanged |
| T6-review | Read-only release review ending in a JSON findings block | workspace byte-identical; exit in {0,3,4} |
| FINAL | none | `npm ci` from the lockfile; typecheck; all named tests; Vitest; build; T1, T3 and T5 contracts; protected files |

The VCP-side `node` check carries the cumulative expected test names for each turn
(`profile-T1` ... `profile-T5`), so VCP's own `vcp_verify` result can be compared with the
harness gates.

### 4.3 VCP command sequence (default parameters)

```text
# P0 - preflight (no inference)
vcp --version
vcp $G doctor
vcp $G setup credential status
vcp $G setup profile --snapshot <gen>\qualified\snapshot.json --catalog <gen>\endpoints.json --output <profiles>\base-setup-profile-<run>.json --trust-workspace --budget-usd 3.00 --autonomy ask --affected-path README.md
vcp $G skills list
vcp $G models

# P1 - profile validation (no inference)
vcp $G --config <profiles>\profile-T1-<run>.json setup check
vcp $G --config <profiles>\profile-T5-short-<run>.json setup check
vcp $G --config <profiles>\profile-review-<run>.json setup check
vcp $G --config <profiles>\profile-guardrail-<run>.json setup check

# G0 - guardrail: autonomy above the profile ceiling (expect exit 2, no task)
vcp $G --config <profiles>\profile-guardrail-<run>.json run --file <logs>\G0-guardrail\prompt.md --budget-usd 0.01 --autonomy autonomous

# T1..T4 - each followed by the evidence sweep (3.4) and, on gate failure, one repair run
vcp $G --config <profiles>\profile-T1-<run>.json run --file <logs>\T1-api\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-T2-<run>.json run --file <logs>\T2-ui\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-T3-<run>.json run --file <logs>\T3-labels\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-T4-<run>.json run --file <logs>\T4-regressions\prompt.md --budget-usd 3.00 --autonomy autonomous
[repair] vcp $G --config <profiles>\profile-Tn-<run>.json run --file <logs>\Tn-<name>-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T5 - short deadline, then continuation with the full profile
vcp $G --config <profiles>\profile-T5-short-<run>.json run --file <logs>\T5-production\prompt.md --budget-usd 3.00 --autonomy autonomous
[if exit 8] vcp $G --config <profiles>\profile-T5-<run>.json resume --last
[repair]    vcp $G --config <profiles>\profile-T5-<run>.json run --file <logs>\T5-production-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T6 - plan-mode review
vcp $G --config <profiles>\profile-review-<run>.json run --file <logs>\T6-review\prompt.md --budget-usd 2.00 --autonomy plan

# P9 - read-only evidence sweep
vcp $G sessions list
vcp $G workspace discover
vcp $G history list --limit 128
vcp $G history search tasks --limit 32
vcp $G retention show
vcp $G optimize status
vcp $G optimize report
vcp $G memory search tasks --limit 8
```

### 4.4 Left behind

`workspace\` holds the full TaskBoard project, with `dist\client` (the production bundle),
`package-lock.json`, one git commit per stage, and
`artifacts\taskboard-client-<run>.zip`. Run it with `npm start` (port 41731) after
`npm run build`.

---

## 5. Scenario B - Contoso Inventory (ASP.NET Core Razor Pages, REST, SQL Server)

### 5.1 Summary

An electronics distributor needs inventory management: products, suppliers and stock
movements. It is one ASP.NET Core app with a Razor Pages UI and a RESTful JSON API, using
EF Core migrations against SQL Server (LocalDB by default). VCP designs the data model and
migrations, builds the API and the UI, fixes protected regression tests, and adds
optimistic concurrency with ETags and rowversion. The test focuses on schema evolution, a
real database, problem-details error conventions, antiforgery-protected forms,
WebApplicationFactory testing, and a Release publish that runs.

**Seed (B0).** The harness uses the real templates: `dotnet new sln`, `webapp` (Razor Pages),
`xunit`, `gitignore` and `tool-manifest`. It adds the project reference and pins
`Microsoft.EntityFrameworkCore.SqlServer`/`.Design`/`.InMemory`,
`Microsoft.AspNetCore.Mvc.Testing` and the local `dotnet-ef` tool to the newest stable
release of the installed SDK's major version, queried from NuGet's flat-container index.
The pinned versions are recorded in the scorecard notes. The harness also writes
`appsettings.Development.json` with a per-run database
`VcpInventory_<run>` on `(localdb)\MSSQLLocalDB`, then builds and tests the baseline.

**Ports.** `-AppPort` (default 41750) is used for `dotnet run` gates and `-PublishedPort`
(41751) for the published executable.

### 5.2 Turns and gates

| Stage | Task given to VCP | Required gates |
|---|---|---|
| T1-data | `InventoryDbContext` (Products, Suppliers, StockMovements), validation rules, `HasData` seed of 3 suppliers and 10 products with exact SKUs, `InitialCreate` migration, `public partial class Program`, >= 3 unit tests | build; >= 4 tests pass (TRX); `migrations list` contains `InitialCreate`; `database update` applies to SQL Server; advisory: `sqlcmd` counts 10 seeded SKUs; protected appsettings unchanged |
| T2-api | REST API: suppliers, paged and searchable products, CRUD with 201+Location / 409 duplicate / 400 validation-problem keys, movements with sign rules, stock, low-stock report; >= 8 integration tests (InMemory, `Testing` environment) | build; >= 12 tests; migrations apply; **against SQL Server**: suppliers seeded; paging 5/page with the seeded SKUs in exact order; `pageSize=101` gives 400; search; create with Location; duplicate gives 409; error keys `sku` and `unitPrice`; unknown supplier gives a `supplierId` error; receipt 50 + sale -20 gives onHand 30; zero or negative receipt gives 400; movements newest first; low-stock order; delete 409/204/404 |
| T3-razor | Razor Pages `/Products` (search, paging), Create/Edit bound to `Input.*`, Details with "Record movement", `/Reports/LowStock` | build; >= 15 tests; the form flow with a real antiforgery token creates a product that is then found through the API; an invalid post re-renders with `field-validation-error`; details page; low-stock page; post without token gives 400; T2 contract replayed |
| T4-regressions | Harness adds protected `RegressionTests.cs` (SKU normalization, insufficient-stock problem type, whitespace names, supplier email) | >= 19 tests incl. the 4 named; protected files unchanged; API and Razor contracts replayed |
| T5-concurrency (short deadline) | rowversion plus `AddProductRowVersion` migration; ETag on GET; PUT requires If-Match (428/412/200); Edit page concurrency error | resume `<task>` continues the same task (if paused); >= 22 tests; both migrations apply; ETag flow: 428 without, 412 bogus, 200 with a new ETag, 412 on replay; API and Razor contracts replayed |
| T6-review | Read-only review | workspace byte-identical |
| FINAL | none | build; >= 22 passing tests; migrations; `dotnet publish -c Release` gives `Inventory.Web.exe`; the **published exe** passes the API, Razor and ETag gates on port 41751; idempotent migration SQL script |

### 5.3 VCP command sequence (default parameters)

```text
# P0 - preflight (no inference)
vcp --version
vcp $G doctor
vcp $G setup credential status
vcp $G setup profile --snapshot <gen>\qualified\snapshot.json --catalog <gen>\endpoints.json --output <profiles>\base-setup-profile-<run>.json --trust-workspace --budget-usd 3.00 --autonomy ask --affected-path README.md
vcp $G skills list
vcp $G models

# P1 - profile validation (no inference)
vcp $G --config <profiles>\profile-main-<run>.json setup check
vcp $G --config <profiles>\profile-short-<run>.json setup check
vcp $G --config <profiles>\profile-review-<run>.json setup check

# G0 - guardrail: profile without trust_workspace (expect exit 2, no task)
vcp $G --config <profiles>\profile-untrusted-<run>.json run --file <logs>\G0-guardrail\prompt.md --budget-usd 0.01 --autonomy autonomous

# T1..T4 - each followed by the evidence sweep (3.4) and, on gate failure, one repair run
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T1-data\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T2-api\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T3-razor\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T4-regressions\prompt.md --budget-usd 3.00 --autonomy autonomous
[repair] vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\Tn-<name>-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T5 - short deadline, then explicit task resume
vcp $G --config <profiles>\profile-short-<run>.json run --file <logs>\T5-concurrency\prompt.md --budget-usd 3.00 --autonomy autonomous
[if exit 8] vcp $G --config <profiles>\profile-main-<run>.json resume <T5 task id>
[repair]    vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T5-concurrency-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T6 - plan-mode review
vcp $G --config <profiles>\profile-review-<run>.json run --file <logs>\T6-review\prompt.md --budget-usd 2.00 --autonomy plan

# P9 - read-only evidence sweep
vcp $G sessions list
vcp $G workspace discover
vcp $G history list --limit 128
vcp $G history search products --limit 32
vcp $G retention show
vcp $G optimize status
vcp $G optimize report
vcp $G memory search products --limit 8
```

### 5.4 Left behind

`workspace\` holds the solution (`Inventory.sln`/`.slnx`, `src\Inventory.Web`,
`tests\Inventory.Tests`, migrations) with git history. It also holds
`artifacts\publish\Inventory.Web.exe` (the Release publish), a zipped copy and
`artifacts\migrations.sql` (an idempotent deployment script). The LocalDB database
`VcpInventory_<run>` is kept for inspection unless `-DropDatabase` is passed.

---

## 6. Scenario C - ledger-cli (Java console application)

### 6.1 Summary

A personal-finance command-line tool imports bank CSV exports into a local JSON ledger,
categorizes transactions with rules, reports monthly and quarterly totals, checks budgets and
exports data. The harness generates a deterministic three-month fixture: 152 rows, 146 unique
after 6 duplicate rows, with 13 rows that no rule matches. It computes every expected report
itself, so CLI output is compared against harness expectations: money strings, category order, counts and exit
codes. The test focuses on precise CLI contracts, BigDecimal arithmetic, CSV edge cases,
atomic writes, exit-code discipline, and working inside a constrained toolchain: Maven is a
batch script, so VCP reaches it through `java.exe` and the classworlds launcher.

**Seed (B0).** `pom.xml` targets Java 21 and pins picocli 4.7.6, Jackson 2.18.2 and JUnit
5.11.4, with compiler, surefire, jar and shade plugins that produce
`target/ledger-cli-1.0.0-all.jar`. The seed also has a `LedgerApp` picocli root command, a
version test, `samples/transactions-2026Q1.csv` and `samples/rules.csv` (both protected),
and a README. Baseline: `mvn -B -ntp clean verify`.

### 6.2 Turns and gates

| Stage | Task given to VCP | Required gates |
|---|---|---|
| T1-import | `import` with RFC 4180 parsing, normalization, duplicate rules, the exact summary line, line-numbered errors, exit 4, all-or-nothing atomic writes | `mvn verify` with >= 4 tests and the shaded jar; `Imported 146 transactions (6 duplicates skipped)`; re-import gives `Imported 0 transactions (152 duplicates skipped)`; malformed line 7 gives exit 4, `line 7` on stderr and no ledger written; missing file gives exit 4 |
| T2-reports | `categorize` (first-match rules), `report --format json` with exact field semantics and ordering | `Categorized 146 transactions (13 uncategorized)`; JSON reports for 2026-01/02/03 **equal** the harness-computed income, expenses, net and byCategory; empty month gives zeros; missing ledger gives exit 4; T1 replayed |
| T3-budgets | Table format, `budget check` lines and exit 3, usage errors exit 2 | table has the header, every category row and Net; tight budget gives exit 3 with the exact `Dining: spent X of Y (OVER)` and `Housing ... (OK)` lines; loose budget gives exit 0; unknown subcommand gives exit 2; T2 replayed |
| T4-regressions | Harness adds the protected `RegressionTest.java` (parenthesized negatives, thousands separators, quoted commas, re-import duplicates, empty month) | >= 15 tests incl. the 5 named; bank-export fixture with `(1,204.10)`, `"3,250.00"`, quoted commas, extra whitespace and a duplicate gives `Imported 7 transactions (1 duplicates skipped)` and an exact April report; T2 replayed; protected files unchanged |
| T5-export (short deadline) | `export` (csv/json, ordered), `--from/--to` ranges, mutually exclusive options, help, README | `sessions resume <session>` continues the same task (if paused); export JSON has 146 ordered, categorized items; CSV header and rows; quarter report equals the harness Q1 totals; `--month` with `--from` gives exit 2; `--help` lists all subcommands |
| T6-review | Read-only review | workspace byte-identical |
| FINAL | none | `mvn verify`; import, reports, budgets, bank-export and export/range suites against the final jar; protected files |

### 6.3 VCP command sequence (default parameters)

```text
# P0 - preflight (no inference)
vcp --version
vcp $G doctor
vcp $G setup credential status
vcp $G setup profile --snapshot <gen>\qualified\snapshot.json --catalog <gen>\endpoints.json --output <profiles>\base-setup-profile-<run>.json --trust-workspace --budget-usd 3.00 --autonomy ask --affected-path README.md
vcp $G skills list
vcp $G models

# P1 - profile validation (no inference)
vcp $G --config <profiles>\profile-main-<run>.json setup check
vcp $G --config <profiles>\profile-short-<run>.json setup check
vcp $G --config <profiles>\profile-review-<run>.json setup check

# G0 - guardrail: empty task file (expect exit 2, no task)
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\G0-guardrail\empty-task.md --budget-usd 0.01 --autonomy autonomous

# T1..T4 - each followed by the evidence sweep (3.4) and, on gate failure, one repair run
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T1-import\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T2-reports\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T3-budgets\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T4-regressions\prompt.md --budget-usd 3.00 --autonomy autonomous
[repair] vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\Tn-<name>-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T5 - short deadline, then session-level resume
vcp $G --config <profiles>\profile-short-<run>.json run --file <logs>\T5-export\prompt.md --budget-usd 3.00 --autonomy autonomous
[if exit 8] vcp $G --config <profiles>\profile-main-<run>.json sessions resume <T5 session id>
[repair]    vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T5-export-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T6 - plan-mode review
vcp $G --config <profiles>\profile-review-<run>.json run --file <logs>\T6-review\prompt.md --budget-usd 2.00 --autonomy plan

# P9 - read-only evidence sweep
vcp $G sessions list
vcp $G workspace discover
vcp $G history list --limit 128
vcp $G history search ledger --limit 32
vcp $G retention show
vcp $G optimize status
vcp $G optimize report
vcp $G memory search ledger --limit 8
```

### 6.4 Left behind

`workspace\` holds the Maven project with git history. `artifacts\` contains
`ledger-cli-1.0.0-all.jar` (run it with `java -jar`), a sample ledger,
`report-2026-02.json` with `expected-report-2026-02.json` beside it for comparison, and
`export-2026Q1.csv`. The thin jar is in `target\`.

---

## 7. Scenario D - textlab (Python machine learning for text analysis)

### 7.1 Summary

A support team wants to triage tickets automatically by category (account, billing,
feedback, shipping, technical) and sentiment, see the keywords that drive each category,
and get an evaluation report and a model card. The harness generates deterministic,
balanced, templated ticket data in the workspace: train 1,500 and dev 300, both protected.
It also generates a **withheld evaluation set** of 501 tickets outside the workspace: 35% use
phrasings and sentiment cues never seen in training, and one row contains an XSS payload.
Quality gates use that set. It is withheld from the prompt and workspace, but the harness
reuses it across stages and repairs, so the resulting score is not an untouched final-test
estimate. Reduced-isolation processes are also not an access-control boundary around
the run's `hidden` directory. The test focuses on ML engineering:
reproducibility, honest evaluation, robustness to real-world input, calibrated
thresholds, safe reporting, packaging, and following a preprocessing specification
expressed as protected tests.

**Seed (B0).** `pyproject.toml` (setuptools, src layout, scikit-learn/numpy/joblib, a dev
extra with pytest), an argparse entry point with `--version`, one test, README and
`.gitignore`. The harness creates a venv **outside the workspace** (`env\venv`), installs the
project in editable mode, records `pip freeze` in `results\environment-freeze.txt`, and
runs the baseline tests.

**Thresholds** are parameters, because they were chosen from the dataset design and have not
yet been measured: `-BaselineMacroF1 0.80` (T1-T2), `-TargetMacroF1 0.85` (T3 onward),
`-TargetSentimentAccuracy 0.70`. Use a separately labeled pilot to assess feasibility;
freeze thresholds before measured runs and retain any original failing scorecard.
Do not lower a threshold after observing a run and relabel that same run as passing.

### 7.2 Turns and gates

| Stage | Task given to VCP | Required gates |
|---|---|---|
| T1-baseline | `normalize_text`, CSV loading (BOM, quoted newlines), category classifier, `train`/`evaluate`/`predict` (single and batch) with exact JSON shapes, exit 4 for input errors, >= 6 tests | pytest >= 7; train writes models and metadata with sorted labels; **holdout macro-F1 >= 0.80**, n = 501, confusion matrix 5x5 summing to n; **determinism**: two seed-13 trainings give byte-identical batch predictions and metadata, with order preserved; `predict --text` returns billing with confidence in [0,1]; missing file or column gives exit 4 |
| T2-sentiment | Sentiment model, extended predict output, `keywords` command | pytest >= 10; T1 gates with sentiment; **holdout sentiment accuracy >= 0.70**; keywords: 10 lowercase non-stop-word terms per category, with >= 2 known signal words each |
| T3-robustness | Empty text gives `unknown`; 5,000-char truncation; probabilities; `--min-confidence` gives `needs_review`; BOM, emoji, non-English and multi-line input | pytest >= 14; **holdout macro-F1 >= 0.85**; edge-case batch (BOM, empty, whitespace, emoji, Spanish, a 20,000-char text, a multi-line quoted field): ids kept in order, empty rows `unknown`, long refund text billing, confidences in [0,1]; `--min-confidence 0.999` gives >= 1 `needs_review` and `0` gives none |
| T4-regressions | Harness adds the protected `tests/test_regressions.py` (NFKC/casefold, `<url>`, `<email>`, `<order>` masking, whitespace) | pytest >= 19 incl. the 5 named; model and robustness gates still pass after preprocessing changes; protected files unchanged |
| T5-report (short deadline) | Self-contained HTML report (tables, confusion matrix, keywords, escaped examples) and MODEL_CARD.md | If paused: `workspace discover` lists the task with an expected revision; **stale `--expected-revision` gives exit 2 with no events**; valid revision resumes the same task. Report has >= 2 tables and every label; an intentionally misclassified XSS fixture checks escaping; no scripts or external URLs; MODEL_CARD sections |
| T6-review | Read-only ML review | workspace byte-identical |
| T7-fork | `sessions fork` of the review session through its last completed turn | new session and task; workspace byte-identical; advisory: overlap of cited files between the two reviews (Jaccard >= 0.3) |
| FINAL | none | pytest, model, robustness, keywords, report and protection gates on the final code; verified model bytes and metrics copied into deliverables; dev-set report; keywords; wheel built in a fresh directory; `compileall`; required artifact gates |

### 7.3 VCP command sequence (default parameters)

```text
# P0 - preflight (no inference)
vcp --version
vcp $G doctor
vcp $G setup credential status
vcp $G setup profile --snapshot <gen>\qualified\snapshot.json --catalog <gen>\endpoints.json --output <profiles>\base-setup-profile-<run>.json --trust-workspace --budget-usd 3.00 --autonomy ask --affected-path README.md
vcp $G skills list
vcp $G models

# P1 - profile validation (no inference)
vcp $G --config <profiles>\profile-main-<run>.json setup check
vcp $G --config <profiles>\profile-short-<run>.json setup check
vcp $G --config <profiles>\profile-review-<run>.json setup check

# G0 - guardrail: max_requests 0 violates profile bounds (expect exit 2, no task)
vcp $G --config <profiles>\profile-bad-bounds-<run>.json run --file <logs>\G0-guardrail\prompt.md --budget-usd 0.01 --autonomy autonomous

# T1..T4 - each followed by the evidence sweep (3.4) and, on gate failure, one repair run
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T1-baseline\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T2-sentiment\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T3-robustness\prompt.md --budget-usd 3.00 --autonomy autonomous
vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T4-regressions\prompt.md --budget-usd 3.00 --autonomy autonomous
[repair] vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\Tn-<name>-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T5 - short deadline, chooser data, stale-revision rejection, then revision-checked resume
vcp $G --config <profiles>\profile-short-<run>.json run --file <logs>\T5-report\prompt.md --budget-usd 3.00 --autonomy autonomous
[if exit 8] vcp $G workspace discover
[if exit 8] vcp $G --config <profiles>\profile-main-<run>.json resume <T5 task id> --expected-revision <rev+1000>   (expect rejection)
[if exit 8] vcp $G --config <profiles>\profile-main-<run>.json resume <T5 task id> --expected-revision <rev>
[repair]    vcp $G --config <profiles>\profile-main-<run>.json run --file <logs>\T5-report-repair1\prompt.md --budget-usd 3.00 --autonomy autonomous

# T6 - plan-mode review, T7 - fork of that review
vcp $G --config <profiles>\profile-review-<run>.json run --file <logs>\T6-review\prompt.md --budget-usd 2.00 --autonomy plan
vcp $G --config <profiles>\profile-review-<run>.json sessions fork <T6 session id> --through-turn <last completed T6 turn id>

# P9 - read-only evidence sweep
vcp $G sessions list
vcp $G workspace discover
vcp $G history list --limit 128
vcp $G history search holdout --limit 32
vcp $G retention show
vcp $G optimize status
vcp $G optimize report
vcp $G memory search holdout --limit 8
```

### 7.4 Left behind

`workspace\` holds the package with git history, plus:

- `models\` (`category.joblib`, `sentiment.joblib`, `metadata.json`);
- `reports\report.html` and `MODEL_CARD.md`;
- `dist\textlab-0.1.0-py3-none-any.whl` and compiled `__pycache__` bytecode;
- `artifacts\holdout-metrics.json` and `artifacts\keywords.json`.

The holdout and edge-case files stay in `hidden\`.

---

## 8. Known risks and assumptions to confirm on the first run

These limitations need evidence from actual scenario runs:

1. **Environment filtering in process profiles.** Profiles allow only `SYSTEMROOT`, `WINDIR`,
   `PATH`, `PATHEXT`, `TEMP`, `TMP`, `LANG`, `LC_ALL`, `TERM`, `CI` and a few build variables.
   npm, NuGet, Maven and pip normally find the user profile through Windows APIs, but a
   failure inside `vcp_exec` is a real finding: the turn shows it in `inspect-tools.json`
   and the harness gates will still judge the produced code.
2. **Maven invocation (C).** Both the harness and VCP use `java.exe` with the classworlds
   launcher rather than executing `mvn.cmd` as a native binary. Preflight requires Maven
   3.9+ with the distribution's launcher jar and `bin/m2.conf`; a shim or unsupported layout
   fails before paid stages.
3. **`dotnet-ef` inside VCP (B).** Arbitrary environment variables cannot be set through the profile allowlist, so
   prompts tell the agent to pass `-- --environment Development`. The harness's own EF
   commands set `ASPNETCORE_ENVIRONMENT`.
4. **Guardrails exit before acceptance.** The autonomy ceiling, trust, bounds and empty-file
   checks are expected to fail in `profile.prepare` or argument validation, with exit 2 and
   no `accepted` frame. A different outcome is reported as a failed gate.
5. **Short-deadline pause.** If T5 finishes inside `-ShortDeadlineSeconds`, the continuation
   gate is recorded as `skip`, not `fail`. A scenario can therefore pass functional gates
   without covering resume; report that coverage gap explicitly. Lower the value in a
   separate run to make pausing more likely, within the profile's verification bounds.
   D's fork likewise requires a completed review turn; a missing turn leaves fork uncovered.
6. **Final message extraction** depends on the captured SSE response format. Only advisory
   gates depend on it.
7. **ML thresholds (D)** are design estimates, and the fixture is templated synthetic data.
   Keep pilot calibration separate from scored comparisons. These gates establish behavior
   on the fixture, not generalization to real support tickets or calibrated probabilities.
8. **Floating dependency resolution.** npm caret ranges, latest-in-major NuGet versions and
   pip lower bounds resolve when the run happens. The lockfile, the scorecard notes and the
   pip freeze capture what was used. Maven versions are pinned.
9. **Reduced isolation** is used for all toolchain profiles (section 3.3).
10. **Approvals.** If a turn hits an effect outside `automatic_effects`, it stops with exit 4
    and the harness notes the required input. Inspect the requested effect against the
    intended contract before attributing the failure; do not automatically widen permission
    to make the scenario pass. Review stages permit this exit while still checking immutability.
11. **UI and review coverage.** A's component tests and bundle markers and B's HTTP form
    checks do not exercise a real browser or establish visual quality or accessibility.
    A successful read-only review gate proves preservation of files; its findings content
    and D's review consistency are advisory, not proof of a correct review.

---

## 9. The PowerShell scripts

### 9.1 Files

| File | Role |
|---|---|
| `VcpScenarioHarness.psm1` | Shared engine: run-root setup and safety checks; shell-free process execution (`ProcessStartInfo.ArgumentList`) with streamed logs, heartbeats and timeouts; `Invoke-Vcp` with JSONL parsing; the evidence sweep; paged inspect; cost and final-message extraction; workspace manifests; gates; repair loop; profile composition; preflight and guardrail helpers; git checkpoints; scorecard and summary |
| `run-cli-scenarios.ps1` | Interactive scenario and run-mode selection, prerequisite inputs, child process execution and result location |
| `scenario-a-vue-taskboard.ps1` | Seed, prompts, contracts and gates for scenario A |
| `scenario-b-aspnet-inventory.ps1` | Seed, prompts, contracts and gates for scenario B |
| `scenario-c-java-ledger-cli.ps1` | Fixture generator, expected-result calculator, seed, prompts and gates for scenario C |
| `scenario-d-python-textlab.ps1` | Dataset generator, hidden holdout, seed, prompts and gates for scenario D |
| `tests/Harness.Tests.ps1` | Offline regressions for process handling, cost accounting, manifests and scorecards |
| `tests/ScenarioGates.Tests.ps1` | Offline regressions for scenario fixtures and acceptance gates |
| `tests/Accounting.Tests.ps1` | Offline regressions for resume/fork admission and final accounting reconciliation |
| `tests/Launcher.Tests.ps1` | Offline regressions for launcher selection and child-process handoff |
| `tests/Bootstrap.Tests.ps1` | Offline automatic preparation, renewal, receipt recovery, failure preservation and generation reuse |
| `tests/CommandLog.Tests.ps1` | Offline regressions for exact command logging, failure evidence and result-path references |

Each scenario script is self-contained apart from the module. Seeds, protected tests and
prompts are embedded as here-strings; the generated project does not import VCP repository
files. Keep the scenario scripts and module from the same revision when copying them.

Before running a scenario, run the offline checks from the repository workspace root:

```powershell
pwsh -NoProfile -File docs/test-plans/tests/Harness.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/ScenarioGates.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/Accounting.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/Launcher.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/CommandLog.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/Bootstrap.Tests.ps1
```

These checks use local fixtures and subprocess probes; they need no VCP installation,
provider generation, provider credential or network access. They validate the harness,
not the generated applications or paid provider workflows. Run `-SkipPaidStages` next
to check the installed CLI, qualified generation and actual scenario toolchains.

### 9.2 Common parameters

| Parameter | Default | Meaning |
|---|---|---|
| `-ProviderGeneration` | optional in launcher | Reuse an existing qualified directory; individual scenario scripts require it |
| `-Model` / `-Endpoint` | installed selection when available | Launcher provider selection; prompts when missing |
| `-SetupBudgetUsd` | prompt (3 USD offered) | Launcher qualification cap, separate from scenario spend, at most 25 USD |
| `-RequestPriceLimitUsd` | 0.001 | Maximum provider request fee during qualification |
| `-RunRoot` | `%SystemDrive%\vcp-scenarios` | Parent folder for run outputs |
| `-Vcp` | installed launcher | Path to `vcp.exe` (or set `VCP_EXE`) |
| `-TurnBudgetUsd` | 3 | `--budget-usd` per task |
| `-MaxScenarioUsd` | 30 | Harness spend ceiling for the scenario |
| `-MaxRepairTurns` | 1 | Repair turns per failed stage |
| `-OutputTokens` / `-MaxRequests` / `-DeadlineSeconds` | 8192 / 96 / 1800 | Profile limits per task |
| `-ShortDeadlineSeconds` | 150 | Deadline for the T5 continuation test (at least 121 in A because of its check) |
| `-SkipPaidStages` | off | Zero-spend dry run (P0, B0, P1, G0) |

Scenario-specific: A `-ApiPort`; B `-AppPort`, `-PublishedPort`, `-SqlConnectionString`,
`-DropDatabase`; D `-BaselineMacroF1`, `-TargetMacroF1`, `-TargetSentimentAccuracy`.

### 9.3 Running two or more scenarios at the same time

Open one PowerShell 7 window per scenario. In each window:

```powershell
pwsh -NoProfile -File D:\code\Github\vcp\docs\test-plans\run-cli-scenarios.ps1
```

Choose A, B, C or D, then dry run or full run and an output folder. Dry run is the default.
The launcher discovers a valid generation under `%LOCALAPPDATA%\VCP\profiles` or a
previous launch's `setup` directory beneath the chosen output folder, or
prepares one automatically as described in section 2.2. `-ProviderGeneration` and
`VCP_PROVIDER_GENERATION` optionally select an existing generation; neither is required.
The launcher streams progress in that console and prints the preparation, workspace,
result and command-log locations when the child exits. Preparation commands and results
live under `<invocation>\setup\logs` and `<invocation>\setup\results`, including failures.

For a repeatable selection, pass the options explicitly:

```powershell
pwsh -NoProfile -File D:\code\Github\vcp\docs\test-plans\run-cli-scenarios.ps1 -Scenario A -Mode Full -RunRoot D:\clitests\A
```

The individual scenario scripts also remain available for scenario-specific parameters:

```powershell
# 1. zero-spend dry run first (no credential needed)
pwsh -File D:\code\Github\vcp\docs\test-plans\scenario-a-vue-taskboard.ps1 -ProviderGeneration C:\vcp-private\provider-20261002 -SkipPaidStages

# 2. credential for this console only (section 2.3)
$secret = Read-Host 'OpenRouter API key' -AsSecureString
$env:OPENROUTER_API_KEY = [pscredential]::new('k', $secret).GetNetworkCredential().Password
$secret = $null

# 3. the real run
pwsh -File D:\code\Github\vcp\docs\test-plans\scenario-a-vue-taskboard.ps1 -ProviderGeneration C:\vcp-private\provider-20261002
```

The second window does the same with, for example, `scenario-c-java-ledger-cli.ps1`.
Concurrent runs are isolated by design:

- each run has its own timestamped run root, `--data-dir` (and therefore its own canonical
  store and owner pipe), profiles and TEMP;
- ports differ per scenario (A 41731; B 41750/41751; C and D use none). Two copies of the
  **same** scenario need different ports;
- LocalDB is shared, but each B run creates its own database. npm, NuGet, Maven and pip
  caches are safe to share;
- spend ceilings are per process (section 3.7). The shared provider key may hit rate limits.
  Retries are bounded by `max_transport_retries: 2`;
- CPU-heavy builds (`dotnet`, `mvn`, `vite`) in parallel lengthen wall time but do not affect
  correctness. Gate timeouts are generous (15-30 minutes per toolchain command).

Each console shows timestamped stage, event and gate lines prefixed with the scenario name.
A heartbeat appears after 30 seconds and then every 60 seconds during long commands.
Harness timeouts terminate the process tree. Ctrl+C or forced termination is not proof of
durable task cancellation: inspect task state and unresolved effects before resuming or
starting another run. An interrupted script may not write its final scorecard.

### 9.4 Reading the results

1. Open `results\summary.md` for the verdict, the stage table, failed and skipped gates
   (including those later repaired), the produced assets and notes.
2. For a failing gate, read its detail in `scorecard.json`, the tool log it names under
   `logs\<stage>\tools\`, and the stage's `prompt.md`, `final-message.md` and
   `workspace-diff.json`.
3. To see what VCP actually did, use `inspect-tools.json`, `inspect-verification.json` and
   `inspect-policy.json` for the stage. Re-run read-only commands at any time:
   `vcp --format jsonl --non-interactive --workspace <ws> --data-dir <run>\vcp-data inspect <task> --view chain`.
4. To review code changes per turn, run `git -C <run>\workspace log --stat`. Each stage,
   including harness-inserted protected tests, is a separate commit.
5. To compare runs or models, put several `scorecard.json` files side by side; they share
   the schema `vcp-practical-scenario/1`.

### 9.5 Expected duration and spend

These are planning estimates, not measurements or wall-time guarantees. With default
repair settings, a scenario has 5 feature turns, up to 5 repair turns, 1 review and, in D,
1 fork. Each task has a spend cap; each execution has a profile deadline, with a new
deadline when a paused task resumes. The harness guards total spend with `-MaxScenarioUsd`.
Wall time is estimated at
**1.5-4 hours** per scenario, dominated by model turns and toolchain builds. Record actual
time and spend from `scorecard.json` after the first runs and update this section.
