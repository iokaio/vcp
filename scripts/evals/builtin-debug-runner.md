# CR-06 bounded live debugging observations

This runner prepares four selected `review-debug` skill scenarios and makes no
model calls during preparation. They supplement the separate paired usefulness
cohort; they do not estimate a baseline-relative effect themselves.

`debug-v1` and its oracle remain unchanged. The explicit `debug-v2` manifest adds
a frozen `shipping.test.cjs` and a declared Node test script because VCP requires
an executable check before accepting modified-source work. V1's package manifest
has no test script and could support only independent external observations.
The scenario prompts, seeded sources and neighboring human notes retain V1 bytes.

Build the restricted launcher with the already installed Rust compiler and exact
qualified portable Node 26.9.0 runtime:

```powershell
./scripts/evals/builtin-debug-build.ps1 -NodePath <node.exe> -CompilerPath <rustc.exe>
```

The build receipt binds source, compiler, builder, runtime, embedded paths and
launcher bytes. Node accepts only the frozen test argv and runs with read-only
workspace access, no network/process/write capability and no provider environment.
The broker still conservatively classifies the outer process as opaque; this
restriction does not silently waive permission review.

A preparation spec names `executable`, `profile`, `aggregate_cap_usd`, and an
optional `runtime` with `node`, `launcher`, `build_receipt`. Only the explicit
boolean `propose_opaque_launcher_effects: true` proposes the process authority.
Use a current qualified fixed-provider workspace read/write profile without
existing processes, checks, routing or retries. The four equal allocations are
rounded down to integer microdollars and never exceed the aggregate cap.

```powershell
node scripts/evals/builtin-debug-prepare.cjs prepare <spec.json> <new-private-directory>
node scripts/evals/builtin-debug-runner.cjs run <plan.json> <authorized-plan-sha256>
```

Preparation is not authorization. Review the exact plan, scope, runtime, skill
assets, conservative process effects and spend ceiling before authorizing its
SHA-256. The runner rechecks all pinned inputs and creates a one-shot claim before
dispatch; interrupted work or unknown liability stops later scenarios. Replays
are rejected. Model and provider requests occur only in the authorized run step.

For the three available-reproduction scenarios, the sole `cr06-check` profile
accepts `--test --test-reporter=tap --test-concurrency=1 shipping.test.cjs`. The
prompt requests a native failing reproduction before editing and `vcp_verify`
afterward. Canonical process evidence must bind a real failing test to the frozen
source and a passing test to the final source hash, complete output and quiescent
process ownership. Canonical parent verification must independently record the
configured successful Node check. The separate V2 oracle also observes the
original and final threshold values and all preserved file bytes.

The missing-access scenario has only read/write effects and no process or check
profile. Its modified-source task is expected to remain incomplete, with no
executable pass. The runner records `preservation_passed_verification_not_run`
only when the source changed, no native process ran, CLI acceptance stayed
incomplete, and the independent preservation checks passed. Those controls do
not establish semantic correctness; independent review must inspect the fix and
the model's explanation of unavailable reproduction.

No result automatically qualifies live usefulness. Review the final answer and
source changes for evidence-based diagnosis, useful correction, accurate check
claims, retained human work and instrumentation cleanup. The interrupted and
concurrent-edit cases begin from frozen prepared states; they do not demonstrate
actual lifecycle interruption, recovery or concurrent mutation. Candidate code
can interfere with JavaScript globals or output; this bounded oracle is not a
hostile-code correctness proof. Source review remains required.

For native contract tests, set `VCP_CR06_BUILD_RECEIPT` to the exact current
receipt and use the qualified Node runtime:

```powershell
& <node.exe> --test src/tests/contracts/builtin-debug-runner.test.cjs
```

Without that optional build input, the native runtime test is explicitly skipped;
this is not a toolchain qualification pass. All source/profile/receipt contract
tests remain independent of model calls.
