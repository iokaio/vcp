# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [ValidateRange(1,16)][int]$Jobs=2, [switch]$FullHost)
$ErrorActionPreference='Stop'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $IsWindows){Write-Output '{"status":"not_run","reason":"Native Windows required"}';exit 3}
if(-not $OutputRoot){$OutputRoot=Join-Path $repository 'artifacts/delegation'}
if(-not $TargetRoot){$TargetRoot=Join-Path $repository 'artifacts/codex-target'}
foreach($tool in @('node','cargo','rustup','git')){
    if(-not(Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)){Write-Output "Missing required native tool: $tool";exit 3}
}
$paths=& node -e "const m=require(process.argv[1]),p=require('node:path'),r=process.argv[2];console.log(JSON.stringify({output:m.outside(process.argv[3],[p.join(r,'src')]),target:m.outside(process.argv[4],[p.join(r,'src')])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $OutputRoot $TargetRoot
if($LASTEXITCODE -ne 0){exit 2}
$paths=$paths|ConvertFrom-Json
$directory=Join-Path $paths.output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force|Out-Null
$manifest=Join-Path $directory 'manifest.json'
$record=[ordered]@{schema_version=1;task_id='P7-04/P7-05/P7-06';status='prepared';started_at=[DateTime]::UtcNow.ToString('o');stages=@();limits=@('Mock provider fixtures; not live usefulness or release acceptance','No qualified child process filesystem sandbox')}
function Save-Record {$record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $manifest -Encoding utf8}
function Stage([string]$Name,[string[]]$Arguments,[string[]]$Required){
    $log=Join-Path $directory ($Name+'.log')
    Write-Host "Delegation qualification: $Name"
    & cargo @Arguments *> $log
    $code=$LASTEXITCODE
    $record.stages+=@{name=$Name;command=@('cargo')+$Arguments;exit_code=$code;log=$Name+'.log';sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()}
    Save-Record
    $content=Get-Content -LiteralPath $log -Raw
    if($code -ne 0 -or $content -match 'panicked at'){throw "$Name failed or had a background panic"}
    foreach($test in $Required){if($content -notmatch ('(?m)^test [^\r\n]*'+[regex]::Escape($test)+' \.\.\. ok\r?$')){throw "Required native test did not pass: $test"}}
}
Save-Record
try{
    $installed=& rustup toolchain list
    if($LASTEXITCODE -ne 0 -or -not($installed -match '^1\.95\.0')){throw 'Native Rust 1.95.0 required'}
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)){throw 'Visual Studio discovery tool missing'}
    $vsRoot=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if(-not $vsRoot){throw 'Native Visual C++ x64 tools missing'}
    $env:PATH=(Split-Path -Parent $vswhere)+';'+$env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation|Out-Null
    foreach($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin','Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')){$env:PATH=(Join-Path $vsRoot $relative)+';'+$env:PATH}
    $env:RUST_MIN_STACK='16777216';$env:CODEX_TEST_ENVIRONMENT='local';$env:CARGO_TARGET_DIR=$paths.target
    $env:VCP_TEST_NODE=(Get-Command node -CommandType Application).Source
    $env:VCP_TEST_GIT=(Get-Command git -CommandType Application).Source
    $env:VCP_TEST_CARGO=& rustup which --toolchain '1.95.0' cargo
    if($LASTEXITCODE -ne 0 -or -not(Test-Path -LiteralPath $env:VCP_TEST_CARGO)){throw 'Real native Cargo executable unavailable'}
    $env:VCP_TEST_COMPILER_PATH=(Split-Path -Parent $env:VCP_TEST_CARGO)+';'+$env:PATH
    foreach($name in @('LIB','INCLUDE','LIBPATH')){
        [Environment]::SetEnvironmentVariable("VCP_TEST_COMPILER_$name",[Environment]::GetEnvironmentVariable($name),'Process')
    }
    $record.platform=[Runtime.InteropServices.RuntimeInformation]::OSDescription
    $record.rustc=& rustc +1.95.0 --version
    $record.msvc=$env:VCToolsVersion
    $gitSafeRoot=$repository.Replace('\','/')
    $record.commit=& git -c "safe.directory=$gitSafeRoot" -C $repository rev-parse HEAD
    if($LASTEXITCODE -ne 0){throw 'Unable to record repository revision'}
    $inputs=@(Get-ChildItem -LiteralPath (Join-Path $repository 'src/crates') -Recurse -File|Where-Object {$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName)
    $inputs+=@($PSCommandPath,(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'),(Join-Path $repository 'src/third_party/components/codex-files.json'))
    $record.inputs=@($inputs|Sort-Object|ForEach-Object {@{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}})
    $common=@('+1.95.0','test','--locked','--offline','-j',"$Jobs")
    $hostTests=@('-p','vcp-lifecycle','--features','qualification','--test','canonical_host')
    if(-not $FullHost){$hostTests+=@('child_')}
    $record.full_host=[bool]$FullHost
    $record.status='running';Save-Record
    Push-Location -LiteralPath (Join-Path $repository 'src/third_party/codex/codex-rs')
    try{
        Stage 'repository-tools' ($common+@('-p','vcp-repository','-p','vcp-tools','--','--test-threads=1')) @('owner_preparation_keeps_binary_bytes_and_pins_index_through_native_effect','cleanup_admission_gate_stops_between_native_removals_and_reuses_same_intent')
        Stage 'canonical-contracts' ($common+@('-p','vcp-domain','-p','vcp-protocol','-p','vcp-store','-p','vcp-engine','--tests','--','--test-threads=1')) @('child_registration_is_atomic_bounded_and_durable_on_both_backends','dependencies_scope_pause_and_cancellation_are_canonical','eligibility_blocks_zero_remaining_capacity_without_charging_unused_allocations')
        Stage 'connected' ($common+$hostTests+@('--','--test-threads=1')) @('graph_child_dispatch_uses_isolated_bytes_root_budget_and_parent_pause_gate','child_result_integration_uses_parent_broker_and_rejects_concurrent_parent_changes','child_fixed_model_mismatch_never_sends_or_reserves_budget','fresh_owner_recovers_registered_child_edits_and_keeps_sibling_paused','child_integration_partial_native_failure_retains_receipts_and_human_edits','child_integration_pause_between_writes_retains_receipts_and_human_edits','child_cleanup_retains_durable_intent_and_results_and_reconciles_only_original_root','helper_templates_admit_current_read_only_assignments_and_fence_parent_pause')
        Stage 'verification-pause' ($common+@('-p','vcp-lifecycle','--features','qualification','--test','canonical_host','native_verification_pause_before_publish','--','--test-threads=1')) @('native_verification_pause_before_publish_retains_checks_without_completion')
        Stage 'terminal' ($common+@('-p','vcp-cli','--features','qualification','--lib','--','--test-threads=1')) @('agents_pages_preserve_all_children_and_canonical_node_cost_without_resuming','resumed_child_ignores_terminal_events_queued_by_failed_previous_turn')
        Stage 'native-cli' ($common+@('-p','vcp-cli','--features','qualification','--test','executable','executable_terminal_','--','--test-threads=1')) @('executable_terminal_delegates_real_child_with_canonical_transcript','executable_terminal_delegates_real_child_and_pause_fences_both_requests','executable_terminal_hard_close_reopens_two_active_children_without_dispatch','executable_terminal_pause_fences_two_children_and_preserves_quiet_status')
    }finally{Pop-Location}
    foreach($inputRecord in $record.inputs){if((Get-FileHash -LiteralPath (Join-Path $repository $inputRecord.path)).Hash.ToLowerInvariant() -ne $inputRecord.sha256){throw 'Source changed during qualification; rerun with stable inputs'}}
    $record.status='pass';$record.exit_code=0
}catch{$record.status='fail';$record.exit_code=1;$record.reason=$_.Exception.Message}
$record.ended_at=[DateTime]::UtcNow.ToString('o');Save-Record
[pscustomobject]$record|Select-Object status,exit_code,reason,@{n='manifest';e={$manifest}}|ConvertTo-Json -Compress
exit $record.exit_code
