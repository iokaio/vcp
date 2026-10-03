# SPDX-License-Identifier: Apache-2.0
#
# Shared harness for the long-running VCP CLI practical scenarios described in
# cli-test-plans.md. Each scenario script imports this module, owns its own run
# root, data directory, profiles, ports and logs, and can run concurrently with
# the other scenarios in a separate console window.
#
# The harness never prints or persists credential values. VCP reads the
# provider credential from OPENROUTER_API_KEY in the launching console.

Set-StrictMode -Off
$ErrorActionPreference = 'Stop'

$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

# Paths that are dependency caches or build outputs. They are excluded from
# workspace manifests so diffs reflect authored changes.
$script:ManifestExclusions = @(
    'node_modules', '.git', 'bin', 'obj', 'target', '.venv', 'venv', '__pycache__',
    '.pytest_cache', '.mypy_cache', '.ruff_cache', 'dist', 'build', '.vs', '.idea',
    'artifacts', 'models', 'models-repro', 'reports', '.vite', 'coverage', '.egg-info'
)

#region Utilities

function Write-Utf8File {
    param([Parameter(Mandatory)][string]$Path, [AllowEmptyString()][string]$Content)
    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Content, $script:Utf8NoBom)
}

function Write-JsonFile {
    param([Parameter(Mandatory)][string]$Path, $Value)
    Write-Utf8File -Path $Path -Content ($Value | ConvertTo-Json -Depth 64)
}

function Write-SeedFiles {
    <# Writes a hashtable of relative path => content below Root. #>
    param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][System.Collections.IDictionary]$Files)
    foreach ($relative in $Files.Keys) {
        Write-Utf8File -Path (Join-Path $Root $relative) -Content $Files[$relative]
    }
}

function Get-TextTail {
    param([string]$Path, [int]$Lines = 60)
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return '' }
    return ((Get-Content -LiteralPath $Path -Tail $Lines -ErrorAction SilentlyContinue) -join "`n")
}

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-That {
    <# Gate helper: throws the message when the condition is false. #>
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function ConvertTo-Instant {
    <# PowerShell 7 ConvertFrom-Json already turns ISO-8601 strings into DateTime; accept both. #>
    param($Value)
    if ($Value -is [datetime]) { return $Value.ToUniversalTime() }
    if ($Value -is [datetimeoffset]) { return $Value.UtcDateTime }
    return [datetimeoffset]::Parse([string]$Value, [System.Globalization.CultureInfo]::InvariantCulture).UtcDateTime
}

function Format-Usd {
    param([decimal]$Value)
    return $Value.ToString('0.00', [System.Globalization.CultureInfo]::InvariantCulture)
}

function ConvertFrom-JsonLines {
    <# Parses JSONL text. Invalid lines are returned separately; they are evidence, not ignored. #>
    param([AllowEmptyString()][string]$Text)
    $frames = [System.Collections.Generic.List[object]]::new()
    $invalid = [System.Collections.Generic.List[string]]::new()
    foreach ($line in ($Text -split "`r?`n")) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try { $frames.Add(($line | ConvertFrom-Json -Depth 100)) } catch { $invalid.Add($line) }
    }
    return [pscustomobject]@{ Frames = $frames; Invalid = $invalid }
}

function Write-Step {
    param([Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][string]$Message,
        [ValidateSet('info', 'ok', 'warn', 'fail', 'phase')][string]$Level = 'info')
    $prefix = '[{0}][{1}]' -f (Get-Date).ToString('HH:mm:ss'), $Ctx.Name
    $color = @{ info = 'Gray'; ok = 'Green'; warn = 'Yellow'; fail = 'Red'; phase = 'Cyan' }[$Level]
    Write-Host "$prefix $Message" -ForegroundColor $color
    Add-Content -LiteralPath $Ctx.ProgressLog -Value "$prefix [$Level] $Message" -Encoding utf8NoBOM
}

function Find-Executable {
    <# Resolves an absolute .exe path from candidates, then PATH. #>
    param([Parameter(Mandatory)][string]$Name, [string[]]$Candidates = @())
    foreach ($candidate in $Candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return (Resolve-Path -LiteralPath $candidate).Path }
    }
    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue |
        Where-Object { $_.Source -like '*.exe' } | Select-Object -First 1
    if ($command) { return $command.Source }
    return $null
}

#endregion

#region Native process execution

function Invoke-NativeLogged {
    <#
    Runs a native executable without a shell. Arguments go through
    ProcessStartInfo.ArgumentList, so no quoting is reinterpreted. Stdout is
    streamed line by line to a file so long runs can be observed; stderr is
    drained concurrently to avoid pipe deadlock. A timeout kills the tree.
    #>
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = (Get-Location).Path,
        [Parameter(Mandatory)][string]$StdoutPath,
        [Parameter(Mandatory)][string]$StderrPath,
        [int]$TimeoutSeconds = 600,
        [hashtable]$Environment = @{},
        [scriptblock]$OnLine,
        [string]$HeartbeatLabel,
        $Ctx
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new($FilePath)
    foreach ($argument in $ArgumentList) { $psi.ArgumentList.Add([string]$argument) }
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = $script:Utf8NoBom
    $psi.StandardErrorEncoding = $script:Utf8NoBom
    foreach ($key in $Environment.Keys) { $psi.Environment[$key] = [string]$Environment[$key] }

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $StdoutPath) | Out-Null
    $started = Get-Date
    $process = [System.Diagnostics.Process]::Start($psi)
    $process.StandardInput.Close()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $writer = [System.IO.StreamWriter]::new($StdoutPath, $false, $script:Utf8NoBom)
    $timedOut = $false
    $lines = 0
    $nextHeartbeat = $started.AddSeconds(30)
    try {
        $lineTask = $process.StandardOutput.ReadLineAsync()
        while ($true) {
            if ($lineTask.Wait(1000)) {
                $line = $lineTask.Result
                if ($null -eq $line) { break }
                $writer.WriteLine($line)
                $writer.Flush()
                $lines++
                if ($OnLine) { try { & $OnLine $line } catch { } }
                $lineTask = $process.StandardOutput.ReadLineAsync()
                continue
            }
            $now = Get-Date
            if (($now - $started).TotalSeconds -gt $TimeoutSeconds) {
                $timedOut = $true
                try { $process.Kill($true) } catch { }
                break
            }
            if ($Ctx -and $HeartbeatLabel -and $now -ge $nextHeartbeat) {
                Write-Step $Ctx ('{0} still running ({1:hh\:mm\:ss}, {2} stdout lines)' -f $HeartbeatLabel, ($now - $started), $lines)
                $nextHeartbeat = $now.AddSeconds(60)
            }
        }
        if (-not $timedOut) { $process.WaitForExit() }
        else { [void]$process.WaitForExit(15000) }
    }
    finally {
        $writer.Dispose()
    }
    $stderr = ''
    if ($stderrTask.Wait(15000)) { $stderr = $stderrTask.Result }
    Write-Utf8File -Path $StderrPath -Content $stderr
    $exitCode = if ($timedOut) { -1 } else { $process.ExitCode }
    return [pscustomobject]@{
        ExitCode        = $exitCode
        TimedOut        = $timedOut
        DurationSeconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
        StdoutPath      = $StdoutPath
        StderrPath      = $StderrPath
        StdoutLines     = $lines
    }
}

function Invoke-Tool {
    <#
    Runs a toolchain command (npm, dotnet, mvn, python, java) for seeding or
    independent verification. Output is logged under logs/<Stage>/tools.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory,
        [int]$TimeoutSeconds = 900,
        [hashtable]$Environment = @{}
    )
    if (-not $WorkingDirectory) { $WorkingDirectory = $Ctx.Workspace }
    $safe = ($Label -replace '[^A-Za-z0-9_.-]', '-')
    $directory = Join-Path $Ctx.Logs (Join-Path $Stage 'tools')
    $index = '{0:D2}' -f ((Get-ChildItem -LiteralPath $directory -Filter '*.out.log' -ErrorAction SilentlyContinue | Measure-Object).Count + 1)
    $stdout = Join-Path $directory "$index-$safe.out.log"
    $stderr = Join-Path $directory "$index-$safe.err.log"
    $result = Invoke-NativeLogged -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory `
        -StdoutPath $stdout -StderrPath $stderr -TimeoutSeconds $TimeoutSeconds -Environment $Environment `
        -HeartbeatLabel "$Stage/$Label" -Ctx $Ctx
    $line = '{0:o} [{1}] {2} exit={3} {4}s :: {5} {6}' -f (Get-Date), $Stage, $Label, $result.ExitCode, $result.DurationSeconds, $FilePath, ($ArgumentList -join ' ')
    Add-Content -LiteralPath $Ctx.ToolLog -Value $line -Encoding utf8NoBOM
    $result | Add-Member -NotePropertyName Output -NotePropertyValue ([System.IO.File]::ReadAllText($stdout))
    $result | Add-Member -NotePropertyName Errors -NotePropertyValue ([System.IO.File]::ReadAllText($stderr))
    return $result
}

