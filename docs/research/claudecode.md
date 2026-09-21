---
doc_id: vcp-research-claude-code-derived-increments
title: "Research note: Claude Code-derived increments (CC-01 … CC-18)"
status: proposed            # nothing here is confirmed, qualified, or implemented
authority: research input   # vcp-what.md, the ADR register and owner decisions take precedence
suggested_path: docs/research/claude-code.md               # mirrors docs/research/cursor.md
suggested_plan_path: docs/plan/23-claude-code-improvements.md   # mirrors docs/plan/22-cursor-improvements.md
vcp_revision_read: 3fa8ec7686360f710e4f4ce59407ef09ddfa55db   # main, 2026-09-21, includes plan 22
claude_code_evidence_date: 2026-09-21
claude_code_evidence_source: "official docs at code.claude.com/docs (index /docs/llms.txt), read through the pleaseai/claude-code-docs mirror at 939870573f4b988dfccbfd7f798f1b89818b8884 (synced 2026-09-21T18:17Z)"
reuse_form: pattern adoption only   # Claude Code is closed source; no code, prompt, doc text or default list is imported
id_namespace: CC-nn                 # unused in the repo at the read revision; distinct from CR-nn, C01–C07, G, E, M, R, U, FR, I, P
---

# Research note: Claude Code-derived increments

## 0. How an agent should use this file

1. This is a **gap analysis and design proposal**, the Claude Code counterpart of
   `docs/research/cursor.md`. It creates no task IDs, changes no dependency, adds nothing to the
   P8-05 closure and enables nothing. If adopted, it should be turned into a plan supplement the
   same way `docs/plan/22-cursor-improvements.md` was: select, split, gate, or decline each item.
2. **Do not duplicate plan 22.** Many Claude Code features overlap Cursor ones. Where a `CR-nn`
   increment already covers the idea, this note only records *what Claude Code adds to it* (section
   4 column "Relation to plan 22") and never restates the CR design.
3. **The preservation contract in plan 22 applies unchanged** (additive versioned schemas, disabled
   parity, model-visible catalog changes are context changes, no new authority path, one
   controller/store/ledger/scheduler, rebuildable derived state, declared retention and
   portability, local compute, foreign formats stay in P10-02, native Windows first, existing
   suites are the regression gate). It is not repeated here. Section 6 lists only additions.
4. Plan 22's selection rule also applies: prefer deterministic local mechanisms; select nothing
   whose value depends on a new unqualified model judgment, because P6 qualification ended in
   recorded rejection of optional advisory classifiers.
5. **Verify first, against source.** The Cursor note was corrected at plan time because its
   "VCP today" claims were not source-audited. This note audited `src/crates` with targeted greps
   at the read revision (section 2) and says exactly what was and was not checked. Re-run each
   increment's verify step at implementation; the in-flight P7 branch was not visible.
6. Each `CC-nn` section is self-contained (AGENTS.md section 2). Read the index, then one section.

## 1. Machine-readable index

```yaml
increments:
  - {id: CC-01, title: "Cache-aware request layout, breakpoints and switch-cost accounting",
     class: "deterministic", suggested_disposition: "select", priority: P1,
     owning_tasks: [P2-02, P2-01, P2-08, P6-02, P6-03], crates: [vcp-models, vcp-context, vcp-lifecycle, vcp-audit],
     relates_to: [], adr_candidate: "yes"}
  - {id: CC-02, title: "Long-running and background process lifecycle",
     class: "deterministic", suggested_disposition: "select", priority: P1,
     owning_tasks: [P2-04, P2-06, P2-07], crates: [vcp-tools, vcp-engine, vcp-lifecycle, vcp-cli],
     relates_to: [CR-15/M6], adr_candidate: "yes (amend ADR-004)"}
  - {id: CC-03, title: "Process result interpretation on native Windows",
     class: "deterministic", suggested_disposition: "select", priority: P1,
     owning_tasks: [P2-04, P2-06, P8-01], crates: [vcp-tools],
     relates_to: [], adr_candidate: "no"}
  - {id: CC-04, title: "Ranged reads and pattern discovery",
     class: "deterministic", suggested_disposition: "select (bundle with CR-02a schema revision)", priority: P1,
     owning_tasks: [P2-04], crates: [vcp-tools, vcp-repository],
     relates_to: [CR-02a], adr_candidate: "no"}
  - {id: CC-05, title: "Structured work list that survives compaction",
     class: "deterministic store / model-used", suggested_disposition: "evidence-gated per model group", priority: P3,
     owning_tasks: [P2-05, P2-08, P3-02], crates: [vcp-domain, vcp-engine, vcp-context, vcp-cli],
     relates_to: [CR-05], adr_candidate: "no"}
  - {id: CC-06, title: "User-directed range compaction and cold-resume choice",
     class: "deterministic", suggested_disposition: "select", priority: P2,
     owning_tasks: [P2-08, P3-02, P3-04], crates: [vcp-context, vcp-cli],
     relates_to: [CR-04, CR-13, CC-01], adr_candidate: "no (extends ADR-009)"}
  - {id: CC-07, title: "Post-compaction rehydration set",
     class: "deterministic", suggested_disposition: "select", priority: P2,
     owning_tasks: [P2-08], crates: [vcp-context],
     relates_to: [CR-05, CR-13, M5], adr_candidate: "no (extends ADR-009)"}
  - {id: CC-08, title: "Code-execution-adjacent protected paths and destructive-removal circuit breaker",
     class: "deterministic, restrictive-only", suggested_disposition: "fold into CR-10a", priority: P1,
     owning_tasks: [P2-03], crates: [vcp-policy, vcp-tools],
     relates_to: [CR-10a], adr_candidate: "no (extends ADR-005)"}
  - {id: CC-09, title: "Explicit session boundaries as deterministic denials",
     class: "deterministic, restrictive-only", suggested_disposition: "select", priority: P2,
     owning_tasks: [P2-03, P3-02], crates: [vcp-policy, vcp-cli, vcp-context],
     relates_to: [CR-10a], adr_candidate: "no"}
  - {id: CC-10, title: "Usage attribution by extension, cache statistics and behavior flags",
     class: "deterministic, read-only", suggested_disposition: "select", priority: P2,
     owning_tasks: [P3-03, P1-05, P6-05], crates: [vcp-audit, vcp-cli],
     relates_to: [CC-01], adr_candidate: "no"}
  - {id: CC-11, title: "Language-server code intelligence (navigation and post-edit diagnostics)",
     class: "deterministic", suggested_disposition: "evidence upgrade for CR-15/M6; navigation evidence-gated", priority: P2,
     owning_tasks: [P2-04, P2-06], crates: [vcp-tools],
     relates_to: [CR-15, M6, CR-01], adr_candidate: "yes if a server host is added"}
  - {id: CC-12, title: "Bounded event watch (monitor)",
     class: "deterministic", suggested_disposition: "deferred until CC-02 lands", priority: P3,
     owning_tasks: [P2-04], crates: [vcp-tools, vcp-engine],
     relates_to: [CC-02], adr_candidate: "no"}
  - {id: CC-13, title: "Re-runnable orchestration recipes for large fan-outs",
     class: "deterministic runner / model-authored recipe", suggested_disposition: "evidence-gated after P7-06", priority: P3,
     owning_tasks: [P7-04, P7-05, P7-06], crates: [vcp-engine, vcp-domain, vcp-budget],
     relates_to: [CR-07, CR-03], adr_candidate: "yes"}
  - {id: CC-14, title: "Verified-findings pipeline and structured finding records for review",
     class: "mixed", suggested_disposition: "fold deterministic parts into CR-12a; verification stage evidence-gated", priority: P2,
     owning_tasks: [P7-05, P7-02], crates: [vcp-extensions, vcp-domain, vcp-cli],
     relates_to: [CR-12a, CR-12b, CR-03], adr_candidate: "no"}
  - {id: CC-15, title: "Worktree lifecycle refinements",
     class: "deterministic", suggested_disposition: "fold into CR-08", priority: P1,
     owning_tasks: [P7-04], crates: [vcp-repository],
     relates_to: [CR-08], adr_candidate: "no (extends ADR-010)"}
  - {id: CC-16, title: "Schema-validated final output for structured runs",
     class: "deterministic validator / model-produced value", suggested_disposition: "select", priority: P3,
     owning_tasks: [P3-01], crates: [vcp-cli, vcp-protocol],
     relates_to: [], adr_candidate: "no"}
  - {id: CC-17, title: "Context-inheriting child (fork) as a delegation mode",
     class: "deterministic", suggested_disposition: "deferred with CR-11b", priority: P3,
     owning_tasks: [P7-04, P7-06], crates: [vcp-engine, vcp-context],
     relates_to: [CR-11b, CC-01], adr_candidate: "no"}
  - {id: CC-18, title: "Memory deltas: bounded always-present digest, freshness stamps, user-scope preferences",
     class: "deterministic", suggested_disposition: "owner decision (touches A07 scope)", priority: P3,
     owning_tasks: [P5-01, P5-06, P2-01], crates: [vcp-memory, vcp-context],
     relates_to: [], adr_candidate: "only if scope changes (ADR-008)"}
```

