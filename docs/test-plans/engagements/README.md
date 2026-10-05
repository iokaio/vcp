# E-A / E-B engagement runner

This EE-07 runner uses the existing scenario harness and ordinary VCP run/pause/resume commands. It extends **both independently passing fresh A/B bases**; it never fabricates a base, relaxes an original gate, or treats an unrun browser/SQL check as passed. It copies the chosen retained checkpoint into a new workspace and keeps run logs, provider state, original checkpoints, declared fault bytes and databases. It performs no cleanup.

Create an owner input JSON file outside the model workspace:

```json
{
  "schema": "vcp-engagement-input/1",
  "source_revision": "<40-character candidate source commit>",
  "candidate": {"executable": "D:/.../vcp.exe", "sha256": "<actual SHA-256>", "version": "0.2.26"},
  "A": {"scorecard": "D:/.../results/scorecard.json", "scorecard_sha256": "<SHA-256>", "checkpoint": "D:/.../checkpoints/.../manifest.json", "checkpoint_sha256": "<SHA-256>"},
  "B": {"scorecard": "D:/.../results/scorecard.json", "scorecard_sha256": "<SHA-256>", "checkpoint": "D:/.../checkpoints/.../manifest.json", "checkpoint_sha256": "<SHA-256>", "database": {"server": "(localdb)\\MSSQLLocalDB", "name": "<original baseline database>"}}
}
```

The executable must have an adjacent `build-evidence.json` with schema `vcp-execution-diagnostic-build/1`, completed status, stable source, matching actual version/hash/length and source commit. Only whitelisted identity fields enter the run manifest; source commit is labeled as declared by the build receipt. Both source scorecards must include the original T1–T6/native/explicit-resume and final gate identities, pass all latest required gates, and identify fresh, non-dry-run execution. All retained checkpoint files and original workspace files are rehashed, including protected files. `base-contract.cjs` declares the required original gate inventory; its correspondence with trusted scenario source is checked offline.

Validate prerequisites without starting inference, apps or SQL:

```powershell
./docs/test-plans/engagements/run.ps1 -Kind A -InputManifest D:/runs/engagement-input.json -ProjectPath D:/clitests/engagement-a -RunRoot C:/vcp-scenarios -ValidateOnly
```

After prerequisite runs and execution authorization are available, remove `-ValidateOnly` and supply `-ProviderGeneration <qualified-directory> -AllowProcessPublish`. `ProjectPath` must not exist. Execute A and B separately with different new project paths. The existing explicit process-profile authorization applies; no new tool permission is introduced. Provider request/retry/capacity bounds remain in the native path. A and B stage checks can use the existing bounded repair loop; uncertain outcomes still stop dispatch.

The exact original acceptance functions and cumulative named test requirements are extracted from the repository scenario scripts with an explicit AST allowlist. Their top-level scaffolding and paid workflows are never executed by extraction. New workload/API/UI shapes are specified in [contract-A.md](contract-A.md) and [contract-B.md](contract-B.md); original acceptance is unchanged. All executed harness/check sources and prompt contracts are hashed in `results/run-inputs.json`.

The runner performs a real application process restart around persistence/idempotency checks; actual Chromium keyboard/download/import/form/error/history checks; an edit followed by source-matching verification before explicit VCP pause; two credential-denied read-only owner reopen inspections with unchanged watermark/workspace; and explicit same-task resume. The model supplies a **data-only** declaration locating a bounded production validation guard. The harness first passes its own negative oracle, retains original/injected bytes, performs exactly that declared splice, requires a real successful response to invalid input, and then requires the unchanged oracle to pass after repair. A build/transport failure cannot substitute for the intended validation defect.

E-B requires installed `dotnet`, the project's pinned EF tool, SQLCMD, and a local integrated-auth SQL Server matching the protected baseline configuration. It creates a uniquely named COPY_ONLY backup/restore database and a second fresh database; it never drops/replaces an existing database. All ordinary dotnet/app processes use the clone connection override. SQL evidence includes original table rows and object/column/index/definition/foreign-key metadata, migration preservation, globally enabled/unfiltered unique operation-ID enforcement, and API/audit/stock agreement. Original database row/table/schema snapshots must remain unchanged. Remote databases and credential-bearing connection strings are rejected. Missing tools, DB access, browser binaries, protected-byte changes or incomplete evidence fail the run.

Offline qualification:

```powershell
node --test docs/test-plans/engagements/prerequisites.test.cjs docs/test-plans/engagements/acceptance.test.cjs
./docs/test-plans/engagements/contracts.test.ps1
node --test docs/test-plans/engagements/browser.test.cjs
```

Browser tests use the existing pinned `src/tests/skills/browser` Playwright dependency. If installation is needed, use its lockfile and set `PLAYWRIGHT_SKIP_BROWSER_GC=1` before installing the pinned Chromium revision, preserving all existing browser cache revisions. Synthetic reports/screenshots explicitly identify themselves as oracle qualification, not completed engagements. Real E-A/E-B execution, SQL backup/restore/migration and native pause evidence remain unverified until the runner is executed against actual accepted bases. The run emits the ordinary scorecard plus `engagement-matrix.json`; a failure retains all completed evidence and does not claim subsequent stages were tested.