function Start-BackgroundServer {
    <#
    Starts an app under test (API/web server) with redirected logs and waits
    until ReadyUrl answers. Returns the process; stop it with Stop-BackgroundServer.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory,
        [hashtable]$Environment = @{},
        [Parameter(Mandatory)][string]$ReadyUrl,
        [int]$ReadySeconds = 90
    )
    if (-not $WorkingDirectory) { $WorkingDirectory = $Ctx.Workspace }
    $directory = Join-Path $Ctx.Logs (Join-Path $Stage 'servers')
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $safe = ($Label -replace '[^A-Za-z0-9_.-]', '-')
    $psi = [System.Diagnostics.ProcessStartInfo]::new($FilePath)
    foreach ($argument in $ArgumentList) { $psi.ArgumentList.Add([string]$argument) }
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    foreach ($key in $Environment.Keys) { $psi.Environment[$key] = [string]$Environment[$key] }
    $process = [System.Diagnostics.Process]::Start($psi)
    $process.StandardInput.Close()
    # Drain both pipes into files so the server never blocks on a full pipe.
    $outLog = Join-Path $directory "$safe.out.log"
    $errLog = Join-Path $directory "$safe.err.log"
    $outFile = [System.IO.FileStream]::new($outLog, 'Create', 'Write', 'ReadWrite')
    $errFile = [System.IO.FileStream]::new($errLog, 'Create', 'Write', 'ReadWrite')
    $outTask = $process.StandardOutput.BaseStream.CopyToAsync($outFile)
    $errTask = $process.StandardError.BaseStream.CopyToAsync($errFile)
    $server = [pscustomobject]@{
        Process = $process; OutLog = $outLog; ErrLog = $errLog; Label = $Label
        Tasks = @($outTask, $errTask); Streams = @($outFile, $errFile)
    }
    $deadline = (Get-Date).AddSeconds($ReadySeconds)
    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited) { break }
        try {
            $response = Invoke-WebRequest -Uri $ReadyUrl -TimeoutSec 5 -SkipHttpErrorCheck -ErrorAction Stop
            if ($response.StatusCode -lt 500) { return $server }
        }
        catch { }
        Start-Sleep -Milliseconds 750
    }
    Stop-BackgroundServer $server
    throw ("{0} did not become ready at {1} within {2}s. stderr tail:`n{3}" -f $Label, $ReadyUrl, $ReadySeconds, (Get-TextTail $errLog 30))
}

function Stop-BackgroundServer {
    param($Server)
    if (-not $Server) { return }
    try { if (-not $Server.Process.HasExited) { $Server.Process.Kill($true); [void]$Server.Process.WaitForExit(15000) } } catch { }
    foreach ($task in $Server.Tasks) { try { [void]$task.Wait(5000) } catch { } }
    foreach ($stream in $Server.Streams) { try { $stream.Dispose() } catch { } }
}

function Invoke-Http {
    <# Deterministic HTTP probe. Returns status, headers, raw content and parsed JSON when present. #>
    param(
        [Parameter(Mandatory)][string]$Method,
        [Parameter(Mandatory)][string]$Uri,
        $Body,
        [hashtable]$Headers = @{},
        [Microsoft.PowerShell.Commands.WebRequestSession]$Session,
        [string]$ContentType = 'application/json'
    )
    $parameters = @{ Method = $Method; Uri = $Uri; Headers = $Headers; SkipHttpErrorCheck = $true; SkipHeaderValidation = $true; TimeoutSec = 30; ErrorAction = 'Stop' }
    if ($Session) { $parameters.WebSession = $Session }
    if ($null -ne $Body) {
        if ($Body -is [System.Collections.IDictionary] -and $ContentType -eq 'application/x-www-form-urlencoded') {
            $parameters.Body = $Body
        }
        elseif ($Body -is [string]) { $parameters.Body = $Body }
        else { $parameters.Body = ($Body | ConvertTo-Json -Depth 20 -Compress) }
        $parameters.ContentType = $ContentType
    }
    $response = Invoke-WebRequest @parameters
    $json = $null
    $content = [string]$response.Content
    if ($content -and ($content.TrimStart().StartsWith('{') -or $content.TrimStart().StartsWith('['))) {
        try { $json = $content | ConvertFrom-Json -Depth 50 } catch { }
    }
    return [pscustomobject]@{ Status = [int]$response.StatusCode; Headers = $response.Headers; Content = $content; Json = $json }
}

#endregion

#region Scenario lifecycle

