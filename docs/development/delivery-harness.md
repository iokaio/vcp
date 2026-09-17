# Delivery harness

The first executable checks implement the runner portion of plan 00 and P0-01.
They validate repository contracts and the evidence recorder. They do not build
VCP or qualify any product feature. The current source check also verifies the
[imported Codex baseline](codex-source.md).

## Setup and commands

Install Git, PowerShell 7, and Node.js 24 or later. CI pins Node 24.10.0.
The pinned development-only TOML parser requires the install below. Private
credentials, model assets and paid calls are not required.
From the checkout run:

```powershell
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
pwsh -NoProfile -File scripts/test.ps1 -Suite repository
pwsh -NoProfile -File scripts/test.ps1 -Suite harness
pwsh -NoProfile -File scripts/test.ps1 -Suite experiments
pwsh -NoProfile -File scripts/test.ps1 -Suite upstream
```

An absolute script path also works from another directory. `-Case repository`,
`-Case harness`, `-Case experiments` or `-Case upstream-inventory` selects a case within `fast`. These cases support only
`-Backend none`; storage and other product suites remain unimplemented and are
rejected. The `upstream` suite tests inventory/reconstruction, baseline guards and
committed-source bytes; it does not compile upstream code. [Baseline setup](codex-source.md)
has separate native commands. `-OutputRoot` selects a local evidence directory (a relative override
is relative to the caller); the default is the checkout's ignored
`artifacts/tests/`. Keep overrides out of tracked or cloud-synchronized paths.

The PowerShell wrapper locates Node and forwards arguments. The Node dispatcher
resolves source paths relative to itself and validates the registry before
creating output or spawning a child. Case commands run Node directly with an
argument array and repository working directory. Future upstream executables
need an explicit dispatcher extension with tests; they are not implemented here.

## Registration and evidence

[registry.json](../../src/tests/registry.json) is the versioned inventory.
Reusable execution and recording live in
[harness.cjs](../../src/tests/support/harness.cjs); contract checks and regression
tests live under `src/tests/contracts/`. Each case declares task ownership,
arguments, supported backends, file/platform prerequisites, timeout, and maximum
combined output bytes. Add concrete cases with their implementation; do not
register success placeholders for future features.

Every invocation gets a new UUID directory and manifest. The manifest records
source commit, dirty status and hashes of tracked changes and untracked source,
registry/case definition hashes, Node/Git/OS versions, CPU count/RAM, exact argv,
timestamps, backend, task IDs, child exit status, log hashes, and limitations.
The case hash identifies its declaration; source identity identifies its code.
Repeated invocations retain separate evidence. Manifests are replaced atomically
after file synchronization; this is not a hardware power-loss durability claim.
An interrupted recorder may leave a `running` manifest or temporary file; neither
is passing evidence. Inspect retained logs when an attempt fails.

Tracked diff identity is streamed into SHA-256 rather than buffered, supporting
large source imports. External Git diff/textconv helpers are disabled. A failed
or cancelled hashing command cannot produce a successful source identity.

| Outcome | Exit behavior |
|---|---|
| Every selected attempt passes | 0 |
| Invalid selection | 2, before run allocation |
| Missing declared prerequisite | 3 and `not_run` |
| Child fails | Preserve its first positive exit code below 256 |
| Timeout, output overflow, signal termination, recorder error | Nonzero; explicit reason where recording remains possible |
| Cancellation | 130; subsequent attempts remain `not_run` |

Missing Node cannot produce a Node manifest; the wrapper reports not run and
exits 3. A recorder failure cannot produce a successful overall result. Logs
are bounded; exceeding the limit fails the case and labels capture incomplete.
Cancellation attempts to terminate the owned child tree using Windows `taskkill`
or a Unix process group. This is test orchestration, not a product sandbox or
proof of complete process isolation. Forced host termination cannot guarantee
final records or orphan cleanup.

Ordinary children inherit only the documented host path/temp/locale environment,
not provider tokens or Node startup injection. The recorder supports explicit
sensitive-value redaction across UTF-8 chunks; registered deterministic cases
have no credentials. This is not a general secret detector or network sandbox.
Use only public synthetic inputs. Future live evaluations require a separate
explicit budget and authority path; there is no live mode today.

## CI and qualification boundary

[Delivery checks](../../.github/workflows/ci.yml) runs every PR and push to main,
with read-only repository permissions and immutable action revisions. It uses
`ubuntu-8core`, matching Munarium's
[repository hygiene runner at revision 8da6660](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/.github/workflows/repo-hygiene.yml).
VCP runs its own checks, not Munarium component gates. Evidence is uploaded even
after failures and retained for 14 days. Runner availability is an organization
prerequisite; a queued or skipped job is not a passing check.

The additional `win8core` job runs the same fast suite plus the committed native
CLI build, patch/policy tests, scripted CLI traces and independent reconstruction.
See [Windows CI setup](codex-source.md#native-windows-ci) for its commands,
prerequisites and evidence scope.

The repository case checks Markdown inline relative links and heading anchors,
agent-guidance equality, original harness SPDX headers, task ownership,
architecture/ledger dependencies, cycles, completion prerequisites, and the
56-item first-release closure. It is not a full CommonMark parser or an external
URL checker. Imported Codex Markdown retains upstream links and is excluded from
that VCP-specific link check; its full file set and hashes are checked separately.
Regression tests exercise dependency parsing and recorder failures,
backend aggregation, missing prerequisites, cancellation, bounded output,
redaction, and isolated run identities.

Native Windows execution of these checks establishes harness portability only.
P0-07's [executed baseline results](../evaluations/p0-07-common-rust.md) cover
selected native builds and tests, with wider qualification still open. P0-01 adds
[experiment helpers and fixtures](experiment-fixtures.md), with proposed
measurement sizes and ownership of final acceptance thresholds. Product tests,
storage conformance, local inference, fault campaigns, and release qualification
remain future work under the [test guide](../plan/16-test-fixtures-and-acceptance.md).
