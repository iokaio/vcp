# P2 host tool authority increment

P2-03 remains `in_progress`. The [source guide](../development/p2-host-tool-authority.md)
describes explicit immutable owner configuration, separate host/user denials,
canonical preflight and fresh observation admission. Original native tests use
synthetic files and the existing process fixture on both canonical stores.

The focused `host_tool_authority` case passed on both stores, exit 0, in
1.88 seconds. It rejects an invalid host-origin configuration before storage
opens, preserves a protected file despite user grants and policy replacement,
allows an unrelated read, rejects a read-denied search, denies an opaque native
process with no dispatch identity/marker, and checks recorded host-rule evidence.
Command: `artifacts/p2-tools-build.ps1 -Packages @('vcp-lifecycle') -TestTarget
canonical_host -Filter host_tool_authority`, using `cargo +1.98.0 test --offline
--target x86_64-pc-windows-msvc -j 4 -p vcp-lifecycle --test canonical_host
host_tool_authority -- --test-threads=1`.
Log: `artifacts/p2-host-tool-authority-native.log`, SHA-256
`c26d8c8a8c5b40d499db18bf67ad21390f298539a4b5928ba903d68488f61d0b`.

The first tool-suite build found a borrow-lifetime error in the added invalid-rule
test. The test now scopes each borrowed fact to its case; no product check was
relaxed. Failed manifest:
`artifacts/tools/1bec5351-81de-486d-ae37-ee87951349d2/manifest.json`.
The stable `pwsh -NoProfile -File scripts/test-tools.ps1 -Jobs 2` rerun passed all
28 tool/repository/authority contracts and 65 retained patch regressions, exit 0.
Manifest: `artifacts/tools/ff876ed3-3259-4746-86bd-9e74e5b65711/manifest.json`,
SHA-256 `923043586e290023b41e7290a0da9b842ff8218d8b6e432ed9af2193bdde36b4`.

The existing native file/process broker cases additionally check authority
rejection before native revalidation and explicit incomplete fresh observations
after coordinated changes. `pwsh -NoProfile -File scripts/test-integration.ps1
-Jobs 2` passed all 37 contracts, two retained launch regressions and the private
CLI trace, exit 0. Manifest:
`artifacts/integration/5622f6a4-239c-4379-a297-621ce89e1dce/manifest.json`, SHA-256
`c40fbfdda3af47a67684a6f229d8f23b8fb349e0590c6864c2f778013abcf86d`.

`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 78 foundation,
accounting, history and retained-host contracts, exit 0. Manifest:
`artifacts/p1/453a8486-c7fc-48f5-98ab-caf9165d0588/manifest.json`, SHA-256
`5febcc6c2431719fbdfa314d0128ccfb587dbedbe9fdd3c7fcedcb238b841147`.

`pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
-Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
-TargetRoot artifacts/upstream/codex-target -Jobs 2` passed native recovery,
exit 0. Manifest: `artifacts/build/204b3d11-9332-461b-b702-325f2749a2a3/manifest.json`,
SHA-256 `e8a588cb09c6a9cc1020d15d7865da98f0818e5122025dabaed31f24b2ff908a`.

The same native command with `-Mode LifecycleTests` passed all 29 retained
lifecycle regressions, exit 0. Manifest:
`artifacts/build/d50981fd-478c-4ba6-86b9-b67c0be6a859/manifest.json`, SHA-256
`b7469fd02eb842cc79865250fcb3cc9e43f3330267a5f26d064206bfba699108`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0. Manifest: `artifacts/tests/35fc367e-4539-4279-a8ff-cfc83f0d3a24/manifest.json`,
SHA-256 `4d5725f94ee082ff801f335e42ade59708edc41ba7c5b5d30e571e1aec9f1313`.
Repository links/task graph, static boundaries and diff checks passed. The
inventory remains 172 packages, 35 groups and 83 named seams. Remote CI is
verified separately on the published head.

No paid provider calls, new upstream imports, external dependencies, applied
migration changes or vendor patches are part of this increment. Evidence uses
native Windows build 26200/NTFS, MSVC 14.50.35717 and the explicit Rust 1.98.0
experiment; the original compiler selection remains unchanged. Retained tool
wrappers, coding-loop/context refresh, terminal interaction and full recovery
acceptance remain outstanding.