function Assert-SafeRunRoot {
    <# VCP rejects data/profiles inside repositories or OneDrive roots; fail before any spend. #>
    param([Parameter(Mandatory)][string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    foreach ($name in 'OneDrive', 'OneDriveConsumer', 'OneDriveCommercial') {
        $sync = [Environment]::GetEnvironmentVariable($name)
        if ($sync -and $full.StartsWith([System.IO.Path]::GetFullPath($sync), [StringComparison]::OrdinalIgnoreCase)) {
            throw "Run root '$full' is inside a OneDrive root ($name). Choose a local, non-synchronized -RunRoot."
        }
    }
    $probe = $full
    while ($probe) {
        if (Test-Path -LiteralPath (Join-Path $probe '.git')) {
            throw "Run root '$full' is inside a git repository ('$probe'). VCP data and profiles must live outside repositories."
        }
        $parent = Split-Path -Parent $probe
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
    if ($full -like '\\*') { throw "Run root '$full' is a network path; use a local drive." }
}

function Resolve-VcpExecutable {
    param([string]$Requested)
    $candidates = @($Requested, $env:VCP_EXE, (Join-Path $env:LOCALAPPDATA 'Programs\VCP\vcp.exe'))
    $found = Find-Executable -Name 'vcp' -Candidates $candidates
    if (-not $found) { throw 'vcp.exe not found. Pass -Vcp <path>, set VCP_EXE, or install VCP to %LOCALAPPDATA%\Programs\VCP.' }
    return $found
}

function Initialize-VcpScenario {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$RunRoot,
        [string]$Vcp,
        [Parameter(Mandatory)][string]$ProviderGeneration,
        [decimal]$TurnBudgetUsd = 3,
        [decimal]$MaxScenarioUsd = 30,
        [int]$MaxRepairTurns = 1,
        [int]$OutputTokens = 8192,
        [int]$MaxRequests = 96,
        [int]$DeadlineSeconds = 1800,
        [int]$ShortDeadlineSeconds = 150,
        [double]$MinSnapshotHours = 5,
        [switch]$SkipPaidStages
    )
    if (-not $IsWindows) { throw 'The VCP native CLI scenarios require Windows.' }
    if ($PSVersionTable.PSVersion -lt [version]'7.4') { throw 'PowerShell 7.4 or later is required.' }
    $runId = '{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), ([guid]::NewGuid().ToString('N').Substring(0, 6))
    $runRootFull = [System.IO.Path]::GetFullPath($RunRoot)
    Assert-SafeRunRoot $runRootFull
    $root = Join-Path $runRootFull (Join-Path $Name $runId)
    $ctx = @{
        Name                 = $Name
        RunId                = $runId
        Root                 = $root
        Workspace            = Join-Path $root 'workspace'
        Data                 = Join-Path $root 'vcp-data'
        Profiles             = Join-Path $root 'profiles'
        Logs                 = Join-Path $root 'logs'
        Hidden               = Join-Path $root 'hidden'
        Results              = Join-Path $root 'results'
        Temp                 = Join-Path $root 'tmp'
        Env                  = Join-Path $root 'env'
        TurnBudgetUsd        = $TurnBudgetUsd
        MaxScenarioUsd       = $MaxScenarioUsd
        MaxRepairTurns       = $MaxRepairTurns
        OutputTokens         = $OutputTokens
        MaxRequests          = $MaxRequests
        DeadlineSeconds      = $DeadlineSeconds
        ShortDeadlineSeconds = $ShortDeadlineSeconds
        SkipPaidStages       = [bool]$SkipPaidStages
        SpentUsd             = [decimal]0
        CostUnknown          = $false
        Gates                = [System.Collections.Generic.List[object]]::new()
        Stages               = [System.Collections.Generic.List[object]]::new()
        Notes                = [System.Collections.Generic.List[string]]::new()
        Assets               = [System.Collections.Generic.List[object]]::new()
        Started              = Get-Date
        Fatal                = $null
    }
    foreach ($key in 'Workspace', 'Data', 'Profiles', 'Logs', 'Hidden', 'Results', 'Temp', 'Env') {
        New-Item -ItemType Directory -Force -Path $ctx[$key] | Out-Null
    }
    $ctx.ProgressLog = Join-Path $ctx.Logs 'progress.log'
    $ctx.CommandLog = Join-Path $ctx.Logs 'vcp-commands.log'
    $ctx.ToolLog = Join-Path $ctx.Logs 'tool-commands.log'
    Start-Transcript -LiteralPath (Join-Path $ctx.Logs 'console-transcript.log') | Out-Null
    $ctx.Transcript = $true
    Write-Step $ctx "Run root: $root" 'phase'

    $ctx.Vcp = Resolve-VcpExecutable $Vcp
    $generation = [System.IO.Path]::GetFullPath($ProviderGeneration)
    $ctx.Snapshot = Join-Path $generation 'qualified\snapshot.json'
    $ctx.Catalog = Join-Path $generation 'endpoints.json'
    foreach ($required in $ctx.Snapshot, $ctx.Catalog) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Provider generation file missing: $required. Run 'vcp setup provider' once (plan section 2.2)."
        }
    }
    $ctx.SnapshotText = [System.IO.File]::ReadAllText($ctx.Snapshot)
    $snapshot = $ctx.SnapshotText | ConvertFrom-Json -Depth 100
    $validUntil = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
    $hoursLeft = ($validUntil - [DateTimeOffset]::UtcNow).TotalHours
    $ctx.SnapshotValidUntil = $validUntil.ToString('o')
    $ctx.Model = $snapshot.compatibility.model
    $ctx.Endpoint = $snapshot.compatibility.endpoint
    $maxOutput = [int64]$snapshot.max_output
    if ($maxOutput -gt 0 -and $ctx.OutputTokens -gt $maxOutput) {
        $ctx.Notes.Add("Output tokens clamped from $($ctx.OutputTokens) to endpoint max_output $maxOutput.")
        $ctx.OutputTokens = [int]$maxOutput
    }
    if ($hoursLeft -lt $MinSnapshotHours) {
        throw ('Provider snapshot expires {0} ({1:N1}h left); renew with vcp setup provider into a new directory before a long run.' -f $ctx.SnapshotValidUntil, $hoursLeft)
    }
    if (-not $SkipPaidStages -and [string]::IsNullOrEmpty($env:OPENROUTER_API_KEY)) {
        throw 'OPENROUTER_API_KEY is not set in this console. Set it with a masked prompt (plan section 2.3); the harness never logs it.'
    }
    if ($env:VCP_DENY_PROVIDER_CREDENTIALS -and -not $SkipPaidStages) {
        throw 'VCP_DENY_PROVIDER_CREDENTIALS is set; paid stages cannot run. Remove it or pass -SkipPaidStages.'
    }
    Write-Step $ctx ("vcp: {0}; model {1} endpoint {2}; snapshot valid until {3}" -f $ctx.Vcp, $ctx.Model, $ctx.Endpoint, $ctx.SnapshotValidUntil)
    return $ctx
}

function Add-GateResult {
    param($Ctx, [string]$Stage, [string]$Id, [string]$Description, [string]$Outcome, [string]$Detail, [bool]$Required)
    $entry = [pscustomobject]@{
        stage = $Stage; id = $Id; description = $Description; outcome = $Outcome
        required = $Required; detail = $Detail; at = (Get-Date).ToString('o')
    }
    $Ctx.Gates.Add($entry)
    $level = switch ($Outcome) { 'pass' { 'ok' } 'skip' { 'warn' } default { if ($Required) { 'fail' } else { 'warn' } } }
    $suffix = if ($Detail -and $Outcome -ne 'pass') { " :: $Detail" } else { '' }
    Write-Step $Ctx ("gate {0}/{1} [{2}] {3}{4}" -f $Stage, $Id, $Outcome.ToUpperInvariant(), $Description, $suffix) $level
    return $entry
}

function Invoke-Gate {
    <#
    Runs one deterministic assessment. The scriptblock passes by returning
    $true; it fails by returning anything else or throwing (the message is the
    failure detail). Advisory gates are reported but do not fail the scenario.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$Description,
        [Parameter(Mandatory)][scriptblock]$Test,
        [switch]$Advisory
    )
    $outcome = 'fail'
    $detail = ''
    try {
        $value = & $Test
        if ($value -is [array]) { $value = $value[-1] }
        if ($value -eq $true) { $outcome = 'pass' } else { $detail = "returned '$value'" }
    }
    catch { $detail = $_.Exception.Message }
    if ($detail.Length -gt 4000) { $detail = $detail.Substring(0, 4000) + ' ...' }
    return Add-GateResult -Ctx $Ctx -Stage $Stage -Id $Id -Description $Description -Outcome $outcome -Detail $detail -Required (-not $Advisory)
}

function Skip-Gate {
    param($Ctx, [string]$Stage, [string]$Id, [string]$Description, [string]$Reason)
    return Add-GateResult -Ctx $Ctx -Stage $Stage -Id $Id -Description $Description -Outcome 'skip' -Detail $Reason -Required $false
}

function Get-FailedGates {
    param($Ctx, [string]$Stage)
    return @($Ctx.Gates | Where-Object { $_.stage -eq $Stage -and $_.required -and $_.outcome -eq 'fail' })
}

#endregion

#region VCP invocation and evidence

function Get-VcpGlobalArguments {
    param($Ctx, [string]$Config)
    $arguments = @('--format', 'jsonl', '--non-interactive', '--workspace', $Ctx.Workspace, '--data-dir', $Ctx.Data)
    if ($Config) { $arguments += @('--config', $Config) }
    return , $arguments
}

