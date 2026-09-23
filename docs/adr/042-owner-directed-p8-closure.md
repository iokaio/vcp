# ADR-042 — Owner-directed P8 closure and P9 continuation

Date: September 23, 2026.
Status: accepted by explicit owner direction.
Owning items: P8-01 through P8-05; P9-01 through P9-03 next.

## Decision and authority

The owner instructed: “Close P8 and move on to P9. Document that in a PR with
these uncommitted changes.” This closes the P8 milestone with its recorded
qualification gaps accepted for development progression. P8-06 was already
complete. P9 now proceeds in order: public protocol, authenticated local server
and attachment, then the TypeScript SDK tested against the compiled server.
P4-01 extension integration follows P9-02/03.

This decision supersedes the P8-before-P9 sequencing gate in ADR-002 and ADR-012
for this continuation. The dependency IDs remain intact: P8-05 is closed by this
owner decision, not by new passing qualification evidence. It does not change
ADR-018's truthful release-evidence requirements or authorize publishing a release.
P10 work is not included in this continuation.

## Evidence and accepted gaps

The [corrected production package](../evaluations/p8-allowance-package-2026-09-23.md)
passed its recorded distribution checks. The following remain failed, unknown,
not run or unreviewed as recorded; closure does not relabel them:

- The renewal attempt failed, has no provider request ID, and remains consumed.
  Its canonical unresolved liability is 6,397,576 micro-USD; its full 12,000,000
  micro-USD campaign reservation remains held. Actual charge is unknown.
- Successful renewed provider qualification and the six owner tasks on the
  corrected package have not been completed. Prior package task results retain
  their original artifact identities and failures.
- Remaining integrated package evidence, clean Windows, production network
  isolation, minimum-hardware and physical full-volume recovery checks remain
  unexecuted. Independent-machine handoff remains skipped as previously recorded.
- Usefulness, correctness and architecture-fit scoring has not been completed.
  The owner's milestone closure is not a claim that those reviews passed.

The last inspected shared campaign remains 3,637,947 settled and 78,127,269
reserved micro-USD under its 100,000,000 ceiling. This decision neither clears
liabilities nor authorizes a new paid retry or a higher budget. P9 work does not
require resolving that campaign or running paid provider tests.

## Retained work and validation

The [contained qualification design](../development/p8-contained-qualification.md)
is retained as a concrete route for later evidence collection, not an active
prerequisite for P9 or a passing qualification report. It describes disposable
Windows CI, bounded virtual-disk exhaustion, independently verified network
denial and explicit resource-envelope limits.

The accompanying regression uses synthetic receipts with the recorded campaign
totals. It preserves old liabilities, exercises settlement of only a new probe,
rejects concurrent owner admission and tests exact-budget admission versus a
one-micro-USD overrun with unchanged ledger bytes on refusal. Both focused
coordinator suites passed: 21 tests, zero failures. No provider call, real ledger
mutation or infrastructure qualification occurred.

## Consequences

The task ledger marks P8-01 through P8-05 complete by owner-directed closure,
with this exception linked in every affected row. Historical reports preserve
their evidence and dates. Any future release/support claim must still disclose
the unresolved gaps and satisfy the applicable evidence requirements.

P9 retains its full protocol, authentication, ownership, idempotency, bounded
streaming, compatibility and compiled-server SDK acceptance conditions. No
production security, accounting or authorization check is weakened by P8 closure.
