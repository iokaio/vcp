# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallationReport, [string]$CandidateRoot, [string]$OutputRoot, [string]$TargetRoot)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Actual native Windows required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $CandidateRoot) { $CandidateRoot = Join-Path $repository 'src/skills/candidates' }
$CandidateRoot = [IO.Path]::GetFullPath($CandidateRoot)
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$TargetRoot = [IO.Path]::GetFullPath($TargetRoot)
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/cs3-installed-skills' }
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory | Out-Null
$installation = Get-Content -LiteralPath $InstallationReport -Raw | ConvertFrom-Json
if ($installation.status -cne 'pass' -or $installation.stages.Count -ne 4) { throw 'Passed install/upgrade/rollback/re-upgrade evidence required' }
$binary = Join-Path $installation.installed_candidate 'vcp.exe'
if ((Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant() -cne $installation.executable_sha256) { throw 'Installed executable changed' }
$catalog = Join-Path $installation.installed_candidate 'skills/builtin/catalog.json'
$catalogHash = (Get-FileHash -LiteralPath $catalog).Hash.ToLowerInvariant()
if ($catalogHash -cne $installation.stages[-1].catalog_sha256) { throw 'Installed catalog changed' }
$record = [ordered]@{
    schema = 'cs3-installed-skills-qualification/1'; task = 'CS-3'; status = 'running'
    installation_report_sha256 = (Get-FileHash -LiteralPath $InstallationReport).Hash.ToLowerInvariant()
    executable_sha256 = $installation.executable_sha256
    catalog_sha256 = $catalogHash
    archive_sha256 = $installation.archive_sha256
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
    target_root = $TargetRoot
    stages = @(); paid_requests = 0
    limitations = @('Explicit candidate-source registration; this does not qualify candidate usefulness or promote a builtin skill.', 'Current-host native qualification build, not a production release or performance assessment.')
}
function Candidate-Identity {
    $files = @()
    foreach ($id in @('document-authoring','skill-authoring','frontend-design','mcp-development','llm-integration','webapp-testing')) {
        $root = Join-Path $CandidateRoot $id
        $descriptor = Get-Content -LiteralPath (Join-Path $root 'skill.json') -Raw | ConvertFrom-Json
        if ($descriptor.id -cne $id) { throw 'Candidate identity differs' }
        foreach ($relative in @('skill.json', $descriptor.body.path) + @($descriptor.resources | ForEach-Object path)) {
            if (-not $relative -or [IO.Path]::IsPathRooted($relative) -or $relative.Contains('\') -or $relative.Contains(':') -or $relative.Split('/') -contains '..') { throw 'Unsafe candidate member' }
            $files += [ordered]@{ skill = $id; version = $descriptor.version; path = $relative; sha256 = (Get-FileHash -LiteralPath (Join-Path $root $relative)).Hash.ToLowerInvariant() }
        }
    }
    return $files
}
$record.candidates_before = @(Candidate-Identity)
$record.test_source_sha256 = (Get-FileHash -LiteralPath (Join-Path $repository 'src/crates/vcp-cli/tests/executable.rs')).Hash.ToLowerInvariant()
$report = Join-Path $directory 'result.json'
function Save-Report { $record | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $report -Encoding utf8 }
Save-Report
$names = @('VCP_TEST_SKILL_PACKAGE','VCP_TEST_CANDIDATE_ROOT','VCP_TEST_NODE','VCP_TEST_GIT','RUST_MIN_STACK')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
    $env:VCP_TEST_SKILL_PACKAGE = $installation.installed_candidate
    $env:VCP_TEST_CANDIDATE_ROOT = $CandidateRoot
    $env:VCP_TEST_NODE = (Get-Command node -CommandType Application | Select-Object -First 1).Source
    $env:VCP_TEST_GIT = (Get-Command git -CommandType Application | Select-Object -First 1).Source
    $env:RUST_MIN_STACK = '16777216'
    foreach ($filter in @(
        'executable_six_candidate',
        'executable_packaged_skills_are_relocatable_lazy_and_integrity_checked',
        'executable_skills_inspection_is_lazy_without_provider_or_budget_admission',
        'executable_terminal_skill_activation_reports_source_version_reason_and_setup_failures'
    )) {
        $log = Join-Path $directory ($filter + '.log')
        & cargo +1.98.0 test --manifest-path (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml') --target-dir $TargetRoot --locked --offline -p vcp-cli --features qualification --test executable $filter -- --nocapture *> $log
        $code = $LASTEXITCODE
        $record.stages += @{ filter = $filter; exit_code = $code; log_sha256 = (Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant() }
        Save-Report
        if ($code -ne 0) { throw "Installed skill qualification failed: $filter" }
    }
    $record.status = 'pass'
    $record.candidates_after = @(Candidate-Identity)
    if (($record.candidates_before | ConvertTo-Json -Depth 8 -Compress) -cne ($record.candidates_after | ConvertTo-Json -Depth 8 -Compress)) { throw 'Candidate packages changed during qualification' }
    if ((Get-FileHash -LiteralPath (Join-Path $repository 'src/crates/vcp-cli/tests/executable.rs')).Hash.ToLowerInvariant() -cne $record.test_source_sha256) { throw 'Native test changed during qualification' }
    if ((Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant() -cne $record.executable_sha256) { throw 'Installed executable changed during qualification' }
    if ((Get-FileHash -LiteralPath $catalog).Hash.ToLowerInvariant() -cne $record.catalog_sha256) { throw 'Installed catalog changed during qualification' }
    if ((Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant() -cne $record.runner_sha256) { throw 'Qualification runner changed during execution' }
    $record.inputs_unchanged = $true
} catch {
    $record.status = 'fail'; $record.error = $_.Exception.Message
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name]) }
    Save-Report
}
Write-Output $report
if ($record.status -cne 'pass') { exit 1 }
