```promptused
Read [iokaio/vcp: Vibe Code Pro - An Experiment](https://github.com/iokaio/vcp) and then gather everything you can find about how Cursor works and write an architecture design addendum on what features of Cursor might be incorporated into vcp that vcp does not already have. Write it in a markdown file that would be easy for an AI agent to consume in planning and implementing the features.
```

---
doc_id: vcp-addendum-cursor-derived-increments
title: "Architecture addendum: Cursor-derived increments (CR-01 … CR-16)"
status: proposed            # nothing here is confirmed, qualified, or implemented
authority: research input   # vcp-what.md and owner decisions take precedence
suggested_path: docs/architecture/cursor-derived-increments.md
suggested_plan_path: docs/plan/22-cursor-derived-increments.md
vcp_revision_read: b7742e594a1b502097246812f8ac8a2cf69468b7   # main, 2026-09-21
cursor_evidence_date: 2026-09-21   # docs at cursor.com/docs (Cursor 3.x era) plus engineering blog
reuse_form: pattern adoption only  # Cursor is closed source; no Cursor code, prompts, or doc text is imported
id_namespace: CR-nn                # unused elsewhere in the repo; does not collide with C/G/E/M/R/U/FR/I/P ids
---

# Architecture addendum: Cursor-derived increments

Repository status note (2026-09-21): this research preserves observations from
`b7742e5`. P7-04 through P7-06 are now in progress with a merged delegation
increment. The [proposed planning supplement](../plan/22-cursor-improvements.md)
records corrections and possible sequencing; neither document adopts new release
requirements or supersedes the current dependency ledger.

## 0. How an agent should use this file

1. This file is a **gap analysis and design proposal**. It does not create work items, change the
   68-item ledger, or change release scope. Follow the precedent of
   `docs/plan/21-markov-integration.md`: increments attach to **existing owning task IDs** where one
   exists; anything that cannot be owned by an existing task is marked `needs_owner_decision`.
2. Each increment in section 5 is **self-contained**. Per `AGENTS.md` section 2 (minimize context), read
   the index in section 1, then read only the one `CR-nn` section you are implementing plus the
   shared rules in section 6.
3. Every increment starts with a **Verify first** step. The "VCP today" statements were derived from
   reading `README.md`, `AGENTS.md`, `docs/architecture/vcp-what.md`, `docs/plan/README.md`,
   `docs/plan/20-traceability.md`, targeted greps across `docs/`, and a shallow look at
   `src/crates/vcp-tools` and `src/crates/vcp-repository`. Source code was **not** audited. If the
   verify step shows the capability already exists, record that in the PR and drop or shrink the
   increment.
4. Cursor behavior described here comes from public documentation and engineering posts for a
   closed-source product. Treat it as a description of observable behavior and published design,
   not of internal topology. `docs/architecture/othertools.md` section 16.6 already sets that rule.
5. Status vocabulary matches `vcp-what.md` section 0: *Confirmed requirement*, *Proposed baseline*,
   *Open decision*, *Deferred*. Everything below is **Proposed** or **Open** unless stated.

## 1. Machine-readable index

```yaml
increments:
  - id: CR-01
    title: Whole-workspace local code index with Merkle change detection
    priority: P1
    release_scope: post_first_release      # optional pull-forward: see section 7
    owning_tasks: [P5-03, P5-04, P5-05, P5-06, P2-01]
    crates: [vcp-repository, vcp-embedding, vcp-search-tantivy, vcp-search-diskann, vcp-context]
    depends_on: []
    adr_candidate: ADR-042
  - id: CR-02
    title: Regex search tool backed by a local sparse n-gram index
    priority: P1
    release_scope: post_first_release      # regex-without-index part is small and could land earlier
    owning_tasks: [P2-04, P5-03]
    crates: [vcp-tools, vcp-repository]
    depends_on: []
    adr_candidate: ADR-043
  - id: CR-03
    title: Built-in context-isolating helper roles (explore, shell-runner, verifier)
    priority: P1
    release_scope: fold_into_in_flight     # P7-04..06 are still Planned
    owning_tasks: [P7-04, P7-06, P6-02]
    crates: [vcp-engine, vcp-routing, vcp-extensions]
    depends_on: []
    adr_candidate: none (extends ADR-010)
  - id: CR-04
    title: Agent-change checkpoints and file rewind
    priority: P1
    release_scope: post_first_release
    owning_tasks: [P2-04, P3-01, P3-02]
    crates: [vcp-tools, vcp-repository, vcp-engine, vcp-cli]
    depends_on: []
    adr_candidate: ADR-044
  - id: CR-05
    title: Plan as a durable, editable, revision-bound artifact
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P3-02, P2-05]
    crates: [vcp-engine, vcp-cli, vcp-context]
    depends_on: []
    adr_candidate: none
  - id: CR-06
    title: Evidence-first debug workflow with tracked instrumentation
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P7-02, P2-06]
    crates: [vcp-extensions, vcp-tools, vcp-engine]
    depends_on: [CR-04]
    adr_candidate: none
  - id: CR-07
    title: Best-of-N across model groups in isolated worktrees, feeding routing evidence
    priority: P1
    release_scope: post_first_release
    owning_tasks: [P7-04, P7-05, P6-05]
    crates: [vcp-engine, vcp-routing, vcp-budget, vcp-cli]
    depends_on: [CR-08]
    adr_candidate: ADR-045
  - id: CR-08
    title: Worktree setup recipes and bounded worktree cleanup
    priority: P1
    release_scope: fold_into_in_flight
    owning_tasks: [P7-04]
    crates: [vcp-repository, vcp-exec, vcp-policy]
    depends_on: []
    adr_candidate: none (extends ADR-010)
  - id: CR-09
    title: Scoped instruction fragments (glob-attached, description-selected, manual)
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P2-01, P7-01, P10-02]
    crates: [vcp-context, vcp-extensions]
    depends_on: []
    adr_candidate: none (extends ADR-011 / ADR-014)
  - id: CR-10
    title: Advisory approval triage ("auto-review") under bounded semantic decisions
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P2-03, P6-02]
    crates: [vcp-policy, vcp-decision]
    depends_on: []
    adr_candidate: ADR-046
    needs_owner_decision: true
  - id: CR-11
    title: Input queue, side sessions, and long-lived goals
    priority: P3
    release_scope: post_first_release
    owning_tasks: [P3-02, P2-05]
    crates: [vcp-engine, vcp-cli]
    depends_on: []
    adr_candidate: none
  - id: CR-12
    title: Local change review with patch-identity dedupe, incremental scope, and learned review rules
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P7-05, P7-02, P5-01]
    crates: [vcp-extensions, vcp-memory, vcp-engine, vcp-cli]
    depends_on: []
    adr_candidate: none
  - id: CR-13
    title: Remaining dynamic-context-discovery pieces (history recall tool, MCP status in context)
    priority: P2
    release_scope: post_first_release
    owning_tasks: [P2-08, P7-03]
    crates: [vcp-context, vcp-tools, vcp-extensions]
    depends_on: []
    adr_candidate: none (extends ADR-009)
  - id: CR-14
    title: Retrieval relevance learning from local traces
    priority: P3
    release_scope: post_first_release
    owning_tasks: [P5-06]   # aligns with Markov M5 context/retrieval follow-up
    crates: [vcp-memory, vcp-audit]
    depends_on: [CR-01]
    adr_candidate: ADR-047
  - id: CR-15
    title: Post-edit diagnostics before verification
    priority: P3
    release_scope: post_first_release
    owning_tasks: [P2-04, P2-06]
    crates: [vcp-tools, vcp-exec]
    depends_on: []
    adr_candidate: none
  - id: CR-16
    title: Evaluate Agent Client Protocol (ACP) as the deferred editor-facing protocol
    priority: P3
    release_scope: deferred
    owning_tasks: [P9-01, P4-01]
    crates: [vcp-protocol]
    depends_on: []
    adr_candidate: amend ADR-002
```

