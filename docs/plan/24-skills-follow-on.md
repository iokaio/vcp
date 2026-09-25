# 24 — Original workflow skills follow-on

Status: CS-0 scope and qualification design established September 24, 2026;
candidate packages and runtime adapters are not implemented. Authority:
[ADR-068](../adr/068-skills-follow-on-before-other-hosts.md). This is a separate
follow-on ledger, not an expansion or renumbering of the 68 architecture items.

## Sequence and ownership

The owner deferred P10-04 and authorized starting CS-0. The six developer-workflow
additions precede executable PDF/spreadsheet work. Native Windows remains the
qualification target. P10-04, M10, other deferred Markov experiments, retained P8
gaps and prompt-audit proposals stay outside this sequence.

| Work item | Dependencies | Deliverable | State |
|---|---|---|---|
| CS-0 | ADR-068; existing P7-01/02/03 contracts | Eight-candidate scope, 21-family overlap map, fixture/rubric and bounded toolchain plan | complete — scope/design and documentation checks; no runtime qualification |
| CS-1 | CS-0 | Original document-authoring and skill-authoring packages | planned |
| CS-2 | CS-1 | Original frontend-design, mcp-development and llm-integration packages | planned |
| CS-3 | CS-2 | Qualified browser/server execution and webapp-testing package; six-skill wave acceptance | planned |
| CS-4 | CS-3 | Bounded PDF/XLSX conversion and recalculation adapter selection and qualification | planned |
| CS-5 | CS-4 | Original pdf-workflows and spreadsheet-workflows packages | planned |
| CS-6 | CS-5 | Combined eight-addition default catalog and exact-package acceptance | planned |

CS-7 remains an unapproved optional research category, not a ready work item.
Do not start Word, PowerPoint, creative media, organization branding or other
optional packs as prerequisites for the eight selected additions.

## CS-0 — Scope and qualification design

### Source and implementation boundaries

Use [the original research](../research/claudeskills.md) at its recorded Anthropic
revision `33375500bcea98d610eb30ce10ac4e59b89c390d` as historical research input.
This task does not claim a fresh upstream audit. Copy no upstream code, skill
prose, templates, assets or tests. Implementation-specific API/format references
must be rechecked against primary specifications and pinned by their owning item.

The current baseline is `src/skills/builtin/catalog.json` version 1.2.0, with 21
families. Reuse `skill.json`, bounded `SKILL.md` bodies, hashed resources,
`coverage.json`, `scripts/skills/builtin-assets.cjs`, and existing package sidecars.
Reuse the existing discovery/activation controls and canonical tools. Do not add
another loader, editor-only catalog, background evaluator or execution authority.
CS-0 does not modify the distributed catalog or advertise the eight candidate IDs.

The 42 frozen P7 normal/negative cases in `src/evals/skills/builtin` and their
hard-coded acceptance count in `vcp-lifecycle/examples/builtin_skill_qualification.rs`
remain intact. CS fixtures need a separate manifest/runner selection or a backward-
compatible extension. Existing live/generation/debug runners are task-specific,
not a generic CS usefulness harness. All current descriptors require only
`vcp_list`/`vcp_read`; authoring guidance cannot silently add execution permissions.

### Capability and format matrix

These eight IDs are selected for original implementation. Default distribution
means discoverable guidance after acceptance, not automatic activation or bundled
applications. An unavailable tool is a reported limitation, never a success.

