#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline functional/security tests: execute the shipped helper against real ZIPs.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$helper = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../skills/builtin/toolchain-installation/scripts/install-verified-archive.ps1'))
$temporary = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $temporary ('vcp-toolchain-tests-' + [guid]::NewGuid().ToString('N'))
$checks = 0
$initialPath = $env:PATH

function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}

function New-Fixture([string]$Name, [object[]]$Entries) {
    $workspace = Join-Path $testRoot $Name
    [void][IO.Directory]::CreateDirectory($workspace)
    $archive = Join-Path $workspace 'fixture.zip'
    $stream = [IO.FileStream]::new($archive, [IO.FileMode]::CreateNew)
    $zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
    try {
        foreach ($spec in $Entries) {
            $entry = $zip.CreateEntry($spec.Name)
            if ($spec.ContainsKey('Attributes')) { $entry.ExternalAttributes = $spec.Attributes }
            $writer = $entry.Open()
            try {
                $content = if ($spec.ContainsKey('Content')) { [string]$spec.Content } else { 'fixture bytes' }
                $bytes = [Text.Encoding]::UTF8.GetBytes($content)
                $writer.Write($bytes, 0, $bytes.Length)
            } finally { $writer.Dispose() }
        }
    } finally { $zip.Dispose(); $stream.Dispose() }
    return @{
        Root = $workspace
        ArchivePath = $archive
        Destination = '.vcp/tools/fixture'
        ExpectedDigest = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    }
}

function Check-Rejected([hashtable]$Arguments, [string]$Pattern) {
    $failure = $null
    $output = $null
    try { $output = & $helper @Arguments } catch { $failure = $_.Exception.Message }
    Check ($null -ne $failure -and $failure -like $Pattern) "Expected rejection '$Pattern', got '$failure'."
    Check ($null -eq $output) 'Failed install emitted a success receipt.'
    $stages = @(Get-ChildItem -LiteralPath $Arguments.Root -Filter '.vcp-install-*' -Directory -Recurse -Force)
    Check ($stages.Count -eq 0) 'Failed install left a staging directory.'
}

function Check-BadArchive([string]$Name, [object[]]$Entries, [string]$Pattern) {
    $arguments = New-Fixture $Name $Entries
    Check-Rejected $arguments $Pattern
    Check (-not (Test-Path -LiteralPath (Join-Path $arguments.Root $arguments.Destination))) 'Rejected ZIP created its destination.'
    Check ((Get-FileHash -LiteralPath $arguments.ArchivePath).Hash -eq $arguments.ExpectedDigest) 'Rejected ZIP changed source bytes.'
}

