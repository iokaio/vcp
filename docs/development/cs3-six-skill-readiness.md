# CS-3 six-skill readiness and paid-proposal inputs

Status: preparation only, September 27, 2026. No paid call, candidate change,
fixture change, replay, promotion or six-skill acceptance is authorized or
recorded here. This inventory supports phase 4 of the
[CS-3 implementation plan](cs3-implementation-phases.md#phase-4--six-skill-comparison-acceptance)
and preserves every historical result and halt.

## Current immutable candidate identities

These are the bytes currently present under `src/skills/candidates/`. A future
source correction needs a new version and hashes. An unchanged candidate may be
tested only against prospectively frozen inputs; known cases cannot be renamed
into untouched holdouts.

| Candidate | Version | Descriptor SHA-256 | Body SHA-256 | Current disposition |
|---|---:|---|---|---|
| `document-authoring` | 1.0.2 | `b3d2a913f02eb3018091d75c2a6369245ea8d04cfbe93203a65e8b136cc12864` | `3d61f9c8831f091775b8d1fc120b77c18c3478309dba78971d63f1b2b2fb3e08` | Fresh normal comparison failed correctness and benefit; terminal and non-default. |
| `skill-authoring` | 1.0.1 | `23911824fc90390faded6a2bad009b1450130d52c9cb7fe5c5f7b565c97d78a7` | `d49fce122d7151b9356b353561fc994a5c2cf322c828f8c8eb877d990427f37a` | Fresh comparison halted before completion; non-default. |
| `frontend-design` | 1.0.0 | `3a90900bb03b65cd552e9f6a1378a0fe1699f6992132fe93b4cbad044e3f785a` | `1a5a604c172627cd264edd1790fa57585c21670021789c934d8eff714410643e` | Candidate gates failed, no normal benefit, browser checks not run; non-default. |
| `mcp-development` | 1.0.0 | `a9a3cc11a86cc484d73944a0944c1bf54a4dbaa2f8e7d7776d3c8d60a8865ef8` | `f51f053ed54e3f7daf438700f8b89e2dbdb2ea3d861f755d5d23df141c11e604` | Candidate gates failed and no normal benefit; non-default. |
| `llm-integration` | 1.0.0 | `e507a719ead01acd48800d4e07b82d6b3c586e929da0b186d6c07d9fa46b2f15` | `8ebd54a7cfc6c0f94fb75468db9213edf9eb67bf5f6691f39741d7201e296457` | Hard gates passed but no normal benefit; non-default. |
| `webapp-testing` | 1.0.0 | `02c1a6f89eddc1270a55f7ea6040fdfa9b21eb5159527d4f96cd42bf3264b19f` | `1480689dfe61a022ecc4878f84300304c968985521170d626b49e0ee76c29e59` | Offline package preparation only; no comparison or browser qualification. |

The authoritative historical dispositions are the
[DOC result](cs1-fresh-doc-disposition.md),
[SKL record](cs1-skill-authoring.md),
[frontend result](cs2-frontend-disposition.md),
[MCP result](cs2-mcp-disposition.md), and
[LLM result](cs2-llm-disposition.md). Owner acceptance closed CS-1 and the
per-skill CS-2 dispositions; it did not satisfy the CS-3 comparison gate.

## Exact implementation gaps before freezing a campaign

Implementation readiness is separate from comparison success. Completing an
item below makes a candidate testable; it does not demonstrate benefit.

| Candidate | Retained defect or missing evidence | Prospective owner paths |
|---|---|---|
| `document-authoring` | The 1.0.2 candidate omitted mandatory source facts in both fresh normal cases. The format result also reported bytes differing from the actual file. Both candidate cases failed correctness and no benefit was established. | Correct and version `src/skills/candidates/document-authoring/SKILL.md` and `skill.json`; add new untouched tasks and independent oracles in a new evaluation revision beside, not inside, the retained `src/evals/skills/authoring-qualification/` history. Reuse the authoring harness only through a new frozen manifest/envelope identity. |
| `skill-authoring` | The old 1.0.0 boundary answer omitted required assertions and its maintenance task tied. Version 1.0.1 includes prospective guidance and package checks, but its fresh normal/boundary/hostile/missing/near-miss comparison never completed after canonical inspection failed. No additional prose defect is established for 1.0.1 merely by that halt. | Audit the existing `src/skills/candidates/skill-authoring/` bytes without changing them gratuitously. Freeze new inputs outside the retained authoring fixture revisions and use new claims/results; correct the canonical inspection path in a successor to `scripts/evals/authoring-qualification.cjs` only if current tests show it remains defective. |
| `frontend-design` | One near-miss run never completed required verification after a model tool-batching rejection. Normal candidates tied their baselines. DOM interaction, accessibility, viewport, reduced-motion and human layout evidence remain absent. | Regrade retained UI artifacts through the qualified CS-3 browser oracle without altering them. Any evidence-based guidance correction belongs in `src/skills/candidates/frontend-design/`; new UI holdouts and browser oracles need a new revision rather than edits to `src/evals/skills/developer/`. |
| `mcp-development` | Candidate outputs accepted extra call parameters, bypassed initialization, mishandled notifications/malformed cancellation, and had one incomplete verification caused by a mistyped evidence identity. Existing functional grading missed several protocol defects. | Add only justified state/schema/notification guidance in a new `src/skills/candidates/mcp-development/` version. Freeze expanded independent-client probes in a new developer-fixture revision and update a successor grader so the retained coverage gaps cannot pass unnoticed. |
| `llm-integration` | All candidate hard gates passed, but neither normal task beat both baselines. Readers noted missing requested JSDoc types and iterator-cleanup shortcomings in some generated implementations; these are output findings, not proof of a package-body correctness defect. | Decide from new authoring evidence whether `src/skills/candidates/llm-integration/` needs a versioned clarification. In either case use untouched normal tasks and a new frozen developer manifest; do not edit a candidate solely to manufacture a new identity. |
| `webapp-testing` | The descriptor/body and separate six-case WEB preparation cohort pass offline integrity tests. Executable browser oracles, retained-UI regrade and comparison remain absent. The body correctly reports the executable adapter unavailable. | Keep package ownership in `src/skills/candidates/webapp-testing/` and offline selection in `scripts/evals/webapp-candidate.cjs`. Bind `scripts/evals/fixtures/webapp/` to a qualified executable interface in a new prospective campaign; the preparation inventory is not an executable or paid-campaign approval. Browser/server qualification remains owned by the CS-3 native and server controls, not by skill prose. |

Before paid preparation, all six exact candidate versions must also pass offline
descriptor/content integrity, negative selection, baseline selection and
preservation checks. Browser feasibility, the frontend regrade and WEB oracle
controls must have exact receipts. None of those checks needs model spending.

The Windows work now has exact production-profile synthetic DOM and owned-server
evidence recorded in the [native DOM checkpoint](cs3-native-dom-checkpoint.md).
The browser ran without additional browser arguments and produced deterministic
interaction, accessibility and denied-origin receipts in the isolated worker.
After that worker became unavailable, the same seven-document oracle completed
twice on fresh current-host profiles under WebView2 `154.0.4258.37`, with an
intercepted Shift readiness sentinel and exact 8/8 process coverage. The bounded
server independently passed all ten Windows lifecycle and adversarial contract
tests with every guest network adapter disconnected. These are prerequisite
controls, not full browser qualification: the zero-network-capability browser
has not been joined to the owned server, and the browser cancellation, pause,
owner-loss, adversarial network/filesystem and WEB-cohort matrices remain open.
The retained frontend artifacts have not been regraded through that interface.

## Comparison gate

Each candidate must use the CS-0 three-arm design: two normal tasks plus boundary,
hostile-input, missing-prerequisite and near-miss tasks, each run with no skill,
the declared nearest skill and the candidate. Thus the complete six-skill wave is:

- `6 candidates × 6 tasks × 3 arms = 108` model task runs;
- `18` task runs per candidate;
- all candidate correctness, preservation, authority, secret-handling,
  unsupported-feature and evidence-honesty gates passing; and
- both independent blinded readers observing benefit over both baselines on the
  same normal task, without lower completeness or clarity or a correctness or
  preservation regression.

The retained nearest arms are `architecture` for document authoring, `testing`
for skill authoring, `javascript-typescript` for frontend design and LLM
integration, `architecture` plus the selected JavaScript language skill for the
existing MCP fixtures, and `testing` for webapp testing. A new fixture written in
another language must freeze its corresponding selected language skill rather
than silently carrying forward JavaScript.

Ties, reader disagreement, incomplete verification and `not_run` required checks
remain unqualified. Deterministic package/browser success cannot substitute for
the comparison, and comparison success cannot substitute for exact native
package install, upgrade, rollback, offline discovery, activation/revocation and
integrity qualification.

## Inputs required for a paid authorization proposal

Preparation must produce one reviewable, hash-bound proposal containing:

1. the six exact candidate package inventories, versions and source hashes;
2. a new fixture manifest, projects, task prompts, private independent oracles,
   three-arm assignment and rubric version, all frozen before dispatch;
3. the exact VCP executable/build receipt, builtin and candidate catalogs,
   checker/grader/browser receipts, Node/native toolchain identities and OS;
4. the exact provider, model, endpoint policy, privacy settings, provider catalog
   and current qualification/expiry evidence;
5. the current authoritative input, cached-input, output, request and other
   applicable price categories, their effective date and source identity;
6. per-run input/context/output byte and token ceilings, maximum root/helper
   requests, deadline and process/output limits;
7. aggregate dollar and request ceilings, atomic reservation/settlement rules,
   stop-on-unknown-charge behavior and exclusive campaign/slot claims;
8. explicit treatment of provider refresh probes, retries, confirmations,
   graders, reviewers and adjudication. Anything not listed is unauthorized;
9. private output locations outside the repository/synchronization roots,
   redaction rules, retained failure evidence and the zero-active/unresolved
   liability precondition; and
10. conditional phase order and stop rules so an early failed skill does not
    silently spend later allocations or transfer unused slots.

The exact model/provider/catalog and current price snapshot are **not available
in the repository**, so an exact dollar request cannot be calculated here without
guessing. Historical campaigns used per-run ceilings of 16 requests and, in
different envelopes, USD 3 or USD 1.75. Those are retained controls and costs,
not current prices or transferable authorization.

If the owner deliberately selects the former 16-request ceiling for the new
proposal, the arithmetic ceiling is `108 × 16 = 1,728` root/helper requests.
This is a proposed maximum, not an estimate or authorization. Let `C` be the new
approved per-run dollar cap derived from the selected current price snapshot and
token/request limits; the base campaign ceiling is then `108 × C`, plus only
separately enumerated refresh, reviewer or confirmation allowances. Until `C`
and every extra category are frozen, the aggregate dollar ceiling is
**unavailable**. Historical unused headroom and settled costs contribute zero
authorization to this proposal.

## Readiness decision

Offline preparation may continue, but paid phase 4 is not ready for authorization.
Candidate corrections/audits, new untouched inputs, the remaining joined-browser
and WEB evidence, the exact execution/toolchain bundle, provider/model selection
and a current price snapshot are still missing. A prepared envelope must say
`authorization: false` and `model_calls: 0` until the owner approves its exact
hash, initial phase, dollar ceiling, request ceiling and process authority.
