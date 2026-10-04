#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Offline regressions: no VCP installation, provider snapshot, network or spend.
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('vcp-harness-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function New-TestContext {
    $logs = Join-Path $temporary ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $logs | Out-Null
    return @{
        Name = 'offline'; RunId = 'test'; Logs = $logs; Results = $logs; Workspace = $temporary
        ProgressLog = Join-Path $logs 'progress.log'; ToolLog = Join-Path $logs 'tools.log'
        Gates = [Collections.Generic.List[object]]::new(); Stages = [Collections.Generic.List[object]]::new()
        Notes = [Collections.Generic.List[string]]::new(); Assets = [Collections.Generic.List[object]]::new()
        Started = Get-Date; SpentUsd = [decimal]0; MaxScenarioUsd = [decimal]30; CostUnknown = $false
        AccountedTaskUsd = @{}; SettledTaskUsd = @{}; SkipPaidStages = $false
    }
}
function Cost-Page([string]$Settled = '1000000', [string]$Active = '0', [string]$Unresolved = '0') {
    return [pscustomobject]@{
        items = @([pscustomobject]@{ collection = 'ledger'; visibility = 'available'
            record = [pscustomobject]@{ settled = $Settled; active = $Active; unresolved = $Unresolved } })
        gaps = @(); next_cursor = $null
    }
}
try {
    # Parse every scenario without executing its top-level paid workflow.
    foreach ($file in Get-ChildItem (Join-Path $PSScriptRoot '..') -File | Where-Object Extension -in '.ps1', '.psm1') {
        $tokens = $null; $errors = $null
        [void][Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
        Check ($errors.Count -eq 0) "$($file.Name): $errors"
    }
    $pwsh = (Get-Process -Id $PID).Path
    $probe = Join-Path $temporary 'native probe.ps1'
    Write-Utf8File $probe @'
param([string]$Mode, [string]$Value)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
switch ($Mode) {
    'arguments' { [Console]::Out.WriteLine($Value); [Console]::Error.WriteLine('stderr captured'); exit 7 }
    'noisy' { while ($true) { [Console]::Out.WriteLine('still running') } }
    'closed' { [Console]::OpenStandardOutput().Dispose(); Start-Sleep -Seconds 30 }
    'quiet' { Start-Sleep -Seconds 30 }
    'unbounded' { Start-Sleep -Milliseconds 1250; 'completed without a deadline'; exit 0 }
    'credential' {
        if ($env:OPENROUTER_API_KEY -or ($env:VCP_SCENARIO_CREDENTIAL_ENV -and [Environment]::GetEnvironmentVariable($env:VCP_SCENARIO_CREDENTIAL_ENV))) { exit 9 }
        'no inherited provider credential'
    }
}
'@
    $literal = 'spaces "quotes" ; $() & β'
    $result = Invoke-NativeLogged -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $probe, 'arguments', $literal) `
        -StdoutPath (Join-Path $temporary 'argv.out') -StderrPath (Join-Path $temporary 'argv.err') -TimeoutSeconds 10
    Check ($result.ExitCode -eq 7 -and -not $result.TimedOut) 'Native exit status lost'
    Check ((Get-Content $result.StdoutPath -Raw).TrimEnd() -ceq $literal) 'Arguments were reinterpreted'
    Check ((Get-Content $result.StderrPath -Raw).Trim() -eq 'stderr captured') 'stderr was not drained'
    foreach ($mode in 'noisy', 'closed', 'quiet') {
        $result = Invoke-NativeLogged -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $probe, $mode) `
            -StdoutPath (Join-Path $temporary "$mode.out") -StderrPath (Join-Path $temporary "$mode.err") -TimeoutSeconds 1
        Check ($result.TimedOut -and $result.ExitCode -eq -1) "$mode evaded timeout"
        Check ($result.DurationSeconds -lt 15) "$mode timeout was not bounded"
    }
    $result = Invoke-NativeLogged -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $probe, 'unbounded') `
        -StdoutPath (Join-Path $temporary 'unbounded.out') -StderrPath (Join-Path $temporary 'unbounded.err') -TimeoutSeconds $null
    Check ($result.ExitCode -eq 0 -and -not $result.TimedOut -and $result.DurationSeconds -ge 1.2) 'Explicit unbounded supervision ended early'
    Check ((Get-Content $result.StdoutPath -Raw).Trim() -eq 'completed without a deadline') 'Unbounded supervisor lost output'
    $ctx = New-TestContext
    $savedCredential = $env:OPENROUTER_API_KEY
    $savedCredentialName = $env:VCP_SCENARIO_CREDENTIAL_ENV
    $savedAlias = $env:VCP_TEST_HARNESS_KEY
    try {
        $env:OPENROUTER_API_KEY = 'offline-test-sentinel'
        $result = Invoke-Tool -Ctx $ctx -Stage 'test' -Label 'credential' -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $probe, 'credential')
        Check ($result.ExitCode -eq 0) 'Tool inherited provider credential'
        Check ($env:OPENROUTER_API_KEY -eq 'offline-test-sentinel') 'Tool changed parent credential'
        $env:VCP_SCENARIO_CREDENTIAL_ENV = 'VCP_TEST_HARNESS_KEY'
        $env:VCP_TEST_HARNESS_KEY = 'offline-alias-sentinel'
        $result = Invoke-Tool -Ctx $ctx -Stage 'test' -Label 'credential-alias' -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $probe, 'credential')
        Check ($result.ExitCode -eq 0) 'Tool inherited custom provider credential'
        Check ($env:VCP_TEST_HARNESS_KEY -eq 'offline-alias-sentinel') 'Tool changed parent alias credential'
    }
    finally {
        $env:OPENROUTER_API_KEY = $savedCredential
        $env:VCP_SCENARIO_CREDENTIAL_ENV = $savedCredentialName
        $env:VCP_TEST_HARNESS_KEY = $savedAlias
    }

    $parsed = ConvertFrom-JsonLines "{`"type`":`"accepted`"}`nnot-json`n{`"type`":`"result`"}"
    Check ($parsed.Frames.Count -eq 2 -and $parsed.Invalid.Count -eq 1) 'Malformed JSONL was hidden'
    $ctx = New-TestContext
    $scope = @{ workspace = 'workspace'; session = 'session'; task = 'task' }
    $accepted = [pscustomobject]@{ type = 'accepted'; scope = $scope; correlation = 'command' }
    $final = [pscustomobject]@{ type = 'result'; scope = $scope; correlation = 'command'; exit_code = 0 }
    $stageResult = @{ accepted_exit = @(0); exit_code = 0; Run = @{ Accepted = $accepted; Result = $final; InvalidLines = 0; Frames = @($accepted, $final) } }
    Test-StageExit $ctx $stageResult 'valid-jsonl'
    Check ((Get-FailedGates $ctx 'valid-jsonl').Count -eq 0) 'Valid lifecycle frames rejected'
    $final.exit_code = 3
    Test-StageExit $ctx $stageResult 'wrong-exit'
    Check ((Get-FailedGates $ctx 'wrong-exit').Count -eq 1) 'Result/process exit disagreement accepted'
    $final.exit_code = 0; $final.scope = @{ workspace = 'workspace'; session = 'other-session'; task = 'task' }
    Test-StageExit $ctx $stageResult 'wrong-scope'
    Check ((Get-FailedGates $ctx 'wrong-scope').Count -eq 1) 'Mismatched command scope accepted'
    Check ((Get-VcpTaskCost @(Cost-Page)).Usd -eq 1) 'Settled micros conversion failed'
    foreach ($page in @((Cost-Page -Active '1'), (Cost-Page -Unresolved '2'), (Cost-Page -Settled ''), (Cost-Page))) {
        if ($page.items[0].record.settled -eq '1000000' -and $page.items[0].record.active -eq '0' -and $page.items[0].record.unresolved -eq '0') { $page.gaps = @(@{ reason = 'missing' }) }
        Check ($null -eq (Get-VcpTaskCost @($page)).Usd) 'Incomplete ledger accepted as settled'
    }
    $page = Cost-Page; $page.next_cursor = @{ after = 'more' }
    Check ($null -eq (Get-VcpTaskCost @($page)).Usd) 'Partial pagination accepted'
    Check ($null -eq (Get-VcpTaskCost @()).Usd) 'Empty costs accepted'

    $ctx = New-TestContext
    $pages = & $module {
        param($context)
        $script:inspectionProbeCalls = 0
        function Invoke-Vcp {
            $script:inspectionProbeCalls++
            if ($script:inspectionProbeCalls -eq 1) {
                return @{ ExitCode = 0; InvalidLines = 0; Result = @{ data = @{ items = @(); gaps = @(); next_cursor = @{ after = 'first' } } } }
            }
            return @{ ExitCode = 2; InvalidLines = 0; Result = $null }
        }
        Invoke-VcpInspect $context 'test' 'task-a' 'costs'
    } $ctx
    Check ($pages.Count -eq 2 -and $pages[1].gaps.Count -eq 1) 'Failed second inspect page lost completeness marker'
    Check ((Get-FailedGates $ctx 'test').Count -eq 1) 'Inspection failure did not fail evidence gate'

    foreach ($mode in 'valid', 'missing-result', 'invalid-jsonl', 'scoped', 'accepted', 'wrong-condition', 'timed-out') {
        $ctx = New-TestContext
        & $module {
            param($context, $mode)
            $frame = @{ type = 'result'; schema_version = 1; correlation = 'guardrail'; scope = $null; receipt = $null;
                exit_code = 2; conditions = [pscustomobject]@{ invalid_configuration = $true } }
            $run = @{ ExitCode = 2; TimedOut = $false; InvalidLines = 0; Accepted = $null; Scope = $null; Frames = @($frame) }
            switch ($mode) {
                'missing-result' { $run.Frames = @() }
                'invalid-jsonl' { $run.InvalidLines = 1 }
                'scoped' { $run.Scope = @{ task = 'unexpected' }; $frame.scope = $run.Scope }
                'accepted' { $run.Accepted = @{ type = 'accepted' } }
                'wrong-condition' { $frame.conditions = [pscustomobject]@{ provider_failure = $true } }
                'timed-out' { $run.TimedOut = $true }
            }
            function Invoke-Vcp { return $run }
            Invoke-GuardrailRun $context 'G0-guardrail' $mode 'rejected before task acceptance' @('run') 'unused.json'
        } $ctx $mode
        Check (((Get-FailedGates $ctx 'G0-guardrail').Count -eq 0) -eq ($mode -eq 'valid')) "$mode guardrail evidence was misclassified"
    }

    $message = & $module {
        param($context)
        $sse = 'data: {"type":"response.completed","response":{"output":[{"content":[{"type":"output_text","text":"latest response"}]}]}}' + "`n`n"
        $bytes = [Text.Encoding]::UTF8.GetBytes($sse)
        $descriptor = @{ length = $bytes.Length; state = 'complete'; sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant(); spec = @{ channel = 'response'; omissions = @('authentication_headers') } }
        function Invoke-Vcp {
            param($Ctx, $Stage, $Label, $TimeoutSeconds, $Arguments)
            if ($Arguments[1] -ne 'a-latest') { throw 'Selected response by canonical key instead of event order' }
            if ($TimeoutSeconds -lt 130) { return @{ ExitCode = 124; InvalidLines = 0; TimedOut = $true; Result = $null } }
            return @{ ExitCode = 0; InvalidLines = 0; Result = @{ data = @{ gaps = @(@{ visibility = 'redacted'; omissions = @('authentication_headers') }); items = @(@{
                bytes = $bytes; artifact = 'a-latest'; descriptor = $descriptor; visibility = 'available'
                range = @{ start = 0; end = $bytes.Length }; next_offset = $null
            }) } } }
        }
        $outputs = @(@{ items = @(
            @{ collection = 'artifact'; id = 'a-latest'; record = $descriptor },
            @{ collection = 'artifact'; id = 'z-earlier'; record = $descriptor }
        ) })
        $frames = @('z-earlier', 'a-latest') | ForEach-Object {
            @{ type = 'event'; event = @{ event = @{ data = @{ facts = @(@{ collection = 'artifact'; id = $_; value = @{ state = 'complete'; spec = @{ channel = 'response' } } }) } } } }
        }
        $valid = Get-VcpFinalMessage $context 'test' $outputs $frames
        $bytes = $bytes[0..($bytes.Length - 2)]
        $partial = Get-VcpFinalMessage $context 'test' $outputs $frames
        if ($null -ne $partial) { throw 'Partial range incorrectly accepted' }
        return $valid
    } $ctx
    Check ($message -eq 'latest response') 'Final response did not use completion chronology'

    $ctx = New-TestContext
    $record = [ordered]@{ stage = 'run'; budget_usd = 3; cost_usd = $null }
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' ([decimal]1.25) $r } $ctx $record
    Check ($ctx.SpentUsd -eq 1.25 -and $record.cost_usd -eq 1.25) 'Initial task cost wrong'
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' ([decimal]2) $r } $ctx $record
    Check ($ctx.SpentUsd -eq 2 -and $record.cost_usd -eq 0.75) 'Resume ledger double counted'
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' $null $r } $ctx $record
    Check ($ctx.SpentUsd -eq 3 -and $null -eq $record.cost_usd) 'Unknown cost did not reserve task cap'
    & $module { param($c, $r) Update-ScenarioCost $c 'task-a' $null $r } $ctx $record
    Check ($ctx.SpentUsd -eq 3) 'Repeated missing evidence double counted task cap'
    & $module { param($c, $r) Update-ScenarioCost $c 'task-b' ([decimal]0.5) $r } $ctx $record
    Check ($ctx.SpentUsd -eq 3.5) 'Distinct task not counted'
    $ctx.TurnBudgetUsd = 3
    $ctx.SkipPaidStages = $false
    [void](Invoke-Gate $ctx 'P1-profiles' 'bad' 'fixture invalid profile' { $false })
    $refused = $false
    try { [void](Invoke-VcpTask $ctx 'T1' 'must not run' 'prompt' 'unused.json') }
    catch { $refused = $_.Exception.Message -like '*refusing paid execution*' }
    Check $refused 'Failed profile did not block paid execution'

    $workspace = Join-Path $temporary 'workspace'
    Write-Utf8File (Join-Path $workspace 'src/main.txt') 'source'
    Write-Utf8File (Join-Path $workspace 'models/model.bin') 'one'
    $authored = Get-WorkspaceManifest $workspace
    $all = Get-WorkspaceManifest $workspace -IncludeGenerated
    Write-Utf8File (Join-Path $workspace 'models/model.bin') 'two'
    Check ((Compare-WorkspaceManifest $authored (Get-WorkspaceManifest $workspace)).Changed -eq 0) 'Authored manifest includes build outputs'
    Check ((Compare-WorkspaceManifest $all (Get-WorkspaceManifest $workspace -IncludeGenerated)).Changed -eq 1) 'Review manifest missed generated file mutation'
    $all = Get-WorkspaceManifest $workspace -IncludeGenerated
    New-Item -ItemType Directory -Path (Join-Path $workspace 'empty') | Out-Null
    Check ((Compare-WorkspaceManifest $all (Get-WorkspaceManifest $workspace -IncludeGenerated)).Changed -eq 1) 'Review manifest missed empty directory'

    $ctx = New-TestContext
    $gate = Invoke-Gate $ctx 'test' 'truthy' 'boolean result required' { 'false' }
    Check ($gate.outcome -eq 'fail') 'Truthy string accepted as gate pass'
    $ctx = New-TestContext
    [void](Invoke-Gate $ctx 'P0-preflight' 'ok' 'fixture' { $true })
    $ctx.SkipPaidStages = $true
    Check ((Complete-VcpScenario $ctx) -eq 0) 'Successful dry run should exit zero'
    Check ((Get-Content (Join-Path $ctx.Results 'scorecard.json') -Raw | ConvertFrom-Json).verdict -eq 'dry-run-pass') 'Dry run advertised full pass'
    $ctx.SkipPaidStages = $false
    $ctx.Stages.Add([pscustomobject]@{ stage = 'T1'; title = 'skipped'; skipped = 'budget ceiling' })
    Check ((Complete-VcpScenario $ctx) -eq 1) 'Budget-skipped paid stage should fail exit status'
    Check ((Get-Content (Join-Path $ctx.Results 'scorecard.json') -Raw | ConvertFrom-Json).verdict -eq 'incomplete') 'Budget skip not marked incomplete'
    Write-Host "PASS: $checks offline harness checks"
}
finally {
    # Only remove this test's unique temporary directory, verified under TEMP.
    $full = [IO.Path]::GetFullPath($temporary)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $full -Leaf) -notlike 'vcp-harness-tests-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