## 2. Evidence base and caveats

| Item | Note |
|---|---|
| Cursor source visibility | Closed source. The editor is a VS Code fork; only an old CodeMirror prototype and a prompting library were ever published. All statements below rest on sources S1–S17 in section 8. |
| Cursor evidence age | Documentation fetched 2026-09-21 (docs reference Cursor 3.5–3.11). Blog posts dated Nov 2025 – Mar 2026. Two items (CR-15, fast-apply note in section 4) rest on older 2024 posts that were **not re-fetched**; they are marked low confidence. |
| Documentation inconsistency | Cursor's current search page (S3) describes Instant Grep as a purely local index, while the indexing post (S4) and older docs describe a server-side embedding index. Both appear to exist; this file treats them as two separate subsystems. |
| VCP read scope | See section 0 item 3. The ledger at the read revision shows P0–P3, P5, P6, P7-01..03 complete for bounded acceptance and **P7-04..06 and P8 Planned**. |
| Implemented search tool at read revision | `src/crates/vcp-tools/src/schema.rs` exposes `vcp_read`, `vcp_list`, `vcp_search`, `vcp_patch`; `vcp_search` is described there as **literal text search over bounded discovered files**. The architecture's `search.text` / `search.symbols` family is broader than what is wired today. |

## 3. Cursor architecture digest (what is being mined)

Short paraphrased summaries. Source keys refer to section 8.

**3.1 Two retrieval systems.** (a) *Semantic index*: on workspace open, the client hashes every
file into a Merkle tree (file hashes roll up into directory hashes), diffs that tree against the
server's copy, and re-processes only diverging branches. Changed files are split into syntactic
chunks; embeddings are cached by chunk content so unchanged chunks are never re-embedded;
embedding runs asynchronously; search unlocks at roughly 80% index completion. A similarity hash
of the tree lets a new clone start from a teammate's near-identical index, with per-file content
proofs filtering out results the client cannot prove it holds (S4). Cursor reports about 12.5%
better question-answering accuracy from semantic + grep together versus grep alone, largest on
repositories over 1,000 files, and trains its embedding model on agent session traces ranked by an
LLM (S5). (b) *Instant Grep*: a local, client-side index using sparse n-grams (variable-length
n-grams chosen by deterministic character-pair weights), memory-mapped lookup tables plus posting
files; regexes are decomposed into required n-grams, candidates are intersected, and the real
regex engine runs only on the survivors. Motivation: ripgrep scans exceeding 15 s on large
monorepos stalled agents (S3, S6).

**3.2 Agent loop and tools.** Instructions + tools + model, tuned per model. Tools: file/dir
search, grep, semantic search, read (including images), edit, shell, web search, rule fetch,
browser, image generation, and a non-blocking ask-question tool (S1).

**3.3 Checkpoints.** Before significant changes the agent snapshots the files it is about to modify.
Restoring reverts files only, leaves the conversation intact, is stored locally, and is explicitly
separate from Git (S1). CLI has `/rewind` and `/fork` (S11).

**3.4 Input handling.** Messages typed during a run either queue (processed after the turn,
reorderable) or steer (delivered at the next tool-call boundary without aborting in-flight work).
Side chats are durable sessions that use the parent as hidden reference context. `/goal` sets a
long-lived objective pursued until complete (S1).

**3.5 Modes.** Agent, Ask (read-only), Plan (clarifying questions → research → editable markdown
plan → build; advice to revert and refine the plan instead of patching a bad run), Debug
(hypotheses → log instrumentation to a local collector → user reproduces → analyze logs → minimal
fix → remove instrumentation) (S7, S8, S10).

**3.6 Subagents.** Each has its own context window; foreground or background; built-ins are
Explore (cheaper/faster model, many parallel searches, returns a summary), Bash (isolates verbose
command output), Browser. Custom subagents are markdown + frontmatter (`name`, `description`,
`model`, `readonly`, `is_background`); one nesting level below direct subagents; resumable by ID;
optional per-subagent worktree isolation (S9).

**3.7 Worktrees and best-of-N.** Per-agent Git worktrees; `.cursor/worktrees.json` holds
OS-specific setup commands or script paths with a root-worktree path variable; interval cleanup
capped at a machine-wide count (default 25). `/best-of-n <models> <task>` runs one task across
several models, each in its own worktree, compares, and leaves merging to the user (S12).

**3.8 Approval model.** Run modes: Allowlist, Auto-review, Run Everything. Auto-review order:
allowlisted calls run; other shell commands run sandboxed when possible; the remainder go to a
small classifier model that can allow, redirect the agent, or escalate to the user. Natural-language
`allow_instructions` / `block_instructions` in user and project `permissions.json`, merged. Cursor
states the classifier is **not a security boundary**. Sandboxing is documented for macOS
(Seatbelt) and Linux (Landlock + seccomp) only; `sandbox.json` controls network allowlists and
extra paths; protected paths include `.git/config`, `.git/hooks`, editor config and the ignore file.
Separate always-on protections: file deletion, files outside the workspace, browser tools (S13).

