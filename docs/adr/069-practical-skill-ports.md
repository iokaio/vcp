# ADR-069: Practical upstream skill ports

Status: accepted by owner direction, September 29, 2026.

## Context

The original CS follow-on required original implementations and comparative
model qualification before default distribution. The owner has directed VCP to
reuse eligible upstream skills and remove disproportionate Munarium-derived
evidence obligations from skill development while refreshing useful Munarium
components. The eight workflow goals remain applicable.

## Decision

Use [the replacement plan](../research/skillsplan-new/.md) for current skills
delivery. It supersedes ADR-068 and CS-0 through CS-7 only where they prescribe
original-only development, blanket comparative qualification, or sequential
all-skill promotion. Preserve historical records and outcomes.

Add Anthropic skills as a pinned upstream. Inspect each selected skill and its
included resources/dependencies individually. Permit compatible open-source
licenses except GPL/LGPL; exclude missing or restrictive copying permission.
Retain licenses, attribution, source identity and modification notes.

Ship each skill through a separate PR after meaningful scoped functional and
package checks. Paid comparisons and research campaigns are optional targeted
experiments, not universal release gates. Keep canonical runtime authorization,
integrity, path/process limits, secret handling, preservation and truthful claims.
Munarium memory governance remains a runtime contract rather than a skill
authoring checklist. Refresh only useful selected Munarium libraries and verify
the affected adapters in a separate PR.

## Consequences

Skills can improve independently using tested upstream work. Packages expose
their actual supported tools and limitations; default distribution is not a
claim of general comparative benefit or universal host support. Historical CS
campaigns remain available for investigation, but cannot block the new plan
merely because their superseded research gates have not been completed.
