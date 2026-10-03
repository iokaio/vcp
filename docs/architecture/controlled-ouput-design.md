# Clear Output for VCP

October 2, 2026 · Proposed plan

## Outcome

Make VCP's answers easier to read and its verification state easier to understand. A user should quickly see the result, the important changes, what was actually checked, and anything still unresolved.

Deliver this as a small improvement to existing presentation and prompt code. VCP already owns verification, completion, history, and delegation. This plan makes those facts visible and gives the model simple writing guidance.

## Use the existing foundation

| Existing code or contract | Responsibility to retain |
| --- | --- |
| [Verification types](../../src/crates/vcp-domain/src/verification.rs) | Recorded checks, outcomes, scope, steering revision, fingerprints, and evidence references. |
| [P2-06 completion contract](../plan/05-openrouter-and-session-loop.md#p2-06--verification-and-honest-completion) | Applicable required checks, invalidation after changes, and the controller's completion decision. |
| [CLI outcome selection](../../src/crates/vcp-cli/src/outcome.rs) | Task disposition and exit conditions from canonical state. |
| [Terminal presentation](../../src/crates/vcp-cli/src/terminal.rs) | Bounded summaries of current verification and navigation to details. |
| [Evidence inspectors](../plan/07-cli-and-inspection.md#p3-03--evidence-inspectors) | Authorized access to full records and artifacts. |

The terminal currently selects applicable verification records and displays at most eight checks from the selected report. That provides a concrete starting point: account for omitted checks and make failures or missing current evidence visible. Inspect nearby tests and final-output callers before choosing the exact edit.

Keep check selection and completion rules in their existing owners. A renderer may summarize those decisions; it must not implement a second completion evaluator.

## What the user sees

Model-written text explains the work in ordinary language. A clearly labeled VCP summary presents recorded verification facts. For example:

```text
Updated configuration loading to handle the missing-file case.
Added a regression test for that behavior.

VCP checks
- unit tests: passed
- integration tests: not_run — required fixture unavailable

Task: incomplete
Remaining: run integration tests when the fixture is available.
```

This example assumes the engine recorded those outcomes and the incomplete task state. Actual summaries include the existing evidence links or references. The renderer must not infer the task state from this wording.

Keep the default view compact. Show failures and unresolved verification first, summarize passing checks when necessary, and expose full details through existing navigation. Always state how many checks were omitted. Concision must not hide an incomplete required check.

For explanations and analysis-only tasks, follow the existing proportionate verification contract. Do not invent a requirement to run application tests merely to fill a report template.

## Rules for verification presentation

Reuse `CheckOutcome::Passed`, `Failed`, and `NotRun`. Keep freshness and evidence availability separate from execution outcome.

| Evidence available to the existing owner | Display behavior |
| --- | --- |
| Current, applicable passed result | Show passed with its evidence reference. |
| Current, applicable failed result | Show failed with the recorded reason and evidence reference. |
| An explicit not-run result | Show not_run with the recorded reason. |
| Only an older result that no longer applies | Say current verification is needed. Identify an accessible older outcome as historical/stale in details. |
| No applicable result for a required check | Say current evidence is missing. Absence of a record does not prove the check never ran. |
| Restricted, redacted, malformed, or unresolved evidence | Use the existing safe unavailable/error handling. Do not expose restricted content or infer success. |

Apply these rules through the existing canonical snapshot and evidence access path:

- Use the task's required-check set and existing record selection. Do not assemble a synthetic pass from unrelated checks or older attempts.
- Account for every required check, either visibly or through an explicit omitted count and detail route. Preserve failures when a later narrower run does not satisfy the same requirement.
- Report task state separately. Passing checks alone do not resolve outstanding effects, child tasks, issues, or other completion conditions.
- Associate the summary with its observed task revision. A historical summary describes that observation; continuation must read current state.
- Preserve current access controls, terminal sanitization, size limits, and artifact references. Retain original records through the existing history path.

A model's statement that everything passed remains narration. It cannot change check outcomes, task completion, or exit status. This plan does not promise to detect every false claim in free text.

## Simple writing guidance

Add or refine one short instruction in the existing provider-neutral prompt assembly path:

> Lead with the result. Explain the important changes and why they matter. Refer to recorded evidence when discussing checks. State uncertainty, missing verification, and useful next actions plainly. Prefer short paragraphs or a few bullets. Use existing project names. Omit filler and repeated summaries, while keeping details the user needs.

Reuse equivalent instructions already present instead of appending duplicates. Honor the user's requested format and level of detail. Progress updates should communicate a meaningful finding, decision, or blocker; they do not need the final-report structure.

Treat this as guidance. Do not reject an answer because it has a long sentence, uses a synonym, or expresses uncertainty. Preserve code, paths, technical identifiers, quoted output, and user terminology. Leave reasoning outside this feature.

Keep original model output in existing capture/history. A displayed verification summary supplements it without silently rewriting it. Prompt capture already records the instructions used; no separate fragment registry is needed.

## Implementation work item

**Proposed follow-up: CO-01 — Clear output using existing verification.** Before implementation, register the item in the [CLI/inspection plan](../plan/07-cli-and-inspection.md) and traceability records using the repository's current ID convention. Reference the existing P2-06 and P3 contracts. This document proposes work; it does not mark that work complete.

Implement in three steps:

1. **Improve the verification view.** Inspect current terminal, final-output, and headless callers. Extend the existing projection/renderer with the missing presentation behavior above. Share a small helper only where callers need the same logic. Reuse existing fields and preserve machine-output compatibility.
2. **Tighten the prompt.** Add or refine the short guidance at the existing instruction boundary. Use canonical verification information already available in context. Keep current precedence, context limits, provider normalization, and request capture.
3. **Verify the connected result.** Exercise the changed presentation path using canonical records and a local or fake-provider task. Review examples of a successful change, a failed check, missing prerequisites, and a detailed explanation. Record actual limitations.

Prefer one focused PR. Split only if a concrete supporting change deserves separate review. Follow AGENTS.md for implementation and delivery. Create another ADR only if the implementation changes an architectural decision.

## Acceptance and testing

The work is complete when the following checks pass:

| Check | Expected result |
| --- | --- |
| Current pass, failure, explicit not-run, stale result, and missing evidence | Distinct, accurate presentations with appropriate reasons and references. |
| More checks than the display limit, with a failure beyond the original cutoff | Failure remains visible; omitted counts and full-detail navigation are correct. |
| Model text says all checks passed while canonical evidence disagrees | Recorded status, task disposition, and exit behavior remain unchanged by the text. |
| Task is restored or later edits invalidate a pass | Current view uses current applicability; old success is historical. |
| Evidence access is denied or content requires sanitization | Existing access and output restrictions remain effective. |
| Terminal and headless consumers observe the same snapshot | Verification facts agree without requiring identical formatting. |
| Prompt guidance and a connected local task | Guidance appears once at the right precedence; final output and evidence navigation work together. |

Extend nearby regression tests rather than creating a new evaluation framework. Keep existing child-integration protections: a child's pass does not verify the integrated parent. Exercise that boundary if the implementation changes its presentation path.

Run affected tests, formatting/static checks, and applicable documentation checks using repository commands. Include Windows path, Unicode, and line-ending cases where rendering changes. Broaden testing for shared-boundary changes or actual failures, not for a wording edit alone.

Review output quality with a few representative saved or synthetic examples. This can reveal missing caveats or unnecessary repetition; it cannot prove universal model behavior. A paid model comparison is optional only for a specific unresolved question and an authorized budget.

## Keep the maintenance cost small

The expected addition is a small presentation change and one short instruction. Use existing crates, dependencies, state, history, and configuration. Preserve streaming, pause/recovery, cost accounting, and permission behavior.

Exclude a standalone output-profile crate, new report/status schemas, prose parsers, grammar linting, term registries, automatic rewriting, repair calls, background work, and new user settings. Memory normalization and delegation contract redesign are outside this item. If implementation reveals a missing prerequisite, address the specific gap in its existing owner and keep the change attributable to this work.

Stop when the acceptance criteria are met. Revisit only an observed problem: confusing terminology can justify a prompt edit; hidden failures can justify better summary ordering; a real machine consumer can justify an additional existing projection field. Each follow-up needs its own concrete benefit.
