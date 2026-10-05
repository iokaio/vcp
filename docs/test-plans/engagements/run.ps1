# SPDX-License-Identifier: Apache-2.0
# EE-07. Uses the ordinary scenario harness and public VCP commands.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('A','B')][string]$Kind,
    [Parameter(Mandatory)][string]$InputManifest,
    [Parameter(Mandatory)][string]$ProjectPath,
    [Parameter(Mandatory)][string]$RunRoot,
    [string]$ProviderGeneration,
    [switch]$ValidateOnly,
    [switch]$AllowProcessPublish,
    [ValidateRange(0,10)][int]$MaxRepairTurns = 2,
    [ValidateRange(1024,65534)][int]$Port = 41761
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path (Split-Path -Parent $PSScriptRoot) 'VcpScenarioHarness.psm1') -Force
. (Join-Path $PSScriptRoot 'scenario-contracts.ps1')
. (Join-Path $PSScriptRoot 'sql.ps1')
$node = Find-Executable -Name 'node'
Assert-That ([bool]$node) 'Node is required for independent engagement checks.'
$spec = Get-Content -LiteralPath $InputManifest -Raw | ConvertFrom-Json -AsHashtable
$proofFile = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($InputManifest))) ("engagement-prerequisites-$([guid]::NewGuid().ToString('N')).json")
$arguments = @((Join-Path $PSScriptRoot 'prerequisites.cjs'), $InputManifest, $proofFile)
if (-not $ValidateOnly) {
    Assert-That ([bool]$ProviderGeneration) 'ProviderGeneration is required for execution.'
    Assert-That $AllowProcessPublish 'Pass the existing explicit process authorization switch to run engagement profiles.'
    $arguments += @($Kind, [IO.Path]::GetFullPath($ProjectPath))
}
& $node @arguments
if ($LASTEXITCODE -ne 0) { throw 'Fresh passing A/B prerequisites or candidate identity failed; no inference/database operation started.' }
if ($ValidateOnly) { Write-Output $proofFile; return }
$proof = Get-Content -LiteralPath $proofFile -Raw | ConvertFrom-Json -AsHashtable
$ctx = Initialize-VcpScenario -Name "engagement-$($Kind.ToLowerInvariant())" -RunRoot $RunRoot -ProjectPath $ProjectPath `
    -Vcp $proof.candidate.executable -ProviderGeneration $ProviderGeneration -AllowProcessPublish:$AllowProcessPublish -MaxRepairTurns $MaxRepairTurns
$ws = $ctx.Workspace; $ApiPort = $Port; $AppPort = $Port; $PublishedPort = $Port + 1; $base = "http://127.0.0.1:$Port"
$protected = $proof[$Kind].protected
. (Get-OriginalScenarioContracts $Kind)
$complete = $false
function Require-Gates([string]$Stage) {
    $latest = @{}
    foreach ($gate in $ctx.Gates) {
        if ($gate.required -and ($gate.stage -replace '-repair\d+$','') -eq $Stage) { $latest[$gate.id] = $gate }
    }
    $failed = @($latest.Values | Where-Object outcome -ne 'pass')
    if ($failed.Count) { throw "Required gates failed at ${Stage}: $($failed.id -join ', ')" }
}
function Native-Check([string]$Stage, [string]$Label, [string[]]$Arguments) {
    $run = Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $node -ArgumentList $Arguments
    Assert-That ($run.ExitCode -eq 0) ("$Label failed: " + $run.Errors + $run.Output)
}
function Get-CurrentPauseVerification($Bundle, [string]$Task) {
    return @($Bundle.views.verification | ForEach-Object items | Where-Object {
        $record=$_.record
        $_.collection -eq 'verification' -and $_.visibility -eq 'available' -and $record.scope.task -eq $Task -and
        $record.scope.workspace -eq $Bundle.task.scope.workspace -and $record.scope.session -eq $Bundle.task.scope.session -and
        $record.steering -eq $Bundle.task.steering -and $record.fingerprint.repository -match '^[a-f0-9]{64}$' -and
        ($record.fingerprint | ConvertTo-Json -Compress) -ceq ($Bundle.task.fingerprint | ConvertTo-Json -Compress) -and
        $record.checks.Count -gt 0 -and @($record.checks | Where-Object { $_.outcome.status -ne 'passed' -or $_.exit_code -ne 0 }).Count -eq 0 -and
        @($Bundle.task.required_checks | Where-Object { $_ -notin $record.checks.specification }).Count -eq 0 -and
        $record.outputs.Count -gt 0 -and $record.outstanding_issues.Count -eq 0 -and $record.unresolved_effects.Count -eq 0
    })
}
function Start-EngagementApp([string]$Stage, [string]$DataFile, [switch]$Published) {
    if ($Kind -eq 'A') { return Start-Api $Stage 'engagement-api' $DataFile }
    return Start-App $Stage 'engagement-api' $AppPort -Published:$Published
}
function Test-EngagementApi([string]$Stage, [string]$Phase = 'full', [switch]$Browser, [switch]$Published) {
    $id = [guid]::NewGuid().ToString('N'); $state = Join-Path $ctx.Results "$Stage-$id-state.json"
    $dataFile = if ($Kind -eq 'A') { New-DataFile $Stage 'engagement' } else { '' }
    $server = Start-EngagementApp $Stage $dataFile -Published:$Published
    try { Native-Check $Stage 'independent-api' @((Join-Path $PSScriptRoot 'acceptance.cjs'), $Kind, $Phase, $base, $state, (Join-Path $ctx.Results "$Stage-$id-api.json")) }
    finally { Stop-BackgroundServer $server }
    $server = Start-EngagementApp "$Stage-restart" $dataFile -Published:$Published
    try {
        $restart = if ($Phase -eq 'export') { 'restart-export' } else { 'restart' }
        Native-Check $Stage 'independent-restart' @((Join-Path $PSScriptRoot 'acceptance.cjs'), $Kind, $restart, $base, $state, (Join-Path $ctx.Results "$Stage-$id-restart.json"))
        if ($Kind -eq 'B') { Test-AdjustmentSql $state "$Stage-$id" }
        if ($Browser) {
            $output = Join-Path $ctx.Results "$Stage-$id-browser"
            Native-Check $Stage 'independent-browser' @((Join-Path $PSScriptRoot 'browser.cjs'), $Kind, $base, $state, $output, $(if($Stage.StartsWith('FINAL')){'require-help'}else{'initial'}))
            if ($Kind -eq 'B') {
                $report = Get-Content -LiteralPath (Join-Path $output 'report.json') -Raw | ConvertFrom-Json -AsHashtable
                $browserState = Join-Path $output 'sql-state.json'
                Write-JsonFile $browserState @{schema='engagement-b-state/1';productId=$report.final_application_state.productId;expected=$report.final_application_state}
                Test-AdjustmentSql $browserState "$Stage-$id-browser"
            }
        }
    } finally { Stop-BackgroundServer $server }
    $script:lastApiState = $state; $script:lastApiData = $dataFile
    return $true
}
function Test-Original([string]$Stage, [switch]$Published) {
    if ($Kind -eq 'A') {
        Test-Typecheck $Stage; Test-NodeTests $Stage $namesT5
        Test-UnitAndBuild $Stage ($uiIds + @('label-chip','sort-select','stats-bar')) 5
        Test-ApiContract $Stage; Test-LabelsAndSort $Stage; Test-Production $Stage
    } else {
        Test-Build $Stage; Test-Tests $Stage 22 $namesT5
        Test-Migrations $Stage @('InitialCreate','AddProductRowVersion')
        Invoke-RuntimeGates $Stage { param($p) Test-ApiContract $Stage $p; Test-RazorPages $Stage $p; Test-Concurrency $Stage $p } -Published:$Published
    }
    Test-ProtectedUnchanged $Stage $protected
}
function Run-EngagementStage([string]$Stage, [string]$Body, [scriptblock]$Checks) {
    $result = Invoke-VcpTask -Ctx $ctx -Stage $Stage -Title $Stage -Prompt ($Body + "`n" + $instructions) -Config $profile
    Assert-That ($null -ne $result) 'Stage was not actually dispatched.'
    Test-StageExit $ctx $result $Stage
    & $Checks $Stage
    [void](Invoke-RepairLoop -Ctx $ctx -Stage $Stage -Config $profile -GateScript $Checks)
    Require-Gates $Stage
    Save-Checkpoint $ctx "$Stage verified"
}
function Test-FaultOracle([string]$Label, [switch]$ExpectFailure) {
    $server = Start-EngagementApp $Label $lastApiData
    try {
        $report = Join-Path $ctx.Results "$Label-$([guid]::NewGuid().ToString('N')).json"
        $run = Invoke-Tool -Ctx $ctx -Stage $Label -Label 'negative-oracle' -FilePath $node -ArgumentList @(
            (Join-Path $PSScriptRoot 'acceptance.cjs'),$Kind,'fault',$base,$lastApiState,$report)
        Assert-That (Test-Path -LiteralPath $report) 'Fault oracle did not produce a report.'
        $record = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
        if ($ExpectFailure) {
            # A start/build/connection failure does not demonstrate the intended negative case.
            $post = @($record.exchanges | Where-Object { $_.method -eq 'POST' })
            Assert-That ($run.ExitCode -ne 0 -and $record.status -eq 'failed' -and $post.Count -eq 1 -and $post[0].result.status -in @(200,201)) 'Injection did not exercise the declared accepted-invalid-input defect.'
        } else { Assert-That ($run.ExitCode -eq 0 -and $record.status -eq 'passed') 'Negative oracle failed.' }
    } finally { Stop-BackgroundServer $server }
    return $true
}
try {
    $inputs = [ordered]@{ schema='vcp-engagement-run-inputs/1'; prerequisites=$proof; kind=$Kind; candidate=$proof.candidate; scripts=[ordered]@{}; at=[DateTimeOffset]::UtcNow.ToString('o') }
    foreach ($file in @((Get-ChildItem -LiteralPath $PSScriptRoot -File | Where-Object Extension -in '.ps1','.cjs','.md').FullName) + @(
        (Join-Path (Split-Path -Parent $PSScriptRoot) 'VcpScenarioHarness.psm1'),
        ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../scripts/evals/analyze-execution-bundle.cjs'))),
        (Join-Path (Split-Path -Parent $PSScriptRoot) $(if ($Kind -eq 'A') {'scenario-a-vue-taskboard.ps1'}else{'scenario-b-aspnet-inventory.ps1'})))) {
        $inputs.scripts[$file] = Get-Sha256 $file
    }
    Write-JsonFile (Join-Path $ctx.Results 'run-inputs.json') $inputs
    Invoke-CommonPreflight $ctx
    Require-Gates 'P0-preflight'
    Assert-That ($ctx.VcpVersion -match [regex]::Escape($proof.candidate.version)) 'Actual executable version differs from the admitted candidate.'
    Assert-That ((Get-Sha256 $ctx.Vcp) -eq $proof.candidate.sha256) 'Candidate changed after prerequisite validation.'
    $nodeProcess = New-ProcessProfile -Name 'node' -Executable $node -Ctx $ctx
    $checkpointProcess = New-ProcessProfile -Name 'pause-checkpoint' -Executable $node -Ctx $ctx -MaxTimeoutMs 300000
    if ($Kind -eq 'A') {
        $npmCli = Join-Path (Split-Path -Parent $node) 'node_modules/npm/bin/npm-cli.js'
        Assert-That (Test-Path -LiteralPath $npmCli) 'npm CLI is required.'
        $install = Invoke-Npm 'B0-dependencies' 'npm-ci' @('ci','--no-audit','--no-fund')
        Assert-That ($install.ExitCode -eq 0) 'Baseline npm ci failed.'
        $processes = @($nodeProcess,$checkpointProcess); $checks = @(New-NodeCheck $namesT5 0)
    } else {
        $dotnet = Find-Executable -Name 'dotnet'; $sqlcmd = Find-Executable -Name 'sqlcmd'
        if (-not $sqlcmd) { $sqlcmd = 'C:\Program Files\Microsoft SQL Server\Client SDK\ODBC\170\Tools\Binn\SQLCMD.EXE' }
        Assert-That ($dotnet -and (Test-Path -LiteralPath $sqlcmd)) 'dotnet and SQLCMD are required; SQL evidence cannot be skipped.'
        $webProject = 'src/Inventory.Web/Inventory.Web.csproj'
        Initialize-EngagementDatabase
        $dotnetProcess = New-ProcessProfile -Name 'dotnet' -Executable $dotnet -Ctx $ctx -MaxTimeoutMs 1200000
        $dotnetProcess.environment['ConnectionStrings__Inventory'] = $connection
        $dotnetProcess.environment['ASPNETCORE_ENVIRONMENT'] = 'Development'
        foreach ($name in @('ProgramFiles','ProgramFiles(x86)')) { if ([Environment]::GetEnvironmentVariable($name)) { $dotnetProcess.environment[$name] = [Environment]::GetEnvironmentVariable($name) } }
        foreach ($name in @('APPDATA','LOCALAPPDATA','DOTNET_CLI_HOME')) {
            $directory = Join-Path $ctx.Env $name; New-Item -ItemType Directory -Path $directory | Out-Null; $dotnetProcess.environment[$name] = $directory
        }
        $processes = @($dotnetProcess,$checkpointProcess); $checks = @(New-DotnetCheck $namesT5 0)
        $restore = Invoke-Dotnet 'B0-dependencies' 'tools-restore' @('tool','restore')
        Assert-That ($restore.ExitCode -eq 0) 'dotnet tools restore failed.'
    }
    $profile = New-ScenarioProfile -Ctx $ctx -Name 'engagement' -AffectedPaths @('README.md','server','src','tests','package.json','engagement-fault.json') -Processes $processes -Checks $checks
    $instructions = @"
Preserve every original A/B acceptance contract. Protected bytes: $($protected.Keys -join ', '). Never edit those files or the independent harness. This is a copied, passing application, not a scaffold request. Add focused tests, run all existing tests and build, then call vcp_verify and resolve its outstanding_issues before finishing. Never claim browser/SQL inspection that you did not perform. Do not commit, publish or push. Use only registered process profiles. For Inventory, the dotnet profile overrides ConnectionStrings__Inventory to the isolated engagement copy; never connect to the original database or change its protected config. Retain original routes and migrations.
"@
    # Verify exact original acceptance on the copied workspace before adding features.
    Test-Original 'B0-original'; Require-Gates 'B0-original'
    if ($Kind -eq 'B') { $script:copyRows = Get-DatabaseEvidence $database 'copy-before-feature-migration' }
    $contract = Get-Content -LiteralPath (Join-Path $PSScriptRoot "contract-$Kind.md") -Raw
    $instructions += "`n" + $contract
    if ($Kind -eq 'A') {
        Run-EngagementStage 'EA1-export' 'Implement the versioned deterministic full-field JSON export and its tests.' {
            param($s) Test-Typecheck $s; Test-NodeTests $s $namesT5
            [void](Invoke-Gate $ctx $s 'export-restart' 'Independent export and process restart' { Test-EngagementApi $s 'export' })
            Test-ProtectedUnchanged $s $protected
        }
        Run-EngagementStage 'EA2-import' 'Implement atomic validated import/merge, repeat idempotency, conflict behavior and tests.' {
            param($s) Test-Typecheck $s; Test-NodeTests $s $namesT5
            [void](Invoke-Gate $ctx $s 'import-integrity' 'Independent import, rollback, field integrity and restart' { Test-EngagementApi $s })
            Test-ProtectedUnchanged $s $protected
        }
    } else {
        Run-EngagementStage 'EB1-schema' 'Add the stock adjustment audit entity and migration. Preserve all original database rows and all migrations. Do not implement startup auto-migration.' {
            param($s) Test-Build $s; Test-Tests $s 22 $namesT5; Test-Migrations $s @('InitialCreate','AddProductRowVersion','AddStockAdjustments')
            [void](Invoke-Gate $ctx $s 'migration-preservation' 'Copied original database rows survive the new migration' {
                $after = Get-DatabaseEvidence $database 'copy-after-migration'; Write-JsonFile (Join-Path $ctx.Results 'copied-database-after.json') $after
                Test-DatabasePreserved $copyRows $after -AllowMigrationAppend; $true
            })
            Test-ProtectedUnchanged $s $protected
        }
        Run-EngagementStage 'EB2-api' 'Implement transactional stock adjustment, audit history, global idempotency and concurrency with tests.' {
            param($s) Test-Build $s; Test-Tests $s 22 $namesT5
            [void](Invoke-Gate $ctx $s 'adjustment-integrity' 'Independent API, SQL, concurrency, idempotency and restart' { Test-EngagementApi $s })
            Test-ProtectedUnchanged $s $protected
        }
    }
    Run-EngagementStage "E${Kind}3-browser" 'Implement the accessible browser UI, visible validation/progress/summary, and browser-facing tests. Add the declared fault manifest described below.' {
        param($s) Test-Original $s
        [void](Invoke-Gate $ctx $s 'browser-and-api' 'Actual browser keyboard, feedback, API and persistence checks' { Test-EngagementApi $s -Browser })
    }
    $checkpoint = New-ScenarioPauseCheckpoint $ctx -Profile 'pause-checkpoint'
    $checkpoint = $checkpoint.Replace('Before any other tool work,', 'After the requested UI edit and successful vcp_verify evidence,')
    $sourceBeforePause = Get-WorkspaceManifest $ws
    $pauseEdit = if ($Kind -eq 'A') { 'Add visible help beside the Vue import/export controls with data-testid="transfer-format-help", explaining JSON format version 1, preserved task fields, and whole-document conflict rejection.' }
        else { 'Add visible help to the Razor adjustment form with data-testid="adjustment-help", explaining stable operation IDs, idempotent retries, concurrency and atomic stock/audit updates.' }
    $pause = Invoke-VcpTask -Ctx $ctx -Stage "E${Kind}4-pause" -Title 'Edit, verify, explicit pause and reopen' -Config $profile -PauseAfterProgress -AcceptExit @(8) `
        -Prompt ("$pauseEdit Add its focused UI test and verify the edit before reaching the checkpoint. After resume, document the implemented format/API, repeat/restart and recovery behavior in README.md, verify again, and finish.`n$checkpoint`n$instructions")
    Assert-That ($null -ne $pause) 'Pause task did not run.'
    Test-StageExit $ctx $pause "E${Kind}4-pause"
    [void](Invoke-Gate $ctx "E${Kind}4-pause" 'explicit-pause-reopen' 'Acknowledged pause, real edit and verification, stable read-only reopen' {
        Assert-That ($pause.explicit_pause.acknowledged -and $pause.exit_code -eq 8 -and $pause.conditions -contains 'durably_paused') 'Pause lacks scoped terminal proof.'
        $prior = Get-Content -LiteralPath (Join-Path $ctx.Logs "E${Kind}4-pause/inspection-bundle.json") -Raw | ConvertFrom-Json
        $manifest = Get-WorkspaceManifest $ws
        $pausedDatabase = if ($Kind -eq 'B') { Get-DatabaseEvidence $database 'pause-before-readonly-reopen' } else { $null }
        $a = Invoke-Vcp -Ctx $ctx -Stage "E${Kind}4-reopen" -Label 'first' -Arguments @('inspect-bundle',$pause.task) -DenyProviderCredentials
        $b = Invoke-Vcp -Ctx $ctx -Stage "E${Kind}4-reopen" -Label 'second' -Arguments @('inspect-bundle',$pause.task) -DenyProviderCredentials
        Assert-That ($a.ExitCode -eq 0 -and $b.ExitCode -eq 0) 'Read-only reopen failed.'
        Write-JsonFile (Join-Path $ctx.Results 'reopen-first.json') $a.Result.data
        Write-JsonFile (Join-Path $ctx.Results 'reopen-second.json') $b.Result.data
        Assert-That ($prior.source_watermark -eq $a.Result.data.source_watermark -and $a.Result.data.source_watermark -eq $b.Result.data.source_watermark) 'Read-only reopen changed canonical state.'
        Assert-That ((Compare-WorkspaceManifest $manifest (Get-WorkspaceManifest $ws)).Changed -eq 0) 'Read-only reopen changed workspace bytes.'
        if ($Kind -eq 'B') { Test-DatabasePreserved $pausedDatabase (Get-DatabaseEvidence $database 'pause-after-readonly-reopen') }
        $sourceDelta = Compare-WorkspaceManifest $sourceBeforePause $manifest
        $uiPrefix = if ($Kind -eq 'A') {'src/'} else {'src/Inventory.Web/Pages/'}
        Assert-That (@(($sourceDelta.Added + $sourceDelta.Modified) | Where-Object { $_.StartsWith($uiPrefix) }).Count -gt 0) 'Pause preceded the requested application UI edit.'
        $verified = @(Get-CurrentPauseVerification $prior $pause.task)
        Assert-That ($verified.Count -gt 0) 'No retained successful same-task verification evidence before pause.'
        $true
    })
    Require-Gates "E${Kind}4-pause"
    $resume = Invoke-VcpContinuation -Ctx $ctx -Stage "E${Kind}4-resume" -Title 'Intentional same-task resume' -Config $profile -Arguments @('resume',$pause.task)
    Assert-That ($resume -and $resume.task -eq $pause.task) 'Intentional resume changed task identity.'
    Test-StageExit $ctx $resume "E${Kind}4-resume"; Require-Gates "E${Kind}4-resume"
    [void](Test-EngagementApi "E${Kind}5-before-fault")
    [void](Test-FaultOracle "E${Kind}5-negative-before")
    Save-Checkpoint $ctx "E${Kind}5: passing checkpoint before declared fault"
    Native-Check "E${Kind}5-fault" 'declared-injection' @((Join-Path $PSScriptRoot 'fault.cjs'),$Kind,$ws,(Join-Path $ws 'engagement-fault.json'),(Join-Path $ctx.Results 'declared-fault'))
    if ($Kind -eq 'B') { Test-Build 'fault-build'; Require-Gates 'fault-build' }
    [void](Test-FaultOracle "E${Kind}5-negative-injected" -ExpectFailure)
    Run-EngagementStage "E${Kind}5-repair" 'The external harness deliberately injected the declared validation defect. Inspect engagement-fault.json and repair the production guard. Preserve all oracle/tests/protected files. Add a regression test and verify. The failed negative response is a real acceptance failure, not a reason to loosen the contract.' {
        param($s) if ($Kind -eq 'A') { Test-Typecheck $s; Test-NodeTests $s $namesT5 } else { Test-Build $s; Test-Tests $s 22 $namesT5 }
        [void](Invoke-Gate $ctx $s 'negative-repaired' 'The unchanged independent negative oracle passes after repair' { Test-FaultOracle "$s-negative" })
        Test-ProtectedUnchanged $s $protected
    }
    Run-EngagementStage "E${Kind}6-finalize" 'Prepare local build/publish artifacts and complete documentation, retaining all original behavior. Verify all tests. Do not publish externally.' { param($s) Test-Original $s }
    if ($Kind -eq 'B') {
        [void](Invoke-Gate $ctx 'FINAL' 'publish' 'Local Release publish produces Inventory.Web.exe' {
            $publish = Invoke-Dotnet 'FINAL' 'publish' @('publish',$webProject,'-c','Release','-o',(Join-Path $ws 'artifacts/publish'))
            Assert-That ($publish.ExitCode -eq 0 -and (Test-Path -LiteralPath (Join-Path $ws 'artifacts/publish/Inventory.Web.exe'))) 'Local publish failed or executable is absent.'
            Add-Asset $ctx (Join-Path $ws 'artifacts/publish/Inventory.Web.exe') 'Actual published executable used for independent smoke checks'
            $true
        })
        Require-Gates 'FINAL'
        Test-Original 'FINAL' -Published
        [void](Invoke-Gate $ctx 'FINAL' 'migration-script' 'EF Core generates a nonempty idempotent migration script' {
            $sql = Join-Path $ws 'artifacts/migrations.sql'
            $run = Invoke-Dotnet 'FINAL' 'ef-script' @('tool','run','dotnet-ef','migrations','script','--idempotent','--project','src/Inventory.Web','--output',$sql)
            Assert-That ($run.ExitCode -eq 0 -and (Test-Path -LiteralPath $sql) -and (Get-Item -LiteralPath $sql).Length -gt 0) 'Idempotent migration script failed or is empty.'
            Add-Asset $ctx $sql 'Idempotent SQL migration script'
            $true
        })
        [void](Invoke-Gate $ctx 'FINAL' 'published-engagement' 'Published app independently passes API/browser/SQL/restart' { Test-EngagementApi 'FINAL-published' -Browser -Published })
        [void](Invoke-Gate $ctx 'FINAL' 'fresh-database' 'All migrations and engagement checks work on a fresh isolated database' {
            $copyConnection = $connection; $copyDatabase = $database
            try {
                $script:connection = $freshConnection; $script:database = $freshDatabase
                Test-Migrations 'FINAL-fresh' @('InitialCreate','AddProductRowVersion','AddStockAdjustments'); Require-Gates 'FINAL-fresh'
                [void](Test-EngagementApi 'FINAL-fresh' -Published)
            } finally { $script:connection = $copyConnection; $script:database = $copyDatabase }
            $true
        })
        [void](Invoke-Gate $ctx 'FINAL' 'original-database-untouched' 'Original source database remained unchanged' {
            $after = Get-DatabaseEvidence $originalDatabase 'original-final'; Write-JsonFile (Join-Path $ctx.Results 'original-database-after.json') $after
            Test-DatabasePreserved $originalDatabaseRows $after; $true
        })
    } else {
        [void](Invoke-Gate $ctx 'FINAL' 'npm-ci' 'Clean install reproduces dependencies from the lockfile' {
            $install = Invoke-Npm 'FINAL' 'clean-install' @('ci','--no-audit','--no-fund'); Assert-That ($install.ExitCode -eq 0) 'Final clean install failed.'; $true
        })
        Test-Original 'FINAL'
        [void](Invoke-Gate $ctx 'FINAL' 'engagement' 'Final API/browser/restart checks pass against rebuilt application' { Test-EngagementApi 'FINAL' -Browser })
    }
    Require-Gates 'FINAL'; Save-Checkpoint $ctx 'FINAL: verified engagement state'; $complete = $true
} catch {
    $ctx.Fatal = $_.Exception.Message
    [void](Add-GateResult $ctx 'FINAL' 'engagement-complete' 'Every declared engagement stage and mandatory evidence check completed' 'fail' $_.Exception.Message $true)
} finally {
    $exitCode = Complete-VcpScenario $ctx
    $complete = $complete -and $exitCode -eq 0
    $mapping = if ($Kind -eq 'A') { @(@('EA1'),@('EA2'),@('EA3'),@('EA4-pause','EA4-resume'),@('EA5-repair'),@('EA6','FINAL')) }
        else { @(@('EB1'),@('EB2'),@('EB2'),@('EB3','EB4-pause','EB4-resume'),@('EB5-repair'),@('EB6','FINAL')) }
    $criteria = for ($n=0; $n -lt 6; $n++) {
        $rows = @($ctx.Gates | Where-Object { $stageName=$_.stage; $_.id -ne 'engagement-complete' -and @($mapping[$n] | Where-Object { $stageName.StartsWith($_) }).Count -gt 0 })
        $latest = @($rows | Where-Object required | Group-Object { ($_.stage -replace '-repair\d+$','') + '|' + $_.id } | ForEach-Object { $_.Group[-1] })
        $missing = @($mapping[$n] | Where-Object { $prefix=$_; @($rows | Where-Object { $_.stage.StartsWith($prefix) }).Count -eq 0 })
        $status = if (@($latest | Where-Object outcome -eq 'fail').Count) {'failed'} elseif ($rows.Count -eq 0) {'untested'} elseif ($missing.Count -or @($latest | Where-Object outcome -ne 'pass').Count) {'blocked'} else {'passed'}
        [ordered]@{ criterion="$Kind$($n+1)"; status=$status; stages=@($mapping[$n]); missing_stage_evidence=$missing; gates=$latest;
            artifact_roots=@($ctx.Logs,$ctx.Results); note=$(if($Kind -eq 'B' -and $n -eq 2){'Global idempotency/restart/concurrency is independently exercised in EB2.'}else{''}) }
    }
    Write-JsonFile (Join-Path $ctx.Results 'engagement-matrix.json') ([ordered]@{ schema='vcp-engagement-matrix/1'; kind=$Kind; completed=$complete; status=$(if($complete){'passed'}else{'failed'}); criteria=@($criteria); gates=@($ctx.Gates); stages=@($ctx.Stages); limitations=@('Hosted/real engagement acceptance is established only by this actual run, not offline harness fixtures.'); cleanup='all evidence and databases retained' })
}
exit $exitCode
