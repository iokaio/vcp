#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Extract only the trusted gate function; never execute scenario top-level code.
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$file = Join-Path $repository 'docs/test-plans/scenario-b-aspnet-inventory.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw "Scenario parsing failed: $errors" }
$gate = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Test-DomainModel' }, $true))
if ($gate.Count -ne 1) { throw 'Expected exactly one independent domain gate.' }
# A parsed ScriptBlock has no defining file; supply the same trusted script root.
$scenarioRootLiteral = "'" + ((Split-Path -Parent $file) -replace "'", "''") + "'"
. ([scriptblock]::Create($gate[0].Extent.Text.Replace('$PSScriptRoot', $scenarioRootLiteral)))
$t1 = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left -is [Management.Automation.Language.VariableExpressionAst] -and $n.Left.VariablePath.UserPath -eq 'gatesT1' }, $true))
$commands = @($t1[0].FindAll({ param($n) $n -is [Management.Automation.Language.CommandAst] }, $true) | ForEach-Object GetCommandName)
if (($commands -join ',') -ne 'Test-Build,Test-Tests,Test-DomainModel,Test-Migrations,Test-ProtectedUnchanged') { throw 'T1 gate ordering or original acceptance changed.' }
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-domain-gate-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $ctx = @{ Logs = $temporary; Gates = @([pscustomobject]@{stage='T1-data';id='build';outcome='pass'});
        Stages = @([pscustomobject]@{stage='T1-data';task='task';Run=@{ExitCode=0;TimedOut=$false;InvalidLines=0;Scope=@{task='task'};StdoutPath='completed.jsonl'}}) }
    $ws = Join-Path $temporary 'workspace'; $dotnet = 'dotnet'; $tfm = 'net10.0'
    $script:calls = @(); $script:failedModel = $true; $script:gateFailure = $null; $script:assets = @()
    function Assert-That([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
    function Get-Tail([string]$Text) { $Text }
    function Invoke-Gate { param($Ctx,$Stage,$Id,$Description,[scriptblock]$Test)
        if ($Id -ne 'domain-model') { throw 'Wrong gate identity' }
        try { & $Test | Out-Null } catch { $script:gateFailure = $_.Exception.Message }
    }
    function Add-Asset { param($Ctx,$Path,$Description) $script:assets += $Path }
    function Invoke-Tool { param($Ctx,$Stage,$Label,$FilePath,$ArgumentList)
        $script:calls += [pscustomobject]@{label=$Label;arguments=$ArgumentList}
        if ($Label -eq 'domain-probe') {
            $report = $ArgumentList[-1]
            @{schema='vcp-inventory-domain-probe/1';passed=(-not $script:failedModel);checks=@(@{Id='reason-exact-enum';Passed=(-not $script:failedModel)})} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $report
            return @{ExitCode=$(if($script:failedModel){1}else{0});Output='';Errors=''}
        }
        return @{ExitCode=0;Output='';Errors=''}
    }
    Test-DomainModel 'T1-data'
    Assert-That ($script:calls.Count -eq 2 -and $script:assets.Count -eq 1) "Expected evaluator build/run and retained failure evidence: $script:gateFailure"
    Assert-That ($script:gateFailure -match 'reason-exact-enum') 'Independent failures must enter ordinary gate repair feedback.'
    Assert-That ($script:calls[1].arguments -contains 'completed.jsonl') 'Exact native stage evidence must be passed.'
    Assert-That ($script:calls[0].arguments -contains '-p:TargetFramework=net10.0') 'Evaluator must target selected scenario framework.'
    $script:failedModel = $false; $script:gateFailure = $null
    Test-DomainModel 'T1-data'
    Assert-That ($null -eq $script:gateFailure) 'A passing independent model must satisfy the gate.'
    $script:calls = @(); $script:gateFailure = $null; $ctx.Gates[0].outcome = 'fail'
    Test-DomainModel 'T1-data'
    Assert-That ($script:calls.Count -eq 0 -and $script:gateFailure -match 'fresh successful') 'Failed application build must deny evaluator execution.'
    $ctx.Gates[0].outcome = 'pass'; $ctx.Stages[0].Run.ExitCode = 7; $script:gateFailure = $null
    Test-DomainModel 'T1-data'
    Assert-That ($script:calls.Count -eq 0 -and $script:gateFailure -match 'completed successful') 'Unresolved native stage must deny evaluator execution.'
    Write-Output 'Independent domain gate wiring: 9 assertions passed; no app, database or provider execution.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected fixture cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
