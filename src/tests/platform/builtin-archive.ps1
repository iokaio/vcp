# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
Import-Module (Join-Path $repository 'scripts/skills/archive.psm1') -Force
$root = Join-Path $repository ('artifacts/p7-builtin-archive-tests/' + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $root -Force | Out-Null
$bytes = [Text.Encoding]::UTF8.GetBytes('selected fixture bytes')
$sha = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
$inventory = @{ files = @(@{ path = 'skills/builtin/rust/SKILL.md'; bytes = $bytes.Length; sha256 = $sha }) }
$results = @()
foreach ($mode in @('valid', 'traversal', 'duplicate', 'missing', 'extra', 'changed', 'symlink')) {
    $file = Join-Path $root "$mode.zip"
    $zip = [IO.Compression.ZipFile]::Open($file, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $names = switch ($mode) {
            'traversal' { @('../escape') }
            'duplicate' { @('skills/builtin/rust/SKILL.md', 'skills/builtin/rust/SKILL.md') }
            'missing' { @() }
            'extra' { @('skills/builtin/rust/SKILL.md', 'secret.txt') }
            default { @('skills/builtin/rust/SKILL.md') }
        }
        foreach ($name in $names) {
            $entry = $zip.CreateEntry($name)
            if ($mode -eq 'symlink') { $entry.ExternalAttributes = -1610612736 }
            $stream = $entry.Open()
            try {
                $content = if ($mode -eq 'changed') { [Text.Encoding]::UTF8.GetBytes('changed fixture bytes!') } else { $bytes }
                $stream.Write($content, 0, $content.Length)
            } finally { $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    $accepted = $false
    $reason = $null
    try { Test-VcpSkillArchive -ArchivePath $file -Inventory $inventory | Out-Null; $accepted = $true }
    catch { $reason = $_.Exception.Message }
    $pass = $accepted -eq ($mode -eq 'valid')
    $results += @{ case = $mode; pass = $pass; accepted = $accepted; reason = $reason }
}
$record = @{ schema = 'p7-builtin-archive-tests/1'; cases = $results; pass = @($results | Where-Object { -not $_.pass }).Count -eq 0 }
$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8
Write-Output (Join-Path $root 'result.json')
if (-not $record.pass) { exit 1 }