## 2. Evidence base, source audit, and caveats

**Claude Code evidence.** Closed source, native binary. All behavior statements come from the
official documentation set dated 2026-09-21 (docs reference CLI versions v2.1.19x–v2.1.26x).
Pages used are keyed S1–S22 in section 8. `docs/architecture/othertools.md` section 3 already
holds a good structural summary; this note does not repeat it and section 3 below lists only
**deltas and details that matter for the increments**.

**VCP source audit performed at the read revision** (grep over `src/crates`, excluding
`src/third_party` and tests unless noted):

| Question | Observed | Used by |
|---|---|---|
| Model-facing tool set | `vcp_read`, `vcp_list`, `vcp_search`, `vcp_patch`, `vcp_exec`, `vcp_verify`, `vcp_mcp`, plus `vcp_skill`, `vcp_workspace`, `vcp_decision` (names found in `vcp-tools::schema`, `vcp-lifecycle::foundation::worker::coding`) | CC-02..05 |
| `vcp_read` contract | Schema text: one scoped UTF-8 file, at most 1 MiB, parameters `path`, `max_bytes`. No offset/limit/line-range parameter found in `vcp-tools::schema` or `read.rs` | CC-04 |
| Discovery | `vcp_list` lists one directory (non-recursive, ≤10 000 entries). No glob/pattern tool; `glob` has no model-facing match | CC-04 |
| Process ceiling | `vcp-tools::process::prepare` rejects `timeout_ms == 0` or `> 120_000`; `vcp-tools::verification` builds its runner request with `timeout_ms: 120_000, output_bytes: 1 MiB` | CC-02 |
| Background / detached processes | No model-facing background mode found in `vcp-tools::process`. (`--detach` appears only in `vcp-repository::worktree`.) Docs grep for "long-running", "dev server", "watch mode" in plan 06 and the P2 process/verification/PTY guides: no hits | CC-02, CC-12 |
| Exit-code interpretation | `exit_code: Option<i32>` is carried through `vcp-engine::command_handler` and `vcp-protocol::command`; no per-command benign-exit table found (`benign`, `robocopy`, `findstr`: no hits in product crates) | CC-03 |
| Prompt-cache handling | Cache **charges** are fully modelled (`ChargeCategory::CacheRead/CacheWrite`, `cached_input_tokens`, `cache_write_input_tokens`, quote bounds). No `cache_control`, no cache breakpoint, no cache-affinity or switch-cost term found in `vcp-models::request`, `vcp-models::routing`, `vcp-context` | CC-01 |
| Provider pinning | Requests send `provider: {only:[p], order:[p], allow_fallbacks:false, …}` (`vcp-lifecycle::foundation::conformance`, `vcp-models::decision`) and use the Responses-style body (`input`, `store:false`) in `vcp-models::request` | CC-01 |
| Compaction style | `vcp-context::compaction` is **deterministic**: keeps `keep_recent_pairs` (1–128) complete pairs and replaces older history with bounded previews plus artifact references labelled as untrusted historical excerpts. It is not a model-written summary. No focus/range/instruction parameter and no rehydration step found | CC-06, CC-07 |
| Work list | No task-list/todo tool in product crates. `update_plan` exists only inside vendored Codex (`src/third_party/codex/…`) and is not in VCP's registered tool list | CC-05 |
| Language server | No `lsp` / language-server code in product crates | CC-11 |
| Session-scoped user denials | No `forbid`/session-deny construct found in `vcp-policy` or `vcp-cli::terminal` (terminal commands listed in `terminal/owner.rs`: `/pause /resume /status /cost /history /groups /optimize /escalate /skills /mcp /agents …`) | CC-09 |
| Cost attribution by skill / MCP server | No `by_skill`, `by_server`, `attribut*` cost projection found in `vcp-audit` or `vcp-cli` | CC-10 |
| Output schema for final result | JSON-Schema handling exists only for the MCP schema profile (`vcp-extensions::mcp::schema`); no `--schema`-style CLI argument in `vcp-cli::args` | CC-16 |

**Not audited:** the in-flight P7 branch; `vcp-engine::agents` internals beyond what plan 22
already recorded; whether before-images are retained as artifacts (plan 22 flags that for CR-04).

**Known unknown for CC-01.** Whether OpenRouter's Responses-style endpoint accepts explicit cache
breakpoints for providers that require them was not verified. OpenRouter documents (S21) that most
providers cache automatically, that some (Anthropic, Alibaba) need per-message `cache_control`, and
that its own sticky routing is **not used when `provider.order` is set** — which VCP always sets,
so VCP's pinning already gives the same effect for a fixed endpoint.

## 3. Claude Code deltas that matter here

Paraphrased. Only items not already in `othertools.md` section 3, or that correct it.

