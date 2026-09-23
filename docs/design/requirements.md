# VS Code extension — visual design requirements

Status: design requirements for the deferred VS Code client (P4-01 through
P4-05). This document creates no implementation, acceptance evidence, protocol
version or supported-editor claim. It refines
[architecture section 18](../architecture/vcp-what.md#18-vs-code-extension),
[plan segment 18](../plan/18-deferred-vscode.md) and the
[deferred client design](../architecture/deferred-clients-design.md), and is
bounded by [ADR-011](../adr/011-extension-scope.md) and
[ADR-012](../adr/012-clients-and-distribution.md). Where this document and the
architecture disagree, the architecture governs.

The two concept images in this folder (`interactive.jpg`, `reporting.jpg`) are
stills from a generated promotional video. They are inputs, not designs. Section
2 records what they suggest, what VCP keeps, what is translated into VCP's real
vocabulary, and what is rejected because the engine cannot truthfully show it.

The rendered mocks live in [`mocks/`](mocks/) with their HTML sources in
[`mocks/src/`](mocks/src/). Section 9 indexes them; section 12 explains how to
regenerate them.

## 1. Purpose, scope and grounding

The extension is a second client of the same engine the CLI drives. It renders
the engine's durable state and events; it does not schedule work, call a model,
evaluate policy, hold credentials or keep a private transcript. Engine mutations
and queries use qualified SDK bindings; navigation, focus and form editing are
local presentation actions. Engine state and client transport or command progress
are labelled separately. Section 13 specifies implementation boundaries without
authorizing implementation now.

Grounding used for these requirements:

| Source | What it fixes for the design |
|---|---|
| `vcp-domain` state enums (`task.rs`, `effect.rs`, `accounting.rs`, `agents.rs`, `memory.rs`, `policy.rs`, `verification.rs`) | The exact state, cost, approval, child, claim and policy vocabulary the UI may display |
| `vcp-protocol` (`event.rs`, `command.rs`, `subscription.rs`) | 26 event kinds, envelope fields, approval binding, cursor/gap contract |
| `vcp-cli` (`terminal.rs`, `terminal/owner.rs`, `agents_view.rs`, `continuation.rs`, `history.rs`, `optimize.rs`, `skills.rs`, `mcp.rs`, `exit_status.rs`) | The qualified interactive surface the editor must preserve: slash commands, question semantics, steering semantics, agents page schema, chooser rows, inspector views, bounds and truncation markers |
| `vcp-audit/src/inspection.rs`, `history_query.rs` | Inspector views, page shape, visibility markers, cursor invalidation |
| `vcp-context/src/manifest.rs` | Context part kinds, trust classes, inclusion/omission reasons |
| `vcp-models/src/routing.rs`, `escalation.rs` | Groups, profiles, routing decision record, exclusion codes |
| Plan segments 07, 14, 17, 18; development notes `p3-terminal.md`, `p3-inspection.md`, `p3-continuation.md`, `p7-child-workspaces.md` | Qualified behaviour and the deferred P4/P9 construction sequence |

Two facts constrain everything below:

1. **No public protocol or SDK exists yet.** The engine is driven by the private
   typed `CommandEnvelope`/`EventEnvelope` pair; the JSON-RPC method set in
   [architecture 5.3](../architecture/vcp-what.md#53-minimum-methods) is design
   intent owned by P9. The mocks assume the P9 SDK exists and name methods from
   that table where available. Other controls reference internal/CLI capabilities
   whose public exposure P9 must still define; they are capability-gated, not
   permission to invent an RPC or invoke a CLI subprocess (section 13.3).
2. **The CLI has no TUI.** Its interactive view is a plain, colour-free text
   block re-emitted to stderr (`vcp-cli/src/terminal/owner.rs:526-545`), with
   JSON fragments interpolated. The editor is therefore the first VCP surface
   with real layout, and must not drift from the CLI's semantics while adding
   presentation.

## 2. The concept images: keep, translate, reject

| Concept element (image) | Disposition | VCP translation and reason |
|---|---|---|
| Header "local-agent · Inspecting repository: quantum-core" (interactive) | Translate | Status bar item + Session card: task objective, `TaskState`, controller/observer role, host. "Inspecting" is not a state; the engine's `TurnState` stages are. |
| Branch and ahead count "main ↑2" | Keep (native) | Already provided by VS Code's SCM status item; VCP shows the engine's repository fingerprint, not a second Git widget. |
| "AGENT SESSION" stage timeline (Cloning → Analyzing → Planning → Executing, with Complete / In progress / Pending) | Translate | The "Current turn" rail renders recorded `TurnState` visits with per-stage evidence (manifest, reservation, model, active effect), including repeated visits and suspension branches. Section 6.5 defines the rail; it is not a predetermined sequence. |
| Sub-bullets under a stage ("Scanning files", "Building symbol index") | Translate | Sub-lines are bounded facts from events: context part counts, reservation amount, running effect and elapsed time. No narrative sub-steps that the engine did not emit. |
| Repository tree pane | Reject | VS Code's Explorer already exists. The useful VCP analogue is the **context manifest** (which files/instructions were selected, trust class, rank, omission reason) shown in the Inspector `context` view, and the **Changes** view for prepared/applied files. |
| "SUMMARY · Repository / Branch / Status" block | Translate | Workspace view: engine identity, role, host, sandbox capabilities, root bindings and trust. |
| Progress bar "42%" | Reject as task progress; translate as budget | VCP has no percent-complete metric for a task and must not invent one. The budget meter shows `settled / active / unresolved / protected` against root `cap`. Child usage shows `known + reserved + uncertain` against `allocation`; allocations are separate from spend. |
| Tip footer "Use /help to see available commands" + prompt caret | Translate | The Steer input with slash-command completion limited to the commands the engine implements (section 5.3). |
| "PARENT TASK · Delegated to 3 agents across 3 worktrees · 3/3 worktrees" (reporting) | Keep | Agents view parent card: objective, child count, isolated roots, graph limits (depth/concurrency), allocations meter. |
| "WORKTREE 01 · Agent A · 80%" card with progress bar | Translate | One card per child from the `agents_view` item schema: role, mode (`read_only` / `isolated_write`), exact model, isolated root, readiness, state/reason, node-local cost vs allocation, last activity. The bar is allocation usage, not completion. |
| Checklist rows with checked circles | Translate | Two honest lists: **required checks** with `CheckOutcome` (`passed` / `failed{reason}` / `not_run{reason}`) and **acceptance criteria** shown as text, labelled "assessed at integration". The engine does not tick acceptance items during a child's run. |
| Footer status "IN PROGRESS / COMPLETED" | Keep | `TaskState` pill plus `reason`, and the child's scheduling blockers (`Owner, State, Ancestor, Dependency, Scope, Deadline, Workspace, Concurrency, ResourceConflict, Budget, Cleanup`). |
| "DELEGATION SUMMARY · visible child tasks · checkpoints when the terminal closes · claims, evidence and decisions" | Keep as product framing | Maps to: Agents view (visible child tasks), close-to-pause + continuation chooser (checkpoints), Memory claims + Inspector chain + approvals/routing decisions (claims, evidence, decisions). |
| Dark navy / cyan / amber palette, glowing rings, gradient bars | Reject | The extension binds to VS Code theme tokens so it works in any theme, including light and high-contrast. Section 6 defines the token mapping and the semantic colour set. |

## 3. Design principles

- **VD-P1 Truthful state.** Every badge, colour and count is derived from an
  engine record or event. `outcome_unknown`, `reconciliation_pending`,
  `uncertain`, `not_run`, `unknown` and `pruned` are first-class display states
  with their own treatment; they are never collapsed into success or failure.
- **VD-P2 Same records as the CLI.** The editor renders the same projections
  the CLI renders (status snapshot, agents page, continuation candidates,
  inspection pages, history rows). A synthetic trace replayed through both
  clients must produce the same semantic projection (P4-02 test).
- **VD-P3 Native first, webview only for presentation.** Documents, diffs,
  diagnostics, quick picks, tree views, status bar and notifications use native
  VS Code APIs. Card layouts and persistent forms use webview views; Agent focus,
  Inspector, History and Optimize use webview editor panels. Section 13.1 fixes
  their API placement. All use a closed message schema with opaque action IDs.
- **VD-P4 No invented progress.** No spinner implies work is happening; no
  percentage implies completion. Elapsed time since the last durable event is a
  client-derived display and is labelled as such.
- **VD-P5 Actions are engine commands with revisions.** A control submits the
  revision it displayed (`expected_revision`, effect revision, operation digest,
  steering revision) and is disabled while its durable command is pending. A
  stale action returns the engine's conflict; the UI never retries with a new
  identity.
- **VD-P6 Bounded and paged.** Rows, bytes and pages obey the engine's limits
  (8 children per page, 128 items / 512 KiB per inspection page, 64 KiB per
  artifact range, 128 continuation candidates). Older content is fetched
  through authorized pagination; nothing is cached in persistent webview state.
- **VD-P7 Attribution survives density.** Root and child identity, the owner
  role and the pause state stay visible in every collapsed or concise view.
- **VD-P8 Theme-token compliance.** No hard-coded colours; the semantic palette
  in section 6 maps to named VS Code theme colours and must pass in Dark Modern,
  Light Modern and both high-contrast themes.
- **VD-P9 Secrets never enter the page.** Provider keys, recovery material,
  bootstrap credentials and workspace-storage tokens are neither rendered nor
  stored in webview or workspace state. Redacted artifacts show identity and
  digest only.

## 4. Information architecture

### 4.1 Surfaces

| Surface | Kind | Contents |
|---|---|---|
| Activity bar container **VCP** | Native view container | Badge = actionable questions for the controller (0 hides the badge). |
| **Session** view | Webview view in native container | Task card, current turn stepper, model & policy, budget meter, current result checks, children summary. |
| **Questions** view | Webview view | Approval cards (actionable first), historical decisions. |
| **Steer** view | Webview view | Guidance input, slash-command completion, steering state, objective history. |
| **Agents** view | Webview view | Parent card, one card per child (8 per page), paging footer. |
| **Changes** view | Native tree | Prepared / applied / partial change sets, per-file state and receipts. |
| **Inspector** view | Native tree | Paged `chain` navigation: task → turns → request/attempt/effect/verification → artifacts. |
| **Memory** view | Webview view | Read-only memory search, claim summaries, index status. |
| **Retention** view | Webview view | Due retention notice and controls. |
| **Workspace** view | Webview view | Engine, protocol, role, host, sandbox capabilities, roots, trust; sub-sections Skills and MCP. |
| Editor: **prepared change diff** | Native diff + companion review view | Original snapshot vs prepared content, binding details, apply/re-prepare/discard; drawn banner is conceptual placement. |
| Editor: **VCP: Agent \<task\>** | Webview | Child assignment, lifecycle, cost, integration plan, transcript preview, findings, controls. |
| Editor: **VCP: Inspect \<id\>** | Webview | The ten inspector views: `chain, context, prompts, outputs, routing, policy, tools, costs, verification, memory`. |
| Editor: **VCP: Optimize** | Webview | Baseline report, interview, preview (prior / requested / effective), apply, rollback. |
| Editor: **VCP: History** | Webview | History rows, selector filters, prune preview and application. |
| Panel: **VCP Events** | Webview view in VCP panel container | Attributed durable events (sequence, timestamp, actor/task, kind, summary). |
| Panel: **VCP Receipts** | Webview view in VCP panel container | Per-file application receipts for the current change set. |
| Status bar | Native status items | Engine/role/host, task state, money, questions, agents, memory index, notices. |
| Notifications | Native toasts | Must-act or would-miss events only (section 5.12). |
| Command palette | Native commands | One command per engine command (section 5.12). |
| Quick pick: **Resume unfinished task** | Native quick pick | Continuation candidates with revision binding. |

### 4.2 Data sources

| Surface | Engine query / command (P9 method where defined) | CLI equivalent today |
|---|---|---|
| Session, status bar | `session/read` snapshot + `events/subscribe`; `task/read` | status block `view_at` (`terminal.rs:398-533`) |
| Questions | `approval_requested` / `approval_resolved` events; `approval/respond` | `/answer <id> allow\|deny` |
| Steer | `turn/steer` with expected steering version | plain text input (`owner.rs:241-251`) |
| Agents | `tasks agents <root> --offset N` projection; `/agents …` verbs | `agents_view.rs:160-188` |
| Changes / diff | `editor/context`, `editor/changeResult`, `diff/read` | disk receipts only; editor path is P4-03 |
| Inspector | `InspectionQuery{id, view, limit, cursor, range}` → `InspectionPage` | `vcp inspect <id> --view …` |
| Memory | `memory/query`, `memory/inspect` | `vcp memory search\|inspect` |
| History / retention | `history list\|search`, `prune show\|apply`, `retention show\|set` | `vcp-cli/src/history.rs` |
| Workspace | `initialize` result; `workspace/open`; `workspace trust\|rebind`; `skills list`; `/mcp` | `workspace_trust.rs`, `rebind.rs`, `skills.rs`, `mcp.rs` |
| Start / resume | `turn/start`; `Query::Continuation`; `session/resume --expected-revision` | `continuation.rs`, `resume` |
| Optimize | `optimize report\|answer\|preview\|apply\|rollback`, `/groups` | `optimize.rs`, `optimize/offline.rs` |

## 5. Surface requirements

Requirement IDs are stable; each names the mock that demonstrates it.

### 5.1 Status bar (mock 13)

- **VD-SB-1** Left-aligned VCP item shows the connection: `VCP` when connected;
  `VCP · Restricted` (warning background) when workspace trust is restricted;
  `disconnected` when no engine is attached.
- **VD-SB-2** Task item shows `VCP: <TaskState> · <TurnState>` for the active
  root task, with the pause qualifier `local hold` / `inherited hold` when
  paused. Clicking opens the Session view.
- **VD-SB-3** Money item shows `$ <settled> / <cap>` plus `+<unresolved>?` in
  the warning colour when the ledger has unresolved liability. It covers the
  whole task tree, never one node.
- **VD-SB-4** Questions item appears only when actionable approvals exist for
  this client's controller lease; an observer sees the count without the
  actionable colour.
- **VD-SB-5** Right side shows `engine <version> · <controller|observer> ·
  <host>`; an incompatible engine replaces it with an error-background item.
- **VD-SB-6** `budget_exhausted`, `outcome_unknown` and a due retention notice
  are visible in the status bar while they hold, using warning/error
  backgrounds as in the mock.

### 5.2 Session view (mocks 01, 02)

- **VD-SE-1** The task card shows objective (sanitized, 1024-byte display budget), task ID,
  turn number, task revision, steering revision when it differs, `TaskState`
  pill, owner role (`controller · this window` / `observer`) and host.
- **VD-SE-2** The "Current turn" stepper lists the `TurnState` stages reached
  in this turn, marking done / active / waiting / failed. Each stage shows one
  evidence line: context part counts and manifest id; reserved amount; requested
  model; running effect and its command; verification progress.
- **VD-SE-3** "Model & policy" shows the exact qualified model id, the `fixed
  model` marker when no routing decision projection exists (today's CLI header
  reports "fixed qualified model; routing groups unavailable"), the cost profile
  (`low | med | high`), autonomy preset (`plan | ask | workspace | autonomous`)
  and policy revision. Served model identity is shown as "not normalized — read
  from response evidence" unless the response artifact reports it.
- **VD-SE-4** The budget meter renders the root `Ledger` as four segments —
  `settled` (solid accent), `active` (hatched accent), `unresolved` (hatched
  warning), `protected` (neutral, anchored to the right end) — against `cap`,
  with a legend and the numeric line `<settled> spent · <active> reserved ·
  <unresolved> uncertain · cap <cap>`. Money is formatted from micros with up to
  six decimals; never a float.
- **VD-SE-5** "Current result" lists the newest applicable verification's
  checks with `CheckOutcome`: passed (✓ green), failed (✕ red, reason), not_run
  (dashed circle, reason). Show active check execution separately, derived from an
  observed effect; running is not a recorded `CheckOutcome`. The fingerprint triple
  (repository / buffers / environment) is shown beside the heading.
- **VD-SE-6** "Children" lists up to eight children with role label,
  objective, `TaskState`, and node-local known cost; blocked children show the
  blocker name. The list links to the Agents view.
- **VD-SE-7** Distinct presentations exist for `waiting_for_input`,
  pause-operation progress labelled `pausing`, `paused` (with unresolved effects and uncertain money listed),
  `completed` (completion contract: checks, changed files with receipts,
  outstanding issues, spend certainty), `failed` with reason (verification
  failure shows the failed check and the exit-condition `incomplete`),
  `blocked · budget_exhausted` (available formula and Raise cap / Report partial
  / Cancel), `cancelled`, and effect `outcome_unknown` requiring reconciliation.
  `pausing` and `outcome_unknown` are not `TaskState` values. Terminal failed or
  cancelled tasks offer evidence and New task, never terminal-state Resume.
- **VD-SE-8** `pausing` is shown until the stop boundary and checkpoint are
  established; it lists what is still stopping or unknown. `paused` keeps
  status, cost, history and inspectors usable and offers a single deliberate
  **Resume (revalidates)** action, disabled with a visible reason while blocking
  questions/effects or other admission conditions remain. Reconnect, reload, viewing or a heartbeat
  never resume.
- **VD-SE-9** Raising the cap is an explicit dialog that records a durable
  ledger decision; the UI states it does not apply to unknown provider charges.
- **VD-SE-10** No element in the Session view indicates a completion the
  engine has not committed. A closed subscription or disconnected UI shows
  `disconnected`, not the last known state as current.

### 5.3 Steering (mock 03)

- **VD-ST-1** The Steer input accepts plain text (≤64 KiB) as durable
  guidance. Submitting shows, in order, the engine's actual states: queued
  ("admission is fenced while existing effects stop; task stays paused for
  Resume"), applied as steering revision N, or not applied with the engine's
  reason. A second submission while one is pending is rejected with "guidance
  already queued".
- **VD-ST-2** After guidance is applied the view offers Resume and shows the
  append-only objective history (one row per steering revision).
- **VD-ST-3** Slash-command completion offers only commands the engine
  implements: `/pause /resume /cancel /exit /status /cost /history /groups
  /optimize /escalate /skills /mcp /agents … /inspect /read /next /answer
  /memory inspect|prune /prune show|apply /retention show|set`. It does not
  offer `/plan`, `/model`, `/context`, `/compact`, `/permissions` or `/backup`.
- **VD-ST-4** A not-ready service returns the engine's text ("not ready in this
  stage; no work scheduled") inline; the UI never fabricates a result.

### 5.4 Questions and approvals (mock 03)

- **VD-QA-1** A question card renders one durable `Approval`: tool, sanitized
  command/arguments, effect classes, required isolation, directory, the
  binding (effect id and revision, operation digest, steering revision, policy
  revision), `expires_at` with a live countdown, and the policy origin string
  (for example "operation needs scoped user authority").
- **VD-QA-2** The only answers are **Allow** and **Deny**. There is no default,
  no keyboard default, and no answer is submitted by focus, redraw, resize or
  reload. Broader authority (`Grant` with scope `workspace | session | task`
  and target `exact | configured`) is a separate deliberate action in the
  Policy inspector, not a third button on the card; P4-02 must confirm what
  `decide` records before offering it.
- **VD-QA-3** Answering records a decision and shows "Answer recorded; no work
  was dispatched by the answer" with a Resume affordance. Resume is enabled only
  when no actionable question remains.
- **VD-QA-4** After submit, both buttons are disabled and the card shows the
  pending command id until the durable receipt arrives; the card is reconciled
  by that identity after reload or timeout. Duplicate clicks never duplicate an
  approval.
- **VD-QA-5** Expired, superseded (steering or policy revision changed) and
  previously resolved cards remain visible as history with their reason and
  are never re-enabled; a stale answer shows `APPROVAL_STALE`.
- **VD-QA-6** An observer sees the card with disabled controls, the current
  controller identity and a **Request controller transfer** action that maps to
  a lease operation; it cannot answer.
- **VD-QA-7** Model and tool text inside a card is data: Markdown is sanitized,
  links are inert unless explicitly opened, and no HTML in the payload can
  produce a command.

### 5.5 Agents view and Agent focus (mocks 04, 12)

- **VD-AG-1** The parent card shows objective, task id, graph revision, child
  count, isolated-root count, depth and concurrency against `GraphLimits`, and
  an allocations meter (allocations are subdivisions of the cap, not charges).
  The parent-gating notice ("Parent turn ended while children remain…") is
  shown verbatim when the engine reports it.
- **VD-AG-2** Each child card renders the `agents_view` item: name/objective,
  task id, `TaskState` + `reason`, mode pill (`read_only` / `isolated_write`),
  role or helper template with revision, exact `model_policy`, isolated root
  path and root id, readiness (`recorded_ready` / `not_ready` with setup
  reason; process checks availability), latest result reference and
  `result_evidence_status` text, last activity (event, sequence, timestamp),
  active effects (≤8), scheduling constraints (blocker names), cleanup status,
  and node-local `known / reserved / uncertain` against `allocation`.
- **VD-AG-3** Required checks use `CheckOutcome`; `not_run` shows the engine's
  reason (for example "child process filesystem isolation is not qualified;
  applicable checks must run on the integrated parent"). Acceptance criteria are
  listed as text under "assessed at integration".
- **VD-AG-4** Paging follows the engine: eight children per page, sorted by
  task id, with the observation string ("canonical snapshot; dispatch also
  requires current native state and a live owner") in the footer.
- **VD-AG-5** Card controls map to `/agents focus|follow|pause|cancel|resume|
  integrate|apply|recover|cleanup`. Controls that the child's state does not
  permit are disabled with the reason (terminal child, held child needing
  parent resume first, cleanup blocked by unintegrated edits).
- **VD-AG-6** The Agent focus editor shows the assignment (`ChildSpec`
  fields), the lifecycle stages actually recorded (spec admitted → workspace
  ready → running → result submitted → quiescent → integration plan → parent
  apply → parent verification → cleanup), node-local cost, the integration plan
  with per-path status and `ConflictKind`, the transcript preview (labelled
  "untrusted unstamped child transcript", 4 KiB preview with the explicit
  truncation marker), and review findings with location, trigger, consequence,
  evidence references, uncertainty, defect-vs-suggestion and
  introduced-by-change (default unknown), each labelled with its examined
  revision and "not re-examined against current files" when stale.
- **VD-AG-7** A child pass is always labelled child-state evidence; parent
  completion is shown to require current parent verification.
- **VD-AG-8** New-helper and explicit-spec forms collect exactly the
  `DelegationRequest` fields (template or role, objective, read scope, write
  paths, untracked inputs, allocation USD, seconds ≤3600, git executable,
  disposable parent, acceptance, required checks). The form states that the
  model is inherited and exact, that narrower helper read scopes fail
  explicitly, and that admission is pending until the engine acknowledges.
- **VD-AG-9** Cleanup is three deliberate steps — preview, apply, reconcile —
  with eligibility, retained result, live references, changed-content and
  `--reject-edits` shown before anything is removed. Viewing, reopening, paging
  or a timer never initiates deletion.

### 5.6 Changes, prepared edits and receipts (mocks 01, 05)

- **VD-CH-1** The Changes view groups files by state: prepared (awaiting
  apply), applied (with receipt id), partial / `outcome_unknown` (needs
  reconcile), and conflict (buffer or disk changed since prepare). The header
  shows change-set id, effect id and state, the expected buffer versions and
  disk fingerprint, policy and steering revisions.
- **VD-CH-2** The prepared-change diff uses the native diff editor with the
  original side labelled by its source (`BUFFER vN (dirty)` or `DISK fp…`) and a
  banner showing change-set id, expected document version and hash, disk
  fingerprint, and whether the edit was prepared against the editor buffer or
  disk.
- **VD-CH-3** Apply is enabled only while live document version, hash, disk
  fingerprint, workspace trust and operation authority match; the banner turns
  to the stale/conflict variant the moment any differ, the user's typing is
  preserved, and the only offered paths are re-prepare, show my edits, or
  discard. The UI never applies over newer text.
- **VD-CH-4** Receipts are per file and distinguish buffer receipts from disk
  receipts (before/after version and hash, saved/unsaved, disposition). Partial
  multi-file application stays partial; an interrupted receipt exchange shows
  `outcome_unknown` with a reconcile action; nothing is reapplied on timeout.
- **VD-CH-5** Verification coverage is shown per receipt: which fingerprint the
  checks ran against and whether they covered disk, buffers or both; checks
  tied to an obsolete fingerprint are marked stale.

### 5.7 Inspector (mocks 06, 07)

- **VD-IN-1** The Inspector tree is the paged `chain` view; the editor page has
  one tab per engine view: `chain, context, prompts, outputs, routing, policy,
  tools, costs, verification, memory`. The page header shows scope, source
  watermark and the caller's access.
- **VD-IN-2** The chain strip renders the six-node evidence chain (task →
  request/context → attempt/reservation → tool proposal/policy →
  dispatch/outcome → verification) with ids and one summary line each.
- **VD-IN-3** Every item shows its `visibility`: `available`, `pruned` (reason
  and source, identity and digest retained), `truncated` (serialized bytes),
  `governed_query_required` (claims), plus `partial` for artifacts with
  omissions and `redacted` for omitted authentication/recovery material.
  Attempt items show `model_identity` with requested and served identities
  separately; served is "not_normalized — read from response evidence".
- **VD-IN-4** Artifact content is read on demand in ranges of at most 64 KiB
  with descriptor, byte range, `next_offset`, and the CLI's truncation marker
  style ("… [display truncated; use Next range]"). Content is never persisted in
  webview state and is re-authorized on every range read.
- **VD-IN-5** A cursor invalidated by watermark, authority or retention change
  shows the engine's restart notice and a **Restart without cursor** action;
  pages never mix revisions silently.
- **VD-IN-6** Costs view renders the ledger fields by name (`cap, settled,
  active, unresolved, protected, allocations, overrun, daily`) and the computed
  available projection separately,
  attribution by `RequestRole`, reservations with `ReservationState`, each
  uncertain attempt's reason string, and an explicit **Resolve explicitly**
  action for unresolved liabilities.
- **VD-IN-7** Routing view renders the `RoutingDecision`: profile, ordering,
  selected and fallback identities, candidates (eligible first) with group,
  quality bps and samples, p95 latency, total estimate and exclusion codes,
  the cost-breakdown line, escalation counters, and an explicit gap when no
  decision projection exists. Groups (Frontier/High/Medium/Low) and profiles
  (low/med/high) are labelled as different things.
- **VD-IN-8** Policy view narrates the evaluated decision order (admission,
  identity drift, trust, ceilings, isolation, host denials, user denials, plan
  mode, grants, autonomy preset, question) with the origin string of each
  branch, the operation record, denials in force with `RuleOrigin`, and current
  grants. A historical grant displayed here never implies current authority.

### 5.8 Memory, history and retention (mock 08)

- **VD-ME-1** Memory search is read-only and bounded; each result shows
  `ClaimKind`, the subject · predicate · statement triple, `Outcome`
  (`accepted | disputed | rejected | awaiting_review`), `EvidenceStatus`
  (`verified | observed | inferred | unverified`), version, applicability
  (roots, branch, paths), rank, freshness and evidence references.
  `awaiting_review` and `inferred` claims are visibly not authoritative.
- **VD-ME-2** Navigation works both ways: claim → source events and event →
  derived claims, using the inspector's access checks.
- **VD-ME-3** History rows show sequence, kind, task/agent, summary,
  `content_truncated`, `recall_excluded`, `compacted`, artifact links with
  availability and original bytes, and redacted stand-ins ("content removed,
  identity retained" with digest and epoch). Retained gaps are shown with their
  `GapReason`. The header shows the local timezone and that filters use UTC
  boundaries, the watermark and the count of newer events.
- **VD-ME-4** The retention notice (history older than 30 days) is a
  non-blocking view section and toast with oldest date, size and controls; it
  states that policy is notification-only unless an automatic policy exists
  and repeats on the configured cadence until acknowledged.
- **VD-ME-5** Prune preview shows the selector, action (`exclude |
  restore_recall | compact | purge`), the three buckets (selected, dependent,
  protected with reasons), byte estimate with `bytes_are_exact`, expected search
  impact, and backup copies reported separately. Apply is bound to the preview
  id and watermark; a changed watermark requires re-preview. The four actions
  are described distinctly (exclusion is reversible; compaction does not claim
  deletion; purge propagates and leaves tombstones).

### 5.9 Workspace, connection, trust, skills and MCP (mock 09)

- **VD-WS-1** The Workspace view shows engine executable path, version and
  build, negotiated protocol and event schema versions, frame and subscriber
  limits, role (`controller` with lease id and owner epoch, or `observer` with
  the controller's identity), execution host, provider display name with the
  key shown only as present/absent, and data root and store backend.
- **VD-WS-2** Sandbox capabilities are listed from the host's evidence-backed
  `Isolation` set with supported, unsupported and unknown treatments; the view
  states that unavailable enforcement is unavailable for a required policy and
  that a Git worktree is never an OS sandbox.
- **VD-WS-3** Roots are listed as folder path plus host → workspace id, root
  id, binding revision, repository fingerprint and trust. Identically named
  folders are disambiguated by identity, and the view states that grants and
  memory are not shared across roots.
- **VD-WS-4** Restricted Mode shows a warning banner, what is blocked (new
  tasks, queued effects, prepared actions needing revalidation), the fact that
  workspace files were not read for engine configuration or hooks, and links to
  VS Code's trust management and to the durable `workspace trust` command.
- **VD-WS-5** Engine problems have distinct cards: not configured (no
  download or workspace-specified executable), incompatible
  (`VERSION_CONFLICT` with found/required versions and a link to the
  compatibility matrix), and store locked by another owner (attach as observer;
  never a second writer).
- **VD-WS-6** A moved or replaced root shows the identity mismatch, retained
  history, and explicit **Rebind** / **Open as new workspace** actions with the
  consequences (original store kept, old authority invalidated, trust reset,
  imported grants unused).
- **VD-WS-7** Skills list qualified ids (`source::package::id`), version, source
  kind, and availability (`activated` with revision, `compatible`,
  `incompatible` with the missing environment/tool, `disabled`, `ambiguous`,
  `missing`) plus the stale marker requiring rediscovery; controls are
  rediscover, activate, disable.
- **VD-WS-8** MCP servers show transport, protocol version, generation,
  connection identity digests, allowed tools with trusted effect classes
  (untrusted default `opaque`), resources and prompts, and the rejected
  capabilities (`sampling, roots, elicitation, tasks`). A changed tool schema
  digest is shown as invalidating prior grants. MCP control is session-scoped
  and never blocks pause or cancel.

### 5.10 Start and resume (mock 10)

- **VD-SR-1** Opening a workspace with unfinished tasks shows a non-blocking
  notice and a quick pick of continuation candidates ordered by latest
  task-tree activity then task id, each row showing state, reason, revision,
  last activity, children with `paused_ancestors`, unresolved effects,
  unsettled reservations, pending approvals, waiting-for-input, observed
  changes, artifacts and ledger. Escape leaves everything paused. The bounded
  list (128) and the explicit-id path are stated.
- **VD-SR-2** Selecting a candidate shows a read/reconcile-only revalidation
  preview (workspace, instructions, policy, budget, effects, owner) bound to
  `expected_revision`; unresolved effects must be reconciled before Resume is
  enabled. Nothing is dispatched by selection.
- **VD-SR-3** The new-task form collects objective (task input, never a
  script), required USD cap (decimal, ≤6 fractional digits), cost profile,
  autonomy preset with a one-line explanation, optional skills (≤32) and the
  ephemeral editor context (document, version, dirty flag, selection ranges,
  diagnostics with producer and time). Buffer content is included only on
  explicit request and is labelled ephemeral. Run creates one durable
  `turn/start` with an idempotency key; nothing billable happens before
  validation.

### 5.11 Optimize (mock 11)

- **VD-OP-1** The Optimize page follows the workflow in
  [architecture 7.8](../architecture/vcp-what.md#78-interactive-project-optimization):
  baseline report (tasks by outcome, attempts, retries, support attempts, known
  spend, uncertain attempts, reserved liability, coverage and small-sample
  notes), observed patterns labelled as correlation, pruning suggestions shown
  separately and never applied here, the four interview questions (`priority,
  expected_size, review_preference, model_restrictions`) with saved answers and
  revision checks, and a preview table with **prior / requested / effective**
  columns where effective reflects trusted ceilings.
- **VD-OP-2** The page states the engine's limits verbatim: local analysis
  with no inference or paid trial; cannot grant network/tool capability or raise
  a cap; changing routing never deletes memory; prior revision retained with
  rollback.
- **VD-OP-3** The `/groups` catalog is reachable from the routing view with
  eight candidates per page and the fixed caveat that groups differ from cost
  profiles.

### 5.12 Notifications and commands (mock 13)

- **VD-NO-1** Toasts are limited to: task paused by owner loss/reload,
  prepared change invalidated, workspace trust revoked, budget exhausted,
  children finished after the parent turn ended, engine incompatible,
  retention notice, effect outcome unknown, backup created/failed at a pause or
  completion boundary, controller lease lost or transferred. Each carries the
  affected ids and only actions that map to engine commands.
- **VD-NO-2** No toast announces completion, approval or resumption on the
  client's own initiative; completion toasts are emitted only from a committed
  engine result.
- **VD-NO-3** Command palette entries are `VCP: …` verbs for qualified engine
  operations or local navigation (run, resume, pause, cancel, answer, steer, agents and helper verbs,
  inspect, memory search, history/prune, optimize, add selection to context,
  trust, rebind, cleanup, backup, doctor, show engine output). The palette does
  not offer mode or model switching mid-turn.

## 6. Visual language

### 6.1 Theme tokens

The mocks are rendered in Dark Modern, but every colour is a theme token.
Implementations must use these ids (or the nearest equivalent the extension API
exposes) and must not hard-code the hex values.

| Role in the design | VS Code theme colour | Dark Modern value used in mocks |
|---|---|---|
| Text, secondary text, disabled | `foreground`, `descriptionForeground`, `disabledForeground` | `#cccccc`, `#9d9d9d`, `#6f6f6f` |
| Surfaces | `editor.background`, `sideBar.background`, `panel.background` | `#1f1f1f`, `#181818` |
| Borders | `sideBar.border` / `panel.border`, `input.border` / `widget.border` | `#2b2b2b`, `#3c3c3c` |
| Primary action, focus, active indicator, "running" | `button.background`, `focusBorder`, `activityBar.activeBorder`, `progressBar.background` | `#0078d4` |
| Links and informational | `textLink.foreground`, `editorInfo.foreground` | `#4daafc`, `#3794ff` |
| Success / passed / completed | `testing.iconPassed`, `gitDecoration.addedResourceForeground` | `#73c991` |
| Warning / uncertain / unknown / waiting | `editorWarning.foreground` | `#cca700` |
| Error / failed / denied / blocked / conflict | `errorForeground` | `#f85149` |
| Modified / deleted file decorations | `gitDecoration.modifiedResourceForeground`, `gitDecoration.deletedResourceForeground` | `#e2c08d`, `#c74e39` |
| Diff | `diffEditor.insertedLineBackground`, `diffEditor.removedLineBackground` | translucent green / red |
| Selection, hover | `list.activeSelectionBackground`, `list.hoverBackground` | `#04395e`, `#2a2d2e` |
| Badges | `badge.background`, `badge.foreground` | `#616161`, `#f8f8f8` |
| Inputs | `input.background` | `#313131` |

### 6.2 Typography and iconography

- UI text uses the editor's UI font at the workbench size (13 px in the mocks);
  identifiers, paths, commands, hashes, money and revisions use the editor's
  monospace font (Cascadia Mono in the mocks).
- Icons come from the Codicon set; the mocks use placeholder glyphs. The VCP
  activity-bar icon must be a monochrome Codicon-style glyph that reads at 24
  px and in high contrast.
- Section labels are 11 px uppercase with letter-spacing, matching VS Code
  view headers.

### 6.3 State semantics: colour + icon + text

Colour is never the only carrier. Each state has a fixed pair of glyph and
label so it survives monochrome, high contrast and screen readers.

| Engine state family | Values | Colour | Glyph | Label |
|---|---|---|---|---|
| Running / active | `running`, active turn stage, `dispatch_recorded`, `submitted` | accent | filled dot with pulse ring (static in reduced motion) | state name |
| Success | `completed`, `succeeded`, `settled`, `passed`, `allowed`, `accepted`, `ready` | success | ✓ in filled circle | state name |
| Waiting on the user | `waiting_for_input`, `awaiting_approval`, `pending` approval | warning | ? or hollow warning dot | `actionable` / `waiting_for_input` |
| Uncertain / unknown | `outcome_unknown`, `reconciliation_pending`, `uncertain`, routing `unknown`, selector `Unknown`, `unknown_availability` | warning, hatched fill | ? in outlined circle | the engine word; never "failed" |
| Failed / denied / conflict | `failed`, `cancelled`, `denied`, `rejected`, `blocked`, conflict kinds, `VERSION_CONFLICT` | error | ✕ in filled circle | state name + reason |
| Paused / held | `paused`, `pausing`, `local hold`, `inherited hold` | neutral (foreground dim) | hollow dot / pause glyph | `paused · local hold` |
| Not run / skipped / not evaluated | `not_run{reason}`, acceptance "assessed at integration" | neutral | dashed circle | `not_run: <reason>` |
| Unavailable content | `pruned`, `redacted`, `truncated`, `partial`, `governed_query_required` | error (pruned), dim (redacted), warning (truncated/partial) | pill | the visibility word + reason |

### 6.4 Budget and allocation meter

A single horizontal track representing `cap` (or a child's `allocation`).
Segments from the left: `settled` solid accent; `active` accent at 45 %
opacity with diagonal hatching; `unresolved` warning with reverse hatching.
`protected` is a neutral segment anchored to the right end so that the visible
gap between the left stack and the protected block is `available`. A legend
appears wherever the meter is used with a numeric line beneath. An `overrun`
flag renders a small error pill beside the cap. The meter never animates toward
a target; it changes only on ledger events.

### 6.5 Stage stepper

A vertical list of recorded transitions, keyed by turn and event identity, with
a rail. Each stage has a monospace name, status and optional evidence line.
Reached stages use success, active, waiting or failure treatments. Repeated visits
remain distinct: `executing_tools` can return to `assembling_context`; only the
recorded `processing_response → verifying → completed` branch implies verification.
Do not draw unreached stages on this rail. Future prerequisites, including child
integration and parent verification, belong in a separate **Next requirements**
list explicitly labelled as not yet recorded. A compact rail may collapse earlier
visits behind a count and Inspect link, never fabricate a fixed linear workflow.

### 6.6 Markers for truncation, redaction and unknowns

- Display truncation uses the CLI's marker text ("… [display truncated; use
  /inspect]") adapted to the editor action ("… [display truncated; use Next
  range]"), in warning italic.
- Redaction renders "content removed, identity retained" with the original
  digest and deletion epoch in dim italic; the identity remains selectable.
- Purged artifacts show identity, digest, length and the prune receipt; never
  an empty box that reads as "nothing happened".
- Client-derived values (elapsed time, countdown) are labelled
  "client-derived" in tooltips and must not appear in exported evidence.

### 6.7 Motion

The only motion is a subtle pulse ring on the active stage and running status
dot; it is disabled under `prefers-reduced-motion`. No indeterminate progress
bars, spinners on cards, or animated meters. Native toast lifetime belongs to VS
Code; every unresolved action remains reachable in its view after a toast closes.

### 6.8 Density and bounds

The side bar is designed for 380–500 px. Cards use 10 px padding, 6 px
vertical rhythm, and truncate identifiers with a middle ellipsis while keeping
the last six characters visible. Lists show at most the engine page size and
expose the next page as an explicit action. Tooltips carry the full sanitized
value within its authorized display budget. Match CLI sanitization using UTF-8
bytes after escaping (objective 1024, reason 256, commentary 2048); the explicit
truncation marker is additional. Preserve valid Unicode and never treat a shortened
ID or display path as an action identity. Responsive rules are in section 13.2.

## 7. Accessibility and internationalization

- Every interactive element is reachable by keyboard in DOM order; approval
  cards expose Allow and Deny as separate buttons with `aria-describedby` on the
  binding line; no control activates on focus.
- Status changes announce through live regions with the state word and the
  affected id ("task-7f3a paused · local hold"), never through colour alone.
- Contrast for text and pills meets 4.5:1 in Dark Modern, Light Modern and the
  high-contrast themes; hatching and glyphs distinguish meter segments without
  hue.
- Text from the engine is rendered with Unicode preserved and bidi controls
  and C0/C1 controls escaped exactly as the CLI's sanitizer does; combining and
  wide characters must not break truncation.
- Times display in the local timezone with the UTC offset shown once per
  view; dates in filters are labelled as UTC boundaries.

## 8. Security and trust constraints on presentation

- The extension host is the only process that talks to the engine. Webviews
  receive bounded presentation records and opaque action ids; the host looks
  up the current decision and attaches operation/revision identity. An
  arbitrary webview payload cannot choose an executable command or grant.
- Webviews run with a strict content security policy: no remote resources,
  no inline scripts, `localResourceRoots` limited to the extension bundle,
  Markdown sanitized, links inert until explicitly opened.
- Nothing from the engine (model output, tool output, child transcripts, MCP
  descriptions, skill text) is treated as instructions; it is rendered as data.
- Persisted client state is limited to connection/session/cursor references.
  Purged or restricted content is removed from any in-memory cache on
  `retention_changed` / `access_changed` and cannot resurface from client
  storage after reload.
- Workspace trust is an input to policy display and gating, not a substitute
  for engine policy; the UI shows the engine's own trust state beside VS Code's.
- Engine executable location comes from user-level configuration only.

## 9. Mock index

All mocks are rendered at 1600 px logical width, 1.5× device scale, Dark
Modern tokens. Full-window pages show the extension inside the workbench;
sheets show one view in several states side by side.

| File | Type | Demonstrates | Requirements |
|---|---|---|---|
| [`01-overview.png`](mocks/01-overview.png) | Full window | Session view during `executing_tools`, an actionable approval, steering input, prepared-change diff, VCP Events panel, status bar | VD-SE-1..6, VD-QA-1, VD-ST-1, VD-CH-2, VD-SB-1..5 |
| [`02-session-states.png`](mocks/02-session-states.png) | Sheet | `waiting_for_input`, `pausing`, `paused` with unresolved effect, `completed`, `failed` (verification), `blocked · budget_exhausted` | VD-SE-7..10 |
| [`03-questions-steering.png`](mocks/03-questions-steering.png) | Sheet | Actionable approval, pending/resolved, stale/expired/superseded, observer role, steering compose → queued → applied, slash completion | VD-QA-1..7, VD-ST-1..4 |
| [`04-agents.png`](mocks/04-agents.png) | Full window | Agents view with parent card and three children (isolated_write with integration conflict, read-only review, blocked explore); Agent focus editor | VD-AG-1..7 |
| [`05-changes-receipts.png`](mocks/05-changes-receipts.png) | Full window | Changes view grouping, stale/conflict banner, buffer vs prepared diff, VCP Receipts panel | VD-CH-1..5 |
| [`06-inspector-chain.png`](mocks/06-inspector-chain.png) | Full window | Inspector tree, ten view tabs, chain strip, item visibility markers, artifact range read, cursor invalidation | VD-IN-1..5 |
| [`07-inspector-costs-routing-policy.png`](mocks/07-inspector-costs-routing-policy.png) | Editor page | Costs (ledger, roles, reservations, uncertain reason), routing decision with exclusions and escalation counters, policy decision order, denials, grants | VD-IN-6..8 |
| [`08-memory-history.png`](mocks/08-memory-history.png) | Full window | Memory search results, retention notice, history rows with visibility flags and gaps, prune preview buckets and actions | VD-ME-1..5 |
| [`09-workspace-connection.png`](mocks/09-workspace-connection.png) | Sheet | Connected/controller, restricted mode, engine unavailable/incompatible/locked, rebind, skills catalog, MCP servers | VD-WS-1..8 |
| [`10-start-resume.png`](mocks/10-start-resume.png) | Full window | New-task form with editor context, continuation quick pick, revalidation preview | VD-SR-1..3 |
| [`11-optimize.png`](mocks/11-optimize.png) | Editor page | Baseline report, interview, prior/requested/effective preview, apply/rollback | VD-OP-1..3 |
| [`12-delegate-cleanup.png`](mocks/12-delegate-cleanup.png) | Sheet | Helper template form, explicit isolated_write spec, cleanup preview/blocked/interrupted | VD-AG-8..9 |
| [`13-statusbar-notifications.png`](mocks/13-statusbar-notifications.png) | Sheet | Four status bar states, the toast catalogue, command palette | VD-SB-1..6, VD-NO-1..3 |

The mocks use one consistent fictional scenario (workspace `ledger-api`, task
`task-7f3a` "Fix the parser failure on malformed CRLF input", model
`openai/gpt-5.6-luna`, cap $2.00, three children) so states can be compared
across pages. Identifiers, amounts and paths are illustrative.

## 10. Traceability to P4 work items

| Work item | Requirements this document supplies | Mocks |
|---|---|---|
| P4-01 Connection and workspace mapping | VD-WS-1..6, VD-SB-1, VD-SB-5, VD-NO-1 (engine/lease toasts), VD-SR-1..2 | 09, 10, 13 |
| P4-02 Task and child views | VD-SE-*, VD-ST-*, VD-QA-*, VD-AG-1..7, VD-P1..P7, section 6, section 8 | 01, 02, 03, 04, 13 |
| P4-03 Versioned document edits | VD-CH-1..5, VD-SR-3 (editor context), VD-NO-1 (invalidated change) | 01, 05, 10 |
| P4-04 Inspectors | VD-IN-1..8, VD-ME-1..5, VD-OP-1..3 | 06, 07, 08, 11 |
| P4-05 Packaging and compatibility | VD-WS-5 (compatibility matrix and engine discovery states), VD-P8 (theme matrix), section 7 | 09, 13 |

Delegation forms and cleanup (VD-AG-8..9, mock 12) sit under P4-02 for
presentation but depend on the P7 controls already qualified in the CLI.

## 11. Remaining qualification decisions for P4

Record these outcomes in the owning task or ADR when P4 starts. Section 13 fixes
the default layout and interaction design; these are qualification gates:

1. **Grant creation from the editor.** The card offers only Allow/Deny. Whether
   a "grant for task/session/workspace" affordance ships in P4-02 depends on
   how `decide` and `set_grant` compose in the P9 protocol.
2. **API and package versions.** Pin supported editor/SDK versions and qualify
   the section 13.1 contributions and native edit concurrency guarantees. Agents
   default to the side bar and Events to the panel; preserve VS Code relocation.
3. **Light theme and high contrast verification.** Tokens are chosen to work
   in all themes but the mocks are rendered only in Dark Modern; P4-02 must
   render the theme matrix.
4. **Client-derived elapsed time.** It is optional secondary text, labelled
   client-derived, never progress or evidence. Qualify clock skew and reconnect
   behavior before including it; otherwise omit it.
5. **Remote hosts.** Every mock assumes the local Windows host. SSH, WSL and
   devcontainer placements are P10-04 and must show an explicit unsupported
   state until qualified.
6. **Public capability coverage.** Complete the section 13.3 P9 binding inventory;
   CLI capability alone does not make a public editor command available.

## 12. Regenerating the mocks

The sources are static HTML pages sharing [`mocks/src/vscode.css`](mocks/src/vscode.css),
which defines every colour as a CSS custom property annotated with its VS Code
theme colour id. `mocks/src/render.ps1` rasterizes each page with a local
Chromium-based browser in headless mode:

```powershell
pwsh docs/design/mocks/src/render.ps1 -Src docs/design/mocks/src -Out docs/design/mocks -Scale 1.5
```

Each page declares its logical size in `<meta name="mock-size">`. Without `-Only`,
the script renders **every** HTML page and overwrites matching PNGs, including drafts.
Use `-Only` to render the intended canonical basenames, for example:

```powershell
& ./docs/design/mocks/src/render.ps1 -Src docs/design/mocks/src -Out docs/design/mocks -Only @('01-overview', '02-session-states') -Scale 1.5
```

The original superseded PNGs and matching HTML sources have a `-draft` suffix;
approved corrections use canonical names. Do not regenerate drafts during ordinary
design updates. No package manager or network access is required. These static HTML
files are design assets, not extension implementation.

## 13. Implementation specification for P4

This is the construction contract for future P4 work, not evidence that any client
exists. Preserve the mocks' restrained Dark Modern surfaces, spacing, thin borders,
compact cards, semantic pills and typography. Approved canonical images correct
the errors listed in section 14; neither image set overrides engine authority.

### 13.1 Workbench contributions and module ownership

**VD-IM-1** Implement under `src/packages/vscode/` in the modules named by
[P4](../plan/18-deferred-vscode.md). `engine_connection` owns SDK launch/attach,
negotiation, subscriptions and recovery; `workspace_map` owns URI/host identity;
`trust` owns editor trust observations; `document_context` owns versioned editor
observations; `edit_bridge` owns apply/reconcile; `commands` owns action validation;
`diagnostics` owns sanitized client diagnostics. Presentation modules only render
bounded host projections. No webview imports transport, filesystem or provider code.

**VD-IM-2** Use the following stable proposed contribution IDs. IDs are local
extension identifiers, not proposed RPC methods. Register all disposables with the
extension lifecycle; opening another view must not spawn another engine or writer.

| Contribution | Proposed ID / API | Implementation instruction |
|---|---|---|
| Activity container | `vcp`, `contributes.viewsContainers.activitybar` | Monochrome icon; VS Code owns chrome, moving and collapsing views. |
| Card/form views | `vcp.session`, `vcp.questions`, `vcp.steer`, `vcp.agents`, `vcp.memory`, `vcp.retention`, `vcp.workspace`; `WebviewViewProvider` | Preserve card layouts in section 4; do not attempt arbitrary HTML inside a `TreeItem`. |
| Navigation trees | `vcp.changes`, `vcp.inspector`; `TreeDataProvider` + `createTreeView` | Stable scoped IDs, native selection, expansion, inline actions and context menus. |
| Detail pages | `vcp.agent`, `vcp.inspect`, `vcp.history`, `vcp.optimize`, `vcp.changeReview`; `WebviewPanel` | Reveal existing page for the same scoped identity; opening evidence never changes the active task implicitly. |
| Bottom container | `vcp.panel` with `vcp.events`, `vcp.receipts` webview views | Filters, bounded tables, evidence links; permit user relocation. Mock tab placement among built-in tabs is illustrative. |
| Engine diagnostics | `LogOutputChannel` named `VCP Engine` | Sanitized transport/lifecycle errors; no raw request bodies, credentials or retained transcript. |
| Diff | `vscode.diff` + read-only virtual documents | Snapshot labels include representation/version; companion review contains binding, warnings and actions. Do not inject HTML into the native diff editor. |
| Resume chooser | `createQuickPick` | A short label and detail per candidate; Enter opens revalidation details. Long cards in mock 10 expand in the detail page. |
| Status | `createStatusBarItem` | Stable IDs, accessible names, theme colours, commands to reveal corresponding view. |

The API division follows the official [Tree View guide](https://code.visualstudio.com/api/extension-guides/tree-view)
and [VS Code API reference](https://code.visualstudio.com/api/references/vscode-api).
Qualify against the actual version selected in P4-05. A mock's bespoke diff banner,
card, toast or tab arrangement is not evidence that a native API can draw it.

**VD-IM-3** Default to Session, Questions and Steer expanded; Agents and Changes
remain available below them. Keep supporting Inspector, Memory, Retention and
Workspace collapsed until opened. Never expand a view or move keyboard focus on
an incoming event. Show waiting/failed child counts in the visible parent summary
even when that child's card is offscreen or on another page. Persist only harmless
view preferences and the recovery references permitted in section 8, never contents.

### 13.2 Layout, density and reusable components

**VD-LY-1** Retain 10 px card padding and 6 px rhythm; use theme font variables in
webviews (`--vscode-font-family`, `--vscode-font-size`, `--vscode-editor-font-family`).
Map theme IDs to `--vscode-*` colour variables, including button foreground/hover,
input validation, focus, selection and contrast borders. Do not copy the static
mock CSS's fixed body width/height, hex palette or `overflow:hidden` into the client.
Visible scrollbars, wrapping and keyboard scrolling are required.

**VD-LY-2** At 380–500 px sidebar width use the illustrated layout. At 280–379 px
stack key/value rows and action groups; move secondary actions to an accessible
More menu, retaining the primary action and blocker reason. Below 280 px continue
to wrap; offer Open details rather than silently crop decisions. Main editor pages
use one column below 720 px, two at 720–1199 px, and the illustrated three columns
only at 1200 px or wider. These are CSS pixels after workbench zoom. Only code/diff
and genuinely wide tables may scroll horizontally inside a labelled region.

**VD-LY-3** Shared presentation primitives are: scoped identity header, state badge,
key/value list, evidence link, budget meter with textual amounts, recorded stage
rail, question card, validation message, notice, paged list and action group.
Every state has text and a glyph. Never break an enum into one-letter fragments;
wrap a long value as text outside its pill or show a short label with accessible
full value. Keep unknown costs visible when collapsed. Tooltips supplement, not
replace, essential scope, expiry or disabled-reason text.

**VD-LY-4** Status priority is connection/trust failure, actionable questions or
unknown effect, current task/hold, then budget. Low-priority details (engine version,
host, index generation, child counts) may move into the Session header/tooltip in
compact layouts; all remain reachable by keyboard. Use only supported
`statusBarItem.warningBackground` / `statusBarItem.errorBackground` backgrounds;
the mock's blue normal VCP tile is illustrative. Follow the
[status-bar API](https://code.visualstudio.com/api/references/vscode-api#StatusBarItem)
and [status-bar guidance](https://code.visualstudio.com/api/ux-guidelines/status-bar).
No root's exhausted budget may be paired with another task's completion without
explicit attribution. Counts of paused tasks never become question badges.

### 13.3 Command registry and availability

**VD-AC-1** Build one host-side registry used by palette, title buttons, menus and
webviews. Each entry declares label, local command ID, target scope, query/mutation
kind, required negotiated capability, controller requirement, expected revisions,
idempotency/reconciliation strategy and disabled reason. Register `vcp.*` command
IDs; use `when` and `enablement` for presentation only. Repeat validation inside
the handler because commands can be invoked without clicking a visible control.

| Action family | Contract status / implementation gate |
|---|---|
| Workspace open, session read/create/resume, task read/cancel, turn start/steer/pause/cancel, approval respond | Method design exists in architecture 5.3; bind only to the generated, negotiated P9 SDK. |
| Context, routing, usage, memory queries; artifact/diff read; editor context/result | Minimum methods exist; P9 must supply exact schemas, authorization and error mapping before P4 binds them. |
| Agents/delegate/integrate/cleanup; history/prune/retention; Optimize; skills/MCP control | Internal/CLI capabilities exist, but the minimum method table is not a public SDK contract for them. Track missing bindings under P9 before enabling. |
| Controller transfer, grant management, raise cap, resolve liability, rebind, backup/doctor | Require explicit qualified public binding and authorization semantics; do not infer one from a mock button or shell out to CLI. |
| Reveal view, select task for inspection, copy ID, show diff, edit form, open settings/output | Local actions; no engine mutation and no fabricated durable receipt. Reads still require authorization. |

**VD-AC-2** Hide unsupported optional features from normal command suggestions;
show their capability-unavailable explanation when reached through a saved link
or relevant detail page. Known temporarily unavailable actions stay disabled with
an inline explanation. Observers may inspect authorized state but not answer,
steer, apply, delegate or resume. Lease-transfer controls appear only when the
negotiated API allows a deliberate request; no automatic takeover.

**VD-AC-3** Never interpret a webview string as a command name, executable, file
path or grant. Send an opaque host-issued action ID with only that action's typed
form fields. Bind it to scope, view generation, operation and revisions; invalidate
on root, lease, policy, steering, trust or access changes. The host resolves current
authority and the engine remains final arbiter. Use exhaustive message validation,
field/byte limits and allowlisted variants; reject unknown fields and action IDs.

**VD-AC-4** A submitted mutation follows this client display lifecycle (these are
not engine enums): Ready → Submitting → Accepted, awaiting durable result →
Reconciled. A lost reply becomes **Outcome not yet known**, not Failed. Disable
duplicate submissions before awaiting transport; show the original command ID.
On timeout/reload query that identity. Reuse a key only for the identical payload
under the SDK retry contract; never create a new key just to escape uncertainty.
Conflicts invalidate the preview/action and require a fresh deliberate submission.
Independent Pause/Cancel remain reachable while a slow query or unrelated command
is pending, subject to their actual engine admission rules.

### 13.4 Connection, workspace and subscription lifecycle

**VD-CN-1** Render separate connection states: not configured, connecting,
negotiating, synchronizing, connected, disconnected, incompatible and unsupported
host. A textual “Connecting…” is client activity, not task progress. Restricted
trust and controller/observer are independent attributes. On activation with no
folder show Open folder; with no engine show Choose engine executable in user
settings; with no tasks show Run task and Resume unfinished task. Never auto-run
an objective, download an engine or trust a workspace to make an empty state go away.

**VD-CN-2** Key bindings by folder URI plus execution host, retaining engine
workspace/root IDs and mapping revision. Resolve nested roots through the map;
ambiguous mappings stop context/apply with a root chooser. Display path and host
for same-named folders. Rename, delete, link changes, rebind and host changes
invalidate prepared actions and document observations. No folder display name,
string prefix or local-path normalization authorizes access to another root/host.
Inspecting another task does not retarget the active composer.

**VD-CN-3** Negotiate an authorized snapshot and its sequence boundary S with P9;
subscribe/replay after S using the SDK's race-free handoff. Deduplicate by scoped
event identity/sequence, reject cross-session records, and ignore older revisions.
Unknown governance/state variants fail closed with Incompatible event schema;
do not map an unknown state to success. Keep transport state distinct from the
last durable task state and display “Last known at <time/sequence>” after disconnect.

**VD-CN-4** A gap, retention invalidation, subscriber overflow or failed handoff
shows **Resynchronizing — actions unavailable**. Invalidate affected action IDs,
discard inconsistent pages and acquire a fresh authorized snapshot; preserve a
visible retained-gap notice in history. Never append incompatible snapshots into
one list. Coalesce ordinary render updates, not durable evidence. Render only
bounded pages and keep waiting/failed child summaries independent of noisy output.
Pause/cancel routing must not wait for the event table to finish rendering.

**VD-CN-5** Dispose hidden webviews and reconstruct them from the host projection;
do not rely on `retainContextWhenHidden` for correctness. Persist only scoped
connection/session/cursor/command references needed to recover plus harmless view
preferences. Draft objectives, guidance, buffer text and artifact bytes stay in
memory; state plainly that reload discards an unsent draft. Controller owner loss
uses P9's close-to-pause contract. Hiding a view or closing an inspector is not
owner loss; observer disconnect does not pause another client's task.

### 13.5 Forms and deliberate task interaction

**VD-FM-1** Start form field order is objective, budget, cost profile, autonomy,
skills, editor context, Run. Use persistent labels and inline errors. Parse USD as
decimal integer micros, reject exponent notation/non-finite/negative values and
more than six fractional digits; enforce the engine's range and required positive
cap. Do not silently round. Submission validates all fields, focuses the first
invalid field and creates one command identity only after validation succeeds.
Engine rejections preserve the in-memory draft and explain the affected field.

**VD-FM-2** Use separate Select for review, Reconcile and Resume interactions.
Quick-pick selection/Enter never dispatches or acquires a controller lease. The
detail page lists current revalidation outcomes; unresolved effects disable Resume.
After reconciliation, refresh the preview and require an explicit Resume against
the refreshed revision. Escape/cancel dismisses a form without changing task state.
Do not combine answering, steering or reconciliation with an implicit Resume.

**VD-FM-3** Steer has a labelled multiline input, UTF-8 byte budget and visible
Send guidance button. Enter submits when no IME composition or completion selection
is active; Shift+Enter inserts a newline. Choosing a slash completion inserts text,
not dispatch. Parse only supported commands; plain text remains guidance. A queued
submission becomes read-only until reconciled, with an explicit pending state.
Applied guidance shows its steering revision and a separate Resume when eligible.

**VD-FM-4** Questions sort actionable first then by engine order/identity; resolved
history is collapsed. Label task/child, operation, scope, revisions and expiry before
Allow/Deny. Neither receives automatic focus or a default dialog action. An expiry
countdown is advisory/client-derived; engine validation determines staleness.
Reaching zero disables the stale affordance and refreshes validity; it never denies
or answers. Show engine-issued grant identity only if returned by the decision,
never manufacture a grant from a successful button click.

**VD-FM-5** Delegation fields obey the negotiated schema and existing helpers'
scope limitations; exact paths must not be drawn as globs unless the engine accepts
them. Allocation is a subdivision of the root cap, never additional spend. Present
root ledger and child allocations in separate tracks; a child usage meter includes
known, reserved and uncertain amounts. Cleanup/prune/rebind/Optimize mutations show
their exact preview, affected scope and revision. Editing any field invalidates
the old preview. An interrupted cleanup is reconciled against its original identity,
not retried against whatever now occupies that pathname.

### 13.6 Prepared changes and editor context

**VD-ED-1** Capture context only for the explicitly selected workspace/task.
Show URI, host/root identity, language, document version/hash, dirty state, ranges,
disk fingerprint, and diagnostics' producer/time/document revision. Use VS Code's
position conventions with SDK normalization at one boundary. Label stale diagnostics;
do not equate them to current verification. Offer explicit inclusion/removal of
buffer content and separately explicit durable capture; neither selecting a tab
nor showing a context summary grants permission to retain unsaved text as memory.

**VD-ED-2** Review immutable original/prepared snapshots in the native diff.
The companion review shows change-set/effect identity, expected versions/hashes,
disk fingerprint, policy/steering revisions, target representation and receipts.
Editor title commands can reveal Apply/Re-prepare/Discard in that review; none
may attach to an unrelated editor because only a filename matched. Show my edits
opens original-vs-current separately and never mutates the prepared snapshot.

**VD-ED-3** Before apply, recheck mapping, live document versions/hashes, disk,
trust and current operation authority. Engine intent must be durable before the
editor effect. Qualify the real API race window: a preflight read followed by
`workspace.applyEdit` is not proof of compare-and-swap. If safe version binding
cannot be established, use the P4-03 refreshed review or explicit save/retry
boundary; never overwrite newer typing. The UI must state the qualified limitation.

**VD-ED-4** Track each resource independently as prepared, pending, applied,
conflict, not applied or outcome unknown using the actual receipt. Buffer and disk
observations remain distinct even when grouped in one file row. Unsaved application
does not mean saved. A partial batch does not turn green because one file succeeded.
Crash after apply/before acknowledgement queries receipts and observes current
buffers before deciding anything; never replay an insertion on timeout.

**VD-ED-5** Save, undo, redo, close, rename, delete and rebind invalidate relevant
observations, prepared actions and verification coverage. Applied receipt history
remains evidence of what happened; current state may have changed since. Undoing a
user action or “rollback” requires a newly prepared compensation against current
versions. Never perform unconditional undo over later edits. Verification labels
show disk/buffer/both and the exact fingerprint; historical passes remain stale.

### 13.7 Evidence, retention and rendering security

**VD-EV-1** Every paged surface distinguishes Loading, Empty, Unavailable,
Unauthorized, Pruned, Partial, Error and Disconnected. “No results” requires a
successful empty query, never a timeout or index-not-ready response. Keep query,
scope, watermark and page/range visible. Show bounded Next/Previous where supported;
do not invent total page counts or random access when only a cursor exists. Cancel
superseded read requests and reject late responses from an earlier scope/generation.

**VD-EV-2** Artifact ranges are byte offsets. Decode split UTF-8 sequences safely
without changing server offsets; binary/undecodable data is labelled accordingly.
Read-only virtual documents are subject to the same authorization and invalidation
as webviews. A raw view must not fetch an unbounded artifact or persist it to disk
implicitly. Explicit copying/export is a deliberate authorized action with its
destination disclosed; cloud export remains the engine's encrypted publisher.

**VD-EV-3** On access/retention change, invalidate action IDs, snippets, page/range
caches, virtual-document contents and mounted DOM before accepting more content.
Fence in-flight responses by access/retention generation so old text cannot reappear
after invalidation. Webview reload reacquires authorization; `getState`/`setState`
must not contain engine payloads. Show tombstone identity/digest/reason when permitted.
Do not export client countdowns or estimated elapsed time as engine evidence.

**VD-EV-4** CSP starts with `default-src 'none'`; allow only bundled resources
through `asWebviewUri`, scoped `localResourceRoots`, and nonce/hash-authorized script
files. No remote scripts/fonts, `eval`, inline event handlers, or page network
access. Render plaintext by default; sanitize any supported Markdown. Keep
`MarkdownString.isTrusted` false for external text. Reject `command:`, `javascript:`,
unapproved `file:`/custom URI schemes and traversal; file links resolve through the
host workspace map and external links use explicit user activation. Sanitize errors
and output before logs as well as before display.

### 13.8 Accessibility and notification behavior

**VD-AX-1** Use semantic headings, lists, tables and actual buttons/inputs in the
client (the static mock spans are illustrations). Tab follows visual order; Enter
or Space activates a focused button, Escape closes transient UI and returns focus
to its invoker. Tabs use arrow-key navigation with labelled tab panels; completion
uses combobox/listbox semantics. No keyboard trap inside nested scroll regions.
Provide focus-visible outlines in every theme; disabled reasons stay readable.

**VD-AX-2** Preserve focus by stable record/action identity on event updates. If a
focused question disappears, move to its result heading or Questions heading and
announce the result. Never focus Allow, Run or Resume because a new card arrived.
Batch polite live-region announcements of task/approval state, not every token,
cost tick or countdown. Announce errors once with their scoped identity. Accessible
meter text lists amounts and uncertainty; it is not a task-percent progress bar.

**VD-AX-3** Test keyboard-only navigation, screen reader output, reduced motion,
200% zoom, long localized strings, Unicode paths and both high-contrast themes.
Localize extension-authored labels/errors and plural forms; preserve engine enum,
ID and evidence text verbatim after safe escaping. UTC filter dates mean midnight
UTC unless an explicit timestamp says otherwise; show local display timezone and
offset independently. Zero is a known amount; absent/unknown is never zero.

**VD-AX-4** Deduplicate notifications by scope, event identity and notice kind.
Do not replay old toasts during snapshot recovery. Actionable state remains in the
owning view after dismissal, and notice counts link there. Workspace trust UI and
engine trust are separate deliberate flows; dismissing either grants nothing.
Native notifications are not persistent approval forms. No completion toast is
needed by default; if provided it requires committed engine completion evidence.

### 13.9 Delivery sequence and acceptance evidence

New IDs below supplement section 10. Each future PR must link its fixture, actual
result and not-run environments; mock screenshots prove appearance only.

| Work item / gate | Required implementation and tests |
|---|---|
| P9 prerequisite | Inventory every section 13.3 binding; generate SDK schemas; qualify command lookup/idempotency, snapshot handoff, lease/access changes and capability errors. Missing bindings remain explicit prerequisites. |
| P4-01 / VD-IM, VD-CN | Test clean first run, missing/incompatible engine, nested/same-name/moved roots, unsupported host, trust revocation while queued, controller and observer reload, lease loss and gap resync using real engine/SDK. Assert no auto-resume or second writer. |
| P4-02 / VD-LY, VD-AC, VD-FM, VD-AX | Replay the same synthetic trace through CLI and editor; compare task/turn/child attribution, cost certainty, questions and holds. Test duplicate click, stale revision, lost reply, hostile/unknown action ID, scope switch, repeated stage visits, IME/completion and a noisy child beside a waiting child. |
| P4-03 / VD-ED | Real VS Code tests: dirty buffer differs from disk; typing during review and apply; CRLF/encoding; rename/delete/multi-root; crash after one file; lost receipt; save/undo/redo before reconcile. Independently compare editor/disk observations to per-file receipts and stale verification. |
| P4-04 / VD-EV | CLI-equivalent query fixtures, multibyte range boundaries, large/binary output, empty vs unavailable, cursor invalidation, prune/access revocation during an in-flight read and webview reload. Assert no removed text in DOM, snippets, virtual documents or persisted state; stale optimizer/prune previews cannot apply. |
| P4-05 / all | Packaged extension on the pinned local Windows/VS Code matrix, without checkout/development PATH. Test install, engine discovery/replacement, reload, incompatible schema, upgrade failure and uninstall preserving user data. Record theme/zoom/keyboard/screen-reader matrix and actual editor race results. |

For visual review, capture 280/380/500 px sidebars and 640/960/1280 px detail pages
in Dark Modern, Light Modern and both high-contrast themes; repeat critical forms
at 200% zoom. Assert no clipped primary actions, blocker messages or focus rings.
Check budget geometry against integer-micro totals: available is
`cap − settled − active − unresolved − protected`; child allocations are not added
again to spend. If cap is zero or totals overrun it, show exact values and a labelled
overrun/empty track without division by zero, negative segment widths or hidden debt.
No P4 item is marked accepted by this documentation change.

## 14. Approved mock corrections and draft comparison

All thirteen PNGs were reviewed. The user approved the corrections and requested
that approved versions use the canonical filenames, with superseded originals
renamed `-draft` before the extension. Matching HTML sources follow the same
convention. Original draft bytes and the shared stylesheet are preserved. Mock 03
needed no correction and has no duplicate draft. Engine/model/version numbers,
paths, dimensions and data remain illustrative, not a compatibility claim.

| Approved mock | Original draft | Correction |
|---|---|---|
| [01 overview](mocks/01-overview.png) | [draft](mocks/01-overview-draft.png) | Recorded-only stages, explicit processing_response, active execution separate from check outcomes; remove queued guidance mislabelled as an applied objective event. |
| [02 session states](mocks/02-session-states.png) | [draft](mocks/02-session-states-draft.png) | Pause progress separate from task state; no future execution node; Resume disabled with unresolved effect; terminal failure offers New task. |
| [04 agents](mocks/04-agents.png) | [draft](mocks/04-agents-draft.png) | Root ledger distinct from allocation limits; uncertain cost visible; future requirements off recorded rail; exact write path; blocked child and controls no longer clipped. |
| [05 changes/receipts](mocks/05-changes-receipts.png) | [draft](mocks/05-changes-receipts-draft.png) | Distinct buffer and disk receipts; remove unexplained question badge. Native diff banner placement remains conceptual; use section 13.1 companion review. |
| [06 inspector chain](mocks/06-inspector-chain.png) | [draft](mocks/06-inspector-chain-draft.png) | Paging and cursor invalidation notice visible below long evidence rows. |
| [07 costs/routing/policy](mocks/07-inspector-costs-routing-policy.png) | [draft](mocks/07-inspector-costs-routing-policy-draft.png) | Wrapped identifiers and enum pills no longer overlap; bottom explanatory content visible. |
| [08 memory/history](mocks/08-memory-history.png) | [draft](mocks/08-memory-history-draft.png) | UTC date boundary is midnight UTC; paused recovery dependencies remain protected rather than suggesting another pause makes purge eligible. |
| [09 workspace/connection](mocks/09-workspace-connection.png) | [draft](mocks/09-workspace-connection-draft.png) | Full cards visible; compatibility versions explicitly illustrative. |
| [10 start/resume](mocks/10-start-resume.png) | [draft](mocks/10-start-resume-draft.png) | Select for review; reconciliation and Resume separate; no lease acquisition by selection; no question badge for paused-task count. |
| [11 Optimize](mocks/11-optimize.png) | [draft](mocks/11-optimize-draft.png) | Unsupported numerical predictions replaced by explicit unavailable forecast; observed correlation remains evidence. |
| [12 delegation/cleanup](mocks/12-delegate-cleanup.png) | [draft](mocks/12-delegate-cleanup-draft.png) | Pending helper submit disabled; exact write path; allocation not portrayed as a charge; retained-result details readable. |
| [13 status/notifications](mocks/13-statusbar-notifications.png) | [draft](mocks/13-statusbar-notifications-draft.png) | Remove ambiguous completed/exhausted pairing; budget arithmetic consistent; navigation distinguished from engine mutations. |

Larger corrected canvases expose previously clipped material without changing the
visual language. Runtime views must scroll/reflow as specified in section 13.2;
these expanded static captures do not establish a minimum window height. The mock
sources still simulate workbench chrome and contain placeholder glyphs; implement
native chrome, accessible controls and actual Codicons rather than copying that HTML
as a runnable extension. No extension code is included in this increment.