function Invoke-Vcp {
    <#
    Runs one vcp command in structured mode and records it in
    logs/vcp-commands.log. Returns exit code, frames, the first accepted frame,
    the final result frame and the scope.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string[]]$Arguments,
        [string]$Config,
        [int]$TimeoutSeconds = 300,
        [switch]$Live,
        [switch]$NoGlobals
    )
    $directory = Join-Path $Ctx.Logs (Join-Path $Stage 'vcp')
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $safe = ($Label -replace '[^A-Za-z0-9_.-]', '-')
    $index = '{0:D2}' -f ((Get-ChildItem -LiteralPath $directory -Filter '*.stdout.jsonl' -ErrorAction SilentlyContinue | Measure-Object).Count + 1)
    $stdout = Join-Path $directory "$index-$safe.stdout.jsonl"
    $stderr = Join-Path $directory "$index-$safe.stderr.txt"
    $all = if ($NoGlobals) { $Arguments } else { (Get-VcpGlobalArguments $Ctx $Config) + $Arguments }
    $counts = @{}
    $onLine = $null
    if ($Live) {
        # Invoked from Invoke-NativeLogged, so $Ctx and $counts resolve through
        # this function's scope (same module session state).
        $onLine = {
            param($line)
            $frame = $line | ConvertFrom-Json -Depth 100
            if ($frame.type -eq 'event') {
                $kind = [string]$frame.event.event.kind
                $counts[$kind] = 1 + [int]$counts[$kind]
                if ($kind -in 'task_transition', 'verification_recorded', 'approval_requested' -or
                    ($kind -eq 'effect_transition' -and $counts[$kind] % 25 -eq 1)) {
                    Write-Step $Ctx ("  event {0} #{1}" -f $kind, $counts[$kind])
                }
            }
            elseif ($frame.type -in 'accepted', 'required_input', 'result') {
                Write-Step $Ctx ("  {0}" -f $frame.type)
            }
        }
    }
    $result = Invoke-NativeLogged -FilePath $Ctx.Vcp -ArgumentList $all -WorkingDirectory $Ctx.Workspace -StdoutPath $stdout `
        -StderrPath $stderr -TimeoutSeconds $TimeoutSeconds -OnLine $onLine -HeartbeatLabel "$Stage/$Label" -Ctx $Ctx
    $parsed = ConvertFrom-JsonLines ([System.IO.File]::ReadAllText($stdout))
    $accepted = $parsed.Frames | Where-Object { $_.type -eq 'accepted' } | Select-Object -First 1
    $final = $parsed.Frames | Where-Object { $_.type -eq 'result' } | Select-Object -Last 1
    $scope = if ($accepted -and $accepted.scope) { $accepted.scope } elseif ($final -and $final.scope) { $final.scope } else { $null }
    $printable = ($all | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
    Add-Content -LiteralPath $Ctx.CommandLog -Encoding utf8NoBOM -Value ('{0:o} [{1}] exit={2} {3}s :: vcp {4}' -f (Get-Date), $Stage, $result.ExitCode, $result.DurationSeconds, $printable)
    return [pscustomobject]@{
        ExitCode        = $result.ExitCode
        TimedOut        = $result.TimedOut
        DurationSeconds = $result.DurationSeconds
        StdoutPath      = $stdout
        StderrPath      = $stderr
        Stderr          = [System.IO.File]::ReadAllText($stderr)
        Frames          = $parsed.Frames
        InvalidLines    = $parsed.Invalid.Count
        Accepted        = $accepted
        Result          = $final
        Scope           = $scope
        EventCounts     = $counts
    }
}

function Invoke-VcpInspect {
    <# Reads every page (bounded to 64) of one inspect view and saves them. #>
    param($Ctx, [string]$Stage, [string]$Id, [string]$View, [string]$Name)
    $pages = [System.Collections.Generic.List[object]]::new()
    $cursor = $null
    for ($page = 0; $page -lt 64; $page++) {
        $arguments = @('inspect', $Id, '--view', $View, '--limit', '128')
        if ($cursor) { $arguments += @('--cursor', ($cursor | ConvertTo-Json -Depth 50 -Compress)) }
        $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label "inspect-$View" -Arguments $arguments -TimeoutSeconds 120
        if ($run.ExitCode -ne 0 -or -not $run.Result -or -not $run.Result.data) { break }
        $pages.Add($run.Result.data)
        $cursor = $run.Result.data.next_cursor
        if (-not $cursor) { break }
    }
    if ($Name) { Write-JsonFile -Path (Join-Path $Ctx.Logs (Join-Path $Stage "inspect-$Name.json")) -Value $pages }
    return , $pages
}

function Get-InspectItems {
    param($Pages)
    return @($Pages | ForEach-Object { $_.items } | Where-Object { $_ })
}

function Get-VcpTaskCost {
    <# Settled cost from canonical ledger records (decimal micros strings). Null when evidence is incomplete. #>
    param($CostPages)
    $items = Get-InspectItems $CostPages
    $ledgers = @($items | Where-Object { $_.collection -eq 'ledger' })
    $attempts = @($items | Where-Object { $_.collection -eq 'attempt' })
    $gaps = @($CostPages | ForEach-Object { $_.gaps } | Where-Object { $_ })
    if ($ledgers.Count -lt 1) { return [pscustomobject]@{ Usd = $null; Attempts = $attempts.Count; Gaps = $gaps.Count } }
    $micros = [decimal]0
    foreach ($ledger in $ledgers) { $micros += [decimal]::Parse([string]$ledger.record.settled, [System.Globalization.CultureInfo]::InvariantCulture) }
    return [pscustomobject]@{ Usd = [math]::Round($micros / 1000000, 6); Attempts = $attempts.Count; Gaps = $gaps.Count }
}

function Get-VcpFinalMessage {
    <#
    The final assistant text is not part of the JSONL stream. Read the last
    captured provider response artifact through inspect --view outputs and
    extract output_text from its response.completed SSE event. Best effort.
    #>
    param($Ctx, [string]$Stage, $OutputPages)
    try {
        $responses = @(Get-InspectItems $OutputPages | Where-Object { $_.collection -eq 'artifact' -and $_.record.spec.channel -eq 'response' })
        if ($responses.Count -eq 0) { return $null }
        $item = $responses[-1]
        $length = [int64]$item.record.length
        if ($length -le 0 -or $length -gt 4MB) { return $null }
        $buffer = [System.IO.MemoryStream]::new()
        for ($offset = [int64]0; $offset -lt $length; $offset += 65536) {
            $read = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'inspect-response-range' -TimeoutSeconds 120 `
                -Arguments @('inspect', $item.id, '--view', 'outputs', '--offset', [string]$offset, '--length', '65536')
            $row = $read.Result.data.items | Select-Object -First 1
            if (-not $row -or -not $row.bytes) { return $null }
            $bytes = [byte[]]@($row.bytes)
            $buffer.Write($bytes, 0, $bytes.Length)
        }
        $sse = $script:Utf8NoBom.GetString($buffer.ToArray())
        $text = $null
        foreach ($block in ($sse -split "`r?`n`r?`n")) {
            $data = (($block -split "`r?`n") | Where-Object { $_.StartsWith('data:') } | ForEach-Object { $_.Substring(5).TrimStart() }) -join "`n"
            if (-not $data -or $data -eq '[DONE]') { continue }
            $sseEvent = $data | ConvertFrom-Json -Depth 100
            if ($sseEvent.type -ne 'response.completed') { continue }
            $parts = @($sseEvent.response.output | ForEach-Object { $_.content } | Where-Object { $_.type -eq 'output_text' } | ForEach-Object { $_.text })
            if ($parts.Count) { $text = $parts -join '' }
        }
        return $text
    }
    catch { return $null }
}

function Get-CompletedTurnIds {
    <# Completed turn IDs from canonical event facts (used by sessions fork). #>
    param($Frames)
    $ids = [System.Collections.Generic.List[string]]::new()
    foreach ($frame in $Frames) {
        if ($frame.type -ne 'event') { continue }
        foreach ($fact in @($frame.event.event.data.facts)) {
            if ($fact -and $fact.collection -eq 'turn' -and $fact.value.state -eq 'completed' -and $fact.id) { $ids.Add([string]$fact.id) }
        }
    }
    return , $ids
}