**3.1 Prompt cache discipline (S2).** Requests are ordered so rarely-changing content comes first:
system prompt and tool definitions, then project context, then conversation. Matching is
exact-prefix; any change early in the request recomputes everything after it. Documented
invalidators include switching model, changing effort level on most models, connecting or
disconnecting an MCP server, enabling or disabling a plugin, denying an entire tool, compaction,
and upgrading the client. Documented non-invalidators include editing repository files, changing
permission mode, invoking skills (their text is appended as conversation, not inserted into the
prefix), and rewinding. The CLI asks for confirmation before a model or effort switch **only while
the cache is still warm**. Parallel subagents with identical model/tools/system prompt are briefly
held so that all but the first read the first one's freshly written prefix (default hold cap 5 s).
`/usage` reports request count, share of input served from cache, misses, expected rebuilds
(compaction), and warm/cold state. A large idle session offers "resume from summary" because the
cache has expired anyway (S9).

**3.2 Shell tools (S3).** Each command is a separate process; working directory carries over inside
allowed roots, environment does not. Default timeout two minutes, ceiling ten, both configurable.
Output streams to a working file; large valid output reaches the model as a path plus a short
preview, while failing output is returned as a head-and-tail excerpt. **A command that hits its
timeout is moved to the background instead of being killed**, and the result says so and gives the
output file. Background commands have explicit lifetime rules (a foreground subagent's commands end
with it; in non-interactive runs they end shortly after the final result). Exit code 1 is treated
as a valid negative answer for a fixed set of search/compare commands. A native **PowerShell tool**
is the primary shell on Windows: `pwsh` auto-detected with 5.1 fallback, process-scope execution
policy bypass that still respects Group Policy, UTF-8 for redirection and piped stdin, error output
captured without ANSI sequences, end-of-file delivered to children that wait on stdin, Windows
benign-exit handling (`findstr`, `where.exe`, `fc.exe`, and `robocopy` codes below 8), profiles not
loaded, **no OS sandbox on native Windows** (S4 confirms: sandbox is macOS, Linux, WSL2 only).

**3.3 File tools (S3).** Read returns line-numbered content, pages large files with an explicit
"partial view" notice and `offset`/`limit`, errors instead of loading an oversized explicit range,
and handles images, paged PDFs and notebooks. Edit is exact-string with three checks
(read-before-edit, exact match, uniqueness) and tolerates on-disk changes only when the match is
still exact and unique. Glob is pattern discovery sorted by modification time with a result cap and
a truncation flag. On macOS/Linux/WSL, Glob and Grep are now **absent by default** (search runs
through embedded `find`/`grep` replacements in the shell); on Windows they remain default tools.

**3.4 Work list (S3).** Task tools (`TaskCreate/Get/List/Update`, dependencies, status) replaced
the older checklist tool — and are **omitted by default on the newest models**, because those
models track multi-step work unaided and the tool definitions cost context. They remain default on
older/smaller models and can be forced on.

**3.5 Checkpointing (S5).** A checkpoint per user prompt; 100 most recent per session; rewind menu
offers code, conversation, or both, plus **"summarize from here" / "summarize up to here"** with an
optional focus instruction. Shell-made file changes and most subagent edits are **not** tracked.

**3.6 Compaction survival (S6).** After compaction: root instructions, auto memory and the
plan-mode plan are re-injected from disk; up to five most recently modified touched files are
re-read (large ones as path references); invoked skill bodies are re-injected under per-skill and
total token caps, oldest dropped first; path-scoped rules are *not* retained until their trigger
file is read again; running background work is re-announced so it is not started twice.

**3.7 Permissions (S7, S8).** Modes: manual, acceptEdits, plan, auto (classifier; now the default
on consumer plans), dontAsk, bypass. **Protected paths**: VCS metadata, editor/IDE config, shell
startup files, package-manager and build-wrapper config, git-hook managers, devcontainer config,
MCP and agent config — i.e. *files that cause code to run later* — are never auto-approved for
writes, and `allow` rules cannot pre-approve them. **Critical paths**: removal commands targeting
filesystem/drive roots, top-level directories, home, the working directory or its parents are never
approvable by rule or hook, including when hidden in subshells or when a glob sits under a possibly
empty variable; PowerShell `Remove-Item` on system paths or bare wildcards is denied in every mode.
**Boundaries stated in conversation** ("don't push") are honoured by the classifier but are
explicitly *not durable*: compaction can lose them; the docs recommend a deny rule for a guarantee.

**3.8 Parallel work (S10–S13).** Subagents (now background by default, one extra nesting level),
**fork** (a subagent that inherits the whole conversation and therefore the cached prefix),
agent view, agent teams with a shared task list and messaging, cross-session messaging, and
**dynamic workflows**: a model-written plain-JavaScript script, executed by a runtime outside the
conversation, that fans out to many subagents; intermediate results live in script variables; the
script is saved, diffable, editable and re-runnable; no mid-run user input; no direct filesystem or
shell access from the script; bounded concurrency (default 16), 4 096 items per parallel call,
1 000 agents per run; an advisory "large run" warning at 25 agents or ~1.5 M projected tokens;
resumable because each agent's result is tracked.

**3.9 Worktrees (S14).** `.worktreeinclude` (gitignore syntax) copies untracked files into each new
worktree; base ref is the default branch unless set to current HEAD; a **marker is written into the
git metadata of every tool-created worktree and cleanup removes only marked ones**; a
**`git worktree lock` is held while an agent runs**; sparse checkout is recommended for monorepos
(S15); non-git VCS is supported through creation hooks.

**3.10 Review (S16, S17).** Multi-agent review where each agent hunts a different issue class, then
a **verification step checks candidates against actual code behaviour to drop false positives**,
then dedupe and severity ranking; findings distinguish important / nit / **pre-existing**; a
structured findings tool carries file, summary, failure scenario and category; the CI check always
concludes neutral and ends with a machine-readable summary line; `REVIEW.md` tunes it.

**3.11 Goals and advisor (S18, S19).** `/goal <condition>`: after every turn a small separate model
judges the condition **from the transcript only** (it runs nothing), continuing until met, judged
impossible, or a user-fixable error. Advisor: the main model may consult a stronger model at key
moments (before committing to an approach, when stuck, before declaring done); server-side,
experimental, Anthropic-API only.

