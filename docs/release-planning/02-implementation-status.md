# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The owner's request covers implementation through internal-beta readiness and
manual-test preparation. Publication and paid execution remain separate gates.

| Item | Status | Implementation and evidence |
| --- | --- | --- |
| BETA-01 | Complete | [PR #290](https://github.com/iokaio/vcp/pull/290) merged. Channel, versions, pinned installer, unsigned disposition, support exclusions and external gates recorded. Repository/harness, skill helper and Rust delivery checks passed. No release qualification claimed. |
| BETA-02 | In progress | Shared effective-profile admission and CLI/public-client parity. |
| BETA-03 | In progress | Production setup/renewal prerequisite under investigation; do not fabricate qualified provider metadata. |
| BETA-04 | Native implementation verified; delivery pending | Opt-in strict build/package gates require exact clean reviewed source, production target/features/version, tool hashes and staged asset bindings; final pairing helper checks archive bytes. Eleven focused tests and upstream verification passed. Production build and strict VSIX integration remain for the candidate gates. |
| BETA-05 | Planned | Actual shipped component and notice inventory. |
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
