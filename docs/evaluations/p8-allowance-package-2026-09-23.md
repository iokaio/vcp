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

Clean Windows, production network isolation, actual minimum hardware, physical
full-volume exhaustion, remaining integrated package scenarios and human
usefulness/correctness/architecture-fit acceptance remain open. Independent-machine
handoff remains owner-skipped. P8-05 is not complete; P9 retains that prerequisite.
