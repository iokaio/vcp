#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Original VCP helper. Installs verified ZIP bytes in the workspace; never runs them.
[CmdletBinding(DefaultParameterSetName = 'Archive')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Archive')][string]$ArchivePath,
    [Parameter(Mandatory, ParameterSetName = 'Download')][uri]$Uri,
    [Parameter(Mandatory)][string]$ExpectedDigest,
    [ValidateSet('SHA256', 'SHA512')][string]$Algorithm = 'SHA256',
    [Parameter(Mandatory)][string]$Destination,
    [string]$Root = (Get-Location).Path,
    [ValidateRange(1, 4294967296)][long]$MaxArchiveBytes = 268435456,
    [ValidateRange(1, 17179869184)][long]$MaxExpandedBytes = 2147483648,
    [ValidateRange(1, 200000)][int]$MaxEntries = 50000,
    [ValidateRange(1, 1800)][int]$TimeoutSeconds = 120
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }

function Assert-PlainPath([string]$Path) {
    $cursor = $Path
    while ($cursor) {
        try {
            $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'Symbolic links and reparse points are not accepted in installation paths.'
            }
        } catch [Management.Automation.ItemNotFoundException] {
            # Missing descendants are created only below validated ancestors.
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}

function Assert-Segment([string]$Segment) {
    if (-not $Segment -or $Segment.Length -gt 255 -or $Segment -in '.', '..' -or
        $Segment -match '[\x00-\x1f\x7f<>:"|?*\\/]' -or $Segment -match '[. ]$' -or
        $Segment -match '^(?i:CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|CLOCK\$|COM[1-9¹²³]|LPT[1-9¹²³]) *(?:\.|$)') {
        throw 'Unsafe or nonportable path segment.'
    }
}

function Assert-Https([uri]$Address) {
    if (-not $Address.IsAbsoluteUri -or $Address.Scheme -ne 'https' -or
        $Address.UserInfo -or $Address.Fragment -or $Address.AbsoluteUri.Length -gt 4096) {
        throw 'Download sources and redirects must be bounded HTTPS URLs without credentials or fragments.'
    }
}

function Copy-Download([uri]$Address, [IO.Stream]$Output) {
    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $false
    $handler.UseCookies = $false
    $handler.UseDefaultCredentials = $false
    $client = [Net.Http.HttpClient]::new($handler)
    $client.Timeout = [Threading.Timeout]::InfiniteTimeSpan
    $deadline = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds($TimeoutSeconds))
    try {
        for ($redirect = 0; $redirect -le 5; $redirect++) {
            Assert-Https $Address
            $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $Address)
            $response = $null
            try {
                $response = $client.SendAsync($request, [Net.Http.HttpCompletionOption]::ResponseHeadersRead, $deadline.Token).GetAwaiter().GetResult()
                $status = [int]$response.StatusCode
                if ($status -in 301, 302, 303, 307, 308) {
                    if ($redirect -eq 5 -or $null -eq $response.Headers.Location) { throw 'Too many redirects or missing redirect target.' }
                    $Address = [uri]::new($Address, $response.Headers.Location)
                    Assert-Https $Address
                    continue
                }
                if ($status -ne 200) { throw "Download returned HTTP $status." }
                if ($response.Content.Headers.ContentLength -gt $MaxArchiveBytes) { throw 'Archive exceeds MaxArchiveBytes.' }
                $inputStream = $response.Content.ReadAsStreamAsync($deadline.Token).GetAwaiter().GetResult()
                try {
                    $buffer = [byte[]]::new(65536)
                    [long]$total = 0
                    while (($read = $inputStream.ReadAsync($buffer, 0, $buffer.Length, $deadline.Token).GetAwaiter().GetResult()) -gt 0) {
                        $total += $read
                        if ($total -gt $MaxArchiveBytes) { throw 'Archive exceeds MaxArchiveBytes.' }
                        $Output.Write($buffer, 0, $read)
                    }
                } finally { $inputStream.Dispose() }
                # Query strings may contain credentials; receipts omit them.
                return $Address.GetLeftPart([UriPartial]::Path)
            } finally {
                if ($response) { $response.Dispose() }
                $request.Dispose()
            }
        }
    } finally {
        $deadline.Dispose()
        $client.Dispose()
    }
}

