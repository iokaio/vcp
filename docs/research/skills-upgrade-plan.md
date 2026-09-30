# Skills upgrade: runtime, helpers and content

Status: planned, September 29, 2026. Follows the completed
[skills development plan](skillsplan-new.md) (SP-01 through SP-12, catalog
`1.10.0`). Decision: [ADR-070](../adr/070-skill-resource-roles-and-discovery.md).
[ADR-069](../adr/069-practical-skill-ports.md) policy is unchanged: pinned and
reviewed licenses, one PR per skill, proportionate functional tests, and no
universal comparative campaign.

## Scope

Owner direction for this plan:

* include loader and host contract changes, recorded in ADR-070;
* extend the PDF, spreadsheet and browser helpers in a targeted way, using
  existing dependencies;
* deepen the 21 baseline families as well as fixing their detection.

Optional Apache packs stay deferred. The excluded Office, PDF and XLSX upstreams
stay excluded. Paid model campaigns and dependencies beyond the listed version
bumps are out of scope. Upstream `anthropics/skills` main was identical to the
pinned `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4` when this plan was written,
so no upstream refresh is required.

## Review findings

Observed on September 29, 2026. Package versions were checked on PyPI and npm.

### Runtime

* **All resources enter context.**
  [`skill_parts`](../../src/crates/vcp-lifecycle/src/foundation/worker/skills.rs)
  injects the body and every UTF-8 resource. The about 11 KB `LICENSE.txt`,
  NOTICE and UPSTREAM files are 40–66% of each workflow package. Helper source
  also enters context, 18 KB for spreadsheets.
* **Workflows are absent from discovery.** Discovery context lists only
  cue-matching skills. Twelve packages use `explicit:<id>` cues that the host
  never emits: all eight workflows plus data, infrastructure, shell and sql.
* **Helpers cannot run from their install location.** Bodies run
  `scripts/...`, but the installed package is beside the executable and context
  parts carry no path. The CLI test checks byte delivery, not helper execution.
* **Root detection misses common layouts.** The host checks twelve root markers
  and misses `build.gradle.kts`, `.sln`/`.csproj`, `requirements.txt`/`setup.py`,
  `deno.json` and `go.work`.

### Test coverage

The 24 Python helper tests in `src/tests/skills` and the six Playwright browser
checks do not run in CI. The `delivery` job runs `test.ps1 -Suite fast`, which
sets up neither Python nor a browser.

### Package defects

* **skill-authoring:** `validate.cjs` counts unique values when it limits cue
  arrays, so it never rejects duplicates. It also accepts `.`, `..` and
  leading-dot ids.
* **webapp-testing:**
  * `check-page.cjs` waits with a Playwright locator and then asserts with
    `document.querySelector`, so Playwright-only selectors pass the wait and
    then throw.
  * It rejects `localhost`.
* **spreadsheet-workflows:** The workbook part allowlist omits `calcChain.xml`,
  and any `extLst` or `definedName` is rejected. Excel writes all three
  routinely, so real Excel-saved workbooks are likely refused.
* **pdf-workflows:**
  * Every encrypted PDF is refused, even one with an empty user password.
  * `strict=True` rejects common malformed files.
  * UTF-8 input with a BOM produces a misleading error.
  * The font coverage check uses a private ReportLab attribute, although the
    NOTICE claims public APIs only.
  * The document title is fixed.
* **document-authoring:** Its NOTICE was copied from skill-authoring.
* **Four workflow bodies** carry YAML front-matter, contrary to the package
  format guidance.

### Stale guidance and pins

* **mcp-development:** Guidance names `FastMCP`/`@mcp.tool`. The current Python
  `mcp` 2.x README uses `from mcp.server import MCPServer`.
* **Dependency pins:** `pypdf` 6.14.2 (6.19.0 current; it parses untrusted
  input) and `formualizer` 0.9.3 (0.10.0 current; retest the empty-string
  defect).
* **Playwright evidence:** recorded on 1.61.1 (1.63.0 current).

### Content gaps

* llm-integration is described as provider-neutral but ships only an Anthropic
  reference.
* frontend-design gives no concrete WCAG thresholds.
* The baseline families are about 19 lines each and repeat one boilerplate
  paragraph.

## Delivery rules

Each item is one PR; baseline families are one PR per family. A PR that changes
a package bumps that skill's `version`, the catalog minor version and the
affected hashes. Merge package PRs sequentially and rebase `catalog.json`.
Record actual checks and any unrun checks in the ledger.

