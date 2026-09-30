# Project routing and context optimization

Original VCP guidance.

## Inspect first

- VCP's retained evidence for the project: `/optimize report` and `/optimize status` in the terminal (offline: `vcp optimize report --from <ms> --until <ms>` and `vcp optimize status`), task cost records, latency observations and routing outcomes for the window under review.
- Current routing configuration and policy: selected model/endpoint/group restrictions, project pins, output and input limits, escalation settings, and the developer-selected quality floor.
- Trusted limits that cap any request: budgets, ceilings and allowlists outside project control.
- Which report fields are known spend, reserved or unresolved liability, estimates, and independently graded quality versus verification observations.
- The sample size and age of the evidence, and whether cohorts are comparable.

## Inspect the evidence

Use the project's existing /optimize report/status workflow and current routing configuration. Compare like task cohorts and report windows. Distinguish known spend from unresolved liability, estimated costs from actual charges, selector timing from model latency, and verification observations from independently graded quality.

Ask only for preferences the report cannot establish, recording answers with `/optimize answer priority|size|review|restrictions <value>`. Priority, expected size, review needs, and model restrictions are input to an explicit proposal, not permission to widen trusted allowlists or spend evaluation money. Do not infer a numeric quality floor from prose or treat a synthetic benchmark as live qualification.

## Proceed and verify

1. Summarize the retained evidence by cohort and window, and name the gaps: too few samples, stale data, mixed task types, or unresolved costs.
2. Prepare a concrete preview with `/optimize preview low|med|high --quality-floor <bps>`, using the developer-selected quality floor in basis points. Explain requested and effective policy under trusted limits.
3. Propose, do not silently change. Budgets, quality floors, allowlists and routing policy change only when the developer selects a specific current preview.
4. Apply only an explicitly selected current preview with `/optimize apply`; stale revisions require refresh. Rollback must identify target and expected current revisions: `/optimize rollback <target> --expected <current>`.
5. After an applied change, compare later retained evidence against the same cohort definition before claiming an improvement, for example with `/optimize compare <baseline-report> <current-report>`.

Evidence is the report window, cohort definition, the preview identity that was applied, and subsequent observations. A preview is a proposal; an applied change without later observations is not a demonstrated saving.

## Pitfalls

- Comparing cohorts with different task mixes, sizes or review requirements.
- Counting only successful attempts and hiding retries, escalations, review or support costs.
- Treating an estimate or reservation as an actual charge, or ignoring uncertain cost.
- Lowering a quality floor or skipping required review to make cost look better.
- Starting paid model trials or remote advice to fill evidence gaps without explicit budget authority.
- Reducing context in ways that drop required capabilities, instructions or source attribution.

## Results

Report evidence gaps, proposed changes, expected trade-offs and subsequent observed results. Context reductions must preserve required capabilities and source attribution. Never hide retries/support costs or start paid model trials to make a comparison look complete.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
