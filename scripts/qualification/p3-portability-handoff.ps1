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
if (-not $IsWindows -or $PSVersionTable.PSVersion -lt [version]'7.4') { throw 'This qualification collector requires Windows and PowerShell 7.4 or later.' }
function Hash-Text([string]$Text) {
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text))).ToLowerInvariant()
}
# Pin ancestors before opening children; only the documented Cloud Files tag
# family is acceptable in the vault. Private evidence ancestors remain strict.
if (-not ('VcpHandoff.Native' -as [type])) {
Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace VcpHandoff {
 public static class Native {
  [StructLayout(LayoutKind.Sequential)] struct TagInfo { public uint Attributes; public uint Tag; }
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern SafeFileHandle CreateFileW(string path,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
  [DllImport("kernel32.dll", SetLastError=true)]
  static extern bool GetFileInformationByHandleEx(SafeFileHandle handle,int info,out TagInfo value,uint size);
  [DllImport("ntdll.dll")] static extern sbyte RtlSetThreadPlaceholderCompatibilityMode(sbyte mode);
  static SafeFileHandle Open(string path,bool directory,bool cloud) {
   var handle=CreateFileW(path,directory ? 0x80u : 0x80000000u,directory ? 3u : 1u,IntPtr.Zero,3,0x02200000,IntPtr.Zero);
   if(handle.IsInvalid) { handle.Dispose(); throw new IOException("Native path open refused"); }
   try {
    TagInfo info;
    var prior=RtlSetThreadPlaceholderCompatibilityMode(2);
    if(prior<0) throw new IOException("Native placeholder mode refused");
    try { if(!GetFileInformationByHandleEx(handle,9,out info,8)) throw new IOException("Native tag query refused"); }
    finally { RtlSetThreadPlaceholderCompatibilityMode(prior); }
    if(((info.Attributes & 0x10)!=0)!=directory) throw new IOException("Unexpected native object kind");
    if((info.Attributes & 0x400)!=0 && (!cloud || (info.Tag & ~0xF000u)!=0x9000001Au))
     throw new IOException("Native reparse tag refused");
    return handle;
   } catch { handle.Dispose(); throw; }
  }
  public static List<SafeFileHandle> Pin(string path,bool cloud) {
   var chain=new Stack<string>();
   var item=new DirectoryInfo(Path.GetFullPath(path));
   while(item!=null) { chain.Push(item.FullName); item=item.Parent; }
   var handles=new List<SafeFileHandle>();
   try { while(chain.Count>0) handles.Add(Open(chain.Pop(),true,cloud)); return handles; }
   catch { foreach(var handle in handles) handle.Dispose(); throw; }
  }
  public static FileStream Read(string path) {
   var handle=Open(path,false,true);
   try { return new FileStream(handle,FileAccess.Read); }
   catch { handle.Dispose(); throw; }
  }
 }
}
"@
}
$pins = [Collections.Generic.List[IDisposable]]::new()
function Pinned-Directory([string]$Path,[bool]$Cloud) {
    $full = [IO.Path]::GetFullPath($Path)
    foreach ($handle in [VcpHandoff.Native]::Pin($full,$Cloud)) { $pins.Add($handle) }
    return $full.TrimEnd('\','/')
}
$stream = $null
try {
$vaultPath = Pinned-Directory $Vault $true
$evidencePath = [IO.Path]::GetFullPath($Evidence)
$evidenceParent = Pinned-Directory ([IO.Path]::GetDirectoryName($evidencePath)) $false
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
        # Read and hash the exact checked handle; sharing denies writes/deletion.
        $stream = [VcpHandoff.Native]::Read($objectPath)
        if ($stream.Length -gt 68157440 -or $stream.Length -lt 1) { throw 'object-byte-limit' }
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
} finally {
    if ($stream) { $stream.Dispose() }
    foreach ($handle in $pins) { $handle.Dispose() }
}
if ($observation -ne 'readable-ciphertext') { exit 2 }
exit 0
