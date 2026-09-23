# P8-05 owner acceptance packet — 2026-09-22

**Disposition: NOT APPROVED.** This packet prepares owner review of the existing
unsigned candidate. It does not record an owner task run, acceptance, or permission
to publish. P8-06 is complete; P8-01 through P8-05 remain open. The owner directed
current-workstation validation and excluded machine handoff from this continuation;
the corresponding release requirement remains not run.

Read this with the [release scorecard](p8-release-scorecard-2026-09-22.md) and
[acceptance gaps](p8-acceptance-gaps-2026-09-22.md). Those records join current
evidence and remaining requirements; this packet supplies the owner procedure and
uncompleted acceptance form. Authority: [P8-05](../plan/15-integration-and-release.md#p8-05--owner-acceptance-and-release-evaluation),
[U01–U09](../plan/16-test-fixtures-and-acceptance.md#u01--architecture-aware-analysis),
and [ADR-018](../adr/018-release-acceptance.md).

## Candidate under review

The table and commands below preserve the historical debug candidate. The
[production qualification supplement](p8-production-qualification-2026-09-22.md)
identifies the new optimized artifact and its exact receipts. Use that artifact
identity for production review; do not transfer the debug candidate's identity
or approval form to it. The supplement also links the proposed new owner tasks
and numeric thresholds. Human acceptance remains unrecorded.

| Item | Bound identity |
| --- | --- |
| Local package directory | `artifacts/p8-local-distribution/3f7a6bec-a374-487a-8d0a-06134e079e78` |
| Archive | `vcp-windows-unsigned.zip` |
| Archive SHA-256 | `f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7` |
| `vcp.exe` SHA-256 | `afdd9e0011601c059d82f6f1cc264b59e5a2627f20db36774c701ce5734f4bbd` |
| Manifest source | `74da9bb23c76297f349d986ee73be61c81126d48`, dirty source; use the packaged build receipt/content identity, not the commit alone |
| Candidate kind | Unsigned native Windows AMD64 debug qualification build; qualification feature/loopback facilities enabled, not a production release build |

Any relevant code, assets, configuration or signing change requires a new identity
and rerunning affected checks. Older package receipts are historical evidence.

## Install and inspect without inference

Use PowerShell 7 on the current native Windows workstation. The following creates
fresh, disjoint review directories; it does not modify an existing installation.
The syntax follows the [tested distribution procedure](../development/p8-distribution.md)
and `scripts/evals/distribution-qualification.ps1`.

```powershell
$packageDir = 'D:\code\Github\vcp\artifacts\p8-local-distribution\3f7a6bec-a374-487a-8d0a-06134e079e78'
$archive = Join-Path $packageDir 'vcp-windows-unsigned.zip'
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne 'f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7') { throw 'Wrong candidate archive' }
$reviewRoot = Join-Path $env:LOCALAPPDATA ('VCP-owner-review-' + [guid]::NewGuid())
$install = Join-Path $reviewRoot 'Install'
$data = Join-Path $reviewRoot 'Data'
$workspace = Join-Path $reviewRoot 'Workspace'
$vault = Join-Path $reviewRoot 'Vault'
$staging = Join-Path $reviewRoot 'Staging'
$unpacked = Join-Path $reviewRoot 'Unpacked'
New-Item -ItemType Directory -Path $reviewRoot,$data,$workspace,$vault,$staging | Out-Null
Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
$installer = Join-Path $unpacked 'tools/package-install.ps1'
pwsh -NoProfile -File $installer -Action Install -PackageZip $archive -InstallRoot $install -DataRoot $data
if ($LASTEXITCODE -ne 0) { throw 'Installation failed; retain diagnostics' }
$active = Get-Content -LiteralPath (Join-Path $install 'active.json') -Raw | ConvertFrom-Json
$vcp = Join-Path $install "releases/$($active.release)/vcp.exe"
if ((Get-FileHash -LiteralPath $vcp).Hash.ToLowerInvariant() -cne 'afdd9e0011601c059d82f6f1cc264b59e5a2627f20db36774c701ce5734f4bbd') { throw 'Wrong installed executable' }
$inspectArgs = @('--format','jsonl','--non-interactive','--workspace',$workspace,'--data-dir',$data)
& $vcp --help
& $vcp --version
& $vcp @inspectArgs doctor --vault $vault --staging $staging
```

These smoke commands start no model task. Expect successful help/version and
`path_checks_passed: true` from `doctor`; retain actual output and exit codes.
Path diagnosis is not encryption or recovery proof. Do not put source, history,
keys or vault data under the installation directory.

For an existing reviewed task, the read-only grammar is:

```powershell
& $vcp @inspectArgs tasks status $TaskId
& $vcp @inspectArgs tasks agents $TaskId
& $vcp @inspectArgs inspect $TaskId --view costs
& $vcp @inspectArgs inspect $TaskId --view verification
```

`$TaskId` must come from that workspace's real task output. Do not invent a task
or treat missing task/profile diagnostics as successful owner acceptance.

## Owner task use and spending boundary

Before U01–U03/U07, record the held-out fixture commit, dirty-file inventory,
hidden truth rubric, model/provider qualifications, routing configuration,
request/output/deadline limits, sample size and numeric quality/cost/latency
thresholds. Keep truth labels outside model-visible prompts. Freeze those inputs
before the first attempt; failed attempts, retries, children, compaction, graders
and human interventions stay in the denominator and total root cost.

No paid run is started or authorized by this packet. Use the existing campaign
ledger and its established ceiling; historical reservations and unknown charges
consume available headroom. Predeclare an affordable allocation within that
remaining budget before launch. Do not copy a historical model limit, change a
provider restriction, or increase a cap to obtain a passing score.

After those definitions and a qualified private profile exist, terminal task
syntax is as follows. The variables deliberately have no executable defaults:

```powershell
& $vcp --workspace $FixtureWorkspace --data-dir $FixtureData --config $OwnerProfile run --file $HeldOutTaskFile --budget-usd $ApprovedTaskBudget --autonomy plan
```

The profile belongs outside workspace/sync roots and contains the declared
provider/process/check configuration. `plan` is the read-only analysis/review
mode; generation needs its separately declared write/execute authority. In the
owning terminal use `/pause` and `/resume` for the same-process U06 variant.
External controls are `tasks pause $TaskId`; deliberate new-process continuation
is `resume $TaskId` with the same workspace/data/profile arguments. Inspection
does not resume work. Retain unknown effects for reconciliation.

## U01–U09 review runbook

The existing invariant threshold is **zero forbidden outcomes in each declared
suite**, not a high average score. Plan 16 requires numeric quality/cost/latency
thresholds and sample sizes before final evaluation but does not provide final
owner-approved numbers. The missing definitions below remain blockers, not
implicit defaults. Supporting native fixtures do not replace these owner runs.

| Case | Owner exercise and existing hard gate | Definition/evidence still needed |
| --- | --- | --- |
| U01 | Analyze a multi-module project with nested instructions, generated files and a seeded dependency violation. Validate cited revisions and boundary finding; require unchanged workspace and zero unrelated-workspace recall. | Owner held-out project/truth set, sample size, finding/unsupported-claim thresholds and explanation rubric; cost/latency limits. |
| U02 | Review seeded defects plus benign changes and an unrelated user edit, using visible read-only children. Count missed/false findings; require no writes, hidden work or unaccounted charges. | Owner-approved precision/recall/severity/usefulness thresholds, fixture split and sample size; total cost/time limits. |
| U03 | Implement a bounded cross-module feature with staged, unstaged and untracked edits. Execute feature assertions on integrated current source; preserve unrelated edits and module conventions. | Owner feature/forbidden-scope tests and architecture-fit rubric; sample size and quality/cost/time thresholds. Child-only verification cannot prove completion. |
| U04 | Compare retained records, liabilities and deletion lineage through encrypted handoff; reject wrong keys, unauthorized writers, corruption and unsafe activation; require zero plaintext cloud-bound publication or automatic new-host grants. | Machine handoff/cloud-copy/return run is excluded by the owner for this continuation and remains not run. Fresh local restore evidence is supporting evidence only; final environment and time/resource thresholds remain undeclared. |
| U05 | Inspect full output, then preview and exercise exclude/compact/purge with 30-day boundary cases, stale previews and protected references. Require exact selected/protected sets, default no-delete and zero purged/restricted recall. | Final owner fixture/selector oracle and usability rubric, including retained backup copies; declared task size and resource/time limits. |
| U06 | Pause/resume in the same terminal and independently close/kill/reopen at declared barriers. Require zero new dispatch after the stop boundary, duplicate marker effects or lost acknowledged records; preserve unknown liabilities and independently paused children. | Final owner integrated scenario and bounded cancellation/progress criteria, with both variants observed on this package. Process kill is not power-loss proof. |
| U07 | Compare grouped routing and optimization with fixed baselines; inspect/apply/rollback selected policy. Require eligibility, reproducibility, bounded accounting and zero silent cap/authority changes, pruning or paid trials. | Matched held-out fixtures, calibration labels, quality/cost/latency thresholds and sample sizes. Optional evaluator-disabled mode must make zero evaluator calls; deferred M7 is not a new release prerequisite. |
| U08 | Exercise built-in skills, nested instructions and controlled MCP identity/schema changes and delayed effects. Require correct provenance/authority, actionable missing-tool diagnosis, no unauthorized installation and no blind replay. | Owner language/toolchain/server matrix and usefulness rubric; exact supported/missing environments and expected outcomes before execution. |
| U09 | Use pinned local CPU embeddings with network access denied, build/reopen indexes, compare ANN with exact saved vectors and measure authorized recall. Require zero remote embedding calls and truthful degraded states. | Final corpus/query truth set, size, recall/resource/latency thresholds and qualifying CPU environment. Current high-memory-host measurements do not establish minimum hardware. |

Record executable outcomes and human quality judgments separately. Each row
needs package/configuration/fixture/backend/host identities, pass/fail/not-run,
actual costs including uncertainty, latency/resource measurements, interventions
and evidence links. A model grade cannot override an invariant failure. The
scorecard must cover all 56 first-release tasks and their FR/I mappings.

## Rollback, uninstall and support limits

Rollback is available only after an upgrade has recorded a previous compatible
release. It does not undo source edits, migrate formats or restore deleted data:

```powershell
pwsh -NoProfile -File $installer -Action Rollback -InstallRoot $install -DataRoot $data
```

After rollback, reread `active.json` before launching. On a fresh installation,
absence of a previous release is an expected refusal. To remove only the owned
installation while retaining the separate workspace/history/key/vault roots:

```powershell
pwsh -NoProfile -File $installer -Action Uninstall -InstallRoot $install -DataRoot $data
```

Keep review evidence and recovery material. Locked files, redirects, unexpected
installation entries and mismatched ownership/data roots are explicit errors;
inspect them rather than deleting the installation tree manually.

The candidate is unsigned, current-host Windows AMD64 qualification evidence.
Clean OS, independent-machine recovery, signing/publication and human acceptance
are not established. PowerShell 7 is required for installation; Node and a source
checkout are not installer requirements. Task toolchains and provider access
depend on explicit configuration. Model assets are not bundled or downloaded on
startup; provisioning is explicit, and successful asset verification alone does
not prove offline inference. Compatibility is `vcp-store/1+replay-base/2`,
`vcp-cli-profile/1`, and `derived-index-rebuild-required`; cross-format migration
is not implemented. Consult the scorecard/gaps for current failures and missing
measurements; this packet makes no supported minimum-hardware claim.

## Owner acceptance record — uncompleted

| Field | Current value |
| --- | --- |
| Decision | **NOT APPROVED — pending owner review and unresolved release gates** |
| Owner / review date | Not recorded |
| Package reviewed | Candidate hashes above; actual owner examination not recorded |
| Scorecard / gap revisions reviewed | Not recorded |
| Frozen U01–U09 fixtures, thresholds and budget allocation | Not recorded; missing definitions listed above |
| Owner observations / interventions / task usefulness | Not recorded |
| Required-case outcomes and unresolved blockers | See scorecard and gaps; no missing case counted as passed |
| Machine handoff | Excluded from this continuation by owner direction; release evidence remains not run |
| Publication authorization | Not granted by this packet |

Only the owner can supply the acceptance decision. Automated checks, document
preparation and completion of P8-06 cannot sign this form.
