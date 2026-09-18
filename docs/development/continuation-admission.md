# Controller continuation admission

This first P0-03 increment adds host admission checks to three retained Codex
start paths. It does not implement `/pause`, active cancellation or durable
recovery. The owning requirements are [P0-03](../plan/01-upstream-feasibility.md#p0-03--codex-lifecycle-seam),
[the architecture](../architecture/vcp-what.md) and the
[private protocol decision](../adr/002-internal-and-public-protocol.md).

## Implementation

All paths remain in the selected controller under
`src/third_party/codex/codex-rs/`; no second scheduler is introduced.

| Source | Change |
|---|---|
| `ext/extension-api/src/turn_admission.rs` | `TurnStartAdmission::admit_continuation_start` returns an owned permit or denial |
| `ext/extension-api/src/registry.rs` | Forwards the new hook to the injected host; absent hooks admit work |
| `core/src/session/turn_input.rs` | Parent-delegated child input and one-shot review delegates use continuation admission before starting |
| `core/src/tasks/mod.rs` | Mailbox wakeup obtains admission before reserving an active turn or consuming queued input |
| `core/tests/suite/turn_input_submission.rs` | Five new cases exercise the real retained controller and synthetic loopback provider |

Ordinary input continues using `admit_turn_start`. Existing hosts inherit a
permissive continuation hook so already-delegated work can finish during shutdown
drain. A future pausing host must override **both** hooks with one coordinated
authority boundary. Review delegates always consult continuation admission, even
when ordinary admission is open; otherwise that configuration bypasses the seal.

The controller holds each permit until the admitted start has been published or
abandoned. A host must account for permits acquired before its fence closed;
returning a permit is not proof that the resulting worker has stopped. Denied
mailbox starts retain pending input. Opening admission alone does not wake that
mail; explicit revalidation and a wakeup remain the host's responsibility.
Existing `ServerDraining` rejection remains internal compatibility vocabulary.
This change does not add a public schema or claim it represents a paused task.

## Native verification

With the [native prerequisites](codex-source.md), run from any directory:

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests
```

The wrapper resolves repository paths, verifies both imported source inventories,
and compiles the retained `codex-core` `all` integration target using locked Rust
1.95.0 dependencies. It runs Cargo from the retained workspace so its native
`.cargo/config.toml` applies. `--no-run --message-format=json` identifies the exact
test binary; the observer rejects missing/duplicate artifacts or failed builds.
No filename glob can accidentally select an older binary.

The observer uses the existing allowlisted environment/process-tree harness, an
owned profile, local synthetic test environment, a 180-second bound per group and
a 2 MiB capture limit. `RUST_MIN_STACK=8388608` applies only to the test subprocess:
the retained integration tests overflow the default libtest worker stack on this
Windows baseline. The linker stack setting does not configure those workers.
No paid provider or user credential is needed.

Five original drain tests and five new continuation cases must each appear once
with `ok`, alongside matching summaries with zero failures/ignored tests. Empty,
skipped, duplicate and missing evidence fails the observer. The captured logs,
binary/build-log hashes, source identity and result manifest live under ignored
`artifacts/build/`. Manual Windows qualification runs the same command on
`windows-2025` with `-Jobs 2` and excludes
owned profile contents from uploaded evidence.

## Remaining P0-03 implementation

The subsequent [scoped lifecycle milestone](scoped-lifecycle.md) implements a
registered-tree host, independent/inherited holds, retained interruption,
owner-loss denial and explicit readmission. It adds three identity and seven host
cases to the same native command. The original ten cases above remain required.
The obligations below describe the complete product boundary; the in-memory
host alone does not satisfy durable recovery, startup/effect fencing or CLI pause.

1. Bind the injected fence to workspace, owner, root/task identity and monotonic
   authority/steering revisions. Coordinate child registration and closing the
   root fence at one ordering boundary; a snapshot of known children is insufficient.
2. Recheck authority after awaited hooks and approvals, immediately before model
   requests and tool dispatch. Tag completions with their originating revision.
3. Cancel active model streams and structured tasks after sealing starts. Observe
   process termination through strict owned Windows jobs; do not equate a Ctrl+C
   byte or dropped future with stopped descendants. Preserve late receipts and
   unknown effects for reconciliation.
4. Connect checkpoint completion to canonical observations and outstanding effect
   reconciliation. Keep inspection and steering available while admission is
   sealed. Failure or owner loss must leave execution sealed.
5. Deliver private `/pause` and explicit resume through the same controller.
   Resume must revalidate workspace, policy, reservations and unresolved effects
   before issuing fresh authority. Exercise root/child pause, owner close and
   fresh-process reopen without repeating a non-idempotent effect.

These obligations remain prerequisites to completing P0-03. The existing
`suspend_turn_and_shutdown` closes runtime state and rejects live descendants;
`Interrupt` can restart pending work after abort. Neither is a completed VCP
pause operation. See the [evaluation](../evaluations/p0-03-continuation-admission.md)
for observed coverage and pre-existing native limitations.

## Source maintenance

[ADR-013](../adr/013-upstream-reuse-and-vendoring.md) applies. Patch
`0006-continuation-admission.patch`, modification comments, per-file original/result
hashes and root notices accompany the five changed imported files. No dependency,
lockfile, upstream revision or package count changes. Reconstruct independently
using the [source procedure](codex-source.md#explicit-reconstruction) and compare
the generated inventory with the committed one before publishing.
