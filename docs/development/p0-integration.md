# P0 retained-engine integration

This is a native, synthetic feasibility host over the committed Codex engine.
It is not the installable VCP application. Its evidence is recorded in
[P0-08/P0-09 qualification](../evaluations/p0-08-09-integration.md); the
[P0 handoff](p0-handoff.md) identifies the production work that follows.

The owning contracts remain [engine execution](../architecture/engine-execution-design.md),
[context/provider behavior](../architecture/context-provider-design.md),
[memory/retrieval](../architecture/memory-retrieval-design.md) and
[ADR-013 source maintenance](../adr/013-upstream-reuse-and-vendoring.md).
This guide records tested adapters within those contracts, with the bounded
exceptions below; it does not redefine the architecture around a prototype.

## Run locally

Install the native toolchains and MSVC prerequisites in [source setup](codex-source.md).
The integration runner defaults to Rust 1.98.0, shared with storage/local memory;
`-Toolchain 1.95.0` repeats the original controller compiler comparison. The
ordinary retained CLI baseline keeps its upstream 1.95 pin. Use PowerShell 7,
Node 24 and the committed Cargo lockfile. No API key or paid
provider request is needed:

```powershell
pwsh -NoProfile -File scripts/test-integration.ps1
pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests
```

The integration runner records source hashes, commands, exit codes, native
compiler/OS identity and logs in `artifacts/integration/<run>/manifest.json`.
It runs journal, controller, integration and neutral-port contracts, two retained
Windows process regressions and a separate native CLI process. Missing native
prerequisites mean `not_run`, not success. It rejects background Rust panics and
source edits during qualification. The lifecycle command separately retains the
core review/mailbox/continuation regressions.

The private `integration-owner.exe` accepts one new absolute disposable directory.
It creates `fixture.txt`, starts the retained loop against a scripted loopback
Responses endpoint, reads/prepares/changes `answer = 41` to `answer = 42`, invokes
a native assertion process, proposes/recalls governed memory, produces a summary,
closes and reopens its journal. It prints a JSON evidence record. It does not
accept real tasks or send their content to a service. The existing private
`lifecycle-owner` supplies the separately qualified in-app pause/resume and
fresh-process recovery controls described in [lifecycle recovery](lifecycle-recovery.md).

## Candidate boundaries

| Boundary | Concrete code and behavior |
|---|---|
| Controller and store | `src/crates/vcp-lifecycle/src/lib.rs` retains one registered Codex controller tree and one exclusive journal. `journal.rs` stores intents, receipts, lifecycle commands, flat-tariff reservations and memory evidence together. Codex rollout history is supporting input, never authority to spend or replay. |
| Provider | `integration.rs::configure_fixture_provider` injects a loopback-only Responses transport with explicit synthetic bearer token, zero HTTP/stream retries, no WebSockets, no environment/command/AWS auth, no telemetry exporters and disabled optional features. This is an offline OpenRouter transport candidate; live protocol/model compatibility belongs to P2-02. |
| Request accounting | `HostWorkAdmission::admit` checks the shared cap and appends reservation plus intent under the same lock before transport. `ResponseStream` passes provider usage to one atomic receipt/settlement. An interrupted request keeps its full liability. The declared fixture tariff is 100 synthetic units per request, independent of tokens; it is not provider pricing. |
| Helper paths | Isolated delegate and manager paths keep the host controls while removing other extensions. Unknown startup authority is denied; registered children share the root cap. Remote-memory/realtime unary requests remain disabled under a host gate; prewarming is suppressed. The qualification profile disables automatic helper features. |
| Tool ceiling | Retained `AllowedTools` limits advertisement and dispatch to `vcp_workspace`; host `admit_tool` independently denies every other tool name/namespace before hooks. Upstream executable hooks, MCP and skills are disabled in this profile. |
| Prepared effect | `WorkspaceTools` accepts one existing `fixture.txt`, a bounded whole-file update parsed by retained `codex-apply-patch`, an exact prepared patch digest and expected content digest. It rechecks under an owned file lock, writes/syncs through that handle and records the effect. A changed file or rewritten patch is rejected. |
| Execution | `process.rs` launches the explicit verifier with argv, cwd, cleared environment, bounded output and the retained non-breakaway Job Object. Its actual exit/output is captured before the model receives a verification result. |
| Local memory | Retained Munarium `run_gates` checks model proposals against the journal's scoped claim projection. Accepted and disputed proposals, findings and provenance survive reopen. Recall is labelled evidence, not instructions or permission. No second memory database or model request is introduced. |

