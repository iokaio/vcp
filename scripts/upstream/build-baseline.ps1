# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourceRoot,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$Commit,
    [Parameter(Mandatory)][string]$OutputRoot,
    [string]$TargetRoot,
    [ValidateSet('Build', 'BoundaryTests')][string]$Mode = 'Build',
    [ValidateRange(1, 16)][int]$Jobs = 4
)
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $SourceRoot).Path
$output = [IO.Path]::GetFullPath($OutputRoot)
$target = $(if ($TargetRoot) { [IO.Path]::GetFullPath($TargetRoot) } else { Join-Path $output 'target' })
if ($output -eq $source -or $output.StartsWith($source.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Evidence and target output must be outside the unmodified source checkout.'
}
if ($target -eq $source -or $target.StartsWith($source.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Target output must be outside the unmodified source checkout.'
}
$runDirectory = Join-Path $output ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
$record = [ordered]@{
    schema_version = 1
    task_id = 'P0-07'
    candidate_commit = $Commit
    mode = $Mode
    status = 'prepared'
    started_at = [DateTime]::UtcNow.ToString('o')
    ended_at = $null
    command = @()
    exit_code = $null
    limitations = @('Unmodified upstream experiment; no VCP integration or release qualification.')
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
    $gitArguments = @('-c', ('safe.directory=' + $source.Replace('\', '/')), '-C', $source)
    $actual = & git @gitArguments rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $actual -ne $Commit) { throw 'Source does not match the requested commit.' }
    $dirty = & git @gitArguments status --porcelain=v1 --untracked-files=all
    if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Unmodified baseline requires a clean source checkout.' }
    $toolchainFile = Join-Path $source 'codex-rs/rust-toolchain.toml'
    $toolchainText = Get-Content -LiteralPath $toolchainFile -Raw
    if ($toolchainText -notmatch '(?m)^channel\s*=\s*"(\d+\.\d+\.\d+)"\s*$') { throw 'Expected an immutable stable Rust toolchain.' }
    $toolchain = $Matches[1]
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
    if ($Mode -eq 'Build') {
        $cargoArguments = @("+$toolchain", 'build', '--locked', '-p', 'codex-cli', '--bin', 'codex', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
    } else {
        $cargoArguments = @("+$toolchain", 'test', '--locked', '-p', 'codex-apply-patch', '-p', 'codex-execpolicy', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
    }
    $record.command = @('cargo') + $cargoArguments
    $record.status = 'running'; Save-Record
    Write-Host ($record.command -join ' ')
    Push-Location -LiteralPath (Join-Path $source 'codex-rs')
    try { & cargo @cargoArguments *> (Join-Path $runDirectory 'command.log'); $code = $LASTEXITCODE }
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
