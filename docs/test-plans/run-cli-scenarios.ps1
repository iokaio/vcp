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
    [string]$ProjectPath,
    [switch]$AllowProcessPublish,
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

function Get-LauncherProcessAuthorization([bool]$Allowed, [bool]$Interactive, [string]$Project) {
    $source = 'not-authorized'
    if ($Allowed) { $source = 'explicit -AllowProcessPublish' }
    elseif ($Interactive) {
        Write-Host 'VCP classifies all process launches, including local builds/tests, as capable of publishing.' -ForegroundColor Yellow
        Write-Host 'Allowing this adds publish to automatic process permissions in this run. It can also authorize actual publishing from the configured processes.' -ForegroundColor Yellow
        Write-Host 'Those processes use reduced isolation and can access the network and files outside the project; this is not a workspace sandbox.' -ForegroundColor Yellow
        $Allowed = (Read-LauncherChoice 'Allow process publishing capability for this scenario run? (Yes/No; Enter = No)' @('Yes', 'No') 'No') -eq 'Yes'
        $source = if ($Allowed) { 'interactive explicit Yes' } else { 'interactive declined' }
    }
    return [ordered]@{
        allowed = $Allowed; source = $source; workspace = $Project; at = (Get-Date).ToString('o')
        scope = 'Execution profiles generated for this scenario run; read-only and guardrail profiles unchanged.'
        reason = 'Installed VCP generic process effects include publish even for local build/test commands.'
    }
}