Journal format 2 adds the integration ledger. The reader accepts legacy format 1
without that ledger, then appends format 2; it never rewrites old frames. Unknown
formats fail. This private prototype migration is not the P1 product schema.
The selected production storage candidate remains SQLite WAL/FULL, qualified
separately by [P0-04](portable-storage-spike.md).

## Gemini behavioral comparison

`src/crates/vcp-lifecycle/src/ports.rs` is an attributed, bounded Rust adaptation
of pinned Gemini canonical-argument and scheduler-state behavior. It has no
provider SDK types and never launches work. The shared fixture is
`src/tests/fixtures/gemini/ports.json`. Comparison executes the actual pinned
TypeScript modules; Node remains development tooling:

```powershell
node scripts/upstream/compare-gemini-ports.cjs --source <pinned-Gemini-checkout> --output-root <new-evidence-root>
```

Prepare that checkout using [Gemini baseline setup](gemini-baseline.md). The
comparison verifies its source identity, compiles it, records compiled/source
hashes and compares semantic outcomes. It preserves top-level null-byte
structural separators in Gemini's stable string format. The qualified Rust
subset accepts JSON with ASCII keys and safe integral JavaScript numbers;
functions, circular references, `toJSON`, floats and larger integers are outside
that subset. These bytes are a policy encoding, not a general JSON serialization.

| Scenario | Upstream observation | VCP rule |
|---|---|---|
| Out-of-order results | State manager retains call IDs and completion order `b,a` | Preserve those identities and actual completion order; reject duplicate receipts |
| Argument rewrite/confirmation | State manager replaces arguments and can retain a subsequently supplied outcome | Bind authority to argument hash plus revision; reject the earlier approval after rewrite |
| Client-origin marker | `checkPolicy` upgrades `ask_user` to `allow` for a client-initiated call without extra permissions | An input DTO cannot grant authority; require the host ceiling and durable intent acknowledgement |
| Resource conflict | State manager permits two active entries; higher-level upstream Scheduler owns arbitration | The candidate gate admits one writer per resource until its actual terminal receipt |
| Cancellation | Queued calls become cancelled; late updates after removal do not replace them | Pending work cannot start; executing cancellation retains resource ownership and records observed partial effects before release |

The comparison deliberately names the upstream layer. It does not claim that
Gemini's complete scheduler lacks conflict or confirmation handling. G06 skill
loading/MCP broker seams remain mapped in the component record for P7; G04 hooks
and G07 editor utilities remain deferred. Ports are qualification helpers until
P2/P7 integrate their constraints into general tool scheduling.

## Source maintenance

Patch `0010-integrated-host.patch` carries every retained-source change, including
the test builder's tool-ceiling injection. External dependency pins are unchanged.
Run the normal independent [reconstruction procedure](codex-source.md#explicit-reconstruction).
The bounded maintenance rehearsal is explicit:

```powershell
node scripts/upstream/rehearse-codex-fix.cjs --source <pinned-Codex-object-checkout> --output-root <new-evidence-root>
```

The source needs immutable commit `9daa491f7c27a5513fec554473a7122d88fca367`
and its parent `81b9bc210926b14b2af5c3300f13972909266aab` available as Git
objects. Acquire them explicitly from the recorded OpenAI origin if absent.
The experiment replays the already selected six-file Windows process-cleanup fix
at its parent, with the maintained VCP Job Object overlay. It records the patch
ordering conflict and resolution, verifies exact resulting bytes, and requires
the native retained regressions from the integration runner. This retrospective
experiment does not change the selected dependency revision or claim a release
maintenance qualification.

## Qualified limits

Trusted stable disposable roots, one bounded file, explicit synthetic pricing
and fixed Responses fixtures are intentional P0 limits. The file prototype is
not an atomic multi-file transaction or a defense against hostile root replacement.
Arbitrary CLI commands, real OpenRouter credentials/retries/pricing, installed
tool sandboxing, production migrations, durable index generations and product
key UX remain with P1/P2/P3/P5/P8. Preserve these negative boundaries when using
the candidate code as a starting point.