**3.9 Rules.** `.cursor/rules/*.mdc` with frontmatter (`description`, `globs`, `alwaysApply`) giving
four activation types: always, glob-attached when a matching file is in context,
description-selected by the agent, manual by mention. Nested `AGENTS.md`. Precedence
Team → Project → User. Guidance: under 500 lines, reference files rather than copy them (S14).

**3.10 Dynamic context discovery.** Long tool/MCP/shell output is written to files the agent can
read selectively instead of being truncated; full chat history stays available as a file after
summarization; only MCP tool *names* are static context and full schemas load on demand (about
47% fewer total tokens in runs that used MCP); MCP server status (e.g. re-auth needed) is surfaced
to the agent; terminal sessions are readable as files (S15).

**3.11 Review.** Local Agent Review on uncommitted/branch changes with quick/deep depth; PR
Bugbot with incremental review since the last reviewed commit, nested `BUGBOT.md` rule files,
learned rules with acceptance-rate analytics, `@cursor remember` teaching, verbose mode listing the
rules used, and **Git patch-ID dedupe** so a diff reviewed locally is not re-reviewed remotely
(S10, S16).

**3.12 Routing.** "Auto" uses a per-request classifier with cost / balance / intelligence modes;
the pool is vendor-managed and the chosen model is hidden by default (S17).

**3.13 Hooks, cloud, protocol.** Roughly twenty hook events (session, generic pre/post tool, shell, MCP, file
read/edit, prompt submit, pre-compact, stop, subagent start/stop, Tab, workspace open) with regex
matchers, fail-open default and optional `failClosed` (S18). Cloud agents, Projects (coordinator
that only plans/delegates, shared synced context files, event subscriptions), automations, mobile
(S19). CLI can act as an **ACP (Agent Client Protocol)** server over stdio JSON-RPC (S11).

## 4. Coverage matrix

`Covered` = VCP architecture already specifies an equivalent or stronger contract.
`Partial` = specified in part; the delta is an increment below. `Gap` = not found in the read scope.
`Out` = conflicts with a confirmed VCP decision or needs a surface VCP has deferred.

| Cursor capability | VCP status | Where in VCP | Action |
|---|---|---|---|
| Nested `AGENTS.md` | Covered | vcp-what 6.2; P2-01 | none |
| Skills, lazy skill bodies | Covered | 6.4, 15; P7-01/02; ADR-024/025 | none |
| MCP (stdio, HTTP), deferred schemas | Covered | 6.4, 15.4; P7-03; ADR-026..028 | CR-13 for status-in-context only |
| Large output spooled, ranges returned | Covered | 9.4 process output artifacts | none |
| Ask / Plan / Agent modes | Covered | autonomy `plan`/`ask`/`workspace`/`autonomous` (ADR-005, P2-03); `/plan` | CR-05 for plan artifact |
| Steering a running task | Covered | 17.2 (queued / next-step / requires cancel) | CR-11 for explicit queue |
| Session resume, fork at turn | Covered | 5.3 `session/fork`; `vcp sessions fork --through-turn` | CR-04 adds file state |
| Headless JSONL, non-interactive | Covered | 17.3; P3-01 | none |
| Compaction with protected fields | Covered (stronger) | 6.5; P2-08 | CR-13 for recall tool |
| Model routing with cost modes | Covered (stronger, transparent) | 7; P6; ADR-006/007/041 | do **not** adopt hidden-model default |
| Budget/spend limits | Covered (stronger) | 8; P1-05 | none |
| Subagents with isolation | Partial | 16; P7-04..06 Planned | CR-03, CR-08 |
| Semantic code search | Partial | 13 indexes claims + *retained* source chunks | CR-01 |
| Regex grep, indexed | Gap | implemented `vcp_search` is literal-only | CR-02 |
| File checkpoints / rewind | Gap | prepared changes + receipts exist (9.3) but no restore command | CR-04 |
| Debug mode | Partial | `review-debug` built-in skill family | CR-06 |
| Best-of-N | Gap | — | CR-07 |
| Worktree setup / cleanup | Gap | 16.2 defines ownership, not provisioning | CR-08 |
| Glob/description/manual rule activation | Partial | path-scoped AGENTS.md + skill descriptions | CR-09 |
| Classifier approval triage | Gap (and constrained by I-01) | ADR-020 gives the vehicle | CR-10 |
| Side chats, `/goal` | Gap | — | CR-11 |
| Review dedupe / incremental / learned rules | Partial | P7-05 review; governed memory | CR-12 |
| Trace-trained retrieval | Gap | full history + Markov analytics give the data | CR-14 |
| Post-edit lints (shadow workspace idea) | Partial | diagnostics listed as evidence (6.3) | CR-15 |
| ACP server mode | Gap, deferred surface | P9 JSON-RPC reserved | CR-16 |
| Hooks | Deferred already | P10-01 | note event list in S18 when P10-01 starts; no new increment |
| Image file read for vision models | Partial | 9.1 "optional image" | minor: add `image` artifact type to `file.read` when a routed model declares vision capability |
| Web search/fetch tool | Partial | 9.1 "optional web"; ADR-027 owned HTTP boundary | minor: route through ADR-027; effect class `network.read` |
| Protected paths, deletion/external-file guards | Verify | not found by grep for `.git/hooks` | fold into CR-10 verify step as deterministic policy defaults |
| Tab completion, inline edit, fast-apply model | Out | needs editor surface and a proprietary trained model | see note below |
| Cloud agents, Projects, automations, mobile, Slack | Out | "No hosted VCP" is confirmed (0.1) | none |
| Team rules, marketplace, enterprise admin | Out | single-user local scope (1.3) | none |
| Teammate index reuse via simhash | Out as designed | no server | adapted locally inside CR-01 (snapshot restore) |
| Browser tool, image generation, Design mode | Out for now | 1.3 defers browser/computer use | none |

**Note on fast-apply (low confidence, not re-verified).** Cursor historically had a frontier model
emit an edit sketch and a small fine-tuned model materialize the full file quickly. VCP cannot
train models, but the *routing shape* is expressible: a `low`-profile `apply` role that turns a
sketch into concrete operations which still pass through `file.prepareChange`. Do not build this
without a measured win on the P6 evaluation set; ambiguity in sketches conflicts with 9.3
("an ambiguous search/replace fails").

## 5. Increments

Common schema: **Cursor pattern → Verify first → Proposed design → Contracts → Invariants and
constraints → Acceptance → Open decisions.**

---

### CR-01 — Whole-workspace local code index with Merkle change detection

