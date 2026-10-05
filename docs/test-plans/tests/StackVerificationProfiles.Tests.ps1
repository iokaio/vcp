#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Execute the real C/D profile composition without inference or external processes.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-stack-profiles-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
try {
    foreach ($stack in 'c-java-ledger-cli', 'd-python-textlab') {
        $source = Get-Content -LiteralPath (Join-Path $scenarioRoot "scenario-$stack.ps1") -Raw
        $tokens = $null; $errors = $null
        $ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
        Check ($errors.Count -eq 0) "$stack parse failure"
        $start = $source.IndexOf("    `$stage = 'P1-profiles'")
        $end = $source.IndexOf("    foreach (`$key in 'T1', 'T2'", $start)
        Check ($start -ge 0 -and $end -gt $start) "$stack profile block missing"
        $compose = [scriptblock]::Create($source.Substring($start, $end - $start))
        $regression = $ast.Find({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$regressionNames' }, $true)
        . ([scriptblock]::Create($regression.Extent.Text))
        foreach ($reuse in @($false, $true)) {
            $ws = Join-Path $root "$stack-$reuse"
            $ctx = @{
                Workspace = $ws; Profiles = $ws; Temp = $ws; Env = $ws; Results = $ws; RunId = 'fixture'
                AllowProcessPublish = $true; ReuseProject = $reuse; Catalog = (Join-Path $ws 'endpoints.json'); SnapshotText = '{}'
                TurnBudgetUsd = [decimal]3; OutputTokens = 8192; MaxRequests = 96; DeadlineSeconds = 1800
            }
            $java = $javac = $python = $basePython = (Get-Process -Id $PID).Path
            $mavenHome = Join-Path $ws 'maven'
            $classworlds = @{ FullName = (Join-Path $mavenHome 'boot/plexus-classworlds.jar') }
            $m2conf = Join-Path $mavenHome 'bin/m2.conf'
            foreach ($number in 1..5) { Set-Variable -Name "promptT$number" -Value "Task T$number" }
            . $compose
            $javaStack = $stack.StartsWith('c-')
            $counts = if ($javaStack) { @(4, 7, 10, 15, 17) } else { @(6, 9, 14, 19, 21) }
            $previous = @()
            foreach ($number in 1..5) {
                $profile = Get-Content -LiteralPath $profiles["T$number"] -Raw | ConvertFrom-Json -Depth 100
                Check ($profile.checks.Count -eq 1) "$stack T$number missing canonical verification"
                $check = $profile.checks[0]
                Check ($check.manifest -ceq $(if ($javaStack) { 'pom.xml' } else { 'pyproject.toml' })) "$stack lost root coverage"
                Check ($check.runner -eq $(if ($javaStack) { 'maven' } else { 'pytest' })) "$stack wrong runner"
                Check ($check.profile -eq $(if ($javaStack) { 'java' } else { 'python' })) "$stack wrong process"
                Check ($check.timeout_ms -eq 1200000) "$stack wrong check timeout"
                Check ($check.expected_tests.Count -eq $counts[$number - 1]) "$stack T$number missing named checks"
                Check (@($check.expected_tests | Select-Object -Unique).Count -eq $check.expected_tests.Count) "$stack duplicate names"
                foreach ($name in $previous) { Check ($check.expected_tests -ccontains $name) "$stack lost prior acceptance $name" }
                $prompt = Get-Variable -Name "promptT$number" -ValueOnly
                foreach ($name in @($check.expected_tests | Where-Object { $previous -notcontains $_ })) {
                    Check ($prompt.Contains($name)) "$stack prompt/check mismatch: $name"
                }
                $previous = @($check.expected_tests)
                if ($javaStack) {
                    Check ($check.maven.classworlds_jar -ceq $classworlds.FullName -and $check.maven.classworlds_conf -ceq $m2conf -and $check.maven.home -ceq $mavenHome) 'Maven launcher lost owner paths'
                }
                else { Check (-not $check.PSObject.Properties['maven']) 'Python gained Maven launcher config' }
                Check ($source.Contains("-Prompt `$promptT$number -Config `$profiles['T$number']")) "$stack T$number dispatch lost acceptance profile"
                Check ($source.Contains("-Config `$profiles['T$number'] -GateScript `$gatesT$number")) "$stack T$number repair lost acceptance profile"
            }
            $review = Get-Content -LiteralPath $profileReview -Raw | ConvertFrom-Json -Depth 100
            Check ($review.checks.Count -eq 0 -and $review.processes.Count -eq 0 -and ($review.automatic_effects -join ',') -eq 'read') "$stack review gained execution"
            Check (-not $source.Contains('$profileMain') -and -not $source.Contains('$profileShort')) "$stack has a stale shared or deadline profile"
            Check ($source.Contains('-PauseAfterProgress -AcceptExit @(8)')) "$stack lost explicit T5 pause"
        }
    }
    "Stack verification profile checks passed: $checks"
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture cleanup escaped temp root' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
