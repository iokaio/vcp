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

Current offline evidence: seven generator/validation contracts and eight WEB/adapter
contracts pass. Native UI
positive/negative controls and original CS-2 regrading are not established by
these tests and remain required before paid frontend comparison acceptance.
