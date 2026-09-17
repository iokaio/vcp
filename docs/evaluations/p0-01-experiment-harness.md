# P0-01 experiment harness evidence

Date: September 17, 2026. Task: P0-01, with no task prerequisites.
Scope: reproducible deterministic feasibility infrastructure. The source map,
fixture protocols, proposed workload sizes and threshold ownership are in
[experiment fixtures](../development/experiment-fixtures.md). All 19 ADRs retain
their stated engineering qualification gates; no runtime choice is accepted here.

## Executed checks

On native Windows x64, Node 24.10.0 and PowerShell 7.6.6:

```powershell
node --test src/tests/contracts/experiments.test.cjs
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

Both returned exit 0. The new six tests passed, as did the existing 14 harness
regressions. Repository checks validated relative links, synchronized guidance,
68 unique task owners, acyclic dependencies, the complete 56-task release
closure, 17 owner answers, 17 functional requirements, 19 invariants and 19 ADRs.
The complete run was recorded as `d79925bd-88bc-48b4-a09a-991a78028241` under
`artifacts/tests/`, including its dirty source identity and case output hashes.
CI runs these same registered cases on `ubuntu-8core` and attaches evidence to
its exact checked revision; local results do not substitute for that gate.

| Source/fixture | SHA-256 of tested bytes |
|---|---|
| `src/tests/support/experiments.cjs` | `440499770793c66ffcf6e2fd6cefd6598e1f26b4f36af61d8554e451e21fb720` |
| `src/tests/support/workload-graders.cjs` | `861d11943328550b5f29ba600208d1a9edfca423964ac0530e789e05a75d2d38` |
| `src/tests/contracts/experiments.test.cjs` | `b396c4a978a715e1c1205cddee179c3c37b4c3bd49b86c8a9561cb44d5959cf5` |
| `src/tests/fixtures/workloads.json` | `d6821d8aa69c8f49da2bc845b56a4aaab8e16fb4519693c22578cde4c14214be` |

## Failure behavior and interpretation

Unknown cases, child failure, missing prerequisites, cancellation, output
overflow and recorder failure cannot become passing evidence. The clock rejects
invalid time and runaway callbacks; the provider rejects extra/mismatched calls
without echoing request payloads; fixture cleanup refuses changed ownership;
independent graders reject incorrect and duplicate findings. Synthetic source
contains the intentional shipping boundary defect to make review measurable.

These results qualify the P0-01 infrastructure and truth sets only. No model has
performed analysis, review or generation. No upstream source has been imported,
and no local inference, storage, encryption, Windows sandbox, recovery or VCP
release capability is qualified. Paid model evaluation has no configured cap and
was not run. P0-07 and all dependent feasibility work remain separate gates.
