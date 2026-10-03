#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Exercise real scenario initialization failure paths without CLI or toolchain processes.
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking

function Find-Executable { return $null }
function Get-Command { param($Name, $CommandType, $ErrorAction) return $null }
function Invoke-CommonPreflight { throw 'Unexpected CLI invocation after toolchain failure.' }

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-initialization-' + [guid]::NewGuid().ToString('N'))
try {
    foreach ($file in Get-ChildItem -LiteralPath $scenarioRoot -Filter 'scenario-*.ps1') {
        $root = Join-Path $testRoot $file.BaseName
        New-Item -ItemType Directory -Path $root -Force | Out-Null
        $ctx = @{
            Name = $file.BaseName; RunId = 'fixture'; Root = $root; Workspace = (Join-Path $root 'workspace')
            Logs = $root; Results = $root; ProgressLog = (Join-Path $root 'progress.log')
            Started = Get-Date; Fatal = $null; Transcript = $true; SkipPaidStages = $true
            SpentUsd = [decimal]0; MaxScenarioUsd = [decimal]30; CostUnknown = $false
            AccountedTaskUsd = @{}; TaskBudgetUsd = @{}
            Stages = [Collections.Generic.List[object]]::new(); Gates = [Collections.Generic.List[object]]::new()
            Assets = [Collections.Generic.List[object]]::new(); Notes = [Collections.Generic.List[string]]::new()
        }
        $tokens = $null; $errors = $null
        $ast = [Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
        Assert-That ($errors.Count -eq 0) "Parse errors in $($file.Name)"
        $run = $ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] } | Select-Object -First 1
        Assert-That ($null -ne $run) "Missing lifecycle try block in $($file.Name)"
        # Execute the actual try/catch/finally. Exclude imports, context initialization
        # and the final process exit; the inert context above supplies their boundary.
        $transcript = Join-Path $root 'transcript.txt'
        Start-Transcript -LiteralPath $transcript | Out-Null
        $exitCode = & ([scriptblock]::Create($run.Extent.Text + "`nreturn `$exitCode"))
        Assert-That ($exitCode -eq 1) "$($file.Name) did not return a failure exit"
        $scorecard = Get-Content -LiteralPath (Join-Path $root 'scorecard.json') -Raw | ConvertFrom-Json
        Assert-That ($scorecard.verdict -eq 'fail' -and $scorecard.fatal -match 'required') "$($file.Name) did not retain the missing toolchain reason"
        Assert-That ($scorecard.spend_usd -eq 0 -and @($scorecard.stages).Count -eq 0) 'Initialization failure started a paid stage'
        Assert-That (Test-Path -LiteralPath (Join-Path $root 'summary.md')) 'Failure summary was not written'
        $closedTranscript = Get-Content -LiteralPath $transcript -Raw
        Write-Host 'Initialization transcript closure probe'
        Assert-That ((Get-Content -LiteralPath $transcript -Raw) -ceq $closedTranscript) 'Finalization left the transcript active'
    }
    Write-Host 'Scenario initialization failures passed: all four preserve zero-spend failure scorecards and close transcripts.'
}
finally {
    try { Stop-Transcript | Out-Null } catch { }
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if ($resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
