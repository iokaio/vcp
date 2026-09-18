# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$AssetsRoot,
    [string]$OutputRoot,
    [string]$TargetRoot,
    [ValidateRange(1, 16)][int]$Jobs = 4,
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/embeddings' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/embedding-target' }
if (-not $IsWindows) { Write-Output '{"status":"not_run","reason":"Native Windows is required"}'; exit 3 }
$node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $node) { Write-Output '{"status":"not_run","reason":"Node 24 or later is required"}'; exit 3 }
if (-not $AssetsRoot) { Write-Output '{"status":"not_run","reason":"Supply the explicitly acquired local model directory with -AssetsRoot"}'; exit 3 }
# Resolve junction/short-path aliases before allocating build or evidence output.
$paths = & $node.Source -e "const m=require(process.argv[1]),p=require('node:path'),repo=process.argv[2],assets=m.outside(process.argv[3],[repo]); console.log(JSON.stringify({assets,output:m.outside(process.argv[4],[p.join(repo,'src'),assets]),target:m.outside(process.argv[5],[p.join(repo,'src'),assets])}));" (Join-Path $repository 'src/tests/support/model-assets.cjs') $repository $AssetsRoot $OutputRoot $TargetRoot
if ($LASTEXITCODE -ne 0) { exit 2 }
$paths = $paths | ConvertFrom-Json
$directory = Join-Path $paths.output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$record = [ordered]@{
    schema_version = 1; task_id = 'P0-07'; status = 'prepared'
    started_at = [DateTime]::UtcNow.ToString('o'); stages = @()
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    limitations = @('CPU embedding qualification; no VCP indexes, OS network-denial or production resource envelope.')
}
$manifest = Join-Path $directory 'manifest.json'
if ($Offline) {
    $record.task_id = 'P0-02'
    $record.limitations = @('CPU inference with observed Windows network isolation; no full index containment or production resource envelope.')
}
function Save-Record { $record | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Not-Run([string]$Reason) { $record.status = 'not_run'; $record.reason = $Reason; $record.exit_code = 3; Save-Record; Write-Output $Reason; exit 3 }
function Stage([string]$Name, [string]$Executable, [string[]]$Arguments) {
    $log = Join-Path $directory "$Name.log"
    Write-Host "Embedding qualification stage: $Name"
    & $Executable @Arguments *> $log
    $code = $LASTEXITCODE
    $record.stages += @{ name = $Name; command = @($Executable) + $Arguments; exit_code = $code; log = "$Name.log"; sha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
    Save-Record
    if ($code -ne 0) { $record.exit_code = $code; if ($code -eq 3) { $record.status = 'not_run' }; throw "$Name failed with exit $code" }
}
Save-Record
try {
    $record.node = & $node.Source --version
    if ($LASTEXITCODE -ne 0 -or $record.node -notmatch '^v(\d+)\.' -or [int]$Matches[1] -lt 24) { Not-Run 'Node 24 or later is required' }
    foreach ($tool in @('git', 'rustup', 'cargo')) {
        if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) { Not-Run "Missing prerequisite: $tool" }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $repository 'src/tests/node_modules/@iarna/toml/package.json'))) { Not-Run 'Install the pinned VCP development tools first' }
    Stage 'assets' $node.Source @((Join-Path $repository 'scripts/upstream/model-assets.cjs'), 'verify', '--root', $paths.assets)
    foreach ($component in @('codex', 'munarium')) {
        Stage "source-$component" $node.Source @((Join-Path $repository 'scripts/upstream/reconstruct.cjs'), 'verify', '--component', $component)
    }
    $installed = & rustup toolchain list
    if ($LASTEXITCODE -ne 0 -or -not ($installed | Where-Object { $_ -match '^1\.98\.0-x86_64-pc-windows-msvc(?:\s|$)' })) { Not-Run 'Install Rust 1.98.0 for native Windows' }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { Not-Run 'Visual Studio discovery tool is missing' }
    $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsRoot) { Not-Run 'Visual C++ x64 build tools are missing' }
    $env:PATH = (Split-Path -Parent $vswhere) + ';' + $env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    $record.msvc = $env:VCToolsVersion
    $record.rustc = & rustc +1.98.0 --version
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust compiler' }
    $record.vcp_commit = & git -C $repository rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify VCP commit' }
    $record.vcp_dirty = [bool](& git -C $repository status --porcelain=v1 --untracked-files=all)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify VCP working tree' }
    $workspace = Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml'
    $record.lock_sha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $workspace) 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.asset_spec_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'src/third_party/components/minilm-assets.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.inputs = @('src/crates/vcp-embedding/Cargo.toml', 'src/crates/vcp-embedding/src/lib.rs', 'src/crates/vcp-embedding/src/bin/qualify.rs', 'src/crates/vcp-embedding/src/bin/qualify/network.rs', 'src/tests/fixtures/local-embeddings/minilm-golden.json', 'src/tests/support/model-assets.cjs', 'src/tests/support/dependency-closure.cjs') | ForEach-Object {
        @{ path = $_; sha256 = (Get-FileHash -LiteralPath (Join-Path $repository $_) -Algorithm SHA256).Hash.ToLowerInvariant() }
    }
    $record.platform = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    $record.cpu_count = [Environment]::ProcessorCount
    $record.status = 'running'; Save-Record
    $common = @('--locked', '--manifest-path', $workspace, '-p', 'vcp-embedding', '--release', '--target', 'x86_64-pc-windows-msvc', '--target-dir', $paths.target, '-j', "$Jobs")
    Stage 'unit-tests' 'cargo' (@('+1.98.0', 'test', '--lib') + $common)
    Stage 'build' 'cargo' (@('+1.98.0', 'build', '--bin', 'qualify') + $common)
    Stage 'dependencies' 'cargo' @('+1.98.0', 'tree', '--locked', '--offline', '--manifest-path', $workspace, '-p', 'vcp-embedding', '--target', 'x86_64-pc-windows-msvc', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}')
    Stage 'dependency-check' $node.Source @((Join-Path $repository 'scripts/upstream/embedding-dependencies.cjs'), (Join-Path $directory 'dependencies.log'))
    $record.binary_sha256 = (Get-FileHash -LiteralPath (Join-Path $paths.target 'x86_64-pc-windows-msvc/release/qualify.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    Stage 'qualification' (Join-Path $paths.target 'x86_64-pc-windows-msvc/release/qualify.exe') @($paths.assets)
    $result = Get-Content -LiteralPath (Join-Path $directory 'qualification.log') -Raw | ConvertFrom-Json
    if ($result.status -ne 'pass' -or $result.cases -ne 4 -or $result.checks -ne 4 -or $result.asset_spec_sha256 -ne $record.asset_spec_sha256) { throw 'Incomplete embedding qualification result' }
    $record.result = $result
    if ($Offline) {
        Stage 'offline' $node.Source @((Join-Path $repository 'scripts/upstream/trace-offline-embeddings.cjs'), '--binary', (Join-Path $paths.target 'x86_64-pc-windows-msvc/release/qualify.exe'), '--assets', $paths.assets, '--output-root', (Join-Path $directory 'offline'))
    }
    $record.status = 'pass'; $record.exit_code = 0
} catch {
    if ($record.status -ne 'not_run') { $record.status = 'fail' }
    $record.reason = $_.Exception.Message
    if (-not $record.Contains('exit_code')) { $record.exit_code = 1 }
} finally {
    $record.ended_at = [DateTime]::UtcNow.ToString('o'); Save-Record
}
Write-Output ($record | ConvertTo-Json -Depth 10)
exit $record.exit_code