function Get-WorkspaceManifest {
    <# SHA-256 manifest of authored files (dependency caches and build outputs excluded). #>
    param([Parameter(Mandatory)][string]$Path)
    $root = (Resolve-Path -LiteralPath $Path).Path.TrimEnd('\')
    $manifest = [ordered]@{}
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($root)
    while ($pending.Count) {
        $directory = $pending.Pop()
        foreach ($entry in Get-ChildItem -LiteralPath $directory -Force -ErrorAction SilentlyContinue) {
            if ($entry.PSIsContainer) {
                if ($script:ManifestExclusions -contains $entry.Name -or $entry.Name -like '*.egg-info') { continue }
                if ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { continue }
                $pending.Push($entry.FullName)
            }
            else {
                $relative = $entry.FullName.Substring($root.Length + 1).Replace('\', '/')
                $manifest[$relative] = Get-Sha256 $entry.FullName
            }
        }
    }
    return $manifest
}

function Compare-WorkspaceManifest {
    param($Before, $After)
    $added = @($After.Keys | Where-Object { -not $Before.Contains($_) })
    $removed = @($Before.Keys | Where-Object { -not $After.Contains($_) })
    $modified = @($After.Keys | Where-Object { $Before.Contains($_) -and $Before[$_] -ne $After[$_] })
    return [pscustomobject]@{ Added = $added; Removed = $removed; Modified = $modified; Changed = ($added.Count + $removed.Count + $modified.Count) }
}

function Get-VcpConditions {
    param($Result)
    if (-not $Result -or -not $Result.conditions) { return @() }
    return @($Result.conditions.PSObject.Properties | Where-Object { $_.Value -eq $true } | ForEach-Object { $_.Name })
}

function Invoke-VcpTask {
    <#
    One paid VCP turn: vcp run --file <prompt> with the stage profile, then the
    read-only evidence sweep (tasks status/agents, inspect views, history) and
    a workspace diff. Respects the scenario spend ceiling.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Title,
        [Parameter(Mandatory)][string]$Prompt,
        [Parameter(Mandatory)][string]$Config,
        [ValidateSet('plan', 'ask', 'workspace', 'autonomous')][string]$Autonomy = 'autonomous',
        [decimal]$BudgetUsd = 0,
        [int[]]$AcceptExit = @(0),
        [string[]]$Skill = @()
    )
    if ($BudgetUsd -le 0) { $BudgetUsd = $Ctx.TurnBudgetUsd }
    $stageRecord = [ordered]@{
        stage = $Stage; title = $Title; kind = 'run'; autonomy = $Autonomy; budget_usd = $BudgetUsd
        exit_code = $null; conditions = @(); accepted_exit = $AcceptExit; task = $null; session = $null
        duration_seconds = $null; cost_usd = $null; attempts = $null; tool_items = $null; files_changed = $null
        event_counts = $null; final_message = $null; skipped = $null
    }
    if ($Ctx.SkipPaidStages) {
        $stageRecord.skipped = 'paid stages disabled (-SkipPaidStages)'
        $Ctx.Stages.Add([pscustomobject]$stageRecord)
        Write-Step $Ctx "Skipping paid stage $Stage ($Title)" 'warn'
        return $null
    }
    if (($Ctx.SpentUsd + $BudgetUsd) -gt $Ctx.MaxScenarioUsd) {
        $stageRecord.skipped = ('scenario ceiling {0} USD would be exceeded (spent {1})' -f (Format-Usd $Ctx.MaxScenarioUsd), (Format-Usd $Ctx.SpentUsd))
        $Ctx.Stages.Add([pscustomobject]$stageRecord)
        Write-Step $Ctx "Skipping $Stage; $($stageRecord.skipped)" 'warn'
        return $null
    }
    Write-Step $Ctx "$Stage :: $Title (autonomy $Autonomy, cap $(Format-Usd $BudgetUsd) USD)" 'phase'
    $stageDir = Join-Path $Ctx.Logs $Stage
    $promptPath = Join-Path $stageDir 'prompt.md'
    Write-Utf8File -Path $promptPath -Content $Prompt
    $before = Get-WorkspaceManifest $Ctx.Workspace
    $arguments = @('run', '--file', $promptPath, '--budget-usd', (Format-Usd $BudgetUsd), '--autonomy', $Autonomy)
    foreach ($id in $Skill) { $arguments += @('--skill', $id) }
    $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'run' -Config $Config -Arguments $arguments `
        -TimeoutSeconds ($Ctx.DeadlineSeconds + 300) -Live
    $evidence = Complete-VcpStageEvidence -Ctx $Ctx -Stage $Stage -Run $run -Before $before -Record $stageRecord
    return $evidence
}

function Complete-VcpStageEvidence {
    <# Shared evidence sweep for run, resume and fork stages. #>
    param($Ctx, [string]$Stage, $Run, $Before, $Record)
    $Record.exit_code = $Run.ExitCode
    $Record.duration_seconds = $Run.DurationSeconds
    $Record.conditions = Get-VcpConditions $Run.Result
    $Record.event_counts = $Run.EventCounts
    if ($Run.TimedOut) { $Ctx.Notes.Add("$Stage exceeded the harness timeout and was killed; inspect for unresolved effects.") }
    $task = if ($Run.Scope) { [string]$Run.Scope.task } else { $null }
    $Record.task = $task
    $Record.session = if ($Run.Scope) { [string]$Run.Scope.session } else { $null }
    $requiredInput = @($Run.Frames | Where-Object { $_.type -eq 'required_input' })
    if ($requiredInput.Count) {
        $Ctx.Notes.Add("$Stage stopped on $($requiredInput.Count) required input(s); the profile or autonomy did not cover a requested effect.")
    }
    if ($task) {
        $status = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'tasks-status' -Arguments @('tasks', 'status', $task)
        Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\tasks-status.json") -Value $status.Result
        $agents = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'tasks-agents' -Arguments @('tasks', 'agents', $task)
        Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\tasks-agents.json") -Value $agents.Result
        $costs = Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'costs' -Name 'costs'
        [void](Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'verification' -Name 'verification')
        $tools = Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'tools' -Name 'tools'
        [void](Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'routing' -Name 'routing')
        [void](Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'policy' -Name 'policy')
        $outputs = Invoke-VcpInspect -Ctx $Ctx -Stage $Stage -Id $task -View 'outputs' -Name 'outputs'
        $history = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'history-list' -Arguments @('history', 'list', '--task', $task, '--limit', '128')
        Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\history.json") -Value $history.Result
        $cost = Get-VcpTaskCost $costs
        $Record.cost_usd = $cost.Usd
        $Record.attempts = $cost.Attempts
        $Record.tool_items = (Get-InspectItems $tools).Count
        if ($null -eq $cost.Usd) {
            $Ctx.CostUnknown = $true
            $Ctx.Notes.Add("$Stage cost evidence incomplete; budget guard now assumes the full per-turn cap was spent.")
            $Ctx.SpentUsd += [decimal]$Record.budget_usd
        }
        else { $Ctx.SpentUsd += [decimal]$cost.Usd }
        $message = Get-VcpFinalMessage -Ctx $Ctx -Stage $Stage -OutputPages $outputs
        if ($message) {
            Write-Utf8File -Path (Join-Path $Ctx.Logs "$Stage\final-message.md") -Content $message
            $Record.final_message = "logs/$Stage/final-message.md"
        }
    }
    elseif ($Record.budget_usd) {
        # No accepted task: nothing was dispatched, so no spend is attributed.
        $Ctx.Notes.Add("$Stage produced no accepted task (exit $($Run.ExitCode)); see logs/$Stage/vcp.")
    }
    if ($Before) {
        $after = Get-WorkspaceManifest $Ctx.Workspace
        $diff = Compare-WorkspaceManifest $Before $after
        $Record.files_changed = $diff.Changed
        Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\workspace-diff.json") -Value $diff
        $Record.workspace_diff = $diff
    }
    $level = if ($Record.accepted_exit -contains $Run.ExitCode) { 'ok' } else { 'warn' }
    Write-Step $Ctx ("{0} finished: exit {1} [{2}] task {3} cost {4} USD, {5} files changed, {6}s" -f $Stage, $Run.ExitCode,
        ($Record.conditions -join ','), $task, $Record.cost_usd, $Record.files_changed, $Run.DurationSeconds) $level
    $object = [pscustomobject]$Record
    $Ctx.Stages.Add($object)
    $object | Add-Member -NotePropertyName Run -NotePropertyValue $Run -Force
    return $object
}

function Invoke-VcpContinuation {
    <#
    Continuation stages (resume <task>, resume --last, sessions resume,
    sessions fork). Same evidence sweep as a run stage.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Title,
        [Parameter(Mandatory)][string[]]$Arguments,
        [string]$Config,
        [int[]]$AcceptExit = @(0)
    )
    $record = [ordered]@{
        stage = $Stage; title = $Title; kind = $Arguments[0..([math]::Min(1, $Arguments.Count - 1))] -join ' '; autonomy = $null
        budget_usd = $null; exit_code = $null; conditions = @(); accepted_exit = $AcceptExit; task = $null; session = $null
        duration_seconds = $null; cost_usd = $null; attempts = $null; tool_items = $null; files_changed = $null
        event_counts = $null; final_message = $null; skipped = $null
    }
    if ($Ctx.SkipPaidStages) {
        $record.skipped = 'paid stages disabled (-SkipPaidStages)'
        $Ctx.Stages.Add([pscustomobject]$record)
        return $null
    }
    # Resume keeps the original task cap; a fork opens a new root with the persisted
    # cap. Either way, assume up to one more per-turn cap of spend.
    if (($Ctx.SpentUsd + $Ctx.TurnBudgetUsd) -gt $Ctx.MaxScenarioUsd) {
        $record.skipped = ('scenario ceiling {0} USD would be exceeded (spent {1})' -f (Format-Usd $Ctx.MaxScenarioUsd), (Format-Usd $Ctx.SpentUsd))
        $Ctx.Stages.Add([pscustomobject]$record)
        Write-Step $Ctx "Skipping $Stage; $($record.skipped)" 'warn'
        return $null
    }
    $record.budget_usd = $Ctx.TurnBudgetUsd
    Write-Step $Ctx "$Stage :: $Title" 'phase'
    $before = Get-WorkspaceManifest $Ctx.Workspace
    $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label ($Arguments[0..1] -join '-') -Config $Config -Arguments $Arguments `
        -TimeoutSeconds ($Ctx.DeadlineSeconds + 300) -Live
    return Complete-VcpStageEvidence -Ctx $Ctx -Stage $Stage -Run $run -Before $before -Record $record
}

function Invoke-RepairLoop {
    <#
    When required gates for a stage fail, feed the deterministic failure
    evidence back as a new task (bounded) and rerun the stage gates.
    GateScript receives the stage name to record results under.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Config,
        [Parameter(Mandatory)][scriptblock]$GateScript,
        [string]$Constraints = ''
    )
    $current = $Stage
    for ($attempt = 1; $attempt -le $Ctx.MaxRepairTurns; $attempt++) {
        $failed = Get-FailedGates $Ctx $current
        if ($failed.Count -eq 0) { return $current }
        if ($Ctx.SkipPaidStages) { return $current }
        $evidence = ($failed | ForEach-Object { "- [$($_.id)] $($_.description)`n  Failure: $($_.detail)" }) -join "`n"
        if ($evidence.Length -gt 12000) { $evidence = $evidence.Substring(0, 12000) + "`n... (truncated)" }
        $repairStage = "$Stage-repair$attempt"
        $prompt = @"
# Repair request ($repairStage)

An independent verification harness checked the work from stage $current and
these required checks failed. The checks are authoritative acceptance tests.

$evidence

Fix the underlying causes in the project. Do not weaken, skip, or delete tests,
and do not edit files that the task describes as protected. Keep all previously
working behavior intact. Run the relevant build and test commands available to
you before finishing, then reply with a short summary of the root causes and fixes.
$Constraints
"@
        $result = Invoke-VcpTask -Ctx $Ctx -Stage $repairStage -Title "Repair failures from $current" -Prompt $prompt -Config $Config -AcceptExit @(0, 3)
        if (-not $result) { return $current }
        & $GateScript $repairStage
        $current = $repairStage
    }
    return $current
}

