# P0-02 — Offline CPU embedding qualification

Task state: `in_progress`. Real CPU inference and real missing/corrupt model
failures now run inside observed Windows AppContainer isolation. This supplies
the bounded offline-inference portion of P0-02. It does not qualify larger corpus
resource envelopes, durable canonical recovery or the product sandbox.

## Base and reproducibility

The base is `a7a5fe7946f938b2ca08896bf52df728e12ff885`, the merge of
[PR #19](https://github.com/iokaio/vcp/pull/19). Its exact head
`927fcb724f026095501c2099eaee653ac4eb2c43` passed
[run 35310366302](https://github.com/iokaio/vcp/actions/runs/35310366302).
Windows used `win8core-1000002559` in `wingroup`; repository checks used
`ubuntu-8core-1000002560` in `ubuntu8core`. Those are the preceding governance
change's results, not CI evidence for this new gate.

Executed locally on September 18, 2026, 06:08:49–06:11:58 UTC:

```powershell
pwsh -NoProfile -File scripts/test-embeddings.ps1 -AssetsRoot $modelRoot -Offline
```

The host was native Windows 10.0.26200 x64, Node 24.10.0, Rust
`1.98.0 (88d9e12ae 2026-08-18)`, MSVC 14.50.35717, four build jobs and twelve
logical CPUs. The source was the base above with uncommitted implementation;
manifests retain exact source/input hashes. The nine wrapper stages all exited 0:
asset verification, both source inventories, three library tests, release build,
dependency enumeration/check, ordinary numerical qualification and offline gate.

| Identity | Value |
|---|---|
| Wrapper manifest | `artifacts/embeddings/57e125f8-56a7-44ac-949a-00f838df47be/manifest.json` |
| Nested offline manifest | `offline/49449f6d-0712-44de-8b19-eb787acdd3ba/manifest.json` |
| Native binary SHA-256 | `613800d97179f185607695e6ce0950a408f4efcab44fa0e9cb9e105bea81832b` |
| Model specification SHA-256 | `ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11` |
| MiniLM revision | `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` |

The embedding closure remains 141 packages. No imported source, model assets,
dependency versions or reconstruction patches change. The static effect inventory
now covers 159 packages, 23 groups and 46 source entries: one pure, three
read-only and 42 effectful. The new effect is controlled traffic in the test
executable; the library remains file-only.

## Observed results

| Phase | Native exit | Actual token | Observation |
|---|---:|---|---|
| Control before | 0 | Not AppContainer | Parent observed the expected nonce |
| Real inference | 0 | AppContainer, zero capabilities, matching profile SID | Four cases/four checks passed; canaries before and after blocked |
| Missing `config.json` | 3 | AppContainer, zero capabilities, matching profile SID | `missing_asset`; blocked canary; expected rejection |
| Corrupt `config.json` | 1 | AppContainer, zero capabilities, matching profile SID | `invalid_asset`, SHA-256 mismatch; blocked canary; expected rejection |
| Control after | 0 | Not AppContainer | Parent observed the second expected nonce |

The listener observed exactly two connections, both controls, and no restricted
connection. Every blocked attempt timed out and returned a successful Windows
diagnostic identifying missing capability kind 2. The diagnostic also returned
kind 2 for successful unrestricted controls; the gate therefore requires all
independent observations instead of treating that diagnostic as sufficient proof.
All five profiles reported completed deletion and absence of their owned folder.

Contained CPU output had 384 dimensions, maximum reference/reopen delta
`1.7695128917694092e-7` and single/batch delta `8.754432201385498e-8`.
Load was 511 ms, batch inference 26 ms, total contained phase 5,264 ms including
two network deadlines. Peak job committed bytes were 233,705,472. This is not
resident RAM, mapped memory, build peak or a minimum hardware specification.

Four deterministic observer regression tests pass. They reject missing diagnosis,
connection refusal, wrong tokens, extra capabilities, incomplete capture, pending
cleanup, incorrect numerical/model identities, wrong asset errors and unexpected
traffic. A separate missing-executable invocation returned `not_run`/3 in
`artifacts/embeddings/missing-prerequisite/5ed07ae7-b306-4bb0-92f5-a6d20129ea6c/manifest.json`.
That prerequisite check was added after the full run; it does not change native
inference or the containment broker.

The full deterministic suite passed eight cases and 67 regression tests in
`artifacts/tests/21eb9ed2-a674-4cdb-bc93-918cccc01f2e/manifest.json`. This includes
documentation/ledger checks, both committed source inventories and the expanded
static effect catalog. `git diff --check` also passed.

## Failed probes and remaining gates

Preliminary ignored experiments under `artifacts/network-proposal/` were retained.
An initial assertion requiring WSAEACCES failed because this host reports a
timeout. A loopback attempt lacked a useful isolation diagnosis. The same-host
private IPv4 attempt supplied the diagnostic; a later probe demonstrated that
the diagnostic alone also occurs in successful unrestricted processes. The final
gate combines actual token checks, deadlines, diagnostic and both live controls.

A full `vcp-memory-spike` AppContainer probe failed. Correcting Windows'
local-app-data translation exposed a retained Tantivy 0.22.1 `MmapDirectory`
failure canonicalizing its generated temporary directory with AccessDenied.
Evidence is retained in `model-probe.log` and `model-probe-corrected.log`.
No filesystem permission or retained Tantivy check was weakened to make it pass.
The existing real corpus/index gate still runs separately outside AppContainer;
this new gate deliberately qualifies only the CPU embedding helper.

The broker also needs the two original Windows profile environment paths before
process startup; changing them after startup failed because Windows caches them.
The final bootstrap restores just those paths inside the harness's allowlisted
environment. The native child has its own explicit environment. No provider
credentials, model downloads during inference or machine firewall changes occur.

Forced termination of the trusted broker can leave its uniquely journaled
profile; missing/pending cleanup cannot pass. Product cancellation, crash recovery,
full-index containment, declared larger corpus/resource measurements and durable
canonical transactions remain separate work. In-app root/child `/pause` remains
required and unimplemented under P0-03.

See [implementation, limits and primary Windows API references](../development/offline-embeddings.md).
CI executes the gate on the configured `win8core`/`wingroup` runner and excludes
private configuration/journals from uploaded evidence. A local pass is not a
remote pass; publish and verify exact-head CI results in the PR before merge.
