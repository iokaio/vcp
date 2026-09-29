# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$InstallationReport, [string]$OutputRoot, [string]$TargetRoot)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Actual native Windows required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$InstallationReport = [IO.Path]::GetFullPath($InstallationReport)
$installation = Get-Content -LiteralPath $InstallationReport -Raw | ConvertFrom-Json
if ($installation.schema -cne 'cs3-promoted-package-installation/1' -or $installation.status -cne 'pass' -or
    $installation.inputs_unchanged -ne $true) { throw 'Actual promoted install/upgrade/rollback evidence required' }
$admissionRaw = & node (Join-Path $PSScriptRoot 'cs3-promoted-distribution.cjs') admit $installation.promotion_spec.path
if ($LASTEXITCODE -ne 0) { throw 'Promoted admission failed before test execution' }
$admission = $admissionRaw | ConvertFrom-Json
if (($admission | ConvertTo-Json -Depth 40 -Compress) -cne ($installation.promotion_admission | ConvertTo-Json -Depth 40 -Compress)) { throw 'Installation admission differs' }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/cs3-promoted-installed' }
if (-not $TargetRoot) { $TargetRoot = Join-Path $repository 'artifacts/codex-target' }
$TargetRoot = [IO.Path]::GetFullPath($TargetRoot)
$directory = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $directory | Out-Null
$admissionFile = Join-Path $directory 'admission.json'
[IO.File]::WriteAllText($admissionFile, (($admission | ConvertTo-Json -Depth 40) + "`n"), [Text.UTF8Encoding]::new($false))
$binary = Join-Path $installation.installed_candidate 'vcp.exe'
$catalog = Join-Path $installation.installed_candidate 'skills/builtin/catalog.json'
function Test-Identity {
    if ((Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant() -cne $admission.executable.sha256 -or
        (Get-FileHash -LiteralPath $catalog).Hash.ToLowerInvariant() -cne $admission.catalog.sha256) { throw 'Exact installed promoted executable/catalog required' }
}
function Test-Sources {
    return @('src/crates/vcp-cli/tests/executable.rs','src/crates/vcp-cli/tests/support/promoted_skills.rs') | ForEach-Object {
        [ordered]@{ path = $_; sha256 = (Get-FileHash -LiteralPath (Join-Path $repository $_)).Hash.ToLowerInvariant() }
    }
}
Test-Identity
$record = [ordered]@{
    schema = 'cs3-promoted-installed-skills/1'; task = 'CS-3'; status = 'running'
    installation_report_sha256 = (Get-FileHash -LiteralPath $InstallationReport).Hash.ToLowerInvariant()
    executable_sha256 = $admission.executable.sha256; catalog_sha256 = $admission.catalog.sha256
    archive_sha256 = $installation.archive_sha256
    admission = [ordered]@{path=$admissionFile;sha256=(Get-FileHash -LiteralPath $admissionFile).Hash.ToLowerInvariant()}
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
    test_sources = @(Test-Sources); target_root = $TargetRoot; stages = @(); paid_requests = 0
    limitations = @('Exact promoted builtin package qualification, not release or performance certification.', 'Comparison acceptance is separately rederived before and after these model-free checks.')
}
$report = Join-Path $directory 'result.json'
function Save-Report { $record | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath $report -Encoding utf8 }
Save-Report
$names = @('VCP_TEST_SKILL_PACKAGE','VCP_TEST_CANDIDATE_ROOT','VCP_TEST_PROMOTED_ADMISSION','VCP_TEST_PROMOTED_ADMISSION_SHA256','VCP_TEST_NODE','VCP_TEST_GIT','RUST_MIN_STACK')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
    $env:VCP_TEST_SKILL_PACKAGE = $installation.installed_candidate
    [Environment]::SetEnvironmentVariable('VCP_TEST_CANDIDATE_ROOT', $null)
    $env:VCP_TEST_PROMOTED_ADMISSION = $admissionFile
    $env:VCP_TEST_PROMOTED_ADMISSION_SHA256 = $record.admission.sha256
    $env:VCP_TEST_NODE = (Get-Command node -CommandType Application | Select-Object -First 1).Source
    $env:VCP_TEST_GIT = (Get-Command git -CommandType Application | Select-Object -First 1).Source
    $env:RUST_MIN_STACK = '16777216'
    foreach ($filter in @(
        'promoted_skills::executable_promoted_six_builtin_offline_discovery_and_integrity',
        'promoted_skills::executable_promoted_six_builtin_report_only_profiles_complete_without_workspace_edits',
        'promoted_skills::executable_promoted_six_builtin_terminal_controls_preserve_precedence_and_revocation'
    )) {
        $log = Join-Path $directory ($filter.Replace('::','-') + '.log')
        & cargo +1.98.0 test --manifest-path (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.toml') --target-dir $TargetRoot --locked --offline -j2 -p vcp-cli --features qualification --test executable $filter -- --ignored --exact --nocapture *> $log
        $code = $LASTEXITCODE
        $record.stages += @{filter=$filter;exit_code=$code;log_sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()}
        Save-Report
        if ($code -ne 0 -or -not (Select-String -LiteralPath $log -Pattern 'test result: ok\. 1 passed; 0 failed; 0 ignored;' -Quiet) -or
            -not (Select-String -LiteralPath $log -SimpleMatch ('test ' + $filter + ' ... ok') -Quiet)) { throw "Exact promoted builtin test did not execute successfully: $filter" }
    }
    Test-Identity
    if ((@(Test-Sources) | ConvertTo-Json -Depth 8 -Compress) -cne ($record.test_sources | ConvertTo-Json -Depth 8 -Compress) -or
        (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant() -cne $record.runner_sha256 -or
        (Get-FileHash -LiteralPath $InstallationReport).Hash.ToLowerInvariant() -cne $record.installation_report_sha256 -or
        (Get-FileHash -LiteralPath $admissionFile).Hash.ToLowerInvariant() -cne $record.admission.sha256) { throw 'Promoted test inputs changed' }
    $afterRaw = & node (Join-Path $PSScriptRoot 'cs3-promoted-distribution.cjs') admit $installation.promotion_spec.path
    if ($LASTEXITCODE -ne 0) { throw 'Promoted admission changed during test execution' }
    if ((($afterRaw | ConvertFrom-Json) | ConvertTo-Json -Depth 40 -Compress) -cne ($admission | ConvertTo-Json -Depth 40 -Compress)) { throw 'Promoted admission changed' }
    $record.inputs_unchanged = $true; $record.status = 'pass'
} catch { $record.status = 'fail'; $record.error = $_.Exception.Message }
finally { foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name]) }; Save-Report }
Write-Output $report
if ($record.status -cne 'pass') { exit 1 }
