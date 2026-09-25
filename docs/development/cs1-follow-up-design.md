# Proposed CS-1 follow-up evaluation

Status: **unapproved and unexecuted**. This is prospective design, based only on
the CS-0/CS-1 scope in plan 24. No campaign outcomes, reviews or skill bodies were
consulted. It does not alter existing gates, authorize spending or reinterpret
frozen v1 results. Preserve every v1 input, result, failure and receipt unchanged.

## Smallest useful scope

Use two independently authored new normal tasks per candidate, with the existing
none / nearest / candidate arms: **four tasks and twelve initial task runs**.
Document-authoring's nearest baseline stays architecture; skill-authoring's stays
testing. This adds evidence about benefit, not a replacement for v1's boundary,
hostile, missing-input, near-miss or preservation requirements. Existing failing
or incomplete candidate gates cannot be erased by a favorable follow-up.

Proposed briefs, to be materialized and frozen by an author who has not seen
candidate prose or prior outputs:

| Candidate / task | Realistic task inputs and requested deliverable | Independent acceptance basis |
|---|---|---|
| Document / operator handoff | A short incident timeline, current recovery procedure and shift notes with a still-open diagnostic question. Draft a one-page handoff for the next operator; no operational action or sending. | Correct current state, completed versus pending steps, owner, stop condition, unresolved question and valid source links; no invented cause or completed recovery. Reader can identify the next permitted step without reconstructing the sources. |
| Document / migration notice | Accepted compatibility decision, concise implementation record and partial platform test results. Draft a local migration notice for maintainers. | Correct affected audience, required action, preserved behavior, supported versus untested claims and source links. Historical decisions stay unchanged. Reader can tell whether action is needed and which evidence is missing. |
| Skill / narrow package creation | A small project with a documented local review convention and examples of acceptable and unacceptable change notes. Produce an original VCP guidance package for that convention. | Native descriptor/content validation, exact hashes, bounded resources, clear requested scope and exclusions, unchanged authority and source files. Guidance supports the concrete local convention without inventing a new loader or tools. |
| Skill / scoped package maintenance | An original VCP package, its version record and an independently written change request updating one convention while retaining another. Update only the necessary package artifacts. | Requested version/resource/hash changes, preserved unaffected body/metadata, no stale references or authority expansion, and accurate qualification limitations. The resulting instructions remain coherent for a future reader. |

These outlines specify user problems, not desired phrases. Freeze realistic source
facts, allowed paths, independent oracles and task-specific reader anchors before
execution. Keep answer keys outside model context and authoring samples separate.
Review task feasibility using native tools and disposable deterministic examples,
not candidate-model trial answers. Do not tune briefs to make a skill win.

## Explicit proposed benefit rule

Use **quality improvement only** for acceptance in this follow-up. Operational
metrics are reported, but cannot break a quality tie or substitute for a failing
correctness/preservation gate. This choice resolves the ambiguity without adding
an efficiency success route after seeing outcomes.

1. All inherited hard gates apply. For each new candidate output, independently
   check actual artifacts, facts, hashes, source preservation, authority and
   unsupported-claim handling before assigning quality scores.
2. Two reviewers, blinded to arm identity, independently score completeness,
   clarity and usefulness on the existing 0–3 scale using prewritten brief-specific
   anchors. Reviewers see the same source brief and output representation; they
   do not see cost, latency, model requests or earlier campaign results.
3. Predeclare **usefulness** as the primary benefit dimension for all four tasks.
   A provisional benefit requires a candidate usefulness score at least one point
   higher than **each baseline on the same normal task**, with no lower score on
   completeness or clarity against either baseline. Each
   reviewer must independently support that comparison. No score sums, adjustable
   weights or improvements split across different tasks/baselines.
4. A baseline hard failure remains retained comparison evidence. It does not
   disqualify an otherwise correct candidate, but this proposed benefit rule still
   requires the independently anchored quality comparison rather than assuming
   that a failed run proves every quality dimension inferior.
5. Confirm a provisional advantage with one fresh matched three-arm replication
   of that task. Both the first and replication must satisfy the same rule and
   all candidate hard gates. This is observed repeatability in a small corpus,
   not a general statistical-effect claim.

For two reviewers who disagree, retain both score sets and seek one independently
blinded adjudication that explains the disagreement against the frozen anchors.
Adjudication may establish the final scores only with an explicit evidence-based
resolution; unresolved disagreement stays inconclusive. It is not an opportunity
to alter criteria or select the more favorable review.

## Repeats, uncertainty and stopping

Predeclare all twelve initial runs and a maximum of **one three-arm confirmation
per candidate**, selected by frozen task-ID order if both tasks show provisional
benefit. Maximum: **eighteen task runs**. Do not try the second task as another
confirmation if the first selected confirmation fails. No initial quality win
means a tie/inconclusive outcome, not permission to search for a favorable repeat.

An interrupted or accounting-uncertain run retains its failure and liability.
Reconciliation and any replacement execution require separate explicit scope and
authorization; the confirmation allowance is not an automatic retry pool. Stop
on changed identities, unsupported authority or unknown liability. Do not recycle
unused financial allocations.

Use identical authorized inputs, tool access, model/provider configuration and
output limits within each matched comparison. Freeze task/arm execution order
before running, counterbalancing arm order across tasks where practical. Record
every run, intervention, unnecessary tool call, request count, settled cost,
latency, disagreement and not-run check, including unsuccessful baselines.

## Calls, authorization and optional efficiency route

The initial cohort requires twelve task runs, so at least twelve dispatched model
requests if every run can finish in one request. That is a lower bound, not a
feasible request allowance: reading, editing and canonical verification can require
multiple requests. The exact per-run request ceiling and total dollar ceiling must
be proposed from the pinned tool/model configuration before authorization. Full
confirmation raises the task-run cap to eighteen. Any paid reviewer/adjudicator
requests must be separately included in the declared aggregate call and dollar
ceilings; none are assumed free or silently excluded.

Efficiency-only acceptance is **disabled in this proposal**. If the owner prefers
that route, create and approve a different prospective rubric before running its
cohort. It must identify the primary operational metric, meaningful improvement
threshold, quality noninferiority rule, latency/cost/request tradeoff limits,
replication count and missing-accounting policy in advance. Reporting a convenient
metric after a quality tie is insufficient. Such a change would not relabel v1
or this proposal's quality-only outcomes as accepted.

Before execution, freeze a new manifest/rubric revision, exact package and source
identities, native verification/toolchain receipts, model/provider configuration,
reviewer procedure, run order and explicit authorized budgets. Existing default
distribution gates remain in force; this document grants no promotion or spending
authority.
