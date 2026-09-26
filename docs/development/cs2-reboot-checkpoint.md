# CS work reboot checkpoint — September 26, 2026

Status: stopped at the owner's request at 05:10 UTC. The runner exited after
retaining the current run's evidence. A permanent campaign halt prevents further
dispatches. The workspace is ready for reboot; no campaign command should restart.

## Owner scope and stop

The owner expanded the requested delivery to CS-1 through CS-10, including tests,
reviews and merged PRs, then requested a stopping place and checkpoint for reboot.
The reboot request takes precedence: do not start more work until resumed.

The repository defines CS-0 through CS-7 in [plan 24](../plan/24-skills-follow-on.md).
An asynchronous clarification about undefined CS-8, CS-9 and CS-10 is pending;
do not invent those work items. CS-1 is closed by explicit owner acceptance as
non-default candidates, but its default-promotion gaps remain necessary for
CS-3's six-skill and CS-6's eight-skill acceptance.

## Code and qualification state

- Branch: `feat/cs-2-qualification`, based on main
  `8d6149b6b35c5853535dd6a8f8d8d5c6e1722232`.
- All ten audited CS-2 defects were fixed and merged in
  [PR #184](https://github.com/iokaio/vcp/pull/184). The implementation record
  retains its passing contract, native, containment and CI evidence.
- No implementation or candidate bytes changed during this qualification attempt.
  No specialist has been promoted. CS-2 remains incomplete.
- Exact campaign plan SHA-256:
  `99ff620b9a3597fe6fe817c0b9bb1e60897e751acc3547561574da08c40dfc8a`.
- The owner explicitly approved two refresh probes plus the 54-run campaign,
  capped at USD 163.50 total, using `openai/gpt-5.6-luna` through OpenRouter's
  `amazon-bedrock/us-east-1` endpoint. No retries, replays or budget transfers.
- The two refresh probes completed and settled USD 0.000101. They cannot repeat.
- The live `llm-integration` block was stopped for reboot before completion.
  `mcp-development` and `frontend-design` have not started. Functional grading,
  blind reviews and qualification decisions have not run.

Eight runs dispatched, with 64 settled requests costing USD 0.137568. Including
refresh probes, 66 requests cost USD 0.137669. Canonical accounting reports zero
active or unresolved liability. Seven runs completed. The eighth,
`LLM-boundary-partial-v3--none`, reported incomplete and did not run its required
native checker; its structural check passed and its workspace was preserved.
That case failure predates the stop and must remain recorded. The next baseline
was rejected before dispatch by the halt; its `failed` result is not a paid run.
All later cases are unrun. Final frozen inputs were unchanged.

The frozen one-shot runner does not support a reboot pause inside a block. Its
`halt.json` is permanent. Do not delete claims or the halt, rerun this block,
resume later blocks, or reinterpret unused allocation as new authorization.
The stop is owner-requested, not an observed integrity or secret-handling defect.

## Local evidence and continuation

Local evidence is ignored by Git and must remain private. The original private
directory is named in `artifacts/cs2-private-directory.txt`. The exact approval
proposal is `artifacts/cs2-qualification-authorization.json`; run accounting and
evidence are in its campaign directory. Durable, access-restricted backup:
`artifacts/cs2-reboot-20260926/private-evidence.tar` (96,322,560 bytes; 19,812
archive entries), SHA-256
`48061e6201cc0c4c516185247d858574a893396d07180f4a11a3d14c034af4b6`.
The adjacent receipt records the original path and archive verification. A copy
of the repository campaign claim is preserved alongside it. Keep the original
`.git/vcp-cs2-developer-campaign.json` claim in place.

`artifacts/cs2-reboot-reconciliation.json` records the final accounting and
read-only verification of all eight run evidence hashes and workspace hashes.
Do not dispatch from a restored archive: its embedded paths are historical.

Read-only preparation is preserved in:

- `artifacts/cs1-promotion-prerequisite-notes.md`: concrete DOC defects, SKL
  evidence gaps, future harness/package checks and historical budget boundaries.
- `artifacts/cs3-preparation-notes.md`: browser identities, primary-source links,
  proposed isolated-browser compatibility experiment and acceptance matrix.

No browser experiment, dependency installation or CS-3 implementation ran.

On resumption, read this checkpoint and the CS-2 implementation record first.
Reconcile retained results without running the halted campaign. Prepare a fresh,
reviewable continuation proposal that preserves these outcomes and explicitly
binds its source, fixtures, runtime, provider, calls and dollar ceilings; obtain
new authorization before any new paid dispatch. Ordinary offline implementation
and tests can proceed within the owner's broader scope. Complete CS-2 and the
necessary CS-1 promotion prerequisites before downstream acceptance claims.

This checkpoint is intentionally left uncommitted for the reboot stop. No new PR
was created or merged during this qualification attempt. Resume the authorized
delivery workflow only after the owner asks to continue.
