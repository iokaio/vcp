# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$NativeResult,
    [Parameter(Mandatory)][string]$BuildReceipt,
    [Parameter(Mandatory)][string]$Launcher,
    [Parameter(Mandatory)][string]$CompilerInstaller,
    [Parameter(Mandatory)][string]$ReviewedCommit,
    [string]$OutputRoot,
    [string]$SigningToolsManifest = $env:VCP_SIGNING_TOOLS_MANIFEST
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows setup construction required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$provenance = Join-Path $PSScriptRoot 'release/provenance.cjs'
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
$NativeResult = Ordinary-File $NativeResult
$BuildReceipt = Ordinary-File $BuildReceipt
$Launcher = Ordinary-File $Launcher
$CompilerInstaller = Ordinary-File $CompilerInstaller
$native = Get-Content -LiteralPath $NativeResult -Raw | ConvertFrom-Json -Depth 100
$receipt = Get-Content -LiteralPath $BuildReceipt -Raw | ConvertFrom-Json -Depth 100
$channel = Get-Content -LiteralPath (Join-Path $repository 'release/internal-beta.json') -Raw | ConvertFrom-Json
if ($native.schema -cne 'vcp-distribution-result/1' -or $native.status -cne 'release-candidate' -or
    $native.manifest.release.reviewed_commit -cne $ReviewedCommit -or
    $native.manifest.source.dirty -ne $false -or $native.manifest.build.status -cne 'verified-release-build') { throw 'Strict native release candidate required' }
if ($native.package -cnotmatch '^[a-zA-Z0-9._-]+\.zip$') { throw 'Native result must name an adjacent ZIP' }
$archive = Ordinary-File (Join-Path (Split-Path -Parent $NativeResult) $native.package)
if ((Hash $archive) -cne $native.archive_sha256 -or (Hash $BuildReceipt) -cne $native.manifest.build.receipt_sha256) { throw 'Native archive or receipt identity changed' }
if ($channel.installer.version -cne '6.7.3' -or (Hash $CompilerInstaller) -cne $channel.installer.sha256) { throw 'Release-pinned Inno Setup installer required' }
if ($receipt.launcher_sha256 -cne (Hash $Launcher) -or $receipt.launcher_compiler_artifact.target.name -cne 'vcp-launch' -or
    $receipt.launcher_compiler_artifact.profile.test -ne $false -or $receipt.launcher_compiler_artifact.profile.opt_level -cne '3' -or
    @($receipt.launcher_compiler_artifact.features).Count -ne 0) { throw 'Launcher must be the optimized production build-receipt artifact' }
