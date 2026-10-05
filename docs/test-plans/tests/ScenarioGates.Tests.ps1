#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<# Scenario gate regressions without VCP, network, installed toolchains, or provider spend. #>
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking

function Import-ScenarioFunction([string]$File, [string]$Name) {
    $tokens = $null; $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot $File), [ref]$tokens, [ref]$errors)
    Assert-That ($errors.Count -eq 0) "Parse errors in $File"
    $definition = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name }, $true)
    Assert-That ($null -ne $definition) "Missing $Name in $File"
    # Load actual definitions without scenario initialization or paid turns.
    $source = $definition.Extent.Text -replace ('^function\s+' + [regex]::Escape($Name)), "function script:$Name"
    . ([scriptblock]::Create($source))
}
function Assert-Rejected([scriptblock]$Action, [string]$Reason) {
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    Assert-That $rejected $Reason
}
function Assert-Gate([string]$Stage, [string]$Id, [string]$Outcome) {
    $gate = @($ctx.Gates | Where-Object { $_.stage -eq $Stage -and $_.id -eq $Id })[-1]
    Assert-That ($gate.outcome -eq $Outcome) "$Stage/$Id expected $Outcome, got $($gate.outcome): $($gate.detail)"
}
function Get-Tail([string]$Text, [int]$Count = 40) { $Text }
function Get-Solution { 'fixture.sln' }

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-scenario-gates-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$ctx = @{ Name = 'gate-tests'; Logs = $testRoot; ProgressLog = (Join-Path $testRoot 'progress.log'); Gates = [System.Collections.Generic.List[object]]::new() }
$ws = Join-Path $testRoot 'workspace'
$node = 'fixture-node.exe'

