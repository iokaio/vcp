# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [string]$TargetRoot, [ValidateRange(1,16)][int]$Jobs = 4)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $IsWindows) { Write-Output '{"status":"not_run","reason":"Native Windows required"}'; exit 3 }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/hooks' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/upstream/codex-target' }
$directory = Join-Path $OutputRoot ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$manifest = Join-Path $directory 'manifest.json'
$record = [ordered]@{ schema_version = 1; task_id = 'P10-01'; status = 'running'; started_at = [DateTime]::UtcNow.ToString('o'); stages = @() }
function Save-Record { $record | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Stage([string]$Name, [string[]]$Arguments, [int]$Expected) {
    $log = Join-Path $directory ($Name + '.log')
    Write-Host "Hook qualification: $Name"
    & cargo @Arguments *> $log
    $code = $LASTEXITCODE
    $passed = [regex]::Matches((Get-Content -LiteralPath $log -Raw), '(?m)^test [^\r\n]+ \.\.\. ok\r?$').Count
    $record.stages += @{ name = $Name; command = @('cargo') + $Arguments; exit_code = $code; passed = $passed; minimum_passed = $Expected; log = $log; sha256 = (Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant() }
    Save-Record
    if ($code -ne 0) { throw "$Name failed; see $log" }
    if ($passed -lt $Expected) { throw "$Name did not run its required assertions: expected at least $Expected, observed $passed" }
}
Save-Record
try {
    $record.revision = & git -c "safe.directory=$($repository.Replace('\','/'))" -C $repository rev-parse HEAD
    $inputs = @('scripts/test-hooks.ps1', 'src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/core/src/client.rs', 'src/third_party/codex/codex-rs/ext/extension-api/src/work_admission.rs')
    foreach ($crate in @('vcp-extensions', 'vcp-lifecycle', 'vcp-cli')) {
        $inputs += Get-ChildItem -LiteralPath (Join-Path $repository "src/crates/$crate") -Recurse -File | Where-Object { $_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml' } | ForEach-Object { [IO.Path]::GetRelativePath($repository, $_.FullName).Replace('\','/') }
    }
    $record.inputs = @($inputs | Sort-Object -Unique | ForEach-Object { @{ path = $_; sha256 = (Get-FileHash -LiteralPath (Join-Path $repository $_)).Hash.ToLowerInvariant() } })
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsRoot) { throw 'Native Visual C++ x64 tools missing' }
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    $env:CARGO_TARGET_DIR = [IO.Path]::GetFullPath($TargetRoot)
    $env:RUST_MIN_STACK = '16777216'
    $env:CODEX_TEST_ENVIRONMENT = 'local'
    $env:VCP_TEST_NODE = (Get-Command node -CommandType Application).Source
    $env:VCP_TEST_GIT = (Get-Command git -CommandType Application).Source
    $record.platform = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    $record.rustc = & rustc '+1.98.0' --version
    $record.durability_scope = 'Forced native owner-process termination; not hardware power-loss qualification'
    Push-Location (Join-Path $repository 'src/third_party/codex/codex-rs')
    try {
        $base = @('+1.98.0', 'test', '--locked', '--offline', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs")
        Stage 'pure-hooks' ($base + @('-p', 'vcp-extensions', '--test', 'hooks')) 7
        Stage 'adapter-contracts' ($base + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--lib', 'hooks::')) 2
        Stage 'wrapper-contracts' ($base + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--lib', 'coding::mcp_content_tests::')) 3
        Stage 'native-hooks' ($base + @('-p', 'vcp-lifecycle', '--features', 'qualification', '--test', 'canonical_host', 'hooks::', '--', '--test-threads=1')) 17
        Stage 'cli-contracts' ($base + @('-p', 'vcp-cli', '--features', 'qualification', '--lib', '--', '--test-threads=1')) 1
        Stage 'delegation-adapter-build' @('+1.98.0', 'build', '--locked', '--offline', '--target', 'x86_64-pc-windows-msvc', '-j', "$Jobs", '-p', 'vcp-cli', '--features', 'qualification', '--example', 'delegation-live-adapter') 0
        Stage 'cli-executable' ($base + @('-p', 'vcp-cli', '--features', 'qualification', '--test', 'executable', '--', '--test-threads=1')) 1
        Stage 'cli-terminal' ($base + @('-p', 'vcp-cli', '--features', 'qualification', '--test', 'terminal_console', '--', '--test-threads=1')) 1
        Stage 'local-lifecycle' ($base + @('-p', 'vcp-cli', '--features', 'qualification', '--test', 'local_start', '--test', 'local_pending_input', '--test', 'local_execution_parity', '--', '--test-threads=1')) 3
    } finally { Pop-Location }
    foreach ($inputRecord in $record.inputs) {
        if ((Get-FileHash -LiteralPath (Join-Path $repository $inputRecord.path)).Hash.ToLowerInvariant() -ne $inputRecord.sha256) {
            throw "qualification input changed during execution: $($inputRecord.path)"
        }
    }
    $record.status = 'pass'
    $record.exit_code = 0
} catch {
    $record.status = 'fail'
    $record.exit_code = 1
    $record.reason = $_.Exception.Message
}
$record.ended_at = [DateTime]::UtcNow.ToString('o')
Save-Record
[pscustomobject]$record | Select-Object status,exit_code,reason,@{n='manifest';e={$manifest}} | ConvertTo-Json -Compress
exit $record.exit_code
