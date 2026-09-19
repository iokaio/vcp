# P2 owned terminal increment

P2-04 remains `in_progress`. This increment adds bounded initial terminal input,
merged output capture and native owned ConPTY dispatch. The
[source guide](../development/p2-pty.md) records the API and platform limits.
Follow-up interactive input/resizing, retained-loop integration and full
recovery acceptance remain subsequent work.

## Native observations

The focused canonical process trace passed on SQLite and files/journal after
the terminal input pipe lifetime was corrected. The first run exited with
`STATUS_CONTROL_C_EXIT` when the writer was dropped after initial input; the
observer now keeps it alive until native exit. The passing trace independently
checks terminal handle detection, Unicode input, merged stdout/stderr, real
descendant lock release after root exit, terminal output limits, deadlines and
owner close. It makes no provider requests.

Focused command: `artifacts/p2-tools-build.ps1 -Packages @('vcp-lifecycle')
-TestTarget canonical_host -Filter process_broker`, using the documented native
MSVC setup, `cargo +1.98.0 test --offline --target x86_64-pc-windows-msvc -j 4
-p vcp-lifecycle --test canonical_host process_broker -- --test-threads=1`.
The passing log is `artifacts/p2-pty-input-lifetime.log`; one test passed in
28.75 seconds, exit 0.

Adding Windows PowerShell terminal coverage exposed a .NET initialization
failure for its verbatim (`\\?\`) executable spelling. The adapter now uses an
ordinary spelling only after a canonical round trip confirms the held path's
identity. With the same filtered environment, the full focused trace including
PowerShell passed in 35.56 seconds (`artifacts/p2-pty-application-path.log`, exit
0). The initial full integration failure remains recorded at
`artifacts/integration/3c396a57-cfdf-4810-a2f6-981d863fd6f6/manifest.json`.

The final tool suite passed 28 contracts and 65 retained patch tests, including
non-terminal input rejection, the initial-input ceiling and grant-digest changes.
Command: `pwsh -NoProfile -File scripts/test-tools.ps1`, exit 0. Manifest:
`artifacts/tools/f28b16d0-c0e8-45f5-b26a-bad9cb228a0e/manifest.json`, SHA-256
`49d91d71f8cb509a1d91615d89aed19448fe9525db3154b8c0504c0905590230`.

All three retained ConPTY lifecycle tests passed with `cargo +1.98.0 test
--offline --target x86_64-pc-windows-msvc -j 4 -p codex-utils-pty
win::psuedocon::lifecycle_tests -- --test-threads=1`, exit 0. These verify that
the existing upstream path still preserves descendants and closes output at
the established boundaries. Log: `artifacts/p2-pty-retained.log`, SHA-256
`f306b5601ab10741a06f22b1be58b853a5ff5083e38d2b5a1412f7365fae57b1`.

Final integration passed with `pwsh -NoProfile -File scripts/test-integration.ps1
-Jobs 2`: 35 native contracts, two retained launch regressions and private CLI
checks, exit 0. Manifest:
`artifacts/integration/4eb81caf-5d9c-4025-a32f-6aa8448c1489/manifest.json`, SHA-256
`3f2b2741db476319db9afcb96d165b5ed2c37193d7c31903d9ad680e1856db9d`.

Native recovery passed with `pwsh -NoProfile -File
scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode RecoveryTests
-ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot
artifacts/upstream/codex-target -Jobs 2`, exit 0. Manifest:
`artifacts/build/9c7a7bb8-2bcf-443c-b361-0dd85e86f58f/manifest.json`, SHA-256
`aaee1405c776fb3d11cd5448ffb2d9da63b49f53dbd640cccd4d51ae2872a165`.

The same native command with `-Mode LifecycleTests` passed 29 retained
regressions, exit 0. Manifest:
`artifacts/build/de547e40-f69a-4797-93ec-50538956c869/manifest.json`, SHA-256
`5c47ce576cd1045858b893278d2ab049a45b0e133c56c895193583b556a26954`.

A fast-suite invocation during that native rebuild timed out in source
verification (`artifacts/tests/ed782afa-39ae-4a78-b4f8-f1895a9c143e/manifest.json`).
Standalone source verification passed with the same inventory. The existing
timeout was retained for the final isolated rerun.

That final `pwsh -NoProfile -File scripts/test.ps1 -Suite fast` rerun passed all
eight cases, exit 0. Manifest:
`artifacts/tests/85710f8f-149f-4a1f-b45b-8cbbffd96a7e/manifest.json`, SHA-256
`100db857078b6036c9192ff6c46e8ef7b5bca8990e8627c59897302eec12cd3f`.
Final repository links, task graph and diff checks passed. Remote CI is verified
separately on the published commit.

## Provenance

The selected Codex revision and external dependency pins are unchanged.
Patch `0018-p2-owned-pty.patch` adds explicit owned-job launch to the retained
ConPTY path and an original VCP wrapper. Existing WezTerm MIT source notices are
preserved. The patch SHA-256 is
`690a89cd8fd6ea4ea9f0768a5685bdf0129dec92e4686fb39b1c9349b62ead8d`.

Independent reconstruction from the immutable checkout plus the ordered patch
series passed: 7,939 files, aggregate
`d5da30adab0271f35e09567b43bfb6485033e8a0e4c9158c69e412c8717b4be7`.
Record: `artifacts/p2-pty-reconstruction.json`. The boundary inventory has 172
packages, 35 groups and 83 named seams. Reconstruction needed a process-scoped
Git ownership exception for the exact synthetic upstream checkout; no global
Git configuration was changed.

## Limits

Native measurements use Windows build 26200/NTFS, MSVC 14.50.35717 and the explicit
Rust 1.98.0 experiment. The selected toolchain pin is unchanged. Older hosts
without `ReleasePseudoConsole` are denied before launch. Cmd terminal conversion
is not enabled. Terminal byte output can contain input echo and control sequences;
it does not preserve separate stdout/stderr identities or prove every original
application write before the console's rendering. No general filesystem/network
sandbox, paid provider compatibility or installed application is claimed.
