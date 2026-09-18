# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot,[string]$TargetRoot,[ValidateRange(1,16)][int]$Jobs=4,[ValidateSet('1.95.0','1.98.0')][string]$Toolchain='1.98.0')
$ErrorActionPreference='Stop'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $IsWindows){Write-Output '{"status":"not_run","reason":"Native Windows required"}';exit 3}
if(-not $OutputRoot){$OutputRoot=Join-Path $repository 'artifacts/integration'}
if(-not $TargetRoot){$TargetRoot=Join-Path $repository 'artifacts/upstream/codex-target'}
foreach($tool in @('node','cargo','rustup','git')){if(-not(Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)){Write-Output "{`"status`":`"not_run`",`"reason`":`"Missing $tool`"}";exit 3}}
$paths=& node -e "const m=require(process.argv[1]),p=require('node:path'),r=process.argv[2];console.log(JSON.stringify({output:m.outside(process.argv[3],[p.join(r,'src')]),target:m.outside(process.argv[4],[p.join(r,'src')])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $OutputRoot $TargetRoot
if($LASTEXITCODE -ne 0){exit 2};$paths=$paths|ConvertFrom-Json
$directory=Join-Path $paths.output ([guid]::NewGuid().ToString());New-Item -ItemType Directory -Path $directory -Force|Out-Null
$record=[ordered]@{schema_version=1;task_id='P0-08/P0-09';status='prepared';started_at=[DateTime]::UtcNow.ToString('o');stages=@()}
$manifest=Join-Path $directory 'manifest.json'
function Save-Record {$record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $manifest -Encoding utf8}
function Stage([string]$Name,[string]$Program,[string[]]$Arguments){
    $log=Join-Path $directory ($Name+'.log');Write-Host "Integration qualification: $Name"
    & $Program @Arguments *> $log;$code=$LASTEXITCODE
    $record.stages+=@{name=$Name;command=@($Program)+$Arguments;exit_code=$code;log=$Name+'.log';sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()};Save-Record
    if($code -ne 0 -or (Get-Content -LiteralPath $log -Raw) -match 'panicked at'){throw "$Name failed or had a background panic"}
}
Save-Record
try{
    $installed=& rustup toolchain list
    if($LASTEXITCODE -ne 0 -or -not($installed -match ('^'+[regex]::Escape($Toolchain)))){$record.status='not_run';$record.exit_code=3;throw "Install native Rust $Toolchain"}
    foreach($component in @('codex','munarium')){Stage "source-$component" 'node' @((Join-Path $repository 'scripts/upstream/reconstruct.cjs'),'verify','--component',$component)}
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)){$record.status='not_run';$record.exit_code=3;throw 'Visual Studio discovery tool missing'}
    $vsRoot=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if(-not $vsRoot){$record.status='not_run';$record.exit_code=3;throw 'Native Visual C++ x64 tools missing'}
    $env:PATH=(Split-Path -Parent $vswhere)+';'+$env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation|Out-Null
    $env:RUST_MIN_STACK='16777216';$env:CODEX_TEST_ENVIRONMENT='local';$env:CARGO_TARGET_DIR=$paths.target
    $record.msvc=$env:VCToolsVersion;$record.rustc=& rustc "+$Toolchain" --version
    $record.platform=[Runtime.InteropServices.RuntimeInformation]::OSDescription
    $record.vcp_commit=& git -C $repository rev-parse HEAD
    $inputs=@(Get-ChildItem -LiteralPath (Join-Path $repository 'src/crates/vcp-lifecycle') -Recurse -File|Where-Object {$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName)
    $inputs+=@(@('vcp-models','vcp-context','vcp-repository')|ForEach-Object{Get-ChildItem -LiteralPath (Join-Path $repository "src/crates/$_") -Recurse -File|Where-Object {$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName})
    $inputs+=@($PSCommandPath,(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'),(Join-Path $repository 'src/third_party/components/codex-files.json'),(Join-Path $repository 'src/tests/fixtures/gemini/ports.json'))
    $record.inputs=@($inputs|Sort-Object|ForEach-Object {@{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}})
    $common=@('--locked','--target','x86_64-pc-windows-msvc','-j',"$Jobs")
    $record.status='running';Save-Record
    Push-Location -LiteralPath (Join-Path $repository 'src/third_party/codex/codex-rs')
    try{
        Stage 'contracts' 'cargo' (@("+$Toolchain",'test','-p','vcp-lifecycle','--lib','--tests')+$common+@('--','--test-threads=1'))
        $tests=Get-Content -LiteralPath (Join-Path $directory 'contracts.log') -Raw
        foreach($count in @(5,3,6,16)){if($tests -notmatch "test result: ok\. $count passed; 0 failed; 0 ignored;"){throw "Missing expected $count-test contract group"}}
        $rows=[regex]::Matches($tests,'(?m)^test ([^\r\n]+) \.\.\. ok\r?$')
        if($rows.Count -ne 33){throw 'Expected all 33 native host/port/canonical contracts'}
        foreach($filter in @('contained_spawn_owns_immediate_descendant','rejected_job_assignment_resumes_existing_job_member')){
            Stage $filter 'cargo' (@("+$Toolchain",'test','-p','codex-utils-pty')+$common+@($filter,'--','--test-threads=1'))
            if((Get-Content -LiteralPath (Join-Path $directory ($filter+'.log')) -Raw) -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;'){throw 'Maintenance regression did not execute'}
        }
        Stage 'build-cli' 'cargo' (@("+$Toolchain",'build','-p','vcp-lifecycle','--examples','--bins')+$common)
        $binary=Join-Path $paths.target 'x86_64-pc-windows-msvc/debug/examples/integration-owner.exe'
        $record.binary_sha256=(Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant()
        Stage 'private-cli' $binary @((Join-Path $directory 'cli-workspace'))
        $result=Get-Content -LiteralPath (Join-Path $directory 'private-cli.log')|Select-Object -Last 1|ConvertFrom-Json
        if($result.status -ne 'pass' -or $result.requests -ne 7){throw 'CLI trace did not complete'}
    }finally{Pop-Location}
    foreach($sourceRow in $record.inputs){if((Get-FileHash -LiteralPath (Join-Path $repository $sourceRow.path)).Hash.ToLowerInvariant() -ne $sourceRow.sha256){throw 'Source changed during qualification; rerun with stable inputs'}}
    $record.status='pass';$record.exit_code=0
}catch{if($record.status -ne 'not_run'){$record.status='fail';$record.exit_code=1};$record.reason=$_.Exception.Message}
$record.ended_at=[DateTime]::UtcNow.ToString('o');Save-Record
[pscustomobject]$record|Select-Object status,exit_code,reason,@{n='manifest';e={$manifest}}|ConvertTo-Json -Compress
exit $record.exit_code
