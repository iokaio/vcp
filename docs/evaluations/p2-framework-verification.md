# P2 Cargo and documentation verification

Status: native integration, foundation, recovery/lifecycle and fast checks passed
on September 19, 2026.
P2-06 remains `in_progress`.
This increment extends the existing [verification adapter](../development/p2-verification.md)
with actual Rust and documentation acceptance fixtures.

The Rust fixture uses a dependency-free Cargo package, a committed-in-fixture
lockfile and a named test that calls the changed Rust function. The documentation
fixture uses a real Node test of a README relative link. Both begin with a seeded
defect, require a failed check and refused completion, then repair the input,
rerun the same acceptance and request current-evidence completion. An independent
filesystem marker distinguishes executed checks from parser-only observations.
Earlier verification records and their output artifacts must remain byte-identical.
The fixture makes no model requests.

Native compiler profiles now accept explicitly configured `LIB`, `INCLUDE` and
`LIBPATH` and a dedicated `CARGO_HOME`, alongside the existing explicit `PATH`. They do not inherit the owner
environment. The fixture also selects a disposable Cargo home so ambient user
configuration cannot affect the test. The qualification runners resolve real Cargo/rustc binaries from
the selected toolchain and record their versions/hashes. Profile contracts still
reject credential settings, registry tokens and compiler-wrapper overrides;
model arguments cannot supply an environment.

## Validation

The focused native host fixture passed all four store/framework combinations
(SQLite/files × Cargo/Node), exit 0 in 140.13 seconds. The unchanged named checks
failed against seeded defects, passed after repairs, and preserved earlier evidence.
All 31 native tool/repository/policy
contracts and the retained patch regressions passed with the final explicit
compiler profile. `scripts/test-tools.ps1 -Jobs 2` ran with separate native
target/output roots from the isolated checkout, using Windows `10.0.26200`,
MSVC `14.50.35717` and experimental Rust `1.98.0`; the baseline pin is unchanged.
All 65 retained patch tests passed. Manifest:
`artifacts/framework-tools/8915a4ae-6345-4fc0-83bb-e3b81ef494f9/manifest.json`,
SHA-256 `308eb174ae09475becd03407a3ac449a9e1ce38caf25e80c9687c95bede4cacc`.
A separate prerequisite
probe successfully ran the real Cargo fixture with a cleared environment and
explicit compiler settings; that probe alone does not establish host acceptance.

`pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` passed all 45 native
contracts, both retained process-launch regressions and the seven-request private
CLI trace (exit 0). Manifest:
`artifacts/integration/ea65b333-7e59-47df-8a5c-15d310c3fd98/manifest.json`,
SHA-256 `4e2eea4def57e6df72d49e8158db3b9e78e6101e639900cda40e0b21772ee10c`.
All 54 input hashes from the final tools qualification also match this checkout;
unchanged tool/policy contracts were not rerun solely for branch transfer.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed (exit 0). Manifest:
`artifacts/tests/ffed6429-5f0d-4503-9a43-49884b0ecc26/manifest.json`,
SHA-256 `2ace4a93c51082ccc5f5ad0712fd7c155e17b0206695598b1ded869ca431606f`.
`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 86 foundation
contracts, exit 0. Manifest:
`artifacts/p1/750b6b85-8853-460b-93ec-e5a233cfc082/manifest.json`,
SHA-256 `94559c71437d64d6acbbee9f11e07c92394364a8e9ec24796a574d3b8384ebfc`.
The retained baseline runner passed `RecoveryTests` and `LifecycleTests`, both
exit 0, with `-SelectedCodex -ExperimentToolchain 1.98.0 -Jobs 2`,
`-OutputRoot artifacts/build` and `-TargetRoot artifacts/upstream/codex-target`.
Lifecycle validation observed all 29 retained regressions. Manifests:

- Recovery: `artifacts/build/107764b6-c8c5-4aa3-8784-7ff3f4167f6f/manifest.json`,
  SHA-256 `58e70e5d963c605a9bdeac899a8d20a4fb38c336c72a6e84db19982b5136a764`.
- Lifecycle: `artifacts/build/4271f6c8-8145-4520-a649-6bf33eead2ad/manifest.json`,
  SHA-256 `b378e0c2460bf8148efe825cb60824f1eb07cd3cb4ca6bb4d6705f9e8ba8098c`.

## Scope

The fixtures call the trusted verification API through the canonical process
broker on both stores. They do not yet establish an owning-model Cargo workflow,
arbitrary test frameworks, independent-process verification recovery, Git/index
diff applicability or external dynamic dependency coverage. Existing current
source/authority and completion gates remain required. No new upstream code,
runtime dependency or model asset is imported.
