# Markov models in VCP: where they fit and what they would improve

Note: Max Lee commented on a post about VCP that Markov Models are still underrated. That led to the following research.

Status: research note, September 20, 2026. It proposes no task, dependency, default or requirement change. Nothing here is implemented, measured or qualified. The [architecture](../architecture/vcp-what.md) remains the product authority and the [plan](../plan/README.md) owns execution. Any adoption below belongs to an existing work item or needs its own decision record.

## 1. Question and short answer

The question is whether Markov chains, hidden Markov models (HMMs), Markov decision processes (MDPs) and partially observable MDPs (POMDPs) can extend VCP and make it a better coding agent.

The short answer is yes, in a specific role. VCP should not use a Markov model to generate code. A language model already does that with far more context than any transition table. The value is one level up, in the machinery that decides how the coding model is used: how much a task will really cost, when an attempt has stalled, when to retry or escalate, which context to load, and which check to run first.

VCP is unusually well prepared for this. It already has typed finite state machines, a durable causal event log, persisted escalation counters, a bounded advisory decision contract and a local optimizer that aggregates history. A Markov model is the natural statistical layer over exactly that structure. It is local, free to evaluate, deterministic given its recorded inputs, and explainable as a table of counts. Those properties match [section 9 of the agent rules](../../AGENTS.md), "determinism before heuristics", better than a remote evaluator does.

Ranked by expected value against risk:

| Rank | Application | Model | Main benefit to coding quality or cost | Natural owner |
|---|---|---|---|---|
| 1 | Task-cost and completion forecasting from lifecycle history | Absorbing Markov chain with rewards | Evidence-based retry, handoff and verification estimates; early budget warnings; waste diagnosis in `/optimize` | P6-02, P6-05 |
| 2 | Repository and co-change ranking for context | Random walk on a graph (personalized PageRank) | Better files in the prompt; fewer missed companion edits | P2-01, P2-08 evaluation |
| 3 | Stall and thrash detection | Higher-order chain, then HMM filtering | Earlier, cheaper, injection-resistant escalation signal | P6-03 |
| 4 | Verification ordering | Belief update with one-step value of information | Faster failure feedback with the same required checks | P2-06 |
| 5 | Escalation threshold proposals | Small MDP solved offline | Measured retry and switch limits as an explicit policy diff | P6-03, P6-05 |
| 6 | Synthetic lifecycle traces for fault campaigns | Sampling from a fitted chain | Plausible, seeded event sequences for engine fuzzing | Test segment 16 |
| 7 | Provider burst modelling | Two-state chain | Better choice between transport retry and endpoint switch | P6-03 retry policy |

Token-level Markov text generation, online reinforcement learning in production and a full POMDP solver are analysed and not recommended. Section 8 explains why.

## 2. Why VCP already looks like a Markov system

### 2.1 The state machines exist

A Markov chain needs a finite state set and observable transitions. VCP has four such machines today.

