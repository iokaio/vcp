# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot,[string]$TargetRoot,[ValidateRange(1,16)][int]$Jobs=4)
$ErrorActionPreference='Stop'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $IsWindows){Write-Output '{"status":"not_run","reason":"Native Windows required"}';exit 3}
if(-not $OutputRoot){$OutputRoot=Join-Path $repository 'artifacts/p1'}
if(-not $TargetRoot){$TargetRoot=Join-Path $repository 'artifacts/upstream/codex-target'}
foreach($tool in @('node','cargo','rustup','git')){if(-not(Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)){Write-Output "{`"status`":`"not_run`",`"reason`":`"Missing $tool`"}";exit 3}}
$paths=& node -e "const m=require(process.argv[1]),p=require('node:path'),r=process.argv[2];console.log(JSON.stringify({output:m.outside(process.argv[3],[p.join(r,'src')]),target:m.outside(process.argv[4],[p.join(r,'src')])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $OutputRoot $TargetRoot
if($LASTEXITCODE -ne 0){exit 2};$paths=$paths|ConvertFrom-Json
$directory=Join-Path $paths.output ([guid]::NewGuid().ToString());New-Item -ItemType Directory -Path $directory -Force|Out-Null
$record=[ordered]@{schema_version=1;task_id='P1-01/P1-02/P1-03/P1-04/P1-05/P1-06';status='prepared';started_at=[DateTime]::UtcNow.ToString('o');stages=@()}
$manifest=Join-Path $directory 'manifest.json'
function Save-Record {$record|ConvertTo-Json -Depth 16|Set-Content -LiteralPath $manifest -Encoding utf8}
function Stage([string]$Name,[string]$Program,[string[]]$Arguments){
    $log=Join-Path $directory ($Name+'.log');Write-Host "P1 qualification: $Name"
    & $Program @Arguments *> $log;$code=$LASTEXITCODE
    $record.stages+=@{name=$Name;command=@($Program)+$Arguments;exit_code=$code;log=$Name+'.log';sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()};Save-Record
    if($code -ne 0 -or (Get-Content -LiteralPath $log -Raw) -match 'panicked at'){throw "$Name failed or had a background panic"}
}
Save-Record
try{
    $installed=& rustup toolchain list
    if($LASTEXITCODE -ne 0 -or -not($installed -match '^1\.98\.0')){$record.status='not_run';$record.exit_code=3;throw 'Install native Rust 1.98.0'}
    Stage 'source-codex' 'node' @((Join-Path $repository 'scripts/upstream/reconstruct.cjs'),'verify','--component','codex')
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)){$record.status='not_run';$record.exit_code=3;throw 'Visual Studio discovery tool missing'}
    $vsRoot=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if(-not $vsRoot){$record.status='not_run';$record.exit_code=3;throw 'Native Visual C++ x64 tools missing'}
    $env:PATH=(Split-Path -Parent $vswhere)+';'+$env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation|Out-Null
    $env:RUST_MIN_STACK='16777216';$env:CODEX_TEST_ENVIRONMENT='local';$env:CARGO_TARGET_DIR=$paths.target
    $record.msvc=$env:VCToolsVersion;$record.rustc=& rustc '+1.98.0' --version
    $record.platform=[Runtime.InteropServices.RuntimeInformation]::OSDescription
    $volume=Get-Volume -DriveLetter ([IO.Path]::GetPathRoot($repository).Substring(0,1))
    $record.filesystem="$($volume.FileSystemType)"
    $record.durability_scope='Native forced-process termination with synced files; hardware power-loss durability not established'
    $record.vcp_commit=& git -C $repository rev-parse HEAD
    $packages=@('vcp-domain','vcp-protocol','vcp-store','vcp-engine','vcp-budget','vcp-audit','vcp-lifecycle')
    $inputs=@($packages|ForEach-Object{Get-ChildItem -LiteralPath (Join-Path $repository "src/crates/$_") -Recurse -File|Where-Object{$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName})
    $inputs+=@($PSCommandPath,(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'),(Join-Path $repository 'src/third_party/components/codex-files.json'))
    $record.inputs=@($inputs|Sort-Object|ForEach-Object{@{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}})
    $record.status='running';Save-Record
    Push-Location -LiteralPath (Join-Path $repository 'src/third_party/codex/codex-rs')
    try{
        $arguments=@('+1.98.0','test','--locked','--offline','--target','x86_64-pc-windows-msvc','-j',"$Jobs",'--features','vcp-store/qualification,vcp-budget/qualification,vcp-audit/qualification')
        foreach($package in $packages){$arguments+=@('-p',$package)}
        Stage 'contracts' 'cargo' ($arguments+@('--','--test-threads=1'))
        $tests=Get-Content -LiteralPath (Join-Path $directory 'contracts.log') -Raw
        $rows=[regex]::Matches($tests,'(?m)^test ([^\r\n]+) \.\.\. ok\r?$')
        if($rows.Count -ne 70){throw "Expected all 70 foundation/accounting/history/retained contracts; observed $($rows.Count)"}
        $record.tests=@($rows|ForEach-Object{$_.Groups[1].Value})
    }finally{Pop-Location}
    foreach($row in $record.inputs){if((Get-FileHash -LiteralPath (Join-Path $repository $row.path)).Hash.ToLowerInvariant() -ne $row.sha256){throw 'Source changed during qualification; rerun with stable inputs'}}
    $record.status='pass';$record.exit_code=0
}catch{if($record.status -ne 'not_run'){$record.status='fail';$record.exit_code=1};$record.reason=$_.Exception.Message}
$record.ended_at=[DateTime]::UtcNow.ToString('o');Save-Record
[pscustomobject]$record|Select-Object status,exit_code,reason,@{n='manifest';e={$manifest}}|ConvertTo-Json -Compress
exit $record.exit_code