try {
    # Execute the actual create gate without starting a server or running paid turns.
    $tokens = $null; $errors = $null
    $vueAst = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot 'scenario-a-vue-taskboard.ps1'), [ref]$tokens, [ref]$errors)
    Assert-That ($errors.Count -eq 0) 'Vue scenario parse errors'
    $createCommand = $vueAst.Find({
        param($node)
        $node -is [System.Management.Automation.Language.CommandAst] -and
        $node.GetCommandName() -eq 'Invoke-Gate' -and
        "'api.create'" -in $node.CommandElements.Extent.Text
    }, $true)
    Assert-That ($null -ne $createCommand) 'Missing api.create gate'
    $createTestAst = @($createCommand.CommandElements | Where-Object { $_ -is [System.Management.Automation.Language.ScriptBlockExpressionAst] })[0]
    $createTest = $createTestAst.ScriptBlock.GetScriptBlock()
    $base = 'http://fixture.invalid'
    $script:createdTask = @{
        id = '750e7c3f-c840-498a-8980-d5d657882513'; status = 'todo'; priority = 'high'
        description = ''; dueDate = $null; createdAt = '2026-10-03T14:30:00Z'
    }
    function Invoke-Http { @{ Status = 201; Json = $script:createdTask; Content = '{}' } }
    $state = @{}
    Assert-That ((& $createTest) -eq $true) 'Valid task ID failed the create gate'
    Assert-That ($state.task.id -eq $script:createdTask.id) 'Create gate did not retain the task for later API gates'
    foreach ($invalidId in @($null, '', '   ')) {
        $script:createdTask.id = $invalidId
        $state = @{}
        $createError = $null
        try { & $createTest | Out-Null } catch { $createError = $_.Exception.Message }
        Assert-That ($null -ne $createError -and $createError -match 'missing id') 'Missing ID did not produce the intended assertion'
        Assert-That (-not $state.ContainsKey('task')) 'Rejected create response retained a task'
    }

    Import-ScenarioFunction 'scenario-a-vue-taskboard.ps1' 'Test-ApiContract'
    function New-DataFile { 'fixture-data.json' }
    function Start-Api { @{ Fixture = $true } }
    function Stop-BackgroundServer { }
    $script:httpRequests = [System.Collections.Generic.List[object]]::new()
    function Invoke-Http {
        param($Method, $Uri, $Body)
        $script:httpRequests.Add(@{ Method = $Method; Uri = $Uri })
        if ($Uri -like '*/health') { return @{ Status = 200; Json = @{ status = 'ok' }; Content = '{}' } }
        if ($Uri -like '*/does-not-exist') { return @{ Status = 404; Json = @{ error = @{ code = 'NOT_FOUND' } }; Content = '{}' } }
        if ($Body -is [string]) { return @{ Status = 400; Json = @{ error = @{ code = 'INVALID_JSON' } }; Content = '{}' } }
        if ($Body.title -eq 'Write release notes') { return @{ Status = 201; Json = $script:createdTask; Content = '{}' } }
        $field = if ($Body.ContainsKey('status')) { 'status' } elseif ($Body.ContainsKey('dueDate')) { 'dueDate' } else { 'title' }
        return @{ Status = 400; Json = @{ error = @{ code = 'VALIDATION_ERROR'; field = $field } }; Content = '{}' }
    }
    Test-ApiContract 'vue-create-dependency'
    Assert-Gate 'vue-create-dependency' 'api.create' 'fail'
    Assert-Gate 'vue-create-dependency' 'api.not-found' 'pass'
    foreach ($id in 'api.list', 'api.patch', 'api.filter', 'api.restart', 'api.persistence', 'api.delete') {
        Assert-Gate 'vue-create-dependency' $id 'skip'
        $blocked = @($ctx.Gates | Where-Object { $_.stage -eq 'vue-create-dependency' -and $_.id -eq $id })[-1]
        Assert-That ($blocked.required -and $blocked.detail -match 'api.create') "$id lost its required dependency result"
    }
    Assert-That (@($script:httpRequests | Where-Object { $_.Method -in @('PATCH', 'DELETE') -or $_.Uri -match '/tasks/$' }).Count -eq 0) 'Failed create caused malformed dependent requests'

    Import-ScenarioFunction 'scenario-b-aspnet-inventory.ps1' 'New-InventoryConnection'
    $connection = New-InventoryConnection 'Server=fixture;Initial Catalog=KeepMe;Database=KeepMeToo;Integrated Security=SSPI;Application Name="quoted ""name"""' 'VcpInventory_fixture'
    $builder = [System.Data.Common.DbConnectionStringBuilder]::new()
    $builder.set_ConnectionString($connection)
    Assert-That ($builder['Database'] -eq 'VcpInventory_fixture' -and -not $builder.ContainsKey('Initial Catalog')) 'SQL connection must target only the generated database'
    Assert-That ((ConvertTo-Json -InputObject $connection | ConvertFrom-Json) -ceq $connection) 'SQL connection JSON round-trip failed'
    foreach ($key in 'Password', 'Pwd', 'Access Token') {
        Assert-Rejected { New-InventoryConnection "Server=fixture;Integrated Security=True;$key=fixture-secret" 'VcpInventory_fixture' } "$key was accepted"
    }
    Assert-Rejected { New-InventoryConnection 'Server=fixture;User ID=someone' 'VcpInventory_fixture' } 'SQL authentication without integrated security was accepted'
    foreach ($key in 'AttachDBFilename', 'Initial File Name', 'Extended Properties') {
        Assert-Rejected { New-InventoryConnection "Server=fixture;Integrated Security=True;$key=C:\existing\user-data.mdf" 'VcpInventory_fixture' } "$key can redirect migrations to an existing database file"
    }

    Import-ScenarioFunction 'scenario-b-aspnet-inventory.ps1' 'Test-Migrations'
    $sqlcmd = 'fixture-sqlcmd.exe'; $useLocalDb = $true; $database = 'VcpInventory_fixture'
    $seededSkus = @('fixture-sku')
    $script:migrationCase = 'pass'; $script:seedCount = '10'
    $script:migrationCalls = [Collections.Generic.List[string]]::new()
    $script:seedCalls = 0
    function Invoke-Dotnet {
        param($Stage, $Label, $Arguments)
        $script:migrationCalls.Add($Label)
        if ($Label -eq 'ef-migrations-list') {
            if ($script:migrationCase -eq 'no-context') { return @{ ExitCode = 1; Output = ''; Errors = 'No DbContext was found' } }
            $output = if ($script:migrationCase -eq 'missing-migration') { 'No migrations were found' } else { '20261003000000_InitialCreate' }
            return @{ ExitCode = 0; Output = $output; Errors = '' }
        }
        $exitCode = if ($script:migrationCase -eq 'update-failed') { 1 } else { 0 }
        @{ ExitCode = $exitCode; Output = ''; Errors = 'fixture update result' }
    }
    function Invoke-Tool {
        param($Ctx, $Stage, $Label, $FilePath, $ArgumentList)
        $script:seedCalls++
        @{ ExitCode = 0; Output = $script:seedCount; Errors = '' }
    }
    foreach ($case in 'no-context', 'missing-migration', 'update-failed') {
        $script:migrationCase = $case
        $script:migrationCalls.Clear()
        Test-Migrations "migration-$case" @('InitialCreate')
        Assert-Gate "migration-$case" 'migrations' 'fail'
        Assert-Gate "migration-$case" 'db.seed' 'skip'
        $migrationGate = @($ctx.Gates | Where-Object { $_.stage -eq "migration-$case" -and $_.id -eq 'migrations' })[-1]
        $seedGate = @($ctx.Gates | Where-Object { $_.stage -eq "migration-$case" -and $_.id -eq 'db.seed' })[-1]
        Assert-That ($migrationGate.required -and -not $seedGate.required -and $seedGate.detail -match 'failed migrations') 'Migration failure or seed dependency lost its meaning'
        Assert-That ($script:seedCalls -eq 0) 'Failed migrations still queried an unverified database'
        if ($case -ne 'update-failed') {
            Assert-That ('ef-database-update' -notin $script:migrationCalls) 'Missing migrations still attempted database update'
        }
    }
    $script:migrationCase = 'pass'
    Test-Migrations 'migration-pass' @('InitialCreate')
    Assert-Gate 'migration-pass' 'migrations' 'pass'
    Assert-Gate 'migration-pass' 'db.seed' 'pass'
    Assert-That ($script:seedCalls -eq 1) 'Successful migrations did not verify seed data'
    $script:seedCount = '9'
    Test-Migrations 'migration-bad-seed' @('InitialCreate')
    Assert-Gate 'migration-bad-seed' 'migrations' 'pass'
    Assert-Gate 'migration-bad-seed' 'db.seed' 'fail'
    $sqlcmd = $null
    Test-Migrations 'migration-no-sqlcmd' @('InitialCreate')
    Assert-Gate 'migration-no-sqlcmd' 'migrations' 'pass'
    Assert-Gate 'migration-no-sqlcmd' 'db.seed' 'skip'
    Assert-That ($script:seedCalls -eq 2) 'Unavailable SQL tool still queried the database'

    Import-ScenarioFunction 'scenario-a-vue-taskboard.ps1' 'Test-UnitAndBuild'
    New-Item -ItemType Directory -Path (Join-Path $ws 'dist/client/assets') -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $ws 'dist/client/index.html') -Value '<html></html>'
    Set-Content -LiteralPath (Join-Path $ws 'dist/client/assets/app.js') -Value '// fixture'
    $script:reportPaths = [System.Collections.Generic.List[string]]::new()
    $script:junit = '<testsuites tests="1" failures="0" errors="0"><testsuite><testcase name="required"/></testsuite></testsuites>'
    $script:emitReport = $true
    function Invoke-Tool {
        param($Ctx, $Stage, $Label, $FilePath, $ArgumentList)
        $path = ([string]($ArgumentList | Where-Object { $_ -like '--outputFile=*' })).Substring('--outputFile='.Length)
        $script:reportPaths.Add($path)
        if ($script:emitReport) {
            New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
            Set-Content -LiteralPath $path -Value $script:junit
        }
        @{ ExitCode = 0; Output = ''; Errors = '' }
    }
    function Invoke-Npm { param($Stage, $Label, $Arguments) @{ ExitCode = 0; Output = ''; Errors = '' } }
    Test-UnitAndBuild 'vue' @() 1
    Assert-Gate 'vue' 'vitest' 'pass'
    $script:junit = '<testsuites tests="1" failures="0" errors="0"><testsuite><testcase name="required"><skipped/></testcase></testsuite></testsuites>'
    Test-UnitAndBuild 'vue-skipped' @() 1
    Assert-Gate 'vue-skipped' 'vitest' 'fail'
    $script:emitReport = $false
    Test-UnitAndBuild 'vue' @() 1
    Assert-Gate 'vue' 'vitest' 'fail'
    Assert-That (@($script:reportPaths | Select-Object -Unique).Count -eq 3) 'Vitest reused a stale report path'

    Import-ScenarioFunction 'scenario-b-aspnet-inventory.ps1' 'Test-Tests'
    $script:trxOutcome = 'Passed'
    $script:trxTestName = 'Suite.Required'
    $script:trxPaths = [System.Collections.Generic.List[string]]::new()
    function Invoke-Dotnet {
        param($Stage, $Label, $Arguments)
        $path = $Arguments[[array]::IndexOf($Arguments, '--results-directory') + 1]
        $script:trxPaths.Add($path)
        New-Item -ItemType Directory -Path $path -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $path 'first.trx') -Value '<TestRun><Results><UnitTestResult testName="Suite.First" outcome="Passed"/></Results></TestRun>'
        Set-Content -LiteralPath (Join-Path $path 'second.trx') -Value "<TestRun><Results><UnitTestResult testName=`"$script:trxTestName`" outcome=`"$script:trxOutcome`"/></Results></TestRun>"
        @{ ExitCode = 0; Output = ''; Errors = '' }
    }
    # TRX parsing fixtures represent assessments whose current build passed.
    # Failed/missing build admission is exercised in Campaign.Tests.ps1.
    [void](Add-GateResult $ctx 'dotnet' 'build' 'current build fixture' 'pass' '' $true)
    [void](Add-GateResult $ctx 'dotnet-suffix' 'build' 'current build fixture' 'pass' '' $true)
    Test-Tests 'dotnet' 2 @('Required')
    Assert-Gate 'dotnet' 'dotnet-test' 'pass'
    $script:trxTestName = 'Suite.NotRequired'
    Test-Tests 'dotnet-suffix' 2 @('Required')
    Assert-Gate 'dotnet-suffix' 'dotnet-test' 'fail'
    $script:trxTestName = 'Suite.Required'
    $script:trxOutcome = 'NotExecuted'
    Test-Tests 'dotnet' 2 @('Required')
    Assert-Gate 'dotnet' 'dotnet-test' 'fail'
    Assert-That (@($script:trxPaths | Select-Object -Unique).Count -eq 3) 'TRX reused a stale report directory'

    foreach ($name in 'Assert-Probability', 'Get-PredictionMetrics', 'Assert-ReportedMetrics', 'Test-Pytest') {
        Import-ScenarioFunction 'scenario-d-python-textlab.ps1' $name
    }
    foreach ($value in @($null, '', 'NaN', 'Infinity', '-0.1', '1.1')) {
        Assert-Rejected { Assert-Probability $value 'fixture' } "Invalid probability '$value' was accepted"
    }
    Assert-Probability 0 'lower bound'
    Assert-Probability 1 'upper bound'
    $expected = @(
        [pscustomobject]@{ id = '1'; category = 'a' }, [pscustomobject]@{ id = '2'; category = 'a' },
        [pscustomobject]@{ id = '3'; category = 'b' }, [pscustomobject]@{ id = '4'; category = 'b' })
    $predictions = @(
        [pscustomobject]@{ id = '1'; category = 'a'; category_confidence = '0.8' },
        [pscustomobject]@{ id = '2'; category = 'b'; category_confidence = '0.8' },
        [pscustomobject]@{ id = '3'; category = 'b'; category_confidence = '0.8' },
        [pscustomobject]@{ id = '4'; category = 'b'; category_confidence = '0.8' })
    $metrics = Get-PredictionMetrics $predictions $expected @('a', 'b') 'category'
    Assert-That ($metrics.accuracy -eq 0.75 -and [math]::Abs($metrics.macro_f1 - 0.733333333333) -lt 0.000001) 'Independent metrics disagree with hand-calculated fixture'
    Assert-That (($metrics.confusion_matrix[0] -join ',') -eq '1,1' -and ($metrics.confusion_matrix[1] -join ',') -eq '0,2') 'Confusion matrix differs from hand-calculated fixture'
    Assert-ReportedMetrics @{ accuracy = 0.75; macro_f1 = 0.733333333333 } $metrics 'fixture'
    Assert-Rejected { Assert-ReportedMetrics @{ accuracy = 1; macro_f1 = 1 } $metrics 'fixture' } 'Forged perfect metrics were accepted'
    Assert-Rejected { Get-PredictionMetrics @($predictions[1], $predictions[0], $predictions[2], $predictions[3]) $expected @('a', 'b') 'category' } 'Out-of-order predictions were accepted'
    $predictions[0].category = 'unknown'
    Assert-Rejected { Get-PredictionMetrics $predictions $expected @('a', 'b') 'category' } 'Invalid label was accepted'
    $predictions[0].category = 'a'; $predictions[0].category_confidence = $null
    Assert-Rejected { Get-PredictionMetrics $predictions $expected @('a', 'b') 'category' } 'Missing confidence was accepted'

    # Execute the real input-errors gate, including retained repair detail. The
    # missing-file check belongs to evaluate; predict checks a missing column.
    $tokens = $null; $errors = $null
    $textlabAst = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot 'scenario-d-python-textlab.ps1'), [ref]$tokens, [ref]$errors)
    Assert-That ($errors.Count -eq 0) 'TextLab scenario parse errors'
    $inputErrorsCommand = $textlabAst.Find({
        param($node)
        $node -is [System.Management.Automation.Language.CommandAst] -and
        $node.GetCommandName() -eq 'Invoke-Gate' -and
        "'input-errors'" -in $node.CommandElements.Extent.Text
    }, $true)
    Assert-That ($null -ne $inputErrorsCommand) 'Missing TextLab input-errors gate'
    $inputErrorsGate = [scriptblock]::Create($inputErrorsCommand.Extent.Text)
    $ctx.Temp = $testRoot
    $models = Join-Path $testRoot 'models with spaces'
    $missingColumnPath = Join-Path $testRoot 'missing text column.csv'
    $script:inputErrorCalls = [System.Collections.Generic.List[object]]::new()
    function Invoke-Python {
        param($Stage, $Label, $Arguments)
        $script:inputErrorCalls.Add(@{ Label = $Label; Arguments = @($Arguments) })
        $code = if ($Label -eq 'evaluate-missing') { $script:evaluateExit } else { $script:predictExit }
        @{ ExitCode = $code; Output = ''; Errors = 'fixture input error' }
    }
    foreach ($case in @(
        @{ Evaluate = 4; Predict = 4; Outcome = 'pass' },
        @{ Evaluate = 1; Predict = 4; Outcome = 'fail' },
        @{ Evaluate = 4; Predict = 2; Outcome = 'fail' },
        @{ Evaluate = 1; Predict = 2; Outcome = 'fail' }
    )) {
        $Stage = "input-errors-$($case.Evaluate)-$($case.Predict)"
        $script:evaluateExit = $case.Evaluate; $script:predictExit = $case.Predict
        $script:inputErrorCalls.Clear()
        & $inputErrorsGate | Out-Null
        Assert-Gate $Stage 'input-errors' $case.Outcome
        Assert-That ($script:inputErrorCalls.Count -eq 2) 'Input errors must still execute both checks'
        $evaluateArguments = @('-m', 'textlab', 'evaluate', '--model-dir', $models, '--data', (Join-Path $ctx.Temp 'nope.csv'), '--output', (Join-Path $ctx.Temp 'nope.json'))
        $predictArguments = @('-m', 'textlab', 'predict', '--model-dir', $models, '--input', $missingColumnPath, '--output', (Join-Path $ctx.Temp 'nope.csv'))
        foreach ($index in 0, 1) {
            $expectedArguments = if ($index -eq 0) { $evaluateArguments } else { $predictArguments }
            Assert-That ((ConvertTo-Json -InputObject $script:inputErrorCalls[$index].Arguments -Compress) -ceq (ConvertTo-Json -InputObject $expectedArguments -Compress)) 'Input-error command or fixture changed'
        }
        $gate = @($ctx.Gates | Where-Object { $_.stage -eq $Stage -and $_.id -eq 'input-errors' })[-1]
        Assert-That ($gate.required -and $gate.description -match 'evaluate --data' -and $gate.description -match 'predict --input') 'Input-error gate lost its required command identity'
        if ($case.Outcome -eq 'fail') {
            Assert-That ($gate.detail.Contains("evaluate --data (missing file): expected exit 4, actual $($case.Evaluate)")) 'Repair detail did not identify the evaluate failure accurately'
            Assert-That ($gate.detail.Contains("predict --input (CSV missing text column): expected exit 4, actual $($case.Predict)")) 'Repair detail did not identify the predict result accurately'
            foreach ($call in $script:inputErrorCalls) {
                Assert-That ($gate.detail.Contains((ConvertTo-Json -InputObject $call.Arguments -Compress))) 'Repair detail omitted exact observed arguments'
            }
        }
    }

    Import-ScenarioFunction 'scenario-d-python-textlab.ps1' 'Test-Robustness'
    $holdoutRows = $expected; $labels = @('a', 'b'); $holdoutPath = 'fixture.csv'; $edgePath = 'fixture-edge.csv'
    $script:confidenceFixture = 'valid'
    function Invoke-Python {
        param($Stage, $Label, $Arguments)
        if ($Label -eq 'predict-edge') { return @{ ExitCode = 4; Output = ''; Errors = 'unrelated edge gate fixture' } }
        $out = $Arguments[[array]::IndexOf($Arguments, '--output') + 1]
        $rows = @($holdoutRows | ForEach-Object { [pscustomobject]@{ id = $_.id; category = 'a'; category_confidence = '0.8' } })
        $rows[1].category_confidence = '0.999'
        if ($Label -eq 'predict-strict') {
            foreach ($row in $rows) { $row.category = 'needs_review' }
            $rows[1].category = 'a'
        }
        if ($script:confidenceFixture -eq 'truncated') { $rows = @($rows[0]) }
        if ($script:confidenceFixture -eq 'reordered') { [array]::Reverse($rows) }
        if ($script:confidenceFixture -eq 'invalid') { $rows[0].category_confidence = 'NaN' }
        if ($script:confidenceFixture -eq 'wrong-threshold' -and $Label -eq 'predict-strict') { $rows[0].category = 'a' }
        if ($script:confidenceFixture -eq 'wrong-boundary' -and $Label -eq 'predict-strict') { $rows[1].category = 'needs_review' }
        if ($script:confidenceFixture -eq 'changed-confidence' -and $Label -eq 'predict-strict') { $rows[0].category_confidence = '0.7' }
        if ($script:confidenceFixture -eq 'wrong-open-label' -and $Label -eq 'predict-open') { $rows[0].category = 'invalid' }
        New-Item -ItemType Directory -Path (Split-Path -Parent $out) -Force | Out-Null
        $rows | Export-Csv -LiteralPath $out -NoTypeInformation
        @{ ExitCode = 0; Output = ''; Errors = '' }
    }
    Test-Robustness 'confidence-valid' 'fixture-models'
    Assert-Gate 'confidence-valid' 'min-confidence' 'pass'
    foreach ($case in 'truncated', 'reordered', 'invalid', 'wrong-threshold', 'wrong-boundary', 'changed-confidence', 'wrong-open-label') {
        $script:confidenceFixture = $case
        Test-Robustness "confidence-$case" 'fixture-models'
        Assert-Gate "confidence-$case" 'min-confidence' 'fail'
    }

    $script:pytestSkipped = $false
    function Invoke-Python {
        param($Stage, $Label, $Arguments)
        $path = ([string]($Arguments | Where-Object { $_ -like '--junitxml=*' })).Substring('--junitxml='.Length)
        New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
        $child = if ($script:pytestSkipped) { '<skipped/>' } else { '' }
        Set-Content -LiteralPath $path -Value "<testsuites><testsuite tests=`"2`" failures=`"0`" errors=`"0`"><testcase name=`"other`"/><testcase name=`"required`">$child</testcase></testsuite></testsuites>"
        @{ ExitCode = 0; Output = ''; Errors = '' }
    }
    Test-Pytest 'pytest' 2 @('required')
    Assert-Gate 'pytest' 'pytest' 'pass'
    $script:pytestSkipped = $true
    Test-Pytest 'pytest' 1 @('required')
    Assert-Gate 'pytest' 'pytest' 'fail'

    foreach ($name in 'Format-Money', 'Get-Category', 'Compare-LedgerExport', 'Test-MavenVerify') {
        Import-ScenarioFunction 'scenario-c-java-ledger-cli.ps1' $name
    }
    $inv = [System.Globalization.CultureInfo]::InvariantCulture
    $rules = ,@('PAYROLL', 'Income')
    $ledgerExpected = @([pscustomobject]@{ Date = '2026-01-01'; Description = 'PAYROLL'; Amount = [decimal]1; Account = 'checking' })
    $ledgerActual = @([pscustomobject]@{ date = '2026-01-01'; description = 'PAYROLL'; amount = '1.00'; account = 'checking'; category = 'Income' })
    Compare-LedgerExport $ledgerActual $ledgerExpected
    foreach ($field in 'amount', 'account', 'category') {
        $before = $ledgerActual[0].$field
        $ledgerActual[0].$field = ''
        Assert-Rejected { Compare-LedgerExport $ledgerActual $ledgerExpected } "Altered export $field was accepted"
        $ledgerActual[0].$field = $before
    }
    $script:mavenSkipped = $false
    function Invoke-Maven {
        param($Stage, $Label, $Arguments)
        Assert-That (($Arguments -join ' ') -eq 'clean verify') 'Maven must clean stale reports'
        $path = Join-Path $ws 'target/surefire-reports'
        New-Item -ItemType Directory -Path $path -Force | Out-Null
        $child = if ($script:mavenSkipped) { '<skipped/>' } else { '' }
        Set-Content -LiteralPath (Join-Path $path 'TEST-fixture.xml') -Value "<testsuite name=`"fixture`" tests=`"2`" failures=`"0`" errors=`"0`"><testcase name=`"other`"/><testcase name=`"required`">$child</testcase></testsuite>"
        Set-Content -LiteralPath (Join-Path $ws 'target/ledger-cli-1.0.0-all.jar') -Value 'fixture'
        @{ ExitCode = 0; Output = ''; Errors = '' }
    }
    Test-MavenVerify 'maven' 2 @('required')
    Assert-Gate 'maven' 'mvn-verify' 'pass'
    $script:mavenSkipped = $true
    Test-MavenVerify 'maven' 1 @('required')
    Assert-Gate 'maven' 'mvn-verify' 'fail'

    Write-Host 'Scenario gate regressions passed (API create IDs, SQL safety, fresh reports, skipped tests, ledger exports, independent ML scoring, exact input-error commands).'
}
finally {
    $resolved = [System.IO.Path]::GetFullPath($testRoot)
    if ($resolved.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'vcp-scenario-gates-*') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
