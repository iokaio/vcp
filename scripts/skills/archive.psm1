# SPDX-License-Identifier: Apache-2.0
Set-StrictMode -Version Latest
function Test-VcpSkillArchive {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ArchivePath, [Parameter(Mandatory)]$Inventory)
    $expected = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    $folded = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    if ($Inventory.files.Count -eq 0 -or $Inventory.files.Count -gt 256) { throw 'Archive inventory count bound' }
    foreach ($file in $Inventory.files) {
        $relative = [string]$file.path
        if (-not $relative -or $relative.Length -gt 512 -or $relative -match '[\\:\x00-\x1f]' -or
            @($relative.Split('/') | Where-Object { -not $_ -or $_ -in '.', '..' -or $_ -match '[. ]$|[<>|?*]|^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)' }).Count -gt 0 -or
            -not $folded.Add($relative) -or $file.sha256 -cnotmatch '^[a-f0-9]{64}$' -or
            $file.bytes -lt 0 -or $file.bytes -gt 1073741824) { throw 'Unsafe or duplicate archive inventory entry' }
        $expected.Add($relative, $file)
    }
    $zip = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($ArchivePath))
    try {
        if ($zip.Entries.Count -ne $expected.Count) { throw 'Archive entry count differs from inventory' }
        $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $zip.Entries) {
            if (-not $expected.ContainsKey($entry.FullName) -or -not $seen.Add($entry.FullName)) { throw 'Unexpected or duplicate archive entry' }
            # Reject Unix symlink metadata even when the name and content happen to match.
            if ((($entry.ExternalAttributes -shr 16) -band 61440) -eq 40960) { throw 'Linked archive entry' }
            $file = $expected[$entry.FullName]
            if ($entry.Length -ne $file.bytes) { throw 'Archive length differs from inventory' }
            $stream = $entry.Open()
            $hash = [Security.Cryptography.SHA256]::Create()
            try {
                $buffer = [byte[]]::new(65536)
                [long]$length = 0
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $length += $read
                    if ($length -gt $file.bytes) { throw 'Archive expansion exceeds inventory bound' }
                    [void]$hash.TransformBlock($buffer, 0, $read, $null, 0)
                }
                [void]$hash.TransformFinalBlock([byte[]]::new(0), 0, 0)
                $actual = [Convert]::ToHexString($hash.Hash).ToLowerInvariant()
                if ($length -ne $file.bytes -or $actual -cne $file.sha256) { throw 'Archive content differs from inventory' }
            } finally { $hash.Dispose(); $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    [pscustomobject]@{ status = 'passed'; entries = $expected.Count }
}
Export-ModuleMember -Function Test-VcpSkillArchive
