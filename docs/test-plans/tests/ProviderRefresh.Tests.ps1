#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Real launcher and command audit, inert native/provider/child boundaries.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$scenarioRoot = Split-Path -Parent $PSScriptRoot
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporary = Join-Path $tempBase ('vcp-provider-refresh-tests-' + [guid]::NewGuid().ToString('N'))
$checks = 0
function Check([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message }; $script:checks++ }
function New-Metadata([string]$Path, [int]$Minutes, [string]$Endpoint = 'fixture/endpoint') {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    @{ data = @{ id = 'fixture/model'; endpoints = @(@{ tag = $Endpoint }) } } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $Path 'endpoints.json')
    @{ valid_until = [DateTimeOffset]::UtcNow.AddMinutes($Minutes).ToUnixTimeMilliseconds()
        raw_sha256 = (Get-FileHash -LiteralPath (Join-Path $Path 'endpoints.json')).Hash.ToLowerInvariant()
        compatibility = @{ model = 'fixture/model'; endpoint = $Endpoint; request_price_limit = '0.001' }
    } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $Path 'snapshot.json')
}
$saved = @{}
foreach ($name in 'LOCALAPPDATA', 'VCP_PROVIDER_GENERATION', 'VCP_DENY_PROVIDER_CREDENTIALS', 'VCP_SCENARIO_CREDENTIAL_ENV', 'VCP_REFRESH_CASE', 'VCP_REFRESH_KEY') { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    $fixture = Join-Path $temporary 'launcher fixture'
    New-Item -ItemType Directory -Path $fixture | Out-Null
    $launcher = Join-Path $scenarioRoot 'run-cli-scenarios.ps1'
    Copy-Item -LiteralPath $launcher -Destination $fixture
    $harness = Join-Path $scenarioRoot 'VcpScenarioHarness.psm1'
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($harness, [ref]$tokens, [ref]$errors)
    $native = $ast.Find({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Invoke-NativeLogged' }, $true)
    Check ($errors.Count -eq 0 -and $null -ne $native) 'Cannot isolate native VCP boundary'
    $replacement = @'
function Invoke-NativeLogged {
    param($FilePath, [string[]]$ArgumentList, $WorkingDirectory, $StdoutPath, $StderrPath, $TimeoutSeconds, $Environment, $OnLine, $HeartbeatLabel, $Ctx)
    $index = [Array]::IndexOf($ArgumentList, 'setup')
    $code = 0
    if ($ArgumentList[-1] -eq 'models') { $data = @{ effective = @{ set = @{ roles = @{ main = @('fixture/model') } } } } }
    elseif (($ArgumentList[-3..-1] -join ' ') -eq 'setup credential status') { $data = @{ environment = 'VCP_REFRESH_KEY' } }
    elseif ($index -ge 0 -and $ArgumentList[$index + 1] -eq 'provider-refresh') {
        if ($Environment.VCP_DENY_PROVIDER_CREDENTIALS -ne '1') { throw 'Metadata refresh must deny provider credentials' }
        $command = $ArgumentList[$index + 1]
        $generation = $ArgumentList[-1]
        if (Test-Path -LiteralPath $generation) { throw 'Refresh must create a unique new directory' }
        if ($ArgumentList -notcontains '--snapshot' -or $ArgumentList -notcontains '--catalog' -or $ArgumentList -contains '--budget-usd') { throw 'Refresh requires retained evidence and no inference budget' }
        $prior = Get-Content -LiteralPath $ArgumentList[([Array]::IndexOf($ArgumentList, '--snapshot') + 1)] -Raw | ConvertFrom-Json
        if ($prior.compatibility.model -ne 'fixture/model' -or $prior.compatibility.endpoint -ne 'fixture/endpoint') { throw 'Refresh changed selected identity' }
        New-Item -ItemType Directory -Path $generation | Out-Null
        if ($env:VCP_REFRESH_CASE -in 'failure', 'unsupported') {
            $code = 1; $data = @{ status = 'failed' }
        }
        else {
            $endpoint = if ($env:VCP_REFRESH_CASE -eq 'wrong-endpoint') { 'different/endpoint' } else { 'fixture/endpoint' }
            @{ data = @{ id = 'fixture/model'; endpoints = @(@{ tag = $endpoint }) } } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $generation 'endpoints.json')
            @{ valid_until = [DateTimeOffset]::UtcNow.AddHours(12).ToUnixTimeMilliseconds()
                raw_sha256 = (Get-FileHash -LiteralPath (Join-Path $generation 'endpoints.json')).Hash.ToLowerInvariant()
                compatibility = @{ model = 'fixture/model'; endpoint = $endpoint; request_price_limit = '0.001' }
            } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $generation 'snapshot.json')
            $data = @{ status = 'refreshed'; model_calls = 0 }
        }
    }
    else { throw ('Unexpected native command: ' + ($ArgumentList -join ' ')) }
    $line = @{ type = 'result'; exit_code = $code; data = $data } | ConvertTo-Json -Depth 20 -Compress
    [IO.File]::WriteAllText($StdoutPath, $line + "`n")
    [IO.File]::WriteAllText($StderrPath, '')
    if ($OnLine) { & $OnLine $line }
    return [pscustomobject]@{ ExitCode = $code; TimedOut = $false; DurationSeconds = 0.01 }
}
'@
    $source = [IO.File]::ReadAllText($harness)
    $source = $source.Remove($native.Extent.StartOffset, $native.Extent.EndOffset - $native.Extent.StartOffset).Insert($native.Extent.StartOffset, $replacement)
    [IO.File]::WriteAllText((Join-Path $fixture 'VcpScenarioHarness.psm1'), $source)
    @'
param($ProviderGeneration, $Vcp, $RunRoot, $ProjectPath, $TurnBudgetUsd, $MaxScenarioUsd, $MaxRepairTurns,
    $OutputTokens, $MaxRequests, $DeadlineSeconds, $ShortDeadlineSeconds, [switch]$SkipPaidStages, [switch]$AllowProcessPublish)
$root = Join-Path $RunRoot 'b-aspnet-inventory/fixture-run'
New-Item -ItemType Directory -Path (Join-Path $root 'results') -Force | Out-Null
@{ generation = $ProviderGeneration; cap = $MaxScenarioUsd; credential = $env:VCP_SCENARIO_CREDENTIAL_ENV } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'arguments.json')
@{ verdict = 'fixture-only' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'results/scorecard.json')
Write-Host 'REFRESH_CHILD_STARTED'
exit 0
'@ | Set-Content -LiteralPath (Join-Path $fixture 'scenario-b-aspnet-inventory.ps1')
    $env:LOCALAPPDATA = Join-Path $temporary 'account data'
    $account = Join-Path $env:LOCALAPPDATA 'VCP/account'
    New-Item -ItemType Directory -Path $account -Force | Out-Null
    $connection = Join-Path $account 'connection-original'
    New-Metadata $connection -60
    $completion = Join-Path $account 'setup-complete.json'
    @{ version = 1; connection = @{ status = 'connected'; model = 'fixture/model'; endpoint = 'fixture/endpoint'; evidence = $connection } } | ConvertTo-Json | Set-Content -LiteralPath $completion
    $originalHashes = @{}; foreach ($path in $completion, (Join-Path $connection 'snapshot.json'), (Join-Path $connection 'endpoints.json')) { $originalHashes[$path] = (Get-FileHash -LiteralPath $path).Hash }
    foreach ($name in 'VCP_PROVIDER_GENERATION', 'VCP_DENY_PROVIDER_CREDENTIALS', 'VCP_SCENARIO_CREDENTIAL_ENV') { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
    $env:VCP_REFRESH_KEY = 'synthetic-refresh-fixture-not-a-secret'
    $pwsh = (Get-Process -Id $PID).Path
    $project = Join-Path $temporary 'project B'
    function Invoke-Fixture([string]$Name, [string]$Case = 'success', [string]$Mode = 'Full', [string]$Root = '') {
        $env:VCP_REFRESH_CASE = $Case
        if (-not $Root) { $Root = Join-Path $temporary $Name }
        $before = @(Get-ChildItem -LiteralPath $Root -Directory -Filter 'launch-*' -ErrorAction SilentlyContinue | ForEach-Object FullName)
        $arguments = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $fixture 'run-cli-scenarios.ps1'), '-Scenario', 'B', '-Mode', $Mode,
            '-RunRoot', $Root, '-ProjectPath', $project, '-Vcp', $pwsh, '-MaxScenarioUsd', '10', '-TurnBudgetUsd', '3', '-AllowProcessPublish')
        $output = (& $pwsh @arguments 2>&1 | Out-String); $code = $LASTEXITCODE
        $roots = @(Get-ChildItem -LiteralPath $Root -Directory -Filter 'launch-*' | Where-Object { $_.FullName -notin $before })
        Check ($roots.Count -eq 1) "Expected one invocation: $output"
        $run = $roots[0].FullName
        $commands = @(Get-Content -LiteralPath (Join-Path $run 'setup/logs/vcp-commands.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 30 })
        $literal = Get-Content -LiteralPath (Join-Path $run 'setup/logs/vcp-commands.log') -Raw
        Check (-not $literal.Contains('synthetic-refresh-fixture')) 'Credential leaked into command audit'
        return [pscustomobject]@{ Code = $code; Output = $output; Root = $run; Commands = $commands; Literal = $literal; RunRoot = $Root }
    }
    $success = Invoke-Fixture 'success'
    Check ($success.Code -eq 0 -and $success.Output.Contains('REFRESH_CHILD_STARTED')) "Renewal did not start prepared scenario: $($success.Output)"
    $child = Get-Content -LiteralPath (Join-Path $success.Root 'b-aspnet-inventory/fixture-run/arguments.json') -Raw | ConvertFrom-Json
    Check ([decimal]$child.cap -eq 10 -and $child.credential -eq 'VCP_REFRESH_KEY') 'Metadata refresh reduced task budget or lost configured credential alias'
    Check ($child.generation.StartsWith($success.Root) -and $child.generation -ne $connection) 'Renewal did not use isolated invocation storage'
    Check (@($success.Commands | Where-Object label -eq 'refresh-provider-metadata').Count -eq 1) 'Refresh repeated metadata calls'
    Check ($success.Literal.Contains('provider-refresh') -and $success.Literal.Contains('--snapshot') -and $success.Literal.Contains('results')) 'Literal refresh audit omits command, source or results pointers'
    foreach ($path in $originalHashes.Keys) { Check ((Get-FileHash -LiteralPath $path).Hash -eq $originalHashes[$path]) 'Renewal modified account metadata or original source bytes' }
    $renewal = Get-Content -LiteralPath (Join-Path $success.Root 'setup/results/provider-refresh.json') -Raw | ConvertFrom-Json
    Check ($renewal.status -eq 'refreshed' -and $renewal.model_calls -eq 0) 'Metadata-only refresh evidence incomplete'
    $reuse = Invoke-Fixture 'reuse' -Case 'failure' -Root $success.RunRoot
    Check ($reuse.Code -eq 0 -and @($reuse.Commands | Where-Object label -like 'refresh-provider*').Count -eq 0) 'Fresh same-provider generation unnecessarily renewed'
    foreach ($case in 'failure', 'unsupported', 'wrong-endpoint') {
        $run = Invoke-Fixture $case -Case $case
        Check ($run.Code -ne 0) "Unexpected outcome for ${case}: $($run.Output)"
        Check (-not $run.Output.Contains('REFRESH_CHILD_STARTED')) "$case started an invalid scenario"
        Check (@($run.Commands | Where-Object label -eq 'refresh-provider-metadata').Count -eq 1) "$case repeated metadata refresh"
        Check (@($run.Commands | Where-Object label -eq 'refresh-provider-complete').Count -eq 0) "$case incorrectly fell back to qualification receipt completion"
        Check (Test-Path -LiteralPath (Join-Path $run.Root 'setup/results/provider-refresh.json')) "$case lost failure evidence"
        $selection = Get-Content -LiteralPath (Join-Path $run.Root 'setup/results/provider-selection.json') -Raw | ConvertFrom-Json
        Check ($selection.refresh_attempted -and $selection.model_calls -eq 0 -and (Test-Path -LiteralPath $selection.refresh_evidence)) "$case lost metadata-only failure evidence"
        if ($case -eq 'unsupported') { Check ($run.Output.Contains('installed VCP must support setup provider-refresh')) 'Old executable failure omitted upgrade requirement' }
    }
    $dry = Invoke-Fixture 'dry' -Mode 'DryRun'
    Check ($dry.Code -eq 0 -and $dry.Output.Contains('REFRESH_CHILD_STARTED')) 'DryRun could not refresh metadata without inference'
    Check (@($dry.Commands | Where-Object label -eq 'refresh-provider-metadata').Count -eq 1) 'DryRun omitted logged metadata refresh'
    Check (@($dry.Commands | Where-Object label -eq 'credential-status').Count -eq 0) 'DryRun metadata refresh accessed credentials'
    # A newer cached generation at another endpoint cannot replace this provider.
    $otherRoot = Join-Path $temporary 'other-endpoint-cache'
    New-Metadata (Join-Path $otherRoot 'launch-previous/setup/provider-other') 60 'other/endpoint'
    $other = Invoke-Fixture 'other-endpoint' -Root $otherRoot
    Check ($other.Code -eq 0 -and @($other.Commands | Where-Object label -eq 'refresh-provider-metadata').Count -eq 1) 'Another cached endpoint replaced selected provider instead of refreshing exact identity'
    New-Metadata $connection 10
    $near = Invoke-Fixture 'near-expiry'
    Check ($near.Code -eq 0 -and @($near.Commands | Where-Object label -eq 'refresh-provider-metadata').Count -eq 1) 'Metadata expiring within next task window was reused'
    '{}' | Set-Content -LiteralPath (Join-Path $connection 'endpoints.json')
    $bad = Invoke-Fixture 'tampered'
    Check ($bad.Code -eq 1 -and @($bad.Commands | Where-Object label -like 'refresh-provider*').Count -eq 0) 'Tampered metadata authorized a refresh target'
    Write-Host "Provider refresh regressions passed: $checks checks; no native VCP, network or paid inference executed."
}
finally {
    foreach ($name in $saved.Keys) {
        if ($null -eq $saved[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue } else { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
    $resolved = [IO.Path]::GetFullPath($temporary)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'vcp-provider-refresh-tests-*') { throw 'Unsafe cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
