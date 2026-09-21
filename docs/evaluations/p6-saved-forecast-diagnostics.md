# P6-05 / M3 — Saved forecasts and diagnostics

`/optimize report` retains the forecast and compaction diagnostics it computed at
the report cutoff. An optional versioned pin keeps older reports readable.
Inspection reads those exact bytes under current source authorization and
retention; it never refits an old report from newer task state.

The aggregate artifact uses an explicit workspace source manifest to reference
all contributing tasks, records and events. Its task scope does not grant access
to the aggregate. Generic artifact reads are denied for this schema; the saved
forecast loader validates the complete source task set, current authority and
deletion epoch, retained references, artifact hashes and content identities.
Selective source purge includes the derived manifest and aggregate artifact.
This preserves the store's existing prohibition on direct cross-task artifact
references. Artifact, manifest and report publication is one transaction;
interruption before publication leaves no canonical report or artifact entry.

`/optimize compare` adds descriptive trace likelihood under the saved baseline's
unchanged transition probabilities. It reports episode and transition counts,
total and mean log likelihood, and the change in the mean. Comparable inputs
require disjoint, equally sized explicit windows, increasing cutoffs and exact
historical cohorts. Changed definitions, access, policies, missing support or
overlapping tasks abstain. A zero-probability transition produces an unavailable
result, not infinity or an invented smoothing probability. Context changes and
the baseline's optimistic in-sample score remain explicit caveats; there is no
calibrated drift threshold or causal regression claim.

Compaction diagnostics inspect actual retained projections and context manifests.
A summary is consumed only when a later submitted attempt has the exact manifest
request digest and included summary identity. Creating a projection or manifest
alone does not establish consumption. Matched adjacent submitted contexts expose
before/after connected task-transition counts and interval durations; missing or
changed context, model, source or revisions produces an explicit abstention.
The diagnostic returns source identities and counts, never summary prose.

The projections remain read-only and unqualified. They cannot change routing,
protected verification funds, reservation bounds or provider permissions.
Expected costs remain separate from accounting totals; neither a mean cost nor
an absorption probability is presented as completion within a spending limit.

Verification includes two drift arithmetic tests, two compaction interval/sample
tests and two saved-report integration tests on Files and SQLite. The latter
cover immutable replay, legacy reports, source access, narrowed generic reads,
selective cross-task purge, physical cleanup, non-resurrection and injected
interruption after spool finalization but before publication. The existing
compaction host fixture passed for both stores with normal and oversized inputs,
and detected actual submitted summary consumption. This is contract evidence,
not predictive or causal qualification.

Eleven optimizer CLI tests, four existing routing workflow tests and two shared
advisory-redaction regressions passed. Compilation, formatting and diff checks
passed. The fast suite passed all nine cases
(`1ff8ad5c-ad48-4851-92eb-0c226e788cd1`); its ADR inventory was updated to include
the new fortieth decision record.
