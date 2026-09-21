# 22 — Cursor-derived improvements and integration sequence

Status: proposed; adoption pending. Nothing in this supplement is implemented, qualified or enabled.
It proposes useful directions from [the Cursor research note](../research/cursor.md)
into existing work, following the precedent of the
[Markov supplement](21-markov-integration.md). It creates no new product task IDs,
changes no architecture dependency, adds nothing to the P8-05 first-release closure
and enables no runtime feature. The [owning task sections](20-traceability.md#work-item-ownership-and-readiness)
remain the implementation entry points; the increments below propose their added
work, refactors, order and acceptance evidence. Historical P2/P3/P5/P6/P7
acceptance remains valid for its original scope; it does not cover these planned
extensions, and no completed ledger row reverts because an extension is pending.

The research note is input, not authority. Cursor is closed source, so every item
is pattern adoption: no Cursor code, prompt, documentation text or default list
enters the repository. [Architecture draft 0.4](../architecture/vcp-what.md),
the ADR register and owner decisions take precedence wherever the note disagrees.

## Scope and selection

The note proposes sixteen increments (`CR-01`…`CR-16`). This plan keeps those
labels so an implementer can cross-reference the note's source keys; a suffix
(`CR-02a`, `CR-02b`) splits an increment whose parts have different risk. Labels
are checklist identifiers within existing tasks. They are not architecture work
items, the C01–C07 Codex components or the R01–R08 reuse suites.

Select an increment only when it meets all four conditions: it closes a gap that
repository evidence confirms; it fits an existing owner, contract and trust
boundary; it can ship disabled or additive so accepted behavior is unchanged; and
its benefit is measurable with the existing harness. Prefer deterministic local
mechanisms. P6 qualification ended in [recorded rejection](../evaluations/p6-completion.md)
of optional advisory classifiers for insufficient held-out evidence, so this plan
selects no increment whose value depends on a new unqualified model judgment.

| Increment | Disposition | Reason |
|---|---|---|
| CR-08 Worktree provisioning and bounded cleanup | Selected, wave A | P7-04 materializes isolated roots but provisions nothing and never reclaims them; child checks in a fresh root are otherwise mostly not-run |
| CR-03 Built-in helper roles | Selected, wave A | `ChildSpec.role` and `ChildMode::ReadOnly` already exist; a versioned catalog adds routing defaults without new authority |
| CR-02a Bounded regex search | Selected, wave B | `vcp_search` is literal-only; small additive tool change with a full-scan oracle |
| CR-13 History recall tool and MCP state in context | Selected, wave B | Recovers detail lost to compaction from already retained artifacts; no new store |
| CR-10a Deterministic protected-path, deletion and outside-root guards | Selected, wave B, verify first | Restrictive-only policy defaults; no classifier involved |
| CR-04 Agent-change rewind | Selected, wave C | Highest safety value; derived from the existing effect journal rather than a second snapshot store |
| CR-05 Plan as a revision-bound artifact | Selected, wave C | `Autonomy::Plan` exists but no persisted, editable plan; pairs with CR-04 |
| CR-11a Durable input queue | Selected, wave C | Bare terminal input is steering only; an explicit next-turn queue is small and recoverable |
| CR-12a Local review with patch identity and incremental scope | Selected, wave D | Identical-patch reuse removes paid re-review, including for P7-05 integration |
| CR-09 Scoped instruction fragments (descriptor option only) | Selected, wave D | Optional skill-descriptor fields; absent fields keep P7-01 behavior |
| CR-06 Evidence-first debug workflow | Selected, wave D, after CR-04 | Tagged instrumentation and a completion check that binds only tagged changes |
| CR-07 Best-of-N across model groups | Selected, wave E | Generates the matched, verified, exactly charged evidence P6-04 lacked |
| CR-01 Whole-workspace local code index | Selected, wave E, comparison-gated | Largest addition; must beat lexical and M5 map arms on the frozen task set |
| CR-02b Indexed regex candidates | Evidence-gated | Build only if measured search latency justifies an index |
| CR-12b Learned review rules in governed memory | Evidence-gated | Needs accepted/rejected finding history from CR-12a first |
| CR-14 Retrieval relevance fitting | Evidence-gated, after CR-01 | Same sparse-evidence limits that rejected M4 apply |
| CR-15 Post-edit diagnostics | Folded into [M6](21-markov-integration.md#m6--verification-order) | A fast diagnostic is a cheap check class inside verification ordering, not a new mechanism; its source is low confidence |
| CR-10b Advisory approval triage | Deferred, owner decision | Conflicts with I-01 unless restricted to non-objection; no classifier purpose is currently qualified |
| CR-11b Side sessions, CR-11c long-lived goals | Deferred | Side sessions await P7-06 read-only children; bounded objectives already exist through the completion contract, root cap and bounded escalation |
| CR-16 Agent Client Protocol | Deferred to P9-01 | Evaluate as one public surface when the public protocol starts; amend ADR-002 then |
| Image reads, web fetch, fast-apply role | Not selected | Optional tool families in architecture 9.1; no measured need. A sketch-apply role conflicts with exact unambiguous patch matching |
| Hidden model choice, server-side index, hosted classifier, cloud agents, team rules, marketplace, tab completion, browser tools | Declined | Conflict with confirmed transparency, local-compute, no-hosted-VCP and CLI-first decisions |

## Dependency order

No increment blocks P7-04…P7-06 or P8. Placement states the earliest start;
it does not make the increment a release prerequisite. An increment that lands
before P8 is included in the P8 campaigns like any other enabled consumer; one
that lands after P8-05 runs its own regression against the released baseline.

| Order / increment | Owning work | Prerequisites and placement | Deliverable and gate |
|---|---|---|---|
| CR-08 — Worktree provisioning and cleanup | P7-04; P2-03/04 supporting | P7-04 connected native qualification complete. May ride with P7 only under a recorded owner note; otherwise first follow-up after P7-06 | Granted, bounded setup recipes; explicit copy list; cleanup that respects live references |
| CR-03 — Helper role catalog | P7-04/06, P6-02 | P7-04 complete; `/agents` display needs P7-06. Same owner-note rule as CR-08 | Versioned `explore`, `shell-runner` and `verifier` templates with routing defaults and result packets |
| CR-02a — Regex search | P2-04 | Current P2 baseline; start after the in-flight P7 branch merges because both touch `vcp-tools` | Additive `mode`; literal default byte-identical; full-scan differential oracle |
| CR-13 — History recall and MCP state | P2-08, P7-03 | Current compaction and MCP baselines | Bounded own-session recall tool; static MCP state part; token measurements |
| CR-10a — Deterministic guards | P2-03 | Verify first; current policy baseline | Protected paths, deletion and outside-root guards as a new policy revision, or a recorded "already covered" result |
| CR-04 — Rewind | P2-04, P3-01/02; P5-07/P5-09 supporting | Current effect journal; before-image retention verified or added first | Preview/apply rewind as prepared changes; conflicts never overwrite human edits |
| CR-05 — Plan artifact | P3-02, P2-05, P2-01 | Current CLI; recovery affordance needs CR-04 | Persisted plan, hash sealed in the context manifest, zero effects under plan autonomy |
| CR-11a — Input queue | P3-02, P2-05 | Current terminal and steering revisions | Durable next-turn queue distinct from steering; survives pause and forced kill |
| CR-12a — Local review | P7-05, P7-02 | P7-05 complete; read-only role from CR-03 when available | `vcp review`, patch-identity cache, incremental scope, nested review rule files |
| CR-09 — Scoped fragments | P7-01, P2-01 | Current skill discovery baseline | Optional `activation` and `attach_globs` descriptor fields; manifest attribution |
| CR-06 — Debug workflow | P7-02, P2-06 | CR-04; `review-debug` catalog entry qualified | Tagged instrumentation, removal check at completion, local event file |
| CR-07 — Best-of-N | P7-04/05, P6-05, P6-02 | P7-06 complete and CR-08; explicit root cap | N isolated children from one base, comparison record, cohort-labelled evidence |
| CR-01 — Workspace code index | P5-03…06, P2-01; P5-07/09/10 supporting | M5 comparison arms defined; owner-approved resource envelope | Separate record kind and generation family, Merkle change detection, labelled readiness |
| CR-02b, CR-12b, CR-14 | P2-04/P5-03; P5-01/P7-05; P5-06 | Their parent increment plus a recorded measurement that justifies them | Built, rejected or deferred with evidence |

Waves A–D are independent of one another except where the table names a
prerequisite. CR-01 and [M5](21-markov-integration.md#m5--context-co-change-retrieval-and-compaction)
both change optional context candidates: define them as arms of one frozen
comparison so neither invalidates the other's evidence midway, and freeze the
selected context configuration before any later P6-04 rerun. No paid evaluation
or trial budget is created here.

## Evidence corrections to the research note

The note states that its "VCP today" claims were not source-audited. Checks
against the working tree at plan time (baseline `b7742e5` plus the in-flight P7
branch) found the following. Re-run each increment's **verify first** step at
implementation; this list is a snapshot, not a substitute.

| Note claim | Observed | Consequence |
|---|---|---|
| `vcp_search` is literal-only | Confirmed in `vcp-tools::schema` and `vcp-tools::read::search`; results already report `complete` and `exclusions` | CR-02a extends that function and keeps its disclosure contract |
| No rewind, Merkle, syntactic chunking or n-gram index | Confirmed: no matches in `src/crates` | CR-04, CR-01, CR-02b are genuine gaps |
| Helper roles need a catalog | `ChildSpec.role: String` and `ChildMode::ReadOnly` exist in `vcp-domain::agents`; read-only helpers already cannot patch or execute | CR-03 adds named templates over existing seams, not a new authority model |
| Worktree provisioning and reclamation missing | Confirmed by the [P7-04 guide](../development/p7-child-workspaces.md): worktrees use no checkout filters or hooks, and the APIs never delete worktrees | CR-08 is a gap. The note's stronger claim is wrong: P7-05 acceptance judges the **integrated parent**, where dependencies exist, so missing provisioning reduces child usefulness but does not make P7-05 evidence misleading |
| `/plan` exists as a control | `Autonomy::Plan` exists in `vcp-cli::args`; the terminal parser has no `/plan`, `/review`, `/rewind` or `/index` command | CR-05 adds the command as well as the artifact |
| Steering versus queue | Any non-command line parses to `Input::Steer` | CR-11a adds a distinct, explicit queue input |
| `src/skills/builtin/review-debug/` | The packaged directory contains `SKILL.md` and `skill.json`; the catalog references that descriptor | CR-06 and CR-12a extend the existing packaged skill under ADR-025 |
| Default protected paths absent | `vcp-tools` already rejects any `.git` path component; child-workspace and snapshot code add their own metadata checks | CR-10a must inspect existing tool and policy guards before identifying any remaining gap |
| Before-images retained | A prepared `Change` carries `before_bytes` in `vcp-tools::patch`; durable retention as an artifact was not confirmed | CR-04 starts by proving or adding before-image retention |
| Next free ADR is 042 | Confirmed: the register ends at ADR-041 | Take the next free number at authoring time regardless |

## Preservation contract for accepted behavior

Every increment must satisfy these rules. They are the concrete meaning of
"improve VCP without negatively impacting implemented features."

1. **Additive, versioned schemas.** New fields are optional. Absent fields select
   the legacy path and retain legacy digests, as P6-05's optional policy fields
   already do. Readers accept older records through a tested absent-feature path;
   unknown schemas fail visibly. Never rewrite a historical record to acquire a
   new field.
2. **Disabled parity.** Each increment ships behind an explicit configuration or
   command and needs a fixture proving that, when disabled or unused, the
   request count, effect count, canonical event sequence and serialized model
   request for an existing acceptance trace are unchanged.
3. **Model-visible catalog changes are context changes.** A changed tool schema
   alters the schema digest, invalidates prepared calls and stale approvals by
   design, and changes token counts. Land it as a new schema revision, rerun the
   owning tool fixtures and retained loop traces, and record which frozen
   P5-08/P6-04 comparisons bind the older catalog revision. Those results stay
   valid for their recorded revision and are rerun only before reuse in a new
   decision, per [evidence invalidation](16-test-fixtures-and-acceptance.md#reproducibility-and-evidence-invalidation).
4. **No new authority path.** Every new tool declares effect class, capabilities,
   limits, timeout, cancellation, idempotency class and artifact types and
   dispatches through the [9.2 sequence](../architecture/vcp-what.md#92-tool-dispatch-sequence).
   Repository files, fragments, plans and recipes are attributed data; none can
   grant authority (I-01). New policy defaults may only restrict.
5. **One controller, store, ledger and scheduler.** Index builds, setup commands,
   cleanup, review and best-of-N children are owned work under the existing
   admission fence. `/pause` and owner loss stop them; nothing resumes on a status
   read. Every model-assisted step reserves at the root before send (I-04, I-05).
6. **Derived state is rebuildable and labelled.** Indexes, caches, review caches
   and checkpoint projections are never canonical. Staleness and partial
   readiness are explicit (I-07, I-14). Both stores behave identically (I-13).
7. **Retention and portability are declared up front.** Any new retained payload
   names its pruning selector and dependency rule before it is written (I-17,
   ADR-021/022), and is either added to snapshot contents or marked rebuildable;
   everything published stays encrypted (I-19). No cache survives source pruning.
8. **Local compute stays local** (I-16). No remote embedding, hosted classifier
   or server-side index.
9. **Foreign formats stay in P10-02.** `.cursor/rules`, `.cursor/agents`,
   `.cursor/worktrees.json` and `permissions.json` are read only by deferred
   importers and grant nothing (ADR-014).
10. **Native Windows first.** Argument vectors rather than shell strings, path
    and reparse-point handling, and process profiles pass on native Windows before
    any other claim.
11. **Existing suites are the regression gate.** Each increment names the
    completed tasks whose evidence it touches and reruns those cases. A valid
    test is never weakened to admit the new behavior.

## Existing implementation and required refactors

| Existing boundary | Required change | Preserve / migration evidence |
|---|---|---|
| `vcp-tools::{schema, read::search}` | Add optional `mode`, word-boundary and path filter; bounded regex evaluation | Default literal results byte-identical; `complete`/`exclusions` contract kept; new schema revision recorded |
| `vcp-tools::patch` change sets and per-file receipts | Retain before-images as artifacts where absent; add optional `purpose` tag on prepared changes | Receipts and hashes of existing records unchanged; untagged changes behave as today |
| `vcp-repository::{worktree, dirty_snapshot, merge}` and the child registration records | Add provisioning stage after materialization and before readiness; add reference-checked cleanup | Readiness receipt semantics, no-stash/no-reset rule and "never delete from a guessed path" remain; absent recipe means today's behavior |
| `vcp-domain::agents` and `vcp-engine::agents` | Resolve a named role to a versioned template; record template revision on the node | Free-form `role` strings keep working; authority intersection and `ChildMode` rules are untouched |
| `vcp-policy` rules and `vcp-engine::policy` | New restrictive defaults as a policy revision | Deny-over-allow precedence, grant binding and qualified P2-03 fixtures unchanged; newly affected operations listed for owner review |
| `vcp-context::{selection, manifest, compaction}` | New attributed parts: plan, fragment, MCP state; compaction summary names its source range | Mandatory context, tool pairs, send fences and exact captured request remain intact |
| `vcp-extensions::{skill_manifest, activation}` and `src/skills/builtin/catalog.json` | Optional `activation`/`attach_globs`; staged debug and review procedures as new catalog versions | Descriptors without the fields activate exactly as now; body hashes and ADR-025 packaging hold |
| `vcp-memory` publication, `vcp-domain::search`, `vcp-store::search_contract` | Separate `workspace_source_chunk` kind and generation family | Existing claim and retained-source generations, manifests, M02–M06 results and P5-08 evidence unaffected |
| `vcp-memory::retention*`, `vcp-store::{portable_snapshot, snapshot_inputs}` | Register selectors and dependencies for before-images, review records, debug logs and code-index caches | No aggregate or cache outlives its source; restore rebuilds derived state |
| `vcp-cli::{terminal, args}`, `vcp-audit` inspectors | New commands and inspector views | Existing command grammar, exit codes and JSONL envelopes unchanged; unknown commands still fail |
| `foundation::routing_state` evidence projections | Accept a `cohort` label on retained observations | Unlabelled history remains the default cohort; optimizer totals exclude best-of-N unless asked |

Record the detailed boundary and retention choice in a new ADR before introducing
a new artifact or record kind. Candidates: workspace code index, indexed regex
search and its gate, rewind semantics and before-image retention, best-of-N cohort
evidence. CR-03, CR-08 and CR-09 extend ADR-010, ADR-011, ADR-024 and ADR-025
and need an ADR only if implementation forces a new decision.

## CR-08 — Worktree provisioning and bounded cleanup

Verify first: read [P7-04](14-visible-delegation.md#p7-04--graph-and-workspace-ownership),
the [child workspace guide](../development/p7-child-workspaces.md) and
`vcp-repository::worktree`. Confirm that no provisioning or reclamation exists.

1. Define an optional workspace file `.vcp/worktrees.toml` with a generic
   `[setup]` section, an optional `[setup.windows]` override, an explicit `copy`
   list and a `[cleanup]` section. Commands are argument vectors; a script is a
   path relative to the file. Parse it as data under byte and depth limits. It is
   an executable repository input: reading it runs nothing.
2. Run setup as a distinct stage between materialization and the readiness
   receipt. Each command is a prepared process invocation under the **child's**
   authority, evaluated by ordinary policy, time-limited, output-spooled and
   recorded with a receipt. A failed or denied setup leaves the child in an
   explicit setup-blocked state with its registered directory intact; it never
   reports ready. Expose the root worktree path to setup processes through one
   documented environment variable.
3. Treat `copy` as credential-adjacent. Copy only listed files, never follow
   links out of the root, never symlink dependency trees, and keep copied file
   contents out of capture under the existing sensitive-path rules.
4. Implement cleanup as owned, resumable work over **registered** disposable
   paths only. Skip any worktree with unintegrated changes, an unreconciled
   effect, an unsettled reservation, a live reader or a recovery reference, and
   report what was kept and why. A count cap bounds retained disposable roots;
   it never deletes to satisfy the cap.

Preserve: the no-stash/no-commit/no-reset rule, snapshot fingerprint checks, the
no-hooks/no-filters worktree creation, and the rule that pause or steering
invalidates outstanding preparation and launch tickets.

Acceptance: fresh-root checks pass on Node, Python, Rust and .NET fixtures only
when setup ran; a hostile recipe in an untrusted workspace dispatches nothing
without a grant; setup during `/pause` does not start; kill during setup and
during cleanup reconciles without a duplicate effect or a deleted live root;
an absent file reproduces current P7-04 traces exactly.

## CR-03 — Built-in helper roles

Ship three versioned `ChildSpec` templates packaged like built-in skills
(ADR-025). Selection stays with the controller. A model may request a role; it
cannot grant one or widen it.

| Role | Authority | Routing default | Result packet |
|---|---|---|---|
| `explore` | `ChildMode::ReadOnly` | Cheapest eligible group meeting the context need | Ranked path/range/reason list plus a bounded synthesis; full transcript as an artifact |
| `shell-runner` | The parent's already granted process profiles, never wider; no file-write paths | Cheap eligible group | Exit status, parsed findings and artifact references; no raw log in the parent prompt |
| `verifier` | The existing `vcp_verify` path; no file-write paths | A different eligible group from the implementer when one exists | Per-claim pass/fail bound to the current workspace fingerprint |

Verify first: `ChildMode` has only `ReadOnly` and `IsolatedWrite`, and a read-only
child can neither patch nor execute. `explore` therefore maps directly to
`ReadOnly`. `shell-runner` and `verifier` execute processes, so decide from the
P7-04 broker rules whether `IsolatedWrite` with an empty write set expresses
"may execute, may not write" under qualified filesystem enforcement, or whether
that needs a recorded decision. Do not loosen `ReadOnly` to fit them, and ship
`explore` alone if the other two cannot be expressed without widening authority.

Resolve the template at graph admission and record its revision on the node so a
later catalog change cannot alter a running child. Routing defaults are inputs
to the existing deterministic selector; strict pins, quality floors and root
limits still decide. Each helper draws an ordinary reservation and appears in
`/agents`. Project-defined roles are deferred; if added they use the skill
package format rather than a new file type.

Acceptance: extend U06 with a broad "where is X handled" task run with and
without `explore`, recording parent-context tokens, total cost including the
helper, and answer quality. `verifier` must fail a fixture in which the
implementer claims success from a stale verification record. An unknown role
name behaves as it does today.

## CR-02a — Bounded regex search

Add `mode: literal | regex` (default `literal`), an optional word-boundary flag
and an optional path glob to the existing search tool. Use a linear-time regex
engine already present in the dependency graph if one exists; otherwise justify
the single dependency under AGENTS.md section 11. Bound pattern length, compiled
size, per-file and total bytes scanned, hits and wall time. A rejected or
timed-out pattern is a typed result, never an empty success. Keep the scan set,
ignore rules and authorization identical to `vcp_read`, and keep reporting
`complete`, `exclusions` and, newly, scanned-file counts.

Acceptance: a differential test against an independent full-scan oracle over a
corpus covering literal, alternation, anchors, `.*`, case-insensitive, Unicode
and CRLF cases; pathological patterns hit the bound rather than the deadline of
the turn; literal-mode output is byte-identical to the pre-change fixture
outputs; P2-04 tool fixtures and a retained loop trace rerun under the new
schema revision.

**CR-02b (gated).** Record search p50/p95 on the small, medium and large
fixtures and on an owner repository. Build a candidate index only if p95 exceeds
a threshold recorded before measuring. First test whether the existing Tantivy
code-aware tokenizer gives adequate candidate filtering; compare trigram and
sparse n-gram forms on VCP fixtures before adding an index format. Any index may
only add candidates: files changed since the index generation are scanned
directly, so staleness can never hide a match. A mutation test must prove it.

## CR-13 — History recall and MCP state

1. Add a bounded read-only tool over the task's **own** pre-compaction events
   and artifacts, by range or literal query. Authorize the own session tree
   only, apply current redaction and retention, and return attributed data with
   artifact references. A pruned range returns the explicit gap marker used by
   inspectors, never reconstructed prose. Have each compaction summary name the
   source range it replaced so the model knows what it may ask for.
2. Add one small static context part per configured MCP server carrying
   registration identity, state (`ready`, `auth_required`, `failed`, `disabled`)
   and tool names only. Full schemas stay deferred as today. The part is
   attributed and revisioned like any other; a state change invalidates
   dependent manifests through the existing table in segment 04.
3. Measure prompt tokens before and after on MCP-heavy and long-trace fixtures.
   Cursor's published reduction is theirs and is not a VCP target.

Acceptance: after forced compaction a question about an early tool result is
answered through the recall tool with the correct artifact reference; another
session's ID is denied; a pruned range yields a gap; an `auth_required` server
is reported to the user instead of silently losing tools; with no MCP servers
configured the serialized request is unchanged.

## CR-10a — Deterministic guards

Verify first: read the `vcp-policy` rule set and the P2-03 fixtures to establish
whether writes to `.git/config`, `.git/hooks/**`, `.vcp/**` policy and recipe
files and the ignore file, file deletion, and writes outside registered roots
already require explicit approval in every preset. Record the finding.

If a gap exists, add the guard as a new policy revision that can only move a
decision from allow to question. Run the complete qualified P2-03, P2-04 and
P7-04 fixture sets against it and list every operation whose outcome changes.
That list is an owner review item before the revision becomes a default:
changing what `autonomous` does is a permission-model change under AGENTS.md
section 18. If no gap exists, record that and close the increment.

Cursor documents OS sandboxing for macOS and Linux only and offers no Windows
pattern; VCP's AppContainer and Job Object work is not affected by this item.

## CR-04 — Agent-change rewind

Verify first: establish whether the bytes replaced by each applied change are
retained as artifacts or only hashed. If only hashed, retaining the before-image
is the one new storage requirement; give it a pruning selector and a bounded
window (the current task plus a configured number of completed tasks) before
writing the first byte. Report before-images needed by an unexpired window as a
pruning dependency.

1. Define a checkpoint as a **projection** `(session, turn boundary) → {path,
   content hash before}` over the existing per-file mutation receipts. Add no
   snapshot store and no new canonical record kind.
2. Build `vcp rewind --to-turn <id> --preview`, which returns a preview identity
   bound to the store revision, exactly as prune previews are. `vcp rewind apply
   <preview-id>` and `/rewind` apply it. A stale preview is rejected.
3. Apply rewind as ordinary prepared changes, inheriting version checks,
   authority and receipts. For each file: restore only when the current hash
   equals VCP's last recorded after-hash; otherwise mark a conflict, skip it and
   report it (I-06). Delete a VCP-created file only if unchanged since creation;
   recreate a VCP-deleted file only if the path is still absent. Preserve
   encoding and line endings byte for byte.
4. List effects outside the file journal — process side effects, MCP calls, Git
   operations — as not reversible in the preview. Rewind never claims to undo
   them, and it is not a replay (I-11). The conversation is untouched; combine
   with the existing `sessions fork --through-turn` to branch it.
5. Rewinding a turn that integrated a child result reverses the integration
   diff in the parent, never the child worktree.

Acceptance: E05/R02-style races with a human edit between agent edit and rewind;
CRLF and encoding preserved; rename-then-edit chains; forced kill mid-apply then
reconcile on resume; pruning inside the window reports the dependency; an old
session without before-images reports rewind unavailable rather than failing.

## CR-05 — Plan as a revision-bound artifact

Add `/plan` and `vcp run --plan <path>`. `/plan` runs under `Autonomy::Plan` and
produces a markdown task artifact with a small required header: objective,
acceptance checks, affected paths, open questions and a budget estimate by
profile. `--save` copies it to `.vcp/plans/<slug>.md`. Admitting a plan seals
its content hash into the context manifest as an attributed part. Editing the
file afterwards produces a visible new manifest revision at the next scheduling
boundary, through the existing steering path; it never silently changes a
running task. Clarifying questions use the existing durable question records.
A plan is task input: it cannot raise a cap, widen a grant or select autonomy.
Once CR-04 exists, document the recovery path "rewind to the plan boundary, edit
the plan, run again" as a CLI affordance.

Acceptance: a plan run performs zero write and process effects at the broker; a
mid-run edit yields a recorded revision; a plan containing instructions to
change grants changes none; `vcp run` without `--plan` is unchanged.

## CR-11a — Durable input queue

Distinguish `steer`, which applies at the next safe boundary and exists today,
from `enqueue`, which starts a new turn after the current one completes. Use an
explicit command so bare input keeps meaning steering. Persist the queue as
task-linked pending input with its own revision; allow list, reorder and remove.
Pause keeps the queue and dispatches nothing from it; resume revalidates each
entry against the current task state before use. JSONL exposes the same records.

Acceptance: the queue survives forced kill and reopen; entries queued before
`/pause` do not run until `/resume`; removing an entry while it is being
admitted resolves by revision check to exactly one outcome.

## CR-12a — Local change review

1. Add `vcp review [--base <ref>] [--uncommitted] [--depth quick|deep]` and
   `/review`, run as a read-only child. Depth maps to a routing profile, not a
   separate code path. Findings keep the existing distinction between
   demonstrated defects and suggestions.
2. Key each review record by the stable Git patch identity of the reviewed diff
   plus the rule-set and skill revisions. A repeat on an identical patch returns
   the stored findings with zero provider requests unless forced. Apply the same
   key in P7-05 so an unchanged child diff is not re-reviewed. For non-Git
   folders use a digest of the normalized prepared diff.
3. Store the last reviewed commit per branch; default scope is changes since
   then, with a full override. Discover nested `REVIEW.md` files upward from
   changed paths through the single instruction scope resolver.
4. Review records are retained payloads: register their selector, make them
   source-dependent on the reviewed artifacts and recheck access on display.

Acceptance: second run on an identical patch makes zero provider calls; a
rebased but identical patch still hits; a changed rule file misses; the review
workspace is byte-unchanged (U02); `--explain-rules` lists rules used, truncated
or omitted.

**CR-12b (gated).** After CR-12a has accumulated accepted and rejected findings,
propose `review_rule` claims through P5-01 governance with evidence links and an
acceptance rate. Low-acceptance rules are disputed or superseded through normal
governance, never silently disabled. Build only with enough local history to
show an effect on false findings.

## CR-09 — Scoped instruction fragments

Adopt the descriptor option only. Allow optional `activation = "always" |
"auto" | "glob" | "manual"` and `attach_globs` in skill descriptors, so an
instruction-only skill covers all four rule behaviors using existing discovery,
activation, authority fences and inspection (ADR-024). `glob` attaches when a
matching path is in the evidence set; `manual` attaches only on explicit
selection. Each attached fragment is a context part with source, scope,
revision, inclusion reason and token estimate, visible in context inspection.
Conflicts follow the existing precedence in architecture 6.2. Enforce a size
cap and surface truncation. Reading `.cursor/rules/*.mdc` remains P10-02.

Acceptance: a fragment appears only when a matching path is in the evidence set;
two discovery routes cannot load it twice; a fragment saying "ignore previous
instructions" or "run X" changes no grant and causes no dispatch; descriptors
without the new fields produce identical activation events.

## CR-06 — Evidence-first debug workflow

Extend the `review-debug` catalog entry with a staged procedure: hypotheses,
instrument, reproduce, analyze, minimal fix, remove. The engine adds two
guarantees a prompt cannot. Changes made in the instrument stage carry
`purpose = "instrumentation"` on their prepared-change records. A task cannot
report completed while a tagged change is still present, unless the user
explicitly keeps it; removal is the inverse prepared change, or CR-04 scoped to
the tag. This completion check binds only tagged changes, so tasks that never
use the workflow are unaffected. Instrumentation appends JSON lines to
`.vcp/debug/<task>/events.jsonl` inside the workspace and ignore-listed: no
network listener and no extra authority. Reproduction is a durable question
with exact steps; analysis uses ordinary bounded reads. The log is a retained
payload with a selector and a task-scoped lifetime.

Acceptance: a seeded race fixture records hypotheses before any fix edit; zero
tagged changes remain at completion; killing the CLI mid-workflow and resuming
still ends with instrumentation removed or explicitly reported; a task with no
tagged change completes exactly as before.

## CR-07 — Best-of-N across model groups

P6 qualification [recorded rejection](../evaluations/p6-completion.md) because
held-out evidence was thin. Best-of-N produces the missing kind of observation:
one task, one base, several model groups, real verification outcomes and exact
per-attempt charges (ADR-031).

1. Add `vcp run --best-of <group[,group…]>` and `/best-of`. The controller
   creates N isolated write children from one recorded base and dirty
   fingerprint, each pinned to one group and profile, all running the same
   acceptance checks.
2. Admit all N reservations atomically against the root cap before any child
   starts. If the cap admits only k < N, say so and ask; never drop candidates
   silently. Under a concurrency cap of one the children run sequentially.
3. Persist a comparison record with per-child verification results, diff
   statistics, cost, latency and unresolved issues. The user, or an explicit
   deterministic rule such as "all checks pass, then lowest known cost," selects
   a winner. Integration goes through P7-05 unchanged, and the merged parent is
   re-verified. Losing roots fall under CR-08 cleanup.
4. Write outcomes as retained transition evidence (ADR-029/030) labelled
   `cohort = best_of_n`. Users run best-of-N on hard tasks, so the cohort is
   biased: optimizer statistics exclude it by default and include it only on
   request, always labelled.

An LLM judge that ranks candidates is a bounded semantic decision under ADR-020:
advisory, accounted, with a deterministic fallback, never auto-applying. It is
out of scope until a purpose is qualified.

Acceptance: insufficient-funds admission; identical base fingerprint across
children; winner re-verified on the parent; evidence rows carry the cohort
label; default `/optimize` totals are unchanged by the presence of cohort rows;
parent pause stops all N.

## CR-01 — Whole-workspace local code index

This is the largest increment and the only one adding a derived store. It
deliberately differs from Cursor's server-side index: every byte of chunking,
embedding and search stays local (I-16).

1. `vcp-repository`: build a Merkle tree over the ignore-filtered discovery set,
   reusing existing discovery and ignore policy so the index can never see more
   than `vcp_read` can. Leaf is a hash of file bytes; node is a hash of sorted
   child name/hash pairs. Reuse a Git blob identity only for clean tracked
   files. Persist it as a rebuildable cache keyed by workspace root identity.
2. Diff the previous and current trees and emit index intents for diverging
   leaves only, through the existing generation and publication protocol
   (P5-05). Do not create a second publisher.
3. Chunk syntactically where a grammar is qualified and fall back to the
   existing text chunker for unsupported languages, generated files and parse
   failures, recording the chunker specification in the generation. Grammar
   selection and its native Windows build cost are an open decision; a
   borrowed extractor needs the normal pin, license and provenance review.
4. Cache embeddings by the complete embedding specification, chunker
   specification and chunk bytes. The cache is rebuildable and excluded from
   canonical state.
5. Use a distinct `workspace_source_chunk` record kind and its own generation
   family, so results stay separately inspectable and pruning history never
   deletes the live code index, nor the reverse. Whether it shares the DiskANN
   instance with memory is an open decision settled by measurement.
6. Build in the background under explicit CPU, memory and disk caps with
   cancellation, inside the pause fence. Report readiness as a fraction and
   label partial results rather than gating on a fixed percentage. Default
   `build_on_open = "ask"`: first start must not index a repository without
   telling the user.
7. For portability, a snapshot may carry the tree and chunk vectors. On restore,
   recompute the local tree and reuse vectors only for leaves whose hashes
   match; schedule the rest (I-15).

Expose `search.semantic`-style results through the existing retrieval contract
with path, range, source hash, record kind, score, generation and a `stale`
flag, plus `vcp index status|rebuild|pause|resume` and `/index`.

Acceptance: extend M03/M04. Editing one file in a 10k-file fixture re-embeds
only that leaf's chunks, asserted by inference call count. Cover rename, delete,
ignored, generated and oversized files. Include a semantic-intent truth set whose
query term never appears in the target file. Compare lexical only, lexical plus
the M5 map arms, and lexical plus semantic on the same frozen tasks and token
budgets. A network-blocked run proves no egress. Existing M02–M06 and P5-08
cases rerun unchanged with the index disabled and enabled. Reject the index if
its resource burden or false positives outweigh measured gains.

**CR-14 (gated).** Only after CR-01 shows benefit: log per-retrieval candidates
and downstream use signals, then fit small inspectable artifacts such as fusion
weights, treated exactly like Markov fits — rebuildable (ADR-032), held-out and
task-separated (ADR-033), abstaining on sparse data, never active without
qualification, with rules-only operation always supported.

## Test and fixture additions

Add fixtures beside the owning behavior under the existing layout in
[segment 16](16-test-fixtures-and-acceptance.md); create directories with their
first useful content.

| Increment | Fixture location | Independent oracle |
|---|---|---|
| CR-08, CR-03, CR-07 | `src/tests/fixtures/repositories/`, delegation contracts and recovery cases | External effect markers, broker dispatch counts, root ledger totals |
| CR-02a/b | `src/tests/fixtures/search/` regex corpus | Full-scan oracle that does not call the function under test |
| CR-13, CR-05, CR-09 | `src/tests/fixtures/context/` | Exact expected manifests; serialized-request equality for disabled parity |
| CR-04, CR-06 | `src/tests/recovery/` and `src/tests/platform/windows/` | Initial and final bytes, staging state and outside-scope sentinel files |
| CR-12a | `src/tests/fixtures/repositories/` prepared diffs | Scripted transport request count; seeded-defect rubric kept out of model-visible input |
| CR-01 | `src/tests/fixtures/search/`, `src/evals/fixtures/retrieval/` | Separate relevant-ID truth set, forbidden-text sentinels, inference call counter |

Every increment adds one **disabled-parity** case and one **old-record reopen**
case. Both stores run every case that touches canonical state. Live comparisons
(CR-03, CR-07, CR-01) need an explicit configured evaluation cap and never run
as ordinary checks.

## Adoption steps for the plan documents

This supplement was added without editing other plan files. When it is adopted:

1. Add a row for this file to the [segment index](README.md#segment-index) and a
   "Cursor-derived improvements" row to the implementation reading map, and
   state in the revision note that task counts and dependencies are unchanged.
2. Add a "Cursor follow-up readiness" ledger to [traceability](20-traceability.md)
   beside the Markov follow-up ledger, one row per selected or gated increment
   with owners, remaining acceptance and state `planned`. Completed rows above
   it stay complete.
3. Add a short "Planned follow-up" pointer under each owning task heading named
   in the dependency table, as the Markov supplement did, without altering the
   task's exit criteria.
4. Re-run the [mechanical graph validation](20-traceability.md#mechanical-graph-validation-procedure):
   68 tasks, 56 in the P8-05 closure, no new edges, links and anchors resolving.

## Exit records and exclusions

For every implemented increment, update its owning section and the follow-up
ledger with source and schema revisions, exact commands, linked outcomes,
limitations and remaining gates. Comparison-gated increments publish the frozen
manifest and predeclared criteria with passes, rejections and not-run rows. At
P8-05, or at the first release review after it, publish each increment's
implemented, enabled, rejected or deferred status and reason; an optional
candidate may exit as rejected but must not silently disappear.

Do not build a server-side or shared index, a hosted classifier, hidden model
selection, cloud agents, automations, team or marketplace features, tab
completion or inline edit, or a classifier that can allow an operation the
deterministic policy did not. Do not import Cursor file formats outside P10-02,
and do not copy Cursor documentation, prompts or default lists. A new index,
cache or review record is not authorization to retain data beyond pruning or to
share it across workspaces; either requires a separate explicit product decision.