## Work items

| Item | Dependency | Deliverable | State |
|---|---|---|---|
| SU-00 | Owner direction | This plan, ADR-070 and guidance pointers | complete; [PR #212](https://github.com/iokaio/vcp/pull/212) merged |
| SU-01 | None | Run skill helper tests in CI | complete; [PR #213](https://github.com/iokaio/vcp/pull/213) merged |
| SU-02 | SU-00 | Resource roles in the loader | complete; [PR #214](https://github.com/iokaio/vcp/pull/214) merged |
| SU-03 | SU-02 | Helper materialization | complete; [PR #216](https://github.com/iokaio/vcp/pull/216) merged |
| SU-04 | SU-00 | Discovery cues, root markers and descriptions | complete; [PR #215](https://github.com/iokaio/vcp/pull/215) merged |
| SU-05 | SU-02, SU-03 | Package migration to resource roles, and hygiene | complete; [PR #217](https://github.com/iokaio/vcp/pull/217) merged |
| SU-06 | SU-02 | skill-authoring validator and guidance | planned |
| SU-07 | SU-01, SU-05 | spreadsheet-workflows compatibility and operations | planned |
| SU-08 | SU-01, SU-05 | pdf-workflows robustness and operations | planned |
| SU-09 | SU-01 | webapp-testing fixes and accessibility checks | planned |
| SU-10 | None | mcp-development SDK and protocol refresh | planned |
| SU-11 | None | llm-integration provider references | planned |
| SU-12 | None | document-authoring structures | in review |
| SU-13 | None | frontend-design accessibility and theming | planned |
| SU-14 | SU-04, SU-05 | Deepen the 21 baseline families | planned |
| SU-15 | None | Stale inventory pointers and Windows `fast` timeouts | planned |

SU-00, SU-01 and SU-04 can proceed in parallel. SU-09 through SU-13 can start
before SU-05 if they do not change resource declarations. If they do, rebase
them after SU-05.

### SU-01: Skill helper tests in CI

* Commit the test requirement files under `src/tests/skills`, including the
  test-only `XlsxWriter==3.2.9`.
* Register a Python helper group in `src/tests/registry.json`.
* Add Python setup and the pinned installs to the `delivery` job.
* Install Playwright and Chromium so the `ported-webapp` browser checks run via
  `VCP_SKILL_PLAYWRIGHT`.
* Native Windows Cargo tests remain on the manual Windows job.

**Acceptance:** CI output shows the helper tests and browser checks executed,
not skipped.

Delivered as the separate `Skill helpers` CI job so the ten-minute delivery job
is unchanged. Test pins live in `src/tests/skills/requirements-*.txt`; a unit
test requires them to include every shipped package pin. Playwright is pinned
by `src/tests/skills/browser/package-lock.json`. The job relaxes the Ubuntu
AppArmor user-namespace restriction so Chromium keeps its sandbox. Locally, run
`npm ci --prefix src/tests/skills/browser`, install Chromium with that
Playwright, and set `VCP_SKILL_PLAYWRIGHT` to its `node_modules/playwright`.

### SU-02: Resource roles

* Add the optional `use` field to `ContentRef` in `vcp-extensions`, defaulting
  to `context`.
* Activation still reads, hash-checks and counts every resource. `skill_parts`
  skips `file` resources.
* Mirror the field in the catalog manifest, `scripts/skills/builtin-assets.cjs`
  and the skill-authoring validator.

Delivered with two adjustments. The skill-authoring validator's `use` support
moved to SU-06, because editing that script is a package change. The
`builtin-assets.cjs rehash` command moved here from SU-05, because SU-04 edits
every descriptor. It recomputes content, descriptor, coverage and catalog
digests and leaves versions to the author.

**Tests:**

* A tampered `file` resource fails activation.
* `file` resources are absent from context parts.
* Existing descriptors still parse.

### SU-03: Helper materialization

Add a skill control operation, wired through the `vcp_skill` tool and the CLI
skills surface, that copies a verified `file` resource of an active skill to a
relative workspace destination. The copy must:

* pass the canonical write policy and denials;
* use exclusive create and never overwrite;
* revalidate the skill first;
* record the copy in history.

It grants no execution.

**Tests:**

* Success.
* A denied write root.
* A traversal or linked destination.
* An existing destination.
* A revoked skill.
* A tampered source.
* A CLI test that materializes a helper and runs it through the authorized
  execution tool.

Delivered design:

* **The tool.** No skill tool existed, so SU-03 adds the model tool
  `vcp_skill` with a single `materialize` action. It is permitted exactly
  when the owner's ceiling contains `vcp_patch`. It is not a recorded ceiling
  name, so existing recorded ceilings and legacy defaults stay unchanged.
* **What it does.** The host resolves the active skill's `file` resource and
  re-verifies the captured bytes. It then submits the exact `vcp_patch` Add
  File through the ordinary gated path, which covers hooks, child scope,
  policy, approval, exclusive create, and intent and outcome receipts. Before
  dispatch it checks that the prepared change reproduces the verified bytes
  exactly.
* **Limits.** It accepts non-empty LF text up to 96 KiB that ends with a
  newline, and the destination must be a normalized workspace-relative path.
* **Not added.** There is no CLI subcommand, because broker writes need a
  running task.
* **Moved to SU-05.** The CLI end-to-end run of a real helper moves there,
  because it needs a package that declares `file` helpers.

### SU-04: Discovery, cues and descriptions

* Change the twelve `explicit:*` packages to empty cues.
* Add host markers for `build.gradle.kts`, `settings.gradle(.kts)`,
  `requirements.txt`, `setup.py`, `Pipfile`, `deno.json(c)` and `go.work`.
* Add a bounded root-listing check for `.sln`, `.slnx` and `.csproj`, and map
  the new markers to the jvm, python, javascript-typescript, go and
  dotnet-powershell cues.
* Rewrite all 29 descriptions as what the skill does plus "Use when…".
* Remove the YAML front-matter from the four bodies.
* Update the `coverage.json` limitations.

**Tests:**

* Workflows appear in the discovery context within its byte bound.
* Each new marker is detected.

### SU-05: Package migration and hygiene

* Declare licenses, notices, provenance records, requirement files and helper
  source as `use: "file"`.
* Change bodies to materialize a helper before running it.
* Fix the document-authoring NOTICE.
* Add a deterministic `rehash` mode to `builtin-assets.cjs` for descriptor and
  catalog hashes. Most later items edit packages, and manual hash edits are
  error-prone.

**Acceptance:**

* Every shipped package activates and revokes.
* The archive checks pass.
* The recorded per-package activation bytes decrease.

### SU-06: skill-authoring

* **Validator changes:**
  * Reject duplicate cue values, dot-only and leading-dot ids, an id that
    differs from the directory name, and a non-semver version.
  * Warn about undeclared files.
  * Report a missing descriptor clearly.
  * Accept `use`.
* **Guidance:** Document the "Use when" description rule, cue semantics,
  resource roles and their context cost, and add an example descriptor.

### SU-07: spreadsheet-workflows

* **Excel compatibility:**
  * Accept `calcChain.xml`, custom document properties, printer settings,
    `customXml` and tables.
  * Accept `extLst` in styles and workbook parts, and reserved `_xlnm.*`
    defined names.
  * Preserve these parts on edit and keep rejecting user defined names that
    formulas reference.
  * Test with a synthetic Excel-shaped fixture. Also test one real Excel-saved
    file when Excel is available, and record that check honestly.
* **New operations:**
  * `sheets`: list sheet names and dimensions.
  * `csv-import`: create a typed new workbook from CSV.
  * `csv-export`: write a sheet or range to a new CSV.
* **Formula subset:** Consider SUMIF(S), COUNTIF(S), AVERAGEIF(S), IFERROR,
  AND/OR/NOT, INDEX/MATCH, VLOOKUP, XLOOKUP, DATE, LEN, CONCAT and
  ROUNDUP/ROUNDDOWN. Admit only functions that the recalculation engine is shown
  by test to evaluate correctly.
* **Engine bump:** Bump `formualizer` to 0.10.0 and retest the empty-string
  defect. Lift the rejection only with evidence.
* **Errors:** Report JSON and date parse errors with their location.

### SU-08: pdf-workflows

* **Dependency bump:** Bump `pypdf` to 6.19.0.
* **Input handling:**
  * Try an empty-password decrypt before refusing an encrypted file.
  * Read non-strictly, reporting parser warnings and keeping the size and page
    limits.
  * Check decoded content size incrementally.
  * Accept a UTF-8 BOM, and give a clear error for non-UTF-8 text.
* **New operations:** Add `merge`, `split`, `rotate` and `info`, all with
  exclusive output creation and preserved inputs.
* **Create options:** Add title, page size, margin and font size options.
* **Font check:** Replace the private font attribute with a public coverage
  check, or correct the NOTICE.

**Tests:** Cover each operation, plus encrypted, malformed and BOM inputs.

### SU-09: webapp-testing

* **Fixes:**
  * Accept `localhost`, which Chromium resolves only to loopback.
  * Add opt-in HTTPS certificate-error tolerance for loopback only.
  * Assert text through the same locator that was waited on.
  * Expose the timeout option.
* **Additions:**
  * Add ARIA snapshot output using Playwright's built-in API.
  * Add an optional axe-core scan when the project already provides it. Do not
    bundle axe-core.
  * Add reference patterns for request mocking, stored authentication state and
    traces.
* **Evidence:** Re-record the test evidence on Playwright 1.63.0.

### SU-10: mcp-development

* **Python guidance:** Update to the `mcp` 2.x `MCPServer` API, verified against
  a pinned SDK release.
* **TypeScript guidance:** Keep `McpServer`/`registerTool` and add a Zod v4
  schema note.
* **Protocol revision:** Name the current dated protocol revision.
* **New coverage:** Add output schemas and structured content, resources and
  prompts, elicitation, authorization and the Inspector.
* **Pagination:** Parameterize the pagination limit.
* **Tests:** Extend `test_mcp_port.py`.

### SU-11: llm-integration

* **Tool schema:** Add a provider-neutral tool schema asset with per-provider
  wrapping notes.
* **Provider references:** Add OpenAI tool-call ordering and strict schemas,
  Gemini, and Bedrock/Vertex hosting differences. Keep the rule of no model IDs
  or prices.
* **Additional topics:** Cover reasoning-block preservation, batch processing,
  token counting and embeddings/RAG boundaries.
* **Tests:** Test the schema assets.

### SU-12: document-authoring

* Port the upstream `internal-comms` FAQ and newsletter/leadership-update
  examples (Apache-2.0, at the pin).
* Add ADR and decision-proposal structures.
* Make the description list the covered document types.

### SU-13: frontend-design

* Add WCAG 2.2 AA thresholds: text contrast 4.5:1, non-text contrast 3:1,
  24 × 24 CSS px targets, and visible focus.
* Add guidance on theming and dark mode, RTL and internationalization, and
  loading and layout-shift performance.
* Add a note on framework and component-library conventions.
* Confirm that the September 3, 2026 upstream guidance on avoiding generic
  defaults is reflected.

### SU-14: Baseline families

**Content for each family:**

* the order for discovering build, test, lint and format commands from the
  repository;
* common toolchain variants;
* what counts as verification evidence;
* frequent pitfalls, including Windows-specific ones.

**Constraints:**

* Keep each family version-agnostic and under about 6 KB.
* Reduce the shared authority paragraph to one line.
* Update the matching predeclared rubrics in `src/evals/skills/builtin/projects`.

**Delivery order:**

1. rust, python, javascript-typescript, dotnet-powershell and go.
2. jvm, cpp, ruby, php, swift and dart.
3. shell, sql, data and infrastructure.
4. architecture, review-debug, testing, git-workflow, project-optimize and
   memory-hygiene.

### SU-15: Close-out

* **Stale inventory pointers:** Add current-state pointers to the stale
  inventory statements in `src/skills/candidates/README.md`,
  `docs/research/claudeskills.md` and the CS-1 development notes. Do not
  rewrite history.
* **Identity scans:** Reduce the repeated source-identity scans behind the
  Windows `fast` timeouts (`scripts/evals/authoring-prepare.cjs` `identity()`,
  called per row from the campaign runners). Cache content hashes within a run
  by path, size, modification time and file identity, so per-row change
  detection is preserved. Do not change timeouts or historical outcomes.

**Acceptance:** The cs2-developer, builtin-skills and cs-authoring groups pass
locally on Windows.

## Verification

**Package items:**

* the skill-authoring validator on each changed package;
* `python -m unittest discover -s src/tests/skills`;
* the `ported-*.test.cjs` contracts with Playwright;
* native `cargo test -p vcp-extensions --test catalog --test skills` from
  `src/third_party/codex/codex-rs` on Windows;
* `scripts/package-skills.ps1` for the archive checks.

**Runtime items:** Also run the lifecycle tests and the CLI `executable` tests
with `VCP_TEST_NODE`. End-to-end acceptance for SU-03: the CLI discovers and
activates pdf-workflows, materializes its helper, and extracts text from a
synthetic PDF through the authorized execution tool.

**Every PR:** run `scripts/test.ps1 -Suite fast` locally where practical; the
CI `Repository and harness` check must pass.
