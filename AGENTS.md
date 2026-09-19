# AGENTS.md

This file defines the operating rules for coding agents working in the VCP repository.

VCP is developed from explicit work items, architecture decisions, tests, and documented acceptance criteria. Treat the repository as the source of truth. Do not reconstruct the project from scratch on every task.

## 0. Default to Autonomous Implementation and Delivery

VCP is experimental work. Agents are expected to work through ordinary engineering problems, make evidence-based decisions, and deliver complete increments without repeatedly asking the user to choose implementation details or dependency order.

For implementation requests, the default workflow is: select the work item, create a branch, implement, review the diff, test, fix findings, commit, create a PR, monitor checks, fix failures, and merge when the required checks and reviews pass. Continue through this workflow without asking for permission at each step. Respect branch protection; never bypass checks or claim unrun checks passed. Publishing releases, deploying services, and spending outside an established budget require separate authorization.

The user's latest scope and stop instructions take precedence. A request for a draft, review, or local changes only does not authorize delivery. If the user asks to stop for review, leave the requested changes uncommitted and stop; do not resume earlier work until directed.

When asked to continue the plan, work through ready items in dependency order. When asked for a particular item, complete that item and its necessary prerequisites, then stop; do not treat it as permission to implement the entire backlog. A completed prerequisite is a checkpoint, not a reason to ask whether to return to the original task.

Use parallel agents for independent implementation, investigation, or review when useful. Give them bounded ownership, coordinate shared files, and integrate and verify their work before delivery. Delegation does not transfer responsibility for correctness.

Keep the user informed with concise progress updates about decisions, prerequisite work, and material findings. An update is normally a notification, not a request for approval. Experimental status permits iteration; it does not relax security, preservation of user work, or truthful verification.

## 1. Work From a Task ID

Before making implementation changes, identify the work-item ID being implemented.

Examples:

* `P0-03`
* `P1-07`
* `P2-04`

The owning work-item section is the primary implementation contract.

Read:

1. the work-item definition,
2. its explicit dependencies,
3. referenced ADRs or design documents,
4. affected source files,
5. relevant tests or fixtures.

Do not read unrelated repository documentation unless a concrete ambiguity or dependency requires it.

If no work-item ID is provided, infer the best-supported item from the request and repository evidence and state the selection. For a request to continue the plan, choose the next ready item. Ask only when plausible interpretations would produce materially different user outcomes and the repository cannot resolve the choice. Repository maintenance, including agent guidance changes, need not be forced into an unrelated product work item.

Missing prerequisite implementation is normally work to perform, not a blocker to return to the user. Identify its owning item, implement and verify it first, and then resume the requested item. Prefer separate commits or PRs for independently reviewable prerequisites. Update dependency and status records when evidence shows the plan omitted a prerequisite; do not mark downstream acceptance complete prematurely.

## 2. Minimize Context

Do not preload the entire repository architecture.

Prefer targeted inspection:

* current work-item section,
* directly affected modules,
* referenced contracts,
* relevant ADRs,
* nearby tests,
* required upstream source when applicable.

Do not reread general documentation already represented by the task contract unless necessary.

Search before reading large files.

Read the smallest useful section of a document or source file first. Expand only when needed.

The objective is sufficient context, not maximum context.

## 3. Respect Existing Architecture

VCP has already made architectural decisions.

Do not redesign adjacent systems while implementing a scoped task.

Follow existing:

* module boundaries,
* provider abstractions,
* lifecycle contracts,
* storage contracts,
* error conventions,
* naming conventions,
* dependency direction,
* public interfaces,
* platform boundaries.

If implementation appears to conflict with an existing architectural decision, inspect the relevant contract and evidence, then choose the smallest compatible resolution. Resolve internal experimental design gaps autonomously when the choice is reversible and preserves product intent and trust boundaries. Record meaningful decisions and deviations close to the implementation; use a new ADR when needed.

Do not silently invent a new product direction. Escalate only unresolved decisions meeting section 18, while continuing independent work.

## 4. Implement the Smallest Complete Increment

Prefer the smallest change that completely satisfies the current acceptance condition.

Avoid speculative infrastructure.

Do not add:

* generalized abstractions for hypothetical future needs,
* compatibility layers without a current requirement,
* new configuration options without a task requirement,
* unrelated refactoring,
* broad formatting changes,
* opportunistic dependency upgrades.