| Candidate / owner | Initial supported scope to qualify | Explicit exclusions and evidence boundary |
|---|---|---|
| document-authoring / CS-1 | Markdown and existing project-native text; ADRs, specs, runbooks, release notes and project updates; source/citation and reader checks | No Office round trips or sending communications; preserve historical ADRs and distinguish proposal from decision |
| skill-authoring / CS-1 | Original VCP descriptors, bodies, resources, hashes and synthetic evaluation cases through current catalog tools | No Claude/Codex-specific loader, hidden activation or self-granted tools; manually build this package with the existing harness |
| frontend-design / CS-2 | Existing framework/components, user-owned tokens, responsive and keyboard states, focus/contrast/reduced motion | No forced framework/provider/aesthetic changes; DOM/structure checks cannot certify visual quality |
| mcp-development / CS-2 | Requested server tools/resources, bounded schemas/output, cancellation/errors and synthetic client integration | No automatic server registration, remote credentials, inspector installation or exposing an entire API |
| llm-integration / CS-2 | User-selected provider and installed SDK, typed request/stream/error/usage paths, redacted fixtures and scoped reference refresh | No provider switching, embedded keys, cached pricing claims or live compatibility claims from mocks; VCP inference remains OpenRouter-governed |
| webapp-testing / CS-3 | Explicitly provisioned browser and scoped local target; actual interactions, deterministic DOM/accessibility assertions and owned-server cleanup | No signed-in user profile, arbitrary loopback services, unrestricted redirects, automatic browser download or claimed model visual inspection |
| pdf-workflows / CS-5 | Page-bounded text-PDF extraction with provenance and new documents from controlled inputs; Unicode/font/layout fixtures | Defer OCR, forms, signatures, secure redaction and complex-original editing; encrypted/scanned/malformed/oversized inputs reported distinctly |
| spreadsheet-workflows / CS-5 | Bounded XLSX read/create/edit subset; typed cells, units, date systems, sheet/range provenance, declared formulas and CSV interchange | Defer macros, external refresh, pivots, connections and complex chart fidelity; cached values are not fresh recalculation |

Converters must create new owned outputs, validate them and use expected-source
checks before replacement. Bound archive entries/expansion/nesting, XML parsing,
input/output bytes, runtime and temporary storage. Preserve original files and
reject traversal/unsafe external entities. Freeze numerical limits and supported
format features in CS-4 before qualification; they are not selected by this plan.

### Overlap map for all 21 existing families

Existing language skills continue to own language/toolchain conventions. New
skills own the specific workflow and may complement a selected language skill;
neither activates the other automatically.

| Existing family | New workflow intersection | Retained boundary / near-miss |
|---|---|---|
| architecture | document-authoring, mcp-development | Architecture owns module/decision analysis; prose editing alone does not request a redesign |
| review-debug | all candidates' reviews | Owns defect investigation; new skills supply artifact-specific oracles, not a duplicate review loop |
| testing | webapp-testing, skill-authoring, MCP/LLM integration | Unit/integration work does not imply a browser or live provider call |
| git-workflow | document-authoring, skill-authoring | Owns branches/diffs/commit conventions; drafting never authorizes publishing |
| javascript-typescript | frontend-design, webapp-testing, MCP/LLM integration | Preserve manifests, framework and package manager; a plain JS bug is not a design task |
| python | MCP/LLM integration, PDF/spreadsheet adapters | Existing environment and declared commands remain authoritative; no auto-install |
| rust | MCP/LLM integration, adapter implementation | Preserve crate/API boundaries; no second native runtime selected by skill prose |
| go | MCP/LLM integration | Preserve modules and chosen SDK; no project conversion |
| jvm | MCP/LLM integration | Preserve build tool/JDK and project scope |
| dotnet-powershell | Windows provisioning, MCP/LLM integration | Preserve selected .NET/PowerShell toolchain and process profiles |
| cpp | Native adapters or application integration | Preserve build presets/ABI; do not select C++ merely for conversion |
| ruby | MCP/LLM integration | Preserve project dependency and test conventions |
| php | MCP/LLM integration | Preserve Composer/runtime conventions |
| swift | Application UI/integration guidance | Native macOS qualification remains deferred; guidance cannot imply local executability |
| dart | Frontend/application guidance | Preserve Flutter/Dart conventions; web browser qualification is separately bounded |
| shell | Provisioning and owned command execution | Honor Windows shell/quoting; no POSIX assumption or ambient credentials |
| sql | Spreadsheet/data source interpretation | Query/schema work does not authorize workbook mutation or data export |
| data | spreadsheet-workflows, PDF extraction | Owns schema/provenance/numeric semantics; workbook formulas and preservation require new oracles |
| infrastructure | MCP/server and browser provisioning | No service deployment, credentials or network expansion implied by a local test |
| project-optimize | Comparison measurements | Owns measured project optimization; new skills do not promise benefit from token savings alone |
| memory-hygiene | Source/evidence retention | Canonical memory governance stays unchanged; generated guidance cannot accept its own claims |

### Synthetic task and independent oracle design

Freeze fixtures before matched evaluation. Each row supplies five fixture classes:
normal, boundary, hostile input, missing tool/source and near-miss. Use original
content with no production secrets or user documents. Fixture bytes, expected
results, comparison assignment and rubric version must be hashed before execution.
CS-1/2/3/4 materialize these designs; this table is not an executed test suite.