**Cursor pattern.** Section 3.1(a). Sources S4, S5.

**Verify first.**
```powershell
rg -n -i "retained source|ChunkRecord|IndexIntent" docs/architecture/memory-retrieval-design.md docs/plan/09-local-search-and-generations.md
rg -n -i "merkle|tree-sitter|workspace index|code index" docs src/crates
```
Expected at the read revision: retrieval indexes accepted claims and *retained* source chunks tied
to observed activity (vcp-what 13.1, plan 08/09). No proactive whole-workspace index and no
Merkle/AST chunking were found. Confirm before proceeding.

**Proposed design.**
1. `vcp-repository::merkle`: build a tree over the ignore-filtered discovery set (reuse the
   existing `discovery`/`ignore` policy so the index can never see more than `file.read` can).
   Leaf = hash of file bytes; node = hash of sorted child `(name, hash)` pairs. Persist as a
   rebuildable cache keyed by workspace root identity. Reuse Git blob IDs as leaf hashes **only**
   for clean tracked files; hash bytes for dirty/untracked files.
2. Change detection = diff previous tree vs. current tree; emit `IndexIntent` records
   (insert/supersede/delete) for diverging leaves only. This plugs into the existing generation and
   publication protocol (vcp-what 13.4; P5-05) rather than creating a second publisher.
3. Chunking: syntactic chunker (tree-sitter grammars as an *Open decision*; fall back to the
   existing text chunker for unsupported languages, generated files, or parse failure — explicit
   state per I-14). Record the chunker specification in the generation, as plan 09 already
   requires.
4. Embedding cache keyed by `hash(chunker_spec, embedding_spec, chunk_bytes)`. A cache hit skips
   inference. The cache is rebuildable and excluded from canonical state.
5. New record kind `workspace_source_chunk`, distinct from `claim` and `retained_source_chunk`, so
   results remain separately inspectable (FR-05) and so pruning history never silently deletes the
   live code index, nor the reverse.
6. Background build with explicit CPU/RAM/disk caps and cancellation (vcp-what 19.4). Report
   readiness as a fraction; `search.semantic` returns `index_state: building(p)` with partial
   results labelled, rather than copying Cursor's fixed 80% gate.
7. Portability adaptation of index reuse: a portable snapshot may carry the Merkle tree + chunk
   vectors. On restore to machine B, recompute the local tree; reuse vectors only for leaves whose
   hashes match; schedule the rest. This is the local analogue of Cursor's content-proof filter and
   satisfies I-15 (no silent merge of divergent state).

**Contracts.**
- Tool: `search.semantic { query, roots?, path_glob?, max_hits }` → hits with
  `{path, range, source_hash, record_kind, score, index_generation, stale: bool}`.
- CLI: `vcp index status|rebuild|pause|resume --workspace current`; `/index`.
- Config `.vcp/config.toml`: `[index.workspace] enabled, max_file_bytes, max_files, languages,
  cpu_quota, build_on_open = "ask" | "auto" | "off"`.
- Events: `index.workspace.scan_started|intent_recorded|generation_published|degraded`.

**Invariants and constraints.** I-16 (no remote embedding endpoint — this is where VCP deliberately
differs from Cursor's server-side index). I-08 (every passage authorized and traceable to a source
version). I-07/I-14 (lag and staleness are labelled). vcp-what 19.3: first start must not build a
whole-repository index without telling the user — hence `build_on_open = "ask"` default.

**Acceptance.** Extend M03/M04 fixtures: (a) edit one file in a 10k-file fixture → only that leaf's
chunks re-embed (assert inference call count); (b) rename/delete/ignored/generated/large-file
cases from othertools 16.6; (c) semantic-intent truth set where the query term never appears in
the target file; (d) compare three strategies on the same task set — lexical only, lexical +
repo-map, lexical + semantic — reusing the comparison harness already planned in vcp-what 6.6;
(e) network-blocked run proves no egress.

**Open decisions.** tree-sitter grammar set and native Windows build cost; whether the code index
shares the DiskANN instance with memory or uses a second graph; default for `build_on_open`.

---

### CR-02 — Regex search tool backed by a local sparse n-gram index

**Cursor pattern.** Section 3.1(b). Sources S3, S6.

**Verify first.**
```powershell
rg -n "vcp_search" src/crates/vcp-tools/src
rg -n -i "regex|search\.text|search\.symbols" docs/plan/06-windows-tools-and-recovery.md docs/development/p2-tools.md
```
At the read revision `vcp_search` is literal-only over bounded discovered files.

**Proposed design.** Two steps, independently shippable.
- *Step A (small):* add regex + word-boundary matching to the search tool with bounded results,
  per-file and total byte caps, a regex complexity/time limit, and explicit completeness
  disclosure (keep the existing "completeness and exclusions are explicit" contract). Use the
  `regex` crate (linear-time guarantees) rather than a backtracking engine.
- *Step B (gated on measurement):* local inverted index of sparse n-grams → candidate file set →
  run the real regex on candidates only. Memory-mapped lookup + posting files; dirty/unsaved or
  just-written files handled by a small in-memory overlay that is always scanned directly. Index
  invalidation driven by the CR-01 Merkle diff if present, otherwise by mtime+size+hash.
  Build Step B **only if** the P8-01 matrix or an owner repository shows `search` p95 above a
  recorded threshold; for small/medium repositories a plain scan is faster than maintaining an
  index. At least one open-source reimplementation reports sparse n-grams producing a far larger
  index than trigrams for marginal gain on a mid-size corpus, so compare trigram vs. sparse
  n-gram on VCP fixtures before choosing.

**Reuse candidates (Deferred-candidate form under vcp-what 0.2; licenses and revisions unverified):**
`PythonicNinja/trigrep`, `fastripgrep` (crates.io, MIT per its listing), `erogol/ngi`. Also
evaluate whether Tantivy's existing code-aware tokenizer (P5-03) already gives adequate candidate
filtering for literal fragments, which would avoid a second index format entirely.

**Contracts.** Tool `search.text { pattern, mode: literal|regex, word_boundary?, path_glob?,
max_hits, max_bytes }` → `{hits[], scanned_files, candidate_files, skipped[], complete: bool,
index_generation?}`.

**Invariants.** Same ignore/authorization boundary as `file.read`. A stale index may only ever
*add* candidates (false positives verified by the real regex), never hide a match: the overlay
plus "changed since generation" set must be scanned directly. I-14 applies.