# Validates exact source, version, flags, toolchain, feature inventory and retained
# compiler logs. The native result is not accepted as a substitute for its build.
$releaseJson = & $node $provenance verify-build $repository $BuildReceipt (Ordinary-File $receipt.executable) $ReviewedCommit
if ($LASTEXITCODE -ne 0) { throw 'Strict build provenance rejected setup input' }
$release = $releaseJson | ConvertFrom-Json
if ($release.candidate_id -cne $native.manifest.release.candidate_id) { throw 'Native release candidate differs from build' }
$launcherValidation = & $node -e 'const p=require(process.argv[1]);p.peArchitecture(process.argv[2]);const r=p.json(process.argv[3]);const fs=require("node:fs"),path=require("node:path");const rows=fs.readFileSync(path.join(path.dirname(process.argv[3]),"build.log"),"utf8").split(/\r?\n/).filter(x=>x.startsWith("{")).map(x=>JSON.parse(x));if(!rows.some(x=>JSON.stringify(x)===JSON.stringify(r.launcher_compiler_artifact)))throw Error("Launcher compiler artifact absent from build log");' $provenance $Launcher $BuildReceipt
if ($LASTEXITCODE -ne 0) { throw 'Launcher architecture or compiler evidence rejected' }
foreach ($relative in @('scripts/build-setup.ps1','scripts/installer/vcp.iss','scripts/installer/sign.ps1','scripts/installer/shell.ps1','scripts/installer/notices.cjs','scripts/installer/path-limits.cjs','scripts/package-install.ps1','release/internal-beta.json')) {
    $row = @($receipt.inputs | Where-Object path -CEQ $relative)
    if ($row.Count -ne 1 -or $row[0].sha256 -cne (Hash (Join-Path $repository $relative))) { throw "Setup source differs from reviewed inputs: $relative" }
}
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/beta-setup' }
$out = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
$staged = Join-Path $out 'native-verification'
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
& $node -e 'const p=require(process.argv[1]),path=require("node:path"),i=require(path.join(path.dirname(process.argv[1]),"../package-inventory.cjs"));const root=process.argv[2],n=p.json(process.argv[3]),r=p.json(process.argv[4]),m=p.json(path.join(root,"manifest.json"));if(JSON.stringify(m)!==JSON.stringify(n.manifest))throw Error("Archived manifest differs from native result");i.verifyManifest(root,m);if(p.fileHash(path.join(root,"build-receipt.json"))!==p.fileHash(process.argv[4]))throw Error("Archived build receipt differs");p.verifyPayloadSources(root,r,m.notices.inventory_sha256);' $provenance $staged $NativeResult $BuildReceipt
if ($LASTEXITCODE -ne 0) { throw 'Native archive contents failed independent build-input validation' }
$packagedLauncher = Ordinary-File (Join-Path $staged 'vcp-launch.exe')
$packagedLauncherHash = Hash $packagedLauncher
$launcherRow = @($native.manifest.files | Where-Object path -CEQ 'vcp-launch.exe')
if ($launcherRow.Count -ne 1 -or $launcherRow[0].sha256 -cne $packagedLauncherHash) { throw 'Packaged launcher identity mismatch' }
$compiler = Join-Path $out 'compiler'
$compilerLog = Join-Path $out 'compiler-provision.log'
# Portable extraction is explicit, local and pinned. No network operation or
# product prerequisite installation is part of this builder or the setup EXE.
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
$arguments = @('/Q',('/O' + $out),('/DNativeArchive=' + $archive),('/DSetupFiles=' + $setupFiles),('/DMaxAppRootLength=' + $pathLimits.max_app_root_utf16),('/DProductVersion=' + $channel.native_version),('/DNativeSha256=' + $native.archive_sha256),('/DCandidateId=' + $release.candidate_id),('/DShellSha256=' + (Hash (Join-Path $PSScriptRoot 'installer/shell.ps1'))),('/DEngineScriptSha256=' + (Hash (Join-Path $PSScriptRoot 'package-install.ps1'))),('/DNoticesSha256=' + $notices.inventory_sha256),(Join-Path $PSScriptRoot 'installer/vcp.iss'))
$setupSigning = @{status='unsigned'}
if ($channel.signing.status -ceq 'signed') {
    if (-not $SigningToolsManifest) { throw 'Signed setup requires pinned signing tools' }
    $SigningToolsManifest = Ordinary-File $SigningToolsManifest
    $uninstallerRoot = Join-Path $out 'signed-uninstallers'
    New-Item -ItemType Directory -Path $uninstallerRoot | Out-Null
    $contextPath = Join-Path $out 'signing-context.json'
    @{output_root=$out;uninstaller_root=$uninstallerRoot;tools_manifest=$SigningToolsManifest} | ConvertTo-Json | Set-Content -LiteralPath $contextPath -Encoding utf8NoBOM
    $pwsh = (Get-Process -Id $PID).Path
    # ISCC substitutes $q/$f itself; these are literal strings, never shell code.
    $signCommand = '$q' + $pwsh + '$q -NoProfile -NonInteractive -File $q' + (Join-Path $PSScriptRoot 'installer/sign.ps1') + '$q -Context $q' + $contextPath + '$q -File $f'
    $arguments = @('/DVcpSigned=1',('/DSignedUninstallerRoot=' + $uninstallerRoot),('/Svcp=' + $signCommand)) + $arguments
}
& $iscc @arguments *> $log
if ($LASTEXITCODE -ne 0) { throw "Setup compilation failed; retained $log" }
$setup = Ordinary-File (Join-Path $out ('vcp-' + $channel.native_version + '-windows-x64-' + $channel.signing.status + '-setup.exe'))
if ($channel.signing.status -ceq 'signed') {
    $setupSigning = & (Join-Path $PSScriptRoot 'release/record-signing.ps1') -Stage setup -BuildReceipt $BuildReceipt -Rows @((Join-Path $out 'signing/setup/row.json'),(Join-Path $out 'signing/uninstaller/row.json')) -ToolsManifest $SigningToolsManifest -OutputFile (Join-Path $out 'signing-receipt.json')
    if ((Hash $setup) -cne @($setupSigning.transformation.files | Where-Object role -CEQ 'setup')[0].output_sha256) { throw 'Setup changed after signing' }
    foreach ($role in @('setup','uninstaller')) {
        if ((Hash (Join-Path $out "signing/$role/signed.exe")) -cne @($setupSigning.transformation.files | Where-Object role -CEQ $role)[0].output_sha256) { throw 'Retained signed setup component changed' }
    }
}
# Revalidate after compilation; an input change invalidates the assembled bytes.
& $node $provenance verify-build $repository $BuildReceipt (Ordinary-File $receipt.executable) $ReviewedCommit | Out-Null
if ($LASTEXITCODE -ne 0 -or (Hash $archive) -cne $native.archive_sha256 -or (Hash $Launcher) -cne $receipt.launcher_sha256 -or (Hash $CompilerInstaller) -cne $channel.installer.sha256) { throw 'Setup inputs changed during compilation' }
foreach ($file in $compilerFiles) { if ((Hash (Join-Path $compiler $file.path)) -cne $file.sha256) { throw 'Compiler changed during setup construction' } }
& $node (Join-Path $PSScriptRoot 'installer/notices.cjs') $stagedNotices (Join-Path $repository 'release/internal-beta.json') $compiler | Out-Null
if ($LASTEXITCODE -ne 0 -or (Hash (Join-Path $noticesRoot 'inventory.json')) -cne $notices.inventory_sha256) { throw 'Runtime notice copies changed during setup construction' }
$afterPaths = & $node (Join-Path $PSScriptRoot 'installer/path-limits.cjs') $setupFiles
if ($LASTEXITCODE -ne 0 -or ($afterPaths -join "`n") -cne ($pathLimitsJson -join "`n")) { throw 'Staged setup files changed during compilation' }
$result = [ordered]@{
    schema='vcp-setup-result/1'; status='qualification-required'; candidate_id=$release.candidate_id
    native_archive_sha256=$native.archive_sha256; build_receipt_sha256=(Hash $BuildReceipt); launcher_sha256=$packagedLauncherHash
    archive=@{file=[IO.Path]::GetFileName($setup);sha256=(Hash $setup)}
    compiler=@{name='Inno Setup';version=$channel.installer.version;source_commit=$channel.installer.source_commit;installer_sha256=$channel.installer.sha256;files=$compilerFiles}
    source=@{reviewed_commit=$ReviewedCommit;content_sha256=$release.source_content_sha256}
    log_sha256=(Hash $log); signing=$setupSigning
    notices=$notices; path_limits=$pathLimits
    limitations=@('Setup compilation is not installed-product qualification.','PowerShell 7 is an explicit prerequisite; no tools or models are downloaded.','Uninstall preserves the separately chosen data directory.','Outer setup lock covers registered integration; inner package lock covers engine lifecycle. Direct expert engine operations do not alter registered integration.')
}
$resultPath = Join-Path $out 'setup-result.json'
$result | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $resultPath -Encoding utf8NoBOM
Write-Output $resultPath
