# P8 request-allowance production candidate — 2026-09-23

Status: production build and unpaid package smoke passed. P8 acceptance remains
open. This candidate contains the [request-allowance correction](p8-request-allowance-2026-09-23.md)
merged in PR 129, after the [approved campaign](p8-approved-campaign-2026-09-23.md).
Those paid observations used the prior executable and are not measurements of
this correction's effect on task completion.

## Exact candidate

The native Rust 1.95.0 release build completed in 21 minutes 7 seconds, with no
qualification features. Source content was unchanged across compilation and
upstream inventory verification passed before and after. Build source commit was
`9a1434e6185167aa30c527a055441a399b20121f`; the same change merged as
`b7700a13afd2a80d667aa20f17b5825763730435`. The receipt preserves the dirty-source
flag from retained preparer line endings; the content manifest identifies the
actual build inputs.

| Identity | SHA-256 |
| --- | --- |
| Source content | `ccd57974e4c2395e626deb83d0755961844586d91355bd60da7c00520fff425c` |
| Production executable | `b8d8ab89a5580a1b726144137c38c2bc6898da01964c4fd6f155b2e474ea2f40` |
| Unsigned ZIP | `d6a5daaf2e4efd931bb6988ecb7f4800bdff070f625955157db41d6675c607f5` |
| Build receipt | `74bc2ccc0694f7ec0fc34341b9466f28450e227e305303f293a50e154129eeef` |
| Package result | `6ca1fd4a976b460d7b2f79a9eddb3b26085e4cb46f1e970d98967944c3dccfde` |

Build receipt:
`artifacts/p8-allowance-production/7fc84aee-f236-410c-a24e-e1ec2490c0e4/build-receipt.json`.
Package result:
`artifacts/p8-allowance-package/fd1180ef-12f6-45ad-93a7-2da835828a20/result.json`.

## Unpaid package observation

The existing distribution qualification installed this exact package into a fresh
private directory with Unicode/space paths, exercised help, version, diagnostics
and missing-profile refusal, then uninstalled while preserving all four user-data
sentinels. All four command exits matched their contract. Measured wall times
were 583, 34, 36 and 40 ms respectively; these are single observations, not latency
acceptance statistics. No provider calls or model inference occurred.

Receipt under system TEMP:
`vcp-p8-allowance-distribution-7de12c0f-c385-4585-a5cc-6416db9b366c/result.json`,
SHA-256 `ad611bcf265f734be5462d78c9dc649df2a220b3f9d072dcba61f9c1f812323f`.
This used the current host with an empty profile/process environment, not a clean
Windows machine or network-isolation proof. The broader installation/upgrade
campaign remains recorded against its original package; it was not repeated or
relabeled for this binary.

## Remaining qualification

Six fresh owner fixtures were prepared with zero provider calls or reservations:
system TEMP `vcp-p805-allowance-4b64e06d-310a-4cbb-bfc9-3b3bad852181/preparation.json`,
SHA-256 `bbaec29d0eed7a02428a52da8b6a6a24109b176f9d7b88ac3b3688cca48bfb01`.
The existing launcher and all 23 controls were revalidated. A new paid execution
plan has not been created or authorized. The previous approval was consumed by
the six owner attempts and one interactive observation; no repeat is inferred.
The launcher receipt is
`artifacts/p805-page-launcher-v2-0623c5c1-94d9-430a-9117-b7ad1e22c67f/build-receipt.json`
(SHA-256 `4392449955dca38776378720d1b68b2e1c298824753fb94067cb605fae97dee0`);
controls are under system TEMP at `vcp-p805-page-launcher-v2-mDMivX/controls-receipt.json`
(SHA-256 `9da8d31f70b98f6dd2d2147b1ff702decc320f179679571da57ab1626625f6c1`).

The existing provider qualification expires at 2026-09-23 13:28:37.477 UTC, with
a 15-minute admission margin. Its original 24-hour compatibility window cannot
be extended by refreshing prices. Future execution after that window requires a
new bounded conformance probe and authenticated generation evidence before a new
profile and owner plan can be qualified. Preparation alone authorizes no spend.
The prior profile is under system TEMP at
`vcp-p705-quality-qwen38-review-70abeb52-3f33-4462-a4db-763a626576bc/source-profile.json`
(SHA-256 `78121630c5c81690bbde2f102745e872bdcfbb7d2d7ed4e0f7f9d0a3dabcec8f`).

### Reviewable renewal preparation

The old qualification coordinator launched requests immediately and could not
execute an already reviewed, exact spec. The P8-05 follow-up adds
`scripts/evals/p8-profile-renewal.cjs validate|run <proposal> <sha256>` for one
fixed two-request probe, no retries, with a 12,000,000-micro-USD cap. `validate`
performs no provider calls or reservations. `run` requires separate execution
authority; preparing or validating a proposal supplies none.

The coordinator pins the executable, spec, catalog, loaded repository helpers and
prepared campaign snapshot. It uses the existing owner coordinator's atomic lock
and checks for outstanding launches before reserving under the unchanged shared
$100 ceiling. This is a cooperative, exclusively scheduled campaign; the lock
does not coordinate legacy scripts with different or absent locks. No other
campaign coordinator may run concurrently.

