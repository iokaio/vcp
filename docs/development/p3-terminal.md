# P3-02 terminal workflow

Text-mode execution on a native Windows terminal now keeps input available while
the canonical task runs. Redirected input/output, `--non-interactive`, JSONL and
`--control-stdin` retain the structured CLI behavior. The terminal uses native
cooked line editing and wrapping; it does not maintain a raw-mode screen or a
second scheduler. Selected retained C06 session/event and ConPTY components stay
behind the existing host boundary.

The immutable snapshot reducer shows the objective, current turn, fixed model,
known/reserved/uncertain cost, pending questions, effect/change references,
current-result verification and child states. Current verification is selected by
durable event order, and unresolved effects take priority in bounded summaries.
Routing groups, `/memory` and `/optimize` explicitly report their internal-stage
availability rather than claiming later services exist.

`/pause` and Ctrl+C fence admission and request interruption while the CLI stays
open. Effects can still be stopping or unknown. Repeated pause is safe. `/resume`
releases the retained hold and uses canonical environment/budget/effect
revalidation before submitting another turn. Guidance uses a durable steering
revision after coordinated interruption and leaves the task paused for deliberate
resume. `/cancel` ends the task; `/exit` preserves a durable pause.

Questions show durable IDs and have no implicit default. `/answer <id> allow|deny`
records only an explicit scoped decision; it does not resume scheduling. Expired,
superseded and previous-owner questions remain historical evidence rather than
blocking continuation forever. The engine still authorizes each answer.

`/status`, `/cost`, `/history`, `/agents` and `/inspect <id>` query existing
canonical services. `/read <artifact-id> <byte-offset>` requests a bounded raw
artifact range. `/next` advances display fragments and then the canonical page or
artifact range; changed canonical watermarks reject stale cursors. Full bytes
remain in artifacts. Terminal controls and bidi controls are escaped, Unicode is
preserved, and every display truncation is marked. Renderer updates coalesce in a
single watch slot independently of the eight-line input queue. Final outcomes use
the existing acknowledged owner-output path.

Same-process qualification exposed a P2-07 lifecycle defect: dropping a cancelled
model response recorded uncertain money but left its local producer without a
termination receipt. The correction records producer termination only after that
uncertain liability is durable. Failed cleanup still fences the host. The
two-backend control regression proves retained resume can proceed while the bill
stays uncertain and canonical pause continues to reject dispatch.

## Qualification

Native Windows tests use synthetic loopback responses and no paid provider:

- Fixed-state reducer tests cover scope, current-result checks, unresolved-effect
  priority, historical overflow, explicit question answers, Unicode/control
  sanitization, bounded input, slow output and output loss.
- A real hidden Windows console injects combining, wide and supplementary Unicode
  keyboard input, resizes to 20 columns, verifies that resize and partial input
  never submit an answer, explicitly presses Enter, and closes the window.
- The actual `vcp` executable runs through ConPTY with pause, repeated pause,
  steering, narrow resize, inspection, deliberate same-process resume and cancel.
  A separate case covers approval answers while paused without implicit dispatch.
- P3-01 executable cases continue to verify JSONL, owner authentication,
  cancellation, budget, verification, closed consumers, resume and fork.

Run `scripts/test-p3.ps1 -Toolchain stable` in the native Visual C++ environment.
Qualification fixtures are gated by the `qualification` feature and are not
production entry points. Automated console input/resize/close and ConPTY evidence
do not claim a human Windows Terminal visual review. Workspace discovery and the
broader close/reopen/child continuation campaign remain P3-04.

Completed on 2026-09-19 with native Windows 10.0.26200, Rust 1.98.1,
Node 24.21.0 and Visual C++ x64, using locked offline dependencies:

- `scripts/test-p3.ps1 -Toolchain stable`: 95 passing tests across contracts (83),
  executable workflows (6), native terminal (1), retained controls (1) and coding
  turns (4). Source hashes and logs are in the local ignored
  `artifacts/p3/29c46254-fe01-4fe0-88c7-368bbdc7e151/manifest.json`.
- `cargo +stable test --locked --offline -j 4 -p vcp-context --tests`: 13 passed.
- `cargo +stable test --locked --offline -j 4 -p vcp-lifecycle --test console_recovery`:
  actual close versus forced termination passed.
- `scripts/test.ps1 -Suite fast`: all eight delivery checks passed; local manifest
  `artifacts/tests/65e88dac-5ea3-43bd-821f-0fc670dcd3a8/manifest.json`.

The final equivalent sort-key lint cleanup was followed by all 11 CLI unit tests
and CLI Clippy with warnings denied. Changed Rust files pass rustfmt. The retained
dependency `proc-macro-error2` still emits its existing future-Rust compatibility
notice. Qualification artifacts are not committed; the checked-in runner
reproduces the native campaign.
