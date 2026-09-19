# P2 canonical coding milestone qualification

P2 remains in progress. The [source guide](../development/p2-canonical-coding-loop.md)
describes the native prepared wrappers, fresh canonical assembly, completed-call
eligibility and remaining acceptance work. The retained Codex loop remains the
scheduler. No additional upstream code or dependencies are imported.

The focused fixture uses synthetic loopback Responses traffic on both SQLite and
files/journal stores. It drives read, multi-file patch, read and explicit-profile
execution, then checks subsequent request bodies against actual file contents,
canonical effect receipts and full captured process output. A failing native
check preserves its nonzero exit and stderr. Nested instructions require a fresh
model request before an operation in the newly selected scope. Concurrently
changed instructions deny dispatch. Missing cost preserves liability and pauses
the root. A delayed provider response hits the shared coding deadline and retains
unknown usage without dispatching its proposed tool. Request limits hold independently of available money and survive
deliberate owner reopen; a helper configured with a larger allowance cannot widen
the root limit. Reopen restores original tool result references and
refreshes instructions without restoring executable call eligibility.

An initial missing-cost test exposed a host defect: `hold_uncertain` retained the
liability without pausing the root, allowing further requests. The host now
pauses after that accounting transition. Initial compile/fixture corrections and
all focused attempts remain in ignored `artifacts/p2-canonical-coding-*.log`.
An initial full integration run passed all 39 contracts, both retained launch
regressions and the private CLI trace. Its manifest is
`artifacts/integration/0e59af17-4056-490d-86a2-942bfa1d8508/manifest.json`, SHA-256
`d3f3740d917b5dd685e5f71890480ad9b4b2bd5eef049f4dfe73de4d9b380764`.
Subsequent review tightened shared helper limits and deadline checks; the final
stable-source qualification below supersedes that earlier run.

The final focused run passed all seven scenarios on both stores, including
reopen/helper checks, in 72.83 seconds, exit 0. Command:
`artifacts/p2-tools-build.ps1 -Packages @('vcp-lifecycle') -TestTarget canonical_host
-Filter canonical_coding`, which invokes `cargo +1.98.0 test --offline --target
x86_64-pc-windows-msvc -j 4 -p vcp-lifecycle --test canonical_host canonical_coding
-- --test-threads=1`. Log: `artifacts/p2-canonical-coding-native7.log`, SHA-256
`74f690a2684be76cc49c54a509bb82be6238eb7f7b4d6b0ae6be63fbeeb47782`.

`pwsh -NoProfile -File scripts/test-provider.ps1 -Jobs 2` passed all 17
provider/context contracts and the retained absolute-deadline regression, exit 0.
Manifest: `artifacts/provider/51d45acf-dd5a-47be-811f-cc691e145570/manifest.json`,
SHA-256 `5ed94680f4e16779ccdeaaf93b908be537591d645fb9a03268f4870e4ee60d26`.

`pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2` passed all 39
contracts, two retained launch regressions and the private CLI trace, exit 0.
The 12-test canonical-host group completed in 212.73 seconds. Manifest:
`artifacts/integration/01b5b976-4d99-45bc-8828-7ea686a9ab44/manifest.json`, SHA-256
`1906870b01babac2e75c19916f9eade7c8e5eed17b7eb66a55b64a3e2dcc8b2b`.

`pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2` passed all 80 foundation,
accounting, history and retained-host contracts, exit 0. Manifest:
`artifacts/p1/2533e257-afb9-4347-9da4-f78e94afc4ed/manifest.json`, SHA-256
`218b032ad25e858010b654aa4431243ab14efdfbe035f272acbbd7777759a802`.

`pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
-Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
-TargetRoot artifacts/upstream/codex-target -Jobs 2` passed native recovery,
exit 0. Manifest: `artifacts/build/fcee85b2-9dc9-46c4-84f1-1d59aaf26369/manifest.json`,
SHA-256 `91ababf849813db588ea48529d2c2aeed3a96e630e944733a3e0adc6f82aeb44`.

The same baseline command with `-Mode LifecycleTests` passed all 29 retained
lifecycle regressions, exit 0. Manifest:
`artifacts/build/a22bbaca-1323-4db5-8fe8-44531dc5f235/manifest.json`, SHA-256
`546bdd052d1cb9f5c4d87f7401645e82e7c36616bb24a88a693df915af165725`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0. Manifest: `artifacts/tests/bd66dcc8-7f4d-4737-95dc-90c8da2bdee8/manifest.json`,
SHA-256 `b34a9fcf925e2f08419fc1136025e3b327677ec1e3fa8a28e68acfb3486033b5`.
Final relative-link/task-graph and diff checks passed. Source verification retains
7,939 selected Codex files at aggregate
`8aceac845060e8b5ef3e32264ea0bba1ac84e4ce141e81e4a64c1fee19fb6179`.

This is native Windows internal-host evidence, not a live OpenRouter
compatibility claim or installable-product acceptance. No paid calls occurred.
The original Rust 1.95 selection remains unchanged; local qualification uses the
explicit Rust 1.98.0 (`88d9e12ae`, August 18, 2026) experiment with MSVC
14.50.35717 on Windows 10.0.26200/NTFS. CI is checked
separately against the published commit.
