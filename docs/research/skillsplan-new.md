# Skills development: reuse, adapt, ship

Status: complete, September 29, 2026; delivery recorded in
[PR #210](https://github.com/iokaio/vcp/pull/210). Owner direction replaces
the original-only implementation rule and the universal comparative-evaluation
gate in [the previous plan](../plan/24-skills-follow-on.md).
Decision: [ADR-069](../adr/069-practical-skill-ports.md).
Follow-on: [skills upgrade plan](skills-upgrade-plan.md) (SU series).

## Goal and scope

Deliver the eight workflow skills selected by the
[original research](claudeskills.md): document authoring, skill authoring,
frontend design, MCP development, provider-neutral LLM integration, web testing,
PDF workflows, and spreadsheets. Preserve the existing 21 language and general
workflow families. Rewrite the six additions using reusable, appropriately
licensed upstream material; complete the two missing additions with permitted
implementations. Do not rewrite unrelated language skills merely to rename them
as ports. Optional creative, branding, Office, and presentation packs remain
outside the default eight; record their demand disposition at completion.

Use [anthropics/skills](https://github.com/anthropics/skills) as an additional
upstream alongside Codex and Munarium. Pin each import, inspect each skill's own
license and selected dependencies/assets, preserve license and attribution, and
record modifications. Apache-2.0 and other compatible open-source licenses are
eligible; GPL, LGPL, missing permission, and restrictive source-available terms
are excluded. A public GitHub directory is not sufficient permission to copy.

## Source selection

The first source revision is `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4`.
The [per-skill audit](../../src/third_party/components/anthropic-skills.json)
records all 19 skills, license paths/digests, and decisions. Recheck that record
when the pin or selected files change; do not infer asset rights from a skill's
top-level license. Selected ports carry their own license and source notes in
the distributed package.

| VCP skill | Reuse selection | VCP adaptation |
|---|---|---|
| document-authoring | Apache-2.0 `internal-comms` | Audience, purpose, concise project/status/incident writing; retain ADR/spec/runbook support. `doc-coauthoring` has no explicit license and is excluded. |
| skill-authoring | Apache-2.0 `skill-creator` | Reuse iterative authoring and validation; emit VCP descriptors and hashes, without a Claude CLI dependency or mandatory paid benchmark campaign. |
| frontend-design | Apache-2.0 `frontend-design` | Intentional design and critique within the project's framework/tokens, responsive states and accessibility. |
| mcp-development | Apache-2.0 `mcp-builder` | Reuse tool/schema/error design and relevant SDK references; retain scoped server exposure and host authority. |
| llm-integration | Apache-2.0 `claude-api` | Reuse integration patterns; isolate provider-specific references and preserve the user's selected provider/SDK. |
| webapp-testing | Apache-2.0 `webapp-testing` | Port browser reconnaissance and interaction examples to supported project tools. Do not copy unsafe shell/process ownership assumptions. |
| pdf-workflows | Permissive PDF libraries, independently authored VCP glue | Anthropic `pdf` is restrictive source-available and must not be copied. Support bounded extraction and creation; preserve inputs and report unsupported features. |
| spreadsheet-workflows | Permissive workbook libraries, independently authored VCP glue | Anthropic `xlsx` is restrictive source-available and must not be copied. Support typed cells, formulas, ranges and preservation checks; distinguish stored formulas/caches from recalculation. |

Anthropic `docx` and `pptx` are also excluded by their restrictive licenses.
Unselected Apache skills are eligible sources for future scoped work, not an
instruction to import their assets, dependencies, or optional products now.

## Development and acceptance

Skills are instructions and optional resources consumed by the existing VCP
loader. Keep `skill.json`, source precedence, explicit activation/revocation,
hash checking, resource limits, and canonical execution tools. An executable
resource is never run by discovery or activation. Keep provider integrations
behind existing boundaries. Availability in the default catalog means the
guidance is available; it does not certify every host toolchain or grant tools.

For each skill:

1. Inspect the original goal, current package, upstream source and license.
2. Port the useful implementation, retaining a small source/modification record.
3. Exercise a representative supported workflow and relevant failure case.
   Run added helper code on real synthetic inputs, with preservation checks where
   it edits artifacts. Check missing tools and unsupported inputs honestly.
4. Validate descriptor/resource hashes, discovery, activation, precedence and
   package installation using the existing checks. Test a changed security
   boundary directly; do not rebuild an execution sandbox inside skill prose.
5. Review the diff, update coverage and notices, run applicable repository gates,
   then deliver one PR for this skill. Merge only after required checks/reviews
   pass. Skills need not wait for all other skills to qualify.

No blanket no-skill/nearest-skill/new-skill comparison, fixed 108-run campaign,
statistical superiority threshold, fresh allocation manifest, frozen cohort,
or Munarium memory-admission proof is required to ship a skill. These can be
used for a specific regression or research question when worthwhile. Paid model
experiments still require an established budget. Keep useful historical failures
and test fixtures; do not relabel prior failures as passes or apply new criteria
retroactively to their recorded outcomes.

Retain ordinary correctness, truthful reporting, secret handling, path and
process restrictions, package integrity, user-data preservation, and required CI.
This decision changes skill development/release gates; it does not weaken
Munarium's durable-memory governance or VCP's runtime authorization. A missing
browser/converter is a concrete unavailable capability, not evidence of success.
Do not claim visual inspection from a DOM check or fresh recalculation from a
cached workbook value. Describe supported subsets and limitations in each skill.

## Work items and delivery ledger

| Item | Dependency | Deliverable | State |
|---|---|---|---|
| SP-01 | Owner direction | Update AGENTS/CLAUDE, this replacement plan, ADR, upstream registry and all-skill license audit | complete; [PR #200](https://github.com/iokaio/vcp/pull/200) merged |
| SP-02 | Existing Munarium import | Refresh selected core/store/datastore code to current suitable upstream improvements; retain patches, attribution and native adapter tests | complete; [PR #208](https://github.com/iokaio/vcp/pull/208) merged |
| SP-03 | SP-01 | Make catalog/package tests support incremental skill additions while retaining baseline-family and integrity assertions | complete; [PR #200](https://github.com/iokaio/vcp/pull/200) merged |
| SP-04 | SP-03 | Port and ship document-authoring | complete; [PR #201](https://github.com/iokaio/vcp/pull/201) merged |
| SP-05 | SP-03 | Port and ship skill-authoring | complete; [PR #202](https://github.com/iokaio/vcp/pull/202) merged |
| SP-06 | SP-03 | Port and ship frontend-design | complete; [PR #203](https://github.com/iokaio/vcp/pull/203) merged |
| SP-07 | SP-03 | Port and ship mcp-development | complete; [PR #204](https://github.com/iokaio/vcp/pull/204) merged |
| SP-08 | SP-03 | Port and ship llm-integration | complete; [PR #205](https://github.com/iokaio/vcp/pull/205) merged |
| SP-09 | SP-03 | Complete and ship pdf-workflows with licensed executable support | complete; [PR #207](https://github.com/iokaio/vcp/pull/207) merged |
| SP-10 | SP-03 | Complete and ship spreadsheet-workflows with licensed executable support | complete; [PR #209](https://github.com/iokaio/vcp/pull/209) merged |
| SP-11 | SP-03 | Port and ship webapp-testing using supported browser/process tools | complete; [PR #206](https://github.com/iokaio/vcp/pull/206) merged |
| SP-12 | SP-02, SP-04 through SP-11 | Verify combined default catalog/package and close optional-pack demand review | complete; [PR #210](https://github.com/iokaio/vcp/pull/210) |

SP-02 and skill development can proceed independently. Refresh Munarium for
improvements actually used by VCP, especially parser safety and embedded storage
behavior; do not import its server/provider stack or replace VCP's authority.
Keep the Munarium refresh in its own PR. SP-01/SP-03 are shared prerequisites;
each SP-04 through SP-11 delivery has a separate PR containing its package,
catalog/coverage/notices changes and relevant tests. The requested plan path is
`docs/research/skillsplan-new.md`.

## Completion

Record actual tests and PR links in this ledger as work lands. Close an item
only when its implementation and required checks pass; record blocked external
checks explicitly. Final acceptance requires all eight shipped packages, the
existing 21 families preserved, the selected Munarium refresh verified, valid
licenses/notices, and no undocumented tool or capability claims. Optional packs
receive an explicit disposition, not speculative implementation.

## Verification record

The combined catalog is version `1.10.0`: 29 packages, including all eight
workflows and the unchanged 21 baseline families. Every individual skill PR
passed its repository checks and merged. The combined implementation also passed
the `Repository and harness` CI check in PR #210.

| Check | Observed result |
|---|---|
| Native `vcp-extensions` package tests, Rust 1.95.0 on Windows | 116 passed, including all eight catalog tests; every shipped package activates with its declared resources and can be revoked |
| Python helper regressions on the committed packages | 24 passed: five MCP, eight PDF, eleven spreadsheet |
| Authoring validator and browser helper | Ten passed: four validator tests and six real Playwright/Chromium checks, with browser sandbox enabled |
| Representative document workflow | Incident-update reader exercise preserved required structure, reported unknown facts, and ignored hostile source instructions |
| Representative frontend workflow | Existing framework/tokens preserved; keyboard, empty/error/pending/success and narrow/wide layouts checked in a real browser; three screenshots visually inspected |
| Representative LLM integration | Five mock tests passed for selected provider/model/history preservation, tool validation, timeout/error handling and effect limits; no live provider call |
| Munarium refresh | 426 imported-library and 19 VCP tests passed; native consumers, provenance and regenerated protocol checks passed; see [the refresh report](../evaluations/sp-02-munarium-refresh.md) |
| Combined installed CLI and archive | Six native CLI tests passed, including exact delivery of all eight installed workflows, source precedence, integrity and unchanged authority/workspace; the executable plus assets passed all 97 archive-entry checks |

Reproduce the helper tests with `python -m unittest discover -s src/tests/skills`
after providing the package requirements and test-only `XlsxWriter==3.2.9`.
Run the two `src/tests/contracts/ported-*.test.cjs` files with Node; browser checks
need `VCP_SKILL_PLAYWRIGHT` pointing to an existing Playwright module and an
installed compatible Chromium. Native Cargo commands run from
`src/third_party/codex/codex-rs` so the repository's Windows build settings apply.
The CLI qualification tests also require `VCP_TEST_NODE` to name an ordinary
native Node executable; `VCP_TEST_SKILL_PACKAGE` optionally selects an extracted
archive whose executable must match the compiled test build.
Dependencies and browsers are not installed by skill discovery or activation.

The additional local Windows `fast` run completed with 19 groups passing and
three timeouts: `cs2-developer`, `builtin-skills`, and `cs-authoring`.
These are not recorded as local passes. The same full suite passed in Linux CI.
Bounded Windows probes loaded all six developer test modules in 238 ms and passed
the campaign preparation case in 11.84 seconds. Two repeated source-identity
scans read 2,099 files in 6.23 seconds; an 18-run historical block repeats them
about 20 times. The eight additions total 210,453 bytes, and scanning the entire
builtin tree took 323 ms. This points to the historical fixture workload on this
host during compilation, rather than a new import failure. No test thresholds or
historical outcomes were changed. Optimizing that workload is separate follow-up
work; the focused native and new-helper results above are independently observed.

No fixed comparative campaign, paid model experiment, broad live-quality claim,
release publication or deployment was performed. PDF supports bounded text
extraction/creation; XLSX supports the documented basic subset. Recalculation
explicitly rejects literal empty-string workbooks because the selected engine
miscounts them; creation, inspection and editing preserve those cells.

## Optional-pack disposition

The eight selected workflows cover the original default scope. The license audit
also reviewed `academy-guide`, `algorithmic-art`, `brand-guidelines`,
`canvas-design`, `discernment-nudge`, `slack-gif-creator`, `theme-factory`, and
`web-artifacts-builder`. These Apache-2.0 sources remain eligible for a future
request, but no additional default workflow or organization-specific branding
requirement was identified in the original goals. They are deferred rather than
imported speculatively. The restrictive Office sources and unlicensed
`doc-coauthoring` remain excluded. This closes the optional-pack demand review
for SP-12; it does not promise delivery of another pack.
