#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Evaluate only the optional helper with mocks; never run paid scenario top-level code.
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$file = Join-Path $repository 'docs/test-plans/execution-engine-probe.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw "Probe parsing failed: $errors" }
$helper = @($ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Test-SecondInvocation' }, $true))
if ($helper.Count -ne 1) { throw 'Expected one second invocation helper.' }
$rootLiteral = "'" + ((Split-Path -Parent $file) -replace "'", "''") + "'"
. ([scriptblock]::Create($helper[0].Extent.Text.Replace('$PSScriptRoot', $rootLiteral)))
$script:calls = @(); $script:failures = @(); $script:blocked = $false; $script:changed = 0; $script:toolExit = 0
$ctx = @{Logs='retained-logs';Workspace='untouched-fixture'}
$first = @{stage='repair';Run=@{ExitCode=0;TimedOut=$false;StdoutPath='first.jsonl'}}
function Assert-That([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
function Get-FailedGates { param($Ctx,$Stage) if ($script:blocked -and $Stage -eq 'repair') { 'failure' } }
function Get-WorkspaceManifest { param($Path) [ordered]@{'cart.js'='repaired';'cart.test.js'='protected'} }
function Compare-WorkspaceManifest { param($Before,$After) @{Changed=$script:changed} }
function Get-Sha256 { param($Path) 'protected' }
function Write-JsonFile { param($Path,$Value) $script:calls += @{kind='manifest';path=$Path} }
function Add-Asset { param($Ctx,$Path,$Description) $script:calls += @{kind='asset';path=$Path} }
function Invoke-VcpTask { param($Ctx,$Stage,$Title,$Config,$Prompt)
    $script:calls += @{kind='task';stage=$Stage;profile=$Config;prompt=$Prompt}
    @{stage=$Stage;Run=@{ExitCode=0;TimedOut=$false;StdoutPath='second.jsonl'}}
}
function Test-StageExit { param($Ctx,$Result,$Stage) $script:calls += @{kind='native-gate';stage=$Stage} }
function Invoke-Gate { param($Ctx,$Stage,$Id,$Description,[scriptblock]$Test)
    try { & $Test | Out-Null } catch { $script:failures += "$Id`: $($_.Exception.Message)" }
}
function Invoke-Tool { param($Ctx,$Stage,$Label,$FilePath,$ArgumentList,$TimeoutSeconds)
    $script:calls += @{kind='tool';label=$Label;arguments=$ArgumentList}
    @{ExitCode=$script:toolExit;TimedOut=$false}
}
$protected = @{'cart.test.js'='protected'}
Test-SecondInvocation $ctx $first 'original-profile.json' 'node' $protected
Assert-That ($script:failures.Count -eq 0) "Passing helper failed: $script:failures"
$task = @($script:calls | Where-Object kind -eq 'task')
Assert-That ($task.Count -eq 1 -and $task[0].profile -eq 'original-profile.json') 'Must reuse original profile for a new task.'
Assert-That ($task[0].prompt -match 'Do not change' -and $task[0].prompt -match 'six existing') 'Verification-only intent or original checks lost.'
$tools = @($script:calls | Where-Object kind -eq 'tool')
Assert-That ($tools.Count -eq 2 -and $tools[0].arguments[1] -eq 'first.jsonl' -and $tools[0].arguments[2] -eq 'second.jsonl') 'Wrong retained stream inputs.'
Assert-That ($tools[0].arguments[3] -eq (Join-Path $ctx.Logs 'repair/inspection-bundle.json') -and $tools[0].arguments[4] -eq (Join-Path $ctx.Logs 'second-invocation/inspection-bundle.json')) 'Wrong task encoding bundle inputs.'
Assert-That (($tools[1].arguments -join ' ') -eq '--test cart.test.js') 'Original independent cart tests must run again.'
$script:changed=1; $script:failures=@()
Test-SecondInvocation $ctx $first 'original-profile.json' 'node' $protected
Assert-That ($script:failures.Count -eq 1 -and $script:failures[0] -match '^unchanged-source:') 'Source mutation must fail independent gate.'
$script:changed=0; $script:toolExit=1; $script:failures=@()
Test-SecondInvocation $ctx $first 'original-profile.json' 'node' $protected
Assert-That ($script:failures.Count -eq 2) 'Oracle and independent test failures must remain visible.'
$script:blocked=$true; $script:calls=@(); $rejected=$false
try { Test-SecondInvocation $ctx $first 'original-profile.json' 'node' $protected } catch { $rejected=$true }
Assert-That ($rejected -and $script:calls.Count -eq 0) 'Failed first invocation must prevent another paid invocation.'
Write-Output 'Second invocation probe wiring: 9 assertions passed; no provider, application or database execution.'
