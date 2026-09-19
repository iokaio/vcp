# P3 CLI implementation

Owning work item: P3-01, structured CLI and authenticated owner controls.
Status: P3-01 implemented and natively qualified. See [usage](p3-cli-usage.md).
The checkpoint and increment notes below are historical, not the current backlog.
Starting revision: `a82cc9a1e07bb3ee83647c293c0fcd58c298eaf3`, clean main.

P2-06/07/08 are complete with [P2 evidence](../evaluations/p2-completion.md).
The contract is [plan 07](../plan/07-cli-and-inspection.md), architecture section
17 and ADR-012/016. The CLI uses the selected workspace's argument parser and
the existing canonical/retained owner. Rendering does not execute workers.

The first construction increment validates typed commands, bounded UTF-8 task
files and positive integer-micro USD caps before admission. Versioned JSONL
records separate accepted commands, events, questions, gaps and final results;
final results preserve all conditions with architecture-defined precedence.
Output failure closes the stream. Owner integration must handle that failure
before this increment qualifies the broken-pipe acceptance condition.

Pause/cancel controls authenticate scope and owner and validate the observed
task revision before stopping the retained controller. Existing canonical
command receipts implement retries. A repeated pause has a durable receipt
without changing the paused task revision. A pause receipt is not proof that
every in-flight process has already stopped.

Validation targets include native argument/JSONL contracts, both canonical
backends, real retained-stream pause/cancel and independent process consumers.
Executable wiring, provider/task setup, live control delivery, noninteractive
questions and slow-consumer/owner-loss tests remain part of P3-01. P3-02–04
remain subsequent connected work.

P3-05 requires P5-07; P3-06 requires P5-09/10. Those P5 work items and their
memory/search prerequisites remain planned in the ledger. CLI presentation
cannot substitute for those production services.

## Machine handoff — 2026-09-19

Branch: `p3-cli`. This is a work-in-progress checkpoint for moving development
to another machine, not a completed P3 milestone. These notes were reconstructed
from the interrupted session's working tree; prior test outcomes are unknown.
The checkpoint preserves the implementation as found, including unfinished
formatting. No implementation behavior was changed during handoff.

### Saved implementation

- `src/crates/vcp-cli/`: new library crate with typed Clap commands, bounded
  task-file validation, exact decimal budget parsing, exit-code precedence and
  versioned JSONL framing. Six contract tests cover these local boundaries.
- `vcp-cli/src/session.rs`: Windows-only retained-controller startup adapter
  using the canonical host and OpenRouter transport. This is scaffolding; the
  crate has no `main.rs` or binary target and commands are not wired end to end.
- `vcp-lifecycle/src/foundation/worker/control.rs`: owner control envelopes and
  pause/cancel delivery with scope, owner and revision checks, retained holds
  and durable command retries. Registered by `worker.rs`.
- `vcp-lifecycle/tests/support/cli_control.rs`: retained streaming test for
  invalid controls, duplicate commands, repeated pause and stopped admission,
  iterating SQLite/file backends and pause/cancel. Included by `canonical_host.rs`.
- `vcp-engine/src/command_handler.rs` and `tests/commands.rs`: repeated pause
  records a durable acknowledgement without advancing an already-paused task's
  revision; regression test covers both backends and stale input.
- Selected Codex workspace `Cargo.toml` and `Cargo.lock`: register `vcp-cli`.
  Corresponding vendoring patch/inventory updates are still outstanding; follow
  [upstream qualification](upstream-qualification.md) before milestone completion.

### Validation at checkpoint

- `git diff --check`: passed for the pre-existing tracked changes; repeat after
  staging to include the new files and this document.
- Direct `rustfmt --check --edition 2021 --config skip_children=true` on the
  changed Rust files: failed with formatting differences. Formatting remains
  to be applied to the affected files.
- From the repository root,
  `cargo test --manifest-path src/third_party/codex/codex-rs/Cargo.toml -p vcp-cli --test contracts --locked --offline`
  began compiling dependencies, then was deliberately interrupted to finish
  the transfer checkpoint. No tests completed and compilation is unverified.
- Engine/lifecycle tests, static analysis, vendoring reconstruction, independent
  process consumers and native terminal qualification were not run here.
  The existence of a test is not evidence that it passes.

### Resume here

1. Read this note and P3-01 in [plan 07](../plan/07-cli-and-inspection.md), then
   inspect the saved files above. Continue on `p3-cli`; do not mark the ledger
   complete based on this checkpoint.
