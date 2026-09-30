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
The pinned editor archive keeps `Code.exe` at its root and runtime DLLs plus
`resources/app` below `7debcd0e2a`. The shared qualification resolver requires one
ordinary runtime directory with the pinned version and full commit, and selects
its CLI script and DLL search path. Portable contracts compare the extracted
executable, metadata and CLI against the exact downloaded archive without
launching Code or installing an extension.
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
Before removal, the workflow also runs the separately compiled
`beta_launcher_console` qualification target against the installed launcher and
native executable. Its eight cases cover Files/SQLite, Ctrl+C/Ctrl+Break and
direct/launcher execution at the read-only paused-task chooser. Each case uses a
new hidden console, sends no task selection, compares exit codes and canonical
state, and requires kernel-confirmed descendant termination without forced
cleanup. Explicit private fixture paths override the separately verified installed
default data path. This covers chooser cancellation, not interruption of active
provider work. The outer runner allows 1,200 seconds for the complete matrix and
payload checks; individual readiness, event-exit and cleanup bounds are recorded
in the result. Failed observations retain the registered installation and private
repair evidence instead of removing them during exception handling.

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

The separate ignored test
`final_installed_candidate_editor_lifecycle_preserves_both_stores` adds five
ordered observations through the same final setup/VSIX pair: real window reload
with observer-only restoration and native protocol `99.0` refusal; a new editor
process with explicit observer reconnect; rejection of a truncated copy of the
same final VSIX; missing-engine refusal; and final observer reconnect followed
by extension/native uninstall. It does not fabricate a successor version.
The Rust test reopens each canonical store between observations and compares all
records, accounting, events, commands and transactions with the initial state.
It also checks workspace/data/key sentinels and byte-identical installed
extension inventory. The synthetic fixture contains no real accounting work.
The initial installed payload must match every shipped VSIX file hash. Only the
root `package.json` installation `__metadata` object and JSON formatting are
normalized back to the package builder's emitted format; runtime fields and
dependency manifests remain bound. Later checks also require the original
installed bytes, including that metadata, to remain unchanged.

The existing candidate command runs both ignored tests serially. To select only
the lifecycle test from an already compiled qualification test executable, set
`VCP_BETA_NATIVE_RESULT`, `VCP_BETA_SETUP_RESULT`, `VCP_BETA_VSIX_MANIFEST` and
`VCP_TEST_CODE` to the final receipt paths and the pinned portable editor, then
run:

```powershell
& $TestExecutable --ignored --exact final_installed_candidate_editor_lifecycle_preserves_both_stores --nocapture --test-threads=1
```

This requires the same fresh registration-free user, normal Windows token,
PowerShell 7 and private fixture environment as the observer smoke. Node/VSCE
prepare a separate test driver; they are not editor runtime prerequisites.
`editor-lifecycle.ps1` verifies the complete installed production payload before
each case and binds the editor version, commit and executable hash. Failed runs
retain their private registered installation and repair logs. Successful output
contains the exact artifact hashes and observations; private profiles/stores
must not be uploaded. These are actual extension-host observations, not human
UI acceptance, reviewed-buffer qualification or a distinct-build upgrade.
Both editor tests supervise each runner in a job that disallows process
breakaway, cap its output at 4 MiB and its runtime at 600 seconds, and allow
45 seconds for the native owner's normal 30-second idle shutdown. A passing
observation requires zero remaining descendants without forced cleanup; a
failure retains the private supervision report and installation for repair.

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

The builder records capacity, available free bytes and total free bytes for each
fixed drive before and after every stage, including provisioning, production
compilation and qualification. GitHub documents 14 GB SSD storage for its
[standard Windows runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
The development workstation's existing qualification artifacts measured about
8.2 GiB, with about 1.2 GiB of source; this is an overlap warning, not a measured
fresh-candidate peak or a promised runner capacity margin.

After all three packages pass exact pair validation, the same `pair` stage
removes only the receipt's `<candidate>/build/<GUID>/cargo-target` tree before
qualification starts. It first checks successful packaging, the pair hash,
ordinary paths and every target entry, and the preserved copied executables and
PDB. Failed builds/pairing and redirected or unexpected paths are never cleaned.
Copied programs, symbols, receipts, compiler logs, source/cache inventories,
archives and dependency caches remain available to strict validation and evidence
collection. `run.json.production_target_cleanup` records the deleted path, file
count/logical bytes and observed change in volume free bytes; concurrent disk
activity can make that measured change differ from logical file sizes. No free
space threshold, automatic paid-runner change or hosted capacity guarantee is
introduced. Small Windows regression fixtures verify deletion scope and retained
sentinels, including failed-pair, mismatched-target and junction refusal.
