#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Runs actual scenario initialization helpers on fixtures without toolchains or providers.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-project-reuse-cd-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function Import-ScenarioHelpers([string]$File, [string[]]$Names) {
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot $File), [ref]$tokens, [ref]$errors)
    Check ($errors.Count -eq 0) "$File does not parse"
    Check (@($ast.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -eq 'ProjectPath' }).Count -eq 1) "$File does not expose ProjectPath"
    foreach ($name in $Names) {
        $definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
        if (-not $definition) { throw "Missing helper $name" }
        . ([scriptblock]::Create(($definition.Extent.Text -replace ('^function\s+' + [regex]::Escape($name)), "function script:$name")))
    }
}
function Expect-Incompatible([scriptblock]$Action) {
    $before = Get-WorkspaceManifest $ws -IncludeGenerated
    $rejected = $false
    try { & $Action } catch { $rejected = $_.Exception.Message -like 'Incompatible existing*' }
    Check $rejected 'Incompatible existing project was not rejected clearly'
    Check ((Compare-WorkspaceManifest $before (Get-WorkspaceManifest $ws -IncludeGenerated)).Changed -eq 0) 'Rejected project was modified'
}
try {
    Import-ScenarioHelpers 'scenario-c-java-ledger-cli.ps1' @('Assert-LedgerFixture', 'Initialize-LedgerProject', 'Add-LedgerRegressionTests')
    Import-ScenarioHelpers 'scenario-d-python-textlab.ps1' @('Assert-TextlabFixture', 'Initialize-TextlabProject', 'Add-TextlabRegressionTests')
    foreach ($scenario in 'C', 'D') {
        $ws = Join-Path $temporary $scenario
        $ctx = @{ ReuseProject = $false }
        if ($scenario -eq 'C') {
            $seed = [ordered]@{
                'pom.xml' = '<project>fixture</project>'
                'src/main/java/io/vcp/ledger/LedgerApp.java' = '// seeded source'
                'src/test/java/io/vcp/ledger/LedgerAppTest.java' = '// baseline test'
                'samples/transactions-2026Q1.csv' = "date,description,amount,account`n2026-01-01,test,1.00,checking`n"
                'samples/rules.csv' = "pattern,category`ntest,Example`n"
                'README.md' = 'seed readme'
            }
            $regressionTest = '// protected regression'
            $initialize = { Initialize-LedgerProject }; $inject = { Add-LedgerRegressionTests }
            $source = 'src/main/java/io/vcp/ledger/LedgerApp.java'; $config = 'pom.xml'
            $fixture = 'samples/transactions-2026Q1.csv'; $regression = 'src/test/java/io/vcp/ledger/RegressionTest.java'
        }
        else {
            $seed = [ordered]@{
                'pyproject.toml' = '[project]'
                'src/textlab/__main__.py' = '# seeded source'
                'tests/test_cli.py' = '# baseline test'
                'README.md' = 'seed readme'
            }
            $trainRows = @([pscustomobject]@{ id = 'train-1'; text = 'Please refund, thank you'; category = 'billing'; sentiment = 'neutral' })
            $devRows = @([pscustomobject]@{ id = 'dev-1'; text = "quoted`ntext"; category = 'account'; sentiment = 'negative' })
            $regressionTests = '# protected regression'
            $initialize = { Initialize-TextlabProject }; $inject = { Add-TextlabRegressionTests }
            $source = 'src/textlab/__main__.py'; $config = 'pyproject.toml'
            $fixture = 'data/tickets_train.csv'; $regression = 'tests/test_regressions.py'
        }
        & $initialize
        Check (Test-Path -LiteralPath (Join-Path $ws $fixture)) "$scenario fresh scaffold is missing fixture"
        & $inject
        $ctx.ReuseProject = $true
        Write-Utf8File (Join-Path $ws $source) 'user source edits'
        Write-Utf8File (Join-Path $ws $config) 'user configuration edits'
        Write-Utf8File (Join-Path $ws 'README.md') 'user documentation edits'
        Write-Utf8File (Join-Path $ws 'unrelated.txt') 'unrelated user data'
        # Equivalent CRLF fixtures must keep their original bytes.
        $fixtureText = [IO.File]::ReadAllText((Join-Path $ws $fixture)).Replace("`r`n", "`n").Replace("`n", "`r`n")
        Write-Utf8File (Join-Path $ws $fixture) $fixtureText
        $before = Get-WorkspaceManifest $ws -IncludeGenerated
        & $initialize
        & $inject
        Check ((Compare-WorkspaceManifest $before (Get-WorkspaceManifest $ws -IncludeGenerated)).Changed -eq 0) "$scenario reuse overwrote existing source, config, fixture or regression bytes"
        $seed['new-support-file.txt'] = 'new support file'
        & $initialize
        Check ((Get-Content -LiteralPath (Join-Path $ws 'new-support-file.txt') -Raw) -eq 'new support file') "$scenario did not add missing scaffolding"
        Check ((Get-Content -LiteralPath (Join-Path $ws $source) -Raw) -eq 'user source edits') "$scenario missing scaffolding changed source"

        Write-Utf8File (Join-Path $ws $fixture) 'different user dataset'
        Expect-Incompatible $initialize
        Write-Utf8File (Join-Path $ws $fixture) $fixtureText
        Write-Utf8File (Join-Path $ws $regression) 'different user tests'
        Expect-Incompatible $initialize
        Expect-Incompatible $inject
        $ws = Join-Path $temporary "$scenario-incompatible"
        Write-Utf8File (Join-Path $ws 'user-file.txt') 'unrelated project'
        Expect-Incompatible $initialize
    }
    Write-Host "PASS: $checks existing-project C/D checks"
}
finally {
    $full = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-project-reuse-cd-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