**Acceptance.** Differential test: results identical to a full scan oracle across a regex corpus
(literal, alternation, anchored, `.*`, case-insensitive, Unicode, CRLF). Mutation test: modify a
file after indexing and assert the new match is returned. Latency report on small/medium/large
fixtures, cold and warm.

---

### CR-03 — Built-in context-isolating helper roles

**Cursor pattern.** Section 3.6: Explore, Bash, and verifier-style subagents exist mainly to keep
noisy intermediate output out of the parent context and to run that work on a cheaper model. S9.

**Verify first.** `rg -n -i "role|helper|read-only" docs/plan/14-visible-delegation.md docs/architecture/routing-extensions-design.md`
VCP already requires read-only helpers and isolated write children (16.1) with `ChildSpec`
role/model policy. The delta is a **named built-in role catalog with routing defaults**.

**Proposed design.** Ship three built-in `ChildSpec` templates, versioned like built-in skills
(ADR-025):

| Role | Authority | Default group/profile | Result packet |
|---|---|---|---|
| `explore` | read-only tools only | cheapest group meeting context needs; parallel reads allowed | ranked `{path, range, why}` list + ≤N-token synthesis; full transcript as artifact |
| `shell-runner` | the parent's already-granted process profiles, never wider | cheap group | exit status, parsed findings, artifact refs to full output; no raw logs in parent prompt |
| `verifier` | read-only + `verification.run` | a **different** model group from the implementer when one is available | per-claim pass/fail tied to current workspace fingerprint (I-12) |

Selection stays with the controller (16.1); the model may request a role but cannot grant it
authority (I-01). Each helper draws a reservation from the root ledger (I-05) and appears in
`/agents` (I-18). Custom project roles: **Deferred**; if added, use the skill package format
(ADR-024) rather than a new file format. Reading `.cursor/agents/*.md` belongs to P10-02.

**Acceptance.** U06 extension: a broad "where is X handled" task run with and without `explore`;
record parent-context tokens, total cost, and answer quality. `verifier` must fail a fixture where
the implementer claims success with a stale verification record.

---

### CR-04 — Agent-change checkpoints and file rewind

**Cursor pattern.** Section 3.3. S1, S11.

**Verify first.** `rg -n -i "rewind|restore.*file|undo" docs/plan/06-windows-tools-and-recovery.md docs/architecture/engine-execution-design.md src/crates/vcp-tools/src`
VCP journals every file transition with before/after content hashes and receipts (9.3, ADR-004),
and can fork a *session* at a turn, but no command restores *workspace files* to a prior turn.
`vcp-repository/src/restore.rs` is portability restore, not this.

**Proposed design.** Do **not** add a separate snapshot store. Derive checkpoints from what is
already durable:
1. A checkpoint is a projection: `(session, turn boundary) → set of {path, content_hash_before}`
   for every file VCP changed after that boundary. Requires that pre-change bytes are retained as
   artifacts for `file.applyChange` (verify; if only hashes are kept, retaining the before-image is
   the one new storage requirement, and it falls under the existing retention policy).
2. `rewind` is itself a **prepared change** through `file.prepareChange`/`file.applyChange`, so it
   inherits version checks and receipts. For each file: if current hash == VCP's last recorded
   after-hash → restore; else → **conflict**, skip, and report (I-06: never overwrite a human edit).
   Files VCP created are deleted only if unchanged since creation. Files VCP deleted are recreated
   only if the path is still absent.
3. Effects outside the file journal (process side effects, MCP calls, Git operations) are listed as
   *not reversible* in the preview. Rewind never claims to undo them.
4. Conversation is untouched (matches Cursor). Combining with `sessions fork --through-turn` gives
   "rewind files and branch the conversation".

**Contracts.** CLI `vcp rewind --to-turn <id> --preview` → preview ID bound to store revision
(same pattern as prune previews) → `vcp rewind apply <preview-id>`; `/rewind`. Events
`rewind.previewed|applied|partially_applied`. Children: rewinding a parent turn that integrated a
child result rewinds the integration diff, not the child worktree.

**Invariants.** I-06, I-10, I-11 (a rewind is not a replay). Pruning: before-images needed by an
unexpired checkpoint window are a pruning dependency that must be reported (I-17) — propose a
bounded window (e.g. current task + last N completed tasks) rather than unbounded retention.

**Acceptance.** E05/R02-style races: human edit between agent edit and rewind; CRLF/encoding
preserved byte-for-byte; rename + edit chains; partial application after forced kill, then
reconcile on resume.

---

### CR-05 — Plan as a durable, editable, revision-bound artifact

**Cursor pattern.** Section 3.5 Plan mode. S7.

**Verify first.** `rg -n -i "/plan|plan artifact|autonomy.*plan" docs/plan/07-cli-and-inspection.md docs/development/p3-cli-usage.md`
`plan` exists as an autonomy ceiling and `/plan` as a control. Not found: a persisted plan document
the user can edit and then execute against.

**Proposed design.** `/plan` produces a markdown artifact with a small required header
(objective, acceptance checks, affected paths, open questions, budget estimate by profile). Stored
as a task artifact; `--save` copies it to `.vcp/plans/<slug>.md` in the workspace. `vcp run --plan
<path>` admits the plan as the task's accepted constraints: the file's hash is sealed into the
context manifest (6.3), so an edit after admission produces a new manifest revision rather than
silently changing a running task. Clarifying questions use the existing durable `user.ask`.
Add the Cursor-recommended recovery path as an explicit CLI affordance once CR-04 exists:
"rewind to plan boundary, edit plan, re-run".

**Acceptance.** Editing the plan file mid-run triggers a visible steering/manifest revision, never
a silent change. A plan run in `plan` autonomy performs zero write/process effects.

---

### CR-06 — Evidence-first debug workflow with tracked instrumentation

**Cursor pattern.** Section 3.5 Debug mode. S8.

**Verify first.** Read `src/skills/builtin/review-debug/` and `docs/development/p7-builtin-skills.md`.

**Proposed design.** Extend the `review-debug` skill family with a staged workflow; the engine
contributes two guarantees a prompt alone cannot:
1. **Instrumentation is tagged.** Changes made in the instrument stage carry
   `purpose = "instrumentation"` on their prepared-change records.
2. **Completion contract check (I-12):** a task cannot report `completed` while any
   instrumentation-tagged change is still present in the workspace, unless the user explicitly
   keeps it. Removal is the inverse prepared change (or CR-04 scoped to that tag).
3. Log collection without a server: instruct instrumentation to append JSON lines to
   `.vcp/debug/<task>/events.jsonl` (inside the workspace, ignore-listed, no network listener, no
   extra authority). The reproduce step is a durable `user.ask` with exact steps; the analyze step
   reads the file through ordinary bounded reads.

