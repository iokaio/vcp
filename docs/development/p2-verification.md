# Native verification and completion

P2-06 is in progress. The canonical host now discovers bounded project checks,
runs them through the existing native process broker, and requires current
verification before completing a task. This is an internal native adapter;
the installable CLI and complete P2 acceptance remain outstanding.

## Trusted setup and execution

`foundation::verification::VerificationConfig` is trusted owner setup, without
deserialization from model arguments. Configure native process profiles first,
then configure verification before any task effects. Each requirement names a
real project manifest, a runner profile, expected acceptance-test names and a
scope rationale. The original bounded workspace manifest and source bytes are
captured canonically. Reopening restores that baseline under current history
access, but requires fresh runner configuration and fresh verification.

`CanonicalHost::verify(thread, citations)` discovers checks from that current
configuration. Node support accepts `package.json` scripts of the form
`node --test explicit-file.cjs ...`; explicit targets must exist in the selected
project. Shell syntax, globbing, implicit test discovery and npm pre/post hooks
produce `not_run` records. The broker adds the TAP reporter and serial test-file
execution. Cargo discovery proposes `cargo test --locked --offline
--manifest-path Cargo.toml --all-targets -- --test-threads=1`; Cargo remains
responsible for parsing its manifest. These are proposed operations, not policy
grants. Missing profiles, prerequisites and denied execution remain visible.

The configured runner must be direct and nonterminal. It still uses current
policy, explicit filtered environment, native job ownership, process/time/output
ceilings, durable dispatch and complete output receipts. Streaming native file
hashes support executables up to 256 MiB without retaining their bytes in context;
ordinary source capture keeps its separate 64 MiB maximum. Native handles pin
executable identity through execution and final completion revalidation.

Native compiler profiles may explicitly provide `LIB`, `INCLUDE` and `LIBPATH`
alongside `PATH`, and a dedicated `CARGO_HOME`. No ambient environment is inherited. Registry credentials and
compiler-wrapper overrides remain unsupported. The Cargo fixture uses the
selected toolchain's real executables and a separate disposable build directory.

The bounded parsers require a zero exit, nonzero observed tests, complete runner
summaries and every configured acceptance-test name. Missing, failed, cancelled,
skipped or filtered tests cannot pass. The current TAP subset rejects nested
suites; the Rust parser rejects ambiguous duplicate names across targets. Full
stdout/stderr and native process evidence remain canonical even when parsing
fails. Runner output is untrusted execution evidence, not an independent proof
of semantic test coverage; the trusted requirements and synthetic native
acceptance fixtures establish the qualified scope.

## Source applicability and completion

Each verification records before/after source manifests, original/current source
artifacts, changed paths, accepted task/steering/authority revisions, check plans,
outcomes and cost certainty.
`verification-plan/1` is durable before checks begin. A
`verification-check-intent/1` artifact correlates the plan and proposed effect
before dispatch, so interruption cannot erase the intended check. Failed dispatch
consults canonical effect state: a possible submission is failed/uncertain,
whereas a missing prerequisite or positively undispatched check is `not_run`.
Native instruction loading includes applicable
`AGENTS.md` and absence/version probes even when ordinary discovery ignores the
instruction file. Outside parent instruction grants are not inferred.

Changed source, instructions, policy or task state makes evidence stale separately
from the observed check outcome. A passing process can therefore have stale
evidence. Required checks cannot be narrowed within an owner to hide an earlier
failure, and all earlier results stay in canonical history. A task initially
marked analysis-only still needs checks when its source baseline changes.
Unchanged analysis requires explicit complete, same-task canonical citations.
Changes outside every configured project block completion.

`complete_verified` requires quiescent retained work, the current owner's opaque
verification candidate, unchanged task/authority/effect revisions and fresh
native sources. It pins existing selected files against write/delete, checks
directory membership and dependency probes again, revalidates executable pins,
and confirms current access to every evidence artifact before the canonical
completion transition. Raw native-host `Transition(Completed)` commands cannot
bypass this gate. Canonical outstanding children, effects, issues and failing or
missing required checks still block completion.
The commit fence checks current effects across the workspace again, including
effects added by a terminal child after the root's verification was recorded.
The [retained verification integration](p2-loop-verification.md) additionally
records a new immutable completion candidate when only the final accounted model
response changes cost. Current quantitative ledger fields remain visible.

## Qualification and remaining scope

The [Cargo/documentation fixture increment](../evaluations/p2-framework-verification.md)
passed its focused native trace on both stores, full native integration/foundation
contracts and retained recovery/lifecycle checks. Results and limits are recorded
separately.

Run `scripts/test-tools.ps1`, `scripts/test-integration.ps1` and
`scripts/test-p1.ps1` on native Windows with the documented toolchain. The latter
two resolve and record the explicit installed Node executable and version. The
native fixture copies it to a disposable owned tool root, runs actual tests and
checks an independent filesystem marker. Both canonical stores cover success,
seeded failure, zero/incorrect tests, missing runner, mid-check edit, later edit,
cited/uncited analysis, changed analysis and an uncovered project path.
Additional cases exercise a late child effect, an ignored instruction change
and fresh-owner reopen in the same test process with edits made while closed.

The additional fixture runs real Cargo tests against an edited Rust function and
Node tests against a README link. Both preserve seeded failures before accepting
the repaired source. This does not qualify arbitrary test frameworks, Git/index
diffs, excluded files or dynamic dependencies outside the selected workspace. The native CLI has no
editor buffers; editor integration remains deferred.
Before/after hashes detect changed observations, not a transient edit restored
between observations. Existing files are pinned through completion, but Windows
does not provide an atomic filesystem-plus-canonical-store transaction for new
directory entries. Those acceptance limits remain part of P2-06 work. The model
tool wrapper and owning-driver completion API are described in the
[retained integration](p2-loop-verification.md). The installed CLI's automatic
end-of-turn control flow remains outstanding.

The implementation lives in `src/crates/vcp-tools/src/verification.rs`,
`src/crates/vcp-lifecycle/src/foundation/verification.rs` and its canonical worker module.
It reuses the [prepared process broker](p2-process.md) and
[canonical coding loop](p2-canonical-coding-loop.md), without a second scheduler.
