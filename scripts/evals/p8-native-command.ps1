# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)][string]$CommandFile,
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$RustToolchain = '1.98.0'
)

$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))

function Fail([string]$Message, [int]$Code = 2) {
    [Console]::Error.WriteLine($Message)
    exit $Code
}

if (-not $IsWindows) { Fail 'Native Windows is required.' 3 }
$resolvedCommand = Resolve-Path -LiteralPath $CommandFile -ErrorAction Stop
$commandPath = [IO.Path]::GetFullPath($resolvedCommand.Path)
$commandInfo = Get-Item -LiteralPath $commandPath -ErrorAction Stop
if (-not $commandInfo.PSIsContainer -and $commandInfo.Length -le 1024 * 1024) {
    $spec = Get-Content -LiteralPath $commandPath -Raw | ConvertFrom-Json
} else {
    Fail 'Command file must be a bounded regular file.'
}
if ($spec.schema -ne 'p8-command/1') { Fail 'Unsupported P8 command schema.' }
if ($spec.program -ne 'cargo') { Fail 'P8 native wrapper accepts only the exact cargo command.' }
if ($null -eq $spec.cwd -or $spec.cwd -isnot [string]) { Fail 'P8 command cwd is required.' }
$rawArgs = @($spec.args)
if ($rawArgs.Count -eq 0) { Fail 'P8 command arguments are required.' }
$cwd = [IO.Path]::GetFullPath($spec.cwd)
$rootPrefix = $repository.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if (-not ($cwd.Equals($repository, [StringComparison]::OrdinalIgnoreCase) -or $cwd.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase))) {
    Fail 'P8 command cwd escapes the repository.'
}
if (-not (Test-Path -LiteralPath $cwd -PathType Container)) { Fail 'P8 command cwd does not exist.' }
$commandArgs = @($rawArgs | ForEach-Object {
    if ($_ -isnot [string] -or $_.Length -eq 0 -or $_.Contains([char]0)) { Fail 'P8 command arguments must be non-empty strings.' }
    $_
})
if ($spec.environment) {
    foreach ($property in $spec.environment.PSObject.Properties) {
        if ($property.Name -notin @('VCP_TEST_GIT', 'VCP_TEST_SKILL_PACKAGE')) { Fail "Unsupported P8 command environment key: $($property.Name)" }
        if ($null -ne $property.Value) {
            if ($property.Value -isnot [string] -or $property.Value.Contains([char]0)) { Fail "Invalid P8 command environment value: $($property.Name)" }
            [Environment]::SetEnvironmentVariable($property.Name, $property.Value, 'Process')
        }
    }
}

$rustup = Get-Command rustup -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $rustup) { Fail 'rustup is required.' 3 }
$installed = & $rustup.Source toolchain list
if ($LASTEXITCODE -ne 0 -or -not ($installed | Where-Object { $_ -match ('^' + [regex]::Escape($RustToolchain) + '(?:-|\s|$)') })) {
    Fail "Rust $RustToolchain is required." 3
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { Fail 'Visual Studio discovery tool is missing.' 3 }
$vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsRoot) { Fail 'Native Visual C++ x64 tools are missing.' 3 }
$env:PATH = (Split-Path -Parent $vswhere) + ';' + $env:PATH
& (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin', 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) {
    $env:PATH = (Join-Path $vsRoot $relative) + ';' + $env:PATH
}
foreach ($tool in @('cl', 'cmake', 'ninja')) {
    if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) { Fail "Missing native prerequisite: $tool" 3 }
}
$node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $node) { Fail 'Node is required by the native qualification fixtures.' 3 }
$git = Get-Command git -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $git) { Fail 'Git is required by the native qualification fixtures.' 3 }
$env:VCP_TEST_NODE = $node.Source
if (-not $env:VCP_TEST_GIT) { $env:VCP_TEST_GIT = $git.Source }
$env:VCP_TEST_CARGO = & $rustup.Source which --toolchain $RustToolchain cargo
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $env:VCP_TEST_CARGO)) { Fail 'Rust Cargo executable is unavailable.' 3 }
$env:VCP_TEST_COMPILER_PATH = (Split-Path -Parent $env:VCP_TEST_CARGO) + ';' + $env:PATH
foreach ($setting in @('LIB','INCLUDE','LIBPATH')) {
    [Environment]::SetEnvironmentVariable("VCP_TEST_COMPILER_$setting", [Environment]::GetEnvironmentVariable($setting), 'Process')
}
$env:RUST_MIN_STACK = '16777216'
$env:CODEX_TEST_ENVIRONMENT = 'local'
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = Join-Path $repository 'artifacts/p8-native-target'
}

Push-Location -LiteralPath $cwd
try {
    & $rustup.Source run $RustToolchain cargo @commandArgs
    $exitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
exit $exitCode
