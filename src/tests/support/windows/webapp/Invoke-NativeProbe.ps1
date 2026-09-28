# SPDX-License-Identifier: Apache-2.0
# Draft controller/worker. Default mode only compiles managed source.
# Browser execution is a separately reviewed, explicit -Execute operation.
#requires -Version 7.0
param(
    [ValidateSet('compile-only','webview2-dom','worker','reconcile')][string]$Mode = 'compile-only',
    [switch]$Execute,
    [string]$Receipt,
    [string]$ExpectedInputsSha256,
    [ValidateRange(0,5000)][int]$PauseBeforeResumeMilliseconds = 0,
    [switch]$CancelAfterResume
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Controller-helpers.ps1')
. (Join-Path $PSScriptRoot 'Input-Policy.ps1')
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') { throw 'Native x64 Windows required' }
Add-Type -Path @((Join-Path $PSScriptRoot 'NativeProbe.cs'),(Join-Path $PSScriptRoot 'WorkerGuardian.cs'),(Join-Path $PSScriptRoot 'WebViewSupervisor.cs'),(Join-Path $PSScriptRoot 'ProbeContract.cs'),(Join-Path $PSScriptRoot 'DomEvidence.cs'),(Join-Path $PSScriptRoot 'FrozenWebEvidence.cs'),(Join-Path $PSScriptRoot 'FrozenWebResources.cs'),(Join-Path $PSScriptRoot 'UiArtifactResource.cs'),(Join-Path $PSScriptRoot 'UiArtifactEvidence.cs'),(Join-Path $PSScriptRoot 'InputDiagnosticEvidence.cs'),(Join-Path $PSScriptRoot 'WebDomContract.cs')) -ErrorAction Stop
if ($Mode -eq 'compile-only') { $webCount=[Vcp.Cs3Draft.FrozenWebEvidence]::Test(); "PASS $webCount independent frozen WEB evidence assertions." }
if ($Mode -eq 'compile-only') { $uiCount=[Vcp.Cs3Draft.UiArtifactEvidence]::Test(); "PASS $uiCount independent UI artifact evidence assertions." }
if ($Mode -eq 'compile-only') { $cleanupCount=[Vcp.Cs3Draft.NativeProbe]::TestCollectorCleanupContract(); "PASS $cleanupCount collector stop/identity/termination ordering assertions, including injected failures." }
if ($Mode -eq 'compile-only') { [Vcp.Cs3Draft.NativeProbe]::CheckLayouts(); $count=[Vcp.Cs3Draft.ProbeContract]::Test(); $domCount=[Vcp.Cs3Draft.DomEvidence]::Test(); $inputCount=[Vcp.Cs3Draft.InputDiagnosticEvidence]::Test(); $guardianCount=[Vcp.Cs3Draft.NativeProbe]::TestWorkerGuardianContract(); $scratchCount=[Vcp.Cs3Draft.NativeProbe]::TestScratchVanishedContract(); $scratchDiagnosticCount=[Vcp.Cs3Draft.NativeProbe]::TestScratchDiagnosticContract(); "Worker compiled; $count protocol/coverage, $domCount DOM evidence, $inputCount input diagnostic, $guardianCount guardian, $scratchCount scratch and $scratchDiagnosticCount scratch diagnostic assertions passed. Only a read-only absent-path check; no native probe method, host, Core, profile, ACL or registry operation invoked."; return }
if (-not $Execute) { throw 'Draft native execution requires independent review and explicit -Execute' }
$inputsFile=Join-Path $PSScriptRoot 'inputs.json'
if ($Mode -eq 'webview2-dom' -and ($ExpectedInputsSha256 -cnotmatch '^[a-f0-9]{64}$' -or (Get-FileHash -LiteralPath $inputsFile).Hash.ToLowerInvariant() -cne $ExpectedInputsSha256)) { throw 'Exact reviewed build-manifest hash required before a new native attempt' }
$inputs=Get-Content -LiteralPath $inputsFile -Raw | ConvertFrom-Json
if ($inputs.schema -cne 'cs3-webview2-inputs/1' -or $inputs.version -cne '154.0.4258.37' -or $inputs.browser_argument -cne '' -or $inputs.runtime -cne 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application\154.0.4258.37') { throw 'Exact prospective inputs differ' }
foreach ($entry in $inputs.sources) { if ($entry.path -notmatch '^[A-Za-z-]+\.(cs|ps1)$' -or (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $entry.path)).Hash.ToLowerInvariant() -cne $entry.sha256) { throw 'Frozen draft source changed; compile a new reviewed input manifest' } }
function Hash([string]$File) { (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant() }
function ReceiptHash([string]$File, [switch]$AllowMissing) {
    $stream=$null
    try {
        $stream=[IO.FileStream]::new($File,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::Read -bor [IO.FileShare]::Delete))
        if($stream.Length -gt 4MB){throw 'Receipt identity byte ceiling'}
        return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
    } catch {
        $failure=$_.Exception
        while($failure.InnerException){$failure=$failure.InnerException}
        if($AllowMissing -and $failure -is [IO.FileNotFoundException]){return $null}
        throw
    } finally {if($stream){$stream.Dispose()}}
}
function JsonWrite([string]$File, $Value) {
    $full=[IO.Path]::GetFullPath($File);$parent=[IO.Path]::GetDirectoryName($full)
    if([IO.Path]::GetDirectoryName($parent) -cne $PSScriptRoot -or [IO.Path]::GetFileName($parent) -cnotmatch '^run-[a-f0-9]{32}$' -or [IO.Path]::GetFileName($full) -cne 'native-receipt.json'){throw 'Receipt write is outside exact owned run'}
    [Vcp.Cs3Draft.NativeProbe]::RegularTree($parent)
    $previous=ReceiptHash $full -AllowMissing
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 12))
    if($bytes.Length -gt 4MB){throw 'Receipt serialization byte ceiling'}
    $expected=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    # A killed controller may leave its fixed temporary path open in a dying
    # descendant. Recovery must not adopt, overwrite or wait on that path.
    $temporary = $File + '.next.' + $PID + '.' + [guid]::NewGuid().ToString('N')
    $stream=[IO.FileStream]::new($temporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    # Dispose flushes the managed buffer before the atomic replacement. This
    # journal does not claim power-loss durability; synchronous disk flushes on
    # every event would block the bounded worker acknowledgement protocol.
    try {$stream.Write($bytes,0,$bytes.Length)} finally {$stream.Dispose()}
    Complete-OwnedReceiptReplacement -Validate {
        [Vcp.Cs3Draft.NativeProbe]::RegularTree($parent)
        if((ReceiptHash $temporary) -cne $expected -or (ReceiptHash $full -AllowMissing) -cne $previous){throw 'Receipt bytes changed during atomic replacement'}
    } -Move {[IO.File]::Move($temporary,$full,$true)}
}
function PrivateAcl([string]$Directory, [string]$ContainerSid, [string]$Rights = 'ReadAndExecute') {
    $acl = [Security.AccessControl.DirectorySecurity]::new(); $acl.SetAccessRuleProtection($true, $false)
    $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
    $owner = [Security.Principal.WindowsIdentity]::GetCurrent().User
    foreach ($sid in @($owner, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl',$inherit,'None','Allow')) }
    if ($ContainerSid) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($ContainerSid),$Rights,$inherit,'None','Allow')) }
    Set-Acl -LiteralPath $Directory -AclObject $acl
}
function ReadReceipt([string]$File) {
    $full = [IO.Path]::GetFullPath($File)
    $parent = [IO.Path]::GetDirectoryName($full)
    if ([IO.Path]::GetDirectoryName($parent) -cne $PSScriptRoot -or [IO.Path]::GetFileName($parent) -cnotmatch '^run-[a-f0-9]{32}$' -or [IO.Path]::GetFileName($full) -cne 'native-receipt.json') { throw 'Receipt must be an exact owned draft run path' }
    [Vcp.Cs3Draft.NativeProbe]::RegularTree($parent)
    $value = Get-Content -LiteralPath $full -Raw | ConvertFrom-Json
    if ($value.schema -cne 'cs3-native-probe-receipt/1' -or $value.name -cnotmatch '^iokaio\.vcp\.cs3\.[a-f0-9]{32}$' -or $value.sid -cne [Vcp.Cs3Draft.NativeProbe]::DeriveSid($value.name)) { throw 'Receipt identity differs' }
    $expected = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('Packages\' + $value.name + '\AC')
    if ($value.root -cne $expected) { throw 'Receipt root differs from exact owned profile' }
    return $value
}
function Test-ExactControllerAlive($Value) {
    try {
        $process = [Diagnostics.Process]::GetProcessById([int]$Value.controller_pid)
        try { return $process.StartTime.ToFileTimeUtc() -eq [long]$Value.controller_creation_filetime }
        finally { $process.Dispose() }
    } catch { return $false }
}
function Recover-AbandonedProfile([string]$File) {
    $value = ReadReceipt $File
    if (-not $value.profile_created -or $value.status -eq 'cleaned' -or $value.status -eq 'owner_loss_recovered') { return $false }
    if (Test-ExactControllerAlive $value) { throw 'Owned receipt still has its exact live controller' }
    if ($value.status -notin @('profile_created','staged','running','cleanup_pending','cleanup_failed')) { throw 'Owned receipt is not recoverable' }
    $parent = [IO.Path]::GetDirectoryName($value.root)
    if (Test-Path -LiteralPath $parent) { [Vcp.Cs3Draft.NativeProbe]::RegularTree($parent) }
    $deleted = -not (Test-Path -LiteralPath $value.root)
    $failure = $null
    for ($attempt = 1; -not $deleted -and $attempt -le 40; $attempt++) {
        try {
            [Vcp.Cs3Draft.NativeProbe]::DeleteProfile($value.name, $value.sid)
            $deleted = -not (Test-Path -LiteralPath $value.root)
        } catch { $failure = $_; Start-Sleep -Milliseconds 250 }
    }
    if (-not $deleted) { throw "Exact owner-loss profile did not drain for deletion: $failure" }
    $value.processes_drained = $true
    $value | Add-Member -NotePropertyName owner_loss_recovered -NotePropertyValue $true -Force
    $value | Add-Member -NotePropertyName recovery_basis -NotePropertyValue 'Exact controller identity absent; nested kill-on-close job handles closed; exact AppContainer profile deletion succeeded.' -Force
    $value.status = 'owner_loss_recovered'
    $value.outcome = 'owner_loss_recovered'
    JsonWrite $File $value
    return $true
}
function VerifyDistribution([string]$Root, $ExpectedEntries) {
    [Vcp.Cs3Draft.NativeProbe]::RegularTree($Root)
    $actualFiles = @(Get-ChildItem -LiteralPath $Root -File -Recurse)
    if ($actualFiles.Count -ne $ExpectedEntries.Count) { throw 'Distribution file coverage differs' }
    foreach ($entry in $ExpectedEntries) {
        if ($entry.path -match '(^/|\\|(^|/)\.\.?(/|$)|:)') { throw 'Invalid frozen distribution path' }
        $file = Join-Path $Root $entry.path
        if ((Get-Item -LiteralPath $file).Length -ne $entry.bytes -or (Hash $file) -cne $entry.sha256) { throw 'Distribution bytes differ' }
    }
}
if ($Mode -eq 'worker') {
    $value = ReadReceipt $Receipt
    if ($value.mode -cne 'webview2-dom' -or -not $value.profile_created -or $value.status -cnotin @('staged','running') -or $value.inputs_sha256 -cne (Hash $inputsFile)) { throw 'Worker receipt is not staged and source-bound' }
    VerifyDistribution (Join-Path $value.root 'host') $inputs.host
    Assert-NoWebViewOverrides
    $snapshot=Get-InputSnapshot $inputs.runtime -CheckRuntimeAcl -ContainerSid $value.sid
    Assert-SnapshotSame $value.runtime_before $snapshot
    $imagePaths=@(); $imageHashes=@()
    foreach ($entry in $snapshot.files) { if ($entry.path.EndsWith('.exe',[StringComparison]::OrdinalIgnoreCase)) { $imagePaths+=(Join-Path $inputs.runtime $entry.path); $imageHashes+=$entry.sha256 } }
    $imagePaths+=(Join-Path $value.root 'host/WebViewHost.exe'); $imageHashes+=@($inputs.host | Where-Object path -ceq 'WebViewHost.exe')[0].sha256
    try {
        $portText = [Console]::ReadLine()
        if ($portText -cnotmatch '^PORT [1-9][0-9]{0,18}$') { throw 'Missing transferred completion port' }
        $ownerProcess = [Diagnostics.Process]::GetProcessById([int]$value.controller_pid)
        $null = $ownerProcess.Handle
        if ($ownerProcess.StartTime.ToFileTimeUtc() -ne [long]$value.controller_creation_filetime) { throw 'Controller process identity changed' }
        try { [Vcp.Cs3Draft.NativeProbe]::Run($value.root,$value.name,$value.sid,$inputs.runtime,$value.host_nonce,[string[]]$imagePaths,[string[]]$imageHashes,[long]$portText.Substring(5),$ownerProcess.Handle) }
        finally { $ownerProcess.Dispose() }
        exit 0
    } catch { [Console]::Error.WriteLine($_.Exception.ToString()); exit 1 }
}
if ($Mode -eq 'reconcile') {
    if (-not (Recover-AbandonedProfile $Receipt)) { throw 'Receipt needs no owner-loss recovery' }
    return
}
if ($Receipt) { throw 'A new compatibility attempt cannot adopt an old receipt' }
if ($Mode -eq 'webview2-dom') {
    # A killed outer controller cannot write a final receipt. Before creating a
    # new profile, recover every exact source-bound receipt in this build. The
    # worker and browser jobs both use kill-on-close, so disappearance of the
    # exact controller closes the nested ownership chain. Successful exact
    # AppContainer deletion is the final fail-closed drainage boundary.
    foreach ($candidate in @(Get-ChildItem -LiteralPath $PSScriptRoot -Directory -Filter 'run-*' | ForEach-Object { Join-Path $_.FullName 'native-receipt.json' } | Where-Object { Test-Path -LiteralPath $_ })) {
        $pending = ReadReceipt $candidate
        if ($pending.status -notin @('cleaned','owner_loss_recovered')) { $null = Recover-AbandonedProfile $candidate }
    }
}
Assert-NoWebViewOverrides
$runtimeBefore=Get-InputSnapshot $inputs.runtime -CheckRuntimeAcl
if ((Hash (Join-Path $inputs.runtime 'msedgewebview2.exe')) -cne $inputs.runtime_executable_sha256) { throw 'Installed runtime executable differs' }
foreach ($entry in $inputs.host) { if ($entry.path -notin @('WebViewHost.exe','Microsoft.Web.WebView2.Core.dll','WebView2Loader.dll') -or (Hash (Join-Path $PSScriptRoot $entry.path)) -cne $entry.sha256) { throw 'Pinned host dependency differs' } }
$nonce = [Guid]::NewGuid().ToString('N')
$runDirectory = Join-Path $PSScriptRoot ('run-' + $nonce)
New-Item -ItemType Directory -Path $runDirectory -ErrorAction Stop | Out-Null
PrivateAcl $runDirectory ''
$name = 'iokaio.vcp.cs3.' + $nonce
$sid = [Vcp.Cs3Draft.NativeProbe]::DeriveSid($name)
$root = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('Packages\' + $name + '\AC')
if (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($root))) { throw 'Owned profile path already exists' }
$receiptPath = Join-Path $runDirectory 'native-receipt.json'
$controllerProcess = [Diagnostics.Process]::GetCurrentProcess()
$value = [ordered]@{ schema='cs3-native-probe-receipt/1';name=$name;sid=$sid;root=$root;mode=$Mode;status='creation_intent';profile_created=$false;worker_launch_attempted=$false;processes_drained=$false;controller_pid=$PID;controller_creation_filetime=$controllerProcess.StartTime.ToFileTimeUtc();inputs_sha256=(Hash $inputsFile);host_nonce=([guid]::NewGuid().ToString('N')+[guid]::NewGuid().ToString('N'));runtime_before=$runtimeBefore;provider_calls=0;outcome='not_run';pause_before_resume_milliseconds=$PauseBeforeResumeMilliseconds;cancel_after_resume=[bool]$CancelAfterResume;events=@();primary_controller_failure=$null;cleanup_errors=@();worker_stderr_truncated=$false;runtime_unchanged=$false;policy_unchanged=$false;host_unchanged=$false;serviced_input_limitation='Installed Evergreen is serviced in place. Before/after inventory/ACL/hash equality is not immutable-during-execution proof. No runtime locks or servicing changes.' }
$value.diagnostic_only = $true
$value.prototype_only = $true
$value.production_profile_qualified = $false
$value.browser_qualification = $false
$value.internal_sandbox_qualified = $false
$value.browser_family = 'webview2'
$controllerProcess.Dispose()
JsonWrite $receiptPath $value
$worker = $null; $workerLaunchAttempted=$false; $port = [IntPtr]::Zero; $owned = @{}; $jobEmpty = $false
$controllerFailure = $null; $stderr = $null; $line = $null; $stdoutEof=$false; $outputBytes=0; $workerStopped=$false; $workerExitCode=$null; $observedDrained=$false; $independentEmpty=$false; $drainHandshakeSeen=$false; $intentionalCancellation=$false
try {
    $createdSid = [Vcp.Cs3Draft.NativeProbe]::CreateProfile($name)
    $value.profile_created = $true; $value.status = 'profile_created'; JsonWrite $receiptPath $value
    if ($createdSid -cne $sid) { throw 'Created profile SID mismatch' }
    [Vcp.Cs3Draft.NativeProbe]::RegularTree($root)
    # AppContainer provisioning already creates AC\Temp. Our scratch directory
    # has a distinct name and must still be created exclusively by this attempt.
    foreach ($directory in @('host','profile','probe-temp')) { New-Item -ItemType Directory -Path (Join-Path $root $directory) -ErrorAction Stop | Out-Null }
    foreach ($entry in $inputs.host) {
        $destination = Join-Path (Join-Path $root 'host') $entry.path
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
        [IO.File]::Copy((Join-Path $PSScriptRoot $entry.path), $destination, $false)
    }
    VerifyDistribution (Join-Path $root 'host') $inputs.host
    PrivateAcl ([IO.Path]::GetDirectoryName($root)) $sid
    PrivateAcl $root $sid
    PrivateAcl (Join-Path $root 'host') $sid
    PrivateAcl (Join-Path $root 'profile') $sid 'Modify'
    PrivateAcl (Join-Path $root 'probe-temp') $sid 'Modify'
    $value.host_before=Get-InputSnapshot (Join-Path $root 'host')
    $value.status = 'staged'; JsonWrite $receiptPath $value
    $workerEnvironment = [ordered]@{}
    foreach ($key in @('SystemRoot','WINDIR','SystemDrive','TEMP','TMP','USERPROFILE','LOCALAPPDATA','APPDATA')) { $entry = [Environment]::GetEnvironmentVariable($key); if ($entry) { $workerEnvironment[$key]=$entry } }
    $workerLaunchAttempted=$true; $value.worker_launch_attempted=$true; JsonWrite $receiptPath $value
    $worker = [Vcp.Cs3Draft.NativeProbe]::StartWorker((Join-Path $PSHOME 'pwsh.exe'),$PSCommandPath,$receiptPath,[string[]]$workerEnvironment.Keys,[string[]]$workerEnvironment.Values)
    $stderr = [Vcp.Cs3Draft.NativeProbe]::ReadDiagnosticPrefix($worker.StandardError.BaseStream)
    $line = [Vcp.Cs3Draft.NativeProbe]::ReadBounded($worker.StandardOutput.BaseStream,16384,$true)
    $port = [Vcp.Cs3Draft.NativeProbe]::CompletionPort()
    $remotePort = [Vcp.Cs3Draft.NativeProbe]::DuplicatePort($port,$worker.Handle)
    $worker.StandardInput.WriteLine('PORT ' + $remotePort); $worker.StandardInput.Flush()
    $value.status='running'; JsonWrite $receiptPath $value
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while (-not $worker.HasExited -or $line.IsCompleted) {
        if ($clock.ElapsedMilliseconds -gt 100000) { throw 'Controller wall deadline' }
        if (-not $line.Wait(50)) { continue }
        $text = $line.GetAwaiter().GetResult(); $line=$null
        if ($null -eq $text) { $stdoutEof=$true; break }
        $outputBytes += [Text.Encoding]::UTF8.GetByteCount($text)
        # Includes at most 64 KiB browser stderr encoded in bounded 8 KiB chunks.
        if ($text.Length -gt 16384 -or $outputBytes -gt 262144) { throw 'Worker event output ceiling' }
        $event = $text | ConvertFrom-Json
        $value.events += $event
        if ($event.type -in @('created_suspended','owned_process')) {
            $null = Register-OwnedProcess $owned $event
            # Persist ownership before resume and retain the owner-loss cut
            # point. Ordinary host/chunk events stay in the bounded in-memory
            # stream until cleanup; rewriting the runtime inventory for each
            # chunk can backpressure the worker's fixed startup/drain clocks.
            JsonWrite $receiptPath $value
        }
        if ($event.type -eq 'created_suspended') {
            if ($PauseBeforeResumeMilliseconds -gt 0) {
                Start-Sleep -Milliseconds $PauseBeforeResumeMilliseconds
                if ($worker.HasExited) { throw 'Worker exited during bounded pre-resume pause' }
                $value.events += [ordered]@{type='controller_pause_observed';milliseconds=$PauseBeforeResumeMilliseconds;created_process_still_suspended=$true}
                JsonWrite $receiptPath $value
            }
            $worker.StandardInput.WriteLine('RESUME'); $worker.StandardInput.Flush()
            if ($CancelAfterResume) { $intentionalCancellation=$true; $worker.Kill() }
        }
        if ($event.type -eq 'job_empty_waiting_for_ack') {
            if ($drainHandshakeSeen) { throw 'Duplicate job drain handshake' }
            $drainHandshakeSeen=$true
            # Worker has terminated its job and queried zero, but retains the
            # sole job handle while we observe the independent notification.
            $independentEmpty=[Vcp.Cs3Draft.NativeProbe]::WaitForEmpty($port)
            if (-not $independentEmpty) { throw 'No independent job-zero notification before acknowledgement' }
            $value.events += [ordered]@{type='controller_job_zero_observed';sole_job_handle_still_owned_by_worker=$true}
            # The independent zero observation authorizes this acknowledgement,
            # not a disk write. Cleanup intent and the final receipt persist the
            # complete bounded event stream after the live handshake finishes.
            $worker.StandardInput.WriteLine('DRAINED'); $worker.StandardInput.Flush()
        }
        if ($event.type -eq 'job_empty') { $jobEmpty=$true }
        $line = [Vcp.Cs3Draft.NativeProbe]::ReadBounded($worker.StandardOutput.BaseStream,16384,$true)
    }
    if (-not $worker.WaitForExit(10000)) { throw 'Worker exit deadline' }
} catch {
    $controllerFailure = [Runtime.ExceptionServices.ExceptionDispatchInfo]::Capture($_.Exception)
    $detail = $_ | Out-String
    $value.primary_controller_failure = $detail.Substring(0,[Math]::Min(8192,$detail.Length))
    $value.outcome = 'controller_failure'
} finally {
    # Each cleanup boundary runs even after a previous cleanup error. The first
    # controller exception remains authoritative and is rethrown after receipt.
    try {
        if ($worker) { if (-not $worker.HasExited) { $worker.Kill() }; $workerStopped=$worker.WaitForExit(10000); if (-not $workerStopped) { throw 'Worker termination deadline' }; $workerExitCode=$worker.ExitCode }
    } catch { Record-ControllerCleanupFailure $value 'worker_termination' $_ }
    try {
        $wait = [Diagnostics.Stopwatch]::StartNew()
        foreach ($identity in $owned.Values) { if ($null -eq $identity.Process) { continue }; $remaining=[Math]::Max(0,10000-$wait.ElapsedMilliseconds); if (-not $identity.Process.WaitForExit([int]$remaining)) { throw 'Observed process survived worker termination' } }
        $observedDrained=$true
    } catch { Record-ControllerCleanupFailure $value 'owned_process_drain' $_ }
    # Retain bounded native diagnostics/events even when processing an earlier
    # event caused the controller failure. No control command runs while draining.
    if ($worker -and $workerStopped -and -not $stdoutEof) {
        try {
            $drain = [Diagnostics.Stopwatch]::StartNew()
            while (-not $stdoutEof -and $drain.ElapsedMilliseconds -lt 5000) {
                if ($null -eq $line) { $line=[Vcp.Cs3Draft.NativeProbe]::ReadBounded($worker.StandardOutput.BaseStream,16384,$true) }
                if (-not $line.Wait(50)) { continue }
                $text=$line.GetAwaiter().GetResult(); $line=$null
                if ($null -eq $text) { $stdoutEof=$true; break }
                $outputBytes += [Text.Encoding]::UTF8.GetByteCount($text)
                if ($outputBytes -gt 262144) { throw 'Worker event drainage ceiling' }
                $event=$text | ConvertFrom-Json; $value.events += $event
                if ($event.type -eq 'job_empty') { $jobEmpty=$true }
            }
            if (-not $stdoutEof) { throw 'Worker event drainage deadline' }
        } catch { Record-ControllerCleanupFailure $value 'worker_event_drain' $_ }
    }
    if ($stderr) {
        try {
            if (-not $stderr.Wait(5000)) { throw 'Worker diagnostic drainage deadline' }
            $diagnostics=$stderr.GetAwaiter().GetResult()
            [IO.File]::WriteAllBytes((Join-Path $runDirectory 'worker-errors.txt'),$diagnostics.Bytes)
            $value.worker_stderr_truncated=$diagnostics.Truncated
            if ($diagnostics.Truncated) { throw 'Worker diagnostic ceiling; bounded prefix retained' }
        } catch { Record-ControllerCleanupFailure $value 'worker_stderr_drain' $_ }
    }
    # Never close the independent completion port before the final bounded
    # ACTIVE_PROCESS_ZERO observation, including controller-exception paths.
    if ($port -ne [IntPtr]::Zero) {
        try { if (-not $workerStopped) { throw 'Cannot establish job drainage before worker termination' }; if (-not $independentEmpty) { $independentEmpty=[Vcp.Cs3Draft.NativeProbe]::WaitForEmpty($port) }; $value.processes_drained=$independentEmpty -and $observedDrained; if (-not $value.processes_drained) { throw 'Independent job-empty notification and held-process drainage both required' } }
        catch { Record-ControllerCleanupFailure $value 'independent_job_drain' $_ }
        try { [Vcp.Cs3Draft.NativeProbe]::ClosePort($port) } catch { Record-ControllerCleanupFailure $value 'close_completion_port' $_ }
    }
    foreach ($identity in $owned.Values) { if ($null -eq $identity.Process) { continue }; try { $identity.Process.Dispose() } catch { Record-ControllerCleanupFailure $value 'dispose_owned_process' $_ } }
    if ($worker) { try { $worker.Dispose() } catch { Record-ControllerCleanupFailure $value 'dispose_worker' $_ } }
    try { Assert-NoWebViewOverrides; $value.policy_unchanged=$true } catch { Record-ControllerCleanupFailure $value 'policy_postcheck' $_ }
    try { $after=Get-InputSnapshot $inputs.runtime -CheckRuntimeAcl -ContainerSid $sid; Assert-SnapshotSame $value.runtime_before $after; $value.runtime_unchanged=$true } catch { Record-ControllerCleanupFailure $value 'runtime_postcheck' $_ }
    if ($value.Contains('host_before')) { try { Assert-SnapshotSame $value.host_before (Get-InputSnapshot (Join-Path $root 'host')); $value.host_unchanged=$true } catch { Record-ControllerCleanupFailure $value 'host_postcheck' $_ } }
    if (-not $controllerFailure) {
        $clean = $workerExitCode -eq 0 -and $jobEmpty -and $value.processes_drained -and $value.runtime_unchanged -and $value.host_unchanged -and $value.policy_unchanged -and -not $value.cleanup_errors.Count
        $cancelledClean = $intentionalCancellation -and $value.processes_drained -and $value.runtime_unchanged -and $value.host_unchanged -and $value.policy_unchanged -and -not $value.cleanup_errors.Count
        $inputObserved = @($value.events | Where-Object type -ceq 'input_diagnostic_observed').Count
        $domObserved = @($value.events | Where-Object type -ceq 'dom_observed').Count
        if ($cancelledClean -and $inputObserved -eq 0 -and $domObserved -eq 0) { $value.outcome='cancelled_clean' }
        elseif ($clean -and $inputObserved -eq 1 -and $domObserved -eq 0) { $value.outcome='input_diagnostic_observed' }
        elseif ($clean -and $domObserved -eq 1 -and $inputObserved -eq 0) { $value.outcome='dom_observed' }
        else { $value.outcome='input_diagnostic_inconclusive_or_failure' }
    }
    if ($value.profile_created) {
        $value.status = 'cleanup_pending'; $pendingSaved=$false
        try { JsonWrite $receiptPath $value; $pendingSaved=$true } catch { Record-ControllerCleanupFailure $value 'save_cleanup_intent' $_ }
        if ($pendingSaved -and ($value.processes_drained -or -not $workerLaunchAttempted)) {
            try {
                [Vcp.Cs3Draft.NativeProbe]::RegularTree([IO.Path]::GetDirectoryName($root))
                [Vcp.Cs3Draft.NativeProbe]::DeleteProfile($name,$sid)
                if (Test-Path -LiteralPath $root) { throw 'Exact profile files remain' }
                $value.status='cleaned'
            } catch { $value.status='cleanup_failed'; Record-ControllerCleanupFailure $value 'delete_profile' $_ }
        }
    }
    if ($value.cleanup_errors.Count) {
        if ($value.outcome -ceq 'input_diagnostic_observed') { $value.outcome='input_diagnostic_inconclusive_or_failure' }
        elseif ($value.outcome -ceq 'dom_observed') { $value.outcome='dom_inconclusive_or_failure' }
    }
    try { JsonWrite $receiptPath $value } catch { Record-ControllerCleanupFailure $value 'save_final_receipt' $_; [Console]::Error.WriteLine('Final receipt write failed; primary controller error remains authoritative.') }
}
if ($controllerFailure) { $controllerFailure.Throw() }
if ($value.cleanup_errors.Count) { throw 'Controller cleanup had failures; retained receipt includes details' }
$value | ConvertTo-Json -Depth 12
