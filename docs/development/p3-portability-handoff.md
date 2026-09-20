# U04 actual OneDrive handoff: operator runbook

Status: **prepared, not executed**. A second disposable Windows environment and
operator confirmation of the real OneDrive arrangement are still pending. No
actual provider transfer or two-machine result is claimed by this document or
by the local process-kill/sync-simulator tests. This is the remaining external
qualification step for declared OneDrive support in P3-06/P5-09/P5-10.

Use the exact reviewed VCP build on both machines. Record its commit and executable
SHA-256, `vcp-signed-age/1` envelope format, and the build's qualified `age 0.11.2` /
`ed25519-dalek 2.2.0` dependency identities (verify against that build's lockfile). Run the complete A → B → A procedure twice with separate disposable
workspaces, lineages, vault subdirectories and local data directories:

| Campaign | Source A backend | Restored B backend | Return A backend |
|---|---|---|---|
| `u04-sqlite-files` | SQLite | Files | SQLite |
| `u04-files-sqlite` | Files | SQLite | Files |

Restore's neutral import supplies both backend directions. Also run the explicit
`storage migrate` preview/apply check below; do not equate copying backend files
with conversion. Keep A and B as separate Windows installations, not two folders
or two processes in one installation.

## Prerequisites and local evidence

- Use a synthetic Git workspace with tracked, staged, dirty and untracked files;
  include a unique **nonsecret** source marker. Seed a reviewed VCP task fixture
  containing activity, at least one governed claim, child history, verified checks
  and nonzero accounting. Retain explicit pending work for reconciliation checks.
  Creating this fixture through a model requires a separately approved profile
  and budget; this runbook never starts paid work implicitly.
- Identify the real OneDrive root on each machine, signed into the intended test
  account. Precreate a dedicated empty vault subdirectory there. Confirm the
  desktop client is running. Record its version and observed synchronization
  status separately on each machine. Do not copy objects manually between local
  directories and label that as provider transfer.
- Precreate private local data, staging, enrollment and evidence directories
  outside **all** known/declared synchronization roots. Recovery material belongs
  in an independently selected recovery directory outside workspace, data,
  staging, trust and synchronization roots. Supply B's recovery copy through the
  independent recovery channel; do not clone A's credentials or developer-trust
  directory. Never place recovery files in the vault.
- Close workspace owners before migration and restore. Use a new absent restore
  destination with an existing parent; existing roots are not overwritten. Keep
  original A and the prior selected canonical root until all checks complete.
- Use PowerShell 7.4 or later, an absolute trusted `vcp.exe` and absolute trusted `git.exe`.
  Pass key **paths**, never literal secret contents. Do not enable shell transcript
  capture around secret enrollment. Keep raw inspection output private; publish
  only allowlisted summaries, counts and comparison results.

On each machine set `$Vcp`, `$Git`, `$Data`, `$Work`, `$Vault`, `$SyncRoot`,
`$Staging`, `$RecoveryDir`, `$EvidenceDir`, and `$Commit` to explicitly reviewed
locations/values. `$Work` exists for ordinary commands. For fresh B key enrollment
use an existing empty `$EnrollmentWork`; `$RestoreWork` must not yet exist.

This helper extracts the production JSONL command result and fails on rejection:

```powershell
function Invoke-Vcp([string[]]$Arguments) {
    $lines = & $Vcp @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'VCP command rejected; inspect private local diagnostics.' }
    $rows = @($lines | ForEach-Object { $_ | ConvertFrom-Json })
    $result = @($rows | Where-Object { $_.type -eq 'result' -and $_.PSObject.Properties['data'] })
    if ($result.Count -ne 1) { throw 'Expected one structured command result.' }
    $result[0].data
}
$Base = @('--format','jsonl','--data-dir',$Data,'--workspace',$Work)
```

Store command results only under `$EvidenceDir`. A preview is never a completed
restore or mutation; its reported authentication state must be read explicitly.
Run the read-only path preflight on each machine before key/publication work:

```powershell
Invoke-Vcp ($Base + @('doctor','--vault',$Vault,'--staging',$Staging,
    '--sync-root',$SyncRoot))
```

Doctor checks declared/known root overlap and reparse boundaries. It does not
claim universal detection of every third-party synchronization application.

## A: enroll, capture the source and publish

1. Obtain the stable workspace ID from the selected fixture's public CLI output.
   Capture private baseline pages using `history list --task TASK --limit 128`,
   `memory inspect CLAIM`, `inspect TASK --view costs --limit 128` and
   `inspect TASK --view chain --limit 128`. Follow every returned cursor. Record
   source file SHA-256/length, IDs, claim revisions/statuses, ledger charge and
   liability totals, required checks, pending work, and deletion epoch. A later
   owner/rebind changes authority and current task state intentionally; compare
   retained history at the captured cut rather than hashing entire current state.
2. Create independently recoverable keys and configure the real vault:

```powershell
$enrollment = Invoke-Vcp ($Base + @('backup','keys','create',
    '--recovery-dir',$RecoveryDir,'--sync-root',$SyncRoot))
$WorkspaceId = $enrollment.configuration.workspace
$Lineage = $enrollment.configuration.lineage
$Key = $enrollment.recovery_copy
$PreA = Join-Path $EvidenceDir 'trusted-before-A.json'
$enrollment.configuration.checkpoint | ConvertTo-Json -Compress |
    Set-Content -LiteralPath $PreA -Encoding utf8NoBOM
Invoke-Vcp ($Base + @('backup','configure','--vault',$Vault,
    '--staging',$Staging,'--sync-root',$SyncRoot,'--manual-only'))
$OperationA = [guid]::NewGuid().ToString()
Invoke-Vcp ($Base + @('backup','create','--key',$Key,'--git',$Git,
    '--operation',$OperationA))
Invoke-Vcp ($Base + @('backup','status'))
$AfterA = Invoke-Vcp ($Base + @('backup','keys','verify','--key',$Key,
    '--sync-root',$SyncRoot))
```

The standalone create command waits for local completion. A live owner returns
in-memory preparation progress promptly; poll `backup status` until the matching
canonical job is `published` and inactive. Do not count preparation as a saved
snapshot. The opaque object is `$OperationA.age`. Record the **postpublication**
trusted checkpoint's sequence/deletion for evidence, while preserving `$PreA`
for B's independent prepublication trust floor.

3. Run the collector below on A and independently convey its expected ciphertext
   SHA-256/bytes, workspace ID, lineage, public recipient/writer identity and
   `$PreA` to B. Convey the verified recovery copy separately. Do not obtain these
   trust anchors from decrypted archive metadata or a cloud filename.
4. Observe the actual OneDrive client finish uploading the object. Record UTC
   observation time and provider status. Local `published` is not that observation.

## B: hydrate, independently enroll, restore and inspect

Wait for OneDrive to deliver the exact object. Request full local hydration and
run the collector with A's independent expected hash/length. Matching ciphertext
proves the observed bytes agree, not that archive authentication or restoration
has succeeded. The collector pins vault ancestors and the read handle, allowing only ordinary
objects or the Windows Cloud Files reparse-tag family. Junctions, symbolic links
and unknown reparse tags remain rejected; evidence directories permit no reparse
ancestors. An unsupported provider boundary rejection is an explicit
qualification failure to investigate; do not replace real provider behavior with
a simulated copy to pass the campaign.

Enroll the independently supplied recovery and prepublication checkpoint using
an existing empty enrollment workspace:

```powershell
$Enroll = @('--format','jsonl','--data-dir',$Data,'--workspace',$EnrollmentWork)
$Imported = Invoke-Vcp ($Enroll + @('backup','keys','--workspace-id',$WorkspaceId,
    'import','--key',$Key,'--lineage',$Lineage,'--checkpoint',$PreA,
    '--sync-root',$SyncRoot))
# Compare Imported.configuration.selected and lineage to independent A metadata.
$RestoreBase = @('--format','jsonl','--data-dir',$Data,'--workspace',$RestoreWork)
$RestoreArgs = @('restore','--workspace-id',$WorkspaceId,'--source',
    (Join-Path $Vault "$OperationA.age"),'--key',$Key,'--staging',$Staging,
    '--backend',$DestinationBackend,'--sync-root',$SyncRoot)
$Preview = Invoke-Vcp ($RestoreBase + $RestoreArgs + @('--preview'))
$Apply = $RestoreBase + $RestoreArgs + @('--operation',$Preview.operation,
    '--ciphertext-sha256',$Preview.ciphertext_sha256,'--bytes',([string]$Preview.bytes))
if ($Preview.expected_descriptor) { $Apply += @('--expected-descriptor',$Preview.expected_descriptor) }
$Restored = Invoke-Vcp $Apply
```

Use the checkpoint **before** the incoming snapshot. Importing its already
advanced checkpoint would correctly reject that same snapshot as a replay.
Fresh-host trust cannot prove a globally newest cloud head; independently verify
the selected object and expected sequence before accepting this limitation.

After success, use `$Work = $RestoreWork` and rebuild `$Base`. Confirm:

- Preserved workspace/session/task/claim IDs and retained events match A's cut;
  redactions remain explicit, and charge/liability totals are preserved.
- Source bytes, including dirty/untracked content, match the source checkpoint.
  `.git` is not recreated from the archive. Original Git index/diff/status bytes
  remain evidence; their presence does not grant permission to execute hooks.
- Local authority was invalidated, tasks remain paused, and execution is
  Untrusted/Plan until explicitly reviewed. No model request starts on restore.
- Search reports actual compatible-generation readiness or explicit rebuild
  requirements. Compare source IDs/evidence links after any qualified local
  rebuild. Rebuilt readiness may cover canonical claim chunks only; historical
  source chunks need a current root/Git recapture after rebind. Do not report all
  history indexed merely because a lexical component opened successfully.
- Repeating the exact apply operation is an idempotent recovery, not a fresh
  authority grant. Wrong key, changed ciphertext/length, unavailable hydration,
  untrusted writer or older snapshot fails without replacing active state.

## B → A: create and restore a genuine descendant

Before a new backup, explicitly review B's restored workspace, local tools and
execution policy. A native checkpoint requires trusted read/list/search/exec
capabilities and a trusted Git installation. Restore intentionally grants none.
The maintenance-only `workspace trust` command changes only local workspace
trust, preserves Plan policy and existing denials, and neither loads a provider
profile nor resumes tasks. Do not start model work to bypass a rejected boundary.

For the synthetic fixture only, establish fresh local Git metadata from reviewed
source content (or an independently trusted repository), retaining the restored
Git evidence unchanged. Do not execute restored hooks/configuration. Review the
new Git identity, rebind it explicitly, then enroll local trust at that exact
workspace revision:

```powershell
$Reviewed = Invoke-Vcp ($Base + @('workspace','rebind',$WorkspaceId))
Invoke-Vcp ($Base + @('workspace','trust',$WorkspaceId,
    '--expected-revision',([string]$Reviewed.workspace_revision)))
```

A stale revision or denied tool/root remains a rejection to reconcile locally;
this command does not remove denials. Add an explicit new synthetic source change
and a reviewed canonical event, then verify
B's local state and checkpoint. Configure B's own private staging and its real
OneDrive vault with `backup configure ... --manual-only`; configuration is local
and is not adopted from the archive. Publish with a fresh operation ID using the
same explicit key/Git arguments. Observe B's upload and A's actual download.

On A, preserve the original source directory and restore B's new object to a new
absent `$ReturnWork`, using A's existing independent trust journal and recovery
copy. Repeat the restore preview/apply sequence with the return backend. A's
checkpoint from its prior publication is the parent floor for B's descendant.
Compare the original immutable history plus B's new event/source change. No
in-flight task, execution grant or credential cache may be restored as authority.

Exercise explicit local conversion with owners closed:

```powershell
$Migration = Invoke-Vcp ($Base + @('storage','migrate','--backend',$OtherBackend,'--preview'))
Invoke-Vcp ($Base + @('storage','migrate','--backend',$OtherBackend,
    '--operation',$Migration.operation,'--expected-descriptor',$Migration.expected_descriptor))
```

Reopen and compare the preserved history/ledger again. Both directions must pass
across the two campaigns; preserve retained recovery roots until review ends.

## Read-only observation collector

`scripts/qualification/p3-portability-handoff.ps1` reads one named ciphertext
(up to 65 MiB, matching the ciphertext restore ceiling) and writes a new allowlisted local JSON evidence file. It never
publishes, decrypts, restores, deletes, reads recovery secrets, or declares U04
passed. It records a campaign-salted machine pseudonym, platform/client versions,
filesystem, backend, hash/bytes, and operator-supplied sequence/deletion. Sequence
and deletion are independently checked against CLI trust/job evidence; the
collector cannot infer authenticated inner metadata from ciphertext.

```powershell
& .\scripts\qualification\p3-portability-handoff.ps1 `
  -Campaign u04-sqlite-files -Machine A -Phase a-published -Backend sqlite `
  -Commit $Commit -Vault $Vault -Object "$OperationA.age" `
  -Evidence (Join-Path $EvidenceDir 'a-published.json') `
  -Sequence $AfterA.configuration.checkpoint.sequence `
  -Deletion $AfterA.configuration.checkpoint.deletion `
  -OperatorConfirmsActualOneDrive
```

For B hydration, set `-Machine B -Phase b-hydrated -Backend files` and supply
`-ExpectedSha256 HASH -ExpectedBytes N -WaitSeconds 300`. Repeat for `b-published`
and `a-hydrated` with the descendant. Waiting bounds availability polling; OS
hydration/read latency remains an observed platform dependency. Compare distinct
machine pseudonyms and both pairs of checksums. The provider assertion is an
operator assertion, not independent proof of delivery. Retain separate redacted
provider observations and restore/semantic comparison results.

## Evidence required before changing pending to qualified

For each campaign preserve: build/executable hashes; distinct Windows machine
pseudonyms; Windows/filesystem/OneDrive versions; source/destination backends;
opaque object identity, hash and bytes; sequence/deletion/parent linkage;
upload/download UTC observations; full authenticated restore success; immutable
ID/history/claim/accounting/source comparisons; paused/no-authority state;
search readiness; both conversion results; and negative-test outcomes. Publish
only redacted summaries. Keep absolute paths, raw history, manifests, source
markers and recovery material out of public reports and the cloud vault.

The broader U04 matrix still requires tamper, writer substitution/replay,
traversal/expansion, crash boundaries, rotation/key loss, offline divergence and
prune races. Link the exact native qualification logs/build used; do not substitute
this checksum collector for those tests. Process kills are not power-loss proof.
A missing second machine, unavailable real sync, unsupported placeholder behavior,
or incomplete B→A admission leaves the applicable acceptance **pending**.