$digestLength = if ($Algorithm -eq 'SHA256') { 64 } else { 128 }
if ($ExpectedDigest -notmatch ('\A[0-9a-fA-F]{' + $digestLength + '}\z')) { throw "ExpectedDigest must be an exact $Algorithm hexadecimal digest." }
if (-not [IO.Path]::IsPathFullyQualified($Root)) { throw 'Root must be an absolute workspace path.' }
$rootPath = [IO.Path]::TrimEndingDirectorySeparator([IO.Path]::GetFullPath($Root))
if ($rootPath.Length -gt 2048 -or -not [IO.Directory]::Exists($rootPath)) { throw 'Root must be an existing workspace directory of bounded length.' }
Assert-PlainPath $rootPath
if ($Destination.Length -gt 1024 -or [IO.Path]::IsPathRooted($Destination)) { throw 'Destination must be a bounded workspace-relative path.' }
$segments = $Destination -split '[\\/]'
foreach ($segment in $segments) { Assert-Segment $segment }
$destinationPath = $rootPath
foreach ($segment in $segments) { $destinationPath = [IO.Path]::Combine($destinationPath, $segment) }
$rootPrefix = if ([IO.Path]::EndsInDirectorySeparator($rootPath)) { $rootPath } else { $rootPath + [IO.Path]::DirectorySeparatorChar }
if (-not $destinationPath.StartsWith($rootPrefix, $comparison)) { throw 'Destination escaped Root.' }
Assert-PlainPath $destinationPath
if (Test-Path -LiteralPath $destinationPath) { throw 'Destination already exists; choose a new version directory.' }
$sourcePath = $null
if ($PSCmdlet.ParameterSetName -eq 'Archive') {
    $sourcePath = [IO.Path]::GetFullPath($ArchivePath, $rootPath)
    if (-not $sourcePath.StartsWith($rootPrefix, $comparison)) { throw 'ArchivePath must be a file inside Root.' }
    if ($sourcePath.Length -gt 4096) { throw 'ArchivePath exceeds the bounded path length.' }
    foreach ($segment in ([IO.Path]::GetRelativePath($rootPath, $sourcePath) -split '[\\/]')) { Assert-Segment $segment }
    Assert-PlainPath $sourcePath
    if (-not [IO.File]::Exists($sourcePath)) { throw 'ArchivePath is not an existing file.' }
} else { Assert-Https $Uri }

