# P2 authority coordination increment

P2-03 remains `in_progress`. The [source guide](../development/p2-authority-coordination.md)
describes synchronous idle configuration, owned stopping and durable command
ordering. Native tests cover policy changes during pipe execution, steering
during PTY execution, dropped waiters, queued stale operations and deliberate
resume on both stores. A separate retained loopback stream test covers trust
revocation, exact partial response capture, uncertain charges, late usage and
preservation of an already-paused child.

The focused retained-stream test passed on both stores, exit 0, in 13.50 seconds.
Command: `artifacts/p2-tools-build.ps1 -Packages @('vcp-lifecycle') -TestTarget
canonical_host -Filter authority_stream`, using `cargo +1.98.0 test --offline
--target x86_64-pc-windows-msvc -j 4 -p vcp-lifecycle --test canonical_host
authority_stream -- --test-threads=1`. Log: `artifacts/p2-authority-stream.log`,
SHA-256 `01e961afa8a267d6ac98e21a81228ae004ffedccfd12c8f7a3073eaec4d22d35`.

The first full integration command executed all 36 contracts successfully but
its runner rejected the obsolete eight-test canonical-group count. The new
stream test makes nine. The expected group count was corrected, preserving the
total and individual group checks. That run is retained at
`artifacts/integration/deefdefb-f4e1-47f2-9a09-0b49e6e61bc7/manifest.json`.

The stable `pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` rerun
passed all 36 contracts, two retained launch regressions and private CLI checks,
exit 0. It includes the explicit zero-process observation after authority
acknowledgement and before the caller collects process results. Manifest:
`artifacts/integration/3e397ebe-88d2-4ecc-868c-5d6bbe8c7a9b/manifest.json`, SHA-256
`f3b370a2954947e0dc2c345c3e2a1044f1659672cda0b7b2bc0ba6c38318abc2`.

`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 77 foundation,
accounting, history and retained-host contracts, exit 0. Manifest:
`artifacts/p1/d7bf8377-ae12-4c06-ad90-c1e24c63ecc3/manifest.json`, SHA-256
`1750bc0cdde9a415d29d21b0c876a0d0733b770c9af11b18a4cffb5a60ad5b46`.

`pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
-Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
-TargetRoot artifacts/upstream/codex-target -Jobs 2` passed native recovery,
exit 0. Manifest: `artifacts/build/3c471be0-2cee-4492-bd32-929572bade24/manifest.json`,
SHA-256 `04953a2f49010295134ff6d8c5768664db65f8b26fda2e1ce7a5387790f50856`.

The same native command with `-Mode LifecycleTests` passed all 29 retained
lifecycle regressions, exit 0. Manifest:
`artifacts/build/a1c57af2-137e-4eb5-be3e-a0a83fd1f2da/manifest.json`, SHA-256
`587ff261f9a37d8fa1083fb634a780532259d24e696005f8aa1097af5bfa5795`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0. Manifest: `artifacts/tests/ee5be034-0a0f-43c7-bb96-f43a1564e9e1/manifest.json`,
SHA-256 `6845caef4824448c361fbfba1b7413eca8488586ac1a2872b695d09273ea14f3`.
The static inventory remains 172 packages, 35 groups and 83 named seams. Final
repository links, task graph and diff checks passed. Remote CI is verified
separately on the published head.

No paid provider calls are part of this
increment. Full host-rule, loop, CLI and recovery acceptance remain outstanding.
Native evidence uses Windows build 26200/NTFS, MSVC 14.50.35717 and the explicit
Rust 1.98.0 experiment; the selected pin and external dependency versions are
unchanged. This increment adds original lifecycle code and reuses the selected
retained streaming test helper and interruption APIs; no vendor patch or new
external source is imported.
