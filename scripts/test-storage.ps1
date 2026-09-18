# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$AgeBinary,[string]$OutputRoot,[string]$TargetRoot,[string]$HandoffFixture,[ValidateRange(1,16)][int]$Jobs=4)
$ErrorActionPreference='Stop'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $IsWindows){Write-Output '{"status":"not_run","reason":"Native Windows required"}';exit 3}
if(-not $AgeBinary -or -not (Test-Path -LiteralPath $AgeBinary)){Write-Output '{"status":"not_run","reason":"Supply the pinned official age v1.3.2 Windows binary"}';exit 3}
if(-not $OutputRoot){$OutputRoot=Join-Path $repository 'artifacts/storage'}
if(-not $TargetRoot){$TargetRoot=Join-Path $repository 'artifacts/storage-target'}
$paths=& node -e "const m=require(process.argv[1]),p=require('node:path'),r=process.argv[2];console.log(JSON.stringify({output:m.outside(process.argv[3],[p.join(r,'src')]),target:m.outside(process.argv[4],[p.join(r,'src')])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $OutputRoot $TargetRoot
if($LASTEXITCODE -ne 0){exit 2};$paths=$paths|ConvertFrom-Json
$directory=Join-Path $paths.output ([guid]::NewGuid().ToString());New-Item -ItemType Directory -Path $directory -Force|Out-Null
$record=[ordered]@{schema_version=1;task_id='P0-04';status='prepared';started_at=[DateTime]::UtcNow.ToString('o');stages=@()}
$manifest=Join-Path $directory 'manifest.json'
function Save-Record {$record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $manifest -Encoding utf8}
function Not-Run([string]$Reason){$record.status='not_run';$record.exit_code=3;throw $Reason}
function Stage([string]$Name,[string]$Program,[string[]]$Arguments){
    $log=Join-Path $directory ($Name+'.log');Write-Host "Storage qualification: $Name"
    & $Program @Arguments *> $log;$code=$LASTEXITCODE
    $record.stages+=@{name=$Name;command=@($Program)+$Arguments;exit_code=$code;log=$Name+'.log';sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()};Save-Record
    if($code -ne 0){throw "$Name failed with exit $code"}
}
Save-Record
try{
    foreach($tool in @('cargo','rustup','git')){if(-not(Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)){Not-Run "Missing prerequisite: $tool"}}
    $installed=& rustup toolchain list
    if($LASTEXITCODE -ne 0 -or -not($installed|Where-Object {$_ -match '^1\.98\.0-x86_64-pc-windows-msvc(?:\s|$)'})){Not-Run 'Install native Rust 1.98.0 first'}
    foreach($component in @('codex','munarium')){Stage "source-$component" 'node' @((Join-Path $repository 'scripts/upstream/reconstruct.cjs'),'verify','--component',$component)}
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)){Not-Run 'Visual Studio discovery tool missing'}
    $vsRoot=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if(-not $vsRoot){Not-Run 'Visual C++ x64 build tools missing'}
    $env:PATH=(Split-Path -Parent $vswhere)+';'+$env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation|Out-Null
    $record.msvc=$env:VCToolsVersion;$record.rustc=& rustc +1.98.0 --version;if($LASTEXITCODE -ne 0){throw 'Rust 1.98.0 required'}
    $record.platform=[Runtime.InteropServices.RuntimeInformation]::OSDescription;$record.filesystem=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($directory)).DriveFormat
    $record.vcp_commit=& git -C $repository rev-parse HEAD;$record.vcp_dirty=[bool](& git -C $repository status --porcelain=v1 --untracked-files=all)
    $inputs=@(Get-ChildItem -LiteralPath (Join-Path $repository 'src/crates/vcp-storage-spike') -Recurse -File|Where-Object {$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName)
    $inputs+=@($PSCommandPath,(Join-Path $repository 'scripts/upstream/qualify-storage.cjs'),(Join-Path $repository 'src/tests/support/windows/storage-metrics.ps1'),(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'),(Join-Path $repository 'src/third_party/components/age-qualification.json'))
    $record.inputs=@($inputs|Sort-Object|ForEach-Object {@{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}})
    $common=@('--locked','--manifest-path',(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'),'-p','vcp-storage-spike','--target','x86_64-pc-windows-msvc','--target-dir',$paths.target,'-j',"$Jobs")
    $record.status='running';Save-Record
    Stage 'contracts' 'cargo' (@('+1.98.0','test')+$common)
    $tests=Get-Content -LiteralPath (Join-Path $directory 'contracts.log') -Raw
    if($tests -notmatch 'test result: ok\. 4 passed; 0 failed; 0 ignored;' -or $tests -match 'panicked at'){throw 'Missing storage contracts or background panic'}
    Stage 'build' 'cargo' (@('+1.98.0','build','--bin','vcp-storage-spike')+$common)
    $binary=Join-Path $paths.target 'x86_64-pc-windows-msvc/debug/vcp-storage-spike.exe';$record.binary_sha256=(Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant()
    Stage 'qualification' 'node' @((Join-Path $repository 'scripts/upstream/qualify-storage.cjs'),'--binary',$binary,'--age',([IO.Path]::GetFullPath($AgeBinary)),'--output-root',(Join-Path $directory 'qualification'))
    $record.result=Get-Content -LiteralPath (Join-Path $directory 'qualification.log')|Select-Object -Last 1|ConvertFrom-Json
    if($record.result.status -ne 'pass'){throw 'Storage qualification incomplete'}
    if($HandoffFixture){
        $fixture=[IO.Path]::GetFullPath($HandoffFixture);$transfer=Join-Path $directory 'second-environment';New-Item -ItemType Directory -Path (Join-Path $transfer 'vault') -Force|Out-Null
        $spec=Get-Content -LiteralPath (Join-Path $fixture 'fixture.json') -Raw|ConvertFrom-Json
        foreach($item in $spec.files){
            if($item.path -notmatch '^(snapshot-id\.txt|vault/[a-f0-9]{64}\.age)$'){throw 'Unsafe fixture path'}
            $source=Join-Path $fixture $item.path;if((Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() -ne $item.sha256){throw 'Handoff fixture mismatch'}
            Copy-Item -LiteralPath $source -Destination (Join-Path $transfer $item.path)
        }
        Stage 'separate-test-recovery' $binary @('handoff-recovery',$transfer)
        Stage 'second-environment-restore' $binary @('handoff-consume',$transfer)
        $record.handoff_fixture_sha256=(Get-FileHash -LiteralPath (Join-Path $fixture 'fixture.json')).Hash.ToLowerInvariant()
    }
    foreach($inputFile in $record.inputs){if((Get-FileHash -LiteralPath (Join-Path $repository $inputFile.path)).Hash.ToLowerInvariant() -ne $inputFile.sha256){throw 'Source changed during qualification'}}
    $record.status='pass';$record.exit_code=0
}catch{if($record.status -ne 'not_run'){$record.status='fail';$record.exit_code=1};$record.reason=$_.Exception.Message}
finally{$record.ended_at=[DateTime]::UtcNow.ToString('o');Save-Record}
Write-Output ($record|ConvertTo-Json -Depth 12);exit $record.exit_code
