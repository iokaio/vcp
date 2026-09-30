# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The owner's request covers implementation through internal-beta readiness and
manual-test preparation. Publication and paid execution remain separate gates.

| Item | Status | Implementation and evidence |
| --- | --- | --- |
| BETA-01 | Complete | [PR #290](https://github.com/iokaio/vcp/pull/290) merged. Channel, versions, pinned installer, unsigned disposition, support exclusions and external gates recorded. Repository/harness, skill helper and Rust delivery checks passed. No release qualification claimed. |
| BETA-02 | Runtime fix delivered | [PR #293](https://github.com/iokaio/vcp/pull/293) merged with routine checks passing. Shared effective-profile loading binds base and import revision/content at launch, start and resume. Both stores passed 24 stale-selection and 12 actual MCP allowlist/deadline CLI/public-client scenarios, five import tests and existing coding parity. Synthetic loopback providers; final installed cross-client rows remain separate. |
| BETA-03 | Production workflow implemented; delivery pending | Explicit bounded provider setup/receipt renewal, create-only private workspace profiles and offline preflight. Six production provider and two onboarding tests passed, including credential reflection, retained liability, expiry and catalog matching. No paid calls; final live/clean-installed walkthrough remains not run. |
| BETA-04 | Native increment delivered | [PR #291](https://github.com/iokaio/vcp/pull/291) merged with routine checks passing. Strict build/package gates bind clean reviewed source, production target/features/version, tools and assets; pairing checks final bytes. Version-derived VSIX dependencies and native-generated schema source binding were corrected. Production build and strict VSIX integration remain candidate gates. |
| BETA-05 | Inventory increment delivered | [PR #292](https://github.com/iokaio/vcp/pull/292) merged with routine checks passing. Locked Windows normal/build graph: 997 packages, 509 retained license texts and 11 original source archives. Registry archives/extracted bytes and pinned Git caches verified. Seventeen focused tests pass; compiler-observed components and final archive/helper checks still require production artifacts. License provenance limitations remain explicit. |
| BETA-06 | In progress | Per-user setup and stable launcher under implementation. Engine lifecycle mutex, abandoned-owner recovery and actual retained format refusal passed Windows script tests; full installed-product lifecycle remains not run. |
| BETA-07 | Planned | Beta VSIX and installed-engine onboarding. |
| BETA-10 | Planned | User walkthrough, packaged docs, recovery and known issues. |
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
