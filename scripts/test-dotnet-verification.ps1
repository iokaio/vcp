# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot,[string]$TargetRoot,[string]$Dotnet,[string]$NuGetPackages,[ValidateRange(1,16)][int]$Jobs=4)
$ErrorActionPreference='Stop'
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $IsWindows){Write-Output '{"status":"not_run","reason":"Native Windows required"}';exit 3}
if(-not $OutputRoot){$OutputRoot=Join-Path $repository 'artifacts/dotnet-verification'}
if(-not $TargetRoot){$TargetRoot=Join-Path $repository 'artifacts/upstream/codex-target'}
if(-not $Dotnet){$Dotnet=Join-Path $env:ProgramFiles 'dotnet/dotnet.exe'}
if(-not $NuGetPackages){$NuGetPackages=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.nuget/packages'}
$paths=& node -e "const m=require(process.argv[1]),p=require('node:path'),r=process.argv[2];console.log(JSON.stringify({output:m.outside(process.argv[3],[p.join(r,'src')]),target:m.outside(process.argv[4],[p.join(r,'src')])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $OutputRoot $TargetRoot
if($LASTEXITCODE -ne 0){exit 2};$paths=$paths|ConvertFrom-Json
$directory=Join-Path $paths.output ([guid]::NewGuid().ToString());New-Item -ItemType Directory -Path $directory -Force|Out-Null
$manifest=Join-Path $directory 'manifest.json'
$record=[ordered]@{schema_version=1;task_id='P2-06';status='prepared';started_at=[DateTime]::UtcNow.ToString('o');stages=@();scope='Real MSBuild/VSTest, SQLite and Files stores, seeded failure and current repair; no provider inference'}
function Save-Record {$record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $manifest -Encoding utf8}
Save-Record
try{
    if(-not(Test-Path -LiteralPath $Dotnet -PathType Leaf)){$record.status='not_run';throw 'Explicit native dotnet executable unavailable'}
    foreach($package in @('microsoft.net.test.sdk/17.11.1','xunit/2.9.2','xunit.runner.visualstudio/2.8.2')){
        if(-not(Test-Path -LiteralPath (Join-Path $NuGetPackages $package) -PathType Container)){$record.status='not_run';throw "Offline package cache missing $package"}
    }
    $record.dotnet=& $Dotnet --version
    if($LASTEXITCODE -ne 0 -or $record.dotnet -notmatch '^10\.'){$record.status='not_run';throw 'The .slnx qualification requires an installed .NET 10 SDK'}
    $record.dotnet_sha256=(Get-FileHash -LiteralPath $Dotnet).Hash.ToLowerInvariant()
    $record.packages=@('microsoft.net.test.sdk/17.11.1','xunit/2.9.2','xunit.runner.visualstudio/2.8.2'|ForEach-Object {
        $packagePath=Join-Path $NuGetPackages $_
        $hashFile=Get-ChildItem -LiteralPath $packagePath -Filter '*.nupkg.sha512' -File|Select-Object -First 1
        if(-not $hashFile){throw "Cached package identity missing for $_"}
        @{package=$_;recorded_sha512=(Get-Content -LiteralPath $hashFile.FullName -Raw).Trim();sha256=(Get-FileHash -LiteralPath $hashFile.FullName).Hash.ToLowerInvariant()}
    })
    $record.platform=[Runtime.InteropServices.RuntimeInformation]::OSDescription
    $record.vcp_commit=& git -c "safe.directory=$($repository.Replace('\','/'))" -C $repository rev-parse HEAD
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere -PathType Leaf)){$record.status='not_run';throw 'Visual Studio discovery tool unavailable'}
    $vsRoot=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if(-not $vsRoot){$record.status='not_run';throw 'Native Visual C++ x64 tools unavailable'}
    $env:PATH=(Split-Path -Parent $vswhere)+';'+$env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation|Out-Null
    $env:VCP_TEST_DOTNET=[IO.Path]::GetFullPath($Dotnet)
    $env:VCP_TEST_NUGET_PACKAGES=[IO.Path]::GetFullPath($NuGetPackages)
    $env:RUST_MIN_STACK='16777216';$env:CODEX_TEST_ENVIRONMENT='local';$env:CARGO_TARGET_DIR=$paths.target
    $record.rustc=& rustc '+1.95.0' --version
    if($LASTEXITCODE -ne 0){$record.status='not_run';throw 'Native Rust 1.95.0 unavailable'}
    $inputs=@(Get-ChildItem -LiteralPath (Join-Path $repository 'src/crates') -Recurse -File|Where-Object {$_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml'}|ForEach-Object FullName)
    $inputs+=@($PSCommandPath,(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'),(Join-Path $repository 'src/third_party/components/codex-files.json'))
    $record.inputs=@($inputs|Sort-Object|ForEach-Object {@{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}})
    $record.status='running';Save-Record
    $arguments=@('+1.95.0','test','--manifest-path',(Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'),'--locked','--offline','--target','x86_64-pc-windows-msvc','-j',"$Jobs",'-p','vcp-lifecycle','--features','dotnet-qualification','--test','canonical_host','actual_dotnet_checks_preserve_failed_evidence_before_completion','--','--test-threads=1','--nocapture')
    $log=Join-Path $directory 'native-dotnet.log'
    & cargo @arguments *> $log;$code=$LASTEXITCODE
    $record.stages+=@{name='native-dotnet';command=@('cargo')+$arguments;exit_code=$code;log='native-dotnet.log';sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()};Save-Record
    $text=Get-Content -LiteralPath $log -Raw
    if($code -ne 0 -or $text -match 'panicked at' -or $text -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;'){throw 'Real .NET qualification did not pass exactly the selected regression'}
    foreach($inputRecord in $record.inputs){if((Get-FileHash -LiteralPath (Join-Path $repository $inputRecord.path)).Hash.ToLowerInvariant() -ne $inputRecord.sha256){throw "Qualification input changed: $($inputRecord.path)"}}
    $record.status='pass';$record.exit_code=0
}catch{if($record.status -eq 'not_run'){$record.exit_code=3}else{$record.status='fail';$record.exit_code=1};$record.reason=$_.Exception.Message}
$record.ended_at=[DateTime]::UtcNow.ToString('o');Save-Record
[pscustomobject]$record|Select-Object status,exit_code,reason,@{n='manifest';e={$manifest}}|ConvertTo-Json -Compress
exit $record.exit_code
