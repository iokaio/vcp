# P2 retained verification integration

Status: locally qualified on September 19, 2026. P2-05/P2-06 remain
`in_progress`. See the [implementation guide](../development/p2-loop-verification.md)
for scope and remaining work.

The retained loop can request configured native verification through a captured,
accounted `vcp_verify` call. Its arguments cannot define checks or provide a
success claim. The owning driver completes from the latest observed verification
only after retained work drains and a final model response is accounted for.
Current source, authority, evidence access and workspace effects are rechecked.
Final cost changes produce a new immutable verification with quantitative ledger
evidence; the original check report remains unchanged.

## Native acceptance

The new registered test covers seven scenarios on both SQLite and files stores:
passing checks, failed checks, omitted verification, mixed verification/tool
calls, a later native effect, an explicit named workflow denial, and a context
read denial that must result in zero provider requests and zero native effects. Each trace
uses the retained loop with synthetic local HTTP responses. A real Node
`node:test` fixture reads the source changed by the retained patch and asserts
the expected value. An independent filesystem marker proves actual check
execution. The fixture compares actual HTTP requests with settled canonical
cost, validates quantitative final-cost evidence, preserves original reports,
and rejects completion based solely on final model prose.

The first exploratory run hit the retained helper's short per-event timeout.
The fixture now consumes events directly under a separate 300-second bound.
The second run exposed a test assumption about response order: retained async
tasks can acquire the execution lock in a different order. Verification now
requires an isolated response. Mixed calls receive durable unexecuted pairs
before native dispatch; the next request can reissue verification separately.
The assertion that the sibling patch never writes remains in place.

The full regression run also exposed an invalid-call admission bug in this
increment: an invented pre-response call could reach stale handling and pause the
root before the first turn. Admission now rejects unknown/replayed identities
before that state-changing path. The existing loop fixture explicitly asserts
that its pre-response probe leaves the root running. Its event wait reports the
failing backend/scenario, avoiding the older helper's shorter nested wait.
The first broad run was stopped after its
known failure and is not acceptance evidence.

A grouped regression also exposed the old deadline fixture's one-second setup
assumption: under native load the deadline could expire before HTTP submission,
correctly yielding zero requests instead of the fixture's expected one. The
fixture now allows ten seconds for submission and delays the synthetic response
for twenty seconds. It still requires one observed request, no tool effect and
unresolved post-submission liability. This changes test timing, not product
deadline or accounting rules.

A subsequent focused run timed out during a multi-request turn under the old
30-second harness bound; the same compiled case passed independently. Root and
helper event waits now use one bounded 120-second native qualification wait,
while product deadlines and all request/effect/accounting assertions remain.
The corrected complete fixture passed in 140.13 seconds across its scenarios.
Completion review also moved final-response evidence selection into the same
owner/admission lock as the final transition, eliminating an intervening-turn
race between two worker calls.

## Commands and results

Native Windows `10.0.26200` / NTFS, MSVC `14.50.35717`, experimental Rust
`1.98.0 (88d9e12ae 2026-08-18)`, Node `v24.10.0`. The baseline toolchain pin,
vendored source and dependency locks are unchanged.

- `pwsh -NoProfile -File scripts/test-integration.ps1 -Jobs 2`: pass, 43 contracts,
  both retained launch regressions and the seven-request private host trace.
  Manifest `artifacts/integration/ac1dfbc6-753f-4205-ad4a-9ef61d6e982b/manifest.json`,
  SHA-256 `f38d0b8001c8b510d8c861b4ea65253b2fadd145d6e107744ca31dae4ba12b89`.
- `pwsh -NoProfile -File scripts/test-p1.ps1 -Jobs 2`: pass, all 84 contracts.
  Manifest `artifacts/p1/12a4597b-fd70-4b45-bf03-cc8abb318afb/manifest.json`,
  SHA-256 `c0ae66e99bc9d41603ab953c14b654bd1f1370102007fd99f6c0a7742f75ec3f`.
- `pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex
  -Mode RecoveryTests -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build
  -TargetRoot artifacts/upstream/codex-target -Jobs 2`: pass.
  Manifest `artifacts/build/30b2c2b7-1bdf-4528-905f-41686b6c1d39/manifest.json`,
  SHA-256 `443c74f02443821c42ec9124913af395a5ea97230aba56fb158945ffafc98297`.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: pass.
  Manifest `artifacts/tests/a5689f53-d934-48eb-95dc-209a48c1b859/manifest.json`,
  SHA-256 `42f77c05a0fcc0c2d537a7ffa3de4ef3524c3acc4d7085576ec59d8c23888d21`.

- The same native baseline command with `-Mode LifecycleTests`: pass, all 29
  retained regressions. Manifest
  `artifacts/build/d9da49b0-0293-4605-bc50-a2fa27612bc3/manifest.json`,
  SHA-256 `28fb4a20c73837d7c704d550bfd5d6ee89f0cfdcc450d3a21fe2fca85589d4ea`.

All final commands exited 0. Staged Rust/PowerShell bytes are compared with the
passing integration/P1 input manifests before commit. These are local results;
hosted fast CI is a separate confirmation and does not replace native evidence.

## Limits

This is an internal owning-driver API, not an installed CLI or a completed P2.
The driver must call `complete_coding_turn` after retained `TurnComplete`.
Actual Cargo and documentation-specific checks, independent-process verification
recovery, broader source/dependency coverage, retry orchestration, compaction,
ongoing terminal interaction and capped live OpenRouter smoke testing remain.
The previously recorded native source-pinning and transient external edit limits
remain. No paid provider calls, dependency updates or upstream imports are used.
