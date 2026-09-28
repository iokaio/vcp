# CS-3 generated UI execution adapter

This adapter prepares exact returned UI artifacts for the qualified native host.
Preparation alone is not browser observation or skill qualification. The two
prospective cases are `UI-cs3-filter-selection-v1` and
`UI-cs3-disclosure-form-v1`; historical CS-2 imports remain separately authenticated
by the [retained UI importer](cs3-retained-ui-recovery.md).

`cs3-ui-artifact.cjs` accepts precisely one `index.html`, with 1–65,536 valid UTF-8
bytes. It generates a fixed C# resource class containing Base64 data, the exact
case ID and artifact-inventory hash. No model text becomes C# syntax or executes
in the generator. Unknown cases, additional files, malformed Unicode, redirected
input paths and oversized input fail before compilation.

Pass `-UiArtifact <absolute materialized-files.json> -UiCase <case ID>` to
`webapp-browser-build.ps1`. Both arguments are required together. The builder
binds the raw artifact inventory, HTML, generator and physical Node executable,
and checks them again after compilation. The staged resource hash is separate
from the original disabled template hash. No browser is launched during this
build. Without those options, the UI resource remains disabled.

Each enabled build exposes only the fixed `/ui-artifact/index.html` resource,
through the owned server's hash-acknowledged mediator. It must retain all WEB,
process-identity, runtime, origin and teardown checks. The UI parent independently
grades observations for keyboard reachability, visible focus, 320/1024 widths,
reduced motion, no outbound request and the case's five interaction conditions.
Candidate behavior failure is not silently converted into a controller fault or
passing observation; malformed evidence still fails the harness.

Read-only DOM observations execute in a browser-created isolated world, with
universal access disabled. Page-provided replacements for DOM methods,
`getComputedStyle` or `JSON.stringify` cannot supply the oracle's implementation.
Real CDP key/text/pointer input changes the page; observation expressions never
mutate DOM or invoke page callbacks. The oracle is bounded to 512 elements.

Keyboard reachability is joined to the exact typed native controls used by
pointer actions, not any focusable element with matching text. Visible focus
requires a painted outline/shadow change from the same control's unfocused
computed style; invisible outline offsets and unchanged decorative shadows do
not qualify. The deliberately advertised control subset uses native inputs and
buttons and outline/shadow focus indicators. Hidden/collapsed/fully transparent
ancestors and off-viewport rectangles are rejected. Feedback text must be visible.
Reduced-motion observations include each element and its `::before`/`::after`
pseudo-elements. Animation names determine cyclic duration/play-state/iteration
lists; `none`, paused animations and zero iterations contribute no motion.
Transition-property determines cyclic durations, with `none` and last matching
exact-property/`all` entries handled explicitly. Quoted/escaped commas are
preserved, with 8 KiB/128-item bounds per computed-style list. This is a
conservative declaration-based check: missing keyframe definitions and overlapping
transition shorthands are not generalized into a CSS execution model and can
produce conservative failures. It does not infer essential motion, future event
behavior or pixel-level visual quality.

The form oracle clicks the actual Send request button for empty validation, then
uses real Enter for whitespace and successful trimmed submission. The task-picker
oracle preserves selection while filtering it out and clears that hidden selection.

After actual execution, `cs3-ui-artifact.cjs project CASE ARTIFACT_JSON RUN_ID
NATIVE_RECEIPT BUILD_INPUTS BUILD_SHA256` projects an authenticated browser grade.
Paths must be absolute and the caller supplies the previously pinned build hash.
The validator joins the native WEB/cleanup receipt, exact compiled HTML, reviewed
UI oracle sources, server relay and complete parent assertion vector. A generic
JSON object saying `passed` cannot qualify an artifact. Both passing and failed
authentic candidate observations are retained. Visual review remains `not_run`.

## Qualified native controls