2. Establish compilation and run the focused tests below. Use the selected
   workspace directory so Cargo discovers its toolchain and `.cargo/config.toml`;
   configure the destination machine's normal native Windows build prerequisites
   and cache. Build artifacts and credentials are not part of this checkpoint.
3. Finish executable dispatch, validated workspace/provider/task setup, event
   consumption and final durable outcomes. Wire authenticated owner delivery
   without introducing another canonical writer. Connect broken-pipe/owner-loss
   handling and noninteractive questions to canonical lifecycle behavior.
4. Exercise slow consumers, independent JSONL process readers, cancellation,
   unresolved effects and real pause/inspect/resume. Complete formatting and
   vendoring records, then broaden validation as required by P3-01 acceptance.
5. Continue P3-02/03/04 only against their own contracts. P3-05/06 retain the
   P5 prerequisites described above.

Suggested focused commands (not passing evidence):

```powershell
Set-Location src/third_party/codex/codex-rs
cargo test --locked -p vcp-cli --test contracts
cargo test --locked -p vcp-engine --test commands repeated_pause_is_durable_without_mutating_task_or_accepting_stale_input
cargo test --locked -p vcp-lifecycle --test canonical_host cli_control_authenticates_before_stopping_and_retries_without_another_effect
```

## Destination-machine increment — 2026-09-19

P3-01 remains incomplete. Native Windows dependency provisioning now succeeds;
fetching only pinned Git commits avoided stalled full-history downloads. This
increment uses the owner's requested Rust 1.98.1 (`+stable`) and Node 24.21.0.
The selected upstream's toolchain file is unchanged.

Implemented and tested:

- CLI preflight resolves an existing Unicode workspace and validates task input
  before owner construction. Inspection identifiers use the opaque-ID parser.
- `OwnedJsonl` owns the canonical lifetime. Its bounded writer thread carries
  bytes only; write failure or a 30-second write deadline closes the owner using
  the existing durable pause policy. Authenticated engine cursors provide finite,
  bounded event pages and are released after output, including failure paths.
- A separate Node process parses actual piped UTF-8 JSONL and rejects duplicate
  event sequences. Other tests cover broken output on both canonical backends
  and owner responsiveness while the output writer is blocked.
- The internal `CreateSession` command creates an independent session or records
  ancestry through an existing completed canonical turn. The additive optional
  `Session.fork_through` field preserves old records when absent. Storage validates
  the boundary's ancestry and completion; retries return the original receipt.
  This creates session metadata, not a working forked coding conversation.
- The checkpoint's Rust formatting and missing workspace patch/inventory are
  updated. Boundary inventory includes the new CLI owner adapter. A native
  harness fixture now copies Node when Windows disallows hard-linking its
  protected installation; the original assertions remain intact.

Run `pwsh -NoProfile -File scripts/test-p3.ps1` from the repository root. It records
compiler identity, source hashes, commands and logs under ignored `artifacts/p3/`.
The affected package tests and the separate retained-stream
pause/cancel regression passed on both backends. This is increment evidence, not
P3-01 completion or installed-product qualification.

Repository fast delivery checks, source inventory verification, changed-file
formatting and `git diff --check` passed. Standard Clippy passed for the affected
packages with existing warnings in domain policy/task types, protocol enums,
store helpers and a capture test. The stricter `-D warnings` invocation failed
on existing domain warnings; no lint allowances or unrelated API refactors were
introduced to conceal them.

### Canonical turn and control integration — 2026-09-19

The owner now exposes `begin_coding_turn` for recording actual submitted input.
Retained coding callbacks advance that turn through context assembly, reservation,
model admission, response processing, tool handling and verification. Completion
shares one canonical commit with verified task completion. Retries retain the turn ID
and have separate canonical attempts. Reopening reconciles unfinished turn state;
steering atomically pauses the superseded turn while preserving its original
steering identity. Explicit controls and owner shutdown preserve stopped turns.

Budget admission now distinguishes typed exhaustion from task/authority denials.
A tracked coding turn records exhaustion before a provider send; an arbitrary
admission rejection is not classified as budget exhaustion.

The bounded private structured-input adapter rejects incomplete, oversized,
invalid-version and malformed frames before owner dispatch. Commands use the
existing authenticated stop handler and preserve duplicate receipts. A blocked
output writer does not prevent input delivery. Outcome selection reads durable
task, turn, approval, attempt and effect records, including descendant liabilities,
and keeps simultaneous conditions. `OwnedJsonl::finish` closes the owner, drains
durable events and emits one final result with a committed task receipt.

