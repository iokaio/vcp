#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase ('vcp-lazy-final-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$checks = 0
function Check($Condition, [string]$Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
$ctx = @{
    Name = 'lazy-final'; Logs = $root; Results = $root; Workspace = $root; ProgressLog = Join-Path $root 'progress.log'
    Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
    Notes = [Collections.Generic.List[string]]::new(); SupportsInspectionBundle = $true
    SpentUsd = [decimal]0; TurnBudgetUsd = [decimal]5; AccountedTaskUsd = @{}; SettledTaskUsd = @{}
    UnknownTaskCosts = @{}; TaskBudgetUsd = @{}; CostUnknown = $false; UnscopedCostUnknown = $false
}
try {
    & $module {
        $script:rangeCalls = 0
        $script:missingResponse = $false
        $script:finalText = 'Review src/demo.py.' + "`n" + '```json' + "`n" + '{"findings":[]}' + "`n" + '```'
        $sse = 'data: ' + (@{ type = 'response.completed'; response = @{ output = @(@{ content = @(@{ type = 'output_text'; text = $script:finalText }) }) } } | ConvertTo-Json -Depth 10 -Compress) + "`n`n"
        $script:responseBytes = [Text.Encoding]::UTF8.GetBytes($sse)
        $script:descriptor = @{ state = 'complete'; length = $script:responseBytes.Length
            sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($script:responseBytes)).ToLowerInvariant()
            spec = @{ channel = 'response'; omissions = @('authentication_headers') } }
        function script:Get-VcpStageInspection {
            param($Ctx, $Stage, $Task)
            $outputs = @(@{ items = @(@{ collection = 'artifact'; id = 'response'; record = $script:descriptor }) })
            Write-JsonFile (Join-Path $Ctx.Logs "$Stage/inspect-outputs.json") $outputs
            return @{ outputs = $outputs; tools = @(@{ items = @() }); costs = @(@{ items = @(@{
                collection = 'ledger'; record = @{ settled = '250000'; active = '0'; unresolved = '0' }
            }); gaps = @(); next_cursor = $null }) }
        }
        function script:Invoke-Vcp {
            param($Ctx, $Stage, $Label, $Arguments, $TimeoutSeconds)
            if ($Label -ne 'inspect-response-range') { throw 'Unexpected native command' }
            $script:rangeCalls++
            if ($script:missingResponse) { return @{ ExitCode = 1; InvalidLines = 0; Result = $null } }
            return @{ ExitCode = 0; InvalidLines = 0; Result = @{ data = @{ gaps = @(); items = @(@{
                artifact = 'response'; visibility = 'available'; descriptor = $script:descriptor
                bytes = $script:responseBytes; range = @{ start = 0; end = $script:responseBytes.Length }; next_offset = $null
            }) } } }
        }
        function script:New-FixtureStage {
            param($Ctx, $Stage)
            $scope = @{ workspace = 'workspace'; session = 'session'; task = 'task' }
            $accepted = @{ type = 'accepted'; schema_version = 1; scope = $scope; correlation = 'fixture' }
            $result = @{ type = 'result'; schema_version = 1; scope = $scope; correlation = 'fixture'; exit_code = 0; conditions = [pscustomobject]@{ completed = $true } }
            $frames = @($accepted, @{ type = 'event'; event = @{ event = @{ data = @{ facts = @(@{
                collection = 'artifact'; id = 'response'; value = @{ state = 'complete'; spec = @{ channel = 'response' } }
            }) } } } }, $result)
            $run = @{ ExitCode = 0; Accepted = $accepted; Result = $result; Scope = $scope; Frames = $frames; TimedOut = $false; DurationSeconds = 0; EventCounts = @{}; InvalidLines = 0 }
            $record = [ordered]@{ stage = $Stage; accepted_exit = @(0); budget_usd = [decimal]5; files_changed = 0; final_message = $null }
            Complete-VcpStageEvidence $Ctx $Stage $run $null $record
        }
        function script:Invoke-VcpTask { param($Ctx, $Stage) New-FixtureStage $Ctx $Stage }
        function script:Get-WorkspaceManifest { return [ordered]@{} }
    }
    $coding = & $module { param($c) New-FixtureStage $c 'coding' } $ctx
    Check ((& $module { $script:rangeCalls }) -eq 0) 'Ordinary evidence collection read optional response bytes'
    Check ($ctx.SpentUsd -eq [decimal]0.25 -and -not $ctx.CostUnknown) 'Lazy extraction changed settled accounting'
    Check (Test-Path -LiteralPath (Join-Path $root 'coding/inspect-outputs.json')) 'Canonical output descriptors were lost'
    Check (-not (Test-Path -LiteralPath (Join-Path $root 'coding/final-message.md'))) 'Coding stage eagerly wrote final text'
    $text = Get-VcpStageFinalMessage $ctx $coding
    Check ($text -eq (& $module { $script:finalText })) 'On-demand extraction did not recover verified response text'
    Check ($coding.final_message -eq 'logs/coding/final-message.md') 'Extracted message path was not recorded'
    [void](Get-VcpStageFinalMessage $ctx $coding)
    Check ((& $module { $script:rangeCalls }) -eq 1) 'Repeated final-text access reopened canonical history'

    $review = Invoke-PlanModeReview $ctx 'review' 'fixture' 'Review'
    Check ((& $module { $script:rangeCalls }) -eq 2) 'Review gate did not extract its final text'
    Check ((@($ctx.Gates | Where-Object { $_.stage -eq 'review' -and $_.id -eq 'findings-json' -and $_.outcome -eq 'pass' })).Count -eq 1) 'Review findings gate no longer validated the JSON block'
    & $module { $script:missingResponse = $true }
    [void](Invoke-PlanModeReview $ctx 'missing-review' 'fixture' 'Review')
    $failed = @($ctx.Gates | Where-Object { $_.stage -eq 'missing-review' -and $_.id -eq 'findings-json' })
    Check ($failed.Count -eq 1 -and $failed[0].outcome -eq 'fail' -and -not $failed[0].required) 'Unavailable text did not fail the existing advisory gate'
    Check ($ctx.SpentUsd -eq [decimal]0.25 -and -not $ctx.CostUnknown) 'Optional extraction failure changed complete accounting'

    $corrupt = & $module { param($c)
        $script:missingResponse = $false
        $record = New-FixtureStage $c 'corrupt-response'
        $script:responseBytes[0] = $script:responseBytes[0] -bxor 1
        return $record
    } $ctx
    Check ($null -eq (Get-VcpStageFinalMessage $ctx $corrupt)) 'On-demand extraction accepted response bytes with a different hash'
    Check (-not (Test-Path -LiteralPath (Join-Path $root 'corrupt-response/final-message.md'))) 'Corrupt response bytes were saved as final text'

    # Parse the actual D advisory gate without executing its paid workflow.
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scenario-d-python-textlab.ps1'), [ref]$tokens, [ref]$errors)
    $gate = $ast.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Invoke-Gate' -and $node.Extent.Text -match "-Id 'review-consistency'" }, $true)
    Check ($null -ne $gate -and $gate.Extent.Text -match 'Get-VcpStageFinalMessage -Ctx \$ctx -StageRecord \$fork') 'D fork consistency gate lost its demand for final text'
    Write-Host "PASS: $checks lazy final-message checks"
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if (-not $resolved.StartsWith($tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path -Leaf $resolved) -notlike 'vcp-lazy-final-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