try {
    $arguments = New-Fixture 'valid' @(
        @{ Name = 'vendor/bin/tool.txt'; Content = 'synthetic toolchain payload' },
        @{ Name = 'vendor/'; Content = '' },
        @{ Name = 'vendor/LICENSE'; Content = 'fixture license' }
    )
    $result = & $helper @arguments | ConvertFrom-Json
    Check ($result.status -ceq 'installed_unexecuted') 'Receipt overclaimed readiness.'
    Check ($result.algorithm -ceq 'SHA256' -and $result.digest -ceq $arguments.ExpectedDigest) 'Receipt omitted verified digest.'
    Check ($result.entries -eq 3 -and $result.expanded_bytes -eq 42) 'Receipt counts were incorrect.'
    Check ($result.source -ceq $arguments.ArchivePath) 'Receipt did not identify local source.'
    $installedFile = Join-Path $result.destination 'vendor/bin/tool.txt'
    Check ([IO.File]::ReadAllText($installedFile) -ceq 'synthetic toolchain payload') 'Archive directory layout or bytes changed.'
    Check (@(Get-ChildItem -LiteralPath $arguments.Root -Filter '.vcp-install-*' -Directory -Recurse -Force).Count -eq 0) 'Successful install left a stage.'
    Check-Rejected $arguments '*Destination already exists*'
    Check ([IO.File]::ReadAllText($installedFile) -ceq 'synthetic toolchain payload') 'Existing installation was overwritten.'

    $arguments = New-Fixture 'sha512' @(@{ Name = 'bin/tool.txt' })
    $arguments.Algorithm = 'SHA512'
    $arguments.ExpectedDigest = (Get-FileHash -LiteralPath $arguments.ArchivePath -Algorithm SHA512).Hash.ToLowerInvariant()
    $arguments.ArchivePath = 'fixture.zip'
    Push-Location $arguments.Root
    try {
        $arguments.Remove('Root')
        $result = & $helper @arguments | ConvertFrom-Json
        Check ($result.algorithm -ceq 'SHA512' -and $result.digest -ieq $arguments.ExpectedDigest) 'SHA512 or default Root failed.'
        Check (Test-Path -LiteralPath (Join-Path $result.destination 'bin/tool.txt')) 'Relative source resolution failed.'
    } finally { Pop-Location }

    $arguments = New-Fixture 'bad-hash' @(@{ Name = 'bin/tool.txt' })
    $arguments.ExpectedDigest = '0' * 64
    Check-Rejected $arguments '*digest mismatch*'
    Check (-not (Test-Path -LiteralPath (Join-Path $arguments.Root $arguments.Destination))) 'Bad hash extracted files.'
    $arguments.ExpectedDigest = 'not a digest'
    Check-Rejected $arguments '*exact SHA256 hexadecimal digest*'

    foreach ($name in '../outside', '/absolute', 'C:/drive', 'file:stream', 'CON', 'aux.txt', 'dir/NUL', 'LPT1.txt', 'COM¹', 'CONIN$', 'CONOUT$.txt', 'NUL .txt', 'trailing.', 'trailing ', 'a//b', 'a/./b', 'back\slash', 'a?b') {
        Check-BadArchive ('path-' + [guid]::NewGuid().ToString('N')) @(@{ Name = $name }) '*path*'
    }
    Check-BadArchive 'duplicate' @(@{ Name = 'tool' }, @{ Name = 'tool' }) '*Duplicate*'
    Check-BadArchive 'case-file' @(@{ Name = 'tool' }, @{ Name = 'TOOL' }) '*case*'
    Check-BadArchive 'case-parent' @(@{ Name = 'Bin/one' }, @{ Name = 'bin/two' }) '*case*'
    Check-BadArchive 'file-parent' @(@{ Name = 'bin' }, @{ Name = 'bin/tool' }) '*conflicting*'
    Check-BadArchive 'parent-file' @(@{ Name = 'bin/tool' }, @{ Name = 'bin' }) '*conflicting*'
    Check-BadArchive 'symlink' @(@{ Name = 'link'; Attributes = -1610612736 }) '*links*'
    Check-BadArchive 'reparse' @(@{ Name = 'link'; Attributes = 1024 }) '*links*'
    Check-BadArchive 'fifo' @(@{ Name = 'pipe'; Attributes = 268435456 }) '*special files*'
    Check-BadArchive 'directory-data' @(@{ Name = 'dir/'; Content = 'nonempty' }) '*directory contains*'
    Check-BadArchive 'directory-mode' @(@{ Name = 'file'; Attributes = 1106051072 }) '*type disagrees*'
    Check-BadArchive 'empty' @() '*entry count*'

    $arguments = New-Fixture 'not-zip' @(@{ Name = 'tool' })
    [IO.File]::WriteAllText($arguments.ArchivePath, 'not a ZIP archive')
    $arguments.ExpectedDigest = (Get-FileHash -LiteralPath $arguments.ArchivePath).Hash
    Check-Rejected $arguments '*directory*'
    Check (-not (Test-Path -LiteralPath (Join-Path $arguments.Root $arguments.Destination))) 'Non-ZIP bytes produced an installation.'

    $arguments = New-Fixture 'byte-limit' @(@{ Name = 'tool'; Content = ('x' * 10000) })
    $arguments.MaxExpandedBytes = 10
    Check-Rejected $arguments '*MaxExpandedBytes*'
    $arguments.Remove('MaxExpandedBytes')
    $arguments.MaxArchiveBytes = 10
    Check-Rejected $arguments '*MaxArchiveBytes*'
    $arguments = New-Fixture 'entry-limit' @(@{ Name = 'one' }, @{ Name = 'two' })
    $arguments.MaxEntries = 1
    Check-Rejected $arguments '*MaxEntries*'

    $arguments = New-Fixture 'bad-destination' @(@{ Name = 'tool' })
    foreach ($destination in '../escape', '.vcp/../escape', '/absolute', 'C:\absolute', 'name:stream', 'NUL', 'a//b', 'a/', '.') {
        $arguments.Destination = $destination
        Check-Rejected $arguments '*path*'
    }
    $arguments.Destination = 'existing.txt'
    [IO.File]::WriteAllText((Join-Path $arguments.Root 'existing.txt'), 'preserved')
    Check-Rejected $arguments '*Destination already exists*'
    Check ([IO.File]::ReadAllText((Join-Path $arguments.Root 'existing.txt')) -ceq 'preserved') 'Existing destination file was changed.'
    $arguments.Destination = '.vcp/tools/fixture'
    $arguments.ArchivePath = Join-Path $testRoot 'valid/fixture.zip'
    Check-Rejected $arguments '*inside Root*'

    $arguments = New-Fixture 'bad-url' @(@{ Name = 'tool' })
    $arguments.Remove('ArchivePath')
    foreach ($address in 'http://example.invalid/tool.zip', 'file:///tmp/tool.zip', 'https://user:secret@example.invalid/tool.zip', 'https://example.invalid/tool.zip#fragment') {
        $arguments.Uri = $address
        Check-Rejected $arguments '*HTTPS URLs*'
    }

    # Windows junctions need no symlink privilege; Unix directory symlinks cover the same boundary.
    $arguments = New-Fixture 'path-links' @(@{ Name = 'tool' })
    $outside = Join-Path $testRoot 'outside'
    [void][IO.Directory]::CreateDirectory($outside)
    [IO.File]::WriteAllText((Join-Path $outside 'keep.txt'), 'preserved')
    Copy-Item -LiteralPath $arguments.ArchivePath -Destination (Join-Path $outside 'fixture.zip')
    $linkPath = Join-Path $arguments.Root 'linked'
    $linkType = if ($IsWindows) { 'Junction' } else { 'SymbolicLink' }
    New-Item -ItemType $linkType -Path $linkPath -Target $outside | Out-Null
    try {
        $arguments.Destination = 'linked/install'
        Check-Rejected $arguments '*reparse points*'
        Check (-not (Test-Path -LiteralPath (Join-Path $outside 'install'))) 'Destination link wrote outside workspace.'
        $arguments.Destination = '.vcp/tools/fixture'
        $arguments.ArchivePath = Join-Path $linkPath 'fixture.zip'
        Check-Rejected $arguments '*reparse points*'
        $arguments.Root = $linkPath
        $arguments.ArchivePath = 'fixture.zip'
        Check-Rejected $arguments '*reparse points*'
        Check ([IO.File]::ReadAllText((Join-Path $outside 'keep.txt')) -ceq 'preserved') 'Link handling changed external data.'
    } finally { Remove-Item -LiteralPath $linkPath -Force }
    Check ($env:PATH -ceq $initialPath) 'Installer changed process PATH.'
    Write-Host "ToolchainInstallation: $checks checks passed (offline real ZIP execution; no downloaded programs executed)."
} finally {
    if ($testRoot -and (Test-Path -LiteralPath $testRoot)) {
        $resolved = [IO.Path]::GetFullPath($testRoot)
        if ([IO.Path]::GetDirectoryName($resolved) -ne [IO.Path]::TrimEndingDirectorySeparator($temporary) -or
            [IO.Path]::GetFileName($resolved) -notmatch '^vcp-toolchain-tests-[0-9a-f]{32}$') { throw 'Refusing test cleanup outside owned temporary root.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
