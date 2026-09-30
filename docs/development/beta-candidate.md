# Internal beta candidate workflow

BETA-08 owns [the dispatch-only workflow](../../.github/workflows/beta-candidate.yml).
It constructs reviewable unsigned internal-beta artifacts and records observations.
It does not publish a release, acquire models, configure a provider credential,
authorize inference or grant owner acceptance.

Dispatch from `main` with `reviewed_commit` equal to the exact 40-character commit
selected for review. The workflow requires a successful `Delivery checks` push run
for that same `main` commit. An explicit selection is required; mutable branch
ancestry does not establish review. The workflow's read token is scoped to the
check lookup and is not passed to builders or installed processes.

[Candidate tool pins](../../release/candidate-tools.json) fix Node 24.10.0,
Rust 1.95.0 for production, Rust 1.98.0 for qualification and the official VS Code
1.138.0 Windows x64 archive URL/checksum. The editor pin is from the recorded
Microsoft version metadata endpoint. Inno Setup is pinned in the channel record.
`windows-2025` is a standard hosted image label, not an immutable Windows image;
the packet records the actual image and OS version and native compiler hashes.
It does not expand the supported Windows envelope.

The build provisions npm locks without install scripts and explicitly runs
`cargo +1.95.0 fetch --locked --target x86_64-pc-windows-msvc` before the offline
production recipe. No compiled Cargo target cache is restored. Production uses a
new target directory. Qualification uses a different target directory and never
supplies its executable to packaging. The strict recipe checks original locked
archives, extracted registry files and pinned Git checkouts before and after
compilation. A cache change fails the build.

The orchestrator is also available for a controlled Windows builder after the
three npm development lockfiles have been installed:

```powershell
pwsh -NoProfile -File scripts/release/candidate.ps1 `
  -ReviewedCommit <reviewed-40-character-main-commit> `
  -OutputRoot artifacts/beta-candidate -Jobs 2
node scripts/release/evidence.cjs artifacts/beta-candidate/run.json artifacts/beta-evidence
```

Use a new output directory. Local execution validates the exact clean selection;
the successful GitHub Delivery-run check belongs to the workflow. Do not use an
account with an existing registered VCP installation: smoke runners refuse to
replace that registration. The private fixture directories must remain outside
repositories and synchronized folders.

The sequence builds the production engine/launcher, native payload, registered
setup and actual beta VSIX. VSIX construction receives the original
`--build-receipt` alongside the native result, so the full compiler, dependency,
upstream and tool evidence is revalidated before packaging. Independent pairing requires all three final
artifacts and their actual SHA-256 values. Then separate native tests exercise
import parity and launcher boundaries. The setup smoke installs final bytes with
Unicode/spaced program and data roots, checks registered removal, exact launcher
resolution and preserved synthetic data, and exercises both storage preferences.

The ignored `beta_editor_candidate` integration test seeds the existing offline
paused-history fixture for Files and SQLite, then installs the final setup and
VSIX outside the checkout. Its separate test extension explicitly writes User
engine/data settings, activates the installed VSIX and observes the paused task.
The engine and editor children have a restricted runtime PATH and no provider
credential environment. No test extension is included in the shipping VSIX.
The runner disconnects and waits for the observer's normal idle grace before
uninstalling; the native test then verifies the real retained state remains
paused. These are synthetic retained-state observations. They do not establish a
new user's first useful task, reviewed editing, full editor lifecycle or a
distinct-build upgrade/rollback.

`run.json` records exact stage commands, expected observations, timestamps,
outcomes and log locations. Evidence collection retains final artifacts, build
and source/cache receipts, the exact pair, sanitized command logs and a checksum
list. It excludes Cargo target trees, compiler caches, editor profiles, stores,
keys, fixture configuration and raw private editor logs. Known environment secret
values and common credential forms are redacted from retained logs; receipts
containing an active environment credential are rejected. A receipt records both
original and retained log hashes when sanitization changes bytes.

`evidence.json` always uses `qualification-required`. Its `pipeline_status` is
separate from the eleven-area acceptance matrix in the release plan. Only the
production-identity row can be filled automatically by the exact artifact build;
the other areas remain `not run`, even when related synthetic/source tests pass.
Missing prerequisites and interrupted stages are explicit. Invalid final hashes
fail evidence validation while keeping readable failure logs. Nothing assigns
`excluded from declared support` automatically; exclusions need the documented
support decision. Unauthorized effects or preservation failures remain stop
conditions.

The workflow retains the complete hashed packet for 90 days, including failure
runs. Export those exact bytes to approved durable storage before expiry and
verify `SHA256SUMS`; retention in GitHub is not permanent storage. The manually
completed matrix must cite this packet's pair ID, actual artifact digests, OS,
editor/runtime versions, store, commands, expected/actual results and limitations.
A changed artifact creates a new pair and requires affected requalification.
Clean standard-user Windows, live provider admission, independent-machine/full
volume recovery, performance envelope and owner evaluation remain BETA-09 work.
Owner acceptance and any distribution/publication authorization remain BETA-11.