- **Turn lifecycle.** `TurnState` in [task.rs](../../src/crates/vcp-domain/src/task.rs) defines sixteen states and a closed legal-transition relation. The [architecture](../architecture/vcp-what.md#42-task-and-turn-semantics) requires every transition to carry a reason code and a causative event ID.
- **Task lifecycle.** `TaskState` has eight states, three of them terminal. Terminal states are exactly the absorbing states of a chain.
- **Tool effects.** `proposed` through `succeeded`, `failed`, `cancelled` or `outcome_unknown`.
- **Reservations and escalation.** [escalation.rs](../../src/crates/vcp-models/src/escalation.rs) persists separate transport-retry, quality-switch and decomposition counters with each admitted attempt.

Because transitions are durable, causally linked and replayable, a transition count matrix is a pure projection of canonical history. It needs no new capture. It can be rebuilt at any time, which fits the existing rule that projections are rebuildable and canonical records are authoritative.

### 2.2 VCP's recovery design is already Markov in spirit

The architecture says resume "is not a blind jump back into a saved stack frame". It revalidates policy, workspace, pending effects and budget from current state. That is the Markov property used as an engineering discipline: the future must be computable from the present state, not from the path taken. A design that obeys this rule is one whose behavior a Markov model can describe well.

### 2.3 Compaction is a search for a sufficient statistic

The Markov property holds when the current state contains everything about the past that matters for the future. The [compaction contract](../architecture/vcp-what.md#65-compaction-contract) lists exactly that set: objective, constraints, steering version, grants, file versions, pending effects, accepted decisions and next required verification. Everything else is "descriptive history" and may be summarized.

This gives a precise test of compaction quality. A compaction is good when the distribution of what happens next is the same with the full history as with the compacted state. If post-compaction turns show a different transition profile from matched uncompacted turns, such as more re-reads of the same files or more repeated failed edits, then compaction dropped part of the true state. That is measurable from transition counts alone, without grading any text.

### 2.4 Counters are how VCP repairs the Markov property

A first-order model over raw turn states forgets how many times a repair has failed. The standard fix is to enlarge the state, since a higher-order model is a first-order model over a larger state space. VCP already does this. The escalation counters turn "verification failed" into "verification failed for the third time after one quality switch". Any model in this note should use those augmented states, not the bare enums.

## 3. Constraints any adoption must respect

These come from existing contracts and they shape every proposal below.

- **Advice only.** Under [ADR-020](../adr/020-bounded-semantic-decisions.md), a probability never grants permission, admits spend, clears a failure or proves completion. The [consumer boundaries](../architecture/decision-evaluation-design.md#consumer-boundaries) apply unchanged to a statistical signal.
- **No opaque learned router.** [ADR-007](../adr/007-profiles-and-routing.md) rejects unexplained learned routing. A transition table with counts, sample sizes and a source window is explainable. A policy derived from it must be published as explicit rules, not consulted as a black box.
- **Correlation is not causation.** [ADR-017](../adr/017-project-optimization.md) says production correlations alone do not prove causal improvement. History was produced under the current policy, so counterfactual actions are unobserved. Model outputs are hypotheses for bounded, authorized trials.
- **Sparse evidence keeps the baseline.** Most projects will have tens of tasks, not thousands. Every estimate needs smoothing, a minimum sample gate and an abstain path.
- **Local and private.** Counts stay on the machine. Nothing here needs a network call, a new model asset or an upload of project history.
- **No new dependencies.** The matrices are at most a few dozen states wide. Plain Rust arithmetic is sufficient.
- **Truthful presentation.** An inferred hidden state such as "thrashing" is an estimate. It must be shown as one, in the same way the architecture forbids fabricating private model reasoning.
- **Money stays integer.** Probabilities may be floating point, as they already are in [decision.rs](../../src/crates/vcp-models/src/decision.rs). Monetary results derived from them must convert to micros with upward rounding before they touch an estimate or a ledger comparison.

## 4. Applications

### 4.1 Absorbing chains: what a task will really cost

**Problem.** `CostEstimate` in [routing.rs](../../src/crates/vcp-models/src/routing.rs) has fields for first attempt, retries, handoff, support, children and verification. Today those are configured assumptions. ADR-007 warns that "a cheap request can increase total task cost". The selector needs the expected cost of the whole path to completion, not of the first request.

**Model.** Treat completed, failed, cancelled and blocked as absorbing states. Treat the working states, keyed by task class and exact model endpoint and augmented with attempt counters, as transient states. Estimate the transition matrix from retained history. Partition it into `Q`, the transient-to-transient block, and `R`, the transient-to-absorbing block.

```text
N = (I - Q)^-1        expected visits to each transient state
B = N R               probability of ending in each absorbing state
t = N 1               expected number of steps before absorption
v = N c               expected total cost, where c is the mean cost per visit
```

**Worked illustration.** The numbers are invented to show the mechanics. The three transient states are aggregates across steps, not the literal `TurnState` enum: `M` is one model cycle, `T` is tool execution and `V` is verification. A failed verification that leads to a repair cycle is the `V` to `M` transition.

```text
from \ to     M      T      V      completed   failed
M             -      0.72   0.25   -           0.03
T             0.98   -      -      -           0.02
V             0.33   -      -      0.65        0.02
```

Each visit to `M` returns to `M` with probability `0.72 * 0.98 + 0.25 * 0.33 = 0.7881`. Starting in `M`:

```text
expected model cycles      1 / (1 - 0.7881)   = 4.72
expected tool phases       0.72 * 4.72        = 3.40
expected verifications     0.25 * 4.72        = 1.18
probability of completion  1.18 * 0.65        = 0.767
probability of failure     1 - 0.767          = 0.233
```

A first-attempt estimate prices one model cycle. The chain says the task class costs 4.72 cycles on average and fails about one time in four. Sensitivity analysis is equally direct. Raising first-pass verification success from 0.65 to 0.80 cuts expected model cycles to 4.01 and lifts completion to 0.802. That tells the optimizer which transition is worth improving, which a flat success rate cannot.

**What this improves.**

- **Routing estimates.** The `retries`, `handoff` and `verification` fields gain evidence references in place of assumptions. A cheap model with a low first-pass rate is compared with a strong model on expected total cost, which is the comparison the `low` profile actually needs.
- **Budget foresight.** From the current state, the chain gives the probability that the task completes within the remaining funds. A low value can produce an early, visible warning while the protected verification balance is still intact, instead of a late `budget_exhausted` stop.
- **Waste diagnosis in `/optimize`.** Expected visits show where tasks spend their cycles. A large `T` to `M` loop count for one task class and model is a concrete, inspectable finding for the [optimizer report](../development/p6-routing.md).
- **Regression detection.** After a policy or model change, the log-likelihood of new task traces under the baseline chain measures drift. It complements `/optimize compare`, which today compares counts and cohorts.

**Risk.** Low. It is read-only analysis over existing records. It changes no dispatch path until a later, explicit decision lets an estimate feed the selector.

### 4.2 Random walks on graphs: better context

This is the application most directly tied to code quality, because a coding agent mostly fails by not seeing the right file.

**Repository map ranking.** PageRank is the stationary distribution of a Markov chain that walks a link graph. The architecture already names [Aider-style repository maps](../architecture/vcp-what.md#66-reused-discovery-and-repository-maps) as a context source to evaluate, and Aider ranks its map with PageRank over the symbol reference graph. The personalized variant restarts the walk at seed nodes:

```text
r = (1 - d) * s + d * W r
```

Here `W` is the column-normalized reference graph, `s` is the restart distribution and `d` is the damping factor. Seeds are the files and symbols named in the objective, the current diff and the latest diagnostics. The result ranks definitions by how reachable they are from the work in progress. That ordering fits the token budgeting rule in the architecture, which has to choose among candidates and record why others were excluded. The computation is local, deterministic for a fixed graph and seed, and converges in a few dozen sparse iterations.

**Co-change chains.** Git history gives an empirical transition estimate between files: the probability that `B` changes in a commit given that `A` does. Two uses follow.

- **Context candidates.** When the agent edits `A`, files with high co-change probability become candidates for the context assembler, labelled as navigation evidence and not as memory claims.
- **Missed companion edits.** At completion, a high-probability companion that was not touched, such as the test file, the schema or the changelog, becomes a visible note. Forgetting the companion edit is one of the most common agent defects, and this catches it without any model call.

**Memory retrieval.** A walk over the claim, evidence and file graph could serve as a third retriever beside Tantivy and DiskANN, fused by the existing [reciprocal rank fusion](../architecture/vcp-what.md#135-query-algorithm). Eligibility rechecks apply to its candidates exactly as to the others. Whether it earns a weight is a held-out evaluation question, as the fusion design already requires.

**Risk.** Low to moderate. The ranking is navigation evidence only. The cost is graph extraction quality across languages, which the architecture already lists as a fallback concern for maps.

### 4.3 Higher-order chains and HMMs: knowing when the agent is stuck

**Problem.** P6-03 needs a "repeated-strategy suspicion" signal. The current options are fixed counters, which are blunt, and a remote evaluator, which costs money, adds latency and reads untrusted content.

**Step one, a higher-order chain over actions.** Map each step to a symbol from a small closed alphabet, for example tool kind crossed with outcome class: `read.ok`, `edit.ok`, `edit.rejected`, `check.fail.same`, `check.fail.new`, `check.pass`. The four `ObservationKind` values in escalation.rs, action, error, diff and check, are already the right raw material. Two signals fall out of n-gram statistics over this stream.

- **Exact cycles.** A repeated n-gram such as `edit.ok, check.fail.same, edit.ok, check.fail.same` is a deterministic stall fact. It needs no probability at all and should be a rule.
- **Collapsing entropy.** When the empirical next-symbol distribution of the current task becomes far more predictable than the task-class baseline, the agent is looping even if the loop is not exact.

**Step two, an HMM over progress regimes.** Progress is not directly observable. Model it as a hidden state with a few values: exploring, converging, thrashing and environment-blocked. The observed symbols above are the emissions. The forward algorithm maintains a belief over regimes online:

```text
alpha_t(j) = b_j(o_t) * sum_i alpha_{t-1}(i) * a_ij     then normalize
```

Here `a` is the regime transition matrix and `b` is the emission table. The normalized `alpha_t(thrashing)` is a yes-probability in exactly the form the escalation advisory consumer accepts. Trusted integer thresholds already gate such probabilities, and `constrain_with_advice` already limits what any signal can do. The Viterbi algorithm gives the most likely regime sequence after the fact, which lets history inspection and `/optimize` report time spent thrashing per task class and model. Baum-Welch can fit the parameters offline from retained history, initialised from the labelled "true stall" and "productive repeated attempt" fixtures that the [P6-03 plan](../plan/12-routing-and-optimization.md) already requires.

**Why this is attractive for VCP specifically.**

- **Zero marginal cost.** No reservation, no helper attempt and no unknown liability. The ADR-020 consequence that advice "may add cost and latency without improving decisions" does not apply.
- **Injection resistance.** The model sees event classes, never repository text or model output. Hostile content cannot argue with a count table. An adversary can still shape the event sequence, for example with flaky tests, so the signal stays advisory.
- **A free comparison arm.** P6-04 compares rules, Jev and a conventional LLM. A local statistical evaluator is a natural fourth arm. If it matches remote advice on stall detection, the remote call is unnecessary for that purpose. If it does not, the evidence says so.

**Placement.** ADR-020 treats a "local semantic model" as unselected because of licensing, assets and resource qualification. A smoothed count table built from the user's own history raises none of those concerns, but whether it sits behind the evaluator boundary as a fourth implementation or remains a deterministic rule input is a decision for the P6-03 owner, not for this note.

**Risk.** Moderate. Hidden states are invented constructs. Few regimes, a small alphabet and Dirichlet smoothing limit overfitting. Parameters differ by model and task class, so sparse cohorts must abstain.

### 4.4 Belief and value of information: which check to run first

The honest model of a coding task is a POMDP. The true state, whether the change is actually correct, is hidden. Reads, builds and tests are observations with costs. Solving that POMDP is intractable and unnecessary. One consequence is cheap and exact.

All required checks must run before completion, and [verification](../architecture/vcp-what.md#95-verification) rules forbid suppressing any of them. Their order is free. If check `i` fails with probability `q_i` given the current diff and costs `c_i` to run, then running checks in descending `q_i / c_i` minimizes the expected cost to the first failure, assuming independent outcomes. Estimating `q_i` from history by changed path and check identity gives fail-fast ordering: the agent learns about its most likely mistake soonest, and repair cycles shorten. No check is skipped, so the completion contract is untouched.

The same belief framing explains a behavior worth encoding as a rule. When the belief that the change is correct is already high, another read adds little and a check adds a lot. When belief is low and errors are unfamiliar, information-gathering actions are worth more than another edit. This is an explanation of good agent behavior, and a candidate prompt or skill heuristic, not a proposal for a solver.

### 4.5 MDPs: escalation thresholds as a published table

**Model.** The state is task class, current capability group, the three escalation counters, last trigger kind and a coarse budget bucket. The actions are the ones VCP already names: retry, replan, escalate, decompose and stop. The reward is verified completion value minus all spend, including support and verification. Hard limits make it a constrained MDP. With the transition model from section 4.1, value iteration solves it offline in milliseconds:

```text
V(s) = max over a of [ r(s, a) + sum over s' of P(s' | s, a) * V(s') ]
```

**The useful insight.** If attempts were independent with a fixed success probability, the optimal policy would be trivial: either never use the cheap model or retry it forever. A threshold such as "retry twice, then switch" is only optimal because failure is informative. Each failure raises the probability that the task is hard for this model. So the quantity to estimate is `p_k`, the success probability on the next attempt given `k` failures so far. A myopic index rule then reads:

```text
stay with the current model while   c_current / p_k   <   c_stronger / p_stronger
```

The persisted counters in escalation.rs are exactly the state needed to estimate `p_k`. The output is a small table of limits per task class. It belongs in `/optimize` as a proposed policy diff for `max_transport_retries` and the quality-switch limits, with sample sizes and uncertainty shown, applied only if the developer selects it and reversible by the existing rollback.

**What this is not.** It is not online reinforcement learning. Exploration means spending the user's money on actions believed to be worse, and ADR-017 requires an authorized cap for controlled trials. It is also not a causal estimate. Observed transitions under the old policy are confounded by which tasks reached which states. The table is a hypothesis, the bounded opt-in trial is the test, and the shadow-mode discipline from P6-04 applies.

A one-state MDP is a bandit. Choosing among eligible models within a group by posterior sampling would be the smallest version of this idea. It needs the same authorized trial budget and the same deterministic replay of the recorded choice, so it is not simpler in governance terms than the table.

### 4.6 Sampling: realistic synthetic traces

A fitted chain is a generator. Seeded sampling produces plausible lifecycle event sequences at any volume, and deliberately perturbed sampling produces illegal ones. Both are useful for the fixtures in the [test plan](../plan/16-test-fixtures-and-acceptance.md): fuzzing the turn transition function, driving the scripted provider with realistic retry and failure bursts, and sizing history browsing and retention against representative data. Committed fixtures must come from synthetic or invented matrices, never from a private project's counts.

### 4.7 Two-state chains: provider bursts

Provider failures cluster. A two-state chain with a good and a degraded state, estimated per endpoint from attempt outcomes, captures that `P(fail | just failed)` exceeds the base rate. The practical consequence sits in the boundary between transport retry and endpoint switch: in a degraded burst, another immediate retry on the same endpoint has a poor expected return compared with backoff or an already eligible alternative. The benefit is modest and the estimate needs many attempts per endpoint, so this ranks last.

## 5. Data: what exists and what is missing

| Need | Available today | Gap |
|---|---|---|
| Turn, task and effect transitions with causes | Canonical events with reason codes and causative IDs | None for counting; a projection must define the aggregated state alphabet |
| Attempt lineage and retries | Attempt records with `previous` links; persisted escalation counters | None |
| Cost per state visit | Settled usage per attempt; uncertain liability kept separate | Unknown charges must widen the estimate, never count as zero |
| Task class and endpoint cohort keys | Routing decisions record both | The optimizer notes that task class is unknown without a retained routing decision |
| Normalized action symbols | Tool runs, verification records, the four advisory observation kinds | A versioned closed alphabet and a "same failure as before" comparison |
| Language, task size, intervention, abandonment | The optimizer report states these have no normalized retained observation | Needed for cohorts; absent data must stay "unavailable", not zero |
| Reference and co-change graphs | Git history; repository map work under P2-01 evaluation | Symbol extraction per language, with the documented fallbacks |

Two storage questions need an owner decision before any persistent model exists.

- **Pruning.** A transition table that outlives the history it was computed from retains information about pruned work. The conservative choice is to rebuild models from currently retained history and label the window, as the optimizer already does. A surviving aggregate would need to be declared in the retention contract.
- **Portability.** A model artifact is derived state. It should be rebuildable after restore instead of becoming new canonical content in a snapshot.

## 6. Engineering shape

The mathematics is pure and small, so it fits existing boundaries without a new crate, gateway or store.

- **Pure functions** would sit beside `routing` and `escalation` in `vcp-models`: counting, smoothing, absorbing-chain solves, forward filtering and power iteration. They take typed inputs and return typed results with sample counts.
- **Projections and windows** belong with the optimizer state in `vcp-lifecycle::foundation::routing_state`, which already owns report windows, cohorts and revision checks.
- **Presentation** belongs in `vcp-cli::optimize` and the existing inspectors.
- **Graph ranking** belongs with candidate selection in `vcp-context`, behind the `ContextSource` boundary that the architecture names for discovery and repository maps. That name is a design term today, not an implemented type. The ranking would be one more candidate source with recorded selection reasons.

Details that matter in this repository:

- **Deterministic iteration.** Use ordered maps and fixed iteration counts. A result must not depend on hash order.
- **Record, do not recompute.** Floating-point library functions are not guaranteed bit-identical across platforms. A consumed signal must be persisted with its inputs' digests, and replay must use the recorded value. This matches the existing rule that replay never re-sends a decision request.
- **Smoothing and abstention.** Use a Dirichlet prior per row, a minimum sample gate aligned with the routing policy's existing sample minimum, and credible intervals in reports. Below the gate, the model abstains and the deterministic path is unchanged.
- **Versioned artifacts.** A fitted model names its alphabet revision, source window, cohort keys, sample counts, policy and catalog revisions, and a digest. A change in model identity, policy or alphabet invalidates it, because the process it describes has changed.
- **Numerical safety.** Work in log space for sequence likelihoods. Solve small systems by elimination with a singularity check. A chain with no path to absorption is a modelling error and must surface as one.

## 7. How to know whether any of it works

**Test the assumption first.** Compare first-order and second-order models on held-out traces by log-likelihood with a complexity penalty. If the second-order model wins clearly, the state is missing something and should be augmented, usually with a counter. Check multi-step predictions against observed multi-step frequencies. A chain that predicts one step well and five steps badly is hiding memory.

**Measure signals the way P6-04 already requires.** Use frozen datasets with project-separated and time-separated tuning and held-out partitions. Report the Brier score and calibration for the stall probability, plus false and missed escalations against the labelled stall fixtures. Report sample sizes and uncertainty. Run in shadow mode before any signal is allowed to constrain a transition.

**Measure outcomes, not proxies.** The reward and the success label must come from verification records and reconciled completion, never from a model's claim that it finished. Compare total task cost, completion quality, interventions and latency against the rules-only baseline on matched tasks. Count the engineering and inspection overhead honestly. A forecast that is accurate and changes no decision has no product value.

**Evaluate context ranking inside the existing comparison.** The architecture already plans to compare lexical search alone, lexical plus a map, and the memory-assisted pipeline on one task set. A PageRank-ordered map and a co-change source are additional arms in that same comparison, judged by whether the needed files reached the prompt and whether task outcomes improved.

## 8. What not to build

- **Markov text or code generation.** An n-gram model is a strictly weaker predecessor of the language model VCP already calls. It cannot improve code output.
- **Online reinforcement learning in the production loop.** Exploration spends the user's budget on purpose, makes behavior non-reproducible and conflicts with explicit, versioned policy. Offline analysis plus authorized trials achieves the legitimate part.
- **A general POMDP solver.** The state space of a repository is unbounded. Belief tracking and one-step value-of-information rules capture the useful part.
- **An opaque learned router.** Any learned quantity must surface as an inspectable table or an explicit rule, consistent with ADR-007.
- **Hidden-state labels presented as facts.** "Thrashing" is an inference with a probability and must be displayed that way.
- **Cross-project or uploaded models.** Pooling counts across users would need a data-sharing decision that nothing in VCP currently authorizes.

Feedback is the subtle risk. A model fitted under one policy stops describing the system once its own advice changes that policy. Versioned artifacts, invalidation on policy change and continued shadow measurement are the mitigation. Goodhart effects are the other: if completion probability becomes a target, only verified completion may count.

## 9. Suggested order, mapped to existing work

This is a reading of where each idea would land. It adds no task and changes no dependency.

1. **Chain analytics in `/optimize`, P6-05.** Read-only, local, no dispatch impact. It also produces the transition projection that everything else reuses.
2. **Deterministic cycle rule, then shadow stall probability, P6-03.** The exact repeated-n-gram rule is ordinary software and needs no qualification. The HMM signal starts in shadow beside the existing advisory contract.
3. **Evidence-backed cost estimates and a local comparison arm, P6-02 and P6-04.** Feed `CostEstimate` evidence only after held-out checks show the forecasts are calibrated.
4. **Graph and co-change context sources, inside the P2-01 and P2-08 evaluation.** Independent of the routing work and can proceed in parallel.
5. **Fail-fast check ordering, P2-06.** Small, safe and independently measurable.
6. **Threshold proposals from the offline MDP, P6-05.** Last, because it depends on a trustworthy transition model and on authorized trials to test its hypotheses.
7. **Child allocation forecasts, P7-04 through P7-06, and a regime-filter observer, P10-03.** Natural later reuse of the same projection.

## 10. Conclusion

Markov models do not make VCP's coding model smarter. They make VCP smarter about its coding model. The repository already records the states, transitions, counters and costs that such models need, and already has the advisory, versioning and qualification machinery that keeps a probabilistic signal from becoming an authority. The highest-value, lowest-risk steps are absorbing-chain cost analysis in the optimizer and random-walk context ranking. Stall detection by HMM is the most interesting candidate because it could answer one of P6's advisory questions locally, at no cost and with no exposure to untrusted text. Each step is small, local, deterministic given its recorded inputs and testable against the rules-only baseline that must remain a valid shipping configuration.