function Assert-LauncherProvider([string]$Generation, [switch]$AllowExpired) {
    if ([string]::IsNullOrWhiteSpace($Generation)) { throw 'An existing provider metadata directory is required.' }
    $full = [IO.Path]::GetFullPath($Generation)
    $snapshotFile = if (Test-Path -LiteralPath (Join-Path $full 'qualified/snapshot.json') -PathType Leaf) { 'qualified/snapshot.json' } else { 'snapshot.json' }
    foreach ($file in $snapshotFile, 'endpoints.json') {
        if (-not (Test-Path -LiteralPath (Join-Path $full $file) -PathType Leaf)) {
            throw "Missing provider generation file: $(Join-Path $full $file)."
        }
    }
    $snapshot = Get-Content -LiteralPath (Join-Path $full $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
    [void](Get-Content -LiteralPath (Join-Path $full 'endpoints.json') -Raw | ConvertFrom-Json -Depth 100)
    if (-not $snapshot.valid_until) { throw 'The provider snapshot has no valid_until timestamp.' }
    $expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
    if ($snapshot.raw_sha256 -and (Get-FileHash -LiteralPath (Join-Path $full 'endpoints.json') -Algorithm SHA256).Hash -ine $snapshot.raw_sha256) {
        throw 'The provider catalog does not match its retained snapshot hash.'
    }
    if (-not $AllowExpired -and $expires -le [DateTimeOffset]::UtcNow) {
        throw "Provider snapshot expired $($expires.ToString('o')); expired metadata cannot authorize scenario execution."
    }
    return $full
}

function Get-LauncherRefreshCandidate([string]$Generation, [string]$Model, [string]$Endpoint) {
    # Expired evidence can identify a renewal target, never authorize execution.
    $full = Assert-LauncherProvider $Generation -AllowExpired
    $snapshotFile = if (Test-Path -LiteralPath (Join-Path $full 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
    $snapshot = Get-Content -LiteralPath (Join-Path $full $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
    $catalog = Get-Content -LiteralPath (Join-Path $full 'endpoints.json') -Raw | ConvertFrom-Json -Depth 100
    if ($snapshot.raw_sha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'Renewal requires a retained catalog SHA256.' }
    foreach ($value in [string]$snapshot.compatibility.model, [string]$snapshot.compatibility.endpoint) {
        if ($value.Length -gt 256 -or $value -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -or @($value.Split('/') | Where-Object { $_ -in '.', '..' }).Count) { throw 'Renewal requires exact retained model and endpoint identifiers.' }
    }
    if (($Model -and $snapshot.compatibility.model -ne $Model) -or ($Endpoint -and $snapshot.compatibility.endpoint -ne $Endpoint) -or
        $catalog.data.id -ne $snapshot.compatibility.model -or @($catalog.data.endpoints | Where-Object tag -eq $snapshot.compatibility.endpoint).Count -ne 1) {
        throw 'Renewal metadata differs from the selected provider identity or captured catalog.'
    }
    $expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
    return [pscustomobject]@{ generation = $full; model = [string]$snapshot.compatibility.model; endpoint = [string]$snapshot.compatibility.endpoint; expired_at = $expires.ToString('o') }
}

function Invoke-LauncherProviderRefresh($Ctx, $Candidate) {
    $generation = Join-Path $Ctx.Root ('provider-' + [guid]::NewGuid().ToString('N'))
    $snapshotFile = if (Test-Path -LiteralPath (Join-Path $Candidate.generation 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
    $evidence = [ordered]@{ status = 'starting'; generation = $generation; prior_generation = $Candidate.generation; model = $Candidate.model; endpoint = $Candidate.endpoint; model_calls = 0; scope = 'ADR-081 endpoint metadata refresh; no inference or qualification' }
    $resultPath = Join-Path $Ctx.Results 'provider-refresh.json'
    Write-JsonFile $resultPath $evidence
    Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') @{
        source = 'same-provider metadata refresh'; status = 'refresh starting'; generation = $generation
        prior_generation = $Candidate.generation; model = $Candidate.model; endpoint = $Candidate.endpoint
        refresh_attempted = $true; model_calls = 0; refresh_evidence = $resultPath
    }
    try {
        Write-Step $Ctx "Refreshing endpoint metadata for $($Candidate.model) at $($Candidate.endpoint); no model calls or budget charge." 'phase'
        $run = Invoke-Vcp -Ctx $Ctx -Stage 'provider' -Label 'refresh-provider-metadata' -DenyProviderCredentials -TimeoutSeconds 180 -Live -Arguments @(
            'setup', 'provider-refresh', '--snapshot', (Join-Path $Candidate.generation $snapshotFile),
            '--catalog', (Join-Path $Candidate.generation 'endpoints.json'), '--output', $generation)
        $evidence.exit_code = $run.ExitCode; $evidence.stdout = $run.StdoutPath; $evidence.stderr = $run.StderrPath
        if ($run.ExitCode -ne 0 -or $run.TimedOut -or $run.Result.data.status -ne 'refreshed' -or $run.Result.data.model_calls -ne 0) { throw "Metadata refresh failed (exit $($run.ExitCode)). The installed VCP must support setup provider-refresh (ADR-081); update the executable if this command is unavailable. No paid qualification fallback is used." }
        $valid = Assert-LauncherProvider $generation
        $snapshot = Get-Content -LiteralPath (Join-Path $valid 'snapshot.json') -Raw | ConvertFrom-Json -Depth 100
        $catalog = Get-Content -LiteralPath (Join-Path $valid 'endpoints.json') -Raw | ConvertFrom-Json -Depth 100
        if ($snapshot.raw_sha256 -notmatch '^[0-9a-fA-F]{64}$' -or $snapshot.compatibility.model -ne $Candidate.model -or $snapshot.compatibility.endpoint -ne $Candidate.endpoint -or
            $catalog.data.id -ne $Candidate.model -or @($catalog.data.endpoints | Where-Object tag -eq $Candidate.endpoint).Count -ne 1) { throw 'Refreshed provider identity or catalog differs from the authorized model and endpoint.' }
        $evidence.status = 'refreshed'; $evidence.valid_until = $snapshot.valid_until
        Write-JsonFile $resultPath $evidence
        Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') @{ source = 'same-provider metadata refresh'; status = 'refreshed'; generation = $valid; model = $Candidate.model; endpoint = $Candidate.endpoint; refresh_attempted = $true; model_calls = 0; refresh_evidence = $resultPath }
        return $valid
    }
    catch {
        $evidence.status = 'failed'; $evidence.failure = $_.Exception.Message
        Write-JsonFile $resultPath $evidence
        Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') @{
            source = 'same-provider metadata refresh'; status = 'failed'; generation = $generation
            prior_generation = $Candidate.generation; model = $Candidate.model; endpoint = $Candidate.endpoint
            refresh_attempted = $true; model_calls = 0; refresh_evidence = $resultPath
        }
        throw "Provider metadata refresh failed. Inspect $resultPath. $($_.Exception.Message)"
    }
}

function Find-LauncherProvider([string]$ProfilesRoot, [string]$Model, [string]$Endpoint) {
    $candidates = @(Get-ChildItem -LiteralPath $ProfilesRoot -Directory -Filter 'provider-*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)
    foreach ($candidate in $candidates) {
        try {
            $valid = Assert-LauncherProvider $candidate.FullName
            if ($Model -or $Endpoint) {
                $snapshotFile = if (Test-Path -LiteralPath (Join-Path $valid 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
                $snapshot = Get-Content -LiteralPath (Join-Path $valid $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
                if (($Model -and $snapshot.compatibility.model -ne $Model) -or ($Endpoint -and $snapshot.compatibility.endpoint -ne $Endpoint)) { continue }
            }
            return $valid
        }
        catch { }
    }
    return $null
}

function New-LauncherSetupContext([string]$InvocationRoot, [string]$Executable, [string]$ProjectPath) {
    $root = Join-Path $InvocationRoot 'setup'
    $context = @{ Name = 'setup'; Root = $root; Vcp = $Executable; SkipPaidStages = $false }
    foreach ($pair in @(@('Workspace', 'workspace'), @('Data', 'vcp-data'), @('Logs', 'logs'), @('Results', 'results'))) {
        $context[$pair[0]] = if ($pair[0] -eq 'Workspace' -and $ProjectPath) { $ProjectPath } else { Join-Path $root $pair[1] }
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

function Resolve-LauncherInstalledProvider($Ctx, [string]$AccountRoot, [string]$RunRoot) {
    # Query the installed CLI in the selected project so project overrides win.
    # These are metadata reads; this launcher never changes provider selection.
    $preferences = Invoke-Vcp -Ctx $Ctx -Stage 'account' -Label 'models' -Arguments @('models')
    if ($preferences.ExitCode -ne 0) { throw "Cannot inspect installed provider selection; see $($preferences.StderrPath)." }
    $models = @($preferences.Result.data.effective.set.roles.main)
    if (-not $models.Count -or -not $models[0]) { throw 'Installed VCP did not report an effective main model selection.' }
    $candidates = [Collections.Generic.List[object]]::new()
    $rejected = [Collections.Generic.List[string]]::new()
    $Ctx.ExpiredProvider = $null
    # Registered profiles are owner data outside the project; never discover a
    # trusted profile by scanning arbitrary files inside the agent workspace.
    foreach ($registration in @(Get-ChildItem -LiteralPath $AccountRoot -Filter 'project-profile-*.json' -File -ErrorAction SilentlyContinue)) {
        try {
            $record = Get-Content -LiteralPath $registration.FullName -Raw | ConvertFrom-Json -Depth 100
            $registeredWorkspace = [IO.Path]::GetFullPath([string]$record.workspace).Replace('\\?\', '').TrimEnd('\', '/')
            $requestedWorkspace = [IO.Path]::GetFullPath($Ctx.Workspace).Replace('\\?\', '').TrimEnd('\', '/')
            if ($registeredWorkspace -ine $requestedWorkspace) { continue }
            $profilePath = [IO.Path]::GetFullPath([string]$record.profile)
            if ((Split-Path -Parent $profilePath).Replace('\\?\', '').TrimEnd('\') -ine [IO.Path]::GetFullPath($AccountRoot).Replace('\\?\', '').TrimEnd('\')) { throw 'Registered profile lies outside the account directory.' }
            $document = [System.Text.Json.JsonDocument]::Parse([IO.File]::ReadAllText($profilePath))
            try {
                $snapshotText = $document.RootElement.GetProperty('provider').GetRawText()
                $catalogPath = $document.RootElement.GetProperty('catalog').GetString()
                if (-not [IO.Path]::IsPathFullyQualified($catalogPath)) { throw 'Registered profile catalog must use an absolute path.' }
                $snapshot = $snapshotText | ConvertFrom-Json -Depth 100
                $candidates.Add(@{ Model = $snapshot.compatibility.model; Profile = $profilePath; SnapshotText = $snapshotText; Catalog = $catalogPath; Source = $registration.FullName })
            }
            finally { $document.Dispose() }
        }
        catch { $rejected.Add("$($registration.FullName): $($_.Exception.Message)") }
    }
    $completionPath = Join-Path $AccountRoot 'setup-complete.json'
    if (Test-Path -LiteralPath $completionPath -PathType Leaf) {
        try {
            $completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json -Depth 100
            if ($completion.version -eq 1 -and $completion.connection.status -eq 'connected' -and $completion.connection.evidence) {
                $candidates.Add(@{ Model = $completion.connection.model; Directory = [string]$completion.connection.evidence; Source = $completionPath })
            }
        }
        catch { $rejected.Add("${completionPath}: $($_.Exception.Message)") }
    }
    foreach ($model in $models) {
        $modelEndpoint = $null
        foreach ($candidate in @($candidates | Where-Object Model -eq $model)) {
            try {
                $directory = $candidate.Directory
                if ($candidate.Profile) {
                    # Retain the embedded JSON tokens and captured catalog bytes;
                    # no qualification claims or expiry timestamps are invented.
                    $directory = Join-Path $Ctx.Root ('provider-input-' + [guid]::NewGuid().ToString('N'))
                    New-Item -ItemType Directory -Path $directory | Out-Null
                    Write-Utf8File (Join-Path $directory 'snapshot.json') $candidate.SnapshotText
                    Copy-Item -LiteralPath $candidate.Catalog -Destination (Join-Path $directory 'endpoints.json')
                }
                $valid = Assert-LauncherProvider $directory -AllowExpired
                $snapshotFile = if (Test-Path -LiteralPath (Join-Path $valid 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
                $snapshot = Get-Content -LiteralPath (Join-Path $valid $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
                if ($snapshot.compatibility.model -ne $model) { throw 'Retained metadata differs from the configured model.' }
                $expires = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
                if ($expires -le [DateTimeOffset]::UtcNow) {
                    $rejected.Add("$($candidate.Source): provider metadata expired $($expires.ToString('o')).")
                    $expired = Get-LauncherRefreshCandidate $valid $model
                    if (-not $modelEndpoint) { $modelEndpoint = $expired.endpoint }
                    if (-not $Ctx.ExpiredProvider) { $Ctx.ExpiredProvider = $expired }
                    continue
                }
                Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') ([ordered]@{
                    source = $candidate.Source; profile = $candidate.Profile; generation = $valid; model = $model
                    endpoint = $snapshot.compatibility.endpoint; valid_until = $snapshot.valid_until; model_calls = 0
                    note = 'Reused installed provider metadata; no provider setup or qualification performed.'
                })
                return $valid
            }
            catch { $rejected.Add("$($candidate.Source): $($_.Exception.Message)") }
        }
        $cached = Find-LauncherProvider (Join-Path (Split-Path -Parent $AccountRoot) 'profiles') $model $modelEndpoint
        if (-not $cached) {
            foreach ($previous in @(Get-ChildItem -LiteralPath $RunRoot -Directory -Filter 'launch-*' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending)) {
                $cached = Find-LauncherProvider (Join-Path $previous.FullName 'setup') $model $modelEndpoint
                if ($cached) { break }
            }
        }
        if ($cached) {
            Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') @{ source = $cached; generation = $cached; model = $model; model_calls = 0 }
            return $cached
        }
    }
    Write-JsonFile (Join-Path $Ctx.Results 'provider-selection.json') @{ status = $(if ($Ctx.ExpiredProvider) { 'expired' } else { 'unavailable' }); configured_models = $models; rejected = @($rejected); model_calls = 0; refresh_candidate = $Ctx.ExpiredProvider }
    if ($Ctx.ExpiredProvider) { return $null }
    throw "No current retained metadata was found for the installed model selection ($($models -join ', ')). Provider setup was not changed. Details: $(Join-Path $Ctx.Results 'provider-selection.json')."
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

function Invoke-LauncherChild([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory) {
    # The call operator keeps the child attached to this console. Do not capture
    # its output: progress and native-tool stdout/stderr must remain live.
    $PSNativeCommandUseErrorActionPreference = $false
    Push-Location -LiteralPath $WorkingDirectory
    try {
        & $Executable @Arguments
        $script:launcherChildExitCode = $LASTEXITCODE
    }
    finally { Pop-Location }
}

function Write-LauncherResults([string]$InvocationRoot, [string]$ScenarioName, [string]$ProjectPath) {
    Write-Host "`nInvocation: $InvocationRoot"
    if ($ProjectPath) { Write-Host "Workspace: $ProjectPath" }
    $setup = Join-Path $InvocationRoot 'setup'
    if (Test-Path -LiteralPath $setup) {
        Write-Host "Provider selection: $setup"
        Write-Host "Selection commands: $(Join-Path $setup 'logs/vcp-commands.log')"
        Write-Host "Selection results: $(Join-Path $setup 'results')"
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
        if ($ProjectPath -and $entry[0] -eq 'Workspace') { continue }
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
        Write-Host 'DryRun skips provider inference. Full runs paid tasks and assessment with your configured provider.'
        $Mode = Read-LauncherChoice 'Mode (DryRun/Full; Enter = DryRun)' @('DryRun', 'Full') 'DryRun'
    }
    $runRootFull = [IO.Path]::GetFullPath($RunRoot)
    & $harness { param($path) Assert-SafeRunRoot $path } $runRootFull
    if ($Vcp -and -not (Test-Path -LiteralPath $Vcp -PathType Leaf)) { throw "Requested VCP executable does not exist: $Vcp" }
    if (-not $Vcp -and $env:VCP_EXE -and -not (Test-Path -LiteralPath $env:VCP_EXE -PathType Leaf)) { throw "VCP_EXE does not exist: $env:VCP_EXE" }
    $executable = & $harness { param($requested) Resolve-VcpExecutable $requested } $Vcp
    $scenarioName = switch ($Scenario) {
        'A' { 'a-vue-taskboard' }; 'B' { 'b-aspnet-inventory' }; 'C' { 'c-java-ledger-cli' }; 'D' { 'd-python-textlab' }
    }
    if (-not $ProjectPath) {
        $ProjectPath = Join-Path $runRootFull "projects/$scenarioName"
        if (-not $PSBoundParameters.ContainsKey('Scenario')) {
            $chosenProject = (Read-Host "Project folder (reuse or create; Enter = $ProjectPath)").Trim().Trim('"')
            if ($chosenProject) { $ProjectPath = $chosenProject }
        }
    }
    $ProjectPath = [IO.Path]::GetFullPath($ProjectPath)
    & $harness { param($path) Assert-ScenarioProjectPath $path } $ProjectPath
    if ((Test-Path -LiteralPath $ProjectPath) -and -not (Test-Path -LiteralPath $ProjectPath -PathType Container)) { throw 'ProjectPath must identify a directory.' }
    $projectPrefix = $ProjectPath.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($runRootFull -eq $ProjectPath -or $runRootFull.StartsWith($projectPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'RunRoot must be outside the project so VCP logs, data and profiles remain separate.'
    }
    $scenarioScript = Join-Path $PSScriptRoot "scenario-$scenarioName.ps1"
    if (-not (Test-Path -LiteralPath $scenarioScript -PathType Leaf)) { throw "Scenario script missing: $scenarioScript" }
    $pwsh = Join-Path $PSHOME 'pwsh.exe'
    if (-not (Test-Path -LiteralPath $pwsh -PathType Leaf)) { throw "PowerShell executable missing: $pwsh" }
    & $harness { param($turn, $total) Assert-ScenarioBudgetPrecision $turn 'TurnBudgetUsd'; Assert-ScenarioBudgetPrecision $total 'MaxScenarioUsd' } $TurnBudgetUsd $MaxScenarioUsd
    $projectExisted = Test-Path -LiteralPath $ProjectPath -PathType Container
    if (-not $projectExisted) { New-Item -ItemType Directory -Path $ProjectPath -Force | Out-Null }
    Write-Host ("Project: {0} ({1})" -f $ProjectPath, $(if ($projectExisted) { 'using existing directory' } else { 'created' }))
    $generation = $null
    $explicitExpired = $null
    if ($ProviderGeneration) {
        $generation = Assert-LauncherProvider $ProviderGeneration -AllowExpired
        $snapshotFile = if (Test-Path -LiteralPath (Join-Path $generation 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
        $selectedSnapshot = Get-Content -LiteralPath (Join-Path $generation $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
        if ([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$selectedSnapshot.valid_until) -le [DateTimeOffset]::UtcNow) {
            $explicitExpired = Get-LauncherRefreshCandidate $generation
            $generation = $null
        }
    }
    $invocationRoot = Join-Path $runRootFull ('launch-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $invocationRoot | Out-Null
    $setupCtx = $null
    if ($Mode -eq 'Full' -or -not $generation) { $setupCtx = New-LauncherSetupContext $invocationRoot $executable $ProjectPath }
    if ($explicitExpired) {
        $setupCtx.ExpiredProvider = $explicitExpired
        Write-JsonFile (Join-Path $setupCtx.Results 'provider-selection.json') @{ source = 'explicit ProviderGeneration'; status = 'expired'; refresh_candidate = $explicitExpired; model_calls = 0 }
    }
    elseif (-not $generation) { $generation = Resolve-LauncherInstalledProvider $setupCtx (Join-Path $env:LOCALAPPDATA 'VCP/account') $runRootFull }
    elseif ($setupCtx) {
        Write-JsonFile (Join-Path $setupCtx.Results 'provider-selection.json') @{ source = 'explicit ProviderGeneration'; generation = $generation; model_calls = 0 }
    }
    $canPrompt = -not $PSBoundParameters.ContainsKey('Scenario') -and
        -not @([Environment]::GetCommandLineArgs() | Where-Object { $_ -match '^-NonI' }).Count
    $metadataRefreshed = $false
    if ($generation -and $Mode -eq 'Full') {
        $snapshotFile = if (Test-Path -LiteralPath (Join-Path $generation 'qualified/snapshot.json')) { 'qualified/snapshot.json' } else { 'snapshot.json' }
        $selectedSnapshot = Get-Content -LiteralPath (Join-Path $generation $snapshotFile) -Raw | ConvertFrom-Json -Depth 100
        if ([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$selectedSnapshot.valid_until) -le [DateTimeOffset]::UtcNow.AddSeconds($DeadlineSeconds + 300)) {
            $setupCtx.ExpiredProvider = Get-LauncherRefreshCandidate $generation
            $generation = $null
        }
    }
    if (-not $generation) {
        $generation = Invoke-LauncherProviderRefresh $setupCtx $setupCtx.ExpiredProvider
        $metadataRefreshed = $true
    }
    if ($Mode -eq 'Full') {
        $processAuthorization = Get-LauncherProcessAuthorization ([bool]$AllowProcessPublish) $canPrompt $ProjectPath
        Write-JsonFile (Join-Path $setupCtx.Results 'process-authorization.json') $processAuthorization
        if (-not $processAuthorization.allowed) {
            throw 'Full mode stopped before inference: process permission was not authorized. Review setup/results/process-authorization.json. To explicitly authorize this capability, rerun with -AllowProcessPublish or choose Yes in the interactive launcher.'
        }
        $AllowProcessPublish = $true
        if ($null -ne [Environment]::GetEnvironmentVariable('VCP_DENY_PROVIDER_CREDENTIALS', 'Process')) { throw 'VCP_DENY_PROVIDER_CREDENTIALS is set; Full mode cannot access credentials.' }
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
    Write-Host "Using configured provider metadata: $generation"
    if ($setupCtx) {
        $preparation = if ($metadataRefreshed) { 'Endpoint metadata refreshed without inference using retained adapter evidence. Scenario budget unchanged. See provider-refresh.json.' } else { 'No provider setup or qualification performed.' }
        Write-Utf8File (Join-Path $setupCtx.Results 'summary.md') ("# Installed provider selection`n`nMetadata: $generation`n`n$preparation`n`nCommands: $($setupCtx.CommandLog)`n`nSelection evidence: provider-selection.json`n")
    }
    if ($Mode -eq 'Full') { Write-Host "Scenario budget: $TurnBudgetUsd USD per turn; $MaxScenarioUsd USD scenario ceiling." }
    $parameters = @{
        ProviderGeneration = $generation; Vcp = $executable; RunRoot = $invocationRoot; ProjectPath = $ProjectPath
        TurnBudgetUsd = $TurnBudgetUsd; MaxScenarioUsd = $MaxScenarioUsd; MaxRepairTurns = $MaxRepairTurns
        OutputTokens = $OutputTokens; MaxRequests = $MaxRequests; DeadlineSeconds = $DeadlineSeconds; ShortDeadlineSeconds = $ShortDeadlineSeconds
    }
    $arguments = New-LauncherArguments $scenarioScript $parameters ($Mode -eq 'DryRun')
    if ($AllowProcessPublish) { $arguments += '-AllowProcessPublish' }
    Write-Host "`nRunning scenario $Scenario ($Mode) with $executable"
    Write-Host "Invocation: $invocationRoot"
    Write-Host 'Progress follows in this console. Ctrl+C interrupts execution; an interrupted run may not produce a final scorecard.'
    Invoke-LauncherChild $pwsh $arguments $ProjectPath
    if ($null -ne $script:launcherChildExitCode) { $exitCode = [int]$script:launcherChildExitCode }
}
catch { Write-Host "Launcher failed: $($_.Exception.Message)" -ForegroundColor Red }
finally {
    if ($keyChanged) { Restore-LauncherEnvironment $credentialName $originalKey }
    Restore-LauncherEnvironment 'VCP_SCENARIO_CREDENTIAL_ENV' $originalCredentialName
    if ($invocationRoot) { Write-LauncherResults $invocationRoot $scenarioName $ProjectPath }
    Write-Host "Exit code: $exitCode"
}
exit $exitCode
