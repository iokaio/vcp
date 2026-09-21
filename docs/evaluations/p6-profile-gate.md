# P6-04 profile activation gate

The [frozen candidate gate](../../src/evals/tasks/p6-profile-gate-v1.json) is
declared before task trials. Each individual task still requires every rubric
check to pass. A candidate automatic default additionally needs at least 30
independent held-out tasks in each analysis/review/generation class, a one-sided
95% exact binomial success lower bound of at least 0.80 in each class, no serious
failures, no intervention and complete cost/latency evidence. These are activation
requirements, not newly enabled application settings. Synthetic template repeats
do not establish independent samples or representative population coverage.

The initial six-task corpus cannot meet this gate, including if every task passes.
The expanded v2 corpus adds twelve structurally varied tasks, yielding three
held-out cases per class and 54 matched executions. It also cannot meet activation
requirements; its added coverage tests distinct import/review/schema behaviors
without pretending synthetic variants establish population representativeness.
Its live comparison can measure the cost, latency and observed quality of the
three matched strategies and support a recorded rejection of automatic defaults.
It cannot qualify a shipping model ranking or fitted quality score. No rejected
candidate changes existing explicitly configured values.

`node scripts/evals/p6-profile-qualification.cjs <private-trial-directory>` joins
the immutable execution plan and live result, regrades the exact frozen answers,
and emits hashes of the result, plan, rubric, gate and reporting source. Planned
failures and missing runs stay in denominators. Unknown charges stay null. It
reports nearest-rank p50/p95 wall times with sample counts; these tiny distributions
have no tail-latency generalization. Source binding detects accidental drift but
does not authenticate arbitrarily supplied receipts: reviewed canonical capture,
reservation and settlement evidence remains required.

P5-08's [integrated memory evidence](p5-08-integrated-memory.md) establishes the
existing retrieval/retention boundaries. This campaign uses fresh synthetic
workspaces and does not propose changed context/memory defaults. Any later change
to those defaults requires refreshed affected memory evidence. P8 retains actual
delegation, packaged CLI and final integration qualification.
