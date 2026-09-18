# Bounded semantic decision implementation design

Status: proposed design; no evaluator, model, thresholds or savings qualified.
This incorporates selected ideas from [the JEV exploration](exploring-jev.md)
under [ADR-020](../adr/020-bounded-semantic-decisions.md). It refines existing
[P6 work](../plan/12-routing-and-optimization.md), preserving deterministic
selection, OpenRouter, local retrieval, canonical accounting and controller
authority. The exploration is research, not a replacement product specification.

Planning direction updated September 18, 2026: qualify actual Jev through
OpenRouter as the leading specialized candidate, using a thin Rust adapter.
Compare it with deterministic rules and a conventional OpenRouter LLM. The common
interface remains useful for all three; reproducing Jev's answer shapes with an
LLM does not reproduce Jev's trained behavior. Remote advice stays disabled until
the applicable transport and quality gates pass.

## Adoption map

| Exploration idea | Disposition and existing owner |
|---|---|
| Vendor-neutral Boolean, Choice and Score evaluations | Plan a bounded advisory contract and deterministic implementation in P6-02; optional OpenRouter adapter uses P2-02's existing gateway |
| Actual Jev model through OpenRouter | Preferred specialized candidate for P6-02 transport/conformance work and P6-04 comparison with rules and a conventional LLM; qualification is not assumed from listing availability |
| Complexity, eligible-model suitability and strategy judgments | P6-02 compares advisory signals with deterministic routing; P6-04 qualifies any enabled policy |
| Stalled-attempt and retry/replan/escalate suggestions | P6-03 consumes signals under existing evidence, attempt and budget limits |
| Selective review and delegation triage | P6-03 can propose a review reason; P7-04/05 owns actual children, review and integration; P8 rechecks real end-to-end behavior |
| Waste classification for `/optimize` | P6-05 optional, admitted assistance; local reports/interview and explicit selective apply remain usable without it |
| Memory proposal or contradiction suspicion | Later reuse within P5-02's existing optional model extraction path may propose evidence-linked claims; no P6 dependency is added to P5 and no confidence score accepts truth |
| Context selection, retrieval filtering and reranking | Preserve local P2-08/P5-06 selection and eligibility. No new remote ranking or embedding path is introduced |
| Test selection and completion readiness | At most advisory explanations or additional checks; P2-06's required verification and completion criteria cannot be suppressed or satisfied by a score |
| Decision ladder from cheap judgments to stronger reasoning | Bounded explicit policy transitions, not an automatic multi-call ladder; ordinary tasks can use zero helper calls |
| Always-on observers | Remain P10-03, after release; task-triggered P6 decisions do not create a background observer service |
| Direct TypeSafe endpoint outside OpenRouter | Unselected future option requiring an owner-approved change to the OpenRouter/data-egress contract; neither normal dispatch nor failure fallback may bypass the gateway |
| LangChain Jev connector | Reference for a separately scoped experiment; not a VCP runtime dependency. Production calls use Rust and VCP's gateway, lifecycle and accounting services |
| Local semantic decision model | Unselected future option requiring pinned assets, licensing, resource and native Windows evidence; existing local embeddings are not proof of a classifier |
| Porting the Python adapter or proprietary Jev internals | No import in this change. Any later adapter port needs exact-revision provenance and dependency/license review; no model reproduction or distillation is planned |

The first implementation belongs with P6 routing, after its existing prerequisites.
Earlier engine, gateway, memory and completion tasks retain their own contracts
and deterministic behavior. They do not wait for a new decision service.

## Decision contract

Use `vcp-decision` as a logical module/crate boundary, mapped to the actual workspace
when useful code is implemented. Domain IDs and neutral record types belong with
P1's domain contracts; implementations depend on those contracts, never the reverse.
The controller invokes the evaluator through injected services. `vcp-policy` stays
deterministic and does not invoke a model, import a provider SDK or mint authority
from an answer. No second store, scheduler, ledger or credential facility belongs
in this package.

