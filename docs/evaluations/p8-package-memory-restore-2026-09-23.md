# P8 packaged memory, optimizer state and restore — 2026-09-23

Status: both package restore directions passed the hash-bound qualification.
Owning work items are P8-01/02/03, supplying bounded evidence
for P8-05. P8-05 and its P9 dependency remain open.

## Declared scenario

Run the extracted production executable with SHA-256
`28c32ddd52c89c4c02b994911162c2b1db229b96f7cf365b20aa6ce1748bf3e3`
in two disposable fixtures: Files to SQLite and SQLite to Files. Reuse the
qualified local MiniLM assets. Fixture setup creates two governed memory sources
through library APIs and an explicit Plan policy; exercised operations use the
production CLI. Native Git initialization has an isolated environment, empty
hooks, no inherited configuration or network protocols.

Build and query real vectors, retain an optimizer interview answer, exclude one
task's memory, and leave the sibling retrievable. Create an encrypted snapshot,
enroll the recovery identity and checkpoint in a fresh registry, authenticate
preview/apply and restore into a new workspace on the opposite backend. Check
canonical history, memory evidence, exclusion decisions and optimizer state.
Inspect the retained sibling claim as history, then query retained and excluded
sources before and after an explicit real-model build and process reopen.
The restored repository has a new physical binding: old bound claims must be
excluded from current retrieval, with the independent oracle checking the
generation inventory's explicit stale-binding diagnostic. This does not claim
that the CLI query response itself explains every exclusion.

The independent checks require nonempty retained exclusion decisions, exact
memory records and prior event/transaction prefixes, unchanged source state and
source bytes, increased destination authority, untrusted/paused restoration,
the documented Plan-policy reset, stale authority/preview refusal, and no
provider attempt, reservation, ledger or external-effect records. Exclusion is
not purge; this scenario makes no tombstone or physical-erasure claim.

## Restore search contract

Staged import clears search readiness, but the production CLI immediately calls
its authorized search rebuild. Therefore completed CLI restore must provide
lexical readiness. Compatible retained vectors may be reused only after captured
component checksums, embedding metadata, deletion epoch and newly authorized
chunk identities are checked. Record the observed semantic-pending status; do
not infer wholesale vector-index portability or require an empty result from
the intermediate staging contract. This scenario's old claims are intentionally
inapplicable under the new repository/worktree identity, so a ready empty index
does not establish positive restored-vector recall. The source fixture must prove
real vector use; restored build/query must preserve stale/excluded-source absence
and report its actual readiness and diagnostic state.

## Corrected fixture assumptions

The first exploratory run created Git after seeding memory. Explicit rebind
correctly made those synthetic claims stale before backup. Setup now establishes
the physical Git binding before materializing governed memory. Log:
`artifacts/p8-memory-restore-exploratory-1.log`.

The second run reached actual encrypted restore and verified retained memory,
exclusion/interview/policy records, receipts, paused tasks and authority refusals,
then failed the proposed positive current-recall assertion. That assertion was
an incorrect portability assumption: the query authorization contract in
`memory-retrieval-design.md` requires suppressing inapplicable records, and
production restore/rebind deliberately changes repository/worktree identity.
Before the next run, the oracle was revised to require positive historical
inspection, exact retained evidence and explicit stale current-recall refusal.
The original failure is preserved in
`artifacts/p8-memory-restore-exploratory-2.log`. No production guard is changed.
The CLI currently has no action that revalidates a retained claim for the new
binding; this campaign does not claim that behavior or automatic applicability
transfer.

## Evidence requirements

The ignored test is
`memory_restore::production_memory_optimizer_exclusion_survive_encrypted_cross_backend_restore`.
The runner `scripts/evals/p8-memory-restore.ps1` binds the compiled test, extracted
production executable, native Git, source manifest and model asset manifest by
independently supplied hashes, checks inputs again after execution and retains
stdout/stderr digests. Exit zero alone is insufficient: exactly one passing test
and both direction witnesses are required.

Each child command has a 180-second limit and a four-MiB output limit per stream.
The parent limit is 30 minutes. Parent timeout is not a Windows descendant-job
containment claim; any timeout requires checking owned descendants before retry.
The scenario uses no provider configuration or paid model calls. Network denial
is not enforced by this runner and no OS-offline evidence is claimed.

## Execution

The exact ignored test passed both backend directions in 12.11 seconds:
`artifacts/p8-package-memory-restore/420a974a-0cff-4e63-9c88-f22d306c0499/result.json`,
SHA-256 `7ae369ad35d9ede410a81921214ae9874accfadf4a0e0e7ab300415795963765`.
All pinned input checks passed before and after execution. Source content digest:
`71d4c7ad6a240bbc1a8268c1ce4735451466cc969d0209f664dd713d88304238`.
The source identity manifest and compiled test digests are retained in the receipt.

Both restores reported lexical readiness, `semantic_pending=false`, zero indexed
records and two excluded records. An explicit fresh build published successfully.
Repeated queries retained local embedding observations (four resource samples)
and returned zero passages. Independent inventory checks attributed both old
claim versions to `stale_binding`; production historical inspection preserved
the sibling's exact governed version and evidence. These are successful exclusion
and history-preservation observations, not positive restored semantic recall.

The existing production memory controls passed 2/2 after the fixture helper
change (`artifacts/p8-memory-restore-existing-controls.log`). Six runner input
controls passed, including wrong digest, missing/directory input, UNC and reparse
ancestor refusal. Control receipt:
`artifacts/p8-memory-restore-controls/0aceac21-beaa-4a37-8513-3b0b261cc89e/result.json`.
All 17 fast delivery groups passed:
`artifacts/p8-package-memory-fast/e216149b-ae11-4dce-b9f1-84a4896786d5/manifest.json`.
Formatting and diff checks passed. Independent review corrections are included;
no production code or trust boundary changed.

## Remaining acceptance

This combines selected U04/U05/U07/U09 boundaries on the current package. It
does not complete any whole U-case: routing policy comparison, actual child or
MCP/skill execution, independent-machine handoff, owner-task completion, clean
Windows, minimum hardware, physical full-volume exhaustion and human acceptance
retain their separate evidence requirements. Existing successful package,
crypto and recovery campaigns are reused without relabeling their scope.
