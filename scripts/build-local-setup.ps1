# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# BETA-06: explicit unsigned local candidate; release construction remains build-setup.ps1.
[CmdletBinding()]
param(
    [string]$NativeResult,
    [string]$BuildReceipt,
    [string]$Launcher,
    [string]$CompilerInstaller,
    [string]$OutputRoot,
    # Leave a logical processor free and budget 4 GiB per job for Rust/C++.
    [ValidateRange(1,256)][int]$Jobs = [int][Math]::Max(1, [Math]::Min(32, [Math]::Min(
        [Environment]::ProcessorCount - 1,
        [Math]::Floor([GC]::GetGCMemoryInfo().TotalAvailableMemoryBytes / 4GB))))
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows setup construction required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$localTool = Join-Path $PSScriptRoot 'installer/local-candidate.cjs'
$node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
function Ordinary-File([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $item = Get-Item -LiteralPath $full -Force
    if ($item.PSIsContainer -or $item.Length -eq 0) { throw 'Nonempty ordinary input file required' }
    for ($cursor = $full; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked setup input rejected' }
    }
    return $full
}
function Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Resolve-CompilerInstaller([string]$Requested,$Pin,[string]$CacheRoot) {
    if ($Pin.sha256 -notmatch '^[a-f0-9]{64}$' -or $Pin.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid pinned Inno Setup identity' }
    if ($Requested) {
        $file = Ordinary-File $Requested
        if ((Hash $file) -cne $Pin.sha256) { throw 'Pinned Inno Setup installer hash mismatch' }
        return $file
    }
    $cache = [IO.Path]::GetFullPath($CacheRoot)
    for ($cursor = $cache; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Ordinary compiler cache directory required' }
        }
    }
    New-Item -ItemType Directory -Path $cache -Force | Out-Null
    $file = Join-Path $cache ('innosetup-' + $Pin.version + '-' + $Pin.sha256 + '.exe')
    if (-not (Test-Path -LiteralPath $file)) {
        $uri = [uri]$Pin.download_url
        if (-not $uri.IsAbsoluteUri -or $uri.Scheme -cne 'https') { throw 'HTTPS pinned compiler download required' }
        $temporary = Join-Path $cache ([guid]::NewGuid().ToString('N') + '.download')
        try {
            Write-Information ('Downloading pinned Inno Setup ' + $Pin.version) -InformationAction Continue
            Invoke-WebRequest -Uri $uri -OutFile $temporary -TimeoutSec 180
            $download = Ordinary-File $temporary
            if ((Hash $download) -cne $Pin.sha256) { throw 'Pinned Inno Setup installer hash mismatch' }
            # Concurrent builders may have populated the same verified cache.
            if (-not (Test-Path -LiteralPath $file)) { Move-Item -LiteralPath $download -Destination $file -ErrorAction Stop }
        } finally {
            if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary -Force }
        }
    }
    $file = Ordinary-File $file
    if ((Hash $file) -cne $Pin.sha256) { throw 'Pinned Inno Setup installer hash mismatch' }
    return $file
}
function Assert-SetupProductVersion([string]$Actual,[string]$Expected) {
    # Inno's PE string resource pads ProductVersion with spaces.
    if ($Actual.Trim() -cnotin @($Expected,($Expected + '.0'))) { throw 'Built setup product version differs from the selected candidate' }
}
function Invoke-LocalProductionBuild([string]$BuildScript,[string]$BuildRoot,[int]$BuildJobs) {
    $inheritedFlags = [Environment]::GetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS','Process')
    $recipeFlags = @('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static') -join [char]31
    # Older in-process builds left these exact flags in the calling shell.
    # Other overrides still reach, and are rejected by, the production recipe.
    $retainedRecipe = $inheritedFlags -ceq $recipeFlags
    try {
        if ($retainedRecipe) {
            [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',[NullString]::Value,'Process')
            Write-Information 'Clearing retained VCP compiler flags for this local build' -InformationAction Continue
        }
        & $BuildScript -OutputRoot $BuildRoot -Jobs $BuildJobs
    } finally {
        if ($retainedRecipe) { [Environment]::SetEnvironmentVariable('CARGO_ENCODED_RUSTFLAGS',$inheritedFlags,'Process') }
    }
}
function Open-LocalBuildLock([string]$Repository) {
    $directory = Join-Path $Repository 'artifacts'
    for ($cursor = $directory; $cursor; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Ordinary local build directory required' }
        }
    }
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $file = Join-Path $directory 'local-setup-build.lock'
    if (Test-Path -LiteralPath $file) {
        $item = Get-Item -LiteralPath $file -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Ordinary local build lock file required' }
    }
    try { return [IO.File]::Open($file,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None) }
    catch [IO.IOException] {
        if (($_.Exception.HResult -band 0xffff) -in @(32,33)) { throw 'Another local setup build is already using this checkout. Wait for it to finish before retrying.' }
        throw
    }
}
$provided = @(@($NativeResult,$BuildReceipt,$Launcher) | Where-Object { $_ }).Count
if ($provided -notin @(0,3)) { throw 'Supply NativeResult, BuildReceipt and Launcher together, or omit all three for a local production build.' }
$buildLock = Open-LocalBuildLock $repository
try {
$channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json -Depth 100
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/local-setup' }
if ($provided -eq 3) {
    $versionRoot = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) $channel.native_version
    $canonicalRoot = Join-Path (Join-Path $repository 'artifacts/local-setup') $channel.native_version
    if ((Test-Path -LiteralPath $versionRoot) -or (Test-Path -LiteralPath $canonicalRoot)) { throw 'This local candidate version already has an output directory. Omit the existing build inputs to build a new automatically versioned candidate.' }
}
$CompilerInstaller = Resolve-CompilerInstaller $CompilerInstaller $channel.installer (Join-Path $repository 'artifacts/build-tools/inno-setup')
if ($provided -eq 0) {
    $preparedJson = & $node (Join-Path $PSScriptRoot 'release/local-version.cjs') prepare $repository ([IO.Path]::GetFullPath($OutputRoot))
    if ($LASTEXITCODE -ne 0) { throw 'Local candidate version preparation failed' }
    $prepared = $preparedJson | ConvertFrom-Json -Depth 20
    $channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json -Depth 100
    if ($prepared.version -cne $channel.native_version) { throw 'Prepared candidate version differs from the release channel' }
    $versionRoot = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) $channel.native_version
    if ($prepared.versionRoot -ine $versionRoot -or -not (Test-Path -LiteralPath $versionRoot -PathType Container)) { throw 'Prepared candidate output directory differs from the requested output root' }
    Write-Information ('Building local candidate ' + $channel.native_version) -InformationAction Continue
} else {
    # Existing receipts bind their version and source; never rewrite those inputs.
    New-Item -ItemType Directory -Path $canonicalRoot | Out-Null
    if ($versionRoot -ine $canonicalRoot) { New-Item -ItemType Directory -Path $versionRoot | Out-Null }
}
$out = Join-Path $versionRoot ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
if ($provided -eq 0) {
    $buildOutput = Invoke-LocalProductionBuild (Join-Path $PSScriptRoot 'build-production.ps1') (Join-Path $out 'build') $Jobs
    $BuildReceipt = @($buildOutput)[-1]
    $built = Get-Content -LiteralPath $BuildReceipt -Raw | ConvertFrom-Json -Depth 100
    $Launcher = $built.launcher
    $packageOutput = & (Join-Path $PSScriptRoot 'package.ps1') -Executable $built.executable -BuildReceipt $BuildReceipt -OutputRoot (Join-Path $out 'native')
    $NativeResult = @($packageOutput)[-1]
}
$NativeResult = Ordinary-File $NativeResult
$BuildReceipt = Ordinary-File $BuildReceipt
$Launcher = Ordinary-File $Launcher
$native = Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json -Depth 100
$receipt = Get-Content -LiteralPath $BuildReceipt -Raw | ConvertFrom-Json -Depth 100
if ($native.schema -cne 'vcp-distribution-result/1' -or $native.status -cne 'candidate' -or $native.manifest.artifact -cne 'unsigned-local-candidate' -or $native.manifest.release -or $native.manifest.signing.status -cne 'unsigned' -or $native.manifest.build.status -cne 'recorded-local-build') { throw 'Recorded unsigned local native package required' }
if ([IO.Path]::GetFileName($native.package) -cne $native.package -or $native.package -notmatch '\.zip$') { throw 'Adjacent native ZIP required' }
$archive = Ordinary-File (Join-Path (Split-Path -Parent $NativeResult) $native.package)
if ((Hash $archive) -cne $native.archive_sha256 -or (Hash $BuildReceipt) -cne $native.manifest.build.receipt_sha256) { throw 'Native archive or build receipt identity mismatch' }
$verifiedJson = & $node $localTool verify $repository $BuildReceipt $Launcher
if ($LASTEXITCODE -ne 0) { throw 'Local production build provenance rejected' }
$staged = Join-Path $out 'package'
New-Item -ItemType Directory -Path $staged | Out-Null
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    if ($zip.Entries.Count -gt 4097) { throw 'Native ZIP entry count exceeds bound' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    [int64]$expanded = 0
    foreach ($entry in $zip.Entries) {
        $name = $entry.FullName
        if (-not $name -or $name.Contains('\') -or $name.Length -gt 512 -or
            @($name.Split('/') | Where-Object { -not $_ -or $_ -in @('.','..') -or $_ -match '[<>:"|?*\x00-\x1f]' -or $_ -match '[. ]$' -or $_ -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)' }).Count -or
            -not $seen.Add($name)) { throw 'Unsafe or duplicate native ZIP path' }
        $expanded += $entry.Length
        if ($entry.Length -gt 1073741824 -or $expanded -gt 2147483648) { throw 'Native ZIP expanded size exceeds bound' }
    }
} finally { $zip.Dispose() }
[IO.Compression.ZipFile]::ExtractToDirectory($archive,$staged)
$localJson = & $node $localTool stage $repository $staged $NativeResult $BuildReceipt $Launcher
if ($LASTEXITCODE -ne 0) { throw 'Native archive contents or local candidate provenance rejected' }
$localCandidate = $localJson | ConvertFrom-Json -Depth 100
$packagedLauncher = Ordinary-File (Join-Path $staged 'vcp-launch.exe')
$packagedLauncherHash = Hash $packagedLauncher
$archive = Join-Path $out ('vcp-' + $channel.native_version + '-windows-x64-unsigned-local.zip')
[IO.Compression.ZipFile]::CreateFromDirectory($staged,$archive,[IO.Compression.CompressionLevel]::Optimal,$false)
$native = [ordered]@{
    schema='vcp-distribution-result/1';status='candidate';package=[IO.Path]::GetFileName($archive)
    archive_sha256=(Hash $archive);manifest=(Get-Content -LiteralPath (Join-Path $staged 'manifest.json') -Raw | ConvertFrom-Json -Depth 100)
}
$native.limitations = $native.manifest.limitations
$nativeResultPath = Join-Path $out 'native-result.json'
$native | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $nativeResultPath -Encoding utf8NoBOM
$compiler = Join-Path $out 'compiler'
$compilerLog = Join-Path $out 'compiler-provision.log'
# Extract the verified compiler locally; the setup EXE does not download or
# install product prerequisites.
$process = Start-Process -FilePath $CompilerInstaller -ArgumentList @('/PORTABLE=1','/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER','/NOICONS',('/DIR="' + $compiler + '"'),('/LOG="' + $compilerLog + '"')) -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0) { throw "Pinned compiler extraction failed; retained $compilerLog" }
$iscc = Ordinary-File (Join-Path $compiler 'ISCC.exe')
$noticesRoot = Join-Path $repository 'release/installer-notices'
$noticeJson = & $node (Join-Path $PSScriptRoot 'installer/notices.cjs') $noticesRoot (Join-Path $repository 'release/internal-beta.json') $compiler
if ($LASTEXITCODE -ne 0) { throw 'Setup runtime notice inventory failed' }
$notices = $noticeJson | ConvertFrom-Json -Depth 20
$setupFiles = Join-Path $out 'setup-files'
New-Item -ItemType Directory -Path (Join-Path $setupFiles 'maintenance') | Out-Null
Copy-Item -LiteralPath $packagedLauncher -Destination (Join-Path $setupFiles 'vcp.exe')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'installer/shell.ps1'),(Join-Path $PSScriptRoot 'package-install.ps1') -Destination (Join-Path $setupFiles 'maintenance')
$stagedNotices = Join-Path $setupFiles 'setup-notices'
Copy-Item -LiteralPath $noticesRoot -Destination $stagedNotices -Recurse
$pathLimitsJson = & $node (Join-Path $PSScriptRoot 'installer/path-limits.cjs') $setupFiles
if ($LASTEXITCODE -ne 0) { throw 'Staged setup path inventory rejected' }
$pathLimits = $pathLimitsJson | ConvertFrom-Json -Depth 20
$expectedSetupFiles = @(
    @{path='vcp.exe';sha256=$packagedLauncherHash}
    @{path='maintenance/shell.ps1';sha256=(Hash (Join-Path $PSScriptRoot 'installer/shell.ps1'))}
    @{path='maintenance/package-install.ps1';sha256=(Hash (Join-Path $PSScriptRoot 'package-install.ps1'))}
    @{path='setup-notices/inventory.json';sha256=$notices.inventory_sha256}
) + @($notices.files | ForEach-Object { @{path=('setup-notices/' + $_.path);sha256=$_.sha256} })
if ($pathLimits.files.Count -ne $expectedSetupFiles.Count) { throw 'Unexpected staged setup file inventory' }
foreach ($expected in $expectedSetupFiles) {
    $actual = @($pathLimits.files | Where-Object path -CEQ $expected.path)
    if ($actual.Count -ne 1 -or $actual[0].sha256 -cne $expected.sha256) { throw 'Staged setup file differs from its reviewed source' }
}
# The script checks the compiler's own encoded Ver. These upstream binaries do
# not expose a useful Windows ProductVersion resource (reported as 0.0.0.0).
$compilerFiles = @(Get-ChildItem -LiteralPath $compiler -File -Recurse | Sort-Object FullName | ForEach-Object {
    @{path=$_.FullName.Substring($compiler.Length+1).Replace('\','/');sha256=(Hash (Ordinary-File $_.FullName))}
})
$log = Join-Path $out 'setup-build.log'
$arguments = @('/DVcpLocalCandidate=1','/Q',('/O' + $out),('/DNativeArchive=' + $archive),('/DSetupFiles=' + $setupFiles),('/DMaxAppRootLength=' + $pathLimits.max_app_root_utf16),('/DProductVersion=' + $channel.native_version),('/DNativeSha256=' + $native.archive_sha256),('/DCandidateId=' + $localCandidate.candidate_id),('/DShellSha256=' + (Hash (Join-Path $PSScriptRoot 'installer/shell.ps1'))),('/DEngineScriptSha256=' + (Hash (Join-Path $PSScriptRoot 'package-install.ps1'))),('/DNoticesSha256=' + $notices.inventory_sha256),(Join-Path $PSScriptRoot 'installer/vcp.iss'))
$setupSigning = @{status='unsigned'}
& $iscc @arguments *> $log
if ($LASTEXITCODE -ne 0) { throw "Setup compilation failed; retained $log" }
$setup = Ordinary-File (Join-Path $out ('vcp-' + $channel.native_version + '-windows-x64-unsigned-setup.exe'))
$productVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($setup).ProductVersion
Assert-SetupProductVersion $productVersion $channel.native_version
if ((Get-AuthenticodeSignature -LiteralPath $setup).Status -ne 'NotSigned') { throw 'Local setup must be explicitly unsigned' }
# Revalidate after compilation; an input change invalidates the assembled bytes.
$afterBuild = & $node $localTool verify $repository $BuildReceipt $Launcher
if ($LASTEXITCODE -ne 0 -or ($afterBuild -join "`n") -cne ($verifiedJson -join "`n") -or (Hash $archive) -cne $native.archive_sha256 -or (Hash $Launcher) -cne $receipt.launcher_sha256 -or (Hash $CompilerInstaller) -cne $channel.installer.sha256) { throw 'Setup inputs changed during compilation' }
foreach ($file in $compilerFiles) { if ((Hash (Join-Path $compiler $file.path)) -cne $file.sha256) { throw 'Compiler changed during setup construction' } }
& $node (Join-Path $PSScriptRoot 'installer/notices.cjs') $stagedNotices (Join-Path $repository 'release/internal-beta.json') $compiler | Out-Null
if ($LASTEXITCODE -ne 0 -or (Hash (Join-Path $noticesRoot 'inventory.json')) -cne $notices.inventory_sha256) { throw 'Runtime notice copies changed during setup construction' }
$afterPaths = & $node (Join-Path $PSScriptRoot 'installer/path-limits.cjs') $setupFiles
if ($LASTEXITCODE -ne 0 -or ($afterPaths -join "`n") -cne ($pathLimitsJson -join "`n")) { throw 'Staged setup files changed during compilation' }
$result = [ordered]@{
    schema='vcp-setup-result/1'; status='unsigned-local-candidate'; candidate_id=$localCandidate.candidate_id
    native_archive_sha256=$native.archive_sha256; build_receipt_sha256=(Hash $BuildReceipt); launcher_sha256=$packagedLauncherHash
    archive=@{file=[IO.Path]::GetFileName($setup);sha256=(Hash $setup)}
    compiler=@{name='Inno Setup';version=$channel.installer.version;source_commit=$channel.installer.source_commit;installer_sha256=$channel.installer.sha256;files=$compilerFiles}
    source=@{commit=$localCandidate.source_commit;dirty=$localCandidate.source_dirty;content_sha256=$localCandidate.source_content_sha256}
    local_candidate=$localCandidate; native_result='native-result.json'
    log_sha256=(Hash $log); signing=$setupSigning
    notices=$notices; path_limits=$pathLimits
    limitations=@('Unsigned local candidate from the current checkout; no CI or release qualification is asserted.','Setup compilation is not installed-product qualification.','PowerShell 7 is an explicit prerequisite; no tools or models are downloaded.','Uninstall preserves the separately chosen data directory.','Outer setup lock covers registered integration; inner package lock covers engine lifecycle. Direct expert engine operations do not alter registered integration.')
}
$resultPath = Join-Path $out 'setup-result.json'
$result | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $resultPath -Encoding utf8NoBOM
Write-Output $resultPath
} finally { $buildLock.Dispose() }
