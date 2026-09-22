# P7-02 paired generation qualification

The generation preparation and live runners provide two matched U03 attempts:
the frozen cart-discount feature with no active skill and with the packaged
JavaScript/TypeScript skill explicitly activated. Each starts in a separate
workspace and canonical store. Only `src/cart.cjs` is an allowed edit; the
independent oracle checks every other file and the complete file inventory.

Preparation makes zero model calls. Supply a private JSON spec with `executable`,
`profile`, `aggregate_cap_usd` (an exact USD decimal string), and optionally
`runtime: {"node": "absolute node.exe path", "launcher": "absolute launcher.exe path", "build_receipt": "absolute build-receipt.json path"}`.
The executable needs the exact current bundled skill assets alongside it.
Preparation and dispatch reject an executable that lacks the exact packaged
catalog bytes embedded by VCP. This catches a stale binary paired with newer
sidecars; byte presence is not build attestation. Qualify the rebuilt archive
with the native packaged-skill integrity test before live use, rather than
relying on help/version output or archive hashes alone. The
source profile must satisfy the fixed-provider requirements of the
[read-only runner](builtin-live-runner.md), except that it grants workspace
read/write authority. It must contain no executable checks or process authority.
P7 profiles explicitly request at most 8,192 output tokens within the qualified
provider maximum, at most sixteen requests and a 1,800-second deadline, with
zero retries. A 4,096-token, sixteen-request, 900-second profile is the initial
coding recommendation; the earlier P6-derived 512-token profile truncated both
live generation arms. These new bounds do not change P6 or authorize a new run.
Any new campaign uses its own exact-plan approved cap; original allocations and
unresolved liabilities remain held, with no recycling or replay of old attempts.
Runnable generation profiles need a deadline above the default 120-second
verification duration; 600 seconds leaves time for model work and verification.
The preparer rejects shorter profiles before creating trial state.
Use the existing environment credential boundary; never embed credentials.

Execution requires a separate explicit spec proposal:
`"propose_opaque_launcher_effects": true`. By default even a supplied runtime
produces a non-runnable workspace-only plan with no process authority. The
proposal lists all seven effects required by the broker's conservative process
classification: read, write, execute, network, install, publish and opaque.
The derived profile contains only the pinned argument-restricted launcher and
the frozen test command, with no other process, MCP or routing. This is a
reviewable permission proposal, not authorization: `permission_review` keeps
approval pending, and the user must approve the exact plan's permissions and
spend before execution. Engine effect classification is unchanged.

```powershell
node scripts/evals/builtin-generation-prepare.cjs prepare C:/private/generation-spec.json C:/private/new-generation-trial
node scripts/evals/builtin-generation-runner.cjs run C:/private/new-generation-trial/plan.json <authorized-plan-sha256>
```

The user must authorize the aggregate spend before the live command. The hash
binds the exact executable, catalog, profile, fixture, scripts, runtime and skill
assets. Changing any of these requires a fresh plan. The cap is divided equally
between the two attempts without recycling unused allocations. An immutable
execution claim prevents replay. An interrupted attempt or uncertain accounting
stops dispatch under that plan; it cannot be continued or retried by replaying
the claim. Preserve its canonical liability and full allocation until reliable
accounting resolves them. A fresh, separately approved campaign can run within
its own cap without reusing any original allocation or claiming those historical
liabilities are settled.

The optional runtime currently requires Windows and the exact pinned portable
Node 26.9.0 bytes. Its qualification-only launcher is restricted to the fixture's
declared test command, clears inherited environment values except SystemRoot,
and enables Node permissions with workspace read access only. The derived profile
grants process execution only through that launcher. Build a fresh launcher from
the tracked source using the installed compiler, without downloads:

```powershell
scripts/evals/builtin-generation-build.ps1 -NodePath C:/private/runtime/node.exe -CompilerPath C:/private/rust/bin/rustc.exe
```

The unique artifact directory retains the binary and a build receipt with exact
source, runtime, compiler, builder and output hashes; compiler version and argv;
embedded runtime/SystemRoot paths; and before/after input preservation. Passing
this receipt as `runtime.build_receipt` binds it into the plan and labels
provenance `recorded_local_build`. Validation rechecks every bound input. This is
a local observed build record, not a signed third-party attestation. A supplied
binary without a receipt remains explicitly `owner_supplied_unverified`; matching
binary/source hashes alone do not establish how it was built.

A completed CLI task must also have current canonical parent verification with
the expected passed `package.json#test`, known cost, retained output, and no
unresolved effects, outstanding issues or inspection gaps. The separate oracle
then checks 44 exact integer, rounding, malformed-input and non-mutation cases.
Expected outputs stay outside the candidate's filesystem read grant. The bounded
candidate process receives no network, write, child-process, worker or addon
authority. A successful oracle cannot override failed or absent CLI verification.

Run deterministic controls without model calls using a qualified runtime:

```powershell
$env:VCP_U03_LAUNCHER = 'C:/private/runtime/u03-check.exe'
& 'C:/private/runtime/node.exe' --test src/tests/contracts/builtin-generation.test.cjs
```

On September 21, 2026 the pinned runtime and a freshly compiled launcher passed
all six generation contract tests, including actual denied filesystem/network/
process operations, launcher argument rejection, seeded-failure/corrected-copy
oracle checks, default refusal to add process authority, explicit permission
proposal validation, added-process rejection and tampered-build-receipt rejection.
The combined debug, generation, live-runner and native-toolchain contract suite
passed 20/20 with zero skips; local receipt:
`artifacts/p7-u03-launcher-6970e56acc344e8797e73a895443e740/qualification-contracts.log`.
The same directory retains the launcher and exact build receipt. These are deterministic
controls with zero model calls. They do not qualify live generation usefulness,
independent model-output review, or the separate CR-06 interrupted-debug and
concurrent-human-edit scenarios.