**Acceptance.** Fixture with a seeded race condition: hypotheses recorded before any fix edit;
fix diff ≤ N lines; zero instrumentation remains at completion; killing the CLI mid-workflow and
resuming still ends with instrumentation removed or explicitly reported.

---

### CR-07 — Best-of-N across model groups, feeding routing evidence

**Cursor pattern.** Section 3.7 `/best-of-n`. S12.

**Why this fits VCP unusually well.** P6 qualification at the read revision records *rejections*
for unqualified advice because held-out evidence is thin. Best-of-N produces exactly the missing
data: same task, same base, different model groups, with real verification outcomes and exact
per-attempt charges (ADR-031). Cursor treats it as a convenience; in VCP it is also an evidence
generator for `/optimize` (P6-05, FR-17).

**Proposed design.** `vcp run --best-of <group[,group…]>` or `/best-of`. The controller creates N
isolated write children (P7-04) from one recorded base + dirty fingerprint (16.2), each pinned to
one group/profile. Each child runs the same acceptance checks. A comparison record holds per-child
verification results, diff stats, cost, latency, and unresolved issues. The user (or an explicit
deterministic rule such as "all checks pass, then lowest cost") selects a winner; integration goes
through P7-05 unchanged. Losing worktrees fall under CR-08 cleanup. Outcomes are written as
retained transition evidence (ADR-029/030) labelled `cohort = best_of_n` so optimizer statistics
can include or exclude them explicitly (selection bias is real: users run best-of-N on hard tasks).

**Invariants.** I-04/I-05/I-09: N reservations are admitted atomically against the root budget
before any child starts; if the budget admits only k < N, say so and ask, never silently drop
candidates. Under a one-call concurrency cap the children run sequentially (16.1 already allows
this).

**Acceptance.** Budget admission test with insufficient funds; identical base fingerprint across
children; winner integration re-verified on the parent (16.4: a passing child is not proof the
merged result passes); evidence rows carry the cohort label.

**Open decision.** Whether an LLM judge may *rank* candidates. If so it is a bounded semantic
decision under ADR-020 (advisory, accounted, deterministic fallback) and can never auto-apply.

---

### CR-08 — Worktree setup recipes and bounded cleanup

**Cursor pattern.** Section 3.7. S12.

**Verify first.** `rg -n -i "worktree" docs/plan/14-visible-delegation.md docs/architecture/routing-extensions-design.md`
Ownership and base capture are specified; provisioning (dependency install, env files) and disk
reclamation were not found. Without provisioning, child verification in a fresh worktree fails for
most real projects, which would make P7-05 acceptance misleading.

**Proposed design.** `.vcp/worktrees.toml`:
```toml
[setup]                     # generic fallback
commands = [["npm", "ci"]]  # argv vectors, not shell strings (vcp-what 9.4)
[setup.windows]
script = "setup-worktree.ps1"   # path relative to this file
copy = [".env"]             # explicit copy list from the root worktree; never symlink deps
[cleanup]
max_count = 25
interval_hours = 6
```
Setup commands are **executable repository inputs**: they run through the process broker with the
child's authority, need a grant like any other command (9.5 says configured commands are subject
to policy), are charged/time-limited, and their output is an artifact. Expose
`VCP_ROOT_WORKTREE_PATH` to setup processes. Cleanup never deletes a worktree that has
unintegrated changes, an unreconciled effect, or an unsettled reservation (I-17 analogue);
it reports what it kept and why. Copying `.env` is a credential-adjacent action: require it to be
listed explicitly and keep file contents out of capture (10.4).

**Acceptance.** Fresh-worktree verification passes on Node, Python, Rust and .NET fixtures only
when setup ran; a malicious `worktrees.toml` in an untrusted workspace cannot run without a grant;
cleanup respects the protections above after a forced kill.

---

### CR-09 — Scoped instruction fragments

**Cursor pattern.** Section 3.9. S14.

**Verify first.** `rg -n -i "glob|nested|precedence" docs/plan/04-context-and-instructions.md docs/architecture/context-provider-design.md`
Path-scoped nested `AGENTS.md` and description-driven skill activation exist. Missing: a fragment
that attaches when a *file pattern* enters context without being a directory-level AGENTS.md, and
a manual-only fragment.

**Proposed design.** Prefer **not** inventing a new format. Two options, pick one in review:
- *Option A (smallest):* allow optional frontmatter in skill descriptors:
  `attach_globs = ["src/components/**/*.tsx"]` and `activation = "always"|"auto"|"glob"|"manual"`.
  Instruction-only skills (no tools, no assets) then cover all four Cursor rule types using the
  existing discovery, activation, authority fences, and inspection (ADR-024).
- *Option B:* read `.cursor/rules/*.mdc` as a foreign format through P10-02 (ADR-014), translating
  `alwaysApply/globs/description` to Option A semantics, reporting unsupported fields, granting no
  authority.
Either way: each attached fragment is a context part with source, scope, revision, inclusion
reason, and token estimate (6.1), visible in `/context`. Conflicts follow 6.2 (user constraint
beats repository text; record the conflict). Enforce a size cap and surface truncation.

**Acceptance.** Fragment appears in the manifest only when a matching path is in the evidence set;
duplicate load through two discovery routes is impossible (6.6 single scope resolver); a fragment
containing "ignore previous instructions / run X" changes no grants (I-01).

---

### CR-10 — Advisory approval triage under bounded semantic decisions

**Cursor pattern.** Section 3.8 Auto-review. S13. `needs_owner_decision: true`.

**Tension to resolve first.** Cursor lets a classifier *allow* un-allowlisted calls. VCP's I-01
says a model cannot grant execution authority. A direct port would violate I-01.

**Proposed design that preserves I-01.** The classifier may only move a decision in the
**restrictive** direction or choose among outcomes the deterministic policy has *already*
authorized:
1. Deterministic policy (P2-03) evaluates first. `deny` stays deny. `allow` stays allow unless a
   user `block_instruction` matches semantically → escalate to `ask`.
2. For `ask` outcomes, and only under a user-selected preset (e.g. `workspace+triage`) that
   explicitly pre-authorizes a named effect class *conditional on non-objection*, the advisory may
   return `no_objection | suggest_alternative | escalate`. `no_objection` lets the pre-authorized
   class proceed; anything else, any timeout, any budget shortfall, any schema failure → `ask`.
