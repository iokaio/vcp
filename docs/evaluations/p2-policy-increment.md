# P2 authority core increment

P2-03 remains `in_progress`. This increment implements pure prepared-operation
decisions and durable canonical policy/questions/grants. It does not yet qualify
the native tool broker or permit model-requested effects.

## Local qualification

Executed on native Windows 10.0.26200 / NTFS with MSVC 14.50.35717 and
Rust 1.98.0. The compiler is an explicit experiment; the upstream 1.95 pin is
unchanged. Manifests record the base commit and per-input hashes of the tested
implementation. Raw evidence remains ignored.

`pwsh -NoProfile -File scripts/test-policy.ps1` passed all 47 contracts, exit 0,
including independent process-crash tests for storage, accounting and history.
Evidence: `artifacts/policy/08e15156-e1a9-450f-a0c8-9724a80318f3/manifest.json`,
SHA-256 `252c8069a724afb23ee4c61468f7973050416c9edb3b1d1ac3ac789b241c36f2`.
Contract log SHA-256:
`b9f1d33cb272d4228206153f41b9479007f6236f562990dd0f32b6fd46cdf153`.

`pwsh -NoProfile -File scripts/test-integration.ps1` passed, exit 0: 33 native
host/port contracts, two retained containment regressions and the independently
observed seven-request private CLI trace. Evidence:
`artifacts/integration/4649e115-d482-4f09-96a0-b27a7675092d/manifest.json`, SHA-256
`ca8f8d92a239a113f8257077fa3cc1995c44dc12c35b94d001199fad7252534e`.
An earlier run was correctly rejected when source changed during qualification;
this passing run uses stable final source.

The guide's explicit `build-baseline.ps1 -Mode RecoveryTests` command passed,
exit 0: four journal and sixteen host cases, independent owner recovery and
execution-boundary qualification. Evidence:
`artifacts/build/6ad4ad14-e3c6-47ca-ae8a-ab6e7846ebd6/manifest.json`, SHA-256
`77434b5bed88a7ded8ea77399441ab61f741f91ff98745ea05f8a15b558221ec`.

The guide's explicit `build-baseline.ps1 -Mode LifecycleTests` command passed,
exit 0: all 29 retained regressions. Evidence:
`artifacts/build/f676b563-12b0-4320-b37f-e9e1e9af5759/manifest.json`, SHA-256
`9d7595c2aee4a8d502762cfd37412ebc1ed827fa7fba490716330f3b616d4605`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0; evidence: `artifacts/tests/50f7e040-325c-46e6-9d44-00c218e9bdab/manifest.json`.
The first sandboxed invocation could not hash Git source; the recorded passing
run used normal local Git access.

Independent reconstruction from Codex `3d3ae4965ab370217e871b3a7f0d15589557ee4b`
and ordered patches matches all 7,938 files, aggregate SHA-256
`a264e6a79f332eaa5fdafcb40ae63af6785e5a508e43c3a57a3dc5aae59bcaf7`.
Patch 0016 SHA-256:
`8ed3ff5b38ddd4a26d4f65c1699e76d65ed534b7d5c5d25318a3d87b60de651c`.
External dependency entries and checksums are unchanged. The boundary inventory
covers 171 packages, 34 groups and 73 seam anchors.

## Acceptance boundaries

| Boundary | Regression coverage |
|---|---|
| Presets | Planning rejects mutation; ask reuses grants; workspace scopes edits; autonomous has an explicit effect ceiling |
| Denials | Trusted denial wins over existing grants and preset allowance |
| Approval identity | Changed arguments, schema, host, binding, authority, steering, policy, source version and limits invalidate the exact grant |
| Reuse | Repeated valid use needs no question; foreign scope, expiry and revocation reject |
| Independent controls | Approval does not enable missing isolation or resume held/stale work; money is absent from the policy evaluator |
| Process classification | Shell/process effects are opaque; rewritten arguments and resource-prefix escapes do not match a configured grant |
| Durable questions | JSONL commits waiting state and question together on SQLite/files; identical answers produce one grant/decision and conflicting answers reject |
| Pause and reopen | Answering while paused does not resume; pending questions survive reopening but old-owner authority rejects |
| Canonical compatibility | Existing domain/protocol, store, accounting and history contracts run with native crash qualification enabled |

The [implementation guide](../development/p2-policy.md) documents exact preset,
matching and compatibility behavior. All fixtures are original synthetic inputs.
No live provider calls or hosted Windows jobs are required.

## Remaining acceptance

Pure decisions and canonical questions do not establish native effect
enforcement. P2-04 must bind these decisions to freshly prepared tool operations,
durable dispatch intents and broker-only capabilities, then independently count
actual file/process effects under changed versions, denials, cancellation and
pause. P2-03 cannot be complete until that integration runs. General filesystem
and network isolation, product CLI input/exit codes and later extension policy
retain their owning gates.
