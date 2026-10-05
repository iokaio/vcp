#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot 'scenario-a-vue-taskboard.ps1'), [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Scenario A must parse'
function Get-GateTest([string]$Id) {
    $command = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Invoke-Gate' -and
        "'$Id'" -in $node.CommandElements.Extent.Text
    }, $true)
    Assert-That ($null -ne $command) "Missing $Id gate"
    return @($command.CommandElements | Where-Object { $_ -is [Management.Automation.Language.ScriptBlockExpressionAst] })[0].ScriptBlock.GetScriptBlock()
}
$script:checks = 0
function Check([scriptblock]$Test, [bool]$ShouldPass, [string]$Name) {
    $passed = $true
    $failure = ''
    try { & $Test | Out-Null } catch { $passed = $false; $failure = $_.Exception.Message }
    Assert-That ($passed -eq $ShouldPass) "$Name ($failure)"
    $script:checks++
}
function Get-Tail([string]$Text) { $Text }
$legacyId = 'legacy-fixture'
$base = 'http://fixture.invalid'
$legacyTest = Get-GateTest 'labels.legacy-default'
$script:legacy = @{ id = $legacyId; labels = @() }
$script:filterStatus = 200
$script:filterItems = @()
function Invoke-Http($Method, $Uri) {
    if ($Uri -like '*?label=ui') { return @{ Status = $script:filterStatus; Json = @{ items = $script:filterItems } } }
    if ($Uri.EndsWith('/' + $legacyId)) { return @{ Status = 200; Json = $script:legacy } }
    return @{ Status = 200; Json = @{ items = @($script:legacy) } }
}
Check $legacyTest $true 'Normalized legacy task should pass'
$script:legacy = @{ id = $legacyId }
Check $legacyTest $false 'Missing labels must not pass as an empty array'
$script:legacy = @{ id = $legacyId; labels = $null }
Check $legacyTest $false 'Null labels must fail'
$script:legacy = @{ id = $legacyId; labels = '' }
Check $legacyTest $false 'String labels must fail'
$script:legacy = @{ id = $legacyId; labels = @('ui') }
Check $legacyTest $false 'Invented labels must fail'
$script:legacy = @{ id = $legacyId; labels = @() }
$script:filterStatus = 500
Check $legacyTest $false 'Observed legacy filtering HTTP500 must fail'
$script:filterStatus = 200
$script:filterItems = @($script:legacy)
Check $legacyTest $false 'Unlabeled task must not match label filter'

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('vcp-label-gates-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($testRoot)
$ctx = @{ Logs = $testRoot }
$Stage = 'labels'; $node = 'fixture-node'; $MinUnitTests = 1
$TestIds = @('label-chip', 'sort-select')
$unitTest = Get-GateTest 'vitest'
$script:caseNames = @('sort selector emits selected order', 'label chip requests filtering')
$script:skip = $false
function Invoke-Tool {
    param($Ctx, $Stage, $Label, $FilePath, $ArgumentList)
    $output = @($ArgumentList | Where-Object { $_.StartsWith('--outputFile=') })[0].Substring(13)
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $output))
    $cases = ($script:caseNames | ForEach-Object { '<testcase name="' + $_ + '">' + $(if ($script:skip) { '<skipped />' }) + '</testcase>' }) -join ''
    [IO.File]::WriteAllText($output, '<testsuites failures="0" errors="0"><testsuite>' + $cases + '</testsuite></testsuites>')
    return @{ ExitCode = 0; Output = ''; Errors = '' }
}
try {
    Check $unitTest $true 'Both required UI behavior tests should pass'
    $script:caseNames = @('TaskBoard > sort selector emits selected order', 'TaskBoard > nested scope > label chip requests filtering')
    Check $unitTest $true 'Real Vitest JUnit describe prefixes preserve exact leaf names'
    $script:caseNames = @('unrelated test', 'label chip requests filtering')
    Check $unitTest $false 'Test count alone cannot replace sort coverage'
    $script:caseNames = @('sort selector emits selected order', 'unrelated test')
    Check $unitTest $false 'Chip coverage is required'
    $script:caseNames = @('sort selector emits selected order', 'sort selector emits selected order', 'label chip requests filtering')
    Check $unitTest $false 'Duplicate required test names must fail'
    $script:caseNames = @('sort selector emits selected order', 'label chip requests filtering'); $script:skip = $true
    Check $unitTest $false 'Skipped UI behavior tests must fail'
    $script:skip = $false; $script:caseNames = @('earlier UI behavior'); $TestIds = @('task-card')
    Check $unitTest $true 'Earlier stages do not require later T3 tests'
}
finally {
    $resolved = [IO.Path]::GetFullPath($testRoot)
    $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe temporary cleanup target' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Labels gates: $script:checks checks passed."
