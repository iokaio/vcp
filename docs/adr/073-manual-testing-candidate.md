# ADR-073 — Owner manual-testing candidate before full qualification

Date: October 1, 2026. Status: accepted by owner direction; execution pending.
Owning items: BETA-08/BETA-09 in the [release plan](../release-planning/00-release-plan.md).

## Context and authority

The owner requested an effort review, then approved the recommendation to freeze
broader harness work, prepare one identifiable installer/VSIX pair, verify focused
installed behavior, and begin manual testing with explicit qualification gaps.
This decision supersedes ADR-072 only where it made completion of the full matrix
a prerequisite for that owner testing milestone. ADR-072's channel, versions,
unsigned artifacts, platform exclusions and runtime safeguards remain unchanged.

Recent work expanded into general qualification process supervision, file-release
observers and extensive local runner binding. The latest failure was an immediate
file-availability assertion in a newly added harness test after its expected
descendant refusal and empty-Job checks passed. It has not established a product
defect. A separate Cargo target-selection experiment exposed avoidable dependency
recompilation. Neither requires completing the entire unfinished harness patch
before using the existing artifact-pair checkpoint.

## Decision

Use reviewed clean main and successful ordinary Delivery checks, then one existing
`beta-candidate.yml` dispatch with `stop_after=pair` on the owner-provisioned
`vcpwin` runner. Preserve strict production build and artifact provenance. A
successful pair selection with full pipeline `incomplete` and later stages
`not run` is admissible for the
focused checks; do not rewrite it as full pipeline success or weaken the existing
full-qualification runner's admission checks.

Check exact-byte installation, launch, onboarding guidance, VSIX connection and
uninstall/data preservation in private fixtures outside the checkout. Cover both
stores where state is created and record the actual environment. Developer-host
observations can support this limited handoff; clean-host support stays unproven.
Provide the [manual checklist](../release-planning/04-manual-testing-checklist.md),
exact hashes, actual results and limitations. Paid tasks still require credentials
and explicit bounded admission; no paid execution is authorized by this decision.

Freeze additional generic harness work. Retain its source and failed observations
for follow-up. Fix demonstrated blockers with focused verification. Stop when the
manual handoff is reviewable; resume broader qualification when owner testing or
an explicit release decision makes it useful.

## Remaining gates

The full matrix, clean Windows, distinct-version upgrade, independent-machine
recovery, physical full-volume/network denial, minimum-hardware evidence and owner
quality judgments remain incomplete wherever not actually observed. This decision
authorizes preparing and handing the candidate to the owner for manual testing;
it does not authorize public publication, Marketplace upload or wider distribution.

Permissions, accounting, credential handling, process restrictions, user-data
preservation and truthful completion do not change. Known security or data-loss
failures stop the handoff. BETA-09 completion and BETA-11 release acceptance remain
separate from this narrower milestone.
