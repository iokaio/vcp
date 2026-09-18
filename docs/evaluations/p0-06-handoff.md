# P0-06 — Feasibility decision and handoff

Status: P0-06 complete for bounded feasibility. All P0 prerequisites, retained
native regressions and the common Rust 1.98.0 CLI build passed. This does not
qualify an installable VCP product or assert owner release acceptance.

The [setup and source map](../development/p0-handoff.md) gives concrete commands,
component ownership and segment 02 edit locations. The table below consolidates
the gate without converting prototype evidence into product/release claims.

| Prerequisite | Qualified envelope and evidence | Decision |
|---|---|---|
| P0-01 harness | [Delivery/experiment harness](p0-01-experiment-harness.md), synthetic fixtures, truthful pass/fail/not-run and source identity | Keep implemented local orchestration; require actual registered checks, not placeholder scripts |
| P0-07 immutable source | [Selection gate](p0-07-selection-gate.md), committed Codex/Munarium source, native baselines and independent reconstruction | Keep cohesive Codex revision `3d3ae4965ab370217e871b3a7f0d15589557ee4b`; retained libraries share one Cargo workspace |
| P0-02 local memory | [Governance](p0-02-local-governance.md), [observed offline inference](p0-02-offline-embeddings.md), [100/1,000/10,000-record resource/recall runs](p0-02-local-resources.md) | Select pinned Candle/Tokenizers/MiniLM CPU candidate with Tantivy 0.22.1 and DiskANN 0.56.0; no remote embedding fallback or hosted memory service |
| P0-03 owner lifecycle | [Recovery evidence](p0-03-recovery-execution.md), durable startup/work gates, in-app pause/status/resume, root/child holds, owner-loss and fresh-process reconciliation | Keep one host-owned controller tree; reopening remains paused and cannot replay unknown effects |
| P0-05 native execution | [Independent process/access observations](p0-03-recovery-execution.md), argv/output/path tests, noncooperative descendants, Job Object stop and separate AppContainer controls | Keep retained native process infrastructure; do not claim a general sandbox from separate containment/access prototypes |
| P0-04 storage/portability | [Backend parity and encrypted handoff](p0-04-portable-storage.md), eight crash cases, 24 packaging rows, independent crypto and second Windows restore | Select SQLite WAL/FULL default candidate and immutable encrypted artifact reuse; retain files/journal as an alternative feasibility result, not a supported backend |
| P0-08 retained integration | [Coding trace, common store/ledger, maintenance experiment](p0-08-09-integration.md) | Keep retained controller/client/tool structure with host controls; replace prototype persistence/pricing with P1 contracts |
| P0-09 Gemini boundaries | [Actual pinned comparisons and attributed ports](p0-08-09-integration.md) | Keep small neutral canonicalization/preparation/state adaptations; Node is comparison infrastructure and provider SDK types stay outside the Rust boundary |

## Decisions and failed alternatives

The storage comparison chooses SQLite's existing transactional machinery over
promoting the framed-file spike as the default. Both satisfy the bounded parity
experiment; files/journal would need additional production replay/checkpoint and
migration work. Whole-view JSON full snapshots amplify small updates; immutable
encrypted artifact reuse is the chosen next design, with production canonical
records still to be specified.

Age 0.11.2 and Ed25519-dalek 2.2.0 are the qualified Rust encryption/signature
candidates, separately checked with pinned Go age 1.3.2 and Node verification.
Recipient knowledge never grants writer trust. Restore requires separately
enrolled writer keys and local lineage/watermark checks. Fresh offline global
freshness and revocation of already copied ciphertext cannot be promised.

Retaining the coherent Codex build graph worked with small ordered host patches;
creating a second engine was unnecessary. Isolated helpers must retain authority
gates. Flat synthetic admission successfully demonstrates one atomic shared cap,
but cannot become provider pricing. The Gemini experiment preserves semantic
encoding details and records intentional authority differences instead of
copying SDK types or transplanting a second scheduler.

The unmodified Rust 1.98 Codex recursion-limit failure and its reviewed workaround
remain in the source record. Native 1.95 baseline evidence is preserved; the
integration, lifecycle, recovery and retained CLI build passed on the 1.98
compiler used by storage/local memory. This handoff selects that common
engineering candidate while preserving the ordinary upstream 1.95 build pin.

## Risks with explicit next owners

| Owner | Required follow-up |
|---|---|
| P1-01/02/03/04/05/06 | Typed revision domains, internal event envelopes, full capture, production SQLite transactions/migrations, accurate monetary reservations/reconciliation and projections; replace private prototype formats |
| P2-01/02/05/06/08 | Scoped context, actual OpenRouter capabilities/auth/retries/usage, full helper accounting, production coding/completion and history compaction; fixture success is not live-provider qualification |
| P2-03/04/07 | General trusted policy/confirmation, multi-file prepared effects, hostile path replacement, composed native sandbox and crash/partial-effect recovery |
| P3 | Actual VCP CLI, inspection, in-app pause/resume and deliberate workspace continuation; private qualification binaries are not a shipped UX |
| P5 | Production governance/ingestion, local asset setup, coherent index generations, retention/deletion and encrypted snapshot activation/key ceremonies |
| P6/P7 | Evaluated routing, optional OpenRouter Jev/advisory calls, skills/MCP and visible delegation through the same authority/accounting boundaries |
| P8 | Clean-machine install, complete enabled dependency/license inventory, common-toolchain release checks, measured product limits and release update rehearsal |

No paid model calls, private transcripts, customer inputs or real recovery keys
are needed for these P0 results. Historical hosted runs are evidence for their
stated source heads. Current changes use local native tests and standard fast
PR CI; no large hosted-runner configuration is restored.

## Final local validation

The retained native CLI build passed, exit 0, with Rust 1.98.0 (`88d9e12ae`),
MSVC 14.50.35717 and Windows 10.0.26200:

```powershell
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -Mode Build -ExperimentToolchain 1.98.0 -OutputRoot artifacts/build -TargetRoot artifacts/upstream/codex-target
```

Manifest: `artifacts/build/f25c5581-a94b-41ff-98f8-48d50436687d/manifest.json`.
The resulting binary SHA-256 is
`ae0ff48f4f8b3c71ffd7ec0f94a84f4a7d367171b6b9eeacf7263b254ad3b8a7`;
`codex.exe --version` exited zero with `codex-cli 0.0.0`. This remains the retained
upstream CLI, not the product executable. Cargo reported a future-compiler
compatibility warning for the unchanged `proc-macro-error2` 2.0.1 dependency;
the selected 1.98 build succeeded. Dependency updates remain separate work.
The [integration report](p0-08-09-integration.md#retained-native-regressions)
records the passing 29-test lifecycle and independent native recovery gates.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight registered
groups with Node 24.10.0 on native Windows, exit 0:
`artifacts/tests/2f8c0e2c-70cb-4f96-98a8-c2ae01762507/manifest.json`.
This checks repository links and task dependencies, identical agent guidance,
delivery/experiment contracts, source inventories, module boundaries and CLI
fixtures. The first restricted invocation could not compute Git source identity;
the unchanged checks passed with normal local Git access.
After the completion updates,
`pwsh -NoProfile -File scripts/test.ps1 -Suite repository` passed, exit 0:
`artifacts/tests/3303e4e5-4eb2-48c9-8a20-0d3038b0380c/manifest.json`.
`git diff --check` also passed, and `AGENTS.md` and `CLAUDE.md` are byte-identical.
These local results do not assert publication or remote CI success.
