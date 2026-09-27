# SPDX-License-Identifier: Apache-2.0
# Browser-free native smoke for the controller-owned worker guardian.
# This script accepts no executable or command input. Its private owner mode and
# the C# helper both execute only the fixed waiting PowerShell child.
#requires -Version 7.0
param(
    [ValidateSet('all','owner')][string]$Mode = 'all',
    [string]$OwnerNonce
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') { throw 'Native x64 Windows required' }
if ($Mode -eq 'owner' -and $OwnerNonce -cnotmatch '^[a-f0-9]{64}$') { throw 'Exact owner nonce required' }
if ($Mode -eq 'all' -and $OwnerNonce) { throw 'Owner nonce is private to nested owner mode' }

Add-Type -Path @(
    (Join-Path $PSScriptRoot 'NativeProbe.cs'),
    (Join-Path $PSScriptRoot 'WorkerGuardian.cs'),
    (Join-Path $PSScriptRoot 'WebViewSupervisor.cs'),
    (Join-Path $PSScriptRoot 'ProbeContract.cs'),
    (Join-Path $PSScriptRoot 'DomEvidence.cs')
) -ErrorAction Stop

$workerEnvironment = [ordered]@{}
foreach ($name in @('SystemRoot','WINDIR','SystemDrive','TEMP','TMP','USERPROFILE','LOCALAPPDATA','APPDATA')) {
    $entry = [Environment]::GetEnvironmentVariable($name)
    if ($entry) { $workerEnvironment[$name] = $entry }
}
function Start-FixedGuardian {
    [Vcp.Cs3Draft.NativeProbe]::StartWorkerGuardianSmoke((Join-Path $PSHOME 'pwsh.exe'),[string[]]$workerEnvironment.Keys,[string[]]$workerEnvironment.Values)
}
function Read-HeldWorker($Guardian) {
    $pidLine = $Guardian.StandardOutput.ReadLineAsync()
    if (-not $pidLine.Wait(5000)) { throw 'Guardian smoke PID deadline' }
    [int]$workerPid = 0
    if (-not [int]::TryParse($pidLine.Result,[ref]$workerPid) -or $workerPid -le 0) { throw 'Guardian smoke emitted invalid PID' }
    $identity = [Diagnostics.Process]::GetProcessById($workerPid)
    try {
        $null = $identity.Handle
        if ($Guardian.HasExited) { throw 'Guardian smoke worker exited before owner loss' }
        [pscustomobject]@{ Process=$identity; Pid=$workerPid; CreationFiletime=$identity.StartTime.ToFileTimeUtc() }
    } catch { $identity.Dispose(); throw }
}

if ($Mode -eq 'owner') {
    $guardian = $null; $held = $null
    try {
        $guardian = Start-FixedGuardian
        $held = Read-HeldWorker $guardian
        [ordered]@{schema='cs3-worker-guardian-owner/1';nonce=$OwnerNonce;pid=$held.Pid;creation_filetime=$held.CreationFiletime} | ConvertTo-Json -Compress
        [Console]::Out.Flush()
        # Parent death closes the redirected pipe, causing EOF and cleanup. A
        # fixed deadline independently prevents a stranded owner/helper pair.
        $control = [Console]::In.ReadLineAsync()
        if (-not $control.Wait(30000)) { throw 'Nested guardian owner deadline' }
        if ($control.Result -cne ('STOP ' + $OwnerNonce)) { throw 'Nested guardian owner input closed' }
    } finally {
        if ($null -ne $guardian) { $guardian.Dispose() }
        if ($null -ne $held) { $held.Process.Dispose() }
    }
    return
}

# First prove the wrapper's explicit disposal order.
$guardian = $null; $held = $null
try {
    $guardian = Start-FixedGuardian
    $held = Read-HeldWorker $guardian
    $guardian.Dispose(); $guardian = $null
    if (-not $held.Process.WaitForExit(10000)) { throw 'Worker survived explicit guardian disposal' }
    if ($held.Process.StartTime.ToFileTimeUtc() -ne $held.CreationFiletime) { throw 'Disposed worker identity changed' }
} finally {
    if ($null -ne $guardian) { $guardian.Dispose() }
    if ($null -ne $held) { $held.Process.Dispose() }
}

# Then prove abrupt owner loss independently. The parent launches one exact copy
# of this script in private owner mode, pins both owner and worker identities,
# kills the owner only (never its process tree), and observes the held worker.
$nonce = [Guid]::NewGuid().ToString('N') + [Guid]::NewGuid().ToString('N')
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = Join-Path $PSHOME 'pwsh.exe'
$start.UseShellExecute = $false; $start.CreateNoWindow = $true
$start.RedirectStandardInput = $true; $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
foreach ($argument in @('-NoProfile','-NonInteractive','-File',$PSCommandPath,'-Mode','owner','-OwnerNonce',$nonce)) { $start.ArgumentList.Add($argument) }
$start.Environment.Clear()
foreach ($entry in $workerEnvironment.GetEnumerator()) { $start.Environment[$entry.Key] = $entry.Value }

$owner = [Diagnostics.Process]::new(); $owner.StartInfo = $start
$ownerHeld = $null; $workerHeld = $null; $ownerStarted = $false
$primary = $null; $cleanupErrors = [Collections.Generic.List[Exception]]::new()
try {
    if (-not $owner.Start()) { throw 'Guardian owner process did not start' }
    $ownerStarted = $true
    $null = $owner.Handle; $ownerHeld = $owner.StartTime.ToFileTimeUtc()
    $ownerLine = $owner.StandardOutput.ReadLineAsync(); $ownerError = $owner.StandardError.ReadToEndAsync()
    if (-not $ownerLine.Wait(10000)) { throw 'Guardian owner evidence deadline' }
    $evidence = $ownerLine.Result | ConvertFrom-Json
    if ($evidence.schema -cne 'cs3-worker-guardian-owner/1' -or $evidence.nonce -cne $nonce -or [int]$evidence.pid -le 0 -or [long]$evidence.creation_filetime -le 0) { throw 'Guardian owner evidence differs' }
    $workerHeld = [Diagnostics.Process]::GetProcessById([int]$evidence.pid); $null = $workerHeld.Handle
    if ($workerHeld.StartTime.ToFileTimeUtc() -ne [long]$evidence.creation_filetime -or $workerHeld.HasExited) { throw 'Guardian owner worker identity differs' }
    if ($owner.StartTime.ToFileTimeUtc() -ne $ownerHeld -or $owner.HasExited) { throw 'Guardian owner identity differs' }
    $owner.Kill() # Exact owner only; do not use the entire-process-tree overload.
    if (-not $owner.WaitForExit(10000)) { throw 'Guardian owner termination deadline' }
    if (-not $workerHeld.WaitForExit(10000)) { throw 'Worker survived abrupt guardian owner loss' }
    if (-not $ownerError.Wait(5000)) { throw 'Guardian owner diagnostic drainage deadline' }
    if ($ownerError.Result) { throw 'Guardian owner emitted diagnostics: ' + $ownerError.Result.Substring(0,[Math]::Min(1024,$ownerError.Result.Length)) }
} catch {
    $primary = [Runtime.ExceptionServices.ExceptionDispatchInfo]::Capture($_.Exception)
} finally {
    if ($ownerStarted) {
        try { if (-not $owner.HasExited) { $owner.Kill(); if (-not $owner.WaitForExit(10000)) { throw 'Guardian owner cleanup deadline' } } } catch { $cleanupErrors.Add($_.Exception) }
    }
    if ($null -ne $workerHeld) {
        try {
            if (-not $workerHeld.HasExited) { $workerHeld.Kill(); if (-not $workerHeld.WaitForExit(10000)) { throw 'Fixed worker cleanup deadline' } }
        } catch { $cleanupErrors.Add($_.Exception) }
        try { $workerHeld.Dispose() } catch { $cleanupErrors.Add($_.Exception) }
    }
    try { $owner.Dispose() } catch { $cleanupErrors.Add($_.Exception) }
}
if ($null -ne $primary) { $primary.Throw() }
if ($cleanupErrors.Count) { throw [AggregateException]::new('Guardian smoke cleanup failed',$cleanupErrors) }

'Worker guardian smoke passed: explicit disposal and abrupt sole-owner loss both terminated the independently held fixed worker. No browser was launched.'
