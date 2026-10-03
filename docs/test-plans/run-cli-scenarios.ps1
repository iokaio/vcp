#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Select a scenario and watch its progress in the current PowerShell console.
.EXAMPLE
pwsh -NoProfile -File .\run-cli-scenarios.ps1
.EXAMPLE
pwsh -NoProfile -File .\run-cli-scenarios.ps1 -Scenario A -Mode DryRun -ProviderGeneration C:\vcp-private\provider-generation
#>
[CmdletBinding()]
param(
    [ValidateSet('A', 'B', 'C', 'D')][string]$Scenario,
    [ValidateSet('DryRun', 'Full')][string]$Mode,
    [string]$ProviderGeneration = $env:VCP_PROVIDER_GENERATION,
    [string]$Model,
    [string]$Endpoint,
    [ValidateRange(0, 25)][decimal]$SetupBudgetUsd = 0,
    [ValidateRange(0, 25)][decimal]$RequestPriceLimitUsd = 0.001,
    [string]$Vcp,
    [string]$RunRoot = (Join-Path $env:SystemDrive 'vcp-scenarios'),
    [ValidateRange(0.01, 1000000)][decimal]$TurnBudgetUsd = 3,
    [ValidateRange(0.01, 1000000)][decimal]$MaxScenarioUsd = 30,
    [ValidateRange(0, 100)][int]$MaxRepairTurns = 1,
    [ValidateRange(1, 2147483647)][int]$OutputTokens = 8192,
    [ValidateRange(1, 2147483647)][int]$MaxRequests = 96,
    [ValidateRange(1, 86100)][int]$DeadlineSeconds = 1800,
    [ValidateRange(1, 86100)][int]$ShortDeadlineSeconds = 150
)
$ErrorActionPreference = 'Stop'

function Read-LauncherChoice([string]$Prompt, [string[]]$Choices, [string]$Default = '') {
    while ($true) {
        $answer = (Read-Host $Prompt).Trim()
        if (-not $answer -and $Default) { return $Default }
        foreach ($choice in $Choices) { if ($answer -ieq $choice) { return $choice } }
        Write-Host ('Choose ' + ($Choices -join ', ') + '.') -ForegroundColor Yellow
    }
}

function Assert-LauncherProvider([string]$Generation) {
    if ([string]::IsNullOrWhiteSpace($Generation)) { throw 'A provider generation directory is required.' }
    $full = [IO.Path]::GetFullPath($Generation)
    foreach ($file in 'qualified/snapshot.json', 'endpoints.json') {
        if (-not (Test-Path -LiteralPath (Join-Path $full $file) -PathType Leaf)) {
            throw "Missing provider generation file: $(Join-Path $full $file)."
        }
    }
    $snapshot = Get-Content -LiteralPath (Join-Path $full 'qualified/snapshot.json') -Raw | ConvertFrom-Json -Depth 100
    [void](Get-Content -LiteralPath (Join-Path $full 'endpoints.json') -Raw | ConvertFrom-Json -Depth 100)
    if (-not $snapshot.valid_until) { throw 'The provider snapshot has no valid_until timestamp.' }
    $expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
    if (($expires - [DateTimeOffset]::UtcNow).TotalHours -lt 5) {
        throw "Provider snapshot expires $($expires.ToString('o')); scenarios require at least five hours remaining."
    }
    return $full
}

