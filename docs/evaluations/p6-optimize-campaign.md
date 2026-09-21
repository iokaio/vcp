# P6 / U07 canonical campaign inspection

`scripts/evals/p6-optimize-evidence.cjs` connects a finished live trial to the
actual local optimizer. It binds the frozen executable, plan and trial result,
runs `optimize status`, saves two canonical reports, and compares those report
identities. Paged canonical cost records must be unchanged before and after.
The collector has no provider configuration, run or resume command. Reports are
ordinary retained optimizer artifacts; it does not fabricate a pooled store.

```powershell
node scripts/evals/p6-optimize-evidence.cjs <plan.json> <finished-row-id> <new-output-directory>
```

Each trial deliberately has an isolated workspace and one main task. Its report
therefore establishes actual costs, outcomes and sparse-history inspection, not
a large local project sample. The two reports observe the same trial; their
comparison is not a policy treatment or evidence of causal improvement. Matched
campaign strategy comparisons and held-out qualification remain separate.

The complementary native evidence uses real canonical services and scripted
provider/accounting fixtures:

- `routing_accounting::mixed_failed_child_and_unresolved_support_costs_stay_in_denominator`
  compares retained baseline/current reports, counts failed and unfinished work,
  child/support costs and unresolved liabilities, and advances adaptive questions
  from priority to model restrictions to review preference. It cannot trigger
  automatic policy changes or label unknown charges as savings.
- `report_keeps_failed_cancelled_and_unfinished_denominators_and_scopes_saved_evidence`
  checks three-task and twelve-task reports, the small-sample warning, complete
  failed/cancelled/unfinished denominators, empty windows and current access.
  Removing the small-sample warning does not remove non-causal and missing-metric
  limitations or qualify a policy.
- `selective_apply_is_atomic_idempotent_reopenable_and_rollback_rechecks_ceilings`
  establishes explicitly selected changes, exact effective revisions, idempotent
  publication, reopen and rollback under current trusted ceilings. Other policy
  fixtures cover interrupted publication, stale edits and narrowed limits.
- `local_optimizer_persists_reports_and_answers_without_a_budget_or_provider`
  establishes report/answer/compare durability with no ledger, attempt,
  reservation or model request.

These sources distinguish actual live task observations from deterministic
workflow evidence. They do not claim an unperformed before/after live policy
treatment, live delegated-child results or release qualification.

## Observed campaign inspection

The collector ran successfully on September 21, 2026 against the finished v3
economical tuning-generation task. Both canonical reports contain one completed
task, two accounted main attempts, **USD 0.001526** known spend, no retries or
supporting attempts, and zero uncertain attempts or reserved liability. The
inspection made **zero provider calls** and changed no canonical cost records.

The reports are `routing-report-57ae8de1-64cb-43c0-9a33-fcfb6e373dd2` and
`routing-report-9933096e-5433-404e-b077-5bf5fa516c77`. Their canonical comparison
correctly reports `comparable: false`, empty improvement metrics and
`automatic_action: false`: these are correlated inspections of the same sparse
history with unspecified comparison windows. Forecast comparison also abstains
without a complete eligible cohort. Policy apply/rollback remains the separate
native workflow evidence described above.

| Evidence | SHA-256 |
|---|---|
| Collector result | `a4e081bc7484ae1e1d9656a3c0b5c5d1a1532c5eb05b70a946108297eff8c85f` |
| Frozen trial plan | `86156073ec023b153cc6e0040565fbfe6ef35f06b3cd755ca2edbbe94b187840` |
| Finished task result | `58c8c924cfe8e47710a775ae4a0796d7e924839c62715f4627e69f5d33386352` |
| Executable | `d6a727000f69c9ae4f45ce867dd60c89e72c2bfaf28dbbacb64229e7b2ca0c9b` |
| Collector source | `216dcb2b42a8e745b74d383c9d46154b29feb57368c992cd90f15c5ffb7f73ad` |

Private paths and raw store captures remain local. The three/twelve-task native
report test passed in `artifacts/p6-optimize-history-threshold.log`; it validates
sample-warning behavior and access checks without asserting model quality.
