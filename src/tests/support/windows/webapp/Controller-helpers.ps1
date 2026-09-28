# SPDX-License-Identifier: Apache-2.0
# Pure controller bookkeeping; loading this file starts no processes or profiles.
function Register-OwnedProcess {
    param([hashtable]$Identities, $Event, [scriptblock]$OpenIdentity = {
        param([int]$ProcessId)
        $process = [Diagnostics.Process]::GetProcessById($ProcessId)
        try { $null=$process.Handle; return @{ Process=$process; CreationFileTime=$process.StartTime.ToFileTimeUtc() } }
        catch { $process.Dispose(); throw }
    })
    $key = [int]$Event.pid
    $created = [long]$Event.creation_filetime
    if ($Identities.ContainsKey($key)) {
        # This is the exact Process object/handle acquired for the first event.
        # Do not reopen a numeric PID after the process has exited or been reused.
        if ($Identities[$key].CreationFileTime -ne $created) { throw 'Repeated process event changed creation identity' }
        return $Identities[$key].Process
    }
    try { $identity = & $OpenIdentity $key }
    catch [ArgumentException] {
        # The pinned native worker already held and verified this exact process
        # identity before publishing the event. Preserve a tombstone only when
        # the process exited before the controller could acquire a second
        # handle; other open failures and unverified events remain fatal.
        if ($Event.token_verified -isnot [bool] -or -not $Event.token_verified -or $created -le 0) { throw }
        $identity=@{ Process=$null; CreationFileTime=$created; ExitedBeforeParentOpen=$true }
    }
    if ($identity.CreationFileTime -ne $created) { $identity.Process.Dispose(); throw 'PID creation identity changed' }
    $Identities.Add($key,$identity)
    return $identity.Process
}
function Record-ControllerCleanupFailure {
    param($ReceiptValue,[string]$Operation,$Failure)
    $detail = [string]$Failure
    if ($detail.Length -gt 2048) { $detail=$detail.Substring(0,2048) }
    $ReceiptValue.cleanup_errors += [ordered]@{ operation=$Operation; detail=$detail }
}
function Complete-OwnedReceiptReplacement {
    param([Parameter(Mandatory)][scriptblock]$Validate, [Parameter(Mandatory)][scriptblock]$Move,
        [scriptblock]$Delay = { Start-Sleep -Milliseconds 25 })
    # Only atomic replacement sharing/access conflicts may wait. Validation runs
    # before every attempt, so changed source/destination bytes never get adopted.
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        & $Validate
        try { & $Move; return }
        catch {
            $failure = $_.Exception
            while ($failure.InnerException) { $failure = $failure.InnerException }
            $native = if ($failure -is [ComponentModel.Win32Exception]) { $failure.NativeErrorCode } else { $failure.HResult -band 0xffff }
            if ($native -notin @(5,32) -or $attempt -eq 9) { throw }
            & $Delay
        }
    }
}