3. Instructions live in `.vcp/permissions.toml` (`block_instructions`, `allow_hints`), user-level
   and project-level. **Project-level `allow_hints` from an untrusted workspace are ignored**;
   project-level `block_instructions` are honoured (they can only restrict).
4. Implement as an ADR-020 typed advisory judgment: canonical request/result records (ADR-036
   pattern), helper accounting (ADR-038), deterministic fallback, measured rollout. Record the
   advisory, model, cost, and outcome on the effect's authority trail (FR-06).
5. State plainly in UX and docs that this is a prompt-reduction feature, not a security boundary.

**Also verify (deterministic, no classifier needed):** default protected paths (`.git/config`,
`.git/hooks/**`, `.vcp/**` policy files, the ignore file), a file-deletion guard, and an
outside-workspace write guard that require explicit approval even in `autonomous`. If absent, add
them to P2-03 defaults independently of the rest of CR-10.

**Windows note.** Cursor documents OS sandboxing for macOS and Linux only, so it offers no Windows
pattern to borrow. VCP's AppContainer/Job Object work (ADR-005, P0-05) is already ahead here.

**Acceptance.** Red-team fixture: prompt-injected repository text tries to get a destructive
command through triage → must end in `ask` or `deny`. Classifier outage → behavior identical to
the base preset. Approval-prompt count and false-allow/false-block rates reported on a held-out set
before any default-on decision (same bar ADR-020 already sets).

---

### CR-11 — Input queue, side sessions, long-lived goals

**Cursor pattern.** Section 3.4. S1.

**Proposed design.** (a) *Queue:* distinguish `steer` (apply at next safe boundary — exists) from
`enqueue` (new turn after the current one completes); persist the queue as task-linked pending
input so pause/resume keeps it; allow reorder/remove. (b) *Side session:* `/side <question>` forks
a **read-only** session whose context manifest references the parent's projection as attributed
evidence; separate transcript; charged to the same root budget; cannot write. (c) *Goal:*
`/goal <objective>` = a task whose completion contract is a set of acceptance checks re-evaluated
after every turn, bounded by budget, deadline, and a no-progress detector (reuse the repeated
compaction/stall signals from the Markov plan). It ends `completed`, `budget_exhausted`,
`stalled`, or `paused` — never an unbounded loop.

**Acceptance.** Queue survives forced kill; side session performs zero write effects; goal with an
unsatisfiable check stops on the stall detector with cost accounted.

---

### CR-12 — Local change review: patch identity, incremental scope, learned rules

**Cursor pattern.** Section 3.11. S10, S16.

**Verify first.** Read P7-05 in `docs/plan/14-visible-delegation.md` and the `review-debug` and
`git-workflow` built-in skills.

**Proposed design.**
1. `vcp review [--base <ref>] [--uncommitted] [--depth quick|deep]` and `/review`; runs a
   read-only child (CR-03 `verifier`-class authority). Depth maps to routing profile, not to a
   different code path.
2. **Patch identity:** key each review record by `git patch-id --stable` of the reviewed diff plus
   rule-set revision. Re-running on an identical patch returns the stored findings at zero model
   cost unless `--force`. This also lets integration review (P7-05) skip re-reviewing an unchanged
   child diff.
3. **Incremental:** store `last_reviewed_commit` per branch; default scope = changes since then,
   with `--full` override.
4. **Review rule files:** nested `REVIEW.md` discovered upward from changed files, via the same
   scope resolver as instructions.
5. **Learned review rules → governed memory.** A finding the user accepts or rejects is evidence.
   Propose claims of class `review_rule` with scope globs, evidence links to the findings, and an
   acceptance rate; low-acceptance rules are *disputed/superseded* through normal governance
   (11.3) rather than silently disabled. `vcp review --explain-rules` lists rules used, truncated,
   or omitted.

**Acceptance.** Identical patch → zero provider calls on second run; rebased-but-identical patch
still hits; a rule with recorded 0/10 acceptance is surfaced for supersession; findings
distinguish demonstrated defects from suggestions (15.6 already requires this).

---

### CR-13 — Remaining dynamic-context-discovery pieces

**Cursor pattern.** Section 3.10. S15. Most of it is already in VCP (spooled output, deferred
schemas, non-destructive compaction).

**Delta.**
1. `history.read { session: current, range|query }` — a bounded, read-only tool over the task's
   **own** pre-compaction events and artifacts, so detail lost in a summary is recoverable on
   demand. Authorization: own session tree only; respects redaction/retention states; results are
   attributed data, not instructions. The compaction summary should name the source range
   (6.5 already links it) so the model knows what to ask for.
2. MCP server state as a small static context part: `{server, state: ready|auth_required|failed|
   disabled, tool_names[]}` so the agent can tell the user "re-authenticate X" instead of
   silently losing tools.
3. Measure before/after tokens on MCP-heavy fixtures; Cursor's ~47% figure is theirs, not a VCP
   target.

**Acceptance.** After forced compaction, a question about an early tool result is answered via
`history.read` with the correct artifact reference; pruned ranges return an explicit gap (5.4
semantics), never fabricated content.

---

### CR-14 — Retrieval relevance learning from local traces

**Cursor pattern.** Section 3.1: embedding model trained on agent traces with LLM-ranked
helpfulness. S5. VCP cannot train a frontier embedding model, and I-16 keeps embeddings local —
but VCP retains full history, which Cursor's users do not get locally.

**Proposed design.** Log, per retrieval, `{query, candidates, ranks, fused score}` and downstream
**use signals**: was the chunk's file subsequently read, edited, cited in the result, or present in
a change that passed verification. From these, fit only *small, inspectable* local artifacts:
fusion weights between lexical/vector/repo-map, per-record-kind priors, and optionally a light
local reranker. Treat them exactly like the Markov fit artifacts: rebuildable (ADR-032), held-out
task-separated comparison (ADR-033), explicit abstention on sparse data, never activated without
qualification, rules-only operation always supported. An LLM-ranked label pass is optional, paid,
explicit-budget only, and goes through ADR-020.

**Acceptance.** Held-out recall@k and downstream task success vs. the fixed-weight baseline;
activation refused below the evidence threshold; pruning history invalidates or relabels affected
fits.

---

### CR-15 — Post-edit diagnostics before verification  *(low-confidence source)*

**Cursor pattern.** Cursor's 2024 "shadow workspace" write-up (not re-fetched) described applying
AI edits in a hidden editor instance to obtain language-server diagnostics before showing them.