**3.12 Usage view (S9).** Attribution of recent usage to skills, subagents, plugins and individual
MCP servers (an MCP server's share counts only requests that consumed one of its results),
behaviour flags when one behaviour (long context, cache misses) exceeds 10 % of usage, and per-loop
rows for scheduled work.

**3.13 Memory (S20).** Auto memory is a per-repository directory of markdown notes with a small
index file; only the first 200 lines / 25 KB of the index load at session start, topic files load on
demand; four note types (user, feedback, project, reference); things derivable from code are
deliberately not saved; each note carries a `modified` timestamp; memory is shared across worktrees
of one repository and is machine-local. `.claude/rules/` holds path-scoped instruction files;
`CLAUDE.local.md` is an untracked personal layer; `@path` imports exist.

**3.14 Headless (S22).** `--output-format json|stream-json`, `--json-schema` for a validated final
value, per-invocation cost in the result, `--max-budget-usd`, max turns.

### Corrections worth applying to `othertools.md` section 3

| Existing statement | Current documentation |
|---|---|
| Grep is a default ripgrep-backed tool | Default only on Windows; elsewhere search goes through the shell unless the tools are requested |
| Sandbox: Seatbelt / bubblewrap | Still true; add: **native Windows unsupported**, WSL2 required for sandboxing |
| `auto` is one of several modes | It is now the default permission mode on Pro/Max/Team |
| Sub-agents, background optional | Background is the default in interactive sessions; subagents may spawn one further level |
| Hooks are shell commands or SDK callbacks | Handler types now include command, HTTP, MCP-tool and prompt hooks; many more events (instructions loaded, permission denied, post-tool-batch, task created/completed, config/cwd/file changed, pre-model-switch) |
| `TaskCreate`/`TaskUpdate` listed as standard tools | Omitted by default on the newest models |

## 4. Coverage matrix

| Claude Code capability | VCP status | Relation to plan 22 | Action |
|---|---|---|---|
| Nested instruction files, AGENTS.md | Covered (6.2; P2-01) | — | none |
| Skills with lazy bodies | Covered (ADR-024/025) | — | CC-07 for post-compaction caps only |
| MCP, deferred tool schemas | Covered (6.4; P7-03) | CR-13 (MCP state) | none |
| Subagents with isolated context | Partial (P7-04..06) | CR-03 | CC-17 note only |
| Worktree isolation | Partial | CR-08 selected | **CC-15** refinements |
| Rewind of agent file changes | Gap | CR-04 selected | Claude Code confirms CR-04's scope limit: shell-made changes are not reversible; say so in the preview |
| Partial / directed compaction | Gap | — | **CC-06** |
| What survives compaction | Partial (6.5 protected fields) | CR-05, CR-13 | **CC-07** |
| Plan mode, plan re-injected after compaction | Partial | CR-05 selected | CC-07 includes the plan part |
| Prompt cache discipline | **Gap** (charges modelled, layout/affinity not) | — | **CC-01** |
| Long commands, background jobs | **Gap** (120 s hard ceiling) | — | **CC-02** |
| Windows process result semantics | Gap / verify | — | **CC-03** |
| Paged reads, glob discovery | **Gap** | CR-02a selected | **CC-04**, same schema revision |
| Task list tool | Gap | CR-05 | **CC-05**, gated |
| Protected / critical paths | Partial (`.git` component rejected) | CR-10a selected | **CC-08** widens CR-10a's checklist |
| Conversation-stated boundaries | Gap (prompt-level only) | — | **CC-09** |
| Usage attribution, cache stats | Gap | — | **CC-10** |
| LSP navigation + diagnostics after edit | Gap | CR-15 folded into M6 (low-confidence source) | **CC-11** supplies a high-confidence source |
| Monitor tool | Gap | — | **CC-12**, deferred |
| Dynamic workflows | Gap | CR-07 (best-of-N is one fixed recipe) | **CC-13**, gated |
| Verified review findings | Partial | CR-12a selected | **CC-14** |
| Validated structured final output | Gap | — | **CC-16** |
| Fork (context-inheriting child) | Gap | CR-11b deferred | **CC-17**, deferred |
| Auto memory | Covered and stronger (governed claims) | — | **CC-18** deltas only |
| `/goal` | Deferred | CR-11c deferred | note: Claude Code's evaluator reads the transcript and runs nothing; a VCP version should evaluate with `verification.run` records (I-12), which needs no model judgment |
| Advisor | Covered in shape (P6-03 escalation advisory, ADR-036..039) | — | none; "before declaring complete" trigger is already CR-03's `verifier` role |
| Auto mode classifier | Deferred | CR-10b deferred, owner decision | none; note Claude Code also routes protected-path writes to the classifier, which VCP should **not** copy (I-01) |
| Permission rule grammar over shell strings | Out by design | — | VCP runs explicit executable profiles with argv, not free shell strings; a glob-over-command-string grammar would weaken that |
| Hooks | Deferred (P10-01) | — | record the expanded event and handler-type list when P10-01 starts |
| Plugins, marketplaces, plugin evals | Out / deferred (1.3) | — | VCP already has skill usefulness evals under `src/evals/skills` |
| Cloud sessions, routines, remote control, mobile, channels, Slack, artifacts, agent teams across machines, computer use, Chrome, voice, desktop app | Out | same as plan 22 "Declined" | none |
| OS sandbox for shell on Windows | No pattern to borrow | same finding as Cursor | VCP's AppContainer / Job Object work stays ahead |
| Output styles, status line, keybindings, themes | Not selected | — | no measured need |

## 5. Increments

Schema: **Pattern → Verify first → Proposed design → Contracts → Invariants → Acceptance → Open.**
PowerShell verify commands assume ripgrep (`rg`) on PATH.

---

### CC-01 — Cache-aware request layout, breakpoints and switch-cost accounting

**Pattern.** Section 3.1. S2, S9, S21.

**Why this matters more for VCP than for Claude Code.** Claude Code normally keeps one model per
session and warns the user before a switch. VCP *routes and escalates across model groups by
design* (P6-02/03). Every change of model — and, on most providers, of reasoning setting — turns the
whole accumulated history into uncached input for the next request. VCP already prices cache reads
and writes exactly (ADR-031) but, at the read revision, nothing in routing or context assembly
reasons about cache state. The ledger sees the cost after the fact; the router cannot avoid it.

**Verify first.**
```powershell
rg -n "cache_control|cache_read|cache_write|cached_input" src/crates/vcp-models src/crates/vcp-context src/crates/vcp-lifecycle/src
rg -n "fn assemble|order|Part" src/crates/vcp-context/src/manifest.rs src/crates/vcp-context/src/selection.rs
rg -n -i "handoff|escalat" src/crates/vcp-context/src/handoff.rs src/crates/vcp-models/src/routing
```

**Proposed design (all deterministic).**
1. **Stable-prefix layout rule in the assembler.** Give every context part a volatility class:
   `static` (operating instructions, built-in tool schemas), `session` (instructions, activated
   skill descriptors, MCP tool name list), `turn` (evidence, memory passages, history). Emit parts in
   that order and make ordering *within* a class canonical (sorted by stable ID), so that adding a
   late part never reorders an early one. vcp-what 6.1's logical order already points this way;
   the new requirement is byte-stability of the serialized prefix across turns.
2. **Prefix digest.** Record `prefix_digest(static)`, `prefix_digest(static+session)` on each sealed
   manifest. A changed digest between consecutive requests of one task is an explicit
   `context.prefix_changed{class, cause}` event. Causes are enumerable: tool schema revision, skill
   activation, MCP connect/disconnect, instruction revision, compaction, model change, reasoning
   setting change.
3. **Append, do not insert.** Mid-task additions (skill bodies, plan text, fragments from CR-09,
   MCP state from CR-13) go in as `turn`-class parts after existing history wherever semantics
   allow, mirroring how Claude Code keeps skill and plan-mode text out of the prefix.
4. **Cache breakpoints where the provider needs them.** For catalog entries whose capability record
   says caching is explicit, mark the end of `static` and the end of `session`. Add a catalog
   capability field `cache: none|automatic|explicit` with `unknown` as an explicit state (I-14).
   Do not send markers to endpoints not qualified for them; record rejection and fall back without
   markers (Claude Code's own fallback when a gateway rejects the marker is the same shape).
5. **Switch-cost term in routing (advisory arithmetic, not a model judgment).** When routing
   considers a candidate different from the model that produced the last response *within the
   provider's cache lifetime*, add `uncached_history_tokens × (input_rate − cache_read_rate)` of the
   *current* model as the opportunity cost of leaving, and price the candidate's first request at
   full uncached input. This only changes the deterministic cost comparison P6-02 already performs.
   Escalation for capability reasons still wins; the term just stops cost-motivated flapping.
6. **Reservation accuracy.** The admission path currently reserves with cache read/write bounds set
   to full input (seen in `routing/selection.rs` and worker code). Keep that conservative bound —
   it is correct for I-04 — but record predicted-vs-settled cache share so CC-10 can show it.
7. **Fan-out stagger.** When the controller starts k children with identical model, tool set and
   operating prefix, release the first, hold the rest until its first response bytes arrive or a
   small cap elapses, then release together. Applies to CR-03 helpers, CR-07 best-of-N *within one
   group*, and CC-13.
8. **Warm-cache confirmation.** Interactive `/model`, `/groups` or profile changes while the last
   request is younger than the catalog's cache lifetime show the estimated one-time cost before
   applying. Non-interactive runs never prompt; they log the estimate.

**Contracts.** Manifest fields `volatility`, `prefix_digests`; event `context.prefix_changed`;
catalog field `cache`; routing explanation gains `switch_cost` line (vcp-what 7.5 already requires
explanations); config `[routing] cache_affinity = on|off` (off = today's behaviour, for disabled
parity).

**Invariants.** I-04/I-05 unchanged (bounds stay conservative). I-09: reordering is a projection
change, never an audit rewrite. I-14: unknown cache capability is explicit. Preservation rule 3:
changing part order changes serialized requests, so this is a **new context-assembly revision**;
frozen P5-08/P6-04 comparisons stay bound to the old one.

**Acceptance.** (a) Byte-identical prefix across 20 turns of a recorded trace with memory and
evidence churn. (b) Each enumerated cause produces exactly one `prefix_changed` event with the right
cause. (c) Live, budgeted: same task with affinity on/off on one implicit-cache and one
explicit-cache endpoint; report cached-input share and settled cost. (d) Routing fixture where a
marginally cheaper candidate is *not* chosen mid-task because switch cost exceeds the saving, and
*is* chosen after cache lifetime expiry. (e) Disabled parity per plan 22 rule 2.

**Open.** Whether explicit breakpoints are accepted on the endpoint shape VCP uses (section 2);
per-provider cache lifetimes as catalog data; whether reasoning-setting changes invalidate on each
qualified endpoint (Claude Code documents that this differs by model).

---

### CC-02 — Long-running and background process lifecycle

**Pattern.** Section 3.2. S3.

**Verify first.**
```powershell
rg -n "120_000|timeout_ms" src/crates/vcp-tools/src/process.rs src/crates/vcp-tools/src/verification.rs
rg -n -i "background|detach|long.running" src/crates/vcp-tools/src src/crates/vcp-engine/src docs/plan/06-windows-tools-and-recovery.md
```
At the read revision both `vcp_exec` and the verification runner are capped at 120 000 ms and
1 MiB of output. A cold `cargo build`, a .NET solution build, or an integration suite exceeds that
routinely, so honest verification (P2-06, I-12) of real projects reports "could not run".

**Proposed design.**
1. **Two ceilings, both explicit in the process profile**, not in the model's request:
   `foreground_ms` (default stays 120 s) and `lifetime_ms` (profile-declared, bounded by the task
   deadline). The model may request less, never more.
2. **Timeout transfers, it does not kill**, when the profile allows it: at `foreground_ms` the
   execution becomes a *background execution* owned by the same task; the tool result states that
   explicitly, with execution ID and output artifact reference. Profiles that do not opt in keep
   today's terminate-at-timeout behaviour.
3. **Explicit background start** for servers and watchers (`mode: "background"` on profiles that
   permit it). `process.read` (vcp-what 9.1) gives ranged reads of the spooled output;
   `process.cancel` stops it.
4. **Lifetime rules, enforced by the controller, not the model:** a background execution ends when
   its owning task completes, is cancelled, or pauses (I-10, I-18: `/pause` stops descendant work);
   a helper child's executions end with the child; structured non-interactive runs end them at final
   result. Job Object ownership (ADR-005, P0-05) already gives tree termination.
5. **Recovery.** A background execution is an unresolved effect while it lives. After owner loss it
   is reconciled like any other started effect (I-11): never blindly restarted; reported as
   terminated/unknown.
6. **Verification.** `vcp_verify` checks declare their own `lifetime_ms`; a check still running at
   task completion time is `not_finished`, distinct from failed and from could-not-run (9.5).
7. **Output ceiling.** Spool to an artifact without the 1 MiB truncation for background and
   long executions; return head/tail excerpts plus ranges, consistent with 9.4. Keep a hard disk
   ceiling and kill on breach, reported as such.

**Invariants.** One scheduler (preservation rule 5). I-02: the transfer to background is part of the
originally authorized effect, not a new one; authority is not widened. I-05: no model cost while a
process merely runs. I-17: spooled output has a retention selector.

**Acceptance.** Native Windows: 5-minute build fixture completes under verification; a server
profile started in background survives three turns and dies on `/pause`, on task completion, and on
console close (extend E07/E08/E10); forced kill of the CLI leaves no orphan process tree; disabled
parity for profiles without the new fields.

---

### CC-03 — Process result interpretation on native Windows

**Pattern.** Section 3.2, PowerShell details. S3.

**Verify first.**
```powershell
rg -n -i "exit_code|ExitStatus|from_utf8|lossy|stdin|ansi" src/crates/vcp-tools/src src/crates/vcp-engine/src/command_handler.rs
```

**Proposed design.** A small, table-driven **result interpreter** applied after the raw outcome is
durably recorded (the raw exit code and bytes are never altered):
1. `interpretation: ok | negative_answer | failure | could_not_run`, derived from
   `(profile.executable basename, exit_code)`. Ship a short VCP-authored table for search/compare
   tools where a non-zero code means "no match" or "files differ", including the Windows natives
   (`findstr`, `where`, `fc`, `robocopy` bands). Author the table from each tool's own
   documentation; do not copy Claude Code's list (preservation: no default lists imported).
   Profiles may override.
2. **Encoding contract per profile:** `stdout_encoding = utf8 | oem | utf16le | auto`; decode
   deterministically, record the decision and any replacement characters; for PowerShell profiles,
   set UTF-8 input/output encoding in the generated invocation rather than relying on host defaults.
3. **stdin policy:** when `input` is null, attach a closed stdin so children that wait on input get
   end-of-file instead of hanging until timeout.
4. **Control sequences:** strip ANSI/VT sequences from the *model-facing* text for pipe-mode
   profiles; keep raw bytes in the artifact. PTY profiles are unaffected (9.4).

**Why it matters.** Without (1) the model reads "command failed" for a successful empty search and
retries or escalates — a cost and stall source that the Markov stall signals would then have to
detect after the fact. (2)–(4) are recurring native-Windows papercuts that Claude Code fixed in
successive releases; VCP's Windows-first stance makes them first-order.

**Acceptance.** Fixture matrix on Windows PowerShell 5.1 and 7: non-ASCII paths and output, a child
blocking on stdin, coloured compiler output, each table entry; raw artifact bytes identical with and
without the interpreter; P8-01 matrix gains these cases.

---

### CC-04 — Ranged reads and pattern discovery

**Pattern.** Section 3.3. S3.

**Verify first.** `rg -n "vcp_read|vcp_list|max_bytes|offset" src/crates/vcp-tools/src/schema.rs src/crates/vcp-tools/src/read.rs`

**Proposed design.** Land in the **same tool-schema revision as CR-02a** so the catalog-change cost
(preservation rule 3: digest change, stale approvals, token delta, comparison rebinding) is paid
once.
1. `vcp_read` gains optional `start_line`, `line_count`. Whole-file reads beyond the byte/token
   ceiling return the first page plus `partial: {returned_lines, total_lines, next_start_line}`
   instead of failing, keeping the existing version/hash in the result. An explicit range that
   cannot fit is an error that says so — never a silent truncation (I-14).
2. `vcp_find { pattern, root?, max_results }`: glob over the *same* discovered, ignore-filtered,
   authorized file set that `vcp_search` uses; sorted by modification time; `complete: bool` and
   `exclusions` as today. This replaces multi-call directory walks with one bounded call.
3. A `partial` read does **not** satisfy the version precondition for a later `vcp_patch` on lines
   outside the returned range — VCP's exact-match-plus-version rule (9.3) already gives the safety
   Claude Code gets from read-before-edit; keep it and make the error actionable.
4. Images/PDF/notebooks: not selected (plan 22 already left image reads unselected).

**Acceptance.** Default calls without the new fields are byte-identical to today; 50 MB log file is
readable by pages; CRLF and non-UTF-8 files report encoding rather than mis-splitting lines;
`vcp_find` never returns a path `vcp_read` would refuse.

---

### CC-05 — Structured work list that survives compaction  *(evidence-gated)*

**Pattern.** Section 3.4. S3. **Read the caveat:** Claude Code *removed* these tools from the
default set for its newest models because they cost context and strong models do not need them.

**Proposed design.** A task-scoped `work_items` record (id, text, status, depends_on, evidence
refs) owned by the engine, exposed through one compact tool, rendered in `/status`, and carried as a
protected field through compaction (it fits 6.5's "unresolved work"). It is *not* the CR-05 plan
artifact: the plan is a user-approved document; the work list is the agent's running checklist.

**Gate.** VCP routes to many model groups through OpenRouter, including cheaper ones. Enable per
model group only where a held-out comparison on the U01–U03 task set shows fewer dropped subtasks or
lower cost; default off. Expect it to help `low` profile groups and not frontier ones.

---

### CC-06 — User-directed range compaction and cold-resume choice

**Pattern.** Sections 3.5, 3.1 (resume from summary). S5, S9.

**Verify first.** `rg -n "keep_recent_pairs|pub fn compact" src/crates/vcp-context/src/compaction.rs`
VCP compaction is deterministic preview-plus-artifact-reference, keeping the most recent complete
pairs. That is stronger for audit than a model summary, but the *only* control is "how many recent
pairs".

**Proposed design.**
1. `/compact from <turn>` and `/compact through <turn>`: collapse a user-chosen contiguous range
   into previews, leaving both earlier and later turns intact. Typical use: a long debugging detour
   in the middle of a task. Same safety rules as today (never split a tool call/result pair; never
   compact protected fields; refuse when gain is insufficient).
2. `/compact keep <selector>`: pin specific turns or artifacts as non-compactable for the task.
3. **Cold-resume choice.** On `vcp resume` of a task whose last request is older than the cache
   lifetime *and* whose history exceeds a threshold, offer "resume compacted" vs "resume as is",
   with the estimated first-request cost of each (uses CC-01's arithmetic). Non-interactive resume
   takes a flag; default is unchanged behaviour.
4. Every directed compaction is a new projection linked to its source range (I-09) and is
   recoverable through CR-13's history recall tool.

**Acceptance.** Extends P2-08 fixtures: mid-range collapse preserves corrections, effects, costs and
pending pairs; pinned turns survive automatic compaction; resume estimate within tolerance of the
settled charge.

---

### CC-07 — Post-compaction rehydration set

**Pattern.** Section 3.6. S6.

**Problem.** After compaction the model has previews of old tool results but may no longer hold the
*current* content of the files it is editing, the active plan, or the active skill procedure. It
then re-reads by trial and error, which costs requests, or acts on stale previews.

**Proposed design (deterministic, bounded).** Immediately after a compaction, the assembler adds a
`rehydration` group of `turn`-class parts, each with an inclusion reason:
- the CR-05 plan artifact, if one is bound to the task;
- up to *N* files the task has changed or has pending changes against, **current version from
  disk**, most recently changed first, each under a per-file token cap; over-cap files become a
  path + version + symbol outline reference (this is where M5's co-change/map work can rank);
- bodies of currently active skills under per-skill and total caps, oldest first to drop;
- a one-line reminder per live background execution (CC-02) and live child, so the model does not
  start duplicates;
- active session boundaries (CC-09).
Everything is attributed data with versions (I-08), counted in the token plan (6.3), and skipped if
it would consume the budget needed to report or verify (6.6 rule).

**Acceptance.** Recorded long task: requests-to-next-successful-edit after compaction, with and
without rehydration; no rehydrated file content is older than the on-disk version at send time
(send fence already revalidates); disabled parity.

---

### CC-08 — Code-execution-adjacent protected paths and destructive-removal circuit breaker

**Pattern.** Section 3.7. S7. **Fold into CR-10a's verify-first checklist.**

**Verify first.** Plan 22 already recorded that `vcp-tools` rejects any `.git` path component.
Check the rest: `rg -n -i "protected|\.vscode|\.husky|npmrc|hooks" src/crates/vcp-policy src/crates/vcp-tools/src`

**Proposed design.** Two restrictive-only policy defaults (new policy revision, per plan 22).
1. **Execution-adjacent write guard.** A VCP-authored classification of paths whose content is
   *executed or sourced by some other tool later*: VCS config and hook directories, git-hook manager
   config, shell startup files, package-manager and build-wrapper config that can name scripts or
   registries, IDE task/launch config, devcontainer config, MCP and agent config, CI workflow
   directories, and VCP's own `.vcp/**` policy/config. Writes to these always require explicit
   approval, even under `autonomous`, and **cannot be pre-granted by a repository-supplied file**.
   Author the list from each ecosystem's documentation and keep it as reviewed data with an owner;
   do not copy another product's list.
2. **Destructive-removal circuit breaker.** For process profiles that can delete (shell,
   PowerShell, `git clean`, package-manager cache purges): deny, in every autonomy level, removal
   whose resolved target is a drive/filesystem root, a top-level directory, the user profile, the
   workspace root or any ancestor of it, or a bare wildcard directly under a variable that could be
   empty. VCP's argv-vector preference (9.4) makes this far more tractable than parsing shell text;
   for explicit shell profiles, treat unparseable deletion commands as `ask`.

**Invariants.** Restrictive only; deny-over-allow precedence untouched; newly affected operations
are listed for owner review (plan 22's "Existing implementation" row for `vcp-policy`).

**Acceptance.** Red-team fixtures: patch that adds a post-checkout hook, edits an npm config to add
a pre-install script, rewrites a PowerShell profile; removal with an empty variable; removal via
nested subexpression. All end in `ask` or `deny` under every preset.

---

### CC-09 — Explicit session boundaries as deterministic denials

**Pattern.** Section 3.7, last paragraph. S8. Claude Code admits the weakness: conversational
boundaries are re-read from the transcript by a classifier and can be lost to compaction.

**Proposed design.** Make the boundary a **record, not a sentence**:
`/forbid <effect-class | profile | path-glob | mcp:server[/tool]> [until <turn|task-end>]` and
`/allow-again <id>`. Stored as a task-scoped restrictive policy overlay, evaluated by the ordinary
decision order (10.2), shown in `/permissions`, injected as a short `session`-class context part so
the model plans around it, protected from compaction, inherited (never relaxed) by children.
No natural-language inference is involved; if the user merely *says* "don't push", the terminal can
suggest the command, but only the command creates the rule.

**Acceptance.** A forbidden profile is denied after compaction, after pause/resume, and inside a
child; removing the overlay requires the user, not the model (I-01).

---

### CC-10 — Usage attribution by extension, cache statistics and behaviour flags

**Pattern.** Section 3.12. S9.

**Proposed design.** Read-only projections in `vcp-audit`, exposed by `vcp inspect --view costs`
and `/cost`:
- **Attribution:** share of settled cost by child role, skill, MCP server, tool family, and model
  group. Use Claude Code's corrected rule: a request is attributed to an MCP server only if it
  consumed one of that server's results, not merely because the server was called earlier.
- **Cache line:** requests, cached-input share, prefix changes by cause (CC-01), warm/cold.
- **Behaviour flags:** any single behaviour above a threshold share — long context, prefix churn,
  retries, compaction overhead, helper overhead, not-run verification.
- Feeds `/optimize` (P6-05) as evidence rows; grants nothing.

**Invariants.** Aggregates obey ADR-040 provenance and retention rules; pruned history is labelled,
not silently shrunk.

---

### CC-11 — Language-server code intelligence

**Pattern.** Section 3.3 / tools reference: an LSP tool that, after each file edit, reports type
errors and warnings automatically, and offers definition, references, hover type, document/workspace
symbols, implementations and call hierarchy; inactive until a per-language plugin supplies server
configuration; the user installs the server binary. S3, S15.

**Relation.** Plan 22 folded CR-15 into M6 and noted its low-confidence source. This is a
**high-confidence, current source for the same idea**, so M6 can cite it. It also offers a second,
separable capability (navigation) that competes with CR-01's semantic index and M5's map for the
"find the right code cheaply" job.

**Proposed design.** (a) *Diagnostics:* keep plan 22's decision — a fast diagnostic is a cheap check
class inside verification ordering (M6). A language server is one *provider* of that check class,
beside compiler-check commands. (b) *Navigation:* `code.definition|references|symbols` as read-only
tools backed by a language server started as an explicit, granted, long-lived process profile
(needs CC-02). Evidence-gated: run as a fourth arm in CR-01's frozen comparison (lexical, map,
semantic, LSP). Server binaries are user-installed executables under normal profile policy; VCP
ships no servers.

**Risks.** Native Windows server availability and memory footprint; stale server state after
external edits (must key results to file versions, I-08); startup latency.

---

### CC-12 — Bounded event watch  *(deferred until CC-02)*

**Pattern.** Monitor tool: run a watcher in the background; each output line becomes an event the
agent reacts to; every watch has a deadline (default 5 min, max 30; shorter in one-shot
non-interactive runs); same permission rules as the shell. S3.

**VCP shape.** A background execution (CC-02) plus a line-to-event bridge with debounce, a hard
deadline, a per-watch event cap, and a budget reservation for the turns it may trigger. Useful for
"wait for CI" and "watch the dev-server log while I edit". Without a hosted service, the WebSocket
source is out. Low priority.

---

### CC-13 — Re-runnable orchestration recipes for large fan-outs  *(evidence-gated)*

**Pattern.** Section 3.8 dynamic workflows. S13.

**VCP shape.** VCP already has the right primitive: a task graph with node objective, dependencies,
read/write sets, base revision, acceptance checks, depth and allocation (16.1). A *recipe* is a
**saved, parameterized task-graph template** — declarative data, not a script runtime:
`for_each(file_set) → child(role, objective template, checks) → reduce(child results)`, with
optional `review_by(other role)` and `until(check passes | list stops growing, max_rounds)`.
- Stored under `.vcp/recipes/`, versioned, diffable, re-runnable with inputs; a model may *draft*
  one, the user approves it as they would a plan (CR-05); a recipe is attributed data and grants
  nothing.
- Runner limits copied in spirit: max concurrent children, max items per fan-out (reject, never
  silently cap), max children per run, advisory large-run warning from projected tokens, no mid-run
  input, resumable because each child result is already durable (16.3).
- Budget: admit the whole projected fan-out against the root first (I-04/I-05); if only k of N fit,
  say so and ask.
- CR-07 best-of-N becomes one built-in recipe; CC-01's stagger applies.

**Why gated.** It multiplies spend and needs P7-04..06 qualified first. Start with a read-only audit
recipe ("check every file under X for pattern Y, verify each hit, one report").

---

### CC-14 — Verified-findings pipeline and structured finding records

**Pattern.** Section 3.10. S16, S17. **Extends CR-12a; does not replace it.**

**Add to CR-12a (deterministic parts, select):**
- A `Finding` record: path, range, category, severity, **failure scenario**, evidence refs,
  `introduced_by_change: yes|no|unknown` (the "pre-existing" distinction), verification status.
  Emitted through a structured tool rather than prose so the CLI renders it and JSONL carries it.
- Final machine-readable summary line in structured output for CI callers; the process exit code
  does not depend on findings unless the caller asks (neutral by default).
- `introduced_by_change` is computed from the diff hunks, not judged.

**Verification stage (evidence-gated):** candidates from the reviewer go to a second read-only
helper — different model group when available (CR-03 `verifier`) — that must *demonstrate* the
failure: a failing test it writes in a scratch worktree, a reproduced command, or a cited code path
that contradicts the claim. Unverified candidates are reported separately as suggestions, which
vcp-what 15.6 already requires. Gate on U02 seeded-defect precision/recall and cost.

---

### CC-15 — Worktree lifecycle refinements  *(fold into CR-08)*

**Pattern.** Section 3.9. S14, S15.

1. **Ownership marker.** Write a VCP marker into each created worktree's git metadata; cleanup
   touches only marked worktrees. This complements the P7-04 rule "never delete from a guessed path"
   with a positive proof of ownership, and matches a bug Claude Code fixed (a sweep that removed a
   user-made worktree).
2. **`git worktree lock` while a child runs**, released at child terminal state; reconcile stale
   locks on resume by checking the child's canonical state, not by force-unlocking.
3. **Include file with gitignore syntax** for untracked files to copy (CR-08 proposed an explicit
   copy list; the gitignore-syntax file is the de-facto convention and P10-02 could later read the
   foreign one). Same rule as CR-08: contents stay out of capture (10.4).
4. **Base selection is explicit:** `base = recorded parent commit + dirty snapshot` stays VCP's
   default (16.2). Offer "default branch" only as a named option; never silently branch from
   somewhere other than what the parent saw.
5. **Sparse checkout for large monorepos:** child materializes only the node's read/write set plus
   declared build roots. Measure on the P8 large fixture before adopting.

---

### CC-16 — Schema-validated final output for structured runs

**Pattern.** Section 3.14. S22.

**Proposed design.** `vcp run --format jsonl --result-schema <file>`: the final result event carries
a `value` validated against a bounded JSON-Schema profile (reuse the closed, exact profile from
ADR-028 rather than claiming general JSON-Schema compliance). Invalid schema → exit before any
provider call. Validation failure → bounded retries charged to the task, then a distinct exit
condition (extend 17.3's ordered conditions). Useful for CI callers, for CC-13 reducers, and for
CC-14's summary.

---

### CC-17 — Context-inheriting child (fork)  *(deferred with CR-11b)*

**Pattern.** A subagent that inherits the full conversation instead of a fresh brief, so no
re-explaining and — importantly — it **reuses the parent's cached prefix**. S11.

**Note for when CR-11b / P7-06 read-only children land.** VCP children receive a bounded
`ChildSpec`; that is right for isolation and cost on small briefs. For side questions about the
current work, a read-only child built from the parent's *sealed manifest* is cheaper on
cache-capable endpoints (same model, same prefix) than a fresh brief that re-reads files. Decide per
child by estimated tokens: `inherit` when (brief + expected re-reads) > cached parent prefix cost.
Deterministic arithmetic from CC-01; no judgment.

---

### CC-18 — Memory deltas  *(owner decision)*

VCP's governed memory is stronger than Claude Code's note files. Three small ideas only:
1. **Bounded always-present digest.** Claude Code always loads a tiny index and fetches detail on
   demand. VCP retrieves by query; a query can miss a standing convention ("use pnpm"). Consider a
   small, size-capped `session`-class part of *accepted, undisputed, high-use* convention claims per
   workspace, selected deterministically (claim class + use count + recency), inspectable like any
   part. Compare against retrieval-only on the P5-08 recall set.
2. **Do-not-store-derivable rule.** Claude Code skips facts derivable from the code. VCP's automatic
   extraction (A08) could label such claims `derivable` and rank them below non-derivable ones
   rather than dropping them; measure index size and recall noise.
3. **User-scope preferences across workspaces.** Claude Code has a user layer. VCP's A07 confines
   memory to the workspace. A user-level *instruction* file already exists conceptually (6.2 user
   instructions); a user-level *memory* scope would change A07 and ADR-008 and needs the owner.

## 6. Additions to the shared rules

Beyond plan 22's preservation contract:

1. **Bundle model-visible schema changes.** CC-04, CR-02a, CC-05, CC-14's finding tool and CC-11's
   navigation tools all change the tool catalog. Group them into as few catalog revisions as
   dependency order allows.
2. **Context-assembly revisions are as consequential as schema revisions.** CC-01, CC-06, CC-07 and
   CC-09 change serialized requests. Land CC-01's ordering first, as its own revision, so later part
   additions are measured against a stable-prefix baseline.
3. **Process lifetime belongs to the controller.** Nothing in CC-02/11/12 may outlive its owning
   task, survive `/pause`, or restart on a status read.
4. **Authored data, not borrowed lists.** Exit-code tables (CC-03) and protected-path
   classifications (CC-08) are VCP-authored, reviewed data with an owner and tests.
5. **Windows is the reference platform for every process-facing item** (CC-02, CC-03, CC-08, CC-11,
   CC-12), on both PowerShell 5.1 and 7 where a shell is involved.

## 7. Suggested sequencing (input to a plan 23, not a plan)

| Wave | Items | Rationale |
|---|---|---|
| With plan 22 wave A | CC-15 → into CR-08 | Same code, same PR |
| With plan 22 wave B | CC-04 → with CR-02a; CC-08 → into CR-10a | One catalog revision; one policy revision |
| Next | CC-02, CC-03 | Unblocks honest verification of real projects on Windows; prerequisite for CC-11/12 and for meaningful P8-01 results. Arguably the highest-value pair in this note |
| Next | CC-01, then CC-10 | Direct, measurable cost effect given VCP's multi-model routing; CC-10 makes it visible |
| Then | CC-06, CC-07, CC-09 | Context-continuity polish on the stable-prefix baseline |
| With plan 22 wave D | CC-14 deterministic parts → into CR-12a; CC-16 | Review and CI ergonomics |
| Evidence-gated | CC-05, CC-11 navigation, CC-13, CC-14 verification stage | Need held-out comparisons or P7-06 |
| Deferred / owner | CC-12, CC-17, CC-18 | As marked |

ADR candidates (take the next free number at authoring time): cache-aware assembly and switch-cost
routing (CC-01); background execution semantics as an amendment to ADR-004 (CC-02); recipe records
and runner limits (CC-13); language-server host if adopted (CC-11).

## 8. Sources

Official Claude Code documentation, `https://code.claude.com/docs/en/<page>` (index S1), read
2026-09-21:

| Key | Page | Key | Page |
|---|---|---|---|
| S1 | `/docs/llms.txt` (index) | S12 | `agents`, `agent-teams`, `cross-session-messaging` |
| S2 | `prompt-caching` | S13 | `workflows` |
| S3 | `tools-reference` | S14 | `worktrees` |
| S4 | `sandboxing` | S15 | `large-codebases` |
| S5 | `checkpointing` | S16 | `code-review` |
| S6 | `context-window` | S17 | `ultrareview` |
| S7 | `permission-modes` | S18 | `goal` |
| S8 | `permissions`, `auto-mode-config` | S19 | `advisor` |
| S9 | `costs`, `sessions` | S20 | `memory` |
| S10 | `how-claude-code-works`, `features-overview` | S22 | `headless`, `cli-reference` |
| S11 | `sub-agents` | | |

Other: **S21** OpenRouter prompt caching and provider sticky routing —
`https://openrouter.ai/docs/guides/best-practices/prompt-caching`.
**V1** VCP repository at the front-matter revision: `docs/plan/22-cursor-improvements.md`,
`docs/research/cursor.md`, `docs/architecture/othertools.md` §3 and §16.1,
`docs/architecture/vcp-what.md`, `docs/plan/21-markov-integration.md` (M5, M6), and the crates
named in section 2.