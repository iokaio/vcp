# Codex package and effect boundaries

The shared workspace contains 159 Cargo packages: 154 from Codex, three
[selected Munarium libraries](munarium-source.md), and the original
[VCP CPU embedding helper](local-embeddings.md) plus the original
[local corpus qualification executable](local-memory-spike.md). Its
[boundary inventory](../../src/third_party/components/codex-boundaries.json)
assigns each package to exactly one of 23 review groups and anchors 46 concrete
source seams. It is a **static ownership and navigation record**. It does not
prove absence of hidden effects, enable a VCP runtime or claim that an adapter
already exists. Read it alongside [the source selection](codex-source.md),
[engine execution design](../architecture/engine-execution-design.md),
[ADR-001](../adr/001-runtime-topology.md) and [ADR-013](../adr/013-upstream-reuse-and-vendoring.md).

The [classification contract](upstream-effect-classes.md) defines each group's
conservative effect ceiling, the narrower named entries and their primary VCP
owners. The checker rejects absent or inconsistent classifications as well as
missing package ownership. [Selection gate evidence](../evaluations/p0-07-selection-gate.md)
separates this source review from later runtime enforcement.

## Reproduce and maintain coverage

```powershell
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
node scripts/upstream/check-boundaries.cjs
pwsh -NoProfile -File scripts/test.ps1 -Suite upstream
```

The checker reads the committed Cargo manifests with the pinned TOML parser,
following direct/inherited path dependencies, including build, dev and target
tables. Membership uses resolved directory identity: equivalent external
dependency paths count once, while distinct directories with duplicate package
names still fail. Source containment is checked before canonicalization.
Four packages are implicit members: `codex-windows-sandbox`,
`codex-message-history`, `core_test_support` and `app_test_support`. Checking only
the root `members` list would miss them. The discovered set was independently
compared with `cargo +1.98.0 metadata --locked --offline --no-deps --format-version
1 --manifest-path src/third_party/codex/codex-rs/Cargo.toml`.

This is the union of workspace source dependencies, not the enabled dependency
graph for a particular target/feature. External paths are allowed only beneath
explicit selected component roots; lexical and symbolic-link escapes fail.
Unsupported membership rules, missing
inheritance, path escapes, duplicate names, absent/duplicate ownership, unknown
task IDs and stale source symbols fail the check. Exclude rules and membership
globs need explicit parser work if an upstream update introduces them; they are
not silently ignored. CI needs no Rust compiler or acquired upstream checkout.

Package capability lists identify conservative review surfaces. `retain-data`
is a proposed reuse disposition, not a semantic proof that every function is
pure. `disable-upstream-route` does not remove code from the baseline; P0-08 must
implement and test that restriction in VCP execution. Source-symbol checks prove
that navigation anchors remain present, not that a function was executed or that
a network/filesystem boundary is enforced. Imported bytes have their separate
provenance check in the same suite.

When changing the upstream selection, update each affected group's packages,
gate and owner tasks, then review the seam observations against actual code.
There is no wildcard/default group for new packages. Keep per-file changes and
ordered patches synchronized with [reconstruction](codex-source.md#explicit-reconstruction).

## Controller and in-app pause

P0-03 starts at `core/src/session/mod.rs` (`submit_with_id`) and
`core/src/session/handlers.rs` (`interrupt`, `inter_agent_communication`).
The existing interrupt delegates to active task abort. Queued child input can
cause an idle session to start work. Therefore forwarding `/pause` to an upstream
interrupt alone does not satisfy VCP's pause contract.

Place the prototype inside the retained session/controller path. Carry stable
workspace/task IDs and a steering/authority revision into submissions and worker
completions. Establish an admission fence before draining active work; persist
the pause intent and retain explicit independently paused child state. The CLI
stays open for status/history inspection. `/resume` revalidates configuration,
policy, budget and reconciled effects before opening admission. Closing the owner
uses the same pause boundary; reopening never blindly replays non-idempotent work.

Use the P0 fixture's independent request/effect observer to pause during streaming,
between preparation and dispatch, and while child input is queued. Assert zero
new admissions after acknowledgement, prevent stale completions from authorizing
further work, and retain late observations and partial/unknown effects for
reconciliation. Then repeat owner loss in a fresh Windows process. A
separate mock loop that does not traverse the retained session is insufficient
evidence for this seam. [Engine execution design](../architecture/engine-execution-design.md)
owns exact ordering and acceptance.

## Every model request is an admission boundary

`core/src/client.rs` contains the main `stream`, WebSocket prewarm, remote memory
summarization and realtime API paths. Remote compaction lives in
`core/src/compact_remote_v2.rs`. `memories/write/src/runtime.rs` constructs another
`ModelClient` for background stage-one work. Review threads and guardian retry
helpers add further request lifetimes. These are separate entries in the catalog
so a main-stream-only adapter cannot be mistaken for complete accounting.

P0-08 must route required coding/helper requests through the OpenRouter gateway
with current capability checks, atomic budget reservation and request/usage
attribution. Observe prewarm and discovery traffic even when no generated tokens
are expected. Retry admission must retain attempt identity and reconcile uncertain
usage; background memory/review/children cannot use their own ungoverned client.
Replace upstream memory ownership with VCP's local governed memory contract.
Retained non-VCP service/provider APIs remain disabled in VCP entry points.

The [helper effect map](helper-effect-traces.md) classifies the exercised review,
compaction and shutdown entries and gives the adapter sequence. Nine native
traces now include successful and rejected review/compaction requests. The
observer records scripted provider usage separately from parent CLI usage;
review's zero parent usage is an observed upstream limitation, not VCP accounting.

Use scripted responses and an independent observer before live evaluations.
Record the complete expected request sequence for main coding, compaction,
memory, review and cancellation, then fail on any extra request. A provider base
URL change alone does not establish the accounting or authority boundary. The
[context and provider design](../architecture/context-provider-design.md) owns normalization
and spend semantics; live evaluations still require an explicit cap.

## Credentials, tools and persistence

Provider auth resolution, ambient key helpers and credential-presence telemetry
are separate catalog entries. Preserve useful credential primitives while
removing implicit account/token/command discovery and upstream telemetry from
VCP paths. Tests use isolated synthetic credentials/configuration; do not inspect
a developer's real credential store to make an experiment work.

MCP has distinct process startup, HTTP transport, OAuth discovery and tool-call
seams. All need explicit server identity and authority, with schema validation,
cancellation and untrusted result handling. Executable hooks and foreign config
imports stay deferred. [Routing and extensions design](../architecture/routing-extensions-design.md)
owns those boundaries and visible child-task integration.

`exec-server/src/client.rs` exposes dispatch and termination independently.
Prepared argv/environment/workspace and durable admission precede dispatch;
termination does not undo completed writes. P0-05 must observe real Windows
process trees, locks and access canaries. `execpolicy/src/amend.rs` writes policy
files, so retaining policy matching cannot implicitly authorize saved permission
changes. Canonical storage must subsume upstream SQLite initialization and local
thread persistence without creating another authoritative store/ledger. Keep
observed history distinct from display truncation and model summaries.

The [static inventory report](../evaluations/p0-07-boundary-inventory.md) records
what was checked. Runtime traces and enforced adapters remain separate P0-03,
P0-05 and P0-08 evidence; package coverage does not complete those tasks.