Proposed modules are `question`, `answer`, `evaluator`, `validation`, `provenance`,
`calibration`, and `adapters/{deterministic,openrouter_jev,openrouter_llm}`. Both
remote adapters share the VCP gateway transport, admission and credential services;
their request/answer codecs differ. Synthetic fixtures and
fake providers use the shared test harness. Create no empty scaffolding to reserve
these names. The conceptual interface is `evaluate(DecisionRequest, cancellation)
-> DecisionOutcome`; adapter errors and abstentions are typed outcomes, not a
successful answer filled with defaults.

| Record | Required meaning |
|---|---|
| Decision request | Decision/root/task/step IDs; purpose and question-set revision; closed questions; authorized context-manifest and evidence refs; workspace, steering, authority, policy, catalog and deletion revisions; input fingerprint/canonical cutoff; deadline and resource/call bounds |
| Boolean question | Stable question ID, exact proposition and allowed Boolean answer; optional two-label probability distribution requested explicitly |
| Choice question | Stable question ID and finite, versioned choice IDs; exact single-choice or probability-distribution mode; no model-invented executable action or choice |
| Score question | Stable ID, declared scale, units and finite min/max; a score is not a probability or confidence estimate |
| Decision outcome | Valid typed answers or explicit abstain/unsupported/invalid/unavailable/cancelled result, with reason; no implicit false/zero answer for missing evidence |
| Provenance | Input/schema/prompt and adapter revisions, requested and observed model/provider/config identities, rule version for deterministic output, original artifacts and validation transforms, timing and attempt/reservation/usage refs, calibration cohort and evidence state |
| Consumer receipt | Decision reference, consumer/policy revision, applied or ignored reason, affected task revision and any separately admitted action; no effect from merely receiving an answer |

Record unknown provider versions as unknown. Provenance links to canonical charges;
the response's estimated cost is never an independent authoritative ledger total.
Distinguish rule output, discrete model output, model-reported probabilities and
empirically calibrated probabilities. A one-hot encoding of a discrete choice is
not measured certainty. Calibration is scoped to a task class, schema and evaluated
model configuration; schema/model/cohort drift invalidates that qualification.

Each required question has either a valid answer or an explicit abstention; the
versioned consumer policy states whether a complete batch containing abstentions
is usable. Missing answers never become a silently consumed partial batch.

Reject duplicate JSON keys/question IDs, unexpected/missing questions, invalid
choice IDs, non-finite/out-of-range values, wrong modes, oversized output and tool
calls. Validate a complete result before using any answer; no streaming partial
answer can trigger work. Probability distributions must include exactly the declared
labels and sum to one within a documented numerical tolerance. Reject negative,
zero-mass and materially malformed distributions. If small rounding correction is
permitted, record the original and transform; normalization does not calibrate a
model. Numeric tolerances and application thresholds are versioned P6-04 choices,
not the exploration's illustrative constants.

Treat all supplied evidence as untrusted data, including strings that imitate
questions, system instructions or options. Questions and answer schemas come from
trusted versioned code/configuration; evidence cannot edit them. Carry relevant
evidence references and missing/truncated-input flags. A result without adequate
evidence can abstain; a fluent explanation is not an observed fact.

## Jev through OpenRouter qualification