function Test-StageExit {
    <# Gate: the VCP stage ended with an accepted exit code. #>
    param($Ctx, $StageResult, [string]$Stage)
    if (-not $StageResult) { return }
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "vcp-exit" -Description "vcp exit code in {$($StageResult.accepted_exit -join ',')}" -Test {
            Assert-That ($StageResult.accepted_exit -contains $StageResult.exit_code) ("exit {0} conditions [{1}]; stderr: {2}" -f $StageResult.exit_code, ($StageResult.conditions -join ','), (Get-TextTail $StageResult.Run.StderrPath 15))
            $true
        })
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "jsonl" -Description 'JSONL stream has accepted and result frames and no invalid lines' -Advisory -Test {
            Assert-That ($null -ne $StageResult.Run.Accepted) 'no accepted frame'
            Assert-That ($null -ne $StageResult.Run.Result) 'no final result frame'
            Assert-That ($StageResult.Run.InvalidLines -eq 0) "$($StageResult.Run.InvalidLines) invalid JSONL lines"
            $true
        })
}

function Invoke-PlanModeReview {
    <#
    Read-only review turn in plan autonomy: the workspace manifest must be
    byte-identical afterwards. Exit 4 (required input) is acceptable when the
    model attempted an effect, and is recorded.
    #>
    param($Ctx, [string]$Stage, [string]$Config, [string]$Prompt)
    $before = Get-WorkspaceManifest $Ctx.Workspace
    $review = Invoke-VcpTask -Ctx $Ctx -Stage $Stage -Title 'Plan-mode review (read-only)' -Prompt $Prompt -Config $Config `
        -Autonomy 'plan' -BudgetUsd ([math]::Min($Ctx.TurnBudgetUsd, [decimal]2)) -AcceptExit @(0, 3, 4)
    if (-not $review) { return $null }
    Test-StageExit $Ctx $review $Stage
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "immutable" -Description 'plan autonomy left the workspace byte-identical' -Test {
            $diff = Compare-WorkspaceManifest $before (Get-WorkspaceManifest $Ctx.Workspace)
            Assert-That ($diff.Changed -eq 0) ("changed: " + (($diff.Added + $diff.Modified + $diff.Removed) -join ', '))
            $true
        })
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "findings-json" -Description 'review ends with a parseable JSON findings block' -Advisory -Test {
            $path = Join-Path $Ctx.Logs "$Stage\final-message.md"
            Assert-That (Test-Path -LiteralPath $path) 'final message unavailable from outputs view'
            $text = [System.IO.File]::ReadAllText($path)
            $match = [regex]::Match($text, '```json\s*(?<j>[\s\S]*?)```')
            Assert-That $match.Success 'no ```json block'
            $findings = $match.Groups['j'].Value | ConvertFrom-Json -Depth 20
            Assert-That ($null -ne $findings.findings) 'missing findings array'
            Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\findings.json") -Value $findings
            $true
        })
    return $review
}

#endregion

#region Profiles and preflight

function New-ProcessProfile {
    <# A VCP process entry: absolute .exe, filtered public environment, explicit isolation choice. #>
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Executable, $Ctx,
        [string[]]$ExtraPath = @(), [int]$MaxTimeoutMs = 900000)
    $pathEntries = @((Split-Path -Parent $Executable)) + $ExtraPath + @("$env:SystemRoot\System32", $env:SystemRoot)
    return [ordered]@{
        name               = $Name
        executable         = $Executable
        environment        = [ordered]@{
            SYSTEMROOT = $env:SystemRoot
            WINDIR     = $env:SystemRoot
            PATH       = (($pathEntries | Where-Object { $_ } | Select-Object -Unique) -join ';')
            PATHEXT    = '.COM;.EXE;.BAT;.CMD'
            TEMP       = $Ctx.Temp
            TMP        = $Ctx.Temp
            CI         = 'true'
        }
        required_isolation = @()
        reduced_isolation  = $true
        inputs             = @()
        max_timeout_ms     = $MaxTimeoutMs
    }
}

function New-ScenarioProfile {
    <#
    Composes a scenario execution profile. The qualified provider snapshot is
    spliced in verbatim from qualified\snapshot.json so its values are never
    re-encoded; everything else is owner configuration for this scenario.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$AffectedPaths,
        [object[]]$Processes = @(),
        [object[]]$Checks = @(),
        [string]$MaximumAutonomy = 'autonomous',
        [string[]]$AutomaticEffects = @('read', 'write', 'execute', 'network', 'install', 'opaque'),
        [int]$DeadlineSeconds = 0,
        [int]$MaxRequests = 0,
        [bool]$TrustWorkspace = $true
    )
    if ($DeadlineSeconds -le 0) { $DeadlineSeconds = $Ctx.DeadlineSeconds }
    if ($MaxRequests -le 0) { $MaxRequests = $Ctx.MaxRequests }
    $placeholder = '__VCP_PROVIDER_SNAPSHOT__'
    $profileDocument = [ordered]@{
        version                  = 1
        workspace                = $Ctx.Workspace
        trust_workspace          = $TrustWorkspace
        sync_roots               = @()
        maximum_autonomy         = $MaximumAutonomy
        automatic_effects        = $AutomaticEffects
        budget_usd               = (Format-Usd $Ctx.TurnBudgetUsd)
        provider                 = $placeholder
        catalog                  = $Ctx.Catalog
        affected_paths           = $AffectedPaths
        canonical_tools          = @('vcp_read', 'vcp_list', 'vcp_search', 'vcp_patch', 'vcp_exec', 'vcp_verify')
        max_requests             = $MaxRequests
        output_tokens            = [string]$Ctx.OutputTokens
        provider_timeout_seconds = [math]::Min(300, $DeadlineSeconds)
        max_transport_retries    = 2
        deadline_seconds         = $DeadlineSeconds
        processes                = @($Processes)
        checks                   = @($Checks)
    }
    $json = $profileDocument | ConvertTo-Json -Depth 32
    $json = $json.Replace('"' + $placeholder + '"', $Ctx.SnapshotText.Trim())
    $path = Join-Path $Ctx.Profiles ("{0}-{1}.json" -f $Name, $Ctx.RunId)
    Write-Utf8File -Path $path -Content $json
    return $path
}

