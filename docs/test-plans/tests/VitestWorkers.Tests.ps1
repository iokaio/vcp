#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Keep the generated test runner inside VCP's native process-tree limit.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $scenarioRoot 'scenario-a-vue-taskboard.ps1'), [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Scenario A has parse errors'
$seed = @{}
foreach ($target in '$seed[''vitest.config.ts'']', '$environmentBlock', '$affected') {
    $assignment = $ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.AssignmentStatementAst] -and
        $node.Left.Extent.Text -eq $target
    }, $true) | Select-Object -First 1
    Assert-That ($null -ne $assignment) "Missing scenario contract: $target"
    . ([scriptblock]::Create($assignment.Extent.Text))
}
# The default forks pool eagerly starts CPU-count-minus-one workers (63 on
# the failing machine). An explicit small cap leaves room for npm/cmd/Vite.
Assert-That ($seed['vitest.config.ts'] -match 'maxWorkers:\s*2\b') 'Generated Vitest worker pool is unbounded'
Assert-That ($seed['vitest.config.ts'] -match 'environment:\s*''jsdom''' -and
    $seed['vitest.config.ts'] -match 'src/\*\*/\*\.spec\.ts') 'Worker fix changed the UI test contract'
Assert-That ($environmentBlock.Contains('32 processes') -and
    $environmentBlock.Contains('maxWorkers: 2') -and
    $environmentBlock.Contains('reusing an older project')) 'Prompts do not preserve or repair bounded workers'
Assert-That ($affected -contains 'vitest.config.ts') 'Reused project cannot repair its Vitest configuration'
Write-Host 'Vitest worker contract passed: bounded scaffold, unchanged test selection, reuse guidance and writable configuration.'
