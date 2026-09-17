# P0-07 hosted Windows qualification

Date: 2026-09-17. [PR #11](https://github.com/iokaio/vcp/pull/11) added native
qualification on the owner-provided `win8core` runner, alongside Munarium's
`ubuntu-8core` label. [Run 35279871627](https://github.com/iokaio/vcp/actions/runs/35279871627)
passed both jobs. This establishes the committed Codex baseline's build and
reconstruction on a second Windows environment. P0-07 remains in progress for
other component selections and effect qualification.

## Source and environment identity

The PR head was `960831999944b44910bb4bb1f771fbe836f76d03`, based on
`dbd9d127785caa1d4eeb153802e266204c374540`. GitHub checked out its test merge,
`e83ad9f3aa24effff5d0e8d4833f68626cf17002`. Both head and test merge have tree
`cd8d2255da2e781595b5cb17c5b174a90295276c`; build and trace manifests record a
clean checkout. PR #11 merged as `5d9ed7c292aef6c38087351f9f8737a6399fa00c`.

The selected upstream remains Codex `3d3ae4965ab370217e871b3a7f0d15589557ee4b`,
with the recorded recursion-limit patch. See [source provenance](../development/codex-source.md)
and [compiler comparison](p0-07-common-rust.md) for selection and patch identities.

| Observed item | Value |
|---|---|
| Windows job / assigned runner | `105399087320` / `win8core-1000002521` |
| Linux job / assigned runner | `105399086891` / `ubuntu-8core-1000002520` |
| Windows image selection | GitHub-owned Windows Latest (2025), x64 |
| Native OS description | Microsoft Windows 10.0.26100 |
| Logical CPUs / physical memory | 8 / 34,354,229,248 bytes |
| Rust / Cargo | 1.95.0 / 1.95.0 |
| MSVC / CMake / Ninja | 14.51.36231 / 4.3.1-msvc1 / 1.13.2 |
| Node / Git | 24.10.0 / 2.55.0.windows.5 |
| Build configuration | Debug, `x86_64-pc-windows-msvc`, four Cargo jobs |

The workflow restores no VCP dependency or compiler cache. Rust and Node are
explicitly provisioned; Cargo downloads locked dependencies. Preinstalled image
tools are recorded above. This is one hosted run, not an offline build or a
minimum hardware/performance qualification. The configured 300 GB disk size is
a runner specification; peak build disk and memory were not measured.

## Executed checks

| Command or stage | Result |
|---|---|
| `scripts/test.ps1 -Suite fast` | Seven cases passed; all 40 regressions executed on Windows, none skipped |
| `reconstruct.cjs verify-index --component codex` | Committed file bytes and Git modes passed |
| `scripts/build.ps1` | Native CLI build passed, exit 0, 22:02:35–22:12:53 UTC (10m18s) |
| `scripts/build.ps1 -Mode BoundaryTests` | 102 tests passed, zero failed/ignored; exit 0, 22:12:54–22:15:19 UTC |
| `trace-cli.cjs --binary artifacts/codex-target/x86_64-pc-windows-msvc/debug/codex.exe` | Five native cases passed; eight synthetic provider requests and one expected patch effect |
| Independent acquisition and reconstruction | Exact upstream commit fetched into a new ignored Git object store; reconstructed JSON equals the complete committed inventory |

The native tests comprise 65 patch unit tests, 3 patch integration tests, 7
execution-policy unit tests and 27 policy integration tests. CLI cases cover completion,
read-only patch rejection, successful patch, a transient retry and provider
denial. The last case correctly exits nonzero while its expected-outcome oracle
passes. The separate missing-binary regression emits an intentional `not_run`
manifest; it is not the native trace result.

Reconstruction reproduced all 7,937 selected records, including the 34 executable
Git modes. Its aggregate file-record SHA-256 is
`f2f87e73619411696d7c62f51505b88f1d1f071353806b247a6beb1d3cc28a62`.
The downloaded hosted inventory was independently compared with the local
committed inventory using Node's strict deep equality.

## Failure and correction

The initial job queued without a runner until the owner moved `win8core` into
`wingroup` with repository access. The workflow label did not change. Public
repository access and any workflow restrictions are documented in
[Windows CI setup](../development/codex-source.md#native-windows-ci).

[Initial run 35278638423](https://github.com/iokaio/vcp/actions/runs/35278638423)
then failed the output-containment regression on `win8core-1000002519`.
PowerShell `Resolve-Path` retained a Windows 8.3 source alias, while .NET's
`GetFullPath` expanded the output path. Comparing those different spellings
failed to recognize output inside the source. The defect was reproduced locally
in an ignored fixture before correction.

The runner now applies the same full-path normalization to source, evidence and
target paths. A native regression supplies a short source alias and checks both
guards return `BASELINE_OUTPUT_IN_SOURCE` / exit 2 before creating output. All
40 local regressions passed in run `d2a7a0e7-744e-4b88-a772-f7aabb225322`, then
passed on the hosted runner. These lexical checks do not qualify junction races
or the VCP OS execution boundary.

## Retained evidence and limits

Artifact `windows-evidence-35279871627-1` is retained for 14 days. Its downloaded
local copy is under ignored `artifacts/ci/windows-35279871627-1/`; the initial
failure remains separately under `artifacts/ci/windows-35278638423-1/`.

| Evidence | Identity / SHA-256 |
|---|---|
| Fast-suite manifest | `c1e154d2-dadf-4028-b728-469e47cc7432` |
| Build manifest | `b7cb5645-db15-47f6-b86e-4fda099a900b` |
| Build log | `319ba988f7945ba562db90bd2b56f58411f23f0ce1af7b19ddbf5c63726a5e92` |
| Boundary-test manifest | `4cdfbba7-f2bf-4995-ac8a-8bd1dd58bb3a` |
| Boundary-test log | `0a8b13fa39383dc69fbde1c48c2847dd8928164c685562724dec05ba0f423c42` |
| Successful CLI trace | `50f1eb3b-7a66-4ff8-ba5c-28d926077602` |
| Tested binary | `1cb941cbeabe1d4ea6c49bb935e4059d9d5b67c0993ed8656c893ca4970ee085` |
| Native build runner script | `2c70a4a073ab7f8d0190872d6ded9a0a3876a040235411ec3477f1db7f600719` |

The artifact contains manifests/logs and the reconstruction inventory, excluding
binaries, acquired/reconstructed source and dependency caches. No paid provider
request occurred. The loopback observer does not prove absence of all other
network traffic; the successful patch fixture uses explicit unsandboxed access
to synthetic files. This run does not qualify OS sandbox enforcement, every
workspace member, optional features, a combined Codex/Munarium workspace, local
embedding inference, VCP accounting or in-app pause/resume.