function Invoke-CommonPreflight {
    <#
    Zero-spend CLI preflight: version, doctor, setup credential status, the
    real 'setup profile' path for this workspace, skills inventory, models.
    #>
    param($Ctx, [string]$AffectedPath = 'README.md')
    $stage = 'P0-preflight'
    Write-Step $Ctx 'P0 preflight (no inference)' 'phase'
    $version = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'version' -Arguments @('--version') -NoGlobals
    $Ctx.VcpVersion = (Get-Content -LiteralPath $version.StdoutPath -Raw).Trim()
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'version' -Description 'vcp --version exits 0' -Test {
            Assert-That ($version.ExitCode -eq 0) "exit $($version.ExitCode)"; $true })
    $doctor = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'doctor' -Arguments @('doctor')
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'doctor' -Description 'vcp doctor reports healthy local paths' -Advisory -Test {
            Assert-That ($doctor.ExitCode -eq 0) "exit $($doctor.ExitCode): $($doctor.Stderr)"; $true })
    $credential = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'credential-status' -Arguments @('setup', 'credential', 'status')
    Write-JsonFile -Path (Join-Path $Ctx.Logs "$stage\credential-status.json") -Value $credential.Result
    $base = Join-Path $Ctx.Profiles ("base-setup-profile-{0}.json" -f $Ctx.RunId)
    $setup = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'setup-profile' -Arguments @(
        'setup', 'profile', '--snapshot', $Ctx.Snapshot, '--catalog', $Ctx.Catalog, '--output', $base,
        '--trust-workspace', '--budget-usd', (Format-Usd $Ctx.TurnBudgetUsd), '--autonomy', 'ask', '--affected-path', $AffectedPath)
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'setup-profile' -Description 'vcp setup profile creates a trusted base profile for this workspace' -Test {
            Assert-That ($setup.ExitCode -eq 0) "exit $($setup.ExitCode): $($setup.Stderr)"
            Assert-That (Test-Path -LiteralPath $base) 'profile file not written'
            $true
        })
    $skills = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'skills-list' -Arguments @('skills', 'list')
    Write-JsonFile -Path (Join-Path $Ctx.Logs "$stage\skills.json") -Value $skills.Result
    $models = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'models' -Arguments @('models')
    Write-JsonFile -Path (Join-Path $Ctx.Logs "$stage\models.json") -Value $models.Result
}

function Test-ProfileCheck {
    <# vcp setup check for a composed profile (no inference). #>
    param($Ctx, [string]$Stage, [string]$Config, [string]$Id)
    $check = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label "setup-check-$Id" -Config $Config -Arguments @('setup', 'check')
    return Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "setup-check.$Id" -Description "vcp setup check accepts profile '$Id'" -Test {
        Assert-That ($check.ExitCode -eq 0) ("exit {0}: {1}" -f $check.ExitCode, $check.Stderr.Trim()); $true }
}

function Invoke-GuardrailRun {
    <#
    Zero-spend negative test: a run the CLI must reject before inference with
    exit 2 (invalid configuration). Asserts no task was accepted.
    #>
    param($Ctx, [string]$Stage, [string]$Id, [string]$Description, [string[]]$Arguments, [string]$Config, [string]$ExpectStderr)
    $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label "guardrail-$Id" -Config $Config -Arguments $Arguments -TimeoutSeconds 120
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "guardrail.$Id" -Description $Description -Test {
            Assert-That ($run.ExitCode -eq 2) "expected exit 2, got $($run.ExitCode)"
            Assert-That ($null -eq $run.Accepted) 'a task was accepted; guardrail did not stop before execution'
            if ($ExpectStderr) {
                $combined = $run.Stderr + (Get-Content -LiteralPath $run.StdoutPath -Raw)
                Assert-That ($combined -match $ExpectStderr) "diagnostic did not match /$ExpectStderr/: $($run.Stderr.Trim())"
            }
            $true
        })
}

function Invoke-WorkspaceDiscover {
    param($Ctx, [string]$Stage)
    $discover = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'workspace-discover' -Arguments @('workspace', 'discover')
    Write-JsonFile -Path (Join-Path $Ctx.Logs "$Stage\workspace-discover.json") -Value $discover.Result
    return $discover
}

function Invoke-FinalEvidenceSweep {
    <# Read-only, zero-spend commands across the whole scenario history. #>
    param($Ctx, [string]$SearchText = 'test')
    $stage = 'P9-evidence'
    Write-Step $Ctx 'P9 evidence sweep (read-only)' 'phase'
    foreach ($entry in @(
            @{ Label = 'sessions-list'; Arguments = @('sessions', 'list') },
            @{ Label = 'workspace-discover'; Arguments = @('workspace', 'discover') },
            @{ Label = 'history-list'; Arguments = @('history', 'list', '--limit', '128') },
            @{ Label = 'history-search'; Arguments = @('history', 'search', $SearchText, '--limit', '32') },
            @{ Label = 'retention-show'; Arguments = @('retention', 'show') },
            @{ Label = 'optimize-status'; Arguments = @('optimize', 'status') },
            @{ Label = 'optimize-report'; Arguments = @('optimize', 'report') },
            @{ Label = 'memory-search'; Arguments = @('memory', 'search', $SearchText, '--limit', '8') })) {
        $run = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label $entry.Label -Arguments $entry.Arguments
        Write-JsonFile -Path (Join-Path $Ctx.Logs "$stage\$($entry.Label).json") -Value ([ordered]@{ exit_code = $run.ExitCode; result = $run.Result })
    }
    $sessions = Get-Content -LiteralPath (Join-Path $Ctx.Logs "$stage\sessions-list.json") -Raw | ConvertFrom-Json -Depth 50
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'sessions-list' -Description 'sessions list succeeds after the scenario' -Advisory -Test {
            Assert-That ($sessions.exit_code -eq 0) "exit $($sessions.exit_code)"; $true })
}

function Initialize-GitCheckpoint {
    <# Optional: a local git history in the workspace gives reviewers one commit per stage. #>
    param($Ctx)
    $Ctx.Git = Find-Executable -Name 'git'
    if (-not $Ctx.Git) { $Ctx.Notes.Add('git not found; per-stage checkpoint commits disabled.'); return }
    [void](Invoke-Tool -Ctx $Ctx -Stage 'git' -Label 'init' -FilePath $Ctx.Git -ArgumentList @('init', '-q', '-b', 'main'))
    Save-Checkpoint $Ctx 'seed: scenario scaffold'
}

function Save-Checkpoint {
    param($Ctx, [string]$Message)
    if (-not $Ctx.Git) { return }
    [void](Invoke-Tool -Ctx $Ctx -Stage 'git' -Label 'add' -FilePath $Ctx.Git -ArgumentList @('add', '-A'))
    [void](Invoke-Tool -Ctx $Ctx -Stage 'git' -Label 'commit' -FilePath $Ctx.Git -ArgumentList @(
            '-c', 'user.name=vcp-scenario-harness', '-c', 'user.email=harness@localhost', '-c', 'commit.gpgsign=false',
            'commit', '-q', '--allow-empty', '--no-verify', '-m', $Message))
}

function Add-Asset {
    param($Ctx, [string]$Path, [string]$Description)
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $item = Get-Item -LiteralPath $Path
    $relative = if ($item.FullName.StartsWith($Ctx.Root)) { $item.FullName.Substring($Ctx.Root.Length + 1) } else { $item.FullName }
    $Ctx.Assets.Add([pscustomobject]@{
            path = $relative; description = $Description; bytes = if ($item.PSIsContainer) { $null } else { $item.Length }
            sha256 = if ($item.PSIsContainer) { $null } else { Get-Sha256 $item.FullName }
        })
}