Tests cover a pending question across explicit pause, ambiguous effects followed
by cancellation/reconciliation, control delivery during output backpressure,
independent Node parsing of the final record, tracked retries, pre-send budget
denial and real retained completion establishing a selectable fork boundary.
Current failed verification takes precedence over pause; a report from an older
steering revision cannot override the new task contract. The native P3 gates
passed, followed by a focused CLI rerun for this outcome regression (55 distinct
tests across those runs). Clippy passed with existing warnings.
The P3 runner includes budget contracts and the retained coding-turn matrix.

### Previous increment's remaining P3-01 work (superseded)

Executable dispatch and trusted workspace/provider setup remain unimplemented.
The structured-input adapter still needs integration into that executable's
lifetime and dispatch loop. The retained session adapter must submit each actual
user turn through `begin_coding_turn` before starting retained work.

Fork metadata and completed turn boundaries exist, but conversation inheritance
does not. History restoration still accepts only same-task parts, and canonical
storage rejects cross-task artifact reuse. A fork must reconstruct historical
context with newly scoped provenance, without replaying effects or transferring
execution authority. Complete installed-process qualification also remains.
Do not advance the P3 ledger or treat these increment tests as P3-01 completion.

## Executable completion — 2026-09-19

The `vcp` binary now dispatches run/task-file input, list/status/inspect,
resume by task/latest/session, completed-turn forks, and live pause/cancel.
Startup validates explicit user profiles, qualified catalog snapshots, exact
caps, native process/check profiles and resolved local data paths. Canonical
budget admission is persisted before output, and resume retains that cap.
Default builds have no configurable provider endpoint override.

The retained controller remains the only execution loop. Turn starts and stages
are canonical; verified task completion establishes its completed fork boundary
atomically. Forks capture bounded, newly scoped historical evidence and do not
replay effects or inherit permissions or accounting liabilities. Recovery pauses
pending/running work across sessions, without widening external command scope.

The Windows owner pipe has an explicit current-user DACL, rejects remote clients,
and authenticates scoped control envelopes through the existing stop handler.
Structured stdin and Ctrl+C handling run independently of bounded stdout writes.
Final task JSONL follows durable owner shutdown and preserves all outcome flags.
Read commands return bounded canonical records/references and truthful capability
states for deferred memory/routing work.

Native subprocess qualification covers real edit/check/completion, task-file
input, listing/inspection, terminal resume, history forks, malformed inputs,
budget denial before HTTP, noninteractive approvals, live owner identity rejection,
cancellation with unresolved spend, and broken-pipe pause followed by resume with
the original cap. A separate Node process validates JSONL framing, event uniqueness,
and the single final record. The existing backend and retained-loop matrix remains
part of `scripts/test-p3.ps1`.

Completion evidence on native Windows with Rust 1.98.1 and Node 24.21.0:

- `artifacts/p3/08f1c358-e0c9-44e1-8bdc-394a5572c2dd/manifest.json`: pass,
  80 tests across contracts, executable, retained controls and coding turns.
  The manifest records source hashes, commands, tool versions and log hashes.
- `artifacts/p3-final-executable.log`: 20 passing CLI tests after lint-only argument
  grouping and boxing of the private control envelope; no wire format change.
- `artifacts/tests/bfa9578d-d7ea-4681-8d4b-6db2efe1352e/manifest.json`: all eight
  fast repository/harness suites pass.
- Selected-source inventory: 7,939 files verified; static boundary inventory:
  173 packages, 36 groups and 86 seams. Patch 0022 records workspace registration.
- Changed Rust files pass rustfmt; the final CLI Clippy run has no CLI warnings.
  Broader static analysis passes with existing warnings in retained/shared crates.
  Evidence files are local ignored artifacts;
  the committed runner reproduces the qualification.

P3-02 terminal interaction, P3-03 full paged inspectors and P3-04 expanded workspace
continuation remain separate work items. Qualification uses synthetic loopback
responses; it is not a live paid-provider conformance claim or release packaging.

P3-03 subsequently completed its [common paged evidence inspectors](p3-inspection.md),
including on-demand exact captured bytes and live-owner query delivery.

P3-02 subsequently completed the [interactive terminal workflow](p3-terminal.md),
including native input/resize/close and actual ConPTY pause/steer/answer/resume.
