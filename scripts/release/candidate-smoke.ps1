# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$NativeResult,[Parameter(Mandatory)][string]$SetupResult,[Parameter(Mandatory)][string]$OutputRoot)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'candidate-runtime.ps1')
$root=[IO.Path]::GetFullPath($OutputRoot)
$data=Join-Path (Split-Path -Parent $root) ('Retained Data β '+[guid]::NewGuid())
New-Item -ItemType Directory -Path $data -Force | Out-Null
$sentinel=Join-Path $data 'preserve-synthetic-history.txt'
[IO.File]::WriteAllText($sentinel,'preserve-exact-synthetic-bytes')
$installed=$null
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
} finally { if ($installed) { Uninstall-BetaCandidate $installed $root } }
if ([IO.File]::ReadAllText($sentinel) -cne 'preserve-exact-synthetic-bytes') { throw 'Uninstall changed protected data' }
foreach ($preference in $preferences) { if ((Get-FileHash -LiteralPath $preference.path).Hash.ToLowerInvariant() -cne $preference.sha256) { throw 'Uninstall changed a real storage preference' } }
@{schema='vcp-installed-native-smoke/1';status='pass';native_sha256=$installed.native_sha256;setup_sha256=$installed.setup_sha256;engine_sha256=$installed.engine_sha256;stores=@('files','sqlite');registered_install=$true;launcher_resolution=$true;uninstall_preserved_data=$true;model_calls=0;limitations=@('Storage preferences exercised; no canonical task/history created.','Hosted-image observation, not clean standard-user Windows.','No distinct-build upgrade or rollback qualified.')} | ConvertTo-Json -Depth 8