function Complete-VcpScenario {
    <#
    Writes results/scorecard.json and results/summary.md, stops the transcript
    and returns the process exit code: 0 when every required gate passed and
    no fatal error occurred, 1 otherwise.
    #>
    param($Ctx)
    # Latest state: a repair stage's result for the same gate supersedes the
    # original stage's result. First-pass statistics keep the original misses.
    $required = @($Ctx.Gates | Where-Object { $_.required } |
            Group-Object { '{0}|{1}' -f ($_.stage -replace '-repair\d+$', ''), $_.id } |
            ForEach-Object { $_.Group[-1] })
    $passed = @($required | Where-Object { $_.outcome -eq 'pass' })
    $failed = @($required | Where-Object { $_.outcome -eq 'fail' })
    $advisoryFailed = @($Ctx.Gates | Where-Object { -not $_.required -and $_.outcome -eq 'fail' })
    $paid = @($Ctx.Stages | Where-Object { -not $_.skipped })
    $repairs = @($paid | Where-Object { $_.stage -like '*-repair*' })
    $primary = @($paid | Where-Object { $_.stage -notlike '*-repair*' -and $_.kind -eq 'run' -and $_.autonomy -ne 'plan' })
    $firstPass = @($primary | Where-Object {
            $name = $_.stage
            @($Ctx.Gates | Where-Object { $_.stage -eq $name -and $_.required -and $_.outcome -eq 'fail' }).Count -eq 0
        })
    $scorecard = [ordered]@{
        schema                   = 'vcp-practical-scenario/1'
        scenario                 = $Ctx.Name
        run_id                   = $Ctx.RunId
        started                  = $Ctx.Started.ToString('o')
        finished                 = (Get-Date).ToString('o')
        wall_minutes             = [math]::Round(((Get-Date) - $Ctx.Started).TotalMinutes, 1)
        vcp                      = $Ctx.Vcp
        vcp_version              = $Ctx.VcpVersion
        model                    = $Ctx.Model
        endpoint                 = $Ctx.Endpoint
        snapshot_valid_until     = $Ctx.SnapshotValidUntil
        spend_usd                = [math]::Round($Ctx.SpentUsd, 6)
        spend_evidence_complete  = -not $Ctx.CostUnknown
        max_scenario_usd         = $Ctx.MaxScenarioUsd
        required_gates           = $required.Count
        required_passed          = $passed.Count
        required_failed          = $failed.Count
        advisory_failed          = $advisoryFailed.Count
        gate_pass_rate           = if ($required.Count) { [math]::Round($passed.Count / $required.Count, 3) } else { $null }
        feature_turns            = $primary.Count
        first_pass_turns         = $firstPass.Count
        first_pass_rate          = if ($primary.Count) { [math]::Round($firstPass.Count / $primary.Count, 3) } else { $null }
        repair_turns             = $repairs.Count
        exit_code_distribution   = ($paid | Group-Object exit_code | ForEach-Object { [ordered]@{ exit = $_.Name; count = $_.Count } })
        fatal                    = $Ctx.Fatal
        verdict                  = if (-not $Ctx.Fatal -and $failed.Count -eq 0 -and $required.Count -gt 0) { 'pass' } else { 'fail' }
        stages                   = @($Ctx.Stages | ForEach-Object { $_ | Select-Object * -ExcludeProperty Run, workspace_diff })
        gates                    = $Ctx.Gates
        assets                   = $Ctx.Assets
        notes                    = $Ctx.Notes
    }
    Write-JsonFile -Path (Join-Path $Ctx.Results 'scorecard.json') -Value $scorecard

    $md = [System.Text.StringBuilder]::new()
    [void]$md.AppendLine("# VCP practical scenario: $($Ctx.Name)")
    [void]$md.AppendLine()
    [void]$md.AppendLine("Run ``$($Ctx.RunId)`` - verdict **$($scorecard.verdict.ToUpperInvariant())** - $($passed.Count)/$($required.Count) required gates - spend $(Format-Usd $Ctx.SpentUsd) USD$(if ($Ctx.CostUnknown) { ' (incomplete cost evidence)' }) - $($scorecard.wall_minutes) min")
    [void]$md.AppendLine()
    [void]$md.AppendLine("VCP ``$($Ctx.VcpVersion)``, model ``$($Ctx.Model)`` endpoint ``$($Ctx.Endpoint)``. First-pass feature turns: $($firstPass.Count)/$($primary.Count); repair turns: $($repairs.Count).")
    if ($Ctx.Fatal) { [void]$md.AppendLine(); [void]$md.AppendLine("**Fatal:** $($Ctx.Fatal)") }
    [void]$md.AppendLine()
    [void]$md.AppendLine('## Stages')
    [void]$md.AppendLine()
    [void]$md.AppendLine('| Stage | Title | Exit | Conditions | Cost USD | Files | Seconds | Required gates |')
    [void]$md.AppendLine('|---|---|---|---|---|---|---|---|')
    foreach ($stage in $Ctx.Stages) {
        $name = $stage.stage
        $stageGates = @($Ctx.Gates | Where-Object { $_.stage -eq $name -and $_.required })
        $ok = @($stageGates | Where-Object { $_.outcome -eq 'pass' }).Count
        $exit = if ($stage.skipped) { 'skipped' } else { $stage.exit_code }
        [void]$md.AppendLine("| $name | $($stage.title) | $exit | $($stage.conditions -join ', ') | $($stage.cost_usd) | $($stage.files_changed) | $($stage.duration_seconds) | $ok/$($stageGates.Count) |")
    }
    [void]$md.AppendLine()
    [void]$md.AppendLine('## Failed or skipped gates (all attempts, including later-repaired ones)')
    [void]$md.AppendLine()
    $notable = @($Ctx.Gates | Where-Object { $_.outcome -ne 'pass' })
    if ($notable.Count -eq 0) { [void]$md.AppendLine('None.') }
    foreach ($gate in $notable) {
        $kind = if ($gate.required) { 'required' } else { 'advisory' }
        [void]$md.AppendLine("- **$($gate.id)** ($kind, $($gate.outcome)): $($gate.description) - $($gate.detail -replace "`r?`n", ' ')")
    }
    [void]$md.AppendLine()
    [void]$md.AppendLine('## Produced assets')
    [void]$md.AppendLine()
    if ($Ctx.Assets.Count -eq 0) { [void]$md.AppendLine('None recorded.') }
    foreach ($asset in $Ctx.Assets) { [void]$md.AppendLine("- ``$($asset.path)`` - $($asset.description)$(if ($asset.bytes) { " ($($asset.bytes) bytes)" })") }
    if ($Ctx.Notes.Count) {
        [void]$md.AppendLine()
        [void]$md.AppendLine('## Notes')
        [void]$md.AppendLine()
        foreach ($note in $Ctx.Notes) { [void]$md.AppendLine("- $note") }
    }
    Write-Utf8File -Path (Join-Path $Ctx.Results 'summary.md') -Content $md.ToString()
    Write-Step $Ctx ("Scenario {0}: {1} ({2}/{3} required gates, {4} USD). Results: {5}" -f $Ctx.Name, $scorecard.verdict.ToUpperInvariant(), $passed.Count, $required.Count, (Format-Usd $Ctx.SpentUsd), $Ctx.Results) $(if ($scorecard.verdict -eq 'pass') { 'ok' } else { 'fail' })
    if ($Ctx.Transcript) { try { Stop-Transcript | Out-Null } catch { } }
    return $(if ($scorecard.verdict -eq 'pass') { 0 } else { 1 })
}

#endregion

Export-ModuleMember -Function @(
    'Write-Utf8File', 'Write-JsonFile', 'Write-SeedFiles', 'Get-TextTail', 'Get-Sha256', 'Assert-That', 'ConvertTo-Instant', 'Format-Usd',
    'ConvertFrom-JsonLines', 'Write-Step', 'Find-Executable', 'Invoke-NativeLogged', 'Invoke-Tool',
    'Start-BackgroundServer', 'Stop-BackgroundServer', 'Invoke-Http', 'Initialize-VcpScenario', 'Add-GateResult',
    'Invoke-Gate', 'Skip-Gate', 'Get-FailedGates', 'Invoke-Vcp', 'Invoke-VcpInspect', 'Get-InspectItems',
    'Get-VcpTaskCost', 'Get-VcpFinalMessage', 'Get-CompletedTurnIds', 'Get-WorkspaceManifest',
    'Compare-WorkspaceManifest', 'Invoke-VcpTask', 'Invoke-VcpContinuation', 'Invoke-RepairLoop', 'Test-StageExit',
    'Invoke-PlanModeReview', 'New-ProcessProfile', 'New-ScenarioProfile', 'Invoke-CommonPreflight',
    'Test-ProfileCheck', 'Invoke-GuardrailRun', 'Invoke-WorkspaceDiscover', 'Invoke-FinalEvidenceSweep',
    'Initialize-GitCheckpoint', 'Save-Checkpoint', 'Add-Asset', 'Complete-VcpScenario'
)
