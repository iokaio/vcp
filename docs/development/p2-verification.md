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

The P2-06 .NET increment adds `dotnet` requirements for observed C# project and
solution manifests. The qualified command uses `dotnet test` with `--no-restore`,
`--disable-build-servers`, `--nologo` and the normal console logger. Dependencies
must be restored separately through the configured process authority. Solution checks execute in the solution
directory and cover its observed source scope; a test project check cannot claim
coverage of sibling application sources. Solution project paths must remain within
that directory and identify observed C# manifests. Unsupported solution forms
produce `not_run` rather than inferred coverage.

The console parser requires an explicit successful run, matching nonempty test
counts, distinct passed names and every owner-configured fully qualified test name.
Failed/skipped result rows must match VSTest's test-name and bracketed-duration
format; application log messages such as `Failed to determine the https port for
redirect.` are not test results. Failure summaries and incomplete counts still
reject verification.
Skipped, failed, ambiguous, truncated and unsupported localized output cannot prove
completion. These checks qualify the VSTest console format for one test assembly;
multiple success summaries are rejected. They do not qualify arbitrary .NET test
platforms or model assertions about test quality.

The .NET SDK needs explicit public directory values even without restore. Owner
profiles may configure `APPDATA`, `LOCALAPPDATA` and `ProgramFiles(x86)`; no ambient
environment is inherited. Scenario B assigns the first two to fresh directories
outside its source tree and resolves the last through the Windows known-folder
API. Credentials, arbitrary SDK variables and model-supplied environment remain
excluded. Verification disables build servers so it cannot reuse a worker
initialized with another invocation's environment.

Build output exclusions must be explicit in the observed project configuration.
Scenario B's .NET template supplies a `.gitignore`; the qualification fixture
likewise excludes its specific `bin` and `obj` directories before establishing
the source baseline. Generic directories with those names remain visible to
repository discovery. Otherwise regenerated SDK files correctly make check
evidence stale. The [.NET qualification](../evaluations/p2-dotnet-verification.md)
records the supported toolchain and both-store evidence.

Maven requirements use an observed `pom.xml` and a configured Java process
profile. The owner supplies a `maven` object with absolute `home`,
`classworlds_jar` (under that installation's `boot` directory) and
`classworlds_conf` (`bin/m2.conf`) paths. Discovery constructs literal Java
ClassWorlds arguments; it does not execute `mvn.cmd`, enable a shell, or accept
extra launcher options. The current project directory supplies Maven's base
directory. Verification runs offline `clean test`, disables test skipping and
requires tests, with Surefire's plain console reports and color disabled.
Dependencies must already be resolved through authorized process execution.
The parser reconciles distinct `package.Class.method` results with every running
class's nonempty count, the aggregate count and a single successful build.
Missing method results, skipped/error/failure counts, duplicate names and
multiple aggregate summaries fail closed. This qualifies ordinary single-project
Surefire output; reactor builds, localized output and test engines that omit or
duplicate method identities need separate qualification.

Pytest requirements use an observed `pyproject.toml` and the configured Python
process profile. Discovery runs `python -B -X pycache_prefix=<source namespace>
-m pytest -vv --color=no -o addopts= -p no:cacheprovider`. The cache namespace is
`.vcp-verification-pycache/` plus a digest of the complete observed source
manifest. This bypasses ordinary timestamp-based bytecode caches, including
same-size edits made within one timestamp interval; `-B` prevents new cache
writes. The namespace is source-specific, not a sandbox against deliberately
forged caches or test output. Clearing `addopts` prevents configured filtering or quiet
output from hiding required tests; pytest still reads the project's test paths
and other configuration. Distinct exact node IDs such as
`tests/test_cli.py::test_version_flag` must all pass, and the collected count and
final passed count must agree. Skipped, deselected, failed, xfailed/xpassed,
duplicate and missing results cannot prove completion. Output from both runners
uses the existing one-MiB parser ceiling and current process receipts; an exit
code alone never satisfies either check. Build/cache directories must be
explicitly ignored before establishing the observed source baseline.

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

An accounted final response with no prior verification can trigger a local
integrity observation at the quiescent completion boundary. This narrow path
requires an unchanged source baseline, no editing flag, and no configured,
task-required or discovered checks. It supplies the final response's canonical
citations to the existing verification pipeline and dispatches no process.
Current verification denials, authority, source, effect and accounting fences
still apply. It cannot replace an earlier failed verification or complete changed
work without required checks; semantic answer quality remains separately graded.

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
