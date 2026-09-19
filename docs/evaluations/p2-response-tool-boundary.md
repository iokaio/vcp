# P2 response tool boundary increment

P2-05 remains `in_progress`. The [source guide](../development/p2-response-tool-boundary.md)
describes deferred construction, successful terminal admission and discarded
calls after failure/cancellation. Native synthetic traces check gate counts,
actual marker files, request counts, complete/aborted captures and settled or
uncertain reservations on both stores. The default-mode control preserves the
retained streaming behavior.

The initial focused test's successful-control call was rejected by the test
gate's plain-versus-default namespace comparison. The fixture now uses the
existing `AllowedTools` namespace matcher. The corrected four-mode test passed
on both stores in 6.05 seconds, exit 0; the final full suite also includes the
default-mode control. Logs are retained at `artifacts/p2-response-boundary-native.log`
and `artifacts/p2-response-boundary-native-rerun.log`.
The successful focused log SHA-256 is
`7b4ff70fcbead339b6e9cc44e4c9fe533446c9a6f40ee79ae0088fcbdca27a26`.
Its command was `artifacts/p2-tools-build.ps1 -Packages @('vcp-lifecycle')
-TestTarget canonical_host -Filter response_boundary`, using `cargo +1.98.0
test --offline --target x86_64-pc-windows-msvc -j 4 -p vcp-lifecycle --test
canonical_host response_boundary -- --test-threads=1`.

`pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` passed all 38
contracts, two retained launch regressions and the private CLI trace, exit 0.
This includes the final five modes on both stores. Manifest:
`artifacts/integration/d2e702a5-f148-4789-ba98-e5b51c71f14b/manifest.json`, SHA-256
`9025ee62c2dfd7a3916ef528c0b4b7a8efcbe19d3803f8c980aa27bed3ee3fa1`.

`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 79 foundation,
accounting, history and retained-host contracts, exit 0. Manifest:
`artifacts/p1/3d0a61b2-e271-4853-b8f4-cd6322990f84/manifest.json`, SHA-256
`7603a4375f4c8918c13d734bf3710e1886ed3ca25b7dfc9c195e1a412abf664d`.

`pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
-Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
-TargetRoot artifacts/upstream/codex-target -Jobs 2` passed native recovery,
exit 0. Manifest: `artifacts/build/e1bac7fc-87b7-4786-b06a-7a9ddf91b8dd/manifest.json`,
SHA-256 `86b428afb9506644a50b99f19849d526f13911a929fb02d13ff9a45ed2ac7d4b`.

The same command with `-Mode LifecycleTests` passed all 29 retained lifecycle
regressions, exit 0. Manifest:
`artifacts/build/01a492e5-06f6-42e0-b9d3-ecdc4857861f/manifest.json`, SHA-256
`4dd9b1945757330befca80113e290892a4afe7824ee00c69359ba4a25a9772a6`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0. Manifest: `artifacts/tests/c7c807ab-d1f6-4b90-a720-720ce9c83cc2/manifest.json`,
SHA-256 `3c0e0115ccc53893bca65e590f8ac54e6cd12b36b8cb9857a0e27f82e2a0a088`.
Final repository links/task graph and diff checks passed. Remote CI is checked
separately on the published head.

Independent reconstruction from selected Codex revision
`3d3ae4965ab370217e871b3a7f0d15589557ee4b` and ordered patches matched all 7,939
files. Aggregate SHA-256:
`8aceac845060e8b5ef3e32264ea0bba1ac84e4ce141e81e4a64c1fee19fb6179`.
Patch `0020-p2-response-tool-boundary.patch` SHA-256:
`6ebea6efc96b0b82f92a8f79375b70fae8c73984e81f3fa407d3b2be6f42765e`.
Disposable result: `artifacts/upstream/p2-response-boundary-reconstruction`;
record: `artifacts/p2-response-boundary-reconstruction.json`. Static boundaries
remain 172 packages, 35 groups and 83 named seams.

Native evidence uses Windows build 26200/NTFS, MSVC 14.50.35717 and the explicit
Rust 1.98.0 experiment. Compiler/dependency selections are unchanged. The test
provider and marker tool are private synthetic fixtures. The canonical product
host still denies model tools pending prepared wrappers; automatic context,
full coding-loop/verification, terminal interaction and recovery acceptance
remain P2 work. No paid provider calls are included.