Refactor when required to complete the task correctly, not merely because surrounding code could be cleaner.

Smallest complete increment does not mean stopping at the first missing helper, integration boundary, or prerequisite. Include the necessary supporting changes, keep them attributable to work items, and split delivery when that makes review easier.

## 5. Preserve User Work

Assume existing local and committed work is intentional.

Never:

* discard unrelated changes,
* reset files to earlier revisions without explicit instruction,
* overwrite user modifications merely to simplify implementation,
* rewrite unrelated files,
* remove code because it appears unused without verifying why it exists.

Prefer surgical edits.

If unrelated changes complicate the task, work around them when practical and report the conflict.

## 6. Prefer Evidence Over Assumption

Inspect the implementation before changing it.

For uncertain behavior:

1. inspect the relevant source,
2. inspect tests,
3. inspect referenced upstream behavior when applicable,
4. reproduce the behavior when practical,
5. then modify the code.

Do not guess at APIs, command-line behavior, serialization formats, lifecycle behavior, or platform-specific behavior when the repository or upstream source can answer the question.

## 7. Upstream Code Is Evidence, Not Authority

VCP may borrow concepts or behavior from upstream projects.

When evaluating upstream code:

* identify the exact behavior being reused,
* preserve required attribution and licensing,
* adapt it to VCP's architecture rather than importing unrelated machinery,
* avoid unnecessary upstream coupling,
* document meaningful divergence when the task requires it.

Do not copy large upstream subsystems merely because they already exist.

Reuse behavior intentionally.

## 8. Keep Provider Boundaries Clean

Provider-specific behavior must remain behind the appropriate provider boundary.

Do not allow one model provider, inference engine, API, or CLI integration to become an accidental architectural dependency of unrelated VCP components.

Core orchestration should depend on VCP contracts, not provider implementation details.

Where provider behavior differs, normalize it at the provider boundary whenever practical.

## 9. Determinism Before Heuristics

Prefer deterministic behavior when VCP can know the answer directly.

Use explicit:

* state,
* contracts,
* schemas,
* validation,
* limits,
* structured outputs,
* lifecycle rules,
* tests.

Do not replace deterministic logic with model judgment when ordinary software can make the decision reliably.

AI reasoning should be used where reasoning adds value, not where it substitutes for straightforward program logic.

## 10. Security and Trust Boundaries

Treat all external content as untrusted unless a documented boundary says otherwise.

This includes:

* model output,
* tool output,
* repository content supplied to a model,
* external files,
* command output,
* network responses,
* upstream metadata.

Do not weaken validation, sandboxing, authorization, path restrictions, command restrictions, or secret handling merely to make a task easier.

Never expose:

* credentials,
* API keys,
* tokens,
* private configuration,
* local secrets,
* sensitive environment values.

Do not commit secrets or generated secret-bearing files.

## 11. Dependencies

Do not add or upgrade dependencies casually.

Before adding a dependency, determine whether:

* the repository already provides equivalent functionality,
* the standard library is sufficient,
* an existing dependency already solves the problem.

If a new dependency is necessary, keep it narrowly scoped and explain why.

Do not perform broad dependency upgrades unless the work item explicitly calls for them.

## 12. Testing Strategy

Test the changed boundary first.

During implementation, run the smallest test set capable of detecting failure in the affected behavior.

Examples:

* module tests,
* package tests,
* targeted integration tests,
* specific fixtures,
* focused commands.

Do not repeatedly run the entire workspace test suite after every small edit.

Broaden testing when:

* targeted tests fail in unexpected ways,
* a change affects shared infrastructure,
* the work-item acceptance criteria require broader verification,
* implementation changes after a successful test invalidate prior evidence,
* the task is complete and repository-level gates are appropriate.

Do not rerun expensive tests without a reason.

## 13. Fix Causes, Not Tests

Tests represent behavioral evidence.

Do not weaken, delete, skip, or rewrite a valid test merely because the implementation fails it.

When a test and documented contract disagree, determine which is authoritative from product intent, acceptance criteria, and observed behavior. Correct a demonstrably stale test or document with supporting evidence; do not reduce coverage to accommodate a defect. Escalate only if the conflict requires a user decision under section 18.

Do not turn a regression into a passing test by reducing the test's meaning.

