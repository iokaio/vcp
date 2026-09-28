# SPDX-License-Identifier: Apache-2.0
# Byte-only validation. Does not load the image or invoke native APIs.
function Assert-PeExecutable([byte[]]$Bytes, [ValidateSet(2,3)][int]$Subsystem = 2) {
    if ($null -eq $Bytes -or $Bytes.Length -lt 64 -or $Bytes.Length -gt 16777216) { throw 'PE byte bound' }
    if ([BitConverter]::ToUInt16($Bytes,0) -ne 0x5a4d) { throw 'Missing DOS signature' }
    [long]$peOffset=[BitConverter]::ToUInt32($Bytes,0x3c)
    if ($peOffset -lt 64 -or $peOffset -gt $Bytes.Length-24) { throw 'PE header offset out of bounds' }
    if ([BitConverter]::ToUInt32($Bytes,[int]$peOffset) -ne 0x4550) { throw 'Missing PE signature' }
    if ([BitConverter]::ToUInt16($Bytes,[int]$peOffset+4) -ne 0x8664) { throw 'Expected x64 machine' }
    $characteristics=[BitConverter]::ToUInt16($Bytes,[int]$peOffset+22)
    if (($characteristics -band 2) -eq 0 -or ($characteristics -band 0x2000) -ne 0) { throw 'Expected executable, not DLL' }
    [long]$optionalOffset=$peOffset+24
    $optionalSize=[BitConverter]::ToUInt16($Bytes,[int]$peOffset+20)
    if ($optionalSize -lt 112 -or $optionalOffset+$optionalSize -gt $Bytes.Length) { throw 'Optional header out of bounds' }
    if ([BitConverter]::ToUInt16($Bytes,[int]$optionalOffset) -ne 0x20b) { throw 'Expected PE32+ optional header' }
    if ([BitConverter]::ToUInt16($Bytes,[int]$optionalOffset+68) -ne $Subsystem) { throw 'Unexpected PE subsystem' }
}
