# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Install','Upgrade','Rollback','Uninstall')][string]$Action,
    [string]$PackageZip,
    [Parameter(Mandatory)][string]$InstallRoot,
    [Parameter(Mandatory)][string]$DataRoot,
    [string]$StateManifest
)
$ErrorActionPreference = 'Stop'
function Assert-PortablePath([string]$Name) {
    if (-not $Name -or $Name.Length -gt 512 -or $Name.Contains('\') -or
        @($Name.Split('/') | Where-Object { -not $_ -or $_ -in @('.', '..') -or $_ -match '[<>:"|?*\x00-\x1f]' -or $_ -match '[. ]$' -or $_ -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)' }).Count) {
        throw 'Unsafe portable package path'
    }
}
function Assert-NoReparseAncestors([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse-point path is not allowed: $cursor" }
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if (-not $parent -or $parent.FullName -eq $cursor) { break }
        $cursor = $parent.FullName
    }
}
function Assert-OrdinaryTree([string]$Root) {
    Assert-NoReparseAncestors $Root
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($Root)
    while ($pending.Count) {
        foreach ($entry in Get-ChildItem -LiteralPath $pending.Dequeue() -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked installation entry is not allowed' }
            if ($entry.PSIsContainer) { $pending.Enqueue($entry.FullName) }
        }
    }
}
function Write-AtomicJson([string]$TargetPath, $Value) {
    $parent = [IO.Path]::GetFullPath((Split-Path -Path $TargetPath -Parent))
    $temporary = Join-Path $parent ('.tmp-' + [guid]::NewGuid().ToString())
    $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $temporary -Encoding utf8
    if (Test-Path -LiteralPath $TargetPath) {
        $backup = $TargetPath + '.bak'
        if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force }
        [IO.File]::Replace($temporary, $TargetPath, $backup, $true)
        if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force }
    } else { Move-Item -LiteralPath $temporary -Destination $TargetPath }
}
function Assert-Manifest([string]$Root, $Manifest) {
    Assert-OrdinaryTree $Root
    if ($Manifest.schema -ne 'vcp-distribution-manifest/1' -or -not $Manifest.files -or $Manifest.files.Count -gt 4096) { throw 'Unsupported package manifest' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $Manifest.files) {
        Assert-PortablePath $entry.path
        if ($entry.path -ieq 'manifest.json' -or -not $seen.Add($entry.path) -or $entry.sha256 -cnotmatch '^[a-f0-9]{64}$' -or $entry.bytes -lt 0 -or $entry.bytes -gt 1073741824) { throw 'Unsafe or duplicate manifest entry' }
        $file = Join-Path $Root ($entry.path -replace '/', '\')
        $info = Get-Item -LiteralPath $file -Force -ErrorAction Stop
        if ($info.PSIsContainer -or ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $info.Length -ne [int64]$entry.bytes) { throw "Manifest file mismatch: $($entry.path)" }
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256) { throw "Manifest digest mismatch: $($entry.path)" }
    }
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $Manifest.files) { [void]$expected.Add($entry.path) }
    [void]$expected.Add('manifest.json')
    $actual = @(Get-ChildItem -LiteralPath $Root -Recurse -File -Force | ForEach-Object { $_.FullName.Substring($Root.Length + 1).Replace('\','/') })
    if ($actual.Count -ne $expected.Count -or @($actual | Where-Object { -not $expected.Contains($_) }).Count) { throw 'Extracted package contains unlisted files' }
    if ($Manifest.model_provisioning.bundled) { throw 'Bundled model assets are not permitted' }
}
function Assert-Compatible($Candidate, $State) {
    if (-not $State) { return }
    foreach ($field in @('canonical','config','index')) {
        $left = $Candidate.compatibility.$field; $right = $State.compatibility.$field
        if (-not $left -or -not $right -or $left -ne $right) { throw "Incompatible or unspecified $field format" }
    }
}
$install = [IO.Path]::GetFullPath($InstallRoot)
$data = [IO.Path]::GetFullPath($DataRoot)
if ($install.TrimEnd('\') -ieq $data.TrimEnd('\') -or $data.StartsWith($install.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -or $install.StartsWith($data.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Install root and protected data root must be disjoint'
}
Assert-NoReparseAncestors $install; Assert-NoReparseAncestors $data
$releases = Join-Path $install 'releases'; $pointer = Join-Path $install 'active.json'; $marker = Join-Path $install '.vcp-install-owned.json'
function Assert-OwnedInstallation {
    Assert-OrdinaryTree $install
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { throw 'Installation ownership marker is missing' }
    $owned = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
    if ($owned.schema -ne 'vcp-install-owned/1' -or [IO.Path]::GetFullPath($owned.install_root) -ine $install -or [IO.Path]::GetFullPath($owned.data_root) -ine $data) { throw 'Installation ownership or protected data root does not match' }
}
function Get-ValidatedRelease([string]$Id) {
    if ($Id -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid installed release identity' }
    $root = Join-Path $releases $Id
    Assert-NoReparseAncestors $root
    $manifest = Get-Content -LiteralPath (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
    Assert-Manifest $root $manifest
    return $manifest
}
function Assert-RecoverableInstall {
    Assert-OwnedInstallation
    if (Test-Path -LiteralPath $pointer) { throw 'Install root already has an active release' }
    $unexpected = @(Get-ChildItem -LiteralPath $install -Force | Where-Object { $_.Name -notin @('.vcp-install-owned.json','releases','.staging') })
    if ($unexpected.Count) { throw 'Unexpected installation files; refusing install recovery' }
    if (-not (Test-Path -LiteralPath $releases -PathType Container)) { throw 'Install recovery requires a retained release' }
    $releaseEntries = @(Get-ChildItem -LiteralPath $releases -Force)
    if (-not $releaseEntries.Count) { throw 'Install recovery requires a retained release' }
    foreach ($releaseEntry in $releaseEntries) { $null = Get-ValidatedRelease $releaseEntry.Name }
    $staging = Join-Path $install '.staging'
    if ((Test-Path -LiteralPath $staging) -and @(Get-ChildItem -LiteralPath $staging -Force).Count) { throw 'Unfinished staging content requires recovery before install retry' }
}
if ($Action -eq 'Uninstall') {
    if (-not (Test-Path -LiteralPath $install)) { Write-Output 'already-uninstalled'; exit 0 }
    Assert-OwnedInstallation
    $unexpected = @(Get-ChildItem -LiteralPath $install -Force | Where-Object { $_.Name -notin @('.vcp-install-owned.json','active.json','releases','.staging') })
    if ($unexpected.Count) { throw 'Unexpected installation files; preserving them' }
    foreach ($releaseEntry in Get-ChildItem -LiteralPath $releases -Force) { $null = Get-ValidatedRelease $releaseEntry.Name }
    $staging = Join-Path $install '.staging'
    if ((Test-Path -LiteralPath $staging) -and @(Get-ChildItem -LiteralPath $staging -Force).Count) { throw 'Unfinished staging content requires recovery before uninstall' }
    foreach ($payload in Get-ChildItem -LiteralPath $install -Recurse -File -Force) {
        $deadline = [Diagnostics.Stopwatch]::StartNew()
        while ($true) {
            try {
                $probe = [IO.File]::Open($payload.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
                $probe.Dispose()
                break
            } catch [IO.IOException] {
                # Windows can briefly retain an image lock after process exit.
                # A persistent lock still fails before any payload is removed.
                if ($deadline.ElapsedMilliseconds -ge 5000) { throw }
                Start-Sleep -Milliseconds 100
            }
        }
    }
    # All recursive targets are now ordinary owned descendants of the exact
    # validated installation root. Keep ownership if a locked removal fails.
    if (Test-Path -LiteralPath $releases) { Remove-Item -LiteralPath $releases -Recurse -Force }
    if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    if (Test-Path -LiteralPath $pointer) { Remove-Item -LiteralPath $pointer -Force }
    Remove-Item -LiteralPath $marker -Force
    if (Test-Path -LiteralPath $install) {
        $remaining = @(Get-ChildItem -LiteralPath $install -Force)
        if ($remaining.Count) { throw 'Install root contains locked or unexpected files; refusing incomplete uninstall' }
        Remove-Item -LiteralPath $install -Force
    }
    if (Test-Path -LiteralPath $data) { Write-Output 'data-root-preserved' }
    exit 0
}
if ($Action -eq 'Rollback') {
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf) -or -not (Test-Path -LiteralPath $pointer -PathType Leaf)) { throw 'Rollback requires an owned active installation' }
    Assert-OwnedInstallation
    $current = Get-Content -LiteralPath $pointer -Raw | ConvertFrom-Json
    if (-not $current.previous_release) { throw 'No compatible previous release is recorded' }
    $targetManifest = Get-ValidatedRelease $current.previous_release
    $currentManifest = Get-ValidatedRelease $current.release
    Assert-Compatible $targetManifest $currentManifest
    $target = Join-Path $releases $current.previous_release
    if ($StateManifest) { Assert-Compatible $targetManifest (Get-Content -LiteralPath ([IO.Path]::GetFullPath($StateManifest)) -Raw | ConvertFrom-Json) }
    Write-AtomicJson $pointer ([ordered]@{ schema='vcp-install-pointer/1'; release=$current.previous_release; package_sha256=$current.previous_package_sha256; activated_utc=[DateTime]::UtcNow.ToString('o'); data_root=$data; rollback_from=$current.release })
    Write-Output (Join-Path $target 'manifest.json'); exit 0
}
if (-not $PackageZip) { throw "$Action requires -PackageZip" }
$zipPath = [IO.Path]::GetFullPath($PackageZip)
if (-not (Test-Path -LiteralPath $zipPath -PathType Leaf)) { throw 'Package ZIP not found' }
$installExisted = Test-Path -LiteralPath $install
if ($Action -eq 'Install' -and $installExisted) { Assert-RecoverableInstall }
if ($Action -eq 'Upgrade') { Assert-OwnedInstallation }
$archiveHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$stageParent = Join-Path $install '.staging'
$installCreatedByCall = -not $installExisted
$stageParentCreatedByCall = -not (Test-Path -LiteralPath $stageParent)
$releasesCreatedByCall = -not (Test-Path -LiteralPath $releases)
$stage = $null
try {
    New-Item -ItemType Directory -Path $stageParent,$releases -Force | Out-Null
    $stage = Join-Path $stageParent ([guid]::NewGuid().ToString()); New-Item -ItemType Directory -Path $stage | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        if ($zip.Entries.Count -gt 4097) { throw 'Package entry count exceeds limit' }
        [int64]$expandedBytes = 0
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName
            Assert-PortablePath $name
            $expandedBytes += $entry.Length
            if (-not $names.Add($name) -or $entry.Length -gt 1073741824 -or $expandedBytes -gt 2147483648) { throw 'Duplicate or oversized ZIP entry' }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($zipPath, $stage)
    $manifestPath = Join-Path $stage 'manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'Package manifest missing' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Assert-Manifest $stage $manifest
    if ($StateManifest) { Assert-Compatible $manifest (Get-Content -LiteralPath ([IO.Path]::GetFullPath($StateManifest)) -Raw | ConvertFrom-Json) }
    $release = Join-Path $releases $archiveHash
    if (Test-Path -LiteralPath $release) { $null = Get-ValidatedRelease $archiveHash; Remove-Item -LiteralPath $stage -Recurse -Force }
    if ($Action -eq 'Install') { Write-AtomicJson $marker ([ordered]@{ schema='vcp-install-owned/1'; install_root=$install; data_root=$data; created_utc=[DateTime]::UtcNow.ToString('o') }) }
    if ($Action -eq 'Upgrade' -and -not (Test-Path -LiteralPath $pointer)) { throw 'Upgrade requires an active installation' }
    if (-not (Test-Path -LiteralPath $release)) { Move-Item -LiteralPath $stage -Destination $release }
    if ($env:VCP_PACKAGE_INSTALL_FAULT -eq 'before-activation') { throw 'Deterministic fault before active pointer replacement' }
    $previous = if (Test-Path -LiteralPath $pointer) { Get-Content -LiteralPath $pointer -Raw | ConvertFrom-Json } else { $null }
    if ($previous) { Assert-Compatible $manifest (Get-ValidatedRelease $previous.release) }
    $pointerValue = [ordered]@{ schema = 'vcp-install-pointer/1'; release = [IO.Path]::GetFileName($release); package_sha256 = $archiveHash; activated_utc = [DateTime]::UtcNow.ToString('o'); data_root = $data; previous_release = if($previous){$previous.release}else{$null}; previous_package_sha256 = if($previous){$previous.package_sha256}else{$null} }
    Write-AtomicJson $pointer $pointerValue
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    Write-Output (Join-Path $release 'manifest.json')
} catch {
    if ($stage -and (Test-Path -LiteralPath $stage)) {
        Assert-NoReparseAncestors $stage
        Assert-OrdinaryTree $stage
        Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($Action -eq 'Install' -and $installCreatedByCall -and -not (Test-Path -LiteralPath $marker)) {
        # Remove only empty scaffolding created by this invocation. A concurrent
        # or unexpected entry makes the non-recursive removal fail closed.
        Assert-NoReparseAncestors $install
        if (Test-Path -LiteralPath $install) { Assert-OrdinaryTree $install }
        foreach ($createdDirectory in @(
            @{ Path = $releases; Created = $releasesCreatedByCall },
            @{ Path = $stageParent; Created = $stageParentCreatedByCall },
            @{ Path = $install; Created = $installCreatedByCall }
        )) {
            if ($createdDirectory.Created -and (Test-Path -LiteralPath $createdDirectory.Path) -and -not @(Get-ChildItem -LiteralPath $createdDirectory.Path -Force).Count) {
                Remove-Item -LiteralPath $createdDirectory.Path -Force -ErrorAction SilentlyContinue
            }
        }
    }
    throw
}
