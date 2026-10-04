#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Execute the real A/B baseline blocks with external processes stubbed out.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking

function Get-BaselineBlock([string]$File) {
    $source = Get-Content -LiteralPath (Join-Path $scenarioRoot $File) -Raw
    $start = $source.IndexOf('    # --- B0:')
    $end = $source.IndexOf('    Initialize-GitCheckpoint $ctx', $start)
    Assert-That ($start -ge 0 -and $end -gt $start) "Baseline boundaries missing in $File"
    return [scriptblock]::Create($source.Substring($start, $end - $start))
}
function Invoke-Npm {
    param($Stage, $Label, $Arguments, $TimeoutSeconds)
    $calls.Add(($Arguments -join ' '))
    @{ ExitCode = 0; Output = ''; Errors = '' }
}
function Invoke-Dotnet {
    param($Stage, $Label, $Arguments)
    $calls.Add(($Arguments -join ' '))
    @{ ExitCode = 0; Output = ''; Errors = '' }
}
function Get-LatestNuGetVersion { throw 'Reused projects must retain existing dependency versions.' }
function Get-Solution { Join-Path $ws 'Inventory.sln' }
function Test-Typecheck {}
function Test-NodeTests {}
function Test-UnitAndBuild {}
function Test-Build {}
function Test-Tests {}
function Get-FailedGates {
    if ($script:failBaseline) { return @(@{ id = 'typecheck'; detail = "TS18046: 'task' is of type 'unknown'." }) }
    @()
}
function Save-Checkpoint {}

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-reuse-ab-' + [guid]::NewGuid().ToString('N'))
$calls = [Collections.Generic.List[string]]::new()
$seed = @{ 'src/App.vue' = 'must never overwrite existing content' }
$webProject = 'src/Inventory.Web/Inventory.Web.csproj'
$testProject = 'tests/Inventory.Tests/Inventory.Tests.csproj'
$major = 8
$tfm = 'net8.0'
$database = 'VcpInventory_fixture'
$useLocalDb = $false

