# P6-04 matched evaluator comparison

The fixed workload at `src/evals/markov/decision-workload-v1.json` reuses the
original twelve M4 observable prefixes and tuning/calibration/held-out split.
Its generator is `scripts/evals/p6-decision-comparison.cjs freeze`. The source
manifest and its original sample floors remain unchanged. Model state contains
only training/window symbols, before/after fingerprints and failed-verification
counts. Opaque question IDs replace human case names, which could reveal labels.
Labels, outcomes and synthetic costs stay in the grader's original source.

After the actual native mixed-batch bootstrap, the qualification binary accepts
`--native-cohort` or `--comparator-cohort` followed by the same spec/private-output/
spec-hash arguments. Each mode sends one fixed twelve-question batch and admits
one canonical attempt; no arbitrary prompt, tools, fallback or retries are
available. Native mode requires output sentinel 1 and reserves the whole 64k
request. The conventional Chat comparator requires a 512-token output cap and
catalog support for strict structured output. Its actual body has a closed
Boolean/null schema, with the same state and question text as native mode.

The native answers retain actual Noul probabilities and use a frozen 0.5
threshold. Conventional answers remain Boolean predictions or null abstention;
they never acquire a probability or calibration score. Both paths validate the
whole answer batch, exact question IDs, answer types and served attribution.
Duplicate JSON keys, a missing question, refusal, truncation or a wrong identity
make the whole comparison abstain. Actual charges are settled before validating
answers; invalid output cannot erase spend.

`scripts/evals/p6-decision-comparison.cjs report <native-result>
<comparator-result> <new-report>` combines the remote results with the retained
M4 rules/statistics measurements, preserving the earlier source/run identity.
The output includes full and held-out validity/coverage, confusion counts, serious
misses including abstention, actual native Brier score, batch latency and canonical
known/unknown cost. A batch is one latency observation, not twelve independent
measurements. No task-level improvement, review outcome, fitted calibration or
forecast interval coverage is inferred from these classification results.
The remote batches share all twelve unlabeled observable prefixes. Each question
names only its own opaque case, but this is not twelve isolated remote sessions;
cross-case context effects are another reason these results cannot qualify an
independent project cohort or be treated as task-level policy comparisons.

The frozen source has only four held-out synthetic cases and correlated fixture
families. It cannot meet the declared twenty independent projects / forty cases
floor. Every remote purpose remains disabled regardless of average answer accuracy;
actual quality and service failures supply further explicit rejection reasons.
No negative result relaxes required tests, review, a quality floor, a strict pin,
or a reservation. Provider/model/schema/prompt drift, missing cost, stale sources
and serious misses require continued disablement or rollback.

The local tests cover both native and conventional request shapes, label exclusion,
distinct answer semantics, observed and unknown accounting, single-attempt claims,
duplicate keys and frozen rejection criteria.

## Measured disposition, September 21, 2026

The [source-bound result](p6-decision-comparison-result.json) records both actual
remote arms. Jev returned `typesafe/jev-1.13-20260917` through `TypeSafe`;
the conventional comparator returned `anthropic/claude-haiku-4.5` through
`Anthropic`. Both batches returned all twelve valid answers and settled known
charges, with no unresolved liability for these two attempts.

| Arm | Held-out coverage | Precision | Recall | Serious misses, including abstention |
| --- | ---: | ---: | ---: | ---: |
| Existing rules | 1.00 | 0.50 | 0.50 | 1 |
| Existing local statistics | 0.75 | 0.50 | 0.50 | 1 |
| Actual Jev | 1.00 | 0.50 | 0.50 | 1 |
| Conventional comparator | 1.00 | 0.50 | 1.00 | 0 |

Jev's four held-out native probabilities had Brier score 0.277625. The comparator
predicted true for every case; its higher recall came with two held-out false
positives and does not demonstrate a better task policy. No Brier score is
assigned to these Boolean predictions. These small descriptive results do not
establish population calibration or uncertainty bounds for a shipping policy.

Jev's batch latency was 653 ms and settled cost 80 microdollars. The conventional
batch took 1,775 ms and settled 2,101 microdollars. The total for the two batches
was USD 0.002181; the separate native protocol bootstrap adds 18 microdollars.
There were no helper retries or correction calls. A single batch does not supply
an empirical latency distribution, so p50/p95 comparisons remain unavailable.

Both remote purposes receive a recorded rejection and remain disabled. Both
miss the declared precision and independent-project/sample floors; Jev also
misses recall and the zero-serious-miss condition. No advisory policy trial or
task-benefit claim is admitted after these failures. Deterministic routing and
ordinary escalation remain the production behavior.

The retained local result used CRLF source bytes, while the current Git text is
LF. Reconstructing only CRLF produces the exact historical manifest SHA256
`51bedc59eae56aba4340f730c7164eedc7e3981b4c914783e1c4b2414d520dd1`.
The report retains both hashes and the original local run/source identity; it
does not pretend to rerun those measurements or accept a changed fixture.