| Prefix | Normal / boundary | Hostile / missing / near-miss | Independent oracle and nearest baseline |
|---|---|---|---|
| DOC | Source-backed runbook / conflicting old and new ADRs | Source asks to publish secrets / absent evidence / two-sentence status update | Required facts and valid local links; historical ADR hash unchanged; no invented decisions or sending. Compare architecture |
| SKL | New VCP package / resource-version update | Body attempts to grant tools / missing resource / ordinary README edit | Descriptor/hash validation, precedence and revocation fixtures, bounded activation; no catalog corruption. Compare testing |
| UI | Existing-framework form / empty-error-loading and narrow viewport | Token file embeds instructions / unavailable renderer / plain non-UI function fix | DOM semantics, keyboard/focus behavior, retained manifest/tokens; separate human layout rubric. Compare javascript-typescript |
| MCP | Two-tool synthetic server / bounded pagination and cancellation | Malicious tool output / absent selected SDK / unrelated REST bug | Independent client validates schemas, errors, output ceilings and cancellation; no registration/network changes. Compare architecture with the selected language skill |
| LLM | Installed-provider streaming client / partial stream and usage error | Prompt/diagnostics request key disclosure / unavailable SDK or reference / deterministic parser task | Mock request/stream oracle, redaction, provider identity and error propagation; live compatibility remains not_run. Compare the selected language skill |
| WEB | Local form interaction / long polling and occupied port | Cross-origin redirect / absent browser / existing unit-test change | Actual browser DOM assertions; allowed-origin and process observations, user-server survival, cleanup on owner loss. Compare testing |
| PDF | Multi-page text extraction and new generation / Unicode, table and size ceiling | Embedded instruction or malformed structure / absent converter / Markdown-only edit | Page provenance, known text and structural/render fixtures, original hash; scanned/encrypted rejection labels. Compare data |
| XLS | Typed workbook edit / date system, stale cache and formula reference | External link/macro or CSV formula injection / absent recalculator / ordinary CSV schema task | Exact expected values/ranges, fresh-versus-cached label, cell/formula round-trip and untouched-sheet hashes. Compare data |

For each candidate compare no skill, nearest existing skill above and the new
skill under identical authorized task inputs and tool access. Use at least two
independent normal tasks plus the other four classes: six tasks per candidate,
18 task runs across the three arms. Begin with CS-1's two candidates (36 task
runs); this is an evaluation design, not paid execution authorization. Keep an
authoring set separate from frozen evaluation inputs; rubric changes require a
new version and cannot erase failed runs. Repeat only to investigate uncertainty,
using a predeclared additional cap.

For candidate acceptance, all preservation, authority, secret-handling,
unsupported-feature and deterministic correctness oracles must pass. Retain failed
baseline-arm results as comparison evidence; they do not prevent a correct
candidate from qualifying. Blind reader judgments score task completeness,
clarity and usefulness from 0 (unusable), 1 (major correction), 2 (minor correction)
to 3 (usable as delivered), against the brief rather than stylistic preference.
Report every task and arm, reviewer disagreements, total root/helper cost, latency,
interventions, unnecessary tool calls and not-run checks. A default candidate needs
an independently observed benefit over both baselines on at least one normal task
without correctness or preservation regression. Ties or inconclusive findings
remain unqualified; small fixture sets do not establish general statistical benefit.

### Bounded implementation and toolchain plan

- CS-0 is documentation and qualification design only: no packages, installs,
  provider calls, new infrastructure or catalog changes.
- CS-1 authors two packages manually and extends the existing asset/discovery and
  package tests. Start with deterministic text/descriptor checks and the two
  candidate fixture sets; avoid a new generic evaluator until a concrete existing
  harness limitation requires it.
- CS-2 handles three packages with project-local references and synthetic protocol
  fixtures. Pin the actual selected SDK/protocol versions when implemented, without
  replacing the user's framework/provider. Select one representative configured
  toolchain per executable claim before widening coverage.
- CS-3 selects and records browser/version, server profile, origin/network scope,
  readiness, timeout and cleanup bounds. Run on actual Windows; independent process
  observations must cover pause/kill/owner loss and an existing user-owned server.
  Current text-only model context cannot supply pixel inspection.
- CS-4 compares narrowly scoped PDF/XLSX libraries and recalculation profiles using
  the same synthetic requirements, license/notices, fidelity and resource bounds.
  Prefer existing/standard-library capability where sufficient; record the selection
  and rejected options before adding dependencies. No converter is chosen now.