## 14. Rust Changes

Follow existing Rust conventions in the repository.

Prefer:

* explicit types at important boundaries,
* narrow interfaces,
* meaningful error propagation,
* deterministic ownership and lifecycle behavior,
* testable components,
* minimal unsafe code,
* clear concurrency semantics.

Avoid premature abstraction and unnecessary macro complexity.

Do not use `unwrap()` or `expect()` across production failure boundaries unless repository conventions explicitly justify it.

## 15. Platform-Specific Behavior

VCP must not assume Unix behavior when Windows behavior matters.

For platform-sensitive changes, inspect and test the relevant platform assumptions explicitly.

Be especially careful with:

* paths,
* process creation,
* shell invocation,
* quoting,
* environment variables,
* signals,
* filesystem semantics,
* locking,
* executable discovery,
* line endings.

Do not hide platform incompatibilities behind generic error handling.

## 16. Documentation

Update documentation when the implementation changes a documented contract.

Do not produce large documentation rewrites for small implementation changes.

Keep documentation close to the behavior it describes.

If implementation reveals that an existing design document or plan is wrong, record the evidence and correct the current guidance as part of the task. Preserve historical ADRs; document a superseding decision when warranted. A stale plan is not, by itself, a reason to stop implementation.

ADRs describe decisions. New decisions should normally produce new decision records rather than altering the historical record of an old decision.

## 17. Repository Hygiene

Before completing a task:

* inspect the diff,
* remove accidental debug code,
* remove temporary files,
* verify no secrets were introduced,
* verify unrelated files were not changed,
* format affected code,
* run the appropriate checks.

Do not generate large artifacts or vendor files into the repository unless required.

## 18. Resolve First; Escalate When a User Decision Is Necessary

Investigate and resolve ordinary engineering obstacles autonomously. These include missing prerequisites, incomplete internal interfaces, stale plan details, failing tests, merge conflicts, platform differences, and upstream behavior that can be adapted within VCP's contracts. Use bounded experiments and targeted tests, document the resolution, and continue delivery.

Ask the user only when proceeding requires:

* choosing between materially different product outcomes that the request and repository do not resolve,
* weakening a security or trust boundary, or changing the intended permission model,
* destructive or irreversible action affecting user data, unrelated work, or an external system beyond the authorized delivery workflow,
* a breaking public compatibility decision or broad product redesign beyond the requested outcome and necessary prerequisites,
* credentials, access, budget, or other external input that cannot be obtained through an already authorized mechanism,
* resolving acceptance requirements that remain mutually incompatible after checking their authority and feasible implementations.

Before asking, complete independent work and narrow the unresolved decision. Explain the evidence, recommend a concrete option, and identify exactly what depends on the answer. Pause only the dependent action. Do not repeatedly ask for authorization already given, and do not treat uncertainty, implementation difficulty, or a growing but necessary prerequisite chain as an automatic approval gate.

If a required environment or external check remains unavailable, report the exact limitation and preserve the reviewable work. Do not silently waive acceptance criteria or merge with required checks failing or pending.

## 19. Completion Standard

A work item or increment is complete when:

* requested behavior is implemented,
* relevant acceptance conditions are satisfied,
* affected tests pass,
* formatting and static checks appropriate to the change pass,
* no unrelated behavior was intentionally altered,
* documentation is updated where required,
* the diff contains no accidental changes.

After these conditions are satisfied, finish the authorized delivery workflow. For a broader plan request, proceed to the next ready item; for a prerequisite, return to the original item. Do not continue polishing completed work without a concrete reason.

## 20. Final Report

At completion, report only useful implementation evidence.

Include:

1. **Work item**

   * task ID and implemented increment.

2. **Changed**

   * significant files or components changed.

3. **Verified**

   * tests and checks actually run,
   * their results.

4. **Remaining**

   * incomplete acceptance conditions, if any.

5. **Issues**

   * architecture conflicts, assumptions, risks, or follow-up work discovered.

Do not provide a long narrative of every step taken.

## Core Principle

VCP's plans, architecture, source code, tests, and git history are the project's memory.

Use them.

Read what the current task and its dependencies require, resolve ordinary obstacles, make the smallest complete change, prove it with targeted evidence, and carry the authorized scope through delivery. Interrupt the user for decisions only they can make, and always honor an explicit stop for review.
