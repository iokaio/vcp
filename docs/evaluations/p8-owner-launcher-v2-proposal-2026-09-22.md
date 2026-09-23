# P8-05 U03 launcher correction proposal

Status: offline correction verified; future qualification binding remains a draft. No provider calls or spending are authorized by this proposal. Existing cohort results remain failed, and its fixtures, thresholds, canonical roots and receipts are unchanged.

The frozen `p8-owner-v3/u03/workspace/AGENTS.md` and `package.json` instruct `node --test test/page.test.cjs`. Its original restricted launcher accepts only `--test --test-reporter=tap --test-concurrency=1 test/page.test.cjs`. The U03 attempts encountered this mismatch. Correcting it does not establish that the requests would have completed within the unchanged 16-request threshold.

The Apache-2.0 standalone [launcher](../../scripts/evals/p805-page-check-launcher-v2.rs) accepts exactly those two argument arrays. Both normalize to the existing canonical command with the pinned Node runtime, read permission for the canonical local workspace, no write/network/child/worker/addon authority, cleared environment except `SystemRoot`, closed stdin and a hidden native child process. Extra, reordered, alternate-path or unrelated commands remain rejected. No file in `src/evals/release/p8-owner-v3` was changed.

The [builder](../../scripts/evals/p805-page-launcher-v2-build.ps1) pins Node 26.9.0 and Rust 1.95.0 by SHA-256 and retains compiler arguments, embedded paths, source/builder/runtime/compiler/launcher hashes, parser results and log hashes. The [native controls](../../scripts/evals/p805-page-launcher-v2-controls.cjs) verify the build identities and frozen manifest, create separate temporary workspaces, and retain output hashes and a non-executable preparation proposal. They never open an owner campaign.

Verified on Windows on 2026-09-23 UTC (2026-09-22 local):

- Rust parser tests: 2 passed. Both exact forms normalize identically; other arguments fail closed.
- Native subprocess controls: 23 passed. Both forms pass the same 3 visible reference tests; the copied reference passes 37/37 existing oracle checks; 18 hostile variants exit 1 before Node execution; both forms pass permission and environment probes.
- Reference/probe workspaces and the complete original fixture inventory remain unchanged after execution. The pinned test runner creates its own `NODE_TEST_WORKER_ID`; the launcher does not inherit the supplied fake secret.

Retained local receipts (not portable distribution artifacts):

| Receipt | SHA-256 |
| --- | --- |
| `artifacts/p805-page-launcher-v2-0623c5c1-94d9-430a-9117-b7ad1e22c67f/build-receipt.json` | `4392449955dca38776378720d1b68b2e1c298824753fb94067cb605fae97dee0` |
| `%TEMP%/vcp-p805-page-launcher-v2-mDMivX/controls-receipt.json` | `9da8d31f70b98f6dd2d2147b1ff702decc320f179679571da57ab1626625f6c1` |
| `%TEMP%/vcp-p805-page-launcher-v2-mDMivX/preparation-proposal.json` | `172c634da705bddf1ac0642fb43c194facb2725e12cded11e901fc7f0e3510b8` |

The launcher executable hash is `3bd39f8abd453b5c2d933eeb75f0335f4780d58c11914709ad24834e39b405a5`. The preserved fixture manifest hash is `6a27284e55f440ffbc8580562b415f8cab1157e54b569dde88fbe07ec5c8969b`.

The first native control receipt at `%TEMP%/vcp-p805-page-launcher-v2-6JoGja` is retained as failed evidence: the environment assertion omitted Node's runtime-created `NODE_TEST_WORKER_ID`. The corrected assertion permits that exact additional key; all filesystem, process and network controls remain enforced. The later receipt above contains the complete passing run.

A future attempt requires a separately reviewed runner/preparer binding to this launcher, pinned runtime and receipts, preservation of the existing task material and limits, and explicit authorization for any further provider execution. This proposal creates no runnable paid plan, does not modify the existing bound preparer, and does not claim owner acceptance or clean-OS installation qualification.
