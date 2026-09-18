# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding(DefaultParameterSetName = 'Baseline')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Baseline')][string]$SourceRoot,
    [Parameter(Mandatory, ParameterSetName = 'Baseline')][ValidatePattern('^[a-f0-9]{40}$')][string]$Commit,
    [Parameter(ParameterSetName = 'Baseline')][ValidateSet('Codex', 'Munarium')][string]$Candidate = 'Codex',
    [Parameter(Mandatory, ParameterSetName = 'Selected')][switch]$SelectedCodex,
    [Parameter(Mandatory, ParameterSetName = 'SelectedMunarium')][switch]$SelectedMunarium,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string]$TargetRoot,
    [ValidateSet('Build', 'BoundaryTests')][string]$Mode = 'Build',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$ExperimentToolchain,
    [ValidateRange(1, 16)][int]$Jobs = 4
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if ($SelectedCodex) {
    $SourceRoot = Join-Path $repository 'src/third_party/codex'
    $selection = Get-Content -LiteralPath (Join-Path $repository 'src/third_party/components/codex-selection.json') -Raw | ConvertFrom-Json
    $Commit = $selection.commit
}
if ($SelectedMunarium) {
    $Candidate = 'Munarium'
    $SourceRoot = Join-Path $repository 'src/third_party/munarium'
    $selection = Get-Content -LiteralPath (Join-Path $repository 'src/third_party/components/munarium-selection.json') -Raw | ConvertFrom-Json
    $Commit = $selection.commit
}
# Use the same normalization for all three paths: Resolve-Path alone retains
# Windows 8.3 aliases while GetFullPath expands them on the native runtime.
$source = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $SourceRoot).Path)
$output = [IO.Path]::GetFullPath($OutputRoot)
$target = $(if ($TargetRoot) { [IO.Path]::GetFullPath($TargetRoot) } else { Join-Path $output 'target' })
$protectedSources = @($source)
if ($SelectedCodex -or $SelectedMunarium) {
    $protectedSources = @('codex', 'munarium') | ForEach-Object {
        [IO.Path]::GetFullPath((Resolve-Path -LiteralPath (Join-Path $repository "src/third_party/$_")).Path)
    }
}
foreach ($protectedSource in $protectedSources) {
    foreach ($destination in @($output, $target)) {
        if ($destination -eq $protectedSource -or $destination.StartsWith($protectedSource.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            [Console]::Error.WriteLine('[BASELINE_OUTPUT_IN_SOURCE] Evidence and target output must be outside every selected source checkout.')
            exit 2
        }
    }
}
$runDirectory = Join-Path $output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
$record = [ordered]@{
    schema_version = 1
    task_id = 'P0-07'
    candidate_commit = $Commit
    candidate = $Candidate.ToLowerInvariant()
    mode = $Mode
    status = 'prepared'
    started_at = [DateTime]::UtcNow.ToString('o')
    ended_at = $null
    command = @()
    exit_code = $null
    limitations = @("$Candidate baseline qualification only; no VCP integration or release qualification.")
    source_kind = $(if ($SelectedCodex -or $SelectedMunarium) { 'committed-selection' } else { 'unmodified-upstream' })
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
}
$manifest = Join-Path $runDirectory 'manifest.json'
function Save-Record { $record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Not-Run([string]$Reason) {
    $record.status = 'not_run'; $record.reason = $Reason; $record.exit_code = 3
    $record.ended_at = [DateTime]::UtcNow.ToString('o'); Save-Record
    Write-Host $Reason; exit 3
}
Save-Record
try {
    if (-not $IsWindows) { Not-Run 'Native Windows is required.' }
    foreach ($tool in @('git', 'rustup', 'cargo')) {
        if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) { Not-Run "Missing prerequisite: $tool" }
    }
    if ($Candidate -eq 'Munarium') {
        $node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $node) { Not-Run 'Node 24 is required for the dependency-closure check.' }
        $nodeVersion = & $node.Source --version
        if ($LASTEXITCODE -ne 0 -or $nodeVersion -notmatch '^v(\d+)\.' -or [int]$Matches[1] -lt 24) { Not-Run 'Node 24 or later is required.' }
        $record.node = $nodeVersion
        $record.dependency_checker_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'scripts/upstream/dependency-closure.cjs') -Algorithm SHA256).Hash.ToLowerInvariant()
        $record.dependency_policy_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'src/tests/support/dependency-closure.cjs') -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    if ($SelectedCodex -or $SelectedMunarium) {
        $node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $node -or -not (Test-Path -LiteralPath (Join-Path $repository 'src/tests/node_modules/@iarna/toml/package.json'))) { Not-Run 'Install Node 24 and run npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund.' }
        foreach ($selectedComponent in @('codex', 'munarium')) {
            & $node.Source (Join-Path $repository 'scripts/upstream/reconstruct.cjs') verify --component $selectedComponent
            if ($LASTEXITCODE -ne 0) { throw "Selected source verification failed: $selectedComponent" }
        }
        $record.vcp_commit = (& git -C $repository rev-parse HEAD)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot identify VCP source.' }
        $record.vcp_dirty = [bool](& git -C $repository status --porcelain=v1 --untracked-files=all)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot identify VCP changes.' }
        $selectedId = $Candidate.ToLowerInvariant()
        $record.selection_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository "src/third_party/components/$selectedId-selection.json") -Algorithm SHA256).Hash.ToLowerInvariant()
        $record.inventory_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository "src/third_party/components/$selectedId-files.json") -Algorithm SHA256).Hash.ToLowerInvariant()
        $record.workspace_inventory_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'src/third_party/components/codex-files.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    } else {
        $gitArguments = @('-c', ('safe.directory=' + $source.Replace('\', '/')), '-C', $source)
        $actual = & git @gitArguments rev-parse HEAD
        if ($LASTEXITCODE -ne 0 -or $actual -ne $Commit) { throw 'Source does not match the requested commit.' }
        $dirty = & git @gitArguments status --porcelain=v1 --untracked-files=all
        if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Unmodified baseline requires a clean source checkout.' }
    }
    $workspace = if ($SelectedMunarium) { Join-Path $repository 'src/third_party/codex/codex-rs' } else { Join-Path $source $(if ($Candidate -eq 'Codex') { 'codex-rs' } else { 'server' }) }
    $toolchainFile = if ($SelectedMunarium) { Join-Path $source 'server/rust-toolchain.toml' } else { Join-Path $workspace 'rust-toolchain.toml' }
    $toolchainText = Get-Content -LiteralPath $toolchainFile -Raw
    if ($toolchainText -notmatch '(?m)^channel\s*=\s*"(\d+\.\d+\.\d+)"\s*$') { throw 'Expected an immutable stable Rust toolchain.' }
    $upstreamToolchain = $Matches[1]
    $toolchain = $(if ($ExperimentToolchain) { $ExperimentToolchain } else { $upstreamToolchain })
    $record.upstream_toolchain = $upstreamToolchain
    $record.experiment_toolchain = $(if ($ExperimentToolchain) { $ExperimentToolchain } else { $null })
    if ($ExperimentToolchain) {
        $record.limitations += 'Explicit compiler experiment; this does not change or qualify the upstream toolchain pin.'
    }
    $installed = & rustup toolchain list
    if ($LASTEXITCODE -ne 0 -or -not ($installed | Where-Object { $_ -match ('^' + [regex]::Escape($toolchain) + '-x86_64-pc-windows-msvc(?:\s|$)') })) {
        Not-Run "Install Rust $toolchain for x86_64-pc-windows-msvc before running this experiment."
    }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { Not-Run 'Visual Studio installer discovery tool is missing.' }
    $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsRoot) { Not-Run 'Visual C++ x64 build tools are missing.' }
    $env:PATH = (Split-Path -Parent $vswhere) + ';' + $env:PATH
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin', 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) {
        $env:PATH = (Join-Path $vsRoot $relative) + ';' + $env:PATH
    }
    foreach ($tool in @('cl', 'cmake', 'ninja')) {
        if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) { Not-Run "Missing native prerequisite: $tool" }
    }
    $record.rustc = (& rustc "+$toolchain" --version)
    $record.cargo = (& cargo "+$toolchain" --version)
    $record.cmake = (& cmake --version | Select-Object -First 1)
    $record.ninja = (& ninja --version)
    $record.msvc = $env:VCToolsVersion
    $record.cpu_count = [Environment]::ProcessorCount
    $record.total_memory_bytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    $record.platform = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    $env:CARGO_TARGET_DIR = $target
    if ($Candidate -eq 'Munarium') {
        $verb = $(if ($Mode -eq 'Build') { 'build' } else { 'test' })
        $cargoArguments = @("+$toolchain", $verb, '--locked', '-p', 'munarium-core', '-p', 'munarium-store-mem', '-p', 'munarium-datastore', '--features', 'munarium-datastore/vector-diskann', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
    } elseif ($Mode -eq 'Build') {
        $cargoArguments = @("+$toolchain", 'build', '--locked', '-p', 'codex-cli', '--bin', 'codex', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
    } else {
        $cargoArguments = @("+$toolchain", 'test', '--locked', '-p', 'codex-apply-patch', '-p', 'codex-execpolicy', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
    }
    $record.command = @('cargo') + $cargoArguments
    $record.status = 'running'; Save-Record
    Write-Host ($record.command -join ' ')
    Push-Location -LiteralPath $workspace
    try {
        & cargo @cargoArguments *> (Join-Path $runDirectory 'command.log'); $code = $LASTEXITCODE
        if ($Candidate -eq 'Munarium' -and $code -eq 0) {
            $treeArguments = @("+$toolchain", 'tree', '--locked', '--offline', '-p', 'munarium-core', '-p', 'munarium-store-mem', '-p', 'munarium-datastore', '--features', 'munarium-datastore/vector-diskann', '--target', 'x86_64-pc-windows-msvc', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}')
            $record.dependency_command = @('cargo') + $treeArguments
            $treeLog = Join-Path $runDirectory 'dependencies.log'
            & cargo @treeArguments > $treeLog 2> (Join-Path $runDirectory 'dependencies.stderr.log')
            if ($LASTEXITCODE -ne 0) { throw 'Cannot capture the native dependency closure.' }
            $closure = Join-Path $runDirectory 'dependencies.json'
            $closureArguments = @($treeLog, $closure)
            if ($SelectedMunarium) {
                $reference = Join-Path $repository 'src/third_party/components/munarium-dependencies.json'
                $record.dependency_reference_sha256 = (Get-FileHash -LiteralPath $reference -Algorithm SHA256).Hash.ToLowerInvariant()
                $closureArguments += $reference
            }
            & $node.Source (Join-Path $repository 'scripts/upstream/dependency-closure.cjs') @closureArguments
            if ($LASTEXITCODE -ne 0) { throw 'Munarium dependency boundary failed.' }
            $record.dependency_log_sha256 = (Get-FileHash -LiteralPath $treeLog -Algorithm SHA256).Hash.ToLowerInvariant()
            $record.dependency_record_sha256 = (Get-FileHash -LiteralPath $closure -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    finally { Pop-Location }
    $record.exit_code = $code
    $record.status = $(if ($code -eq 0) { 'pass' } else { 'fail' })
    $record.log_sha256 = (Get-FileHash -LiteralPath (Join-Path $runDirectory 'command.log') -Algorithm SHA256).Hash.ToLowerInvariant()
    $record.ended_at = [DateTime]::UtcNow.ToString('o'); Save-Record
    Write-Host "Evidence: $manifest"
    exit $code
} catch {
    $record.status = 'fail'; $record.reason = $_.Exception.Message; $record.exit_code = 1
    $record.ended_at = [DateTime]::UtcNow.ToString('o'); Save-Record
    Write-Error $_ -ErrorAction Continue
    exit 1
}