Seven generator/validation contracts and eight WEB/adapter contracts pass. The
final native matrix independently projects all 23 controls against the exact
current 23-source closure, generated HTML, process identities and cleanup evidence:
six positive grades and 17 expected negative grades. A negative grade is a correct
oracle result for the seeded defect, not a passing candidate artifact.

| Controls | Required observed result |
| --- | --- |
| Positive, poisoned-positive, inactive-motion; both cases | All 11 assertions pass |
| Negative and poisoned-negative; picker | Hidden selection persistence fails |
| Negative and poisoned-negative; form | Whitespace validation and trimmed success fail |
| Keyboard decoy; both cases | Reachability and focus fail |
| Hidden feedback; picker | Initial state, filtering, hidden selection and clearing fail |
| Hidden feedback; form | All three submission feedback assertions fail |
| Static shadow; both cases | Visible focus fails |
| Pseudo-motion and mixed-motion; both cases | Reduced motion fails |
| Broken submit button; form | Empty validation fails |
| Transparent page; both cases | Exact two-document prefix, `ui-keyboard:control_missing`, all assertions false |

Every nontransparent control supplies all ten picker or nine form documents.
Transparent controls terminate the bounded choreography as authentic candidate
failures; all controls still require complete WEB/runtime/process/cleanup evidence.
All owned profiles are independently absent. Native runs were strictly serial,
with compilers and tests idle. No parallel-native qualification is claimed.

The retained projection is `artifacts/cs3-ui-final-controls.json`, SHA-256
`818074b97799d7d1bf52852dc16ba704de49f6f7f763924b3b6d909d7fc25163`.
It includes each exact artifact, build and native receipt hash. Its normalized
23-source closure (generated resource replaced by its reviewed template identity)
has SHA-256 `1f2f0c5001ad490c625325fa0a49d7ef365640a139af4ae85809be24c1e07a13`.
Twenty-two slots are retained under `artifacts/cs3-ui-matrix-11-*`; the identical
current-source picker pseudo-motion slot is the focused
`cs3-ui-teardown-diagnostic-02` run `2f405a8c5ba74abfac95d3b3fbc0c236`.
No already-passed exact-source slot was rerun merely to populate that directory.

