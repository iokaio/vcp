# Exploring Jev and bounded semantic decisions

Status: research discussion supplied for planning; original discussion preserved
below. Its recommendations, illustrative thresholds, reuse percentages, vendor
performance and service/license claims are not confirmed VCP requirements or
qualification evidence. In particular, a second remote gateway is not adopted.

The September 18, 2026 planning integration is recorded in
[ADR-020](../adr/020-bounded-semantic-decisions.md), the
[adoption map and supporting design](decision-evaluation-design.md#adoption-map),
and [P6 implementation work](../plan/12-routing-and-optimization.md).
Those documents distinguish selected engineering proposals from deferred ideas
while preserving OpenRouter, local retrieval, budgets and Rust authority.

Plan revision 7 explicitly prioritizes actual Jev through OpenRouter qualification
with a thin Rust adapter, compared with rules and a conventional OpenRouter LLM.
See the [gateway qualification contract](decision-evaluation-design.md#jev-through-openrouter-qualification).
LangChain's connector is a research reference, not a required VCP runtime. The
discussion below remains the original exploration rather than current policy.

---

Yes. In fact, after reading the current VCP architecture, I think **VCP is unusually well suited to a Codex + System-One hybrid**.

The interesting part is not simply “add Jev.” The better architecture is:

> **Codex-style LLMs do the expensive thinking, planning, coding, and explanation. A System-One layer does the frequent little judgments that surround that work. Rust remains in charge of policy, state, budgets, authority, and side effects.**

That lines up almost uncannily with where VCP is already headed.

### Why VCP is such a good fit

VCP already separates the Codex-derived coding engine from routing, budgets, policy, memory, execution, delegation, and verification. The architecture explicitly calls for model routing based on task evidence, selective escalation, selective independent review, bounded delegation, cost accounting, and `/optimize`. ([GitHub][1])

And the planned Rust responsibility map already has exactly the seams you'd want: `vcp-engine`, `vcp-models`, `vcp-routing`, `vcp-budget`, `vcp-policy`, `vcp-memory`, `vcp-tools`, and so on. ([GitHub][2])

The repository is also early enough that this can be introduced cleanly. The current Codex source is the engine foundation, Munarium libraries are being added to that Cargo workspace, while the VCP-specific logical packages are still integration destinations rather than a mature frozen implementation. ([GitHub][2])

So I would change the architecture **now**, before `vcp-routing` hardens around the assumption that every fuzzy decision is either a Rust heuristic or another LLM call.

## The architecture I would build

Think of VCP as having two AI nervous systems:

```text
                 VCP Rust Engine
                      │
          ┌───────────┴───────────┐
          │                       │
     FAST JUDGMENT            DEEP WORK
     "System One"             "System Two"
          │                       │
    ┌─────┴─────┐           ┌─────┴─────┐
    │           │           │           │
  Jev       LLM-backed     Codex      Claude/
            evaluator       style      Gemini/etc.
    │           │              │
    └─────┬─────┘              │
          │                    │
          └──────── Rust ──────┘
                policy
                budgets
                evidence
                authority
                memory
                verification
```

I would actually create a new conceptual package:

```text
vcp-decision/
```

or perhaps:

```text
vcp-eval/
```

Do **not** put Jev directly inside `vcp-routing`.

Instead define a vendor-neutral primitive:

```rust
trait DecisionEngine {
    async fn evaluate(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse, DecisionError>;
}
```

with something conceptually like:

```rust
enum Question {
    Boolean(BooleanQuestion),
    Choice(ChoiceQuestion),
    Score(ScoreQuestion),
}

struct DecisionResponse {
    answers: BTreeMap<QuestionId, Answer>,
    model: ModelIdentity,
    latency: Duration,
    cost: Money,
    provenance: DecisionProvenance,
}
```

Then VCP could have multiple implementations:

```text
DeterministicDecisionEngine
OpenRouterDecisionEngine
JevDecisionEngine
LocalDecisionEngine        // later
MockDecisionEngine         // tests
```

That separation is the key.

## Where Jev could materially improve VCP

| VCP decision                  | System-One role                                                 | LLM still needed?                    |
| ----------------------------- | --------------------------------------------------------------- | ------------------------------------ |
| Model routing                 | “Which eligible model group fits this task?”                    | Only if ambiguous                    |
| Complexity classification     | Score trivial → difficult                                       | No, usually                          |
| Escalation                    | “Has this attempt genuinely stalled?”                           | Sometimes                            |
| Retry vs change strategy      | Choice: retry / replan / escalate / stop                        | Sometimes                            |
| Delegate or stay single-agent | Probability that delegation adds value                          | No                                   |
| Reviewer trigger              | “Does this diff warrant independent review?”                    | Reviewer itself may be LLM           |
| Review triage                 | Security / correctness / concurrency / API-risk probabilities   | Deep findings may need LLM           |
| Context selection             | Rank candidate chunks for usefulness                            | No                                   |
| Retrieval filtering           | Relevant / contradictory / likely noise                         | No                                   |
| Test selection                | Pick from known applicable test families                        | Usually no                           |
| Memory proposal triage        | “Is this worth remembering?”                                    | Possibly                             |
| Contradiction suspicion       | “Does new observation appear inconsistent?”                     | Deterministic verification afterward |
| Loop detection                | “Are we repeating the same failing strategy?”                   | No                                   |
| Completion readiness          | “Is there evidence the requested outcome is actually complete?” | Verification remains deterministic   |
| `/optimize`                   | Classify where observed waste is occurring                      | LLM explanation optional             |

That is a **lot** of little model decisions.

VCP's present design already says routing should observe tool validity, patch applicability, tests, repeated failure, and intervention, then decide whether to escalate, change strategy, or stop. ([GitHub][1])

Right now the design largely assumes those decisions will be rules derived from metrics.

I would make that:

```text
hard deterministic constraints
        ↓
semantic judgment
        ↓
probability/confidence
        ↓
Rust policy
        ↓
action
```

That is much more powerful.

### Example: model routing

Current VCP intends to derive task role and complexity signals, filter candidates, rank quality/cost/latency, then select a model. ([GitHub][1])

You could retain the deterministic filters:

```text
requires tools?
context fits?
provider allowed?
budget fits?
model available?
data policy permitted?
```

Then have a System-One evaluator consider the surviving candidates:

```text
STATE
-----
task objective
languages involved
estimated affected files
current errors
historical model performance
project architecture
previous failed attempts
remaining budget

QUESTIONS
---------
task_complexity:
    trivial
    routine
    difficult
    architectural

best_strategy:
    low
    medium
    high
    frontier

needs_planner:
    true / false

benefits_from_parallel_investigation:
    true / false
```

The Rust router then decides what to do with those probabilities.

That is fundamentally different from allowing AI to control the system.

**AI supplies a measurement. Rust supplies authority.**

That distinction fits VCP beautifully.

## Another excellent use: catching the “agent is stuck” problem

Coding agents waste remarkable quantities of tokens doing this:

```text
try A
fails
think harder
try A'
fails
think harder
try A''
fails
think harder...
```

You don't necessarily need GPT-6-class reasoning to recognize that pattern.

VCP has the complete task history.

Give a decision engine:

```text
last 6 actions
last 6 errors
files changed
tests attempted
tool outcomes
current objective
```

Ask:

```text
repeating_strategy?        0.94
making_progress?           0.17

next_action:
    continue               .04
    retry                  .07
    replan                 .61
    escalate_model         .21
    ask_user               .07
```

Then Rust applies bounded policy.

That could save real money.

## Code review may be the killer application

There's already an experimental Jev code-review project using essentially this architecture. It uses Jev for staged bounded judgments such as risk screening, file profiling, evidence selection, mechanism classification, severity scoring, and reviewer routing while keeping workflow orchestration in ordinary code. ([GitHub][3])

VCP already requires selective independent review and has a dedicated reviewer role. ([GitHub][1])

Instead of immediately spawning another expensive frontier model over a giant diff, VCP could run something like:

```text
correctness_risk       .81
security_risk          .08
concurrency_risk       .72
api_compatibility_risk .39
test_gap_risk          .91
architecture_risk      .24
```

Then:

```rust
if test_gap_risk > 0.85 {
    run_test_review();
}

if concurrency_risk > 0.70 {
    delegate_to_concurrency_reviewer();
}

if security_risk > 0.80 {
    delegate_to_security_reviewer();
}
```

Instead of:

```text
"Hey Claude, review this entire thing and tell me whatever you notice."
```

That is a substantial architectural improvement.

---

# How much of Jev can actually be put into Rust?

Here's the crucial distinction.

| Part                                |                               Can VCP borrow it? |          My estimate |
| ----------------------------------- | -----------------------------------------------: | -------------------: |
| Jev model weights                   |                                               No |                   0% |
| Jev model architecture              |                           Not publicly available |                  ~0% |
| RLCD training implementation        | Not publicly disclosed sufficiently to reproduce |                  ~0% |
| Parallel model sampler              |        Concept known, implementation unavailable |                  low |
| Typed decision abstraction          |                                       Absolutely |                ~100% |
| Boolean / Choice / Score primitives |                                       Absolutely |                ~100% |
| Probability-based control flow      |                                       Absolutely |                ~100% |
| Confidence gates                    |                                       Absolutely |                ~100% |
| Workflow decomposition              |                                       Absolutely |                ~100% |
| Decision provider abstraction       |                                       Absolutely |                ~100% |
| LLM-backed System-One emulation     |          **Yes, substantial open source exists** | ~70–90% transferable |
| Retry/schema/normalization logic    |                                              Yes |             ~80–100% |
| Telemetry/evaluation methodology    |                              Yes, adapted to VCP |                 high |

TypeSafe itself has released an **MIT-licensed System One Adapter** that makes ordinary OpenAI-compatible and Anthropic LLMs behave behind the same decision-oriented interface. It supports probability responses, discrete responses, native structured output, schema validation, malformed-response retries, probability normalization, latency and token accounting, and detailed attempt traces. ([GitHub][4])

That is the gold nugget here.

You don't need to reverse engineer Jev.

### Port the System One Adapter idea to Rust.

You could do something like:

```text
                DecisionEngine
                     │
       ┌─────────────┼─────────────┐
       │             │             │
 OpenRouter       TypeSafe       Local
 SystemOne          Jev          future
  Adapter
       │
 Claude/GPT/
 Gemini/etc.
```

Then VCP works even when Jev isn't available.

And you can A/B them.

## One legal/IP caution

There is a very clean boundary here.

TypeSafe's **System One Adapter is MIT licensed**, so it explicitly permits use, copying, modification, distribution, sublicensing, and sale with the required copyright/license notice. ([GitHub][5])

Their hosted Jev service is a different matter. The current customer agreement prohibits reverse engineering the service and using its outputs for model distillation or developing a similar/competing model or service. ([TypeSafe AI][6])

So I would:

**Port the MIT adapter architecture.**

**Implement the public typed-decision paradigm.**

**Integrate Jev as a provider.**

**Do not attempt to clone Jev's proprietary model using Jev responses.**

That gives VCP essentially all of the useful software architecture without wandering into ugly territory.

# And there's an even better consequence

VCP currently says:

> all remote model-assisted reasoning, extraction and optimization uses the OpenRouter boundary. ([GitHub][1])

I would revise that slightly.

Make OpenRouter the **Generative Model Gateway**.

Create a second abstraction:

```text
Decision Gateway
```

But provide an OpenRouter-backed implementation.

So by default:

```text
VCP
 ├── GenerativeModelGateway
 │      └── OpenRouter
 │
 └── DecisionEngine
        ├── deterministic
        ├── OpenRouter System-One Adapter
        └── TypeSafe Jev [optional]
```

Why?

Because VCP currently has a wonderfully clean data-egress story. Engine, memory, retrieval and embeddings stay local; coding/model-assisted work goes through OpenRouter. ([GitHub][7])

Making Jev mandatory would suddenly introduce another external processor of repository information.

That's unnecessary.

Instead:

```text
VCP default
    ↓
System-One-over-OpenRouter

optional optimization
    ↓
native Jev
```

Then `/optimize` can even discover:

> “Your project makes 843 bounded decision calls per week. Native Jev would likely save X latency/cost. Enable it?”

Very VCP.

# The decision ladder I'd use

This is where I think VCP gets genuinely interesting.

```text
1. Deterministic Rust
        │
        │ cannot decide semantically
        ▼
2. Fast probabilistic decision
   Jev / System-One adapter
        │
        │ insufficient confidence
        ▼
3. Cheap generative model
        │
        │ insufficient evidence / complexity
        ▼
4. Frontier reasoning model
        │
        │ requires ownership decision
        ▼
5. Human
```

That architecture is much more interesting than today's coding agents.

The model isn't the application anymore.

**The model becomes one cognitive component inside an engineered system.**

And VCP already has the pieces around it: budgets, authority, governed memory, evidence, task history, delegation, verification, routing and durable state. ([GitHub][1])

# One boundary I would defend fiercely

Do **not** let a Jev probability become authority.

VCP already has the right rule:

> a model, retrieved document, memory claim or project instruction cannot grant execution authority. ([GitHub][1])

Keep that.

Similarly, VCP explicitly states that a model confidence score never substitutes for evidence in governed memory. ([GitHub][1])

Perfect.

So:

```text
Jev says:
"92% likely safe"

Rust says:
"Interesting."

Policy says:
"Doesn't matter. User approval required."

```

Exactly right.

Probability informs policy.

Probability **never becomes policy**.

# What I would change in VCP now

I would add an **ADR-020: Probabilistic Decision Layer** before doing serious implementation of P6 routing.

Its core principle would be:

> VCP distinguishes generative reasoning from bounded semantic judgment. Generative models produce plans, code, explanations and novel artifacts. Decision engines evaluate structured state against closed answer spaces and return typed probabilistic results. Rust code owns composition, thresholds, authority, budgeting, side effects and deterministic verification.

Then introduce:

```text
src/crates/vcp-decision/
```

with:

```text
question.rs
answer.rs
engine.rs
policy.rs
calibration.rs
telemetry.rs

providers/
    deterministic.rs
    openrouter.rs
    typesafe.rs

testing/
    fixtures.rs
    mock.rs
```

And make `vcp-routing`, `vcp-engine`, `vcp-memory`, and `vcp-policy` **consume** that interface instead of knowing about Jev.

That prevents vendor goo from leaking through the architecture.

## Then benchmark it ruthlessly

VCP is already planning evaluation-driven routing rather than assuming vendor claims. ([GitHub][1])

I'd build a decision fixture set from the VCP acceptance suites and compare:

```text
A. deterministic rules
B. cheap OpenRouter LLM as System One
C. stronger OpenRouter LLM as System One
D. Jev
```

Measure:

```text
decision accuracy
Brier score / calibration
false escalation
missed escalation
end-to-end coding success
p50/p95 latency
decision cost
total task cost
extra LLM calls avoided
tokens avoided
human interventions
```

That would tell you whether Jev actually earns a permanent place instead of just admiring its launch benchmarks.

TypeSafe reports 70–500 ms latency and very large workflow cost advantages for Jev, but even TypeSafe says those published gains are toward the high end of expected real-world improvements. ([TypeSafe AI][8])

I would **not** expect VCP to suddenly become 400× cheaper. The expensive part, actually writing and reasoning about code, still belongs to Codex-style models.

What I *would* expect is a dramatic reduction in the number of times VCP wakes up a big generative model merely to answer:

> Should I continue?

> Is this complicated?

> Which specialist should look at this?

> Is this relevant?

> Does this look suspicious?

> Is this attempt stuck?

> Should I review this?

> Which of these five options applies?

Those are exactly the cognitive crumbs that coding agents currently feed to very expensive dragons. 🐉

### My conclusion

**Yes, I would modify VCP around this idea.**

Not “VCP powered by Jev.”

Something more durable:

> **VCP becomes a hybrid cognitive architecture: deterministic Rust for control, System-One engines for rapid judgment, frontier LLMs for deliberation and creation, Munarium-style governed memory for evidence and continuity.**

Jev would then be an excellent accelerator, but not an architectural dependency.

And in terms of your original question, **you can borrow far more than I initially expected**, because TypeSafe has deliberately open-sourced the LLM System-One adapter under MIT. The proprietary bit you cannot put into Rust is Jev's actual intelligence engine. But **most of the surrounding software abstraction that makes Jev useful can absolutely become native Rust inside VCP.**

I think this is significant enough that **I would alter the VCP architecture before implementing P6** rather than retrofit it afterward.

[1]: https://github.com/iokaio/vcp/blob/main/docs/architecture/vcp-what.md "vcp/docs/architecture/vcp-what.md at main · iokaio/vcp · GitHub"
[2]: https://github.com/iokaio/vcp/blob/main/docs/plan/code-layout.md "vcp/docs/plan/code-layout.md at main · iokaio/vcp · GitHub"
[3]: https://github.com/devagrawal09/jev-review?utm_source=chatgpt.com "GitHub - devagrawal09/jev-review: A staged code-review workflow and local dashboard built with TypeSafe Jev. · GitHub"
[4]: https://github.com/typesafe-ai/system-one-adapter-python "GitHub - typesafe-ai/system-one-adapter-python: Drop-in TypeSafeClient replacement backed by LLM APIs · GitHub"
[5]: https://github.com/typesafe-ai/system-one-adapter-python/blob/main/LICENSE "system-one-adapter-python/LICENSE at main · typesafe-ai/system-one-adapter-python · GitHub"
[6]: https://typesafe.ai/legal/mca?utm_source=chatgpt.com "Master customer agreement - TypeSafe AI"
[7]: https://github.com/iokaio/vcp "GitHub - iokaio/vcp: Vibe Code Pro - An Experiment · GitHub"
[8]: https://typesafe.ai/blog/introducing-system-one-models-and-jev "Introducing System One Models & Jev - TypeSafe AI Blog"