try {
    foreach ($case in 'a', 'b-config-manifest', 'b-root-manifest') {
        $scenario = if ($case -eq 'a') { 'a' } else { 'b' }
        $manifest = if ($case -eq 'b-root-manifest') { 'dotnet-tools.json' } else { '.config/dotnet-tools.json' }
        $ws = Join-Path $testRoot $case
        $ctx = @{ ReuseProject = $true; Name = $scenario; Logs = $testRoot; ProgressLog = (Join-Path $testRoot 'progress.log'); Notes = [Collections.Generic.List[string]]::new() }
        $fixtures = if ($scenario -eq 'a') {
            @{
                'package.json' = '{"scripts":{"test":"node --test tests/health.test.ts"}}'
                'package-lock.json' = '{}'
                'server/app.ts' = '// existing app'
                'server/index.ts' = '// existing startup'
                'src/App.vue' = '<p>User edits</p>'
                'tests/health.test.ts' = '// existing tests'
                'tests/regressions.test.ts' = '// existing protected regression tests'
            }
        }
        else {
            @{
                $webProject = '<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup><PackageReference Include="Microsoft.EntityFrameworkCore.SqlServer" Version="8.0.9" /></ItemGroup></Project>'
                $testProject = '<Project />'
                $manifest = '{"version":1,"isRoot":true,"tools":{"dotnet-ef":{"version":"8.0.9","commands":["dotnet-ef"]}}}'
                'src/Inventory.Web/appsettings.Development.json' = '{"ConnectionStrings":{"Inventory":"existing configuration"}}'
                'tests/Inventory.Tests/RegressionTests.cs' = '// existing protected regression tests'
                'Inventory.sln' = '// existing solution'
                'README.md' = 'User documentation'
                '.gitignore' = 'custom-ignore/'
            }
        }
        Write-SeedFiles -Root $ws -Files $fixtures
        $before = @{}
        foreach ($relative in $fixtures.Keys) { $before[$relative] = Get-Sha256 (Join-Path $ws $relative) }
        $file = if ($scenario -eq 'a') { 'scenario-a-vue-taskboard.ps1' } else { 'scenario-b-aspnet-inventory.ps1' }
        $baseline = Get-BaselineBlock $file
        $calls.Clear()
        & $baseline
        foreach ($relative in $fixtures.Keys) {
            Assert-That ((Get-Sha256 (Join-Path $ws $relative)) -eq $before[$relative]) "Reused $scenario baseline modified $relative"
        }
        if ($scenario -eq 'a') {
            Assert-That ($calls.Count -eq 1 -and $calls[0] -eq 'ci --no-audit --no-fund') 'Reused TaskBoard must restore from the existing lockfile.'
            foreach ($directory in @('src/api', 'src/composables', 'src/components')) {
                Assert-That (Test-Path -LiteralPath (Join-Path $ws $directory) -PathType Container) "Required patch parent missing: $directory"
                Assert-That (@(Get-ChildItem -LiteralPath (Join-Path $ws $directory)).Count -eq 0) 'Scaffolding must not preimplement UI behavior.'
            }
            # A retained project can contain broken output from a prior turn. Reject it
            # before paid execution without blaming or rewriting the toolchain/tests.
            $script:failBaseline = $true
            $baselineError = $null
            try { & $baseline } catch { $baselineError = $_.Exception.Message }
            finally { $script:failBaseline = $false }
            Assert-That ($baselineError -like 'The retained TaskBoard project failed baseline checks:*') 'A failed reused baseline must identify retained project errors.'
            Assert-That ($baselineError.Contains($ws) -and $baselineError.Contains('B0-baseline')) 'Baseline diagnostic must identify the workspace and check logs.'
            Assert-That ($baselineError -match 'no paid VCP turns started') 'Baseline diagnostic must explain the zero-spend stop.'
            foreach ($relative in $fixtures.Keys) {
                Assert-That ((Get-Sha256 (Join-Path $ws $relative)) -eq $before[$relative]) "Failed TaskBoard baseline modified $relative"
            }
        }
        else { Assert-That ($calls.Count -eq 2 -and $calls[0] -eq 'tool restore' -and $calls[1] -eq "restore $(Get-Solution)") 'Reused Inventory must restore rather than recreate templates or add packages.' }

        $source = Get-Content -LiteralPath (Join-Path $scenarioRoot $file) -Raw
        $start = $source.IndexOf('    # --- T4: protected regression tests')
        $end = $source.IndexOf('    $gatesT4 =', $start)
        $protected = @{}
        $regressionTest = '// replacement that must not overwrite existing tests'
        $regressionTests = $regressionTest
        & ([scriptblock]::Create($source.Substring($start, $end - $start)))
        foreach ($relative in $fixtures.Keys) {
            Assert-That ((Get-Sha256 (Join-Path $ws $relative)) -eq $before[$relative]) "Regression setup modified existing $relative"
        }
        $regressionPath = if ($scenario -eq 'a') { 'tests/regressions.test.ts' } else { 'tests/Inventory.Tests/RegressionTests.cs' }
        Assert-That ($protected[$regressionPath] -eq $before[$regressionPath]) 'Protected test hash must describe the preserved file.'
        $protected[$regressionPath] = 'original-baseline-hash'
        & ([scriptblock]::Create($source.Substring($start, $end - $start)))
        Assert-That ($protected[$regressionPath] -eq 'original-baseline-hash') 'Regression setup must not replace an existing baseline hash.'

        if ($scenario -eq 'b') {
            Remove-Item -LiteralPath (Join-Path $ws $manifest)
            $calls.Clear()
            $rejected = $false
            try { & $baseline } catch { $rejected = $_.Exception.Message -like 'Existing project is not *scenario project:*dotnet-tools.json*' }
            Assert-That $rejected 'Inventory without either manifest must fail before dependency commands.'
            Assert-That ($calls.Count -eq 0) 'Missing manifest triggered dependency commands.'
            New-Item -ItemType Directory -Path (Join-Path $ws $manifest) -Force | Out-Null
            $rejected = $false
            try { & $baseline } catch { $rejected = $_.Exception.Message -like 'Existing project is not *scenario project:*dotnet-tools.json*' }
            Assert-That $rejected 'A directory named dotnet-tools.json must not count as a manifest.'
            Assert-That ($calls.Count -eq 0) 'Invalid manifest path triggered dependency commands.'
            Remove-Item -LiteralPath (Join-Path $ws $manifest)
            Write-Utf8File (Join-Path $ws $manifest) $fixtures[$manifest]
        }

        $missing = if ($scenario -eq 'a') { 'server/app.ts' } else { $webProject }
        Remove-Item -LiteralPath (Join-Path $ws $missing)
        $calls.Clear()
        $rejected = $false
        try { & $baseline } catch { $rejected = $_.Exception.Message -like 'Existing project is not *scenario project:*' }
        Assert-That $rejected 'An incompatible project must fail with an actionable error.'
        Assert-That ($calls.Count -eq 0) 'An incompatible project must fail before dependency commands.'
    }
    Write-Host 'A/B project reuse passed (root and .config tool manifests, preserved source/config/tests, locked dependency restore, missing manifest and incompatible project rejection).'
}
finally {
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if ($resolved.StartsWith($tempBase.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
