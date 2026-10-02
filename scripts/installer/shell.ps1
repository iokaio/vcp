# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Prepare','Check','Verify','AddPath','RemovePath')][string]$Action,
    [Parameter(Mandatory)][string]$AppRoot,
    [string]$DataRoot,
    [ValidateSet('Private','User')][string]$DataScope = 'Private',
    [string]$ExpectedArchive,
    [string]$CandidateId
)
$ErrorActionPreference = 'Stop'
function Local-Root([string]$Value) {
    if ($Value -notmatch '^[a-zA-Z]:[\\/]' -or $Value -match '["\r\n]' -or $Value.Split([char[]]'\/') -contains '..') { throw 'Explicit local absolute path required' }
    $full = [IO.Path]::GetFullPath($Value).TrimEnd('\')
    if ($full.Length -lt 4) { throw 'A drive root cannot be a VCP installation or protected data directory' }
    for ($cursor = $full; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or -not $item.PSIsContainer) { throw 'Redirected or non-directory path rejected' }
        }
    }
    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($full))
    if ($drive.DriveType -eq [IO.DriveType]::Network) { throw 'Network roots are unsupported' }
    return $full
}
function Within([string]$Child,[string]$Parent) {
    return $Child.Equals($Parent,[StringComparison]::OrdinalIgnoreCase) -or $Child.StartsWith($Parent.TrimEnd('\') + '\',[StringComparison]::OrdinalIgnoreCase)
}
$app = Local-Root $AppRoot
if ($app.Contains(';') -or $app.Contains('%')) { throw 'Program directory cannot contain PATH delimiters or environment references' }
if ($DataScope -eq 'User' -and -not (Within $app (Local-Root ([Environment]::GetFolderPath('ProgramFiles'))))) {
    throw 'Shared VCP installation must be under Program Files so other users can run its executable'
}
$data = if ($DataScope -eq 'Private') {
    if (-not $DataRoot) { throw 'Private installation requires a data directory' }
    Local-Root $DataRoot
} else {
    if ($DataRoot) { throw 'Shared installation cannot bind a user data directory' }
    Local-Root (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'VCP')
}
if ((Within $app $data) -or (Within $data $app)) { throw 'Data and the entire registered application root must be disjoint' }
if ($DataScope -eq 'Private') {
    for ($cursor = $data; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath (Join-Path $cursor '.git')) { throw 'Protected data must be outside repositories' }
    }
    foreach ($name in @('OneDrive','OneDriveConsumer','OneDriveCommercial')) {
        $sync = [Environment]::GetEnvironmentVariable($name)
        if ($sync -and (Within $data ([IO.Path]::GetFullPath($sync).TrimEnd('\')))) { throw 'Protected data must be outside known sync roots' }
    }
}
$marker = Join-Path $app '.vcp-setup-owned.ini'
if (Test-Path -LiteralPath $app) {
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf) -or (Get-Item -LiteralPath $marker -Force).Length -gt 8192) { throw 'Existing application root is not owned by VCP setup' }
    if ((Get-Item -LiteralPath $marker -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected ownership marker rejected' }
    $lines = [IO.File]::ReadAllLines($marker)
    $expectedLast = if ($DataScope -eq 'Private') { 'DataRoot=' + $data } else { 'DataScope=user' }
    $expectedSchema = if ($DataScope -eq 'Private') { 'Schema=vcp-setup-owned/1' } else { 'Schema=vcp-setup-owned/2' }
    if ($lines.Count -ne 4 -or $lines[0] -cne '[VCP]' -or $lines[1] -cne $expectedSchema -or $lines[2] -ine ('AppRoot=' + $app) -or $lines[3] -ine $expectedLast) { throw 'Setup ownership or selected protected data directory changed' }
    foreach ($entry in Get-ChildItem -LiteralPath $app -Force) {
        if ($entry.Name -notin @('.vcp-setup-owned.ini','engine','maintenance','setup-notices','vcp.exe','unins000.exe','unins000.dat','unins000.msg') -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected or redirected application content must be preserved' }
    }
    $maintenance = Join-Path $app 'maintenance'
    if (Test-Path -LiteralPath $maintenance) {
        foreach ($entry in Get-ChildItem -LiteralPath $maintenance -Force) {
            if ($entry.Name -notin @('package-install.ps1','shell.ps1') -or $entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected maintenance content must be preserved' }
        }
    }
    $notices = Join-Path $app 'setup-notices'
    if (Test-Path -LiteralPath $notices) {
        $inventoryPath = Join-Path $notices 'inventory.json'
        if (-not (Test-Path -LiteralPath $inventoryPath -PathType Leaf) -or (Get-Item -LiteralPath $inventoryPath).Length -gt 65536) { throw 'Setup notice inventory missing or too large' }
        $inventory = Get-Content -LiteralPath $inventoryPath -Raw | ConvertFrom-Json
        if ($inventory.schema -cne 'vcp-setup-notices/1' -or @($inventory.files).Count -gt 64) { throw 'Setup notice inventory rejected' }
        $expected = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($row in $inventory.files) {
            if (-not $row.path -or $row.path -match '[\\:]' -or $row.path.StartsWith('/') -or $row.path.Split('/') -contains '..' -or $row.sha256 -cnotmatch '^[a-f0-9]{64}$' -or -not $expected.TryAdd($row.path,$row)) { throw 'Invalid setup notice identity' }
        }
        $count=0
        foreach ($entry in Get-ChildItem -LiteralPath $notices -Recurse -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected setup notice rejected' }
            if ($entry.PSIsContainer) { continue }
            $relative=$entry.FullName.Substring($notices.Length+1).Replace('\','/')
            if ($relative -ceq 'inventory.json') { continue }
            if (-not $expected.ContainsKey($relative) -or $entry.Length -ne $expected[$relative].bytes -or (Get-FileHash -LiteralPath $entry.FullName).Hash.ToLowerInvariant() -cne $expected[$relative].sha256) { throw 'Changed or unrelated setup notices must be preserved' }
            $count++
        }
        if ($count -ne $expected.Count) { throw 'Installed setup notice source is incomplete' }
    }
} elseif ($Action -ne 'Prepare') { throw 'Owned setup root is missing' }
if ($Action -eq 'Prepare') {
    if (-not (Test-Path -LiteralPath $app)) {
        New-Item -ItemType Directory -Path $app | Out-Null
        $file = [IO.File]::Open($marker,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try {
            $schema = if ($DataScope -eq 'Private') { 'vcp-setup-owned/1' } else { 'vcp-setup-owned/2' }
            $last = if ($DataScope -eq 'Private') { "DataRoot=$data" } else { 'DataScope=user' }
            [byte[]]$bytes = [Text.Encoding]::Unicode.GetPreamble() + [Text.Encoding]::Unicode.GetBytes("[VCP]`r`nSchema=$schema`r`nAppRoot=$app`r`n$last`r`n")
            $file.Write($bytes, 0, $bytes.Length); $file.Flush($true)
        } finally { $file.Dispose() }
    }
    exit 0
}
if ($Action -in @('AddPath','RemovePath')) {
    $registry = if ($DataScope -eq 'Private') { [Microsoft.Win32.Registry]::CurrentUser } else { [Microsoft.Win32.Registry]::LocalMachine }
    $environmentName = if ($DataScope -eq 'Private') { 'Environment' } else { 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment' }
    $environmentKey = if ($DataScope -eq 'Private') { $registry.CreateSubKey($environmentName, $true) } else { $registry.OpenSubKey($environmentName, $true) }
    if (-not $environmentKey) { throw 'Selected environment registry key is unavailable' }
    $ownedKey = $null
    try {
        $ownedKey = $registry.CreateSubKey('Software\Ioka LLC\VCP\InstallerPath', $true)
        if (-not $ownedKey) { throw 'VCP installer PATH ownership key is unavailable' }
        $identity = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::Unicode.GetBytes($app.ToUpperInvariant())))
        $owned = $ownedKey.GetValue($identity, $null) -ieq $app
        $kind = if ($environmentKey.GetValueNames() -contains 'Path') { $environmentKey.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
        if ($kind -notin @([Microsoft.Win32.RegistryValueKind]::String, [Microsoft.Win32.RegistryValueKind]::ExpandString)) { throw 'Selected PATH has an unsupported registry type' }
        $path = [string]$environmentKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $parts = [Collections.Generic.List[string]]::new()
        foreach ($part in $path.Split(';')) { if ($part) { $parts.Add($part) } }
        $matches = @($parts | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ieq $app })
        if ($Action -eq 'AddPath') {
            if ($matches.Count -eq 0) {
                $updated = if (-not $path -or $path.EndsWith(';')) { $path + $app } else { $path + ';' + $app }
                if ($updated.Length -gt 32767) { throw 'Selected PATH is too long to add VCP' }
                $environmentKey.SetValue('Path', $updated, $kind)
                $ownedKey.SetValue($identity, $app, [Microsoft.Win32.RegistryValueKind]::String)
            }
        } elseif ($owned) {
            $removed = $false
            $remaining = [Collections.Generic.List[string]]::new()
            foreach ($part in $path.Split(';')) {
                if (-not $removed -and $part -ieq $app) { $removed = $true; continue }
                $remaining.Add($part)
            }
            if ($removed) { $environmentKey.SetValue('Path', [string]::Join(';', $remaining), $kind) }
            $ownedKey.DeleteValue($identity, $false)
        }
    } finally {
        if ($ownedKey) { $ownedKey.Dispose() }
        $environmentKey.Dispose()
    }
    exit 0
}
if ($Action -eq 'Verify') {
    if ($ExpectedArchive -cnotmatch '^[a-f0-9]{64}$' -or $CandidateId -cnotmatch '^[a-f0-9]{64}$') { throw 'Exact candidate identity required' }
    $engine = Join-Path $app 'engine'
    $pointer = Get-Content -LiteralPath (Join-Path $engine 'active.json') -Raw | ConvertFrom-Json
    # A shared installation binds no data root; each account resolves its own.
    $scopeMatches = if ($DataScope -eq 'Private') {
        $pointer.schema -ceq 'vcp-install-pointer/1' -and $pointer.data_root -and [IO.Path]::GetFullPath($pointer.data_root).TrimEnd('\') -ieq $data
    } else {
        $pointer.schema -ceq 'vcp-install-pointer/2' -and $pointer.data_scope -ceq 'user' -and $pointer.PSObject.Properties.Name -notcontains 'data_root'
    }
    if (-not $scopeMatches -or $pointer.package_sha256 -cne $ExpectedArchive -or $pointer.release -cne $ExpectedArchive) { throw 'Active engine differs from selected setup candidate' }
    $manifest = Get-Content -LiteralPath (Join-Path $engine "releases\$ExpectedArchive\manifest.json") -Raw | ConvertFrom-Json
    if ($manifest.release.candidate_id -cne $CandidateId) { throw 'Installed candidate provenance differs' }
    # The Rust launcher writes UTF-8 even when Inno starts PowerShell without a
    # console. Its inherited OEM decoder would corrupt non-ASCII selected paths.
    $previousOutputEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
        $selection = & (Join-Path $app 'vcp.exe') --resolve-installation
        $launcherExit = $LASTEXITCODE
    } finally {
        [Console]::OutputEncoding = $previousOutputEncoding
    }
    if ($launcherExit -ne 0) { throw 'Installed launcher failed validation' }
    $selection = $selection | ConvertFrom-Json
    $expected = Join-Path $engine "releases\$ExpectedArchive\vcp.exe"
    if ($selection.schema -cne 'vcp-installed-engine/1' -or [IO.Path]::GetFullPath($selection.executable).Replace('\\?\','') -ine $expected -or [IO.Path]::GetFullPath($selection.data_directory).Replace('\\?\','').TrimEnd('\') -ine $data) { throw 'Launcher resolves a different engine or data root' }
}
