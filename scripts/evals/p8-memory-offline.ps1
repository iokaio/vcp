# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Config schema vcp-production-memory-offline-request/1:
# output, executable, executable_sha256, fixture_binary, fixture_sha256,
# canary_binary, canary_sha256, assets, asset_specification, asset_specification_sha256,
# network: {address, port, nonce_before, nonce_blocked, nonce_after}.
# All binaries are independently built inputs; this broker never compiles VCP,
# downloads assets, inherits provider credentials, or enables a model provider.
[CmdletBinding(DefaultParameterSetName='Run')]
param(
    [Parameter(Mandatory,ParameterSetName='Run')][string]$Config,
    [Parameter(Mandatory,ParameterSetName='SelfTest')][switch]$SelfTest
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Native Windows required' }
function Plain([string]$Path) {
    $full=[IO.Path]::GetFullPath($Path)
    if ($full.StartsWith('\\') -or -not [IO.Path]::IsPathFullyQualified($full)) { throw 'Local absolute path required' }
    for ($entry=[IO.FileInfo]::new($full); $null -ne $entry; $entry=if ($entry -is [IO.DirectoryInfo]) {$entry.Parent} else {$entry.Directory}) {
        if ($entry.Exists -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse path rejected' }
    }
    return $full
}
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Verify([string]$Path,[string]$Digest) {
    $full=Plain $Path
    if ($Digest -cnotmatch '^[a-f0-9]{64}$' -or (Hash $full) -cne $Digest) { throw 'Input digest mismatch' }
    return $full
}
function New-Json([string]$Path,$Value) {
    $stream=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try { $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 40)+"`n"); $stream.Write($bytes) } finally {$stream.Dispose()}
}
function Integer($Value) { return ($Value -is [int] -or $Value -is [long] -or $Value -is [uint32] -or $Value -is [uint64]) }
function Assert-Canary($Observation,[string]$Nonce,[bool]$Denied) {
    if ($Observation.nonce -cne $Nonce -or $Observation.outcome -cne $(if($Denied){'blocked'}else{'connected'})) { throw 'Canary outcome/nonce mismatch' }
    if ($Denied -and (-not (Integer $Observation.diagnosis_status) -or $Observation.diagnosis_status -ne 0 -or
        -not (Integer $Observation.missing_capability) -or $Observation.missing_capability -notin @(1,2,3) -or
        -not (($Observation.timed_out -is [bool] -and $Observation.timed_out) -or ((Integer $Observation.os_error) -and $Observation.os_error -eq 10013)))) { throw 'Insufficient network-denial diagnosis' }
}
function Assert-Token($Token,[bool]$Restricted) {
    if (-not (Integer $Token.ExitCode) -or $Token.ExitCode -ne 0 -or
        $Token.AppContainer -isnot [bool] -or $Token.AppContainer -ne $Restricted -or
        -not (Integer $Token.CapabilityCount) -or $Token.CapabilityCount -ne 0 -or
        $Token.TokenSidMatchesProfile -isnot [bool] -or $Token.TokenSidMatchesProfile -ne $Restricted -or
        $Token.IntegrityLevel -cne $(if($Restricted){'S-1-16-4096'}else{'S-1-16-8192'}) -or
        -not (Integer $Token.PeakJobCommittedBytes) -or $Token.PeakJobCommittedBytes -le 0 -or $Token.PeakJobCommittedBytes -gt 9007199254740991 -or
        -not (Integer $Token.WallMilliseconds) -or $Token.WallMilliseconds -lt 0 -or $Token.WallMilliseconds -gt 9007199254740991) { throw 'Incomplete process/token/resource evidence' }
}
if ($SelfTest) {
    $nonce='a'*32; $controls=0
    $denial=@{outcome='blocked';nonce=$nonce;diagnosis_status=0;missing_capability=2;timed_out=$true;os_error=10060}
    $token=@{ExitCode=0;AppContainer=$true;CapabilityCount=0;TokenSidMatchesProfile=$true;IntegrityLevel='S-1-16-4096';PeakJobCommittedBytes=250000000;WallMilliseconds=5000}
    Assert-Canary ([pscustomobject]$denial) $nonce $true; $controls++
    $permission=$denial.Clone(); $permission.timed_out=$false; $permission.os_error=10013
    Assert-Canary ([pscustomobject]$permission) $nonce $true; $controls++
    Assert-Canary ([pscustomobject]@{outcome='connected';nonce=$nonce}) $nonce $false; $controls++
    Assert-Token ([pscustomobject]$token) $true; $controls++
    $desktop=$token.Clone(); $desktop.AppContainer=$false; $desktop.TokenSidMatchesProfile=$false; $desktop.IntegrityLevel='S-1-16-8192'
    Assert-Token ([pscustomobject]$desktop) $false; $controls++
    foreach ($change in @(@{nonce='b'*32},@{outcome='connected'},@{diagnosis_status=5},@{diagnosis_status='0'},@{missing_capability=0},@{missing_capability=4},@{timed_out=$false;os_error=10060},@{timed_out='true';os_error=10060})) {
        $invalid=$denial.Clone(); foreach($key in $change.Keys){$invalid[$key]=$change[$key]}
        $rejected=$false; try {Assert-Canary ([pscustomobject]$invalid) $nonce $true} catch {$rejected=$true}
        if(-not $rejected){throw 'Invalid denial evidence accepted'}; $controls++
    }
    foreach ($change in @(@{ExitCode=1},@{AppContainer=$false},@{CapabilityCount=1},@{TokenSidMatchesProfile=$false},@{IntegrityLevel='S-1-16-8192'},@{PeakJobCommittedBytes=0},@{PeakJobCommittedBytes='100'},@{WallMilliseconds=-1},@{WallMilliseconds=0.5})) {
        $invalid=$token.Clone(); foreach($key in $change.Keys){$invalid[$key]=$change[$key]}
        $rejected=$false; try {Assert-Token ([pscustomobject]$invalid) $true} catch {$rejected=$true}
        if(-not $rejected){throw 'Invalid token evidence accepted'}; $controls++
    }
    foreach ($invalid in @(@{outcome='connected';nonce='b'*32},@{outcome='blocked';nonce=$nonce})) {
        $rejected=$false; try {Assert-Canary ([pscustomobject]$invalid) $nonce $false} catch {$rejected=$true}
        if(-not $rejected){throw 'Invalid positive control accepted'}; $controls++
    }
    $desktop.CapabilityCount=1; $rejected=$false
    try {Assert-Token ([pscustomobject]$desktop) $false} catch {$rejected=$true}
    if(-not $rejected){throw 'Desktop capability mismatch accepted'}; $controls++
    $desktop.CapabilityCount=0; $desktop.IntegrityLevel='S-1-16-4096'; $rejected=$false
    try {Assert-Token ([pscustomobject]$desktop) $false} catch {$rejected=$true}
    if(-not $rejected){throw 'Accidental low desktop control accepted'}; $controls++
    @{status='passed';controls=$controls;child_processes=0;network_calls=0} | ConvertTo-Json -Compress
    return
}
$request=Get-Content -LiteralPath (Plain $Config) -Raw | ConvertFrom-Json
if ($request.schema -cne 'vcp-production-memory-offline-request/1') { throw 'Unsupported request schema' }
$output=Plain $request.output
if (Test-Path -LiteralPath $output) { throw 'New output directory required' }
$exe=Verify $request.executable $request.executable_sha256
$seed=Verify $request.fixture_binary $request.fixture_sha256
$canary=Verify $request.canary_binary $request.canary_sha256
$specification=Verify $request.asset_specification $request.asset_specification_sha256
if ($request.asset_specification_sha256 -cne (Hash (Join-Path $PSScriptRoot '../../src/third_party/components/minilm-assets.json'))) { throw 'Asset specification differs from repository pin' }
$spec=Get-Content -LiteralPath $specification -Raw | ConvertFrom-Json
if ($spec.schema_version -ne 1 -or $spec.id -cne 'all-minilm-l6-v2' -or $spec.files.Count -ne 10) { throw 'Pinned ten-file MiniLM specification required' }
$assets=Plain $request.assets
$network=$request.network
$address=[Net.IPAddress]::Parse($network.address)
$octets=$address.GetAddressBytes()
if ($octets.Length -ne 4 -or -not ($octets[0] -eq 10 -or ($octets[0] -eq 172 -and $octets[1] -ge 16 -and $octets[1] -le 31) -or ($octets[0] -eq 192 -and $octets[1] -eq 168)) -or $network.port -lt 1 -or $network.port -gt 65535) { throw 'Private IPv4 canary address and nonzero port required' }
$nonces=@($network.nonce_before,$network.nonce_blocked,$network.nonce_after)
if (@($nonces | Select-Object -Unique).Count -ne 3 -or @($nonces | Where-Object {$_ -cnotmatch '^[a-f0-9]{32}$'}).Count) { throw 'Three distinct canary nonces required' }
$support=Join-Path $PSScriptRoot '../../src/tests/support/windows/AppContainerFixture.cs'
Add-Type -Path $support
# Native children inherit OS handles, not PowerShell's managed output pipeline.
# Change only the broker's two handles for the duration of a synchronous Run;
# create-new files bind each command's bytes independently before parsing them.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
public sealed class P8MemoryTraffic : IDisposable {
    readonly TcpListener listener; readonly Task task;
    readonly List<string> lines = new List<string>();
    public P8MemoryTraffic(string address, int port) {
        listener=new TcpListener(IPAddress.Parse(address),port); listener.Start(4);
        task=Task.Run(async () => {
            while (true) {
                TcpClient client;
                try { client=await listener.AcceptTcpClientAsync(); }
                catch (SocketException) { break; } catch (ObjectDisposedException) { break; }
                using (client) {
                    client.ReceiveTimeout=3000;
                    using (var stream=client.GetStream()) {
                        var bytes=new List<byte>();
                        try { while(bytes.Count<128) { int b=stream.ReadByte(); if(b<0 || b==10)break; bytes.Add((byte)b); } }
                        catch(IOException) { bytes.Add(0); }
                        lock(lines) { lines.Add(Encoding.ASCII.GetString(bytes.ToArray())); }
                    }
                }
            }
        });
    }
    public string[] Snapshot() { lock(lines) { return lines.ToArray(); } }
    public void Dispose() { listener.Stop(); if(!task.Wait(5000))throw new IOException("Canary observer did not stop"); }
}
public static class P8MemoryOutput {
    [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int kind);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetStdHandle(int kind,IntPtr value);
    public static object Run(object fixture,string executable,string[] arguments,bool restricted,int timeout,string stdout,string stderr) {
        using(var output=new FileStream(stdout,FileMode.CreateNew,FileAccess.Write,FileShare.Read))
        using(var error=new FileStream(stderr,FileMode.CreateNew,FileAccess.Write,FileShare.Read)) {
            IntPtr oldOut=GetStdHandle(-11),oldErr=GetStdHandle(-12);
            try {
                if(!SetStdHandle(-11,output.SafeFileHandle.DangerousGetHandle()) || !SetStdHandle(-12,error.SafeFileHandle.DangerousGetHandle()))throw new IOException("Output handle setup failed");
                return fixture.GetType().GetMethod("Run").Invoke(fixture,new object[]{executable,arguments,restricted,timeout});
            } finally { bool a=SetStdHandle(-11,oldOut),b=SetStdHandle(-12,oldErr); if(!a||!b)throw new IOException("Output handle restoration failed"); }
        }
    }
}
'@
[IO.Directory]::CreateDirectory($output) | Out-Null
New-Json (Join-Path $output 'request.json') $request
$report=[ordered]@{schema='vcp-production-memory-offline/1';status='running';created_at=[DateTime]::UtcNow.ToString('o');script_sha256=(Hash $PSCommandPath);supervisor_sha256=(Hash $support);steps=@();cleanup='pending';limitations=@('Current host and tiny governed-memory corpus; not minimum hardware, large-corpus quality, or clean Windows evidence.','AppContainer denial is test confinement, not the production sandbox.','Per-process hard deadline 180 seconds; native inference cancellation is not qualified by this broker.')}
$fixture=$null; $traffic=$null
function Step([string]$Name,[string]$Program,[string[]]$Arguments,[bool]$Restricted,[bool]$Raw=$false) {
    $stdout=Join-Path $output ($Name+'.stdout.jsonl'); $stderr=Join-Path $output ($Name+'.stderr.log')
    try { $result=[P8MemoryOutput]::Run($fixture,$Program,$Arguments,$Restricted,180000,$stdout,$stderr) }
    catch {
        $failed=[ordered]@{name=$Name;arguments=$Arguments;restricted=$Restricted;supervision_error=$_.Exception.Message}
        foreach ($file in @($stdout,$stderr)) { if(Test-Path -LiteralPath $file) {$failed[[IO.Path]::GetFileName($file)]=@{sha256=(Hash $file);bytes=(Get-Item -LiteralPath $file).Length}} }
        New-Json (Join-Path $output ($Name+'.receipt.json')) $failed
        $report.steps+=,$failed
        throw
    }
    $row=[ordered]@{name=$Name;arguments=$Arguments;restricted=$Restricted;token=$result;stdout_sha256=(Hash $stdout);stderr_sha256=(Hash $stderr);stdout_bytes=(Get-Item -LiteralPath $stdout).Length;stderr_bytes=(Get-Item -LiteralPath $stderr).Length}
    New-Json (Join-Path $output ($Name+'.receipt.json')) $row
    $report.steps+=,$row
    Assert-Token $result $Restricted
    if ($row.stdout_bytes -gt 16MB -or $row.stderr_bytes -gt 16MB) { throw 'Command receipt exceeds parsing bound' }
    if ($Raw) { return $result }
    return @(Get-Content -LiteralPath $stdout | Where-Object {$_} | ForEach-Object { $_ | ConvertFrom-Json })
}
function Invoke-MemoryCommand([string]$Name,$Row,[string[]]$Arguments) {
    $frames=Step $Name $program (@('--format','jsonl','--non-interactive','--workspace',$Row.workspace,'--data-dir',$Row.data)+$Arguments) $true
    $results=@($frames | Where-Object type -eq 'result')
    if ($results.Count -ne 1) { throw 'Exactly one command result required' }
    return $results[0].data
}
try {
    $fixture=[Vcp.Qualification.AppContainerFixture]::new()
    $report.profile=$fixture.Name; $report.sid=$fixture.Sid
    $program=Join-Path $fixture.Root 'vcp.exe'; $seeder=Join-Path $fixture.Root 'memory-fixture.exe'; $probe=Join-Path $fixture.Root 'canary.exe'
    $ownerSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    foreach ($pair in @(@($exe,$program,$request.executable_sha256),@($seed,$seeder,$request.fixture_sha256),@($canary,$probe,$request.canary_sha256))) {
        Copy-Item -LiteralPath $pair[0] -Destination $pair[1]
        $null=Verify $pair[1] $pair[2]
        # Match execution-boundary.ps1: the desktop control must not inherit
        # low integrity from the AppContainer directory's executable label.
        & icacls $pair[1] /grant ('*'+$ownerSid+':F') | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Cannot grant owned fixture executable' }
        & icacls $pair[1] /setintegritylevel M | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Cannot label owned fixture executable' }
    }
    $model=Join-Path $fixture.Root 'model'; [IO.Directory]::CreateDirectory($model) | Out-Null
    foreach ($file in $spec.files) {
        if ($file.path -cnotmatch '^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$' -or @($file.path.Split('/') | Where-Object {$_ -in @('.','..')}).Count) { throw 'Invalid pinned asset path' }
        $source=Verify (Join-Path $assets $file.path) $file.sha256
        if ((Get-Item -LiteralPath $source).Length -ne $file.bytes) { throw 'Asset size mismatch' }
        $dest=Join-Path $model $file.path; [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($dest)) | Out-Null
        Copy-Item -LiteralPath $source -Destination $dest; $null=Verify $dest $file.sha256
    }
    $traffic=[P8MemoryTraffic]::new($network.address,[int]$network.port)
    $before=Step 'network-before' $probe @('--network-control',$network.address,[string]$network.port,$network.nonce_before) $false
    if ($before.Count -ne 1 -or $before[0].status -cne 'pass' -or $before[0].phase -cne 'network-control') { throw 'Canary positive control missing' }
    Assert-Canary $before[0].observation $network.nonce_before $false
    $blocked=Step 'network-blocked' $probe @('--offline',$model,$network.address,[string]$network.port,$network.nonce_blocked) $true
    if ($blocked.Count -ne 1 -or $blocked[0].status -cne 'pass') { throw 'Canary network denial missing' }
    Assert-Canary $blocked[0].network.before $network.nonce_blocked $true
    Assert-Canary $blocked[0].network.after $network.nonce_blocked $true
    New-Json (Join-Path $fixture.Root 'memory-offline-seed-request.json') @{schema='vcp-memory-offline-seed-request/1'}
    # Rust's test harness writes text, not JSON; capture its independent receipt.
    $null=Step 'seed' $seeder @('--ignored','--exact','production_memory_prepare_offline_fixture') $false $true
    $seedFile=Join-Path $fixture.Root 'memory-offline-fixtures.json'
    $seedDigest=Hash $seedFile
    $report.fixture_receipt_sha256=$seedDigest
    $fixtures=Get-Content -LiteralPath $seedFile -Raw | ConvertFrom-Json
    if ($fixtures.schema -cne 'vcp-memory-offline-fixtures/1' -or $fixtures.fixtures.Count -ne 2 -or (@($fixtures.fixtures.backend | Sort-Object) -join ',') -cne 'files,sqlite') { throw 'Both-store seed receipt required' }
    New-Json (Join-Path $output 'fixtures.json') $fixtures
    foreach ($row in $fixtures.fixtures) {
        foreach ($path in @($row.workspace,$row.data,$row.canonical_root)) {
            $checked=[IO.Path]::GetFullPath($path)
            if ($checked.StartsWith('\\?\')) { $checked=$checked.Substring(4) }
            if (-not $checked.StartsWith($fixture.Root+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Seed fixture escaped owned AppContainer' }
        }
        $built=Invoke-MemoryCommand ($row.backend+'-build') $row @('memory','build','--assets',$model)
        if ($built.status -cne 'published' -or $null -eq $built.generation) { throw 'Vector generation was not published' }
        foreach ($iteration in @(1,2)) {
            $queried=Invoke-MemoryCommand ($row.backend+'-query-'+$iteration) $row @('memory','query','retained orchard apple harvest','--assets',$model,'--task','task')
            if ($queried.status -cne 'queried' -or $queried.response.passages.Count -eq 0) { throw 'Reopened vector recall missing' }
            if (@($queried.response.passages | Where-Object {$null -ne $_.rank.vector_rank}).Count -eq 0) { throw 'Recall did not use vectors' }
            foreach ($passage in $queried.response.passages) {
                if ($passage.scope.workspace -cne 'workspace' -or $passage.scope.task -cne 'task' -or ($passage | ConvertTo-Json -Depth 30) -match 'foreign-task-sentinel') { throw 'Query scope oracle failed' }
            }
        }
        $excluded=Invoke-MemoryCommand ($row.backend+'-scope') $row @('memory','query','retained','--assets',$model,'--task','absent-task')
        if ($excluded.status -cne 'queried' -or $excluded.response.passages.Count -ne 0) { throw 'Absent task filter returned passages' }
    }
    $null=Step 'verify' $seeder @('--ignored','--exact','production_memory_verify_offline_fixture') $false $true
    $verification=Get-Content -LiteralPath (Join-Path $fixture.Root 'memory-offline-verification.json') -Raw | ConvertFrom-Json
    if ($verification.passed -ne $true -or $verification.fixture_receipt_sha256 -cne $seedDigest -or (Hash $seedFile) -cne $seedDigest) { throw 'Post-run oracle receipt binding failed' }
    New-Json (Join-Path $output 'verification.json') $verification
    $after=Step 'network-after' $probe @('--network-control',$network.address,[string]$network.port,$network.nonce_after) $false
    if ($after.Count -ne 1 -or $after[0].status -cne 'pass' -or $after[0].phase -cne 'network-control') { throw 'Canary final positive control missing' }
    Assert-Canary $after[0].observation $network.nonce_after $false
    $traffic.Dispose(); $observed=$traffic.Snapshot(); $traffic=$null
    New-Json (Join-Path $output 'network-traffic.json') @{observed=$observed;expected=@(('VCP_LOCAL_CANARY '+$network.nonce_before),('VCP_LOCAL_CANARY '+$network.nonce_after))}
    if ($observed.Count -ne 2 -or $observed[0] -cne ('VCP_LOCAL_CANARY '+$network.nonce_before) -or $observed[1] -cne ('VCP_LOCAL_CANARY '+$network.nonce_after)) { throw 'Independent canary traffic differs from exactly two positive controls' }
    foreach ($pair in @(@($exe,$program,$request.executable_sha256),@($seed,$seeder,$request.fixture_sha256),@($canary,$probe,$request.canary_sha256))) {
        $null=Verify $pair[0] $pair[2]; $null=Verify $pair[1] $pair[2]
    }
    $null=Verify $specification $request.asset_specification_sha256
    foreach ($file in $spec.files) { $null=Verify (Join-Path $assets $file.path) $file.sha256; $null=Verify (Join-Path $model $file.path) $file.sha256 }
    if ((Hash $PSCommandPath) -cne $report.script_sha256 -or (Hash $support) -cne $report.supervisor_sha256) { throw 'Broker source changed during execution' }
    $report.inputs_stable=$true
    $report.status='passed'
} catch {
    $report.status='failed'; $report.error=$_.Exception.Message
} finally {
    try {
        try { if ($null -ne $traffic) {$traffic.Dispose()} }
        finally { if ($null -ne $fixture) {$fixture.Dispose()} }
        $report.cleanup='completed'
    }
    catch { $report.cleanup='failed'; $report.status='failed'; $report.cleanup_error=$_.Exception.Message }
    $report.finished_at=[DateTime]::UtcNow.ToString('o')
    New-Json (Join-Path $output 'result.json') $report
}
if ($report.status -ne 'passed') { throw "Production offline memory qualification failed; see $output" }
