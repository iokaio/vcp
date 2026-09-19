# P2 execution ceilings increment

P2-03/P2-04 remain `in_progress`. The prepared process broker now binds a trusted
simultaneous process-count ceiling and requires native enforcement for both
pipes and terminals. Opaque operations conservatively respect scoped denials
because their declared inputs do not establish their complete effects.
The [process guide](../development/p2-process.md) describes the API and limits.

## Acceptance observations

The synthetic native trace exercises SQLite and files/journal. For both pipe
and PTY execution, a two-process profile permits the root and one child holding
a native file lock, rejects another child's marker-producing action, and
observes exactly two live Job Object members. Root exit must release the
descendant's lock and leave the excess marker absent. Abstract policy and
broker fixtures cover scoped write/install/publish denials without executing
those prohibited actions. Profile tests cover invalid ceilings and changed
approval digests.

The focused native trace passed, exit 0, with one test covering both stores in
82.43 seconds. Command: `artifacts/p2-tools-build.ps1 -Packages
@('vcp-lifecycle') -TestTarget canonical_host -Filter process_broker`, using
`cargo +1.98.0 test --offline --target x86_64-pc-windows-msvc -j 4
-p vcp-lifecycle --test canonical_host process_broker -- --test-threads=1`.
Log: `artifacts/p2-ceilings-native.log`, SHA-256
`3f89b6966838046a19235532bf660fbe6a6ebff4a925c0d2aa84032bdcfac733`.
No paid provider call is part of this increment.

`pwsh -NoProfile -File scripts/test-tools.ps1` passed all 28 tool/repository/
authority contracts and 65 retained patch tests, exit 0. Manifest:
`artifacts/tools/bc94822b-7ced-4509-bfee-77ed9a04c5db/manifest.json`, SHA-256
`7e5ce6a9a9240a33deed661fffcf4fabb34ba304f522df63cd5b6dd912c75b1d`.
The first tool run rejected an invalid new test fixture that omitted the write
effect for a declared writable resource. The fixture was corrected; no product
validation was relaxed. The failed run remains at
`artifacts/tools/1e1a1873-cef6-4388-965a-2d192ac2248e/manifest.json`.

`pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` passed 35 native
contracts, two retained launch regressions and private CLI checks, exit 0.
Manifest: `artifacts/integration/a39f0ad6-c7fb-44fe-a719-4ec40eb5ed05/manifest.json`,
SHA-256 `2492e8ac63e1be6413053679216e823f5912957f7e4df2ea40be14985aa9c5d9`.

`pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
-Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
-TargetRoot artifacts/upstream/codex-target -Jobs 2` passed native recovery,
exit 0. Manifest: `artifacts/build/bae7e423-c549-4182-aeba-558b10e561b2/manifest.json`,
SHA-256 `692044c81ad88c441e982ea7848a6218e41e51e09e735222a76f41823fe8cd87`.

The same native command with `-Mode LifecycleTests` passed all 29 retained
lifecycle regressions, exit 0. Manifest:
`artifacts/build/b2fabe64-0010-4d0d-a389-ff354bb4fe6d/manifest.json`, SHA-256
`e17f79a9f36783f8299223f6d855fcc48fe9186da569b051839f4bbfa32cf49a`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0. Manifest: `artifacts/tests/ebd6b100-ac6b-46e3-9f13-ec147edfb5ca/manifest.json`,
SHA-256 `9634fb8c69b0414db9268450cdda47973b4dcb21fa5c482fc43b3c198a320e4a`.
Final repository links, task graph and diff checks passed. Remote CI is checked
separately against the published head.

## Provenance and scope

Codex revision `3d3ae4965ab370217e871b3a7f0d15589557ee4b` and external dependency
pins are unchanged. Patch `0019-p2-process-count.patch` extends the selected
Windows Job Object utility with bounded owned-job creation. Its SHA-256 is
`e1770c0a22ab3121b4e5b609b169e5d911482192de7057a40ae22093f1570858`.
The 7,939-file aggregate is
`30fe21442b20dba458f8fd26709196b6f66f6fe848b0cc47f91b16c8d0a436e8`.
The static boundary inventory remains 172 packages, 35 groups and 83 named seams.
Independent reconstruction from the immutable checkout and ordered patches
passed with the same aggregate, exit 0. Record:
`artifacts/p2-ceilings-reconstruction.json`. The reconstruction used a
process-scoped Git ownership exception for that exact synthetic input checkout;
no global Git setting was changed.

The process count is a simultaneous membership limit, not a cumulative spawn,
CPU or memory quota. It does not establish filesystem/network isolation.
Ordinary retained upstream callers keep their existing behavior. Coordinated
authority changes, ongoing terminal interaction, retained-loop integration and
full recovery acceptance remain subsequent work. Native evidence uses Windows
build 26200/NTFS, MSVC 14.50.35717 and the explicit Rust 1.98.0 experiment; the
selected toolchain pin is unchanged.