A permanent claim prevents replay. Bounded execution has a five-minute deadline;
unconfirmed termination, missing/invalid accounting, changed inputs or unknown
liability retain the full cap. Even a refusal after reservation but before launch
conservatively retains the reservation and blocks subsequent launches; it is not
reported as a charge. Release requires an independently joined exact canonical
ledger, attempts and reservations. Known-cost failure can settle while remaining
failed. Credential-safe diagnostics and redacted stream hashes prevent raw output
from entering coordinator logs; detected sensitive output cannot pass.

Fresh unauthenticated endpoint metadata was captured at 2026-09-23 13:11:54.249 UTC,
with unchanged selected capabilities and tariffs. The new spec retains 512 output
tokens and the same immutable model/provider choice. Its proposed observation
window ends the following day; it does not extend the previous profile's window.
Preparation under system TEMP is
`vcp-p8-allowance-conformance-proposal-732d8798-4a94-4441-9c62-7ab6bd6d60b3/`.
Catalog SHA-256: `83206136323e1febde0d707b4cbdab64f30a3d327235a42be4705015056fa097`.
Spec SHA-256: `ebe778e63d6e40648ae156445d9ba34f602bf1a6eab4c68e2db5aa37d674c4dd`.
The draft v2 proposal validated without calls or reservations. Final bindings must
be regenerated and validated against the merged checkout before approval, because
checkout can change script line endings. Original proposals remain immutable.

All eight local coordinator controls passed: reservation/collision limits,
canonical accounting joins, known-failure settlement and sensitive-output handling,
unknown/unreaped/missing-report retention, freshness, prelaunch expiry, malformed
credential-bearing JSON, and input/helper tampering. Independent review found no
blocking issue. A passing probe would still require authenticated generation
receipts and the existing strict offline qualification join before producing a
new profile. Six owner attempts would then need their own frozen $8/task, $48
aggregate plan under the shared ceiling. Neither stage has execution authority.
All 17 registered fast-suite groups passed with the new controls included:
`artifacts/p8-profile-renewal-fast/38a7e7d6-14bf-4ef1-b765-ddb3f4901359/manifest.json`.
A separate read-only check ran the new accounting join against the retained
successful native probe, reproducing its 2,232-micro-USD charge exactly without
any new call; this guards against fixture-only assumptions about record shape.

Clean Windows, production network isolation, actual minimum hardware, physical
full-volume exhaustion, remaining integrated package scenarios and human
usefulness/correctness/architecture-fit acceptance remain open. Independent-machine
handoff remains owner-skipped. P8-05 is not complete; P9 retains that prerequisite.

### Authorized renewal outcome and distribution follow-up

The subsequent owner instruction authorized the bounded $12 renewal and up to
$48 for six owner tasks, with no retries and the existing $100 ceiling. The
merged-checkout `proposal-v3.json` validated with zero calls, then ran once.
Its SHA-256 is
`8086d3698f96f8070b514d393f46c9bd1da8ce41740551893f4e67e007eab069`.
The probe failed sending a request to `https://openrouter.ai/api/v1/responses`.
The process was reaped, but the canonical ledger retained 6,397,576 micro-USD
unresolved liability, zero active liability and zero settled cost. Actual charge
and provider receipt are unknown; the transport error does not prove non-delivery.
The coordinator rejected settlement and retained its full 12,000,000-micro-USD
reservation. No retry, qualification promotion or owner-task launch followed.

The shared ledger now records 3,637,947 settled and 78,127,269 reserved micro-USD,
leaving 18,234,784 under the unchanged ceiling. Prior unknown reservations remain
unchanged. Renewed qualification is a prerequisite for the six owner tasks;
additional authorization does not remove liabilities or relax admission checks.
The one-shot proposal is consumed, including this transport failure.

Receipts under the proposal directory above:

| Receipt | SHA-256 |
| --- | --- |
| `renewal-result.json` | `98b68334512a3762465b011d3d591a2e99863852daeac9411870d438ea29e0ba` |
| `probe/result.json` | `a6e2350877c43fa0a5f320f5c001f44e371ecaa28a01c8dd72612671e7325212` |

The unpaid `production-distribution-qualification.ps1` campaign passed all 16
commands and 32 assertions against this corrected package, using the prior
memory-enabled package as the distinct previous version. It exercised private
installation, interrupted upgrade/retry, compatible state round-trip, locked
rollback/uninstall and preservation of user data. This is current-host evidence,
not a clean OS, model task, network-isolation or physical-full-volume pass.
Receipt under system TEMP:
`vcp-p8-allowance-full-distribution/bbbf54fe-1508-49d6-8c3b-cbb6ecdb3222/result.json`,
SHA-256 `f37b5185e83b181cd2784e36c216b766407d84bc78513754526a1a1ac71324ff`.

Read-only environment inspection found Hyper-V tooling, but VM enumeration and
hardware/volume management queries were denied to the current token. No clean
test VM or disposable volume was established. C:, D: and E: are ordinary fixed
NTFS volumes and were not filled. The existing AppContainer filesystem-ancestor
incompatibility remains; no production guard or host ACL was weakened.
Clean Windows, independently controlled network denial, declared minimum-hardware
measurements and disposable-volume recovery therefore remain unexecuted.
P8-05 acceptance, final owner review, P9 and P4-01 remain pending their prerequisites.
