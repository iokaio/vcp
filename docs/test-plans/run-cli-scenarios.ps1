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
            throw "Missing provider generation file: $(Join-Path $full $file). Prepare a generation with vcp setup provider separately; the launcher never runs qualification automatically."
        }
    }
    $snapshot = Get-Content -LiteralPath (Join-Path $full 'qualified/snapshot.json') -Raw | ConvertFrom-Json -Depth 100
    [void](Get-Content -LiteralPath (Join-Path $full 'endpoints.json') -Raw | ConvertFrom-Json -Depth 100)
    if (-not $snapshot.valid_until) { throw 'The provider snapshot has no valid_until timestamp.' }
    $expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
    if (($expires - [DateTimeOffset]::UtcNow).TotalHours -lt 5) {
        throw "Provider snapshot expires $($expires.ToString('o')); scenarios require at least five hours remaining. Prepare a fresh generation separately."
    }
    return $full
}

function Find-LauncherProvider([string]$ProfilesRoot) {
    $candidates = @(Get-ChildItem -LiteralPath $ProfilesRoot -Directory -Filter 'provider-*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
    foreach ($candidate in $candidates) {
        try { return Assert-LauncherProvider $candidate.FullName } catch { }
    }
    return $null
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
$originalKey = [Environment]::GetEnvironmentVariable('OPENROUTER_API_KEY', 'Process')
$keyChanged = $false
$exitCode = 1
$script:launcherChildExitCode = $null
try {
    if (-not $IsWindows) { throw 'The installed VCP scenario launcher requires Windows.' }
    $harness = Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
    Write-Host 'VCP CLI scenarios'
    Write-Host 'A: Vue/Node TaskBoard   B: ASP.NET/SQL inventory   C: Java ledger   D: Python TextLab'
    if (-not $Scenario) { $Scenario = Read-LauncherChoice 'Scenario (A/B/C/D)' @('A', 'B', 'C', 'D') }
    if (-not $Mode) {
        Write-Host 'DryRun checks setup and baseline without provider inference. Full runs paid tasks and assessment.'
        $Mode = Read-LauncherChoice 'Mode (DryRun/Full; Enter = DryRun)' @('DryRun', 'Full') 'DryRun'
    }
    if (-not $ProviderGeneration) {
        $ProviderGeneration = Find-LauncherProvider (Join-Path $env:LOCALAPPDATA 'VCP/profiles')
        if ($ProviderGeneration) { Write-Host "Using existing provider generation: $ProviderGeneration" }
        else { $ProviderGeneration = (Read-Host 'Existing provider generation directory').Trim().Trim('"') }
    }
    $generation = Assert-LauncherProvider $ProviderGeneration
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
    if ($Mode -eq 'Full') {
        if ($env:VCP_DENY_PROVIDER_CREDENTIALS) { throw 'VCP_DENY_PROVIDER_CREDENTIALS is set; choose DryRun or clear it explicitly before a Full run.' }
        Write-Host "Full run budget: $TurnBudgetUsd USD per turn; $MaxScenarioUsd USD scenario ceiling."
        if ([string]::IsNullOrEmpty($originalKey)) {
            $key = Read-Host 'OPENROUTER_API_KEY (masked, for this run only)' -MaskInput
            if ([string]::IsNullOrWhiteSpace($key)) { throw 'A provider key is required for Full mode.' }
            [Environment]::SetEnvironmentVariable('OPENROUTER_API_KEY', $key, 'Process')
            $keyChanged = $true
            $key = $null
        }
    }
    $invocationRoot = Join-Path $runRootFull ('launch-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $invocationRoot | Out-Null
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
    if ($keyChanged) { [Environment]::SetEnvironmentVariable('OPENROUTER_API_KEY', $originalKey, 'Process') }
    if ($invocationRoot) { Write-LauncherResults $invocationRoot $scenarioName }
    Write-Host "Exit code: $exitCode"
}
exit $exitCode