$parent = [IO.Path]::GetDirectoryName($destinationPath)
$stage = $null
$archiveStream = $null
$zip = $null
try {
    Assert-PlainPath $parent
    [void][IO.Directory]::CreateDirectory($parent)
    Assert-PlainPath $parent
    $candidateStage = Join-Path $parent ('.vcp-install-' + [guid]::NewGuid().ToString('N'))
    # No -Force: never adopt or clean an existing stage directory.
    New-Item -ItemType Directory -Path $candidateStage -ErrorAction Stop | Out-Null
    $stage = $candidateStage
    Assert-PlainPath $stage
    if ($sourcePath) {
        # Keep these exact bytes open through verification and extraction.
        $archiveStream = [IO.FileStream]::new($sourcePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $source = $sourcePath
    } else {
        $archiveStream = [IO.FileStream]::new((Join-Path $stage 'download.zip'), [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $source = Copy-Download $Uri $archiveStream
        $archiveStream.Flush()
    }
    if ($archiveStream.Length -gt $MaxArchiveBytes) { throw 'Archive exceeds MaxArchiveBytes.' }
    $archiveStream.Position = 0
    $hasher = if ($Algorithm -eq 'SHA256') { [Security.Cryptography.SHA256]::Create() } else { [Security.Cryptography.SHA512]::Create() }
    try { $actualDigest = [Convert]::ToHexString($hasher.ComputeHash($archiveStream)) } finally { $hasher.Dispose() }
    if (-not $actualDigest.Equals($ExpectedDigest, [StringComparison]::OrdinalIgnoreCase)) { throw "$Algorithm digest mismatch; no archive entries were extracted." }
    $archiveStream.Position = 0
    $zip = [IO.Compression.ZipArchive]::new($archiveStream, [IO.Compression.ZipArchiveMode]::Read, $true)
    if ($zip.Entries.Count -eq 0 -or $zip.Entries.Count -gt $MaxEntries) { throw 'Archive entry count is empty or exceeds MaxEntries.' }

    # Validate the entire namespace and declared sizes before writing any entry.
    $paths = [Collections.Generic.Dictionary[string, bool]]::new([StringComparer]::OrdinalIgnoreCase)
    $spellings = [Collections.Generic.Dictionary[string, string]]::new([StringComparer]::OrdinalIgnoreCase)
    $explicitPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $validated = [Collections.Generic.List[object]]::new()
    [long]$expandedBytes = 0
    foreach ($entry in $zip.Entries) {
        $name = $entry.FullName
        if ($name.Length -gt 1024 -or $name.Contains('\') -or $name.StartsWith('/')) { throw 'Unsafe ZIP entry path.' }
        $directory = $name.EndsWith('/')
        $relative = if ($directory) { $name.Substring(0, $name.Length - 1) } else { $name }
        $parts = $relative -split '/'
        foreach ($part in $parts) { Assert-Segment $part }
        $mode = ($entry.ExternalAttributes -shr 16) -band 0xf000
        if ($mode -notin 0, 0x8000, 0x4000 -or ($entry.ExternalAttributes -band 0x400) -ne 0) { throw 'ZIP links, reparse points and special files are not accepted.' }
        if (($mode -eq 0x4000 -and -not $directory) -or ($mode -eq 0x8000 -and $directory)) { throw 'ZIP entry type disagrees with its path.' }
        if ($directory -and $entry.Length -ne 0) { throw 'ZIP directory contains file data.' }
        if (-not $explicitPaths.Add($relative)) { throw 'Duplicate or case-colliding ZIP entries.' }
        $key = ''
        for ($index = 0; $index -lt $parts.Length; $index++) {
            $key = if ($key) { $key + '/' + $parts[$index] } else { $parts[$index] }
            $isDirectory = $index -lt $parts.Length - 1 -or $directory
            if ($paths.ContainsKey($key)) {
                if ($spellings[$key] -cne $key -or $paths[$key] -ne $isDirectory) { throw 'ZIP contains case collisions or conflicting file and directory paths.' }
            } else { $paths.Add($key, $isDirectory); $spellings.Add($key, $key) }
        }
        $expandedBytes += $entry.Length
        if ($expandedBytes -gt $MaxExpandedBytes) { throw 'Archive exceeds MaxExpandedBytes.' }
        $validated.Add([pscustomobject]@{ Entry = $entry; Parts = $parts; Directory = $directory })
    }

    $payload = Join-Path $stage 'payload'
    New-Item -ItemType Directory -Path $payload -ErrorAction Stop | Out-Null
    [long]$written = 0
    $buffer = [byte[]]::new(65536)
    foreach ($record in $validated) {
        $target = $payload
        foreach ($part in $record.Parts) { $target = [IO.Path]::Combine($target, $part) }
        Assert-PlainPath $target
        if ($record.Directory) { [void][IO.Directory]::CreateDirectory($target); continue }
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
        Assert-PlainPath $target
        $outputStream = [IO.FileStream]::new($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $inputStream = $null
        try {
            $inputStream = $record.Entry.Open()
            [long]$entryWritten = 0
            while (($read = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                $written += $read
                $entryWritten += $read
                if ($written -gt $MaxExpandedBytes -or $entryWritten -gt $record.Entry.Length) { throw 'Expanded ZIP bytes exceed the declared size or MaxExpandedBytes.' }
                $outputStream.Write($buffer, 0, $read)
            }
            if ($entryWritten -ne $record.Entry.Length) { throw 'ZIP entry length does not match its declared size.' }
        } finally {
            if ($inputStream) { $inputStream.Dispose() }
            $outputStream.Dispose()
        }
    }
    $zip.Dispose(); $zip = $null
    $archiveStream.Dispose(); $archiveStream = $null
    Assert-PlainPath $destinationPath
    Assert-PlainPath $payload
    # Same-volume move publishes a complete directory and refuses overwrite.
    [IO.Directory]::Move($payload, $destinationPath)
    $receipt = [ordered]@{
        status = 'installed_unexecuted'
        source = $source
        destination = $destinationPath
        algorithm = $Algorithm
        digest = $actualDigest
        entries = $validated.Count
        expanded_bytes = $written
    }
} finally {
    if ($zip) { $zip.Dispose() }
    if ($archiveStream) { $archiveStream.Dispose() }
    if ($stage) {
        $stagePath = [IO.Path]::GetFullPath($stage)
        if ([IO.Path]::GetDirectoryName($stagePath).Equals($parent, $comparison) -and
            [IO.Path]::GetFileName($stagePath) -match '^\.vcp-install-[0-9a-f]{32}$') {
            Assert-PlainPath $stagePath
            $links = @(Get-ChildItem -LiteralPath $stagePath -Force -Recurse | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
            if ($links.Count) { throw 'Unexpected link in owned staging directory; cleanup refused.' }
            Remove-Item -LiteralPath $stagePath -Recurse -Force
        } else { throw 'Staging cleanup path validation failed.' }
    }
}
$receipt | ConvertTo-Json -Compress
