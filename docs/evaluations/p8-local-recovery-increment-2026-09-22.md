# P8-02 bounded local recovery increment — 2026-09-22

This increment addresses the first missing local cases identified by the
[receipt-to-boundary audit](p8-local-recovery-boundary-map-2026-09-22.md).
It preserves the earlier 46 passing executable receipts and does not rerun the
full matrix. Machine handoff remains owner-skipped. P8-02 remains in progress;
these native component and qualification-CLI results are not production-package
qualification or owner acceptance.

All four selected tests pass: thirteen fault scenarios comprising five capacity
failures, two corruption/rebuild sequences and six restore-activation cases.

## Selected evidence

Every command has a 600-second wall deadline covering compilation and execution.
The local supervisor terminates its process tree on expiry and retains command,
stdout/stderr, source inventories and outcome receipts. Rust uses 1.95.0, MSVC,
two Cargo jobs, `--locked --offline`, and the existing serialized native cache.
Fixture roots survive success and failure. No provider request or paid model
call is part of these cases.

| Case | Result and scope | Command wall time |
|---|---|---:|
| Canonical capacity failures | Two tests pass: real SQLite `SQLITE_FULL` (code 13) from a connection-local page ceiling, and injected Files `StorageFull` after partial header, payload, checksum and commit-marker writes | 11.419 s |
| Canonical versus derived corruption | Both backends pass fresh reopen, unavailable corrupt lexical generation, rebuild with identical nonempty ranked source hits, retained canonical state/receipt/history, and refusal of damaged canonical bytes despite a valid replacement index | 9.206 s |
| Interrupted restore with competing roots | Six scenarios pass: two stores × receipt-before-selection, descriptor-before-trust, and changed-candidate refusal | 53.464 s |

Capacity failures leave the writer unhealthy and unable to accept another
transaction. Reopen preserves the previously acknowledged state; retry adds
exactly one transaction, and subsequent retries before and after another reopen
return the same receipt. Files recovery retains the precise unacknowledged short
write as a quarantined tail. These cases do not exercise a physically full
volume, artifact finalization, journal-tip publication, vault staging or restore
staging exhaustion. The injected Files error is not an OS-observed ENOSPC result.

Corruption uses a populated lexical-only generation. Damaged derived data yields
no reader and requires rebuilding; acknowledged canonical records remain intact.
After rebuilding, the same query returns identical ranked hits. A byte change in
the Files frame or SQLite header then prevents canonical reopen and leaves the
damaged canonical bytes unchanged. This is a specific corruption schedule, not a
claim that every SQLite page or index corruption pattern is qualified.

Restore first selects a predecessor, then interrupts an encrypted descendant
restore with both canonical roots present. The supervisor durably records the
observed barrier and termination outside canonical state. Valid candidates win
even when the predecessor has a newer timestamp; a changed newer candidate is
refused without replacing the predecessor descriptor. Recovery preserves the
captured candidate's accounting, root/child task state, retained records, event
prefix and command/transaction receipts. Assertions require nonempty ledger,
reservation, attempt and settlement records and multiple tasks. Original-root
state/artifacts and a later local edit survive; an exact retry adds no mutation.
This proves preservation across activation, not independent capture/import
correctness. The CLI remains untrusted/Plan with tasks not resumed.

Each CLI child has a 180-second operation/barrier limit and a ten-second reap
limit. An assertion failure also kills the guarded child, retaining failed roots
without leaving a process parked at its barrier. No timeout occurred in the
selected runs.

## Receipt identity and reproduction

Receipts remain under `artifacts/p802-recovery/`. Each `result.json` records
command arguments, exact expected tests, exit status, elapsed timestamps, timeout
status, source identity and SHA-256 digests for command/log/source files.

| Selected result.json directory | SHA-256 |
|---|---|
| `disk-59dffc0d-d35e-4525-ad2c-820546070838` | `b53eb4f4a2c42ecdcf04cb87cf2305ec19af77123c6c858c6092ac3d7f14da03` |
| `corruption-358aa399-f4cb-46a0-b34c-58a669ed0b4d` | `299b79bc84cc763e696d547ef0d065426257d7ae259345e1246c2d425b19dfdb` |
| `restore-0282fe2b-760e-460f-9b55-b7e220ac8ac8` | `8d8375fc388af4840811fff216cf21fae3df9fd75772cfaffb3db62dda88a779` |

