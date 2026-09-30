# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$CompilerRoot)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows installer shell test required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-setup-shell-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$identifier = 'VCP.SetupShellTest.' + [guid]::NewGuid().ToString('N')
$registration = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $identifier + '_is1'
$app = Join-Path $temporary 'Program Files β'
$data = Join-Path $temporary 'Protected Data β'
$candidate = 'b' * 64
$payload = Join-Path $temporary 'payload'
New-Item -ItemType Directory -Path $payload,$data | Out-Null
[IO.File]::WriteAllText((Join-Path $data 'preserve.sentinel'),'protected user data')
$original = [Text.Encoding]::UTF8.GetBytes('synthetic engine for installer shell tests; never executed')
[IO.File]::WriteAllBytes((Join-Path $payload 'vcp.exe'),$original)
$manifest = [ordered]@{
    schema='vcp-distribution-manifest/1'
    files=@(@{path='vcp.exe';bytes=$original.Length;sha256=(Get-FileHash -LiteralPath (Join-Path $payload 'vcp.exe')).Hash.ToLowerInvariant()})
    compatibility=@{canonical='vcp-store/1+replay-base/2';config='vcp-cli-profile/1';index='derived-index-rebuild-required'}
    model_provisioning=@{bundled=$false}
    release=@{candidate_id=$candidate}
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $payload 'manifest.json') -Encoding utf8NoBOM
$archive = Join-Path $temporary 'native.zip'
[IO.Compression.ZipFile]::CreateFromDirectory($payload,$archive)
$archiveHash=(Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant()
$launcherSource = Join-Path $temporary 'launcher.cs'
@'
using System;
using System.IO;
using System.Collections.Generic;
using System.Web.Script.Serialization;
class Fixture {
  static int Main(string[] args) {
    if(args.Length == 3 && args[0] == "--hold-lock") {
      using(var mutex = new System.Threading.Mutex(false,args[1])) {
        mutex.WaitOne(); File.WriteAllText(args[2],"locked"); System.Threading.Thread.Sleep(-1);
      }
      return 0;
    }
    if(args.Length != 1 || args[0] != "--resolve-installation") return 2;
    var json = new JavaScriptSerializer();
    var app = AppDomain.CurrentDomain.BaseDirectory;
    var engine = Path.Combine(app,"engine");
    var pointer = json.Deserialize<Dictionary<string,object>>(File.ReadAllText(Path.Combine(engine,"active.json")));
    Console.WriteLine(json.Serialize(new { schema="vcp-installed-engine/1", executable=Path.Combine(engine,"releases",(string)pointer["release"],"vcp.exe"), data_directory=(string)pointer["data_root"] }));
    return 0;
  }
}
'@ | Set-Content -LiteralPath $launcherSource -Encoding utf8NoBOM
$launcher = Join-Path $temporary 'fixture-launcher.exe'
$csc = Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $csc /nologo /target:exe /platform:x64 /r:System.Web.Extensions.dll (('/out:')+$launcher) $launcherSource
if ($LASTEXITCODE -ne 0) { throw 'Synthetic launcher fixture compiler failed' }
$compileLog = Join-Path $temporary 'compile.log'
$arguments = @('/Q',('/O'+$temporary),('/DNativeArchive='+$archive),('/DLauncher='+$launcher),'/DProductVersion=0.2.0-beta.1',('/DNativeSha256='+$archiveHash),('/DCandidateId='+$candidate),('/DShellSha256='+(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'shell.ps1')).Hash.ToLowerInvariant()),('/DEngineScriptSha256='+(Get-FileHash -LiteralPath (Join-Path $repository 'scripts/package-install.ps1')).Hash.ToLowerInvariant()),('/DNoticesSha256='+(Get-FileHash -LiteralPath (Join-Path $repository 'release/installer-notices/inventory.json')).Hash.ToLowerInvariant()),('/DVcpAppId='+$identifier),'/DProductName=VCP Installer Shell Test',(Join-Path $PSScriptRoot 'vcp.iss'))
& (Join-Path ([IO.Path]::GetFullPath($CompilerRoot)) 'ISCC.exe') @arguments *> $compileLog
if ($LASTEXITCODE -ne 0) { throw "Pinned shell compilation failed; $compileLog" }
$setup = Join-Path $temporary 'vcp-0.2.0-beta.1-windows-x64-unsigned-setup.exe'
function Invoke-Hidden([string]$Executable,[string[]]$Arguments) {
    $process = Start-Process -FilePath $Executable -ArgumentList $Arguments -WindowStyle Hidden -PassThru -Wait
    return $process.ExitCode
}
function Install-Arguments([string]$Root,[string]$Data,[string]$Log) {
    return @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER',('/DIR="'+$Root+'"'),('/DATADIR="'+$Data+'"'),('/LOG="'+(Join-Path $temporary $Log)+'"'))
}
$unowned = Join-Path $temporary 'Unowned'
New-Item -ItemType Directory -Path $unowned | Out-Null
[IO.File]::WriteAllText((Join-Path $unowned 'preserve.sentinel'),'unrelated application')
if ((Invoke-Hidden $setup (Install-Arguments $unowned $data 'unowned.log')) -eq 0 -or (Test-Path -LiteralPath (Join-Path $unowned 'engine'))) { throw 'Unowned application root was accepted' }
if ((Invoke-Hidden $setup (Install-Arguments $app (Join-Path $app 'data') 'overlap.log')) -eq 0 -or (Test-Path -LiteralPath $app)) { throw 'Nested protected data was accepted' }
$lockName='Global\VCP.Setup.'+[Environment]::MachineName.ToUpperInvariant()+'.'+[Environment]::UserName.ToUpperInvariant()
$ready=Join-Path $temporary 'abandoned-lock-ready'
$holder=Start-Process -FilePath $launcher -ArgumentList @('--hold-lock',('"'+$lockName+'"'),('"'+$ready+'"')) -WindowStyle Hidden -PassThru
$survivingHandle=$null
try {
    $deadline=[Diagnostics.Stopwatch]::StartNew()
    while(-not (Test-Path -LiteralPath $ready)) {
        if($holder.HasExited -or $deadline.Elapsed.TotalSeconds -gt 10){throw 'Abandoned-lock fixture did not become ready'}
        Start-Sleep -Milliseconds 20
    }
    $survivingHandle=[Threading.Mutex]::OpenExisting($lockName)
    $holder.Kill()
    if(-not $holder.WaitForExit(10000)){throw 'Abandoned-lock fixture did not reap'}
    if ((Invoke-Hidden $setup (Install-Arguments $app $data 'install.log')) -ne 0) { throw "Registered fixture install failed to recover an abandoned lock; inspect $temporary" }
} finally {
    if(-not $holder.HasExited){$holder.Kill();$null=$holder.WaitForExit(10000)}
    $holder.Dispose()
    if($survivingHandle){$survivingHandle.Dispose()}
}
if (-not (Test-Path -LiteralPath $registration) -or -not (Test-Path -LiteralPath (Join-Path $app 'vcp.exe'))) { throw 'Expected per-user registration and launcher missing' }
$uninstaller = Join-Path $app 'unins000.exe'
$engineFile = Join-Path $app "engine\releases\$archiveHash\vcp.exe"
$mutex = [Threading.Mutex]::new($false,$lockName)
$owned = $false
try {
    $owned=$mutex.WaitOne(0)
    if (-not $owned) { throw 'Installer shell concurrency fixture requires no active setup' }
    if ((Invoke-Hidden $setup (Install-Arguments $app $data 'competing.log')) -eq 0) { throw 'Competing setup did not refuse the outer lock' }
} finally { if($owned){$mutex.ReleaseMutex()};$mutex.Dispose() }
$helper=Join-Path $app 'maintenance/shell.ps1'
$helperBytes=[IO.File]::ReadAllBytes($helper)
try {
    [IO.File]::WriteAllText($helper,'exit 0')
    $failed=Invoke-Hidden $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+(Join-Path $temporary 'changed-helper.log')+'"'))
    if ($failed -eq 0 -or -not (Test-Path -LiteralPath $registration) -or -not (Test-Path -LiteralPath $engineFile)) { throw 'Changed maintenance helper was accepted' }
} finally { [IO.File]::WriteAllBytes($helper,$helperBytes) }
$notice=Join-Path $app 'setup-notices/Inno-Setup.txt'
$noticeBytes=[IO.File]::ReadAllBytes($notice)
try {
    [IO.File]::AppendAllText($notice,'changed user content')
    $failed=Invoke-Hidden $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+(Join-Path $temporary 'changed-notice.log')+'"'))
    if ($failed -eq 0 -or -not (Test-Path -LiteralPath $registration) -or -not (Test-Path -LiteralPath $engineFile)) { throw 'Changed runtime notice was removed' }
} finally { [IO.File]::WriteAllBytes($notice,$noticeBytes) }
try {
    [IO.File]::WriteAllText($engineFile,'changed fixture makes validated uninstall fail')
    $failed=Invoke-Hidden $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+(Join-Path $temporary 'failed-uninstall.log')+'"'))
    if ($failed -eq 0 -or -not (Test-Path -LiteralPath $registration) -or -not (Test-Path -LiteralPath (Join-Path $app 'vcp.exe')) -or -not (Test-Path -LiteralPath $uninstaller)) { throw 'Failed engine uninstall removed registered integration' }
} finally { if(Test-Path -LiteralPath $engineFile){[IO.File]::WriteAllBytes($engineFile,$original)} }
if ((Invoke-Hidden $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+(Join-Path $temporary 'uninstall.log')+'"'))) -ne 0) { throw "Fixture uninstall failed; preserve $temporary for recovery" }
if (Test-Path -LiteralPath $registration) { throw 'Successful uninstall retained registration' }
if ((Test-Path -LiteralPath (Join-Path $app 'vcp.exe')) -or (Test-Path -LiteralPath (Join-Path $app 'engine'))) { throw 'Successful uninstall retained program integration' }
if ([IO.File]::ReadAllText((Join-Path $data 'preserve.sentinel')) -cne 'protected user data' -or [IO.File]::ReadAllText((Join-Path $unowned 'preserve.sentinel')) -cne 'unrelated application') { throw 'Protected or unrelated data changed' }
[ordered]@{schema='vcp-setup-shell-tests/1';status='pass';evidence=$temporary;compiler='6.7.3';cases=@('unowned-root','data-overlap','registered-install','abandoned-lock-recovery','competing-setup','changed-helper-refused','changed-notice-preserved','failed-uninstall-preserves-registration','uninstall-preserves-data');limitations=@('Synthetic engine and launcher; not installed-product beta qualification.')} | ConvertTo-Json -Depth 5
