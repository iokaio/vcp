# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$Node, [Parameter(Mandatory)][string]$NodeSha256,
    [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') { throw 'Native x64 Windows required' }
$Node = [IO.Path]::GetFullPath($Node)
if ($NodeSha256 -cnotmatch '^[a-f0-9]{64}$' -or (Get-FileHash -LiteralPath $Node).Hash.ToLowerInvariant() -cne $NodeSha256) { throw 'Node identity mismatch' }
Add-Type -Path (Join-Path $PSScriptRoot '../../src/tests/support/windows/AppContainerFixture.cs')
$sourcePaths = @($PSCommandPath, (Join-Path $PSScriptRoot 'node-fixture-runner.ps1'),
    (Join-Path $PSScriptRoot 'node-fixture-bootstrap.cjs'), (Join-Path $PSScriptRoot 'node-fixture-protocol.cjs'),
    (Join-Path $PSScriptRoot '../../src/tests/support/windows/AppContainerFixture.cs'),
    (Join-Path $PSScriptRoot '../../src/tests/support/windows/node-fixture-owner.ps1'))
$sourceHashes = @($sourcePaths | ForEach-Object { @{ file = [IO.Path]::GetFileName($_); sha256 = (Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant() } })
$nodeVersion = (& $Node --version).Trim()
$results = [Collections.Generic.List[object]]::new()
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-node-qualification-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($root) | Out-Null
$ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
function Assert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function New-Fixture {
    $fixture = [Vcp.Qualification.AppContainerFixture]::new()
    try {
        Copy-Item -LiteralPath $Node -Destination (Join-Path $fixture.Root 'node.exe')
        Assert ((Get-FileHash -LiteralPath (Join-Path $fixture.Root 'node.exe')).Hash.ToLowerInvariant() -ceq $NodeSha256) 'Copied Node identity mismatch'
        & icacls (Join-Path $fixture.Root 'node.exe') /setintegritylevel M | Out-Null
        Assert ($LASTEXITCODE -eq 0) 'Cannot label owned executable'
        [IO.File]::WriteAllText((Join-Path $fixture.Root 'package.json'), '{"type":"commonjs"}')
        return $fixture
    } catch { $fixture.Dispose(); throw }
}
function Invoke-Case([string]$Name, [string]$Code, [scriptblock]$Check,
    [int]$Limit = 65536, [ulong]$Memory = 268435456, [int]$Timeout = 5000, [int]$CancelAfter = 0) {
    $fixture = New-Fixture
    $profileRoot = $fixture.Root
    $cancel = [Threading.CancellationTokenSource]::new()
    try {
        if ($CancelAfter -gt 0) { $cancel.CancelAfter($CancelAfter) }
        $receipt = $fixture.RunBounded((Join-Path $fixture.Root 'node.exe'), @('--no-addons', '-e', $Code),
            $true, [byte[]]::new(0), $Limit, $Memory, $Timeout, $cancel.Token)
        $record = [ordered]@{ case = $Name; passed = $false; receipt = $receipt }
        $results.Add($record)
        Assert $receipt.Process.AppContainer 'Missing actual AppContainer token'
        Assert ($receipt.Process.CapabilityCount -eq 0 -and $receipt.Process.TokenSidMatchesProfile) 'Token authority mismatch'
        & $Check $receipt
        $record.passed = $true
    } finally { $cancel.Dispose(); $fixture.Dispose() }
    Assert (-not [IO.Directory]::Exists($profileRoot)) 'Profile files survived disposal'
}
try {
    $legacy = New-Fixture
    try {
        $legacyResult = $legacy.Run((Join-Path $legacy.Root 'node.exe'), @('-e', 'process.exit(0)'), $true, 5000)
        Assert ($legacyResult.ExitCode -eq 0 -and $legacyResult.AppContainer) 'Existing Run API regressed'
        $results.Add([ordered]@{ case = 'existing Run API'; passed = $true; receipt = $legacyResult })
    } finally { $legacy.Dispose() }
    $env:VCP_NODE_SECRET_CANARY = 'synthetic-not-a-secret'
    Invoke-Case 'minimal environment' @'
process.stdout.write(JSON.stringify({ secret: Object.hasOwn(process.env, 'VCP_NODE_SECRET_CANARY'), keys: Object.keys(process.env).sort() }));
'@ {
        param($r)
        Assert ($r.Process.ExitCode -eq 0) 'Environment probe failed'
        $value = [Text.Encoding]::UTF8.GetString($r.Output) | ConvertFrom-Json
        Assert (-not $value.secret) 'Environment boundary failed'
        Assert (($value.keys -join ',') -ceq 'APPDATA,LOCALAPPDATA,PATH,SYSTEMROOT,TEMP,TMP,USERPROFILE,WINDIR') 'Unexpected inherited environment'
    }
    Remove-Item Env:VCP_NODE_SECRET_CANARY
    Invoke-Case 'denied process creation' @'
const cp = require('node:child_process');
process.stdout.write('attempt\n');
const child = cp.spawn(process.execPath, ['-e', 'process.stdout.write("unexpected")']);
child.on('error', e => { process.stdout.write(e.code); });
'@ {
        param($r)
        $text = [Text.Encoding]::UTF8.GetString($r.Output)
        Assert ($r.PeakActiveProcesses -eq 1 -and $text.StartsWith("attempt`n") -and -not $text.Contains('unexpected')) 'Process containment observation failed'
        Assert ($r.Termination -eq 'timeout' -or ($r.Process.ExitCode -eq 0 -and $text -match '(EPERM|EACCES|EAGAIN)$')) 'Unbounded child creation attempt'
    } -Timeout 500
    Invoke-Case 'simultaneous output ceiling' @'
const fs = require('node:fs'), chunk = Buffer.alloc(4096, 120);
while (true) { fs.writeSync(1, chunk); fs.writeSync(2, chunk); }
'@ {
        param($r)
        Assert ($r.Termination -eq 'output_limit') 'Output flood was not stopped'
        Assert ($r.Output.Length + $r.Error.Length -le 1024) 'Retained output exceeded ceiling'
        Assert ($r.OutputBytes + $r.ErrorBytes -gt 1024) 'Output overflow was not observed'
    } -Limit 1024
    Invoke-Case 'wall timeout' 'setInterval(() => {}, 1000)' {
        param($r); Assert ($r.Termination -eq 'timeout' -and $r.Process.WallMilliseconds -lt 6000) 'Timeout cleanup failed'
    } -Timeout 200
    Invoke-Case 'explicit cancellation' 'setInterval(() => {}, 1000)' {
        param($r); Assert ($r.Termination -eq 'cancelled' -and $r.Process.WallMilliseconds -lt 6000) 'Cancellation cleanup failed'
    } -CancelAfter 250
    Invoke-Case 'committed memory ceiling' 'const chunks = []; while (true) chunks.push(Buffer.alloc(8 * 1024 * 1024, 1));' {
        param($r)
        Assert ($r.Process.ExitCode -ne 0 -and $r.Termination -eq 'exited') 'Memory exhaustion did not fail before timeout'
        Assert ($r.MemoryLimitBytes -eq 67108864 -and $r.Process.PeakJobCommittedBytes -le 67108864) 'Memory ceiling was not preserved'
    } -Memory 67108864

    $outside = Join-Path $root 'outside'
    [IO.Directory]::CreateDirectory($outside) | Out-Null
    [IO.File]::WriteAllText((Join-Path $outside 'read.txt'), 'synthetic outside canary')
    & icacls $outside /grant ('*' + $ownerSid + ':(OI)(CI)F') /setintegritylevel '(OI)(CI)L' | Out-Null
    Assert ($LASTEXITCODE -eq 0) 'Cannot prepare disposable outside canary'
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try {
        foreach ($mode in @('control-before', 'restricted', 'control-after')) {
            $fixture = New-Fixture
            $redirect = Join-Path $fixture.Root 'redirect'
            try {
                [IO.Directory]::CreateDirectory($redirect) | Out-Null
                [IO.Directory]::Delete($redirect)
                New-Item -Path $redirect -ItemType Junction -Target $outside | Out-Null
                $inputText = @{ outside = $outside; port = $listener.LocalEndpoint.Port; nonce = [Guid]::NewGuid().ToString('N') } | ConvertTo-Json -Compress
                $code = @'
const fs = require('node:fs'), net = require('node:net'), path = require('node:path');
const input = JSON.parse(fs.readFileSync(0, 'utf8')), report = {};
function attempt(name, action) { try { action(); report[name] = true; } catch (e) { report[name] = false; report[name + '_code'] = e.code; } }
attempt('inside', () => fs.writeFileSync('inside.txt', 'owned'));
attempt('read', () => fs.readFileSync(path.join(input.outside, 'read.txt')));
attempt('write', () => fs.writeFileSync(path.join(input.outside, 'write.txt'), input.nonce));
attempt('junction', () => fs.readFileSync('redirect/read.txt'));
const socket = net.connect(input.port, '127.0.0.1');
socket.setTimeout(1000);
socket.on('connect', () => { report.network = true; socket.end(input.nonce, () => socket.destroy()); });
socket.on('error', () => { report.network = false; });
socket.on('timeout', () => { report.network = false; socket.destroy(); });
socket.on('close', () => process.stdout.write(JSON.stringify(report)));
'@
                $restricted = $mode -eq 'restricted'
                $r = $fixture.RunBounded((Join-Path $fixture.Root 'node.exe'), @('--no-addons', '-e', $code), $restricted,
                    [Text.Encoding]::UTF8.GetBytes($inputText), 65536, 268435456, 5000, [Threading.CancellationToken]::None)
                $record = [ordered]@{ case = $mode; passed = $false; observations = $null; receipt = $r }
                $results.Add($record)
                Assert ($r.Process.ExitCode -eq 0) 'Boundary probe failed'
                $value = [Text.Encoding]::UTF8.GetString($r.Output) | ConvertFrom-Json
                $record.observations = $value
                Assert $value.inside 'Owned write failed'
                foreach ($name in @('read', 'write', 'junction', 'network')) { Assert ($value.$name -eq (-not $restricted)) ('Unexpected ' + $mode + ' ' + $name) }
                if ($restricted) { Assert (-not $listener.Pending()) 'Restricted TCP canary reached listener' }
                else {
                    Assert $listener.Pending() 'Control TCP connection missing'
                    $client = $listener.AcceptTcpClient()
                    try {
                        $stream = $client.GetStream(); $stream.ReadTimeout = 2000
                        $buffer = [byte[]]::new(64); $length = $stream.Read($buffer, 0, 64)
                        Assert ([Text.Encoding]::UTF8.GetString($buffer, 0, $length) -ceq ($inputText | ConvertFrom-Json).nonce) 'Control nonce mismatch'
                    } finally { $client.Dispose() }
                }
                $record.passed = $true
            } finally {
                if ([IO.Directory]::Exists($redirect)) {
                    Assert (((Get-Item -LiteralPath $redirect).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) 'Owned junction changed'
                    [IO.Directory]::Delete($redirect)
                }
                $fixture.Dispose()
            }
        }
    } finally { $listener.Stop() }
    # The supervisor retains the owned profile identity before deliberately killing
    # its broker. No process is selected by name or guessed PID.
    $statePath = Join-Path $root 'owner-state.json'
    $ownerHelper = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../src/tests/support/windows/node-fixture-owner.ps1'))
    $start = [Diagnostics.ProcessStartInfo]::new((Get-Command pwsh).Source)
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    foreach ($argument in @('-NoProfile', '-File', $ownerHelper, '-Node', $Node, '-NodeSha256', $NodeSha256, '-State', $statePath)) { $start.ArgumentList.Add($argument) }
    $owner = [Diagnostics.Process]::Start($start)
    $profile = $null; $child = $null
    try {
        $deadline = [Diagnostics.Stopwatch]::StartNew()
        while (-not [IO.File]::Exists($statePath) -and $deadline.ElapsedMilliseconds -lt 10000 -and -not $owner.HasExited) { Start-Sleep -Milliseconds 25 }
        Assert ([IO.File]::Exists($statePath)) 'Owner profile receipt missing'
        $profile = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
        $expectedRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('Packages/' + $profile.name + '/AC')
        Assert ($profile.name -cmatch '^iokaio\.vcp\.memory\.[a-f0-9]{32}$' -and
            [IO.Path]::GetFullPath($profile.root) -ceq [IO.Path]::GetFullPath($expectedRoot)) 'Unexpected owner-loss profile identity'
        $childState = Join-Path $profile.root 'child.json'
        while (-not [IO.File]::Exists($childState) -and $deadline.ElapsedMilliseconds -lt 10000 -and -not $owner.HasExited) { Start-Sleep -Milliseconds 25 }
        Assert ([IO.File]::Exists($childState)) 'Owned Node readiness missing'
        $childId = (Get-Content -LiteralPath $childState -Raw | ConvertFrom-Json).pid
        $child = [Diagnostics.Process]::GetProcessById($childId)
        Assert (-not $child.HasExited) 'Owned Node exited before owner-loss test'
        $owner.Kill(); Assert $owner.WaitForExit(5000) 'Owner did not exit'
        Assert $child.WaitForExit(5000) 'Node survived abrupt owner loss'
        $results.Add([ordered]@{ case = 'abrupt owner loss'; passed = $true; owned_pid = $childId; profile_cleanup = 'supervisor' })
    } finally {
        if (-not $owner.HasExited) { $owner.Kill(); $null = $owner.WaitForExit(5000) }
        $owner.Dispose()
        if ($null -ne $child) { $child.Dispose() }
        if ($null -ne $profile) {
            Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class NodeQualificationCleanup {
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)] public static extern int DeleteAppContainerProfile(string name);
}
'@
            [Runtime.InteropServices.Marshal]::ThrowExceptionForHR([NodeQualificationCleanup]::DeleteAppContainerProfile($profile.name))
            Assert (-not [IO.Directory]::Exists($profile.root)) 'Owner-loss profile cleanup failed'
        }
    }
    Assert ((Get-FileHash -LiteralPath $Node).Hash.ToLowerInvariant() -ceq $NodeSha256) 'Node changed during qualification'
    for ($i = 0; $i -lt $sourcePaths.Count; $i++) {
        Assert ((Get-FileHash -LiteralPath $sourcePaths[$i]).Hash.ToLowerInvariant() -ceq $sourceHashes[$i].sha256) 'Qualification source changed during execution'
    }
} finally {
    [ordered]@{ schema = 1; node_sha256 = $NodeSha256; node_version = $nodeVersion;
        windows_version = [Environment]::OSVersion.VersionString; architecture = 'x64'; sources = $sourceHashes;
        cases = $results } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $Output
    # The generated absolute root is owned by this invocation. Never recurse through links.
    if ([IO.Path]::GetFullPath($root) -ne $root -or (Get-Item -LiteralPath $root).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unsafe cleanup root' }
    Remove-Item -LiteralPath $root -Recurse -Force
}