The [OpenRouter listing](https://openrouter.ai/typesafe/jev-1.13) identifies
`typesafe/jev-1.13` as a structured decision model. This September 18, 2026
observation nominates a candidate; it does not freeze availability, price, context
limits or API compatibility. P6-01 stores current dated model/provider metadata;
P6-02 qualifies its decision transport within the existing gateway. Prefer an
explicit version for evaluation, retaining requested and observed served identities.
An alias change or unavailable served identity must be visible and must not inherit
old qualification silently.

P6-02 must produce a transport/semantics record before enabling even a shadow trial:

1. Verify the official OpenRouter route, request/response schema, supported question
   types, batch/context/output limits, error behavior and authentication. The
   inspected documentation does not establish a standard chat-completions model
   swap. If a distinct decision endpoint is required, add a gateway operation using
   the existing OpenRouter origin, credentials, admission and receipts. Do not
   invent an endpoint, proxy TypeSafe secretly or stringify native answers into
   a chat transcript to claim support. Missing documentation or required semantics
   leaves this adapter unavailable with a recorded reason.
2. Map VCP question IDs, criteria and answer spaces explicitly. TypeSafe's
   [confidence documentation](https://docs.typesafe.ai/confidence) distinguishes
   Noul's yes probability from Choice/Score distributions and their derived
   confidence statistic. Preserve these fields and their original meaning when
   exposed by OpenRouter. Noul-to-Boolean thresholding is a versioned consumer rule;
   Choice confidence is not the winning option probability. Score uses declared
   ordered levels and an explicit mapping to the VCP scale; an unsupported arbitrary
   scale abstains rather than fabricating numeric precision. Missing distributions
   cannot be replaced with one-hot certainty. Any reduced-capability mode requires
   separate qualification and cannot masquerade as native probability support.
3. Retain request identity, served attribution and all exposed usage/charge fields.
   Qualify pricing units and bounded admission for this operation rather than
   assuming chat output-token accounting applies. Missing usage stays uncertain;
   advertised free output does not make a failed or cancelled request unbilled.
   Verify cancellation, rate limits, deadlines, retry ownership and reconciliation
   with the common task ledger. Client cancellation does not prove remote billing
   stopped. Record unsupported gateway controls and block uses that require them.
4. Enforce current provider/data restrictions for TypeSafe as an upstream processor
   even though OpenRouter remains the sole gateway. Use minimized authorized state;
   no new TypeSafe credential, endpoint discovery, LangChain tracing destination or
   fallback outside OpenRouter is introduced. Native answers still undergo the
   complete validation and lifecycle checks above.
5. Create small synthetic request/response fixtures for each supported primitive,
   including mixed batches, missing fields, unexpected model IDs, malformed/partial
   results and failures. Label locally invented responses as synthetic; they are
   not evidence of a live API contract. A separately authorized, capped smoke run
   must verify required behavior before live quality trials. Keep exact source,
   model, schema, configuration and evidence identities with pass/fail/not-run state.

The primary-source inspection did not establish all gateway response details.
Native TypeSafe documentation defines semantics to verify, not proof of OpenRouter
parity. A missing feature may limit which purposes Jev can qualify for without
blocking the existing rules-only product behavior.

The [LangChain TypeSafe connector](https://github.com/langchain-ai/langchain/blob/master/libs/partners/typesafe/langchain_typesafe/classifier.py)
defaults to TypeSafe credentials and `/v1/systemone` on its own service. It does
not establish a ready-made OpenRouter adapter. VCP uses its existing Rust HTTP
facilities, avoiding another production language runtime and competing middleware
for lifecycle, retries or tracing. A standalone LangChain comparison experiment
would require its own scope, pinned dependencies and data/spend controls; it is
not required to qualify VCP and is not scheduled by this plan.

## Dispatch and recovery

1. The controller checks an explicit trigger and effective feature policy. Disabled
   means no helper network call. Use deterministic rules when they suffice; skip a
   remote helper when its expected benefit cannot justify its bounded cost. Batch
   questions only for compatible scope, purpose, freshness and deadline.
2. Capture the scoped canonical evidence projection and revisions. Apply current
   access/deletion rules and ordinary context-manifest/data-egress validation.
   Do not send full history merely because the evaluator can accept a large string.
3. Select the evaluator through a fixed, qualified role configuration or pure
   deterministic selector. It must never invoke itself to choose its own model,
   repair its answer or decide whether to retry. Unsupported structured output
   either uses a separately qualified parse path or produces an unavailable result.
4. For remote work, use the existing model gateway and atomically reserve the actual
   bounded request through the root ledger. Helper concurrency, call count,
   deadline and repair limits apply across all questions and attempts. Protect the
   verification reserve. Shadow evaluations cost money too and require declared
   enablement, permitted inputs and a configured cap.
5. Persist manifest, decision identity and attempt linkage before dispatch. Disable
   SDK-hidden retries. Each correction, transient retry or stronger-model fallback
   needs its own admitted attempt within the same total limits. Invalid output and
   failed calls still count toward cost. A helper cannot extend its own allowance.
6. Validate the complete answer and durably retain its interpretation/provenance.
   Before applying it, recheck task, steering, authority, policy, catalog, evidence
   freshness, deletion revision and pause state. Compare expected revisions and
   persist the consumer receipt plus transition in one conditional canonical
   commit under the same lifecycle fence, so pause/revocation cannot race a prior
   check. A failed comparison records or returns a stale outcome with no action.
   Any downstream model/tool action gets fresh admission
   and the normal prepared-effect protocol. No transaction spans a network wait.
7. On failure, abstention, invalid/stale output or insufficient qualification,
   record the reason and follow only the finite configured transition sequence
   below. Without a qualified permitted remote fallback and remaining allowance,
   use rules or surface a blocked/input-needed state if no permitted action remains.
   Failure cannot relax a hard constraint or reset the shared allowance.

An explicitly configured per-purpose fallback may use the qualified conventional
OpenRouter LLM for explicitly enumerated Jev failure or abstention reasons. Bind
that transition to a finite policy sequence with shared deadline/call limits, fresh capability/data
checks and a new reservation; prior uncertain liabilities remain. No ping-pong
between backends, silent model switch under a strict pin or automatic TypeSafe
direct retry is permitted. If the fallback lacks authority, qualification or
budget, use rules or expose the existing stop/input-needed state. Record backend
changes so LLM-generated confidence is never relabelled as native Jev evidence.
Stale output triggers reconstruction from current authorized evidence and revisions
before reconsidering any fallback. Superseded tasks, revocation or a pause never
trigger a replacement call; resume still requires deliberate lifecycle admission.

`/pause`, owner loss, child-local holds and supersession stop new decision dispatch
through the same root/child lifecycle barrier as all other model assistance. Cancel
active requests where supported; keep late output and usage linked to their original
attempt. A late answer cannot resume or mutate paused/superseded work. Receipt and
charge reconciliation remain permitted under the existing pause contract. Reopening
or `/resume` first reconciles ambiguous sends and validates current inputs; never
blindly repeat a possibly billed request or reapply a stale consumer action.

Decision records use the canonical artifact/store lifecycle and encrypted snapshot
contract. Pruning protects live accounting/recovery references; otherwise deletion
or restriction prevents reuse through a cache, saved report or prompt. An optional
cache binds purpose, question/schema, all relevant input fingerprints, workspace/
access/deletion scope, model/configuration, policy and calibration revisions. A
cache hit rechecks current access and freshness, records its origin, incurs no new
remote charge, and cannot count the original attempt twice. Caching may initially
be omitted; correctness must not depend on it.

## Consumer boundaries

Routing applies hard eligibility and measured quality constraints first. Optional
advice then supplies recorded task-class or suitability signals for explicit policy
rules, never new model/provider eligibility. Final ranking remains deterministic
given captured inputs and the recorded validated answer; a new probabilistic call
is not promised to repeat the same output. Inspectors show baseline choice, advice,
its uncertainty, policy rule and final choice or why advice was ignored.

Escalation can consider suspected repeated strategy, lack of progress and proposed
retry/replan/escalate/stop classes. Persist the existing counters and triggering
observations. An answer does not reset retries, enlarge a cap, ignore a strict pin
or claim a failed tool succeeded. Prefer direct test/error evidence where available.

Review/delegation recommendations flow to P7's existing scheduler under current
authority, depth/concurrency, worktree and root allocation limits. A low risk score
cannot suppress a required review/test. A high score may justify an additional
bounded review but is not itself a finding, an approval or a reason to merge work.
Completion still requires P2-06's actual acceptance evidence and reconciled effects.

`/optimize` may classify waste using authorized history with visible sample limits.
It proposes a versioned configuration diff; only selected permitted fields are
applied. Advice cannot run paid comparison trials, delete history, change grants or
raise a budget implicitly. No automatic training or upload of project history is
introduced. P5 memory proposals keep evidence, claim classes and governance gates;
probabilities cannot establish truth or discard observations. Local retrieval and
embedding paths stay local even when extraction uses an admitted OpenRouter call.

## Qualification and rollout

Implement one coherent P6 behavioral milestone containing the contract, the
deterministic path and separate Jev/LLM codecs within the OpenRouter path,
real routing/escalation callers,
inspectable outcomes, failure tests and documentation. Keep P6-04's existing memory
prerequisite and P8's actual delegation/release evidence separate; no task becomes
complete from adding this design. Use the local-first delivery workflow.

Start disabled for remote advice. Exercise deterministic and fake-provider cases
without credentials: malformed/duplicate/out-of-range output, abstain, unsupported
model, hostile evidence, unknown price, zero budget, retry exhaustion, recursive
selection prevention, concurrent reservations and changed input. Independently
observe request/effect counts and ledger state. Disabled/denied/paused cases must
send zero new requests. Test pause between validation and consumer application,
late settlement, crash after send/before receipt, duplicate recovery, cache access
revocation and pruning. Real lifecycle/native evidence belongs in existing suites;
mock cancellation alone does not establish OS or retained-loop behavior.

For quality, P6-04 declares frozen datasets, labels/rubrics, project/time-separated
tuning and held-out partitions, baseline policies, cost caps and acceptance margins
before running comparisons. Compare deterministic routing, actual Jev through
OpenRouter and a conventional OpenRouter LLM on matched tasks; a stronger LLM and
discrete/probabilistic variants are optional further comparisons under the same
declared cap. Jev is the preferred specialized candidate to evaluate, not a presumed
winner. Keep the downstream coding strategy fixed when isolating evaluator effects.
If Jev's transport cannot qualify, report that arm as not run and preserve the
baseline; do not substitute an emulation and label it a Jev result. Direct TypeSafe
access outside OpenRouter is excluded. No live calls in routine CI.

Include the vendor's documented [Jev 1.13 limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13)
in VCP-specific fixtures: adversarial evidence, distracting context, literal or
ambiguous criteria, numerical/date questions and multi-step indirection. Compute
exact counts, arithmetic and date comparisons in Rust. A typed response or native
probability does not establish immunity to prompt injection or VCP-domain accuracy.
Exercise Jev outages and removed/changed model versions, permitted LLM fallback,
fallback budget denial and lost probability fields with independent request counts.

Measure accuracy and abstention coverage; false and missed escalation/review;
Brier score and calibration for labelled Boolean/categorical distributions; and
scale-appropriate errors for scores. Also measure downstream coding quality,
required-check/review coverage, interventions, p50/p95 latency, total task spend,
uncertain liabilities and every failed/helper/repair attempt. Count net calls and
tokens saved against the same baseline, including evaluation overhead. Report
sample sizes and uncertainty; vendor benchmarks and self-reported probabilities
do not select a default.

Admitted shadow mode records suggestions while the baseline controls work. Promote
only a versioned task-class policy whose held-out quality floor and total-cost or
latency objective pass declared margins. Sparse evidence keeps the baseline. A
configuration change is explicit and reversible; retain a deterministic fallback
and expose model/schema drift or regressions. P8 rechecks enabled and disabled
behavior with real children, pause/reopen and the packaged Windows product.

## Research and source qualification

The OpenRouter model listing, TypeSafe confidence/limitation pages and LangChain
connector linked above were checked September 18, 2026. They support the candidate
selection and identify qualification questions; no Jev inference or gateway
conformance test was run in this planning change. Recheck and record exact source
versions and dates at implementation. Do not use vendor speed/cost claims as a
substitute for measured VCP outcomes.

The [TypeSafe System One adapter README](https://github.com/typesafe-ai/system-one-adapter-python)
describes typed evaluation over ordinary LLM APIs, discrete/probability modes,
structured-output validation, retry traces and usage reporting. Its
[license file](https://github.com/typesafe-ai/system-one-adapter-python/blob/main/LICENSE)
identifies an MIT-licensed adapter. These primary sources were consulted September
18, 2026; they are research inputs, not an imported revision or a review of its
dependency closure. This design copies no upstream implementation or fixtures.

Before any port, record immutable origin, exact files, dependencies, modifications,
license/notice obligations and tests through [ADR-013](../adr/013-upstream-reuse-and-vendoring.md).
Do not inherit provider SDK auto-discovery, raw credential logging, hidden retries
or permissive normalization. The adapter's license establishes nothing about Jev
model weights or hosted-service permissions. Review current service/data terms and
provider eligibility when qualifying Jev through OpenRouter; direct TypeSafe access
outside OpenRouter would additionally require an owner scope decision. No
vendor performance or reuse-percentage claim is accepted as VCP evidence here.
