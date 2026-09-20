# P6 routing foundation qualification

This increment implements versioned routing catalogs, deterministic selection,
per-attempt model/price pinning, bounded escalation, and reversible local
optimization. Logical routing and decision contracts live in `vcp-models`;
canonical publication and admission remain in the existing lifecycle/store/budget
boundaries. No additional provider gateway or routing database is introduced.

The new configuration is optional for a workspace that has never enabled routing.
Once a canonical routing policy exists, reopening requires explicit routing
configuration; omission cannot silently bypass it. Catalog and policy refreshes
are checked before dispatch, while an admitted attempt retains its captured price.
Transport retries retain the original endpoint and consume durable bounded counts.
Quality switches use a fresh selected context, handoff, attempt and reservation.

The local optimizer reports retained task outcomes, known and uncertain spend,
support/child attempts, verification outcomes and observed usage latency. Missing
language, size, retrieval contribution and other unsupported metrics remain
unavailable. Comparisons expose cohort differences and uncertainty, and never
automatically apply a policy or rollback. Editable fields in this increment are
profile/order, quality floor and evidence requirements under trusted ceilings;
effort, context/retrieval, concurrency and remote-evaluator enablement remain
unsupported closed-schema edits.

## Evidence

Native Windows verification uses the provisioned stable Rust toolchain, locked
offline dependencies and ignored `artifacts/codex-target` outputs. The model
boundary suite passed 41 tests: 15 routing, seven escalation, seven decision
codec and 12 existing provider tests. The canonical routing-state suite passed
eight tests covering both Files and SQLite, including actual process termination
immediately before and after policy publication. Its ignored child entry point
is launched by the process-kill supervisor.

The initial full retained-host run passed 31 tests and exposed a new routing
record-envelope bug plus six tests competing for the process-wide maintenance
capacity. The record format was corrected without weakening store validation.
Native host qualification uses serial test execution for the shared maintenance
gate. The final serial run passed 38 tests with zero failures in 763.24 seconds
(`artifacts/p6-host-serial-tests.log`). Six explicitly gated asset/export cases
were not run in this invocation; the prior P5-08 qualification retains their
applicable real-asset evidence. This run includes the new end-to-end routing,
quality-switch, strict-pin, retry-cap and missing-configuration cases on both
storage backends.

Native executable checks exposed a Windows startup stack overflow before provider
dispatch. The retained launcher (`codex-rs/arg0/src/lib.rs`) uses the 16 MiB main
and worker stack budget declared in `codex-rs/async-utils/src/lib.rs`; VCP's prior
entry point used the smaller OS default. The VCP bootstrap now applies that
explicit budget without importing upstream ambient configuration or `.env`
loading. Executable verification removes `RUST_MIN_STACK` from CLI children.
All ten actual executable workflows passed in 209.82 seconds, the native terminal
case passed, and all three local-optimizer cases passed
(`artifacts/p6-cli-native-final-tests.log`). These exercise pipe and PTY commands,
pause/resume, current authority, source reconciliation, task verification and
profile-free optimizer access. The final general CLI suite passed 63 tests, with
one explicit external cloud-directory qualification case not run
(`artifacts/p6-cli-final-tests.log`). Its executable-help regression also exercised
the final bootstrap after restoring upstream panic propagation.

The repository fast gate passed all eight checks in
`artifacts/p6-fast-final/f711cada-f45e-41a6-826c-463cbacb8ddd/manifest.json`.
An earlier attempt rejected non-schema status text in the traceability ledger;
the statuses were corrected to `In_progress` without weakening the checker.
Source reconstruction and provider/effect boundaries remain part of that gate.

The frozen offline comparison passed all 54 assertions (18 tuning and 36
held-out), retaining every expected selection or stop and zero setup errors.
It also reran all 29 routing, escalation and decision contract tests. The final
run is `artifacts/p6-routing-qualification/7979cd4d-ca77-4b5f-b090-21c8c2fbac9b/manifest.json`;
its before/after bounded source digest is
`6cd5b8ae97b247d6a986eb29c9da7528f717f7567dd516cdd19c7fb62843c7ed`.
The complete report SHA-256 is
`f508ac8c967afc5e4b87e5f7a2add059e5166a25e5aadcce1fe2472bf2417529`.
The harness made zero model calls and spent zero dollars; actual task quality,
model cost and provider latency remain unmeasured. Changed-file Rust formatting
and diff checks passed. Clippy passed for all targets of `vcp-models`,
`vcp-lifecycle` and `vcp-cli` (`artifacts/p6-clippy-final.log`); warnings remain
in preexisting unchanged code and retained dependencies.

## Qualification limits

The frozen [offline experiment](../../src/evals/routing/README.md) compares three
strategies on tuning and held-out synthetic inputs. It tests selector decisions,
including rejection of scripted evidence, and records selector timing. It does
not solve model tasks or establish real model quality, price or latency.

There are no live-qualified group memberships or measured shipping profile
defaults in this increment. Actual Jev and conventional evaluator comparisons,
end-to-end live task outcomes, and P8 delegation/package comparisons are not run.
Remote evaluator dispatch remains disabled; the pure codecs grant no permission
to send. Paid trials require an explicit configured evaluation budget. The P6
milestone remains incomplete until its remaining qualification and policy-surface
acceptance conditions are satisfied.