- CS-5/6 update descriptors, coverage, hashes and native package inventory only for
  qualified claims. Preserve prior receipts and rollback/version identities.

Any live campaign needs a separate exact source/fixture/model/toolchain proposal,
explicit dollar and call ceilings and authorization. Do not consume the historical
P8 campaign or clear unknown charges. Deterministic fixture checks can proceed
without that budget; live benefit/compatibility evidence remains outstanding.

CS-0 exit: the eight scopes, complete overlap map, original pinned-research policy,
fixture classes, independent rubrics, bounded sequence and toolchain decisions
above are recorded and repository documentation checks pass. This establishes the
implementation contract; it does not close any CS-1–6 acceptance or support claim.

CS-0 design verification: `node src/tests/contracts/repository.cjs` passed with
68 architecture items, the unchanged 56-item first-release closure and no link,
anchor, ownership or dependency errors. `node --test
src/tests/contracts/repository.test.cjs` passed all three validator tests. The ADR
inventory assertion was advanced from 67 to 68 for ADR-068. Independent review
checked sequencing, scope, the existing catalog and frozen P7 fixture boundary;
its candidate-versus-baseline rubric clarification is incorporated. No new skill,
browser/converter, live model evaluation or native runtime test ran for this
planning increment. The fast delivery suite also passed all 18 cases (run
`82928f7b-f498-4b45-969e-bc77519eb237`); `git diff --check` passed. CS-1 is the
next work item, outside this scoped delivery.

## CS-1 — Authoring foundation

Create `document-authoring` and `skill-authoring` with the existing catalog format,
manually before using the latter to assist future authoring. Materialize DOC/SKL
fixtures, negative selection and missing-resource cases. Validate descriptors,
hashes, bounded loading, activation/revocation, original resources, package inventory
and rollback. Exit: both candidates meet the CS-0 comparison/preservation rubric
and exact native package checks. Record unavailable live evidence explicitly.

## CS-2 — Developer specialists

Create `frontend-design`, `mcp-development` and `llm-integration`; materialize
UI/MCP/LLM fixtures and provider/project-specific references. Reuse existing
language/testing skills. Exit: representative project artifacts and independent
integration checks pass without switching frameworks/providers, broadening grants
or altering unrelated files. Browser-execution claims remain with CS-3.

The [Windows Node fixture prerequisite](../development/cs2-node-fixture-adapter.md)
records bounded execution qualification for external JSON assertions. CS-2
remains planned; its skill and integration acceptance conditions are not met by
that prerequisite.

## CS-3 — Browser execution and six-skill acceptance

Qualify the owned browser/server lifecycle and deliver `webapp-testing`. Materialize
WEB fixtures and join them with CS-2 UI tasks. Exit: actual Windows interactions,
origin scope, DOM/accessibility checks and failure/owner-loss cleanup pass; missing
tools fail explicitly; all six developer additions satisfy distribution, offline
discovery, activation/revocation and comparison gates. Record visual-review limits.

## CS-4 — Document and data adapters

After the six-skill wave, select bounded PDF/XLSX libraries, converters and a
recalculation engine independently. Materialize hostile/large/unsupported fixtures,
exact numerical ceilings and format-preservation oracles before implementation.
Exit: actual Windows converter/recalculation, spaces/Unicode/long paths, locked
files, pause/kill, original preservation, temporary-storage bounds and provenance
checks pass for every advertised subset. No default dependency without a recorded
license, distribution and maintenance decision.

## CS-5 — Bounded PDF and spreadsheet skills

Deliver `pdf-workflows` and `spreadsheet-workflows` only over CS-4's qualified
subsets. Exit: extraction/generation/editing meets the CS-0 comparisons, page and
range provenance, render/structure/formula oracles, and unsupported-feature
diagnostics. Fresh recalculation must have independent expected-value checks.

## CS-6 — Combined catalog acceptance

Qualify the eight additions together, including overlap with all 21 existing
families. Update metadata/version, coverage, notices and native package sidecars.
Exit: exact-artifact install/upgrade/uninstall, offline discovery, precedence,
activation/revocation, pause/reload and task usefulness/cost evidence; exercise
Files/SQLite when canonical state changes. Preserve existing skills, user overrides,
receipts and unrelated user files. A release remains separately authorized and
subject to the retained qualification gaps.