**Proposed design.** After `file.applyChange`, optionally run a fast, repository-configured
*diagnostic* command scoped to touched files/packages (`cargo check`, `tsc --noEmit -p`,
`dotnet build -clp:NoSummary`) and return parsed findings with the tool result, before the heavier
`verification.run`. A language-server client is a later option (9.1 lists LSP as optional). Same
policy, budget, and artifact rules as any process.

**Acceptance.** Measured reduction in edit→verify→fix cycles on the U01–U03 task set; diagnostics
that cannot run are reported as "could not run", not as clean (9.5).

---

### CR-16 — Evaluate ACP for the deferred editor-facing protocol

**Cursor pattern.** Cursor CLI can run as an Agent Client Protocol server over stdio JSON-RPC, an open
protocol some editors (Zed originated it) use to host third-party agents. S11.

**Proposal.** When P9-01 starts, evaluate implementing ACP as *a* public surface (possibly beside
VCP's own richer protocol) so P4's editor goals are reachable in multiple editors without writing
one extension per editor. Map ACP sessions/turns/tool-call permission requests onto VCP
session/task/turn/approval records; VCP-only concepts (budgets, child graphs, memory inspection)
ride as extensions or stay on the native protocol. No work before P8-05. Record the outcome as an
amendment to ADR-002.

## 6. Rules that apply to every CR increment

1. **No new authority paths.** Every new tool declares effect class, capabilities, caps, timeout,
   cancellation, idempotency class, and artifact types, and dispatches through the 9.2 sequence.
2. **Everything model-assisted is accounted.** Helpers, judges, classifiers, rerank labelers all
   reserve against the root ledger (I-04, I-05).
3. **Derived state is rebuildable and labelled.** Indexes, caches, fits, and review caches are
   never canonical; staleness is explicit (I-07, I-14); both storage backends behave the same
   (I-13).
4. **Local-only computation where VCP already requires it** (I-16). Do not copy Cursor's
   server-side index, hosted classifier, or cloud agents.
5. **Portability:** any new persisted record kind must be added to snapshot contents (12.9) or
   explicitly marked rebuildable, and must be encrypted on publication like everything else (I-19).
6. **Retention:** any new retained payload (before-images, review findings, debug logs) needs a
   pruning selector and a stated dependency rule (I-17; ADR-021/022).
7. **Foreign formats** (`.cursor/rules`, `.cursor/agents`, `.cursor/worktrees.json`,
   `permissions.json`) are read only through P10-02 importers; imported data grants no authority
   (ADR-014).
8. **Pattern adoption only.** Cite this file and the source keys in the ADR/PR. Do not paste Cursor
   documentation text, prompts, or default lists into the repository.
9. **Windows first.** Setup scripts, path handling, and process profiles must pass on native
   Windows before anything else (0.1).
10. **Do not expand first-release scope implicitly.** Only `fold_into_in_flight` items may ride
    with P7-04..06, and only if they do not change those tasks' exit criteria without an owner note.

## 7. Suggested sequencing

| Wave | Increments | Rationale |
|---|---|---|
| Ride with P7-04..06 (owner approval needed) | CR-08, CR-03 | Without worktree provisioning, child verification is unrealistic; helper roles are templates on the same `ChildSpec`. |
| First post-release | CR-04, CR-02 step A, CR-05 | Highest user-visible safety/usability per unit of work; no new subsystems. |
| Second | CR-01, CR-12, CR-13, CR-07 | New derived stores and evidence generation; CR-07 starts feeding P6 qualification. |
| Third | CR-06, CR-09, CR-10, CR-11 | Workflow polish and the owner-decision item. |
| Evidence-gated | CR-02 step B, CR-14, CR-15 | Build only after measurements justify them. |
| With P9 | CR-16 | Deferred surface. |

ADR candidates: ADR-042 (workspace code index), ADR-043 (indexed regex search and its gate),
ADR-044 (rewind semantics and before-image retention), ADR-045 (best-of-N cohort evidence),
ADR-046 (advisory approval triage vs. I-01), ADR-047 (retrieval fit artifacts). Numbers are
suggestions; take the next free number at authoring time.

## 8. Sources

| Key | Source |
|---|---|
| S1 | Cursor docs — Agent overview (tools, checkpoints, queue/steer, side chats, `/goal`): https://cursor.com/docs/agent/overview.md |
| S2 | Cursor docs index: https://cursor.com/llms.txt |
| S3 | Cursor docs — Search / Instant Grep / Explore subagent: https://cursor.com/docs/agent/tools/search.md |
| S4 | Cursor blog — Securely indexing large codebases (Jan 27, 2026): https://cursor.com/blog/secure-codebase-indexing |
| S5 | Cursor blog — Improving agent with semantic search (Nov 6, 2025): https://cursor.com/blog/semsearch |
| S6 | Cursor blog — Fast regex search: indexing text for agent tools (Mar 23, 2026): https://cursor.com/blog/fast-regex-search |
| S7 | Cursor docs — Plan Mode: https://cursor.com/docs/agent/plan-mode.md |
| S8 | Cursor docs — Debug Mode: https://cursor.com/docs/agent/debug-mode.md |
| S9 | Cursor docs — Subagents: https://cursor.com/docs/subagents.md |
| S10 | Cursor docs — Agent Review: https://cursor.com/docs/agent/agent-review.md |
| S11 | Cursor docs — CLI usage and slash commands: https://cursor.com/docs/cli/using.md , https://cursor.com/docs/cli/reference/slash-commands.md |
| S12 | Cursor docs — Worktrees and `/best-of-n`: https://cursor.com/docs/configuration/worktrees.md |
| S13 | Cursor docs — Run Modes, Auto-review, sandboxing: https://cursor.com/docs/agent/security/run-modes.md |
| S14 | Cursor docs — Rules: https://cursor.com/docs/rules.md |
| S15 | Cursor blog — Dynamic context discovery (Jan 6, 2026): https://cursor.com/blog/dynamic-context-discovery |
| S16 | Cursor docs — Bugbot: https://cursor.com/docs/bugbot.md |
| S17 | Cursor docs — Cursor Router: https://cursor.com/docs/cursor-router.md |
| S18 | Cursor docs — Hooks: https://cursor.com/docs/hooks |
| S19 | Cursor docs — Projects: https://cursor.com/docs/agent/projects.md |
| V1 | VCP repository at the revision in the front matter: `README.md`, `AGENTS.md`, `docs/architecture/vcp-what.md`, `docs/architecture/othertools.md` §7 and §16.6, `docs/plan/README.md`, `docs/plan/20-traceability.md`, `docs/adr/README.md` |
