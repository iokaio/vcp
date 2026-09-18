# P0-07 native review and compaction traces

Date: 2026-09-18. Nine baseline cases pass through the rebuilt native Codex CLI
against a scripted loopback provider. Four new cases exercise successful and
rejected review and Responses-based compaction helpers. This qualifies their
observability in the upstream baseline; it does not implement VCP accounting,
canonical history, `/pause` or recovery. No paid request was made.

## Reproduction and identity

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode Build -OutputRoot artifacts/helper-trace-baseline-build -TargetRoot artifacts/upstream/codex-target
node scripts/upstream/trace-cli.cjs --binary artifacts/upstream/codex-target/x86_64-pc-windows-msvc/debug/codex.exe
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The local build passed with Rust/Cargo 1.95.0, MSVC 14.50.35717, CMake
4.2.3-msvc3 and Ninja 1.12.1 on Windows 10.0.26200, using four build jobs.
Its manifest is
`artifacts/helper-trace-baseline-build/93ccf760-318b-4a06-b7e2-ebc00df1c979/manifest.json`.
Build source is PR #15 head `f106ffd2e972fe96fbb0023d9120fe2264638a56`;
the subsequent merge did not change imported source. Dirty state is recorded,
not represented as a clean checkout.

The final trace ran at 03:10:11–03:10:35 UTC with Node 24.10.0 against local
edits based on merge `ebbc991b57cca51680c68df00bdb8226ec4838b8`, exit 0. Its
manifest is `artifacts/cli-trace/3faa2c2b-b8ac-4570-8758-ec6d2b88707b/manifest.json`.
It records source/diff identity, commands, raw request observations, response
status/usage and process logs. Exact inputs:

- Codex pin: `3d3ae4965ab370217e871b3a7f0d15589557ee4b`.
- Imported source: all 7,937 files verified against result digest
  `a48004db34f6f784c3312995939979ad27b37c81e04ee5e922c5d354d8da7b9f`.
- Executable SHA-256:
  `28b679854be98f31c3458792067d86c2ef53584212ff589aceaeb0347bdf5804`.
- Runner SHA-256:
  `6540e423bd65db7b4918c7b8578df76cf67b87a5d889bbd91835851d1aa1629e`.
- Observer SHA-256:
  `33f3a977b235825a9a3d0ede306b137a36fcb7ad381695864ba297b59f9a3928`.

## Observations

Usage below is synthetic input/output tokens supplied by the fixture. A failed
turn has no successful final CLI usage event; this does not erase earlier usage.

| Case | Requests | Exit | Supplied usage | Parent CLI usage |
|---|---:|---:|---|---|
| Completion | 1 | 0 | 10 / 2 | 10 / 2 |
| Read-only patch | 2 | 0 | 20 / 4 | 20 / 4 |
| Synthetic unsandboxed patch | 2 | 0 | 20 / 4 | 20 / 4 |
| Transient retry | 2 | 0 | 10 / 2 | 10 / 2 |
| Provider denied | 1 | 1 | 0 / 0 | No completed-turn usage |
| Review | 1 | 0 | 10 / 2 | **0 / 0** |
| Review denied | 1 | 1 | 0 / 0 | No completed-turn usage |
| Compaction | 3 | 0 | 30 / 6 | 30 / 6 |
| Compaction denied | 2 | 1 | 10 / 2 | No completed-turn usage |

The review request carries the retained review rubric. Its parent CLI usage
cannot account for the observed helper response. The baseline oracle expects
and reports this discrepancy; future VCP integration must replace that
expectation with admitted root/child request receipts and correct settlement.

Compaction is forced after a read-only patch rejection. The second request has
no tools, includes that rejection and a synthetic compaction prompt, and receives
a synthetic summary. The third request consumes the summary and restores coding
tools but no longer carries the earlier tool receipt. The independent observer
retains it. This establishes why model-facing history is insufficient as VCP's
canonical work history.

HTTP 401 at either helper produces a failed turn and exit 1. Compaction rejection
ends after the second request, with no following coding request. Review rejection
can display an interrupted-review explanation, which is not a success result.

## Scope and remaining work

The first hosted follow-up, [run 35302378321](https://github.com/iokaio/vcp/actions/runs/35302378321),
passed Linux but failed the Windows `upstream-inventory` suite: the existing
mutable compiler-alias rejection subprocess hit its 20-second PowerShell
deadline (`ETIMEDOUT`). The six helper-trace regressions passed. No altered
policy outcome was observed; the retained failure did not include enough
subprocess output to identify where the delay occurred. Evidence remains under
`artifacts/ci/windows-35302378321-1/`.

The follow-up gives these native preflight subprocesses a bounded 60-second
deadline and the enclosing upstream-inventory case 90 seconds. The rejection,
containment and no-output-allocation assertions remain intact. Timeout errors
now include captured stdout/stderr for diagnosis. This is execution headroom
for native tests, not a relaxed product policy or a passing result for the
failed run. The replacement head subsequently passed all hosted checks below.
The follow-up `fast` run passed locally with the same 56 regressions, exit 0;
its manifest is `artifacts/tests/5437d848-5a5f-4af6-8067-445bba4466f3/manifest.json`.

[Hosted run 35302784538](https://github.com/iokaio/vcp/actions/runs/35302784538)
passed both jobs for head `3ad1eb11c70f0596c74fa3a0de800ae109895986`.
Windows used `win8core-1000002548` in `wingroup`; Linux used
`ubuntu-8core-1000002547` in `ubuntu8core`. The Windows job passed source
reconstruction, 402 Gemini tests, local CPU embeddings, the CLI build, 102 native
patch/policy tests, 200 Munarium tests and all nine CLI traces. The successful
trace manifest is `cli-trace/ca822fb1-c824-4998-b9b4-6ec2d99b4468/manifest.json`
under `artifacts/ci/windows-35302784538-1/`. A separate expected `not_run`
manifest comes from the missing-binary regression and is not the native run.
[PR #16](https://github.com/iokaio/vcp/pull/16) merged as
`057acc893e51fb68e57e6deebd7cd6cff0c8b80f` at September 18, 2026, 03:45:12 UTC.

The [source/effect map](../development/helper-effect-traces.md) classifies eight
entries and describes the required gateway, history and pause adapters. Four
new checked anchors bring the catalog to 36 anchors across the unchanged 158
packages and 23 groups. No imported upstream bytes or dependency pins changed.

Six observer regressions exercise missing native prerequisites, bounded provider
behavior, false completion, independent patch effects, review usage and compaction
history/failure checks. Review identified a dropped-evidence risk when rejecting
extra requests; the final observer retains those records and its regression
checks their HTTP 400 status and body identity. The full `fast` suite passed
all eight registered cases and 56 regressions, including source/anchor and
documentation/dependency checks, exit 0. Its manifest is
`artifacts/tests/64dd124a-dd76-4116-bd86-daaf504962a9/manifest.json`.
The [initial five-case report](p0-07-cli-trace.md) remains historical evidence
with its own source/binary identity. Exploratory helper probes were retained
under ignored `artifacts/helper-trace-probe-*` and `artifacts/helper-trace-checked-*`;
the qualified run above uses the committed-source build and final runner/helper.

All successful observations are baseline behavior. Loopback observation does
not prove absence of other traffic. Provider-specific remote compaction,
background memory, guardian, realtime, credential discovery, crash recovery and
root/child in-app pause remain unqualified. The upstream fallback named local
compaction still calls a model; [local CPU embeddings](p0-07-local-embeddings.md)
are a separate facility. P0-07 stays in progress; P0-03/P0-08 retain their
implementation and acceptance obligations.