function Find-LauncherProvider([string]$ProfilesRoot, [string]$Model, [string]$Endpoint) {
    $candidates = @(Get-ChildItem -LiteralPath $ProfilesRoot -Directory -Filter 'provider-*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
    foreach ($candidate in $candidates) {
        try {
            $valid = Assert-LauncherProvider $candidate.FullName
            if ($Model -or $Endpoint) {
                $snapshot = Get-Content -LiteralPath (Join-Path $valid 'qualified/snapshot.json') -Raw | ConvertFrom-Json -Depth 100
                if (($Model -and $snapshot.compatibility.model -ne $Model) -or ($Endpoint -and $snapshot.compatibility.endpoint -ne $Endpoint)) { continue }
            }
            return $valid
        }
        catch { }
    }
    return $null
}

function New-LauncherSetupContext([string]$InvocationRoot, [string]$Executable) {
    $root = Join-Path $InvocationRoot 'setup'
    $context = @{ Name = 'setup'; Root = $root; Vcp = $Executable; SkipPaidStages = $false }
    foreach ($pair in @(@('Workspace', 'workspace'), @('Data', 'vcp-data'), @('Logs', 'logs'), @('Results', 'results'))) {
        $context[$pair[0]] = Join-Path $root $pair[1]
        New-Item -ItemType Directory -Force -Path $context[$pair[0]] | Out-Null
    }
    $context.ProgressLog = Join-Path $context.Logs 'progress.log'
    $context.CommandLog = Join-Path $context.Logs 'vcp-commands.log'
    $context.CommandAuditLog = Join-Path $context.Logs 'vcp-commands.jsonl'
    return $context
}

function Restore-LauncherEnvironment([string]$Name, $Value) {
    if ($null -eq $Value) { Remove-Item -LiteralPath "Env:$Name" -ErrorAction SilentlyContinue }
    else { [Environment]::SetEnvironmentVariable($Name, [string]$Value, 'Process') }
}

function Invoke-LauncherQualification($Ctx, [string]$Model, [string]$Endpoint, [decimal]$Budget, [decimal]$RequestPrice) {
    if ($Budget -le 0 -or $Budget -gt 25 -or $RequestPrice -gt $Budget) { throw 'Setup budget must be positive, at most 25 USD, and cover the request-price ceiling.' }
    if ([decimal]::Round($Budget, 6) -ne $Budget -or [decimal]::Round($RequestPrice, 6) -ne $RequestPrice) { throw 'Provider setup amounts support at most six decimal places.' }
    foreach ($value in $Model, $Endpoint) {
        if ($value.Length -gt 256 -or $value -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -or @($value.Split('/') | Where-Object { $_ -in '.', '..' }).Count) {
            throw 'An exact model ID and endpoint tag are required for provider setup.'
        }
    }
    $generation = Join-Path $Ctx.Root ('provider-' + [guid]::NewGuid().ToString('N'))
    $format = [Globalization.CultureInfo]::InvariantCulture
    Write-Step $Ctx "Qualifying $Model at $Endpoint (up to $Budget USD; at most two requests)." 'phase'
    $run = Invoke-Vcp -Ctx $Ctx -Stage 'provider' -Label 'setup-provider' -TimeoutSeconds 1800 -Live -Arguments @(
        'setup', 'provider', '--model', $Model, '--endpoint', $Endpoint,
        '--request-price-limit', $RequestPrice.ToString('0.######', $format), '--budget-usd', $Budget.ToString('0.######', $format), '--output', $generation)
    # Delayed generation receipts can be completed without repeating inference.
    $report = Join-Path $generation 'result.json'
    if ($run.ExitCode -ne 0 -and (Test-Path -LiteralPath $report -PathType Leaf)) {
        $observed = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json -Depth 100
        if ($observed.status -eq 'observed') {
            Write-Step $Ctx 'Retrieving qualification receipts (no additional inference).' 'phase'
            $run = Invoke-Vcp -Ctx $Ctx -Stage 'provider' -Label 'setup-provider-complete' -TimeoutSeconds 300 -Live `
                -Arguments @('setup', 'provider-complete', '--directory', $generation)
        }
    }
    Write-JsonFile (Join-Path $Ctx.Results 'setup.json') ([ordered]@{
        exit_code = $run.ExitCode; budget_usd = $Budget; model = $Model; endpoint = $Endpoint
        generation = $generation; result = $run.Result; stdout = $run.StdoutPath; stderr = $run.StderrPath
    })
    Write-Utf8File (Join-Path $Ctx.Results 'summary.md') ("# Provider preparation`n`nExit: $($run.ExitCode)`n`nGeneration: $generation`n`nSetup budget: $Budget USD (separate from scenario budget).`n`nCommand log: $($Ctx.CommandLog)`n`nResult details: setup.json`n")
    if ($run.ExitCode -ne 0 -or $run.Result.data.status -ne 'qualified') {
        throw "Provider preparation failed (exit $($run.ExitCode)). No scenario was started. Inspect $($Ctx.Results) and $($run.StderrPath)."
    }
    return Assert-LauncherProvider $generation
}

function New-LauncherArguments([string]$Script, [hashtable]$Parameters, [bool]$DryRun) {
    $arguments = [Collections.Generic.List[string]]::new()
    foreach ($value in '-NoLogo', '-NoProfile', '-File', $Script) { $arguments.Add($value) }
    foreach ($name in $Parameters.Keys) {
        $arguments.Add("-$name")
        $value = $Parameters[$name]
        if ($value -is [IFormattable]) { $arguments.Add($value.ToString($null, [Globalization.CultureInfo]::InvariantCulture)) }
        else { $arguments.Add([string]$value) }
    }
    if ($DryRun) { $arguments.Add('-SkipPaidStages') }
    return $arguments.ToArray()
}

function Invoke-LauncherChild([string]$Executable, [string[]]$Arguments) {
    # The call operator keeps the child attached to this console. Do not capture
    # its output: progress and native-tool stdout/stderr must remain live.
    $PSNativeCommandUseErrorActionPreference = $false
    & $Executable @Arguments
    $script:launcherChildExitCode = $LASTEXITCODE
}

function Write-LauncherResults([string]$InvocationRoot, [string]$ScenarioName) {
    Write-Host "`nInvocation: $InvocationRoot"
    $setup = Join-Path $InvocationRoot 'setup'
    if (Test-Path -LiteralPath $setup) {
        Write-Host "Provider preparation: $setup"
        Write-Host "Setup commands: $(Join-Path $setup 'logs/vcp-commands.log')"
        Write-Host "Setup results: $(Join-Path $setup 'results')"
    }
    $scenarioPath = Join-Path $InvocationRoot $ScenarioName
    $runs = @(Get-ChildItem -LiteralPath $scenarioPath -Directory -ErrorAction SilentlyContinue)
    if ($runs.Count -ne 1) {
        Write-Host 'The scenario did not create a unique run folder; inspect the invocation directory and console error above.' -ForegroundColor Yellow
        return
    }
    $root = $runs[0].FullName
    Write-Host "Run folder: $root"
    foreach ($entry in @(@('Workspace', 'workspace'), @('Results', 'results'), @('Logs', 'logs'), @('VCP data', 'vcp-data'), @('Profiles', 'profiles'))) {
        Write-Host ('{0}: {1}' -f $entry[0], (Join-Path $root $entry[1]))
    }
    Write-Host "VCP commands: $(Join-Path $root 'logs/vcp-commands.log')"
    Write-Host "Command audit (JSONL): $(Join-Path $root 'logs/vcp-commands.jsonl')"
    $scorecard = Join-Path $root 'results/scorecard.json'
    if (Test-Path -LiteralPath $scorecard -PathType Leaf) {
        Write-Host "Scorecard: $scorecard"
        Write-Host "Summary: $(Join-Path $root 'results/summary.md')"
        try {
            $result = Get-Content -LiteralPath $scorecard -Raw | ConvertFrom-Json -Depth 100
            Write-Host "Verdict: $($result.verdict)"
        }
        catch { Write-Host 'The scorecard could not be parsed; inspect the saved file.' -ForegroundColor Yellow }
    }
    else { Write-Host 'No scorecard was produced; startup failure or interruption may have prevented finalization.' -ForegroundColor Yellow }
}

$invocationRoot = $null
$scenarioName = $null
$credentialName = 'OPENROUTER_API_KEY'
$originalKey = $null
$originalCredentialName = $env:VCP_SCENARIO_CREDENTIAL_ENV
$keyChanged = $false
$exitCode = 1
$script:launcherChildExitCode = $null
try {
    if (-not $IsWindows) { throw 'The installed VCP scenario launcher requires Windows.' }
    $harness = Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
    Write-Host 'VCP CLI scenarios (automatic preparation)'
    Write-Host 'A: Vue/Node TaskBoard   B: ASP.NET/SQL inventory   C: Java ledger   D: Python TextLab'
    if (-not $Scenario) { $Scenario = Read-LauncherChoice 'Scenario (A/B/C/D)' @('A', 'B', 'C', 'D') }
    if (-not $Mode) {
        Write-Host 'DryRun skips scenario inference. Full runs paid tasks and assessment. Missing provider qualification needs a separate setup budget in either mode.'
        $Mode = Read-LauncherChoice 'Mode (DryRun/Full; Enter = DryRun)' @('DryRun', 'Full') 'DryRun'
    }
    if (-not $PSBoundParameters.ContainsKey('RunRoot') -and -not $PSBoundParameters.ContainsKey('Scenario')) {
        $chosenRoot = (Read-Host "Test output folder (Enter = $RunRoot)").Trim().Trim('"')
        if ($chosenRoot) { $RunRoot = $chosenRoot }
    }
    $runRootFull = [IO.Path]::GetFullPath($RunRoot)
    & $harness { param($path) Assert-SafeRunRoot $path } $runRootFull
    if ($Vcp -and -not (Test-Path -LiteralPath $Vcp -PathType Leaf)) { throw "Requested VCP executable does not exist: $Vcp" }
    if (-not $Vcp -and $env:VCP_EXE -and -not (Test-Path -LiteralPath $env:VCP_EXE -PathType Leaf)) { throw "VCP_EXE does not exist: $env:VCP_EXE" }
    $executable = & $harness { param($requested) Resolve-VcpExecutable $requested } $Vcp
    $scenarioName = switch ($Scenario) {
        'A' { 'a-vue-taskboard' }; 'B' { 'b-aspnet-inventory' }; 'C' { 'c-java-ledger-cli' }; 'D' { 'd-python-textlab' }
    }
    $scenarioScript = Join-Path $PSScriptRoot "scenario-$scenarioName.ps1"
    if (-not (Test-Path -LiteralPath $scenarioScript -PathType Leaf)) { throw "Scenario script missing: $scenarioScript" }
    $pwsh = Join-Path $PSHOME 'pwsh.exe'
    if (-not (Test-Path -LiteralPath $pwsh -PathType Leaf)) { throw "PowerShell executable missing: $pwsh" }
    & $harness { param($turn, $total) Assert-ScenarioBudgetPrecision $turn 'TurnBudgetUsd'; Assert-ScenarioBudgetPrecision $total 'MaxScenarioUsd' } $TurnBudgetUsd $MaxScenarioUsd
    $generation = $null
    if ($ProviderGeneration) {
        try { $generation = Assert-LauncherProvider $ProviderGeneration }
        catch { Write-Host "The supplied provider generation cannot be reused: $($_.Exception.Message) A fresh one will be prepared." -ForegroundColor Yellow }
    }
    else {
        $generation = Find-LauncherProvider (Join-Path $env:LOCALAPPDATA 'VCP/profiles') $Model $Endpoint
        if (-not $generation) {
            foreach ($previous in @(Get-ChildItem -LiteralPath $runRootFull -Directory -Filter 'launch-*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)) {
                $generation = Find-LauncherProvider (Join-Path $previous.FullName 'setup') $Model $Endpoint
                if ($generation) { break }
            }
        }
    }
    if ($generation -and ($Model -or $Endpoint)) {
        $selected = Get-Content -LiteralPath (Join-Path $generation 'qualified/snapshot.json') -Raw | ConvertFrom-Json -Depth 100
        if (($Model -and $selected.compatibility.model -ne $Model) -or ($Endpoint -and $selected.compatibility.endpoint -ne $Endpoint)) { $generation = $null }
    }
    $invocationRoot = Join-Path $runRootFull ('launch-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $invocationRoot | Out-Null
    if ($Mode -eq 'Full' -or -not $generation) {
        if ($null -ne [Environment]::GetEnvironmentVariable('VCP_DENY_PROVIDER_CREDENTIALS', 'Process')) { throw 'VCP_DENY_PROVIDER_CREDENTIALS is set; provider preparation and Full mode cannot access credentials.' }
        $setupCtx = New-LauncherSetupContext $invocationRoot $executable
        $credentialStatus = Invoke-Vcp -Ctx $setupCtx -Stage 'account' -Label 'credential-status' -Arguments @('setup', 'credential', 'status')
        if ($credentialStatus.ExitCode -ne 0) { throw "Cannot inspect installed VCP credential selection; see $($credentialStatus.StderrPath)." }
        if ($credentialStatus.Result.data.environment) { $credentialName = [string]$credentialStatus.Result.data.environment }
        if ($credentialName.Length -gt 128 -or $credentialName -notmatch '^[A-Za-z_][A-Za-z0-9_]*$') { throw 'Unsupported credential environment-variable name.' }
        $originalKey = [Environment]::GetEnvironmentVariable($credentialName, 'Process')
        if ([string]::IsNullOrEmpty($originalKey)) {
            $key = Read-Host "$credentialName (masked, for this run only; automated CLI runs need an environment credential)" -MaskInput
            if ([string]::IsNullOrWhiteSpace($key)) { throw 'A provider key is required.' }
            [Environment]::SetEnvironmentVariable($credentialName, $key, 'Process')
            $keyChanged = $true
            $key = $null
        }
        $env:VCP_SCENARIO_CREDENTIAL_ENV = $credentialName
    }
    if (-not $generation) {
        $preferences = Invoke-Vcp -Ctx $setupCtx -Stage 'account' -Label 'models' -Arguments @('models')
        if (-not $Model -and $preferences.ExitCode -eq 0) { $Model = @($preferences.Result.data.account.set.roles.main)[0] }
        $completionPath = Join-Path $env:LOCALAPPDATA 'VCP/account/setup-complete.json'
        if (Test-Path -LiteralPath $completionPath -PathType Leaf) {
            $completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json -Depth 100
            if (-not $Model) { $Model = $completion.connection.model }
            if (-not $Endpoint -and $completion.connection.model -eq $Model) { $Endpoint = $completion.connection.endpoint }
        }
        if (-not $Model) { $Model = (Read-Host 'Provider model ID (organization/model)').Trim() }
        if (-not $Endpoint) { $Endpoint = (Read-Host "Exact provider endpoint tag for $Model").Trim() }
        Write-Host "Provider preparation: $Model at $Endpoint; request-price ceiling $RequestPriceLimitUsd USD."
        if ($SetupBudgetUsd -eq 0) {
            Write-Host 'A fresh provider qualification makes up to two paid requests. This setup cap is separate from the scenario cap, even in DryRun mode.' -ForegroundColor Yellow
            $cap = Read-Host 'Maximum setup USD (Enter authorizes 3.00; Ctrl+C cancels)'
            if (-not $cap) { $cap = '3.00' }
            $SetupBudgetUsd = [decimal]::Parse($cap, [Globalization.CultureInfo]::InvariantCulture)
        }
        $generation = Invoke-LauncherQualification $setupCtx $Model $Endpoint $SetupBudgetUsd $RequestPriceLimitUsd
    }
    Write-Host "Using provider generation: $generation"
    if ($Mode -eq 'Full') { Write-Host "Scenario budget: $TurnBudgetUsd USD per turn; $MaxScenarioUsd USD scenario ceiling (provider preparation accounted separately)." }
    $parameters = @{
        ProviderGeneration = $generation; Vcp = $executable; RunRoot = $invocationRoot
        TurnBudgetUsd = $TurnBudgetUsd; MaxScenarioUsd = $MaxScenarioUsd; MaxRepairTurns = $MaxRepairTurns
        OutputTokens = $OutputTokens; MaxRequests = $MaxRequests; DeadlineSeconds = $DeadlineSeconds; ShortDeadlineSeconds = $ShortDeadlineSeconds
    }
    $arguments = New-LauncherArguments $scenarioScript $parameters ($Mode -eq 'DryRun')
    Write-Host "`nRunning scenario $Scenario ($Mode) with $executable"
    Write-Host "Invocation: $invocationRoot"
    Write-Host 'Progress follows in this console. Ctrl+C interrupts execution; an interrupted run may not produce a final scorecard.'
    Invoke-LauncherChild $pwsh $arguments
    if ($null -ne $script:launcherChildExitCode) { $exitCode = [int]$script:launcherChildExitCode }
}
catch { Write-Host "Launcher failed: $($_.Exception.Message)" -ForegroundColor Red }
finally {
    if ($keyChanged) { Restore-LauncherEnvironment $credentialName $originalKey }
    Restore-LauncherEnvironment 'VCP_SCENARIO_CREDENTIAL_ENV' $originalCredentialName
    if ($invocationRoot) { Write-LauncherResults $invocationRoot $scenarioName }
    Write-Host "Exit code: $exitCode"
}
exit $exitCode
