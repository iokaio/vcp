# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Collect bounded, redacted U04 handoff evidence without publishing or restoring.
.DESCRIPTION
Reads one explicitly selected ciphertext in the operator's real OneDrive vault.
Writes only a new local evidence file outside that vault. This is an observation,
not proof of authenticated restore, provider delivery, or the full U04 campaign.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9_-]{1,64}$')][string]$Campaign,
    [Parameter(Mandatory)][ValidateSet('A','B')][string]$Machine,
    [Parameter(Mandatory)][ValidateSet('a-published','b-hydrated','b-published','a-hydrated')][string]$Phase,
    [Parameter(Mandatory)][ValidateSet('files','sqlite')][string]$Backend,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$Commit,
    [Parameter(Mandatory)][string]$Vault,
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9_-]{1,128}\.age$')][string]$Object,
    [Parameter(Mandatory)][string]$Evidence,
    [Parameter(Mandatory)][ValidateRange(1,[long]::MaxValue)][long]$Sequence,
    [Parameter(Mandatory)][ValidateRange(0,[long]::MaxValue)][long]$Deletion,
    [ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedSha256,
    [ValidateRange(1,68157440)][long]$ExpectedBytes,
    [ValidateRange(0,300)][int]$WaitSeconds = 0,
    [switch]$OperatorConfirmsActualOneDrive
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This qualification collector requires Windows and PowerShell 7.' }
function Hash-Text([string]$Text) {
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text))).ToLowerInvariant()
}
function Ordinary-Directory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer) { throw 'Select an existing directory.' }
    $ancestor = $item
    while ($null -ne $ancestor) {
        if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'Redirected directory ancestors are not qualified.'
        }
        $ancestor = $ancestor.Parent
    }
    # Resolve only after every declared ancestor passed. This is a read-only
    # collector, not an atomic production publication or filesystem capability.
    return [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $item.FullName).ProviderPath).TrimEnd('\','/')
}
$vaultPath = Ordinary-Directory $Vault
$evidencePath = [IO.Path]::GetFullPath($Evidence)
$evidenceParent = Ordinary-Directory ([IO.Path]::GetDirectoryName($evidencePath))
$evidenceName = [IO.Path]::GetFileName($evidencePath)
if ($evidenceName.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0) { throw 'Invalid evidence filename.' }
$evidencePath = Join-Path $evidenceParent $evidenceName
if ($evidencePath.Equals($vaultPath,[StringComparison]::OrdinalIgnoreCase) -or
    $evidencePath.StartsWith($vaultPath+'\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'Evidence must remain outside the vault.'
}
if (Test-Path -LiteralPath $evidencePath) { throw 'Evidence destination already exists; choose a new file.' }
$objectPath = Join-Path $vaultPath $Object
$deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
$observation = 'pending'
$hash = $null
$length = $null
$attempts = 0
$failure = $null
$stream = $null
while ($true) {
    $attempts++
    try {
        $item = Get-Item -LiteralPath $objectPath -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'object-boundary'
        }
        if ($item.Length -gt 68157440 -or $item.Length -lt 1) { throw 'object-byte-limit' }
        # ShareRead refuses a concurrently writing publisher and pins this exact
        # ordinary file during hashing. Opening a cloud placeholder may hydrate it.
        $stream = [IO.File]::Open($objectPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        if ($stream.Length -gt 68157440) { throw 'object-byte-limit' }
        $length = $stream.Length
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = [Convert]::ToHexString($sha.ComputeHash($stream)).ToLowerInvariant() }
        finally { $sha.Dispose() }
        $stream.Dispose(); $stream = $null
        $observation = 'readable-ciphertext'
        if ($ExpectedSha256 -and $hash -ne $ExpectedSha256.ToLowerInvariant()) { $observation = 'checksum-mismatch' }
        if ($ExpectedBytes -and $length -ne $ExpectedBytes) { $observation = 'length-mismatch' }
        $failure = $null
        break
    } catch {
        # Exception text can carry local account/path names. Keep only a category.
        $failure = 'unavailable-incomplete-or-unqualified-object'
        if ($stream) { $stream.Dispose(); $stream = $null }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds 250
    }
}
$machineGuid = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Cryptography' -Name MachineGuid).MachineGuid
$machineIdentity = Hash-Text ($Campaign+':'+$machineGuid)
$machineGuid = $null
$oneDriveVersion = $null
try {
    $process = Get-Process -Name OneDrive -ErrorAction Stop | Select-Object -First 1
    $oneDriveVersion = $process.MainModule.FileVersionInfo.FileVersion
} catch { }
$filesystem = 'unobserved'
try {
    $drive = [IO.Path]::GetPathRoot($vaultPath).TrimEnd('\').TrimEnd(':')
    $filesystem = (Get-Volume -DriveLetter $drive -ErrorAction Stop).FileSystemType.ToString()
} catch { }
$result = [ordered]@{
    schema_version=1; campaign=$Campaign; machine=$Machine; machine_identity=$machineIdentity
    phase=$Phase; recorded_at_utc=[DateTime]::UtcNow.ToString('o'); commit=$Commit.ToLowerInvariant()
    windows_version=[Environment]::OSVersion.Version.ToString(); powershell=$PSVersionTable.PSVersion.ToString()
    filesystem=$filesystem; backend=$Backend; onedrive_version=$oneDriveVersion
    actual_onedrive_operator_assertion=[bool]$OperatorConfirmsActualOneDrive
    sequence=$Sequence; deletion_epoch=$Deletion; object=$Object; bytes=$length; ciphertext_sha256=$hash
    expected_sha256=$ExpectedSha256; expected_bytes=$ExpectedBytes; observation=$observation; failure=$failure
    attempts=$attempts; restore_verified=$false; full_u04_qualified=$false
}
$encoded = [Text.UTF8Encoding]::new($false).GetBytes(($result | ConvertTo-Json -Depth 4))
$out = [IO.File]::Open($evidencePath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
try { $out.Write($encoded,0,$encoded.Length); $out.Flush($true) } finally { $out.Dispose() }
# The output contains only allowlisted redacted evidence; no recovery material,
# machine name, workspace contents, absolute directory, or decrypted manifest.
$result | ConvertTo-Json -Depth 4
if ($observation -ne 'readable-ciphertext') { exit 2 }
exit 0
