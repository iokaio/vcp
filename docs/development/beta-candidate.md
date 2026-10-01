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

Select `stop_after` explicitly when advancing beyond the default preflight:

| Selection | Last work performed | Qualification status |
| --- | --- | --- |
| `portable-contracts` (default) | Source/tool checks, pinned provisioning and portable contracts. | Incomplete; no production build. |
| `production-build` | Fresh optimized engine/launcher build and strict receipt verification. | Incomplete; no packaged pair. |
| `pair` | Native ZIP, registered setup, VSIX and independent exact pair validation. | Incomplete; installed tests remain unrun. |
| `installed-editor` | All eleven stages, including separate native qualification and installed native/editor observations. | Automated pipeline complete only if every stage passes; manual acceptance still required. |

Each stage is a named workflow step with its own deadline. Production and native
qualification compilation each allow 100 minutes; all selected stage limits sum
to at most 325 minutes within the 360-minute job. Setup consumes part of the
remaining time. Failure/cancellation collection is best effort and cannot survive
runner loss or an exhausted job budget. A new dispatch always starts fresh;
these checkpoints do not import a previous run's build or resume a canceled job.
The smaller selections bound the work attempted and make failures visible; they
do not reduce compilation time.

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
The workflow selects the owner-provisioned GitHub-hosted runner `vcpwin` in
`wingroup`: Windows Latest (2025), 16 cores, 64 GB RAM and 600 GB SSD. Production
and qualification compilation use 16 Cargo jobs. The native Windows qualification
and storage handoff jobs in [Delivery checks](../../.github/workflows/ci.yml)
use the same runner and compilation limit; Linux checks retain their platform
coverage. Stateful installed tests keep their existing serial execution. The
image selection is mutable; the packet records the actual image and OS version,
native compiler hashes and measured build resources. This changes build capacity,
not the supported Windows envelope or the clean-host qualification requirement.

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
  -OutputRoot artifacts/beta-candidate -Jobs 2 -StopAfter portable-contracts
