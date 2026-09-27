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
    $identity = & $OpenIdentity $key
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
