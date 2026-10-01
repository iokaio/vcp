# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$SetupResult,[Parameter(Mandatory)][string]$OutputRoot,[string]$ConsoleTestExecutable)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
$root=[IO.Path]::GetFullPath($OutputRoot)
$data=Join-Path (Split-Path -Parent $root) ('Retained Data β '+[guid]::NewGuid())
New-Item -ItemType Directory -Path $data -Force | Out-Null
$sentinel=Join-Path $data 'preserve-synthetic-history.txt'
[IO.File]::WriteAllText($sentinel,'preserve-exact-synthetic-bytes')
$installed=$null
$console=@{status='not run';reason='No final-artifact console test executable supplied.'}
try {
    $installed=Install-BetaCandidate $NativeResult $SetupResult $root $data
    foreach ($backend in @('files','sqlite')) {
        $workspace=Join-Path $root "Project 漢字 $backend"; New-Item -ItemType Directory -Path $workspace | Out-Null
        $null=Invoke-BetaProcess $installed.launcher @('--version') $workspace
        $null=Invoke-BetaProcess $installed.launcher @('--workspace',$workspace,'--non-interactive','--format','jsonl','storage','configure','--backend',$backend) $workspace
        $null=Invoke-BetaProcess $installed.launcher @('--workspace',$workspace,'--non-interactive','--format','jsonl','storage','configure','--backend',$backend,'--preview') $workspace
    }
    $preferences=@(Get-ChildItem -LiteralPath $data -Filter storage-preference.json -File -Recurse | ForEach-Object { @{path=$_.FullName;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()} })
    if ($preferences.Count -ne 2) { throw 'Both real backend preferences must exist' }
    if ($ConsoleTestExecutable) {
        $consoleRoot=Join-Path $root 'console-observations'
        $environment=@{
            VCP_BETA_NATIVE_RESULT=[IO.Path]::GetFullPath($NativeResult)
            VCP_BETA_SETUP_RESULT=[IO.Path]::GetFullPath($SetupResult)
            VCP_BETA_INSTALLED_LAUNCHER=$installed.launcher
            VCP_BETA_LAUNCHER_CONSOLE_OUTPUT=$consoleRoot
            VCP_TEST_NODE=(Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
            RUST_MIN_STACK='16777216'
        }
        $observed=Invoke-BetaProcess $ConsoleTestExecutable @('--ignored','--exact','final_installed_launcher_console_cancellation_preserves_both_stores','--nocapture','--test-threads=1') $root $environment 1200
        if ($observed.stdout -notmatch 'test result: ok\. 1 passed') { throw 'Final console test did not execute successfully' }
        $console=Get-Content -LiteralPath (Join-Path $consoleRoot 'result.json') -Raw | ConvertFrom-Json
        if ($console.schema -cne 'vcp-beta-launcher-console/1' -or $console.status -cne 'pass' -or $console.candidate_unchanged -ne $true -or @($console.cases).Count -ne 8 -or $console.candidate.native_archive_sha256 -cne $installed.native_sha256 -or $console.candidate.setup_archive_sha256 -cne $installed.setup_sha256 -or $console.candidate.engine_sha256 -cne $installed.engine_sha256 -or $console.qualification_executable_sha256 -cne (Get-FileHash -LiteralPath $ConsoleTestExecutable).Hash.ToLowerInvariant()) { throw 'Final console report differs from tested candidate or qualification executable' }
        foreach ($backend in @('Files','Sqlite')) {
            foreach ($signal in @('Ctrl+C','Ctrl+Break')) {
                foreach ($mode in @('direct','launcher')) {
                    $case=@($console.cases | Where-Object { $_.backend -ceq $backend -and $_.signal -ceq $signal -and $_.mode -ceq $mode })
                    if ($case.Count -ne 1 -or $case[0].status -cne 'pass' -or $case[0].canonical_unchanged -ne $true -or $case[0].workspace_sentinel_unchanged -ne $true -or $case[0].observation.owned_job_active_processes -ne 0 -or $case[0].observation.forced_cleanup -ne $false -or $case[0].observation.engine_exit_u32 -ne $case[0].observation.root_exit_u32 -or $case[0].observation.engine_exit_u32 -ne $case[0].direct_exit_u32) { throw 'Final console case lacks successful exit, containment or preservation evidence' }
                    foreach ($exitCode in @($case[0].observation.engine_exit_u32,$case[0].observation.root_exit_u32,$case[0].direct_exit_u32)) {
                        if (($exitCode -isnot [int] -and $exitCode -isnot [long]) -or $exitCode -lt 0 -or $exitCode -gt [uint32]::MaxValue) { throw 'Final console case lacks an observed Windows process exit code' }
                    }
                }
            }
        }
    }
} catch {
    # A failed native observation may leave an owned process or incomplete
    # operation. Retain the registered installation and private repair evidence.
    throw
}
Uninstall-BetaCandidate $installed $root
if ([IO.File]::ReadAllText($sentinel) -cne 'preserve-exact-synthetic-bytes') { throw 'Uninstall changed protected data' }
foreach ($preference in $preferences) { if ((Get-FileHash -LiteralPath $preference.path).Hash.ToLowerInvariant() -cne $preference.sha256) { throw 'Uninstall changed a real storage preference' } }
@{schema='vcp-installed-native-smoke/1';status='pass';native_sha256=$installed.native_sha256;setup_sha256=$installed.setup_sha256;engine_sha256=$installed.engine_sha256;signatures=$installed.signatures;stores=@('files','sqlite');registered_install=$true;launcher_resolution=$true;uninstall_preserved_data=$true;console=$console;model_calls=0;limitations=@('Storage-preference smoke has no canonical tasks; optional console test uses separate synthetic paused histories.','Hosted-image observation, not clean standard-user Windows.','No distinct-build upgrade or rollback qualified.')} | ConvertTo-Json -Depth 20
