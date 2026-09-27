# SPDX-License-Identifier: Apache-2.0
# Pure bookkeeping only: no registry/ACL access, processes, Core or browser.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Controller-helpers.ps1')
. (Join-Path $PSScriptRoot 'Input-Policy.ps1')
. (Join-Path $PSScriptRoot 'Pe-Contract.ps1')
$script:checks=0
function Check([bool]$Value) { if (-not $Value) { throw 'Pure PowerShell assertion failed' }; $script:checks++ }
function Reject([scriptblock]$Action) { $failed=$false; try { & $Action } catch { $failed=$true }; Check $failed }
Assert-NoPolicyValueNames @('another-app.exe'); Check $true
foreach ($name in @('iokaio.vcp.cs3.webview2.probe','webviewhost.EXE','*')) { Reject { Assert-NoPolicyValueNames @($name) } }
Assert-NoWritableRuntimeRule 'S-1-15-2-1' ([int][Security.AccessControl.FileSystemRights]::ReadAndExecute) 'Allow'; Check $true
Assert-NoWritableRuntimeRule 'S-1-5-18' ([int][Security.AccessControl.FileSystemRights]::FullControl) 'Allow'; Check $true
foreach ($principal in @('S-1-1-0','S-1-5-11','S-1-5-32-545','S-1-15-2-1','S-1-15-2-2','S-1-15-2-999')) { Reject { Assert-NoWritableRuntimeRule $principal ([int][Security.AccessControl.FileSystemRights]::Write) 'Allow' 'S-1-15-2-999' } }
$held=[pscustomobject]@{tag='same held object'}; $identities=@{42=@{Process=$held;CreationFileTime=100}}
$actual=Register-OwnedProcess $identities ([pscustomobject]@{pid=42;creation_filetime=100}) { throw 'Must not reopen an existing held PID' }; Check ([object]::ReferenceEquals($actual,$held))
Reject { Register-OwnedProcess $identities ([pscustomobject]@{pid=42;creation_filetime=101}) { throw 'Must not reopen reused PID' } }
Reject { Register-OwnedProcess @{} ([pscustomobject]@{pid=43;creation_filetime=100}) { throw 'Uninspectable process must fail' } }
Assert-SnapshotSame ([ordered]@{root='synthetic';files=@(@{sha256='a'})}) ([ordered]@{root='synthetic';files=@(@{sha256='a'})}); Check $true
Reject { Assert-SnapshotSame (@{sha256='a'}) (@{sha256='b'}) }
$pe=[byte[]]::new(256)
[BitConverter]::GetBytes([uint16]0x5a4d).CopyTo($pe,0)
[BitConverter]::GetBytes([uint32]64).CopyTo($pe,0x3c)
[BitConverter]::GetBytes([uint32]0x4550).CopyTo($pe,64)
[BitConverter]::GetBytes([uint16]0x8664).CopyTo($pe,68)
[BitConverter]::GetBytes([uint16]112).CopyTo($pe,84)
[BitConverter]::GetBytes([uint16]2).CopyTo($pe,86)
[BitConverter]::GetBytes([uint16]0x20b).CopyTo($pe,88)
[BitConverter]::GetBytes([uint16]2).CopyTo($pe,156)
Assert-PeExecutable $pe; Check $true
foreach($mutation in @(@{offset=0;value=0},@{offset=64;value=0},@{offset=68;value=0x14c},@{offset=84;value=70},@{offset=84;value=512},@{offset=86;value=0},@{offset=86;value=0x2002},@{offset=88;value=0x10b},@{offset=156;value=3})) {
    $changed=[byte[]]$pe.Clone(); [BitConverter]::GetBytes([uint16]$mutation.value).CopyTo($changed,$mutation.offset)
    Reject { Assert-PeExecutable $changed }
}
foreach($offset in @([uint32]0,[uint32]250,[uint32]::MaxValue)) {
    $changed=[byte[]]$pe.Clone(); [BitConverter]::GetBytes($offset).CopyTo($changed,0x3c)
    Reject { Assert-PeExecutable $changed }
}
Reject { Assert-PeExecutable ([byte[]]::new(63)) }
Reject { Assert-PeExecutable ([byte[]]$pe[0..100]) }
$console=[byte[]]$pe.Clone(); [BitConverter]::GetBytes([uint16]3).CopyTo($console,156)
Assert-PeExecutable $console 3; Check $true
'PASS '+$script:checks+' pure policy/ACL-rule/snapshot/held-identity/PE assertions; no OS policy or process operations.'