Real positive-receipt checks reject changed assertions, changed HTML and changed
reviewed supervisor source. `webapp-execution.validateUiArtifact` now exposes the
same authenticated validator used by the projection command. Final pause, cancel
and owner-loss evidence is in the matching
[source-26 lifecycle checkpoint](cs3-frozen-web-execution.md#current-source-26-lifecycle-checkpoint).

Earlier failures remain retained and are not upgraded: a parallel synthetic input
readiness failure; source-08 startup and drain-acknowledgement failures; source-09
collector access denial without a captured native stage; and source-10 collector
PID-inventory rejection without original count diagnostics. Corrections preserve
the original deadlines, exact identity coverage and independent drainage checks.
The final controls have no harness failures or hidden retries. The new collector
deferral branch was not observed in this final matrix; its classifier is covered
by pure acceptance/rejection tests, and unresolved native identities remain fatal.

These controls qualify the two new generated-UI tasks, not missing historical
CS-2 artifacts. The owner's explicit
[prospective replacement decision](../plan/24-skills-follow-on.md#cs-3--browser-execution-and-six-skill-acceptance)
allows new canonical UI outputs to replace the unavailable historical regrade for
CS-3 acceptance; it does not retroactively regrade or certify the missing bytes.
The paid six-skill comparisons and their independent reviews remain required.
Visual review remains `not_run`; model calls for this qualification are zero.

## Grading prospective paid output slots without replay

After the canonical `frontend-design` block finishes, its six normal output slots
are `UI-cs3-filter-selection-v1--{none,nearest,candidate}` and
`UI-cs3-disclosure-form-v1--{none,nearest,candidate}`. Read their exact
`materialized-files.json` files; never reconstruct HTML from prose or modify the
canonical output. Validate the complete block with
`cs3-comparison-review.block(planPath, planHash, 'frontend-design')` first.

Do not place any new file inside a canonical slot directory: its evidence digest
includes all top-level files other than `result.json`. Use new build directories
under repository `artifacts`, and a separate private sibling directory for browser
projections and the six-row `browser-grades.json`. Leave the paid run, its result,
claims and accounting untouched.

For each valid materialized output, compile a new immutable resource with the
existing command below. Resolve `$slotId`, `$caseId` and `$materialized` from the
validated frozen plan, not from model-supplied paths. `$build` must be a new absolute
directory under repository `artifacts`. Use the pinned physical Node executable;
the builder also verifies the generator runtime and artifact bytes before/after
compilation. Set `$pwsh` to the current PowerShell executable path. Ensure the
builder's `node` command resolves to the pinned provisioned runtime (prepend its
directory to the child process PATH if needed); call projection/settlement with
that physical executable as well. Each build and native invocation needs a fresh
PowerShell process.

```powershell
& $pwsh -NoProfile -File scripts/evals/webapp-browser-build.ps1 `
  -CoreAssembly D:/code/Github/vcp/artifacts/cs3-frozen-web-26/Microsoft.Web.WebView2.Core.dll `
  -Loader D:/code/Github/vcp/artifacts/cs3-frozen-web-26/WebView2Loader.dll `
  -OutputDirectory $build -UiArtifact $materialized -UiCase $caseId

$inputs = Join-Path $build inputs.json
$buildHash = (Get-FileHash -LiteralPath $inputs -Algorithm SHA256).Hash.ToLowerInvariant()
& $pwsh -NoProfile -File (Join-Path $build Invoke-NativeProbe.ps1) `
  -Mode webview2-dom -Execute -ExpectedInputsSha256 $buildHash
```

Finish all builds before native execution; run the six browser probes strictly
serially with other compilation/tests idle. Retain every native result. A native
failure is not a passing or ordinary failed UI grade and must not be converted to
`not_run_output_invalid`. A clean native result may correctly grade the candidate
`failed`; preserve that result without repeating its paid call.

Project a completed native receipt with:

```text
node scripts/evals/cs3-ui-artifact.cjs project CASE_ID MATERIALIZED_JSON CANONICAL_SLOT_ID NATIVE_RECEIPT INPUTS_JSON BUILD_SHA256
```

The projection's run ID must be the canonical paid slot ID, not the native probe
GUID. The native identity remains hash-bound through `NATIVE_RECEIPT`. Save the
projection unchanged in the separate private browser-evidence directory. The
existing `project` API returns the same object; the existing exclusive-create
`p6-live-runner.boundaries.write` helper can persist it without overwriting evidence.

The browser-grades envelope has `plan_sha256`, `skill: "frontend-design"`, and six
`runs`. Each materialized slot contributes `run_id`, `artifact_sha256`, `status`,
and `receipt: {path, sha256}` pointing to its authenticated projection. If and only
if materialization is absent and the canonical result records `status: "failed"`
with `output_error`, use `status: "not_run_output_invalid"` and the exact
`canonical_result_sha256`, without an invented browser receipt.

Validate the envelope with `cs3-comparison-review.browserGrades`, then use the
same immutable envelope for both blind packet preparation and settlement:

```text
node scripts/evals/cs3-comparison-review.cjs prepare PLAN PLAN_SHA frontend-design NEW_REVIEW_DIRECTORY BROWSER_GRADES
node scripts/evals/cs3-comparison-review.cjs settle PLAN PLAN_SHA frontend-design REVIEW_DIRECTORY READER_A READER_B BROWSER_GRADES
```

These commands perform no paid model calls. Two independently prepared blind
reader receipts remain required; browser measurements are not pixel-level visual
review. Do not invoke `cs3-comparison.cjs run` again to supply missing browser
grades or repair a failed slot. The next skill block remains gated by the completed
disposition of this one.
