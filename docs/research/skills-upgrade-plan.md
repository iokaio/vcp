# Skills upgrade: runtime, helpers and content

Status: SU series complete, September 30, 2026 (catalog `1.41.0`); the
[post-upgrade review and SH series](#post-upgrade-review-and-hardening-plan-sh-series)
is planned. Follows the completed
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
| SU-06 | SU-02 | skill-authoring validator and guidance | complete; [PR #219](https://github.com/iokaio/vcp/pull/219) merged |
| SU-07 | SU-01, SU-05 | spreadsheet-workflows compatibility and operations | complete; [PR #246](https://github.com/iokaio/vcp/pull/246) merged |
| SU-08 | SU-01, SU-05 | pdf-workflows robustness and operations | complete; [PR #245](https://github.com/iokaio/vcp/pull/245) merged |
| SU-09 | SU-01 | webapp-testing fixes and accessibility checks | complete; [PR #221](https://github.com/iokaio/vcp/pull/221) merged |
| SU-10 | None | mcp-development SDK and protocol refresh | complete; [PR #222](https://github.com/iokaio/vcp/pull/222) merged |
| SU-11 | None | llm-integration provider references | complete; [PR #223](https://github.com/iokaio/vcp/pull/223) merged |
| SU-12 | None | document-authoring structures | complete; [PR #218](https://github.com/iokaio/vcp/pull/218) merged |
| SU-13 | None | frontend-design accessibility and theming | complete; [PR #220](https://github.com/iokaio/vcp/pull/220) merged |
| SU-14 | SU-04, SU-05 | Deepen the 21 baseline families | complete; one PR per family, [#224](https://github.com/iokaio/vcp/pull/224) through [#244](https://github.com/iokaio/vcp/pull/244), merged |
| SU-15 | None | Stale inventory pointers and Windows `fast` timeouts | complete; this close-out PR |

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

## Verification record

Observed during delivery (Windows 11, Rust 1.95.0, Node 24, Python 3.11).
Every PR passed the required `Repository and harness` and `Skill helpers` CI
checks before merge. `Skill helpers` runs the Python helper suite and the real
Playwright browser checks with no skips.

**Runtime (SU-02 to SU-05):**

* `vcp-extensions` skills and catalog tests pass.
* `canonical_host skills::` passes 4 of 4. This includes the materialization
  cases: exact bytes, context-role refusal, an existing destination, a write
  denial, traversal, a revoked skill and a tampered source.
* The CLI `ported_skills` test passes 2 of 2. The installed pdf-workflows
  package withholds its helper from context, materializes it byte-exactly
  through `vcp_skill`, and extracts text through a Python `vcp_exec` profile.
  It was rerun after the SU-08 helper upgrade.

**Context cost.** File-role resources cut the eight workflows' activation
context from 202,543 to 60,891 bytes. Later content additions grew some
packages again. llm-integration is the largest, at about 35 KB of context.

**Helpers.** A final count of 65 Python helper tests pass. Independent reviews of
the SU-07 and SU-08 rewrites found injection, resource-exhaustion,
permission-stripping and echoed-text issues. All were fixed with regression
tests before merge.

**Windows `fast` groups (SU-15).** The local failures had two causes:

* **Symlinked Node.** Node reached through a symlinked version-manager path is
  rejected by the scripts' link guard. That is a host setup issue; tests must use
  the real executable path.
* **`plain()` syscalls.** `plain()` performed two syscalls per ancestor
  directory on every file access, and this was 300 of 342 seconds in the
  slowest test.

Two fixes, with the real Node executable:

* one `lstat` per ancestor, which also rejects dangling links;
* a per-run digest cache for source-identity scans, keyed by path and
  nanosecond stat stamp.

| Group | Before | After | Limit |
|---|---|---|---|
| builtin-skills | timed out at 30 s | 28 s | 30 s |
| cs2-developer | 597 s, failed | 233 s, passed | 600 s |
| cs-authoring | timed out at 600 s | 411 s, passed | 600 s |

No timeouts or historical outcomes were changed. `builtin-skills` passes with
little headroom on this host.

## Not run

* **Real third-party tools.** No real Excel-saved workbook was tested, only a
  synthetic Excel-shaped fixture. The axe-core rule engine was not run because
  it is not installed; the tests used a stand-in. The OAuth flow and MCP
  Inspector were not exercised.
* **No live model campaign.** No paid or live model calls were made, and no
  OpenRouter budget was used.
* **Manual Windows jobs.** The native Windows qualification job is manual and was
  not dispatched. The equivalent native tests were run locally, as listed above.
* **Lifecycle unit test.** `backup_run` needs `VCP_TEST_GIT`, which is not set
  on this host.

## Follow-up candidates

* **Lazy per-topic references.** A resource role for references loaded only on
  request would let llm-integration keep per-provider references out of
  context.
* **Performance headroom.** The `builtin-skills` group needs more headroom on
  slower Windows hosts.
* **Real-tool evidence.** A real Excel-saved workbook fixture and an installed
  axe-core run would give stronger evidence.

# Post-upgrade review and hardening plan (SH series)

Status: planned, September 30, 2026. The owner requested a full review of the
skills implementation after SU-15; the findings are recorded here, skill by
skill, together with the work items that address them. Decision:
[ADR-071](../adr/071-on-demand-skill-references.md). Delivery rules are
unchanged from the SU series: one PR per item, one PR per skill for package
changes, merge on green CI, record actual and unrun checks.

Owner choices for this series:

* add an on-demand `reference` resource role, recorded in ADR-071;
* add an Ubuntu Rust job to routine CI;
* record the findings and work items in this plan.

Severity: **H** high, **M** medium, **L** low.

## Findings: runtime and tooling

Discovery and resolution (`vcp-extensions`):

* **H: one source can break discovery.** Discovery shares its entry and
  descriptor budgets across all sources, and any one source's limit or open
  error fails the whole call (`discovery.rs`). A large workspace source can
  leave the session with no skills.
* **M: disabling an override hides the builtin.** Disabling the
  highest-precedence copy of a bare id errors instead of falling back to the
  builtin copy.
* **M: shadowing is silent.** A cross-precedence shadow produces no
  diagnostic.
* **L: limits differ between components.** The license length is 128 in the
  catalog and 256 in the descriptor. The skill count is 128 in Rust and 64 in
  the stager. Descriptor bytes are 64 KiB in the validator and 16 KiB in
  discovery.
* **L: diagnostic paths keep Windows separators.**

Lifecycle and `vcp_skill`:

* **H: `vcp_skill` is offered in child tasks**, where no skill state exists, so
  it can only fail.
* **M: the model cannot see what it may copy.** Nothing lists an active skill's
  `file` resources, so the model must guess helper paths from the body text.
* **M: large markers are missed.** Marker detection reads up to 64 KiB of each
  marker file on every turn, and misses a larger `package.json`. It should
  check existence instead.
* **M: discovery ordering ignores relevance.** The list is sorted by qualified
  id and truncated. Cue-matched skills are not listed first, and `total`
  counts incompatible or disabled skills.
* **M: repeated work per request.** Each request re-captures the discovery
  artifact. Skill validation and the tool-ceiling lookup re-read the store
  every time.
* **M: incompatibility fails every turn.** An active skill that becomes
  incompatible makes every later turn fail instead of being reported as
  unavailable.
* **M: hooks see materialization inconsistently.** Before-hooks see
  `vcp_patch`, after-hooks see `vcp_skill`, and neither receives the skill,
  resource or digest.
* **M: the CLI and runtime compute compatibility differently.** The CLI's
  offline compatibility tools differ from the runtime's, so `vcp skills list`
  can disagree with activation.
* **L: materialization rough edges.**
  * The destination's parent must exist, and the tool schema does not say so.
  * Destination validation is duplicated.
  * Ambiguous skill ids are not listed.
  * `/skills list` hides active resources.
  * Five `LICENSE.txt` files lack a trailing newline, so they cannot be
    materialized.

Tooling and CI:

* **H: the validator is looser than the runtime.** It accepts descriptors that
  discovery rejects (64 KiB against 16 KiB). It does not check that the body is
  UTF-8, that `file` resources can be materialized, that cues can be emitted,
  that `required_tools` names exist, or links to undeclared files. It is not
  run against the shipped packages.
* **M: `rehash` has no safe mode.** It writes before validating, has no check
  mode, accepts an installed tree, and does not warn when content changes but
  the version does not.
* **M: no Rust skill test runs in routine CI.**
* **M: coverage gaps in tests.** Untested behavior includes discovery-context
  contents, end-to-end marker detection, missing-parent and CRLF or binary
  materialization, the path where a hook rewrite fails the exact-bytes check,
  and child tasks.
* **L: the asset enumerator cap is near.** It is 256 entries, and the tree
  already has 147.
* **L: `plain()` repeats ancestor checks** within a call.

## Findings: workflow skills

Across packages:

* **M: every reference is sent.** Bodies say to read only matching references,
  but all context references are sent on activation. Context bytes
  (body plus context resources):

  | Package | Context bytes |
  |---|---:|
  | llm-integration | 30,967 |
  | frontend-design | 20,068 |
  | mcp-development | 18,651 |
  | spreadsheet-workflows | 12,976 |
  | webapp-testing | 11,695 |
  | document-authoring | 10,404 |
  | skill-authoring | 9,226 |
  | pdf-workflows | 7,963 |

* **M: nothing reports or limits context size.**
* **L: stale version line.** `UPSTREAM.md` line 3 says "VCP 2.0.0" in three
  packages.

### document-authoring

* **L: missing triggers.** The description omits READMEs, release notes,
  changelogs and postmortems.
* **L: routing wording implies selective loading.**
* **L: no descriptor-and-link test.**

### skill-authoring

* **M: validator drift from the runtime**, as listed under tooling.
* **M: no context-budget report.**
* **L: raw error on a missing resource.** The validator throws a raw ENOENT.
* **L: no exact run command.** The body gives no `node validate.cjs <dir>` step
  and no note that destinations use forward slashes.
* **L: untested.** The CLI exit code and size limits have no tests.

### frontend-design

* **M: large context.** 20 KB is sent with "load when" wording, and
  reduced-motion and contrast guidance repeats across four files.
* **L: no descriptor-and-link test.**

### mcp-development

* **M: `server-patterns.md` is 14.3 KB of context.** It combines Python,
  TypeScript, elicitation and authorization.
* **M: the tests prove little.**
  * They only parse Python snippets; the TypeScript snippet is unchecked.
  * The API claims have no in-repository evidence.
  * The test ignores `VCP_SKILLS_ROOT`.
* **L: broken license pointer.** The asset's license pointer `../LICENSE.txt`
  breaks after materialization.

### llm-integration

* **H: the largest context package.** It sends 31 KB although only the
  selected provider's reference is needed. `assets/tool-schema.json` can be
  derived from the neutral schema.
* **M: weak model-ID test.** The pattern misses `claude-sonnet-4-5`, `o3` and
  similar names.
* **L: undated claims.** Date-sensitive provider claims lack a re-check date.

### webapp-testing

* **M: raw Playwright error text is printed.** It can include page HTML or
  text.
* **M: stale commands.** Reference commands use the package path instead of
  the materialized copy.
* **M: blocked requests are not documented or named.** Any blocked request
  fails the check; the docs do not say so and blocked origins are not listed.
* **L: outputs are not contained.** Screenshot and ARIA output paths are not
  checked against the project root.
* **L: inaccurate body line.** The body says text capture is opt-in, but
  button labels are always returned.
* **M: untested entry points.** `main`/`--help`, Playwright resolution,
  page-error failure and service-worker blocking are untested.

### pdf-workflows

* **M: the install step cannot work as written.** It points at
  `requirements.txt`, which is a `file` resource the body never materializes.
* **M: misleading error.** A missing output parent reports "Path must stay
  inside the existing workspace".
* **M: `split` reorders pages.** It sorts and de-duplicates the requested
  pages without saying so.
* **L: `info` prints up to about 50 KB of metadata by default.**
* **L: output is validated late.** Output paths are checked after parsing.
* **L: vague merge error.** The combined-limit error does not name the limit.
* **L: junction checks need Python 3.12.** Undocumented.
* **M: duplicated path code.** `local_path` and the I/O helpers duplicate the
  spreadsheet helper; there is no parity test.
* **L: untested edge cases.** Links, a missing parent and the AES dependency
  error.

### spreadsheet-workflows

* **M: `_xHHHH_` text is silently changed (confirmed bug).** `create` and
  `csv-import` store `_xHHHH_` text unescaped, and Excel decodes it. `edit`
  refuses the same text.
* **M: CSV headers are refused.** Header cells beginning with `=`, `+`, `-` or
  `@` are refused even when the column is text.
* **M: table refusal depends on the engine.** It relies on matching the
  engine's error wording.
* **L: incomplete sheet-name validation.** Leading or trailing `'`, "History"
  and control characters are accepted.
* **M: Excel 365 parts rejected.** Common parts (`xl/metadata.xml`,
  thumbnails, comments) are rejected. No real Excel-saved file has been
  tested.
* **L: docs gaps.** They do not say that sheet names are printed on stdout,
  that ranges must be uppercase and unqualified, or the install and
  missing-parent notes.

## Findings: baseline families, fixtures and docs

Frozen fixtures and coverage:

* **H: 11 of 42 fixture expectations are stale.** The fixture author still uses
  the old 12 markers and an explicit-only family list. The python, jvm and
  dotnet negative cases now emit cues, and shell, sql, data and infrastructure
  are always listed. The qualification example would fail those cases, but it
  runs only manually.
* **M: coverage entries are stale.** Some rubric text is outdated, and the
  `generic_suggestion` and `description_discovery` labels overlap.

Families:

* **H: python.** `uv run` may lock, sync and download; `hatch run` creates
  environments.
* **H: dotnet-powershell.** `build`, `test` and `format` restore implicitly
  (NuGet network access and possibly private-feed credentials).
* **M: implicit downloads elsewhere:**
  * dart: `flutter test` and `analyze` run `pub get`.
  * rust: rustup auto-installs a pinned toolchain.
  * go: `GOTOOLCHAIN`, the proxy and read-only module mode are unguarded.
  * jvm: bare `mvn` contradicts the wrapper rule.
  * cpp: preset configuration can fetch dependencies.
  * javascript-typescript: Corepack downloads and Yarn Plug'n'Play are not
    covered.
* **M: infrastructure.** It lacks concrete low-effect checks: `fmt`,
  `validate` after `init -backend=false`, `helm template` or `lint`,
  `kubectl --dry-run=client`, `compose config`, `actionlint`, and the
  `pull_request_target` warning.
* **L/M: sql and data.** `EXPLAIN ANALYZE` executes the statement, and
  `dbt compile` connects to the warehouse.
* **M: review-debug.** Bisecting in the current worktree is unsafe.
* **L/M: git-workflow.** Missing: force-push, `branch -D`,
  `worktree remove --force`, `--no-verify`, `git config` changes, and the
  difference between `restore --staged` and `restore <path>`.
* **M: stale "activate explicitly" sentences** in shell, sql, data,
  infrastructure, jvm, go and dotnet.
* **L: vague VCP commands.** memory-hygiene and project-optimize should name
  the real surfaces (`vcp memory …`, `vcp prune …`, `vcp retention show`,
  `/optimize …`).
* **L: duplicated line 3.** The second half of line 3 repeats the Authority
  line.
* **M: detection gaps.** `meson.build`, `.vcxproj` and `.vbproj` are
  undetected. Empty cues stay correct for infrastructure, sql, data and shell:
  a cue would hide these always-listed skills.

Docs drift:

* `src/evals/skills/builtin/README.md` still describes explicit-only families.
* `docs/development/p7-builtin-skills.md` stops at catalog 1.12.0 and still
  says four families need explicit selection.
* These say "every declared resource is loaded" or "21 skills" with no pointer
  to the later state:
  * `docs/development/cs1-authoring-skills.md`
  * `docs/development/cs2-developer-skills.md`
  * `docs/plan/24-skills-follow-on.md`
  * `docs/claude-audit.md`
* This plan's SU-03 specification still mentions a CLI surface for
  materialization.

## SH work items

| Item | Dependency | Deliverable | State |
|---|---|---|---|
| SH-00 | Owner direction | This review, ADR-071 and the SH ledger | complete; [PR #248](https://github.com/iokaio/vcp/pull/248) merged |
| SH-01 | SH-02 | Ubuntu Rust CI job | planned |
| SH-02 | None | Fixture v2 revision and shared marker source | in review |
| SH-03 | None | Discovery robustness and resolution fixes | planned |
| SH-04 | SH-00 | `reference` role, `vcp_skill read`, scoped tool, resource manifest | planned |
| SH-05 | None | Runtime performance caching | planned |
| SH-06 | None | Tooling: safe rehash, package contract checks | planned |
| SH-07 | SH-04 | skill-authoring validator parity | planned |
| SH-08 | None | pdf-workflows hardening | planned |
| SH-09 | None | spreadsheet-workflows fixes | planned |
| SH-10 | None | webapp-testing hardening | planned |
| SH-11 | SH-04 | llm-integration on-demand references | planned |
| SH-12 | SH-04 | mcp-development split references and tests | planned |
| SH-13 | SH-04 | frontend-design and document-authoring | planned |
| SH-14 | SH-06 | Baseline family safety and accuracy, one PR per family | planned |
| SH-15 | SH-02 | Detection gaps | planned |
| SH-16 | All | Docs drift close-out | planned |

### SH-01: Ubuntu Rust CI job

Add a cached job (keyed on `Cargo.lock` and the toolchain) that runs:

* `cargo test -p vcp-extensions`;
* the lifecycle skills unit tests, if they compile on Linux;
* the SH-02 fixture qualification test.

Windows-native tests stay in the manual job.

### SH-02: Fixture v2 and marker parity

* **One marker source.** Add `src/skills/builtin/markers.json`, read by
  `vcp-lifecycle` through `include_str!` and by
  `src/evals/skills/builtin/author-fixtures.cjs`.
* **Fixture v2.** Generate a v2 revision with the corrected expectations.
* **Coverage.** Update the stale rubric text and resolve the overlapping
  selection labels.
* **Qualification in CI.** Convert the `builtin_skill_qualification` example
  into a test that runs in SH-01.

### SH-03: Discovery robustness

* **Per-source limits.** Give each source its own budget, and report its
  failures as a diagnostic for that source only.
* **Resolution.** Filter disabled candidates before choosing by precedence,
  and add a `shadowed` diagnostic.
* **Shared limits.** Define the limits once for Rust, the stager and the
  validator.
* **Markers.** Check marker existence with metadata instead of reading.
* **Ordering.** List cue-matched skills first, and count compatible skills
  only.
* **Incompatibility.** Report an incompatible active skill as unavailable.

**Tests:** one oversized source, a disabled override, and the discovery
context contents.

### SH-04: References and scoped vcp_skill (ADR-071)

* **Role.** Add `ResourceUse::Reference`, mirrored in the catalog, stager and
  validator.
* **`read` action.** Add `vcp_skill read`, re-verified, UTF-8, at most 64 KiB.
* **Ceiling and scope.** `vcp_skill` is implied by `vcp_read`, `materialize`
  requires `vcp_patch`, and the tool is not offered in child tasks.
* **Resource manifest.** Add a per-skill resource manifest part.
* **Materialization fixes.**
  * Pre-check the destination's parent and state it in the schema.
  * Share one destination validator.
  * List candidates for an ambiguous skill id.
  * Give hooks a `materialized_from` payload.
* **CLI.** Share `available_tools` with the runtime, and show resources and
  shadowing in `/skills list`.

**Tests:** lifecycle cases for `read`, refusals, child scope, the manifest and
a missing parent, plus the CLI end-to-end test.

### SH-05: Runtime performance

* Reuse the discovery artifact when its bytes are unchanged.
* Cache skill validation and ceiling lookups by revision.

**Test:** a counter-based test of repeated turns.

### SH-06: Tooling and contract checks

* **`rehash`.** Validate in memory before writing, add a `--check` mode for
  CI, refuse installed trees, and warn when content changes without a version
  bump.
* **Enumerator cap.** Raise it and warn near the limit.
* **Contract tests:**
  * validate every shipped package with zero warnings;
  * enforce a per-package context budget;
  * lint baseline bodies;
  * parse every quoted `/…` command through the CLI parser.

### SH-07: skill-authoring

* **Validator parity:**
  * a 16 KiB descriptor and a UTF-8 body;
  * `file` materialization rules;
  * the `reference` role;
  * emittable cues and known tool names;
  * undeclared links;
  * a clear missing-resource error;
  * a context-bytes report.
* **Body.** Add the exact run command and the forward-slash note.
* **Tests** for each.

### SH-08: pdf-workflows

* Materialize `requirements.txt` before installing.
* Give a precise missing-parent error, and validate output paths early.
* Name the limit in the combined-merge error.
* Preserve or document the `split` page order.
* Bound `info` output on stdout.
* Document `--root .` and the junction limits before Python 3.12.
* Add a parity test for the shared path code, and tests for links, a missing
  parent and AES.

### SH-09: spreadsheet-workflows

* Reject or escape `_xHHHH_` text in `create` and `csv-import`.
* Treat header cells as explicit text.
* Detect tables before calling the engine.
* Tighten sheet-name validation.
* Decide on the Excel 365 parts.
* Fix the documentation gaps.
* Add regression tests for each.

### SH-10: webapp-testing

* Bound Playwright error text.
* Name blocked origins and document that they fail the check.
* Contain output paths.
* Use the materialized copy in reference commands.
* Correct the body line about text capture.
* Test `main`, resolution, page errors and service-worker blocking.

### SH-11: llm-integration

* Move the provider references to `reference`.
* Derive or demote the Anthropic tool-schema asset.
* Strengthen the model-ID test.
* Add re-check dates to date-sensitive claims.

### SH-12: mcp-development

* Split the server patterns into Python, TypeScript and protocol references.
* Honor `VCP_SKILLS_ROOT` and check the TypeScript snippet.
* Record how the API claims were checked.
* Fix the asset's license pointer.

### SH-13: frontend-design and document-authoring

This is two PRs, one per skill.

* Move references to `reference` where they are topic-specific.
* Remove repeated accessibility text.
* Broaden the document-authoring description triggers.
* Add descriptor-and-link tests.
* Fix the stale `UPSTREAM.md` version lines; each rides with its own package's
  PR.

### SH-14: Baseline family safety and accuracy

One PR per family:

* **Network and install safety.** Add explicit guards against hidden
  downloads for python, dotnet-powershell, dart, rust, go, jvm, cpp and
  javascript-typescript.
* **Effects.** Name the low-effect checks for infrastructure, sql and data.
* **Safer git guidance.** Bisect in a task-owned worktree (review-debug), and
  complete the destructive-command list (git-workflow).
* **Stale sentences.** Replace the "activate explicitly" wording.
* **Commands.** Name the real VCP commands, verified by the SH-06 command
  test.
* **Line 3.** Remove its duplicated half.

### SH-15: Detection gaps

* Add `meson.build`, `*.vcxproj` and `*.vbproj` markers.
* Add them to the cpp and dotnet-powershell cues.
* Keep the documented empty cues.

### SH-16: Docs drift close-out

* Update the evals README and the builtin catalog notes.
* Add later-state pointers to the CS and plan documents and the audit.
* Correct this plan's SU-03 wording.
* Close the SH ledger.

**Order:**

1. SH-00, SH-06 and SH-02.
2. SH-01.
3. SH-03 and SH-05.
4. SH-04.
5. SH-07.
6. SH-08 to SH-14, as parallel content work; SH-11 to SH-13 need SH-04.
7. SH-15.
8. SH-16.
