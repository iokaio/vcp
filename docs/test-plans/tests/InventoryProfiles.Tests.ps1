#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise B's actual profile and prompt composition without processes or inference.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$source = Get-Content -LiteralPath (Join-Path $scenarioRoot 'scenario-b-aspnet-inventory.ps1') -Raw
$start = $source.IndexOf("    `$stage = 'P1-profiles'")
$end = $source.IndexOf("    foreach (`$key in 'T1', 'T2'", $start)
Assert-That ($start -ge 0 -and $end -gt $start) 'Inventory profile block missing'
$compose = [scriptblock]::Create($source.Substring($start, $end - $start))
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Inventory scenario parse failure'
$assignment = $ast.Find({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$environmentBlock' }, $true)
$environmentBlock = $assignment.Right.Expression.Value
$promptFunction = $ast.Find({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'New-Prompt' }, $true)
. ([scriptblock]::Create($promptFunction.Extent.Text))
$regressionAssignment = $ast.Find({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$regressionTests' }, $true)
$protectedRegressionSource = $regressionAssignment.Right.Expression.Value
foreach ($number in 1..4) {
    Assert-That ($source.Contains("-Prompt `$promptT$number -Config `$profiles['T$number']")) "T$number execution lost stage acceptance profile"
    $stageName = @('data', 'api', 'razor', 'regressions')[$number - 1]
    Assert-That ($source.Contains("-Stage 'T$number-$stageName' -Config `$profiles['T$number'] -GateScript `$gatesT$number")) "T$number repair lost stage acceptance profile"
}
Assert-That ($source.Contains("-Prompt `$promptT5 -Config `$profiles['T5'] -PauseAfterProgress")) 'T5 lost explicit pause and full acceptance profile'
Assert-That ($source.Contains("New-ScenarioPauseCheckpoint `$ctx -Profile 'pause-checkpoint'")) 'T5 lost its recorded process checkpoint'
Assert-That ($source.Contains("-Arguments @('resume', `$t5.task) -Config `$profiles['T5']")) 'T5 resume lost full acceptance profile'
Assert-That ($source.Contains("-Stage 'T5-concurrency' -Config `$profiles['T5'] -GateScript `$gatesT5")) 'T5 repair lost full acceptance profile'
function Get-Solution { Join-Path $ws "Inventory.$solutionExtension" }
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-inventory-profiles-' + [guid]::NewGuid().ToString('N'))
$dotnet = (Get-Process -Id $PID).Path
$major = 10; $tfm = 'net10.0'; $efVersion = '10.0.0'
try {
    foreach ($reuse in @($false, $true)) {
      foreach ($solutionExtension in @('sln', 'slnx')) {
        $ws = Join-Path $root ([string]$reuse)
        $ctx = @{
            Workspace = $ws; Profiles = $ws; Temp = $ws; Env = (Join-Path $ws 'env'); Results = $ws; RunId = 'fixture'
            AllowProcessPublish = $true; ReuseProject = $reuse
            Catalog = (Join-Path $ws 'endpoints.json'); SnapshotText = '{}'
            TurnBudgetUsd = [decimal]3; OutputTokens = 8192; MaxRequests = 96
            DeadlineSeconds = 1800; ShortDeadlineSeconds = 150
        }
        $settings = Join-Path $ws 'src/Inventory.Web/appsettings.Development.json'
        Write-Utf8File $settings '{"ConnectionStrings":{"Inventory":"Server=old;Database=preserve-me;Integrated Security=True"}}'
        $before = Get-Sha256 $settings
        $baseline = Join-Path $ws 'tests/Inventory.Tests/UnitTest1.cs'
        Write-Utf8File $baseline 'namespace Inventory.Tests; public class UnitTest1 { [Xunit.Fact] public void Test1() {} }'
        $baselineBefore = Get-Sha256 $baseline
        . $compose
        foreach ($path in @($profiles.Values) + @($profileUntrusted)) {
            $profile = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -Depth 100
            $expectedProcesses = if ($path -eq $profiles['T5']) { 2 } else { 1 }
            Assert-That ($profile.processes.Count -eq $expectedProcesses) 'Inventory process authorization changed outside T5'
            if ($expectedProcesses -eq 2) {
                Assert-That ($profile.processes[1].name -eq 'pause-checkpoint' -and $profile.processes[1].executable -eq $checkpointNode -and $profile.processes[1].max_timeout_ms -eq 300000) 'T5 checkpoint process differs from the authorized Node fixture'
            }
            $process = $profile.processes[0]
            # Runtime public-environment contract; scenario-specific settings are not bootstrap values.
            $allowed = @('SYSTEMROOT', 'WINDIR', 'PATH', 'PATHEXT', 'TEMP', 'TMP', 'LANG', 'LC_ALL', 'TERM', 'CI', 'RUST_BACKTRACE', 'CARGO_TARGET_DIR', 'CARGO_HOME', 'LIB', 'INCLUDE', 'LIBPATH', 'PROGRAMFILES', 'PROGRAMFILES(X86)', 'APPDATA', 'LOCALAPPDATA', 'DOTNET_CLI_HOME')
            foreach ($key in $process.environment.PSObject.Properties.Name) {
                Assert-That ($allowed -contains $key) "Unsupported process environment: $key"
            }
            Assert-That ($process.executable -eq $dotnet -and $process.max_timeout_ms -eq 1200000) 'Inventory process contract changed'
            foreach ($key in 'APPDATA', 'LOCALAPPDATA', 'DOTNET_CLI_HOME') {
                $path = $process.environment.$key
                Assert-That ($path.StartsWith($ctx.Env + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) "$key inherited account state"
                Assert-That (Test-Path -LiteralPath $path -PathType Container) "$key bootstrap directory was not created"
            }
        }
        $review = Get-Content -LiteralPath $profileReview -Raw | ConvertFrom-Json -Depth 100
        Assert-That ($review.processes.Count -eq 0 -and ($review.automatic_effects -join ',') -eq 'read') 'Review gained execution permissions'
        Assert-That ($review.checks.Count -eq 0) 'Review gained native execution checks'
        $previous = @()
        $previousNames = @()
        $expectedCounts = @(3, 11, 14, 18, 21)
        foreach ($number in 1..5) {
            $profile = Get-Content -LiteralPath $profiles["T$number"] -Raw | ConvertFrom-Json -Depth 100
            Assert-That ($profile.checks.Count -eq 1) "T$number missing required native check"
            $check = $profile.checks[0]
            Assert-That ($check.runner -eq 'dotnet' -and $check.profile -eq 'dotnet') "T$number uses wrong runner"
            Assert-That ($check.manifest -ceq "Inventory.$solutionExtension") "T$number lost solution-root coverage"
            Assert-That ($check.timeout_ms -eq 300000) "T$number full timeout changed"
            Assert-That ($check.expected_tests.Count -eq $expectedCounts[$number - 1]) "T$number lost named acceptance tests"
            Assert-That (@($check.expected_tests | Select-Object -Unique).Count -eq $check.expected_tests.Count) "T$number repeats an acceptance name"
            Assert-That ($check.expected_tests -notcontains 'Inventory.Tests.UnitTest1.Test1') 'Baseline template silently satisfies acceptance'
            foreach ($name in $previous) { Assert-That ($check.expected_tests -contains $name) "T$number lost previous acceptance: $name" }
            $previous = @($check.expected_tests)
            $promptAssignment = $ast.Find({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq "`$promptT$number" }, $true)
            $connection = 'Server=fixture;Database=fixture'
            . ([scriptblock]::Create($promptAssignment.Extent.Text))
            $prompt = Get-Variable -Name "promptT$number" -ValueOnly
            Assert-That ($prompt.Contains('Then call `vcp_verify`') -and $prompt.Contains('an empty `verification.outstanding_issues` list')) "T$number does not require current native verification with no outstanding issues"
            if ($number -ne 4) {
                foreach ($name in @($check.expected_tests | Where-Object { $previousNames -notcontains $_ })) {
                    Assert-That ($prompt.Contains(($name -split '\.')[-1])) "T$number prompt/check name mismatch: $name"
                }
            }
            else {
                foreach ($name in @($check.expected_tests | Where-Object { $previousNames -notcontains $_ })) {
                    $method = ($name -split '\.')[-1]
                    Assert-That ($protectedRegressionSource.Contains("public async Task $method()")) "T4 check differs from protected regression: $name"
                }
            }
            $previousNames = @($check.expected_tests)
        }
        $ctx.ShortDeadlineSeconds = 7
        . $compose
        $full = Get-Content -LiteralPath $profiles['T5'] -Raw | ConvertFrom-Json -Depth 100
        Assert-That ($full.checks[0].timeout_ms -eq 300000) 'Legacy short-deadline input changed operational verification containment'
        Assert-That (($full.checks[0].expected_tests -join ',') -ceq ($previous -join ',')) 'Pause qualification weakened T5 acceptance'
        foreach ($connection in @('Server=(localdb)\MSSQLLocalDB;Database=VcpInventory_current;Trusted_Connection=True', 'Server="host with spaces";Database=VcpInventory_current;Integrated Security=True')) {
            $prompt = New-Prompt 'Task body'
            $match = [regex]::Match($prompt, '(?s)`(\["tool".*?\])`')
            Assert-That $match.Success 'EF argument example missing'
            $arguments = $match.Groups[1].Value | ConvertFrom-Json
            $separator = [array]::IndexOf($arguments, '--')
            Assert-That ($separator -gt 0) 'EF application arguments separator missing'
            Assert-That ($arguments[$separator + 1] -eq '--environment' -and $arguments[$separator + 2] -eq 'Development') 'Development environment not forwarded'
            Assert-That ($arguments[$separator + 3] -eq '--ConnectionStrings:Inventory' -and $arguments[$separator + 4] -ceq $connection) 'Run connection did not survive JSON argument encoding'
            Assert-That ($arguments.Count -eq $separator + 5) 'Connection split into extra arguments'
            Assert-That ($prompt -notmatch '\{\{[A-Z_]+\}\}') 'Unresolved prompt placeholder'
        }
        Assert-That ((Get-Sha256 $settings) -eq $before) 'Profile/prompt preparation changed protected appsettings'
        Assert-That ((Get-Sha256 $baseline) -eq $baselineBefore) 'Profile preparation rewrote existing baseline tests'
      }
    }
    Write-Host 'Inventory profile regressions passed: cumulative named dotnet checks, solution coverage, recorded T5 checkpoint, independent verification containment, read-only review, fresh/reused projects, literal EF overrides, protected configuration preserved.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-inventory-profiles-*') { throw 'Unsafe cleanup path' }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
