#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Execute the scenario's real seeded tests against an inert local HTTP fixture.
# No provider, npm install, existing project, or external network is used.
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path (Split-Path -Parent $PSScriptRoot) 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$node = (Get-Command node -CommandType Application -ErrorAction Stop).Source
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path (Split-Path -Parent $PSScriptRoot) 'scenario-a-vue-taskboard.ps1'), [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Scenario parse errors'
function Get-AssignedString([string]$Left) {
    $assignment = $ast.Find({ param($entry) $entry -is [System.Management.Automation.Language.AssignmentStatementAst] -and $entry.Left.Extent.Text -eq $Left }, $true)
    Assert-That ($null -ne $assignment) "Missing scenario fixture $Left"
    return $assignment.Right.Expression.Value
}
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-test-isolation-' + [guid]::NewGuid().ToString('N'))
$workspace = Join-Path $testRoot 'workspace'
$runtime = Join-Path $testRoot 'runtime'
$savedTmp = $env:TMP; $savedTemp = $env:TEMP; $savedAudit = $env:VCP_TEST_DATA_AUDIT
try {
    New-Item -ItemType Directory -Path $runtime -Force | Out-Null
    $app = @'
import { createServer } from 'node:http'
import { writeFileSync, appendFileSync } from 'node:fs'
export function createApp({ dataFile }) {
  if (!dataFile) throw new Error('test must supply an isolated data file')
  writeFileSync(dataFile, 'fixture database')
  appendFileSync(process.env.VCP_TEST_DATA_AUDIT, JSON.stringify(dataFile) + '\n')
  return createServer((request, response) => {
    response.setHeader('content-type', 'application/json')
    response.statusCode = request.url === '/api/health' ? 200 : 404
    response.end(JSON.stringify(request.url === '/api/health' ? { status: 'ok' } : { error: 'deliberate failing API fixture' }))
  })
}
'@
    Write-SeedFiles -Root $workspace -Files @{
        'package.json' = '{"type":"module"}'
        'server/app.ts' = $app
        'tests/health.test.ts' = (Get-AssignedString '$seed[''tests/health.test.ts'']')
        'tests/regressions.test.ts' = (Get-AssignedString '$regressionTest')
    }
    $before = @{}
    Get-ChildItem -LiteralPath $workspace -Recurse -File | ForEach-Object { $before[$_.FullName] = Get-Sha256 $_.FullName }
    $env:TMP = $runtime; $env:TEMP = $runtime
    $env:VCP_TEST_DATA_AUDIT = Join-Path $testRoot 'data-paths.jsonl'
    foreach ($fixture in @('health', 'regressions')) {
        $output = & $node --test --test-reporter=tap (Join-Path $workspace "tests/$fixture.test.ts") 2>&1
        $exitCode = $LASTEXITCODE
        $expected = if ($fixture -eq 'health') { 0 } else { 1 }
        Assert-That ($exitCode -eq $expected) "$fixture expected exit $expected, got ${exitCode}: $output"
        Assert-That (@(Get-ChildItem -LiteralPath $runtime -Force).Count -eq 0) "$fixture left its runtime data behind"
        Assert-That (@(Get-ChildItem -LiteralPath $workspace -Recurse -File).Count -eq $before.Count) "$fixture created workspace files"
        foreach ($file in $before.Keys) { Assert-That ((Get-Sha256 $file) -eq $before[$file]) "$fixture rewrote source $file" }
    }
    $paths = @(Get-Content -LiteralPath $env:VCP_TEST_DATA_AUDIT | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-That ($paths.Count -eq 5) 'Expected health plus all four protected regression fixtures'
    Assert-That (@($paths | Select-Object -Unique).Count -eq 5) 'Tests must not share a database'
    foreach ($path in $paths) {
        Assert-That ([IO.Path]::GetFullPath($path).StartsWith($runtime + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) 'Database escaped its OS temporary directory'
    }
    Write-Host 'Test runtime isolation passed: five unique databases outside sources; cleanup on passing and failing assertions.'
}
finally {
    $env:TMP = $savedTmp; $env:TEMP = $savedTemp; $env:VCP_TEST_DATA_AUDIT = $savedAudit
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if ($resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
