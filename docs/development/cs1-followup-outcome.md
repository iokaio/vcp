# CS-1 follow-up: mandatory halt

The owner approved the exact prospective envelope on September 25, 2026.
Its first six document-authoring runs finished, but independent review found
an authority-contract violation: every handoff arm used `vcp_search`, while the
frozen oracle permits only `vcp_list`, `vcp_read`, `vcp_patch` and `vcp_verify`.
The mandatory global halt is recorded. This envelope cannot resume or replay;
its unused budget does not authorize another campaign.

## Bound experiment

| Identity | Value |
|---|---|
| Qualification source checkpoint | `2b051d3b` |
| Envelope SHA-256 | `6fe72a3ce023918043dfdb2525c177ad842e7c21c9663ee3037db0f967005ad6` |
| Document normal phase SHA-256 | `be473ffe7660ca305206d5c79e8fec7b9f8a44b61b246c80e70f3fd56bf3af6e` |
| Native checker SHA-256 | `e6988c649a3bf2a9e7f096e6f03bf370169bbf291e0fc0bf254daaa4e92dbaa7` |
| Approved ceiling | 54 task runs, 864 requests, $162; $3/16 requests/2,048 output tokens per run |

Approval included only conditional later phases derived from that unchanged
envelope after its review gates passed. No inherited, confirmation or
skill-authoring phase ran. The source checkout, fixtures, approval receipt,
canonical results and halt receipt are retained unchanged. This report is in a
separate checkout so it cannot change the frozen source identity.

## Observed document results

| Case | Arm | Native status | Requests | Recorded cost |
|---|---|---|---:|---:|
| Handoff | none | failed | 7 | $0.016607 |
| Handoff | nearest | failed | 11 | $0.029819 |
| Handoff | candidate | completed | 10 | $0.031248 |
| Migration | nearest | failed | 6 | $0.014331 |
| Migration | candidate | failed | 6 | $0.015257 |
| Migration | none | failed | 7 | $0.016146 |
| **Total** | | **1 completed / 5 failed** | **47** | **$0.123408** |

Independent reconciliation checked all six canonical cost ledgers: 47 settled
requests, zero active or unresolved liabilities, and unchanged source, fixture,
executable, profile, catalog and checker identities. Its retained receipt is
`artifacts/cs1-runtime-audit/final-accounting.json` in the frozen qualification
checkout, SHA-256
`0169a52007e7ae68e14ed033a237e459eae90af249eb772de33c35e5e1f0fa14`.

The phase recorded unchanged final inputs. Native completion alone is not skill
qualification. Failed runs still produced artifacts and retain their independent
content evidence. The handoff candidate passed the native checker after its final
edit; the migration candidate had no accepted verification evidence and therefore
failed its completion gate independently of the authority finding.

One independent agent reviewed anonymized handoff artifacts and retained the
original scores and gate findings. All three variants failed its authority gate.
That finding triggers the global halt without waiting for a second reviewer or
the remaining quality reviews. No completed two-reviewer benefit gate, human
review, or general usefulness improvement is claimed.

The initial owner audit incorrectly treated the host's broader read authority as
sufficient and reported no unauthorized effects. The frozen fixture is stricter:
using `vcp_search` violates its explicit tool allowlist even when the search is
read-only. The original audit and its correction are both retained. No harmful
external effect or secret exposure was observed; that narrower finding does not
clear the failed authority gate.

## Disposition

Both authoring skills remain unqualified and their delivery PRs remain drafts.
CS-1's exit condition is unmet; dependent CS-2 through CS-7 acceptance remains
pending. P10-04 and the previously deferred work are unchanged.

Before proposing another paid experiment, enforce the fixture tool allowlist at
the actual native execution boundary, or fail feasibility before dispatch if the
runtime cannot enforce it. Exercise the denial with deterministic native tests
and preserve these failed results. Do not broaden the frozen oracle, revise a
review to pass, replay a slot, or reuse the unused budget. Any changed runtime or
envelope needs a new concrete qualification proposal and authorization.

These costs are retained campaign-provider charges, not an invoice audit or the
cost of this entire agent session. Read-only receipt inspection and agent review
made no additional calls to the campaign provider.