The verified join `artifacts/p802-recovery/selected.json`, SHA-256
`b7a3858573cdbc6be7d2bf2ce1a144f371b1a8e7f2f1cc0ef3e8e3b115074960`,
rechecks all referenced run hashes and retains the six restore cases' supervisor,
predecessor, candidate-state and recovery receipt hashes. Source content was
stable within each run. Only the CLI and memory test files changed between the
disk run and final runs; disk implementation/fixture inputs stayed identical.
The final bounded source inventory SHA-256 is
`e5a7636c22dd87a0c049e4fd155feafcf9af12000ba9eb8f57e91abbdb3dc9d7`.

The newly built qualification CLI SHA-256 is
`6f9fdd5652fe53ebd7b6a05b08c03dafd8a44f32f21b47b0a3cebfbfe0f39b7d`;
its restore test executable is
`53fa514982a4a7e56044bb7d17fd4dbca74937fda6e04cf061a7f36ffe4f6885`.
These are native debug test artifacts, not the earlier packaged CLI. The
retained unsigned ZIP still matches
`f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7`.

Run from `src/third_party/codex/codex-rs` through the
[native toolchain wrapper](../../scripts/evals/p8-native-command.ps1), under a
process-tree supervisor with the deadline above:

```text
cargo test --locked --offline -j2 -p vcp-store --lib disk_exhaustion_tests -- --nocapture --test-threads=1
cargo test --locked --offline -j2 -p vcp-memory --features qualification --test publication canonical_and_derived_corruption_have_distinct_reopen_outcomes -- --exact --nocapture --test-threads=1
cargo test --locked --offline -j2 -p vcp-cli --features qualification --test restore_crash interrupted_restore_with_competing_roots_uses_validated_selection -- --ignored --exact --nocapture --test-threads=1
```

Restore requires `VCP_TEST_PORTABILITY_FIXTURES` identifying the retained private
encrypted fixture directories and `VCP_TEST_RECOVERY_EVIDENCE` naming a fresh
local directory outside any repository. Recovery material is not committed or
copied into the public report. The test re-encrypts the same captured checkpoint
as its descendant; it does not establish new capture or cloud-transfer coverage.
The retained `p306-u04-20260920/recovery-inputs` fixture metadata hashes are
`a83e72c16fd0432271900b754e3eb8df44de12800914b4b726830a954546fb5d`
(Files source) and
`da48fa19db937ec79da9f2b40fc63822a203f91b6de7282a5cb0ab3947073d08`
(SQLite source). Fixture ciphertext hashes are checked before use.

The first disk build failed because the SQL API refuses unaudited dynamic SQL;
the fixture now uses literal `PRAGMA max_page_count=1` and asserts SQLite clamps
it to the current size. The failure receipt `disk-62e9595a-f580-4aae-bf14-07a050f33b17`
is retained. An initial corruption test passed with an empty index in
`corruption-682c05bb-c594-40db-8b9b-71be2fad7d8d`; review found that insufficient
for search visibility. Only the later populated/query-checked run is selected
above. Prior attempts are not deleted or relabeled.

The first restore attempt, `restore-f5eebf8d-b826-4b32-bebc-cdad48e82218`,
compiled within its deadline but failed key-import setup under the restricted
account. The selected native-account rerun can access the retained private
fixture; no key/path permission check was changed. Compilation retains existing
Munarium/lifecycle warnings; no warning-free workspace claim is made.

The fast delivery suite passed all 17 groups at
`artifacts/p802-fast/f444be4c-e992-420b-b911-0d3567b18167/manifest.json`, SHA-256
`8d577e06002b6abfb32e4cdeb8e56eb60267e1300259ea44dbfef957a49cbfdb`.
Affected Rust files pass `rustfmt --check`; `git diff --check` also passes.
The two earlier fast-harness attempts could not access repository Git metadata
under the restricted account and did not execute suites; the recorded successful
run uses the authorized native account. No failed product assertion was waived.

## Remaining acceptance

The boundary map retains unmapped artifact-finalization, full root/child
dispatch and selected combined-fault schedules, full-volume/staging exhaustion,
cloud hydration and offline-divergence limits. P8-03 sensitive-surface exclusion
and MCP compaction/reopen, P8-01/P8-04 production-artifact install/upgrade/rollback
and interactive pause/resume, and P8-05 frozen fixtures/thresholds and factual
owner acceptance remain separate subsequent work. This increment does not change
the debug package into a production candidate or waive machine-handoff coverage.