node scripts/release/evidence.cjs artifacts/beta-candidate/run.json artifacts/beta-evidence
```

Use a new output directory. Local execution validates the exact clean selection;
the successful GitHub Delivery-run check belongs to the workflow. Do not use an
account with an existing registered VCP installation: smoke runners refuse to
replace that registration. The private fixture directories must remain outside
repositories and synchronized folders.

`-Stage all` is the local default and runs through the selected `-StopAfter`.
The local `-StopAfter` default remains `installed-editor`; specify a shorter
selection as above for preflight. The workflow invokes each `-Stage` separately
in the same job and output directory. Each continuation requires the exact
successful prefix, unchanged source, roots, selected scope, prior log/receipt
hashes and, after pairing, the same verified pair. It restores and verifies the
recorded physical tools, temporary directory and editor identity; qualification
stages initialize their own compiler environment. It refuses concurrent owners,
replayed stages, skipped prerequisites and failed/interrupted predecessors.
Atomic `run.json` replacement preserves the preceding checkpoint if a write is
interrupted. This is sequential execution within one run, not cross-run resume.

During Cargo execution, a supervised child tree emits numeric progress every
30 seconds: elapsed seconds, bytes read, seconds since new output and completed
compiler artifacts. Separate markers identify input verification, Cargo and
post-build verification. The complete compiler output remains in the retained
log, with stdout/stderr lines kept intact. The owned Windows Job prevents child
breakaway and terminates descendants when its owner exits; forced cleanup or an
unclosed process/output stream fails supervision. Hard termination can leave
the last progress snapshot marked `running`; it is never evidence of success.
Before cleanup, the supervisor records the owned process count, bounded process
identities and stdout/stderr completion states. Image queries recheck Job
membership through the opened process handle; diagnostics do not collect command
lines or environment variables. Query failures remain explicit and never change
the supervision result. The final Cargo progress snapshot and production receipt
retain these observations, and production errors preserve the original reason.

Production and native qualification explicitly select the telemetry executable
beside their selected MSVC compiler. These builds use a fresh PDB server endpoint
and `_MSPDBSRV_=-shutdowntime 0` in the child environment. After successful child
and broker exits and fully drained output, the supervisor may stop the one
remaining telemetry service only when its pinned executable and owned process
identity match. This is recorded as planned service cleanup, not natural exit.
Any unknown survivor, identity mismatch or failed cleanup still fails the build;
the owned Job must be empty before success. Parent environment and other builds'
processes are unaffected. A short Windows Delivery check exercises these rules
and a real synthetic MSVC compile/link on `vcpwin` before candidate dispatch.

`build-progress.json` and `native-qualification-progress.json` also record
cumulative Job user/kernel CPU seconds, peak
committed memory and logical processor count. Measurements include the broker,
Cargo and its descendants; unavailable values are null. Compare CPU growth over
elapsed time and memory demand before choosing a different runner. Artifact
counts and quiet periods are observations, not percentages, proof of a stall or
predictions of time remaining. No runner size, Cargo job count or cache policy
changes automatically.

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
The console, editor and helper qualifiers keep canonical paths for candidate
identity and containment, but pass interpreters equivalent ordinary absolute script paths.
Node's CommonJS entry resolver rejects the Windows extended prefix returned by
Rust canonicalization; PowerShell also propagates that prefix into derived Node
script paths. Payload-verifier failures report bounded exit/tree
details and fixed reason categories; raw private output remains local.
After a successful uninstaller exit, smoke runners observe registration and
program-directory removal for up to ten seconds because Inno can finish its
owned cleanup after the initial process returns. Persistent leftovers still
fail; the runner never deletes them, and retained-data comparisons still apply.
The editor observer likewise uninstalls only after a successful observation;
setup or observer failures preserve its registered installation and private evidence.

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
Each observation also emits a bounded diagnostic summary into the captured
stage log. It records process completion, output lengths and hashes, fixed
failure categories and allowlisted observation fields. Raw private output,
fixture paths, task contents and keys are excluded; an unknown failure remains
unknown rather than being inferred from missing output.

The separate `beta_editor_refusals` qualification target can observe additional
refusals against the same final native/setup/VSIX bytes, without rebuilding or
changing the candidate. It records its own executable, source-helper and driver
hashes separately from the artifact commit. With the same artifact/editor inputs
above, absolute `VCP_TEST_NODE`, PowerShell 7, and optional fresh empty private
`VCP_BETA_EDITOR_REFUSALS_OUTPUT`, run its compiled test executable:

```powershell
& $RefusalTestExecutable --ignored --exact final_installed_candidate_editor_refusals_preserve_both_stores --nocapture --test-threads=1
```

For each store, separate profiles verify actual restricted mode, denied trust
and editor commands, uninitialized/wrong-data/unselected-folder refusals, and
trusted dirty drafts invalidated by real typing, undo and close/reopen. The
trusted profile seeds only disposable editor trust preferences; native trust is
granted through the actual explicit command, which must return to observer mode
before control is explicitly reacquired. No execution profile, live task binding
or provider is supplied. The test requires zero prepare/dispatch/result/start RPCs, no
source disk changes, unchanged task/accounting and unrelated records, preserved
acknowledged history, and only the exact trust/controller authority changes.
Normal background context refreshes are counted separately and must all refuse
because the fixture has no tool policy; timers and RPC results are not replaced.
It checks canonical state before uninstalling; failures retain private state and
the registered installation. `result.json` records the observations and hashes;
private stores, profiles and raw logs must not be uploaded. This is stale-draft
refusal and buffer preservation evidence, not successful review/apply, partial
application, undo-before-receipt, paid-task or human UI acceptance. This separate
target is not automatically included in the candidate workflow.

`run.json` records exact stage commands, expected observations, timestamps,
outcomes and log locations. Evidence collection retains final artifacts, build
and source/cache receipts, the exact pair, sanitized command logs and a checksum
list. It excludes Cargo target trees, compiler caches, editor profiles, stores,
keys, fixture configuration and raw private editor logs. Known environment secret
values and common credential forms are redacted from retained logs; receipts
containing an active environment credential are rejected. A receipt records both
original and retained log hashes when sanitization changes bytes.
The exact root-level `native-qualification.log` is retained through the same
sanitizer, including final output drained after the console mirror stops.

`evidence.json` always uses `qualification-required`. Its `selection_status`
reports whether the exact selected prefix and required receipts passed. An early
checkpoint can succeed while `pipeline_status` remains `incomplete`; only all
eleven successful stages and the verified pair can pass the full pipeline. The
collector exits successfully for a valid successful selection. Its `pipeline_status` is
separate from the eleven-area acceptance matrix in the release plan. Only the
production-identity row can be filled automatically by the exact artifact build;
the other areas remain `not run`, even when related synthetic/source tests pass.
Missing prerequisites and interrupted stages are explicit. Invalid final hashes
fail evidence validation while keeping readable failure logs. Nothing assigns
`excluded from declared support` automatically; exclusions need the documented
support decision. Unauthorized effects or preservation failures remain stop
conditions.

An interrupted production build can retain sanitized `source-before.json`,
`dependencies-before.json` and `build-progress.json` from its exact build
directory before a final build receipt exists. These diagnostics are explicitly
excluded from success evidence. Malformed diagnostic files are recorded as
validation failures while valid neighboring diagnostics remain available.

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

The separate `beta_helper_candidate` qualification target observes installed
helper bytes after setup has been independently installed. It does not install
or uninstall VCP. Set these absolute paths before explicitly running the ignored
`final_installed_helpers_preserve_payload_and_drain_process_tree` test:

| Variable | Required input |
| --- | --- |
| `VCP_BETA_NATIVE_RESULT` | Final strict native `result.json`, beside its ZIP. |
| `VCP_BETA_INSTALLED_EXECUTABLE` | That registered installation's `engine/releases/<archive-sha256>/vcp.exe`. |
| `VCP_TEST_PYTHON` | Existing isolated Python 3.11.9 virtual environment executable with the shipped PDF/spreadsheet requirements and test-only `xlsxwriter` already installed. |
| `VCP_TEST_NODE` / `VCP_TEST_PWSH` | Ordinary absolute Node and PowerShell 7 executables. Resolve version-manager links first. |
| `VCP_TEST_BROWSER_PROJECT` | Existing project with its lock, local Playwright 1.63.0 packages, and already cached matching Chromium headless shell. |
| `VCP_BETA_HELPER_OUTPUT` | Fresh private output directory outside any checkout and synchronization tree, with an existing parent. |
| `VCP_TEST_SYNC_ROOT` | Optional additional synchronization root to exclude. Known OneDrive roots are also excluded. |

Compile with the existing Windows qualification toolchain and
`--test beta_helper_candidate --features qualification --no-run`. Invoke that
exact test executable with `--ignored --exact
final_installed_helpers_preserve_payload_and_drain_process_tree --nocapture`.
The ordinary `helper_supervision_refuses_retained_descendants_and_output_overflow`
test needs only `VCP_TEST_NODE` and exercises owned synthetic child processes;
it does not provide final helper qualification evidence.

The installed test requires the matching HKCU setup registration, verifies the
complete native archive and installed payload before and after observations,
and binds launcher bytes, active selection and ownership metadata. It runs four
PDF cases, five spreadsheet cases and seven MCP pagination cases using the
existing source test harnesses with `VCP_SKILLS_ROOT` selecting installed helpers.
It runs the installed authoring validator for every catalog entry, retaining all
warnings, and the installed browser helper against two owned loopback origins:
Save changes the expected text to Saved, while a request to the other origin
must be refused before that server receives it. It records the actual browser
launch options, including the requested Chromium sandbox. This is a renderer
origin-boundary check, not an OS network-denial observation. Axe accessibility
scanning remains `not-run`; an absent `axe-core` is recorded as a missing optional
prerequisite. There is no dependency acquisition or provider/model call.

The wrapper clears inherited credentials, uses private temporary files and
disables Python bytecode writes. Receipts hash the independent qualification
executable, source harnesses, actual installed helper inventory, Python runtime
and virtual environment, Node, Playwright and browser files before and after.
For Windows Store Python, the launch alias is recorded separately from the
ordinary loaded process image, base DLLs and standard library; alias bytes are
not represented as executable provenance. These are exact observed dependency
bytes, not an independent audit of their upstream supply chain.

The runner has a 900-second outer deadline and 4 MiB combined outer stdout/stderr
limit, with bounded individual case commands. All descendants enter a Windows
job before execution. Pass requires natural completion, zero remaining job
processes and no forced cleanup after a five-second grace; failures use bounded
cleanup and remain failures. Private `result.json` uses
`vcp-installed-helper-qualification/1`; its nested raw observation receipt alone
does not establish process-tree completion. Retain private outputs and the
installation on failure. Do not upload raw paths, fixtures or logs: produce a
sanitized, hash-bound evidence summary after review. This target does not claim
first-run onboarding, clean-host acceptance, skill materialization through a
provider, a broader skill campaign, or BETA-09 completion, and is not included
automatically in the candidate workflow.

### Installed configuration refusals

The independent `local_execution_parity` qualification target also contains an
ignored final-engine refusal test. Compile that target with `--features
qualification --no-run`, record its executable SHA-256, then use the bounded
[configuration runner](../../scripts/release/config-refusals.ps1):

```powershell
pwsh -NoProfile -File scripts/release/config-refusals.ps1 `
  -NativeResult <absolute-final-native-result.json> `
  -InstalledEngine <absolute-installed-versioned-vcp.exe> `
  -Node <absolute-ordinary-node.exe> `
  -QualificationExecutable <absolute-local_execution_parity-test.exe> `
  -QualificationSha256 <recorded-test-executable-sha256> `
  -OutputRoot <fresh-private-directory>
