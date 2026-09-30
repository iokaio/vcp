# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The owner's request covers implementation through internal-beta readiness and
manual-test preparation. Publication and paid execution remain separate gates.

| Item | Status | Implementation and evidence |
| --- | --- | --- |
| BETA-01 | Complete | [PR #290](https://github.com/iokaio/vcp/pull/290) merged. Channel, versions, pinned installer, unsigned disposition, support exclusions and external gates recorded. Repository/harness, skill helper and Rust delivery checks passed. No release qualification claimed. |
| BETA-02 | Runtime fix delivered | [PR #293](https://github.com/iokaio/vcp/pull/293) merged with routine checks passing. Shared effective-profile loading binds base and import revision/content at launch, start and resume. Both stores passed 24 stale-selection and 12 actual MCP allowlist/deadline CLI/public-client scenarios, five import tests and existing coding parity. Synthetic loopback providers; final installed cross-client rows remain separate. |
| BETA-03 | Production workflow delivered | [PR #294](https://github.com/iokaio/vcp/pull/294) merged with routine checks passing. Explicit bounded provider setup/receipt renewal, create-only private workspace profiles and offline preflight. Six production provider and two onboarding tests passed, including credential reflection, retained liability, expiry and catalog matching. No paid calls; final live/clean-installed walkthrough remains not run. |
| BETA-04 | Native increment delivered | [PR #291](https://github.com/iokaio/vcp/pull/291) merged with routine checks passing. Strict build/package gates bind clean reviewed source, production target/features/version, tools and assets; pairing checks final bytes. Version-derived VSIX dependencies and native-generated schema source binding were corrected. Production build and strict VSIX integration remain candidate gates. |
| BETA-05 | Inventory increment delivered | [PR #292](https://github.com/iokaio/vcp/pull/292) merged with routine checks passing. Locked Windows normal/build graph: 997 packages, 509 retained license texts and 11 original source archives. Registry archives/extracted bytes and pinned Git caches verified. Seventeen focused tests pass; compiler-observed components and final archive/helper checks still require production artifacts. License provenance limitations remain explicit. |
| BETA-06 | Implementation delivered | [PR #295](https://github.com/iokaio/vcp/pull/295) merged with routine checks passing. Registered per-user setup, hash-bound stable launcher and installed data-root selection. Native script tests passed concurrency, abandoned-owner recovery, retained Files/SQLite format checks, WAL refusal and preservation; compiled Inno fixture passed nine lifecycle cases. Launcher integration and installation metadata tests each passed 3/3. Final shipping lifecycle, console cancellation and a distinct supported upgrade/rollback pair remain candidate gates. |
| BETA-07 | Implementation delivered | [PR #296](https://github.com/iokaio/vcp/pull/296) merged with routine checks passing. Strict beta VSIX binds original native build evidence and freshly compiled locked SDK/editor tools; setup guidance uses explicit User settings and the resolved installed engine. SDK 35/35 and editor 163/163 passed, followed by eight focused release/package regressions, candidate-output preservation and stale SDK/extension output checks. Review fixes reject ancestor compiler fallback, redirected compiler output and missing original native evidence. Final installed candidate and clean-host rows remain not run. |
| BETA-10 | Documentation and staging verified; delivery pending | Current README, installation/onboarding, recovery, known issues and safe support instructions. Native ZIP stages four source-bound guides and identifies the entry point. Eleven inventory/provenance tests, actual Windows ZIP/document hashes, repository links and PowerShell example parsing passed. Debug assembly proves staging only; final clean-installed walkthrough remains a BETA-09 gate. |
| BETA-08 | Planned | Candidate workflow and durable evidence packet. |
| BETA-09 | Not run | Final installed-product matrix; no candidate bytes yet. |
| BETA-11 | Not ready | Owner acceptance and distribution authorization depend on a qualified candidate. |

## Current environment limitations

The development workstation is Windows `10.0.26300.0` with development tools and
caches. Results from it cannot fill the clean-Windows row. GitHub authentication
works through the host keyring outside the restricted execution sandbox. No
provider calls, model downloads, public release or signing operation has run.

The original source-hashing failure was reproduced and traced to the restricted
sandbox's different Windows owner: sanitized child environments omit the injected
Git ownership exception. The harness suite passed when executed as the repository
owner, without changing global Git trust. Its receipt is
`artifacts/tests/46511575-b894-436b-a2d9-01e20505b242/manifest.json` (local generated
evidence). The earlier interrupted TypeScript builds remain historical observations
until fresh checks establish current status.
