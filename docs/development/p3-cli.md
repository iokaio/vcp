# P3 CLI implementation

Owning work item: P3-01, structured CLI and authenticated owner controls.
Status: in progress; this is not a completion or installed-product claim.
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