```

Run this only after independently verifying the full candidate packet and
installing its matching setup. The enclosing qualification record must bind the
setup, native ZIP and VSIX pair. This runner separately verifies the strict native
package, registered installation, launcher selection and complete installed
payload before and after. It records the qualification executable and source
helpers independently from the candidate's reviewed source. The selected Node
and the runner's actual PowerShell executable are hash-bound before and after;
the synthetic imported MCP peer uses that explicit PowerShell host. Source
qualification wrappers supply `VCP_TEST_NODE` and `VCP_TEST_PWSH` explicitly.

For both Files and SQLite, unchanged CLI/start/resume controls must reach budget
exhaustion with no attempt, reservation or send intent. The synthetic profile
allows one microUSD while its minimum request quote is 100 microUSD; it supplies
no live credential or qualification transport. Twenty-four changed-configuration
cases cover base edits, revision reselection, revision content changes, corrupt
history, removed revisions and first imports, through start and resume. Invalid
persisted base/history also exercises CLI and reconnect refusals. The observed
MCP marker must remain absent and rejected commands must not be admitted.

The test runs under the existing Windows Job supervisor with a 300-second
deadline. Pass requires exactly the selected ignored test, all expected control
and refusal observations, natural child completion and zero remaining processes.
Output and synthetic fixtures remain in the private directory on failure. Known
OneDrive roots are excluded; supply other synchronization locations with
`-SyncRoots`. Do not upload raw fixtures or logs.

These observations cover stale-configuration admission on final bytes. Actual
MCP allowlist/deadline execution still requires a separately admitted provider
task under the shipping transport contract. The runner does not establish an OS
network-denial policy, a clean host, editor UI parity or full BETA-09 acceptance.
