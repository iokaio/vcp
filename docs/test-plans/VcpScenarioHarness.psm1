# SPDX-License-Identifier: Apache-2.0
#
# Shared harness for the long-running VCP CLI practical scenarios described in
# cli-test-plans.md. Each scenario script imports this module, owns its own run
# root, data directory, profiles, ports and logs, and can run concurrently with
# the other scenarios in a separate console window.
#
# The harness never prints or persists credential values. VCP reads the
# provider credential from the configured environment variable in the launching
# console (OPENROUTER_API_KEY by default).

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
    param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][System.Collections.IDictionary]$Files, [switch]$MissingOnly)
    foreach ($relative in $Files.Keys) {
        $path = Join-Path $Root $relative
        if ($MissingOnly) {
            $base = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
            $path = [IO.Path]::GetFullPath($path)
            if (-not $path.StartsWith($base + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Seed file must stay inside the selected project.' }
            if (Test-Path -LiteralPath $path) { continue }
            $parent = Split-Path -Parent $path
            $probe = $parent
            while ($probe) {
                if ((Test-Path -LiteralPath $probe) -and ((Get-Item -LiteralPath $probe -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                    throw "Refusing to seed through a link or junction: $probe"
                }
                if ($probe -eq $base) { break }
                $probe = Split-Path -Parent $probe
            }
            [IO.Directory]::CreateDirectory($parent) | Out-Null
            $stream = $null
            try {
                $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
                $bytes = $script:Utf8NoBom.GetBytes([string]$Files[$relative])
                $stream.Write($bytes, 0, $bytes.Length)
            }
            catch [IO.IOException] {
                if ($stream -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { throw }
                # A file created concurrently belongs to its creator, not the seed.
            }
            finally { if ($stream) { $stream.Dispose() } }
            continue
        }
        Write-Utf8File -Path $path -Content $Files[$relative]
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

function Assert-ScenarioBudgetPrecision {
    param([decimal]$Value, [string]$Name)
    if ($Value * 100 -ne [decimal]::Truncate($Value * 100)) {
        throw "$Name must use at most two decimal places so admission accounting matches the CLI budget exactly."
    }
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
        [ValidateRange(1, 86400)][int]$TimeoutSeconds = 600,
        [hashtable]$Environment = @{},
        [switch]$ClearEnvironment,
        [scriptblock]$OnLine,
        [string]$HeartbeatLabel,
        $Ctx
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new($FilePath)
    foreach ($argument in $ArgumentList) { $psi.ArgumentList.Add([string]$argument) }
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = $script:Utf8NoBom
    $psi.StandardErrorEncoding = $script:Utf8NoBom
    if ($ClearEnvironment) { $psi.Environment.Clear() }
    foreach ($key in $Environment.Keys) {
        if ($null -eq $Environment[$key]) { [void]$psi.Environment.Remove($key) }
        else { $psi.Environment[$key] = [string]$Environment[$key] }
    }

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $StdoutPath) | Out-Null
    $started = Get-Date
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
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
            # Check every iteration, including continuously available output and
            # a child that closed stdout but is still running.
            if ($clock.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                $timedOut = $true
                try { $process.Kill($true) } catch { }
                break
            }
            $now = Get-Date
            if ($Ctx -and $HeartbeatLabel -and $now -ge $nextHeartbeat) {
                Write-Step $Ctx ('{0} still running ({1:hh\:mm\:ss}, {2} stdout lines)' -f $HeartbeatLabel, ($now - $started), $lines)
                $nextHeartbeat = $now.AddSeconds(60)
            }
            if ($null -eq $lineTask) {
                if ($process.WaitForExit(100)) { break }
                continue
            }
            if ($lineTask.Wait(100)) {
                $line = $lineTask.Result
                if ($null -eq $line) { $lineTask = $null; continue }
                $writer.WriteLine($line)
                $writer.Flush()
                $lines++
                if ($OnLine) { try { & $OnLine $line } catch { } }
                $lineTask = $process.StandardOutput.ReadLineAsync()
                continue
            }
        }
        if (-not $timedOut) { $process.WaitForExit() }
        else { [void]$process.WaitForExit(15000) }
    }
    finally {
        $writer.Dispose()
        if (-not $process.HasExited) {
            try { $process.Kill($true); [void]$process.WaitForExit(15000) } catch { }
        }
    }
    $stderr = ''
    if ($stderrTask.Wait(15000)) { $stderr = $stderrTask.Result }
    Write-Utf8File -Path $StderrPath -Content $stderr
    $exitCode = if ($timedOut) { -1 } else { $process.ExitCode }
    $process.Dispose()
    return [pscustomobject]@{
        ExitCode        = $exitCode
        TimedOut        = $timedOut
        DurationSeconds = [math]::Round($clock.Elapsed.TotalSeconds, 1)
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
        [hashtable]$Environment = @{},
        [switch]$ClearEnvironment
    )
    if (-not $WorkingDirectory) { $WorkingDirectory = $Ctx.Workspace }
    $Environment = $Environment.Clone()
    $Environment['OPENROUTER_API_KEY'] = $null
    if ($env:VCP_SCENARIO_CREDENTIAL_ENV) { $Environment[$env:VCP_SCENARIO_CREDENTIAL_ENV] = $null }
    $safe = ($Label -replace '[^A-Za-z0-9_.-]', '-')
    $directory = Join-Path $Ctx.Logs (Join-Path $Stage 'tools')
    $index = '{0:D2}' -f ((Get-ChildItem -LiteralPath $directory -Filter '*.out.log' -ErrorAction SilentlyContinue | Measure-Object).Count + 1)
    $stdout = Join-Path $directory "$index-$safe.out.log"
    $stderr = Join-Path $directory "$index-$safe.err.log"
    $result = Invoke-NativeLogged -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory `
        -StdoutPath $stdout -StderrPath $stderr -TimeoutSeconds $TimeoutSeconds -Environment $Environment -ClearEnvironment:$ClearEnvironment `
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
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    foreach ($key in $Environment.Keys) { $psi.Environment[$key] = [string]$Environment[$key] }
    [void]$psi.Environment.Remove('OPENROUTER_API_KEY')
    if ($env:VCP_SCENARIO_CREDENTIAL_ENV) { [void]$psi.Environment.Remove($env:VCP_SCENARIO_CREDENTIAL_ENV) }
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
    $Server.Process.Dispose()
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
    # PowerShell returns byte[] for application/problem+json. Casting those
    # bytes to string produces decimal numbers instead of the JSON error body.
    $content = if ($response.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($response.Content) } else { [string]$response.Content }
    if ($content -and ($content.TrimStart().StartsWith('{') -or $content.TrimStart().StartsWith('['))) {
        try { $json = $content | ConvertFrom-Json -Depth 50 } catch { }
    }
    return [pscustomobject]@{ Status = [int]$response.StatusCode; Headers = $response.Headers; Content = $content; Json = $json }
}

#endregion

#region Scenario lifecycle

function Assert-ScenarioProjectPath {
    <# Scenario projects must be independent of the checkout containing this harness. #>
    param([Parameter(Mandatory)][string]$Path)
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $probe = $full
    while ($probe) {
        if ((Test-Path -LiteralPath $probe) -and ((Get-Item -LiteralPath $probe -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Scenario project path contains a link or junction ('$probe'). Select the independent project's physical directory."
        }
        $parent = Split-Path -Parent $probe
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
    $sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..')).TrimEnd('\', '/')
    if (Test-Path -LiteralPath (Join-Path $sourceRoot 'src/crates/vcp-cli/Cargo.toml') -PathType Leaf) {
        if ($full -eq $sourceRoot -or
            $full.StartsWith($sourceRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            $sourceRoot.StartsWith($full + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Each scenario requires its own project outside the VCP source checkout. Choose a separate -ProjectPath (for example D:\clitests\A or D:\clitests\B).'
        }
    }
    # A project nested in another repository shares that repository's context.
    $probe = Split-Path -Parent $full
    while ($probe) {
        if (Test-Path -LiteralPath (Join-Path $probe '.git')) {
            throw "Scenario project '$full' is nested in another git project ('$probe'). Choose an independent -ProjectPath."
        }
        $parent = Split-Path -Parent $probe
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
}

function Assert-SafeRunRoot {
    <# VCP rejects data/profiles inside repositories or OneDrive roots; fail before any spend. #>
    param([Parameter(Mandatory)][string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    foreach ($name in 'OneDrive', 'OneDriveConsumer', 'OneDriveCommercial') {
        $sync = [Environment]::GetEnvironmentVariable($name)
        $syncRoot = if ($sync) { [System.IO.Path]::GetFullPath($sync).TrimEnd('\', '/') } else { $null }
        if ($syncRoot -and ($full -eq $syncRoot -or $full.StartsWith($syncRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) {
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
        [string]$ProjectPath,
        [string]$Vcp,
        [Parameter(Mandatory)][string]$ProviderGeneration,
        [ValidateRange(0.01, 1000000)][decimal]$TurnBudgetUsd = 3,
        [ValidateRange(0.01, 1000000)][decimal]$MaxScenarioUsd = 30,
        [ValidateRange(0, 100)][int]$MaxRepairTurns = 1,
        [ValidateRange(1, 2147483647)][int]$OutputTokens = 8192,
        [ValidateRange(1, 2147483647)][int]$MaxRequests = 96,
        [ValidateRange(1, 86100)][int]$DeadlineSeconds = 1800,
        [ValidateRange(1, 86100)][int]$ShortDeadlineSeconds = 150,
        [ValidateRange(0, 12)][double]$MinSnapshotHours = 0,
        [switch]$AllowProcessPublish,
        [switch]$SkipPaidStages
    )
    if (-not $IsWindows) { throw 'The VCP native CLI scenarios require Windows.' }
    if ($PSVersionTable.PSVersion -lt [version]'7.4') { throw 'PowerShell 7.4 or later is required.' }
    Assert-ScenarioBudgetPrecision $TurnBudgetUsd 'TurnBudgetUsd'
    Assert-ScenarioBudgetPrecision $MaxScenarioUsd 'MaxScenarioUsd'
    $runId = '{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), ([guid]::NewGuid().ToString('N').Substring(0, 6))
    $runRootFull = [System.IO.Path]::GetFullPath($RunRoot)
    Assert-SafeRunRoot $runRootFull
    $root = Join-Path $runRootFull (Join-Path $Name $runId)
    $workspace = if ($ProjectPath) { [IO.Path]::GetFullPath($ProjectPath) } else { Join-Path $root 'workspace' }
    Assert-ScenarioProjectPath $workspace
    $workspacePrefix = $workspace.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($root -eq $workspace -or $root.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Run logs, profiles and VCP data must be outside the selected project. Choose a separate -RunRoot.'
    }
    if ((Test-Path -LiteralPath $workspace) -and -not (Test-Path -LiteralPath $workspace -PathType Container)) { throw 'ProjectPath must identify a directory.' }
    $reuseProject = (Test-Path -LiteralPath $workspace -PathType Container) -and
        @(Get-ChildItem -LiteralPath $workspace -Force -ErrorAction Stop | Select-Object -First 1).Count -gt 0
    $ctx = @{
        Name                 = $Name
        RunId                = $runId
        Root                 = $root
        Workspace            = $workspace
        ReuseProject         = [bool]$reuseProject
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
        AllowProcessPublish  = [bool]$AllowProcessPublish
        SpentUsd             = [decimal]0
        CostUnknown          = $false
        AccountedTaskUsd     = @{}
        SettledTaskUsd       = @{}
        UnknownTaskCosts    = @{}
        TaskBudgetUsd       = @{}
        UnscopedCostUnknown = $false
        PaidExecutionBlock   = $null
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
    Save-ScenarioProcessAuthorization $ctx
    $ctx.ProgressLog = Join-Path $ctx.Logs 'progress.log'
    $ctx.CommandLog = Join-Path $ctx.Logs 'vcp-commands.log'
    $ctx.CommandAuditLog = Join-Path $ctx.Logs 'vcp-commands.jsonl'
    $ctx.ToolLog = Join-Path $ctx.Logs 'tool-commands.log'
    $ctx.Vcp = Resolve-VcpExecutable $Vcp
    $generation = [System.IO.Path]::GetFullPath($ProviderGeneration)
    $ctx.Snapshot = Join-Path $generation 'qualified\snapshot.json'
    if (-not (Test-Path -LiteralPath $ctx.Snapshot -PathType Leaf)) { $ctx.Snapshot = Join-Path $generation 'snapshot.json' }
    $ctx.Catalog = Join-Path $generation 'endpoints.json'
    foreach ($required in $ctx.Snapshot, $ctx.Catalog) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Configured provider metadata file missing: $required. Select an existing configured provider with run-cli-scenarios.ps1."
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
    if ($hoursLeft -le 0 -or $hoursLeft -lt $MinSnapshotHours) {
        throw ('Configured provider metadata expires {0} ({1:N1}h left) and cannot be used; provider setup was not changed.' -f $ctx.SnapshotValidUntil, $hoursLeft)
    }
    $credentialName = if ($env:VCP_SCENARIO_CREDENTIAL_ENV) { $env:VCP_SCENARIO_CREDENTIAL_ENV } else { 'OPENROUTER_API_KEY' }
    if (-not $SkipPaidStages -and [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($credentialName, 'Process'))) {
        throw "$credentialName is not set in this console. Use run-cli-scenarios.ps1 for the masked credential prompt."
    }
    if ($null -ne [Environment]::GetEnvironmentVariable('VCP_DENY_PROVIDER_CREDENTIALS', 'Process') -and -not $SkipPaidStages) {
        throw 'VCP_DENY_PROVIDER_CREDENTIALS is set; paid stages cannot run. Remove it or pass -SkipPaidStages.'
    }
    Start-Transcript -LiteralPath (Join-Path $ctx.Logs 'console-transcript.log') | Out-Null
    $ctx.Transcript = $true
    Write-Step $ctx "Run root: $root" 'phase'
    Write-Step $ctx ("Project: {0} ({1})" -f $workspace, $(if ($reuseProject) { 'reusing existing files' } else { 'creating scenario scaffold' })) 'phase'
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
        if ($value -is [bool] -and $value) { $outcome = 'pass' } else { $detail = "returned '$value'" }
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

function ConvertTo-PowerShellCommand {
    <# Single-quoted PowerShell literals preserve spaces, quotes, $, backticks and newlines. #>
    param([string]$Executable, [string[]]$Arguments)
    $tokens = @($Executable) + @($Arguments)
    return '& ' + (($tokens | ForEach-Object { "'" + ([string]$_).Replace("'", "''") + "'" }) -join ' ')
}

function Write-VcpCommandCompletion {
    param([string]$LogPath, [string]$AuditPath, [System.Collections.IDictionary]$Record)
    $Record.completed_at = [datetime]::UtcNow.ToString('o')
    $Record.stdout_exists = Test-Path -LiteralPath $Record.stdout_path -PathType Leaf
    $Record.stderr_exists = Test-Path -LiteralPath $Record.stderr_path -PathType Leaf
    $exitText = if ($null -eq $Record.exit_code) { 'unavailable' } else { [string]$Record.exit_code }
    $lines = @(
        ('{0} [{1}/{2}] END {3} id={4} exit={5} timed_out={6} duration_seconds={7}' -f $Record.completed_at, $Record.stage, $Record.label, $Record.status, $Record.id, $exitText, $Record.timed_out, $Record.duration_seconds),
        ('  task={0} session={1} conditions=[{2}] output_format={3} invalid_jsonl_lines={4} error_type={5}' -f $Record.task, $Record.session, ($Record.conditions -join ','), $Record.output_format, $Record.invalid_jsonl_lines, $Record.error_type),
        ('  stdout (exists={0}): {1}' -f $Record.stdout_exists, $Record.stdout_path),
        ('  stderr (exists={0}): {1}' -f $Record.stderr_exists, $Record.stderr_path),
        ('  workspace: {0}' -f $Record.workspace),
        ('  results directory: {0}' -f $Record.results_directory),
        ('  final summary target (written on scenario completion): {0}' -f $Record.summary_path),
        ('  scorecard target (written on scenario completion): {0}' -f $Record.scorecard_path)
    )
    Add-Content -LiteralPath $LogPath -Encoding utf8NoBOM -Value $lines
    Add-Content -LiteralPath $AuditPath -Encoding utf8NoBOM -Value ($Record | ConvertTo-Json -Depth 12 -Compress)
}

function Invoke-Vcp {
    <#
    Runs one vcp command in structured mode and records it in
    logs/vcp-commands.log plus a structured completion record in vcp-commands.jsonl.
    Returns exit code, frames, the first accepted frame,
    the final result frame and the scope.
    #>
    param(
        [Parameter(Mandatory)]$Ctx,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][AllowEmptyString()][string[]]$Arguments,
        [string]$Config,
        [int]$TimeoutSeconds = 300,
        [switch]$Live,
        [switch]$NoGlobals,
        [switch]$DenyProviderCredentials
    )
    $directory = Join-Path $Ctx.Logs (Join-Path $Stage 'vcp')
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $safe = ($Label -replace '[^A-Za-z0-9_.-]', '-')
    $index = '{0:D2}' -f ((Get-ChildItem -LiteralPath $directory -Filter '*.stdout.jsonl' -ErrorAction SilentlyContinue | Measure-Object).Count + 1)
    $stdout = Join-Path $directory "$index-$safe.stdout.jsonl"
    $stderr = Join-Path $directory "$index-$safe.stderr.txt"
    $all = if ($NoGlobals) { $Arguments } else { (Get-VcpGlobalArguments $Ctx $Config) + $Arguments }
    $commandLog = if ($Ctx.CommandLog) { [IO.Path]::GetFullPath($Ctx.CommandLog) } else { Join-Path ([IO.Path]::GetFullPath($Ctx.Logs)) 'vcp-commands.log' }
    $auditLog = if ($Ctx.CommandAuditLog) { [IO.Path]::GetFullPath($Ctx.CommandAuditLog) } else { Join-Path (Split-Path -Parent $commandLog) 'vcp-commands.jsonl' }
    foreach ($path in $commandLog, $auditLog) { New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null }
    $resultsDirectory = if ($Ctx.Results) { [IO.Path]::GetFullPath($Ctx.Results) } else { Join-Path (Split-Path -Parent ([IO.Path]::GetFullPath($Ctx.Logs))) 'results' }
    $record = [ordered]@{
        schema_version = 1; id = [guid]::NewGuid().ToString('N'); stage = $Stage; label = $Label
        started_at = [datetime]::UtcNow.ToString('o'); completed_at = $null; status = 'running'
        executable = [string]$Ctx.Vcp; argv = @($all); command = ConvertTo-PowerShellCommand $Ctx.Vcp $all
        output_format = if ($NoGlobals) { 'text' } else { 'jsonl' }
        workspace = [IO.Path]::GetFullPath($Ctx.Workspace); results_directory = $resultsDirectory
        summary_path = Join-Path $resultsDirectory 'summary.md'; scorecard_path = Join-Path $resultsDirectory 'scorecard.json'
        stdout_path = [IO.Path]::GetFullPath($stdout); stderr_path = [IO.Path]::GetFullPath($stderr)
        exit_code = $null; timed_out = $false; duration_seconds = $null
        accepted = $false; task = $null; session = $null; conditions = @(); invalid_jsonl_lines = $null; error_type = $null
    }
    Add-Content -LiteralPath $commandLog -Encoding utf8NoBOM -Value @(
        ('{0} [{1}/{2}] START id={3}' -f $record.started_at, $Stage, $Label, $record.id),
        ('  working directory: {0}' -f $record.workspace),
        ('  command: {0}' -f $record.command)
    )
    $invocationClock = [Diagnostics.Stopwatch]::StartNew()
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
    try {
        $environment = @{}
        if ($DenyProviderCredentials -or $Ctx.SkipPaidStages) {
            $environment['OPENROUTER_API_KEY'] = $null
            if ($env:VCP_SCENARIO_CREDENTIAL_ENV) { $environment[$env:VCP_SCENARIO_CREDENTIAL_ENV] = $null }
            $environment['VCP_DENY_PROVIDER_CREDENTIALS'] = '1'
        }
        $result = Invoke-NativeLogged -FilePath $Ctx.Vcp -ArgumentList $all -WorkingDirectory $Ctx.Workspace -StdoutPath $stdout `
        -StderrPath $stderr -TimeoutSeconds $TimeoutSeconds -OnLine $onLine -HeartbeatLabel "$Stage/$Label" -Ctx $Ctx `
        -Environment $environment
        $record.exit_code = $result.ExitCode; $record.timed_out = $result.TimedOut; $record.duration_seconds = $result.DurationSeconds
        $parsed = ConvertFrom-JsonLines ([System.IO.File]::ReadAllText($stdout))
        $accepted = $parsed.Frames | Where-Object { $_.type -eq 'accepted' } | Select-Object -First 1
        $final = $parsed.Frames | Where-Object { $_.type -eq 'result' } | Select-Object -Last 1
        $scope = if ($accepted -and $accepted.scope) { $accepted.scope } elseif ($final -and $final.scope) { $final.scope } else { $null }
        $record.accepted = $null -ne $accepted
        $record.task = if ($scope) { $scope.task } else { $null }
        $record.session = if ($scope) { $scope.session } else { $null }
        $record.conditions = @(Get-VcpConditions $final)
        $record.invalid_jsonl_lines = if ($record.output_format -eq 'jsonl') { $parsed.Invalid.Count } else { $null }
        $record.status = if ($result.TimedOut) { 'timed_out' } elseif ($result.ExitCode -eq 0) { 'succeeded' } else { 'nonzero_exit' }
    }
    catch {
        $record.status = 'invocation_failed'
        $record.error_type = $_.Exception.GetType().FullName
        if ($null -eq $record.duration_seconds) { $record.duration_seconds = [math]::Round($invocationClock.Elapsed.TotalSeconds, 3) }
        Write-VcpCommandCompletion $commandLog $auditLog $record
        throw
    }
    Write-VcpCommandCompletion $commandLog $auditLog $record
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
    $complete = $false
    for ($page = 0; $page -lt 64; $page++) {
        $arguments = @('inspect', $Id, '--view', $View, '--limit', '128')
        if ($cursor) { $arguments += @('--cursor', ($cursor | ConvertTo-Json -Depth 50 -Compress)) }
        $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label "inspect-$View" -Arguments $arguments -TimeoutSeconds 120
        if ($run.ExitCode -ne 0 -or $run.InvalidLines -ne 0 -or -not $run.Result -or -not $run.Result.data) { break }
        $pages.Add($run.Result.data)
        $cursor = $run.Result.data.next_cursor
        if (-not $cursor) { $complete = $true; break }
    }
    if (-not $complete) {
        $pages.Add([pscustomobject]@{ items = @(); gaps = @(@{ reason = 'harness inspection failed or exceeded 64 pages' }); next_cursor = $cursor })
    }
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "inspect-$View" -Description "inspect $View returned all pages" -Test { $complete })
    if ($Name) { Write-JsonFile -Path (Join-Path $Ctx.Logs (Join-Path $Stage "inspect-$Name.json")) -Value $pages }
    return , $pages
}

function Get-InspectItems {
    param($Pages)
    return @($Pages | ForEach-Object { $_.items } | Where-Object { $_ })
}

function Get-VcpStageInspection {
    <# One canonical read owner for every page; a failed bundle never silently
       falls back to a costly or potentially inconsistent second evidence sweep. #>
    param($Ctx, [string]$Stage, [string]$Task)
    $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'inspect-bundle' -Arguments @('inspect-bundle', $Task) -TimeoutSeconds 1800
    $bundle = $run.Result.data
    $valid = $run.ExitCode -eq 0 -and -not $run.TimedOut -and $run.InvalidLines -eq 0 -and
        $bundle.schema_version -eq 1 -and $bundle.source_watermark -and $bundle.task.scope.task -eq $Task
    $views = @{}
    foreach ($view in 'costs', 'verification', 'tools', 'routing', 'policy', 'outputs') {
        $pages = @($bundle.views.$view | Where-Object { $null -ne $_ })
        $complete = $valid -and $pages.Count -gt 0 -and -not $pages[-1].next_cursor -and
            @($pages | Where-Object { $_.source_watermark -ne $bundle.source_watermark -or $_.scope.task -ne $Task -or $_.view -ne $view }).Count -eq 0
        [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "inspect-$view" -Description "inspect $view returned all pages in one canonical bundle" -Test { $complete })
        if (-not $complete) { $pages += [pscustomobject]@{ items = @(); gaps = @(@{ reason = 'incomplete or invalid inspection bundle' }); next_cursor = $null } }
        Write-JsonFile (Join-Path $Ctx.Logs "$Stage/inspect-$view.json") $pages
        $views[$view] = $pages
    }
    Write-JsonFile (Join-Path $Ctx.Logs "$Stage/inspection-bundle.json") $bundle
    Write-JsonFile (Join-Path $Ctx.Logs "$Stage/tasks-status.json") $bundle.task
    Write-JsonFile (Join-Path $Ctx.Logs "$Stage/tasks-agents.json") $bundle.agents
    Write-JsonFile (Join-Path $Ctx.Logs "$Stage/history.json") $bundle.history
    return $views
}

function Get-VcpTaskCost {
    <# Settled cost from canonical ledger records (decimal micros strings). Null when evidence is incomplete. #>
    param($CostPages)
    $items = Get-InspectItems $CostPages
    $ledgers = @($items | Where-Object { $_.collection -eq 'ledger' })
    $attempts = @($items | Where-Object { $_.collection -eq 'attempt' })
    $gaps = @($CostPages | ForEach-Object { $_.gaps } | Where-Object { $_ })
    $incomplete = $gaps.Count -gt 0 -or @($CostPages).Count -eq 0 -or @($CostPages)[-1].next_cursor -or
        @($items | Where-Object { $_.visibility -and $_.visibility -ne 'available' }).Count -gt 0
    if ($ledgers.Count -ne 1 -or $incomplete) { return [pscustomobject]@{ Usd = $null; Attempts = $attempts.Count; Gaps = $gaps.Count } }
    $micros = [decimal]0
    foreach ($ledger in $ledgers) {
        foreach ($field in 'settled', 'active', 'unresolved') {
            if ([string]$ledger.record.$field -notmatch '^\d+$') { return [pscustomobject]@{ Usd = $null; Attempts = $attempts.Count; Gaps = $gaps.Count } }
        }
        if ([decimal]$ledger.record.active -ne 0 -or [decimal]$ledger.record.unresolved -ne 0) {
            return [pscustomobject]@{ Usd = $null; Attempts = $attempts.Count; Gaps = $gaps.Count }
        }
        $micros += [decimal]::Parse([string]$ledger.record.settled, [System.Globalization.CultureInfo]::InvariantCulture)
    }
    return [pscustomobject]@{ Usd = [math]::Round($micros / 1000000, 6); Attempts = $attempts.Count; Gaps = $gaps.Count }
}

function Get-VcpFinalMessage {
    <#
    The final assistant text is not part of the JSONL stream. Read the last
    captured provider response artifact through inspect --view outputs and
    extract output_text from its response.completed SSE event. Best effort.
    #>
    param($Ctx, [string]$Stage, $OutputPages, $Frames)
    $buffer = $null
    try {
        $responses = @(Get-InspectItems $OutputPages | Where-Object { $_.collection -eq 'artifact' -and $_.record.spec.channel -eq 'response' })
        if ($responses.Count -eq 0) { return $null }
        # Inspection is ordered by canonical key, not completion time. Use the
        # ordered event facts to identify the last completed response instead.
        $responseId = $null
        foreach ($frame in $Frames) {
            if ($frame.type -ne 'event') { continue }
            foreach ($fact in @($frame.event.event.data.facts)) {
                if ($fact.collection -eq 'artifact' -and $fact.value.spec.channel -eq 'response' -and $fact.value.state -eq 'complete') {
                    $responseId = [string]$fact.id
                }
            }
        }
        $item = $responses | Where-Object { $_.id -eq $responseId } | Select-Object -First 1
        if (-not $item -or $item.record.state -ne 'complete' -or $item.record.sha256 -notmatch '^[0-9a-f]{64}$') { return $null }
        if (@($item.record.spec.omissions | Where-Object { $_ -notin 'authentication_headers', 'recovery_material' }).Count) { return $null }
        $length = [int64]$item.record.length
        if ($length -le 0 -or $length -gt 4MB) { return $null }
        $buffer = [System.IO.MemoryStream]::new()
        for ($offset = [int64]0; $offset -lt $length; $offset += 65536) {
            # Range reads replay the same canonical history as inspect-bundle.
            # The optimized B candidate already needed 268 seconds at stage T2;
            # later stages retain more history. Bound metadata reads separately
            # from paid provider work without discarding verification evidence.
            $read = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'inspect-response-range' -TimeoutSeconds 1800 `
                -Arguments @('inspect', $item.id, '--view', 'outputs', '--offset', [string]$offset, '--length', '65536')
            if ($read.ExitCode -ne 0 -or $read.InvalidLines -ne 0 -or @($read.Result.data.items).Count -ne 1) { return $null }
            $row = $read.Result.data.items | Select-Object -First 1
            $end = [math]::Min($offset + 65536, $length)
            if ($row.artifact -ne $item.id -or $row.visibility -ne 'available' -or
                $row.descriptor.state -ne 'complete' -or $row.descriptor.sha256 -ne $item.record.sha256 -or
                [int64]$row.descriptor.length -ne $length -or $row.range.start -ne $offset -or $row.range.end -ne $end -or
                @($row.bytes).Count -ne ($end - $offset)) { return $null }
            if (($end -lt $length -and $row.next_offset -ne $end) -or ($end -eq $length -and $null -ne $row.next_offset)) { return $null }
            foreach ($gap in $read.Result.data.gaps) {
                if ($gap.visibility -notin 'omitted', 'redacted' -or
                    @($gap.omissions | Where-Object { $_ -notin 'authentication_headers', 'recovery_material' }).Count) { return $null }
            }
            $bytes = [byte[]]@($row.bytes)
            $buffer.Write($bytes, 0, $bytes.Length)
        }
        $captured = $buffer.ToArray()
        $digest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($captured)).ToLowerInvariant()
        if ($captured.Length -ne $length -or $digest -ne $item.record.sha256) { return $null }
        $sse = $script:Utf8NoBom.GetString($captured)
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
    finally { if ($buffer) { $buffer.Dispose() } }
}

function Get-VcpStageFinalMessage {
    <# Extract derived assistant text only when a gate or caller requests it.
       The mandatory evidence sweep already retained output descriptors and
       chronological frames; the existing extractor verifies every byte range. #>
    param($Ctx, $StageRecord)
    if (-not $StageRecord -or -not $StageRecord.task) { return $null }
    $stage = [string]$StageRecord.stage
    $path = Join-Path $Ctx.Logs "$stage/final-message.md"
    try {
        if ($StageRecord.final_message -and (Test-Path -LiteralPath $path)) {
            return [IO.File]::ReadAllText($path)
        }
        $outputsPath = Join-Path $Ctx.Logs "$stage/inspect-outputs.json"
        if (-not $StageRecord.Run -or -not (Test-Path -LiteralPath $outputsPath)) { return $null }
        $outputs = Get-Content -LiteralPath $outputsPath -Raw | ConvertFrom-Json -Depth 100
        $message = Get-VcpFinalMessage -Ctx $Ctx -Stage $stage -OutputPages $outputs -Frames $StageRecord.Run.Frames
        if ($message) {
            Write-Utf8File -Path $path -Content $message
            $StageRecord.final_message = "logs/$stage/final-message.md"
            return $message
        }
    }
    catch { return $null }
    return $null
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
    param([Parameter(Mandatory)][string]$Path, [switch]$IncludeGenerated, [switch]$ForCheckpoint)
    $root = (Resolve-Path -LiteralPath $Path).Path.TrimEnd('\')
    $manifest = [ordered]@{}
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($root)
    while ($pending.Count) {
        $directory = $pending.Pop()
        foreach ($entry in Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop) {
            $relative = $entry.FullName.Substring($root.Length + 1).Replace('\', '/')
            if ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                # Record links without walking outside the workspace or into cycles.
                $manifest[$relative] = 'link:' + $entry.LinkTarget
                continue
            }
            if ($entry.PSIsContainer) {
                $excluded = $script:ManifestExclusions -contains $entry.Name -or $entry.Name -like '*.egg-info'
                # Ambiguous ML/cache/report names are root-only exclusions for
                # checkpoints; nested Models and Pages/Reports may be source.
                if ($ForCheckpoint -and $entry.Name -in 'models', 'models-repro', 'reports' -and $relative.Contains('/')) { $excluded = $false }
                if (-not $IncludeGenerated -and $excluded) { continue }
                if ($IncludeGenerated) { $manifest[$relative + '/'] = 'directory' }
                $pending.Push($entry.FullName)
            }
            else {
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

function Update-PaidExecutionBlock {
    # Record the observed stopping condition before collecting read-only evidence.
    # A killed/missing result is an interruption, not proof of durable cancellation.
    param($Ctx, [string]$Stage, $Run)
    $conditions = @(Get-VcpConditions $Run.Result)
    $reasons = [Collections.Generic.List[string]]::new()
    foreach ($condition in 'required_input', 'unresolved_effect', 'internal_failure', 'cancelled', 'invalid_configuration', 'budget_exhausted') {
        if ($conditions -contains $condition) { $reasons.Add($condition) }
    }
    $exitReason = switch ($Run.ExitCode) { 1 { 'internal_failure' }; 2 { 'invalid_configuration' }; 4 { 'required_input' }; 5 { 'budget_exhausted' }; 6 { 'cancelled' }; 7 { 'unresolved_effect' } }
    if ($exitReason -and -not $reasons.Contains($exitReason)) { $reasons.Add($exitReason) }
    if ($Run.TimedOut) { $reasons.Add('harness_timeout') }
    if (-not $Run.Result) { $reasons.Add('missing_terminal_result') }
    if ($Run.ExitCode -notin 0, 1, 2, 3, 4, 5, 6, 7, 8) { $reasons.Add('interrupted_or_unknown_exit') }
    $approvals = @($Run.Frames | Where-Object type -eq 'required_input' | ForEach-Object { $_.approval } | Where-Object { $_ } | Select-Object -Unique)
    if ($approvals.Count -and -not $Run.Result -and -not $reasons.Contains('required_input')) { $reasons.Add('required_input') }
    $task = if ($Run.Scope) { [string]$Run.Scope.task } else { $null }
    $paused = ($conditions -contains 'durably_paused') -or $Run.ExitCode -eq 8
    $resumable = $paused -and $reasons.Count -eq 0 -and -not [string]::IsNullOrWhiteSpace($task)
    if ($reasons.Count -eq 0 -and -not $paused) {
        if ($Ctx.PaidExecutionBlock -and $Ctx.PaidExecutionBlock.resume_same_task -and $Ctx.PaidExecutionBlock.task -eq $task) {
            $Ctx.PaidExecutionBlock = $null
            Write-JsonFile (Join-Path $Ctx.Results 'paid-execution.json') @{ status = 'resumed'; stage = $Stage; task = $task; blocked = $false }
        }
        return
    }
    if ($paused -and $reasons.Count -eq 0) { $reasons.Add('durably_paused') }
    $block = [ordered]@{
        stage = $Stage; task = $task; session = $(if ($Run.Scope) { [string]$Run.Scope.session } else { $null })
        reasons = @($reasons); approval_ids = $approvals; exit_code = $Run.ExitCode; resume_same_task = [bool]$resumable
        stdout = $Run.StdoutPath; stderr = $Run.StderrPath; inspection = (Join-Path $Ctx.Logs $Stage)
        policy = (Join-Path $Ctx.Logs "$Stage/inspect-policy.json"); tools = (Join-Path $Ctx.Logs "$Stage/inspect-tools.json")
        commands = $(if ($Ctx.CommandLog) { $Ctx.CommandLog } else { Join-Path $Ctx.Logs 'vcp-commands.log' })
    }
    $Ctx.PaidExecutionBlock = [pscustomobject]$block
    Write-JsonFile (Join-Path $Ctx.Results 'paid-execution.json') $block
    $next = if ($resumable) { 'Only a resume of this same task may continue.' } else { 'No further paid tasks, repairs, reviews, forks or resumes will start in this run.' }
    $note = "$Stage stopped paid execution: $($reasons -join ', '); task $task. $next Evidence: $($block.inspection); approvals: $($approvals -join ', ')."
    $Ctx.Notes.Add($note)
    Write-Step $Ctx $note 'warn'
}

function Test-PaidExecutionAdmission {
    param($Ctx, $Record, [string[]]$Arguments = @())
    $block = $Ctx.PaidExecutionBlock
    if (-not $block) { return $true }
    if ($block.resume_same_task -and $Arguments.Count -ge 2) {
        $source = $null
        $prior = @($Ctx.Stages | Where-Object { $_.task -and -not $_.skipped })
        if ($Arguments[0] -eq 'resume') {
            $source = if ($Arguments[1] -eq '--last') { $prior | Select-Object -Last 1 }
                else { $prior | Where-Object task -eq $Arguments[1] | Select-Object -Last 1 }
        }
        elseif ($Arguments.Count -ge 3 -and $Arguments[0] -eq 'sessions' -and $Arguments[1] -eq 'resume') {
            $source = $prior | Where-Object session -eq $Arguments[2] | Select-Object -Last 1
        }
        if ($source -and $source.task -eq $block.task) { return $true }
    }
    $diagnostic = if ($block.task_reason) { "; VCP reason: $($block.task_reason)" } else { '' }
    $Record.skipped = "paid execution stopped by $($block.stage): $($block.reasons -join ', '); task $($block.task)$diagnostic; inspect $($block.inspection)"
    $Ctx.Stages.Add([pscustomobject]$Record)
    Write-Step $Ctx "Skipping $($Record.stage); $($Record.skipped)" 'warn'
    # Let each scenario's catch/finally finalize its evidence now. Returning null
    # here would still run later fixture mutations and unrelated assessments.
    throw "Scenario stopped before $($Record.stage): $($Record.skipped). Results: $($Ctx.Results)"
}

function Get-FreshScenarioProfile {
    <# New tasks may use fresh tariffs; an existing task's captured selection is immutable.
       Retain original profiles and catalogs so every admission remains inspectable. #>
    param($Ctx, [string]$Stage, [string]$Config)
    if (-not $Config) { return $Config }
    $profile = Get-Content -LiteralPath $Config -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    if (-not $profile.provider.valid_until) { throw "Profile has no provider expiry: $Config" }
    $window = [math]::Max(300, [int]$profile.deadline_seconds + 300)
    $neededUntil = [DateTimeOffset]::UtcNow.AddSeconds($window)
    if ([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$profile.provider.valid_until) -gt $neededUntil) { return $Config }
    $snapshot = if ($Ctx.SnapshotText) { $Ctx.SnapshotText | ConvertFrom-Json -AsHashtable -Depth 100 } else { $null }
    $matches = $snapshot -and $snapshot.compatibility.model -ceq $profile.provider.compatibility.model -and
        $snapshot.compatibility.endpoint -ceq $profile.provider.compatibility.endpoint
    if (-not $matches -or [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until) -le $neededUntil) {
        $generation = Join-Path $Ctx.Root ('provider-refresh-' + [guid]::NewGuid().ToString('N'))
        $inputSnapshot = Join-Path $Ctx.Profiles ('refresh-input-' + [guid]::NewGuid().ToString('N') + '.json')
        Write-JsonFile $inputSnapshot $profile.provider
        Write-Step $Ctx "$Stage provider metadata needs renewal; fetching the same endpoint without inference." 'phase'
        $refresh = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'provider-refresh' -DenyProviderCredentials -Arguments @(
            'setup', 'provider-refresh', '--snapshot', $inputSnapshot, '--catalog', $profile.catalog, '--output', $generation)
        if ($refresh.ExitCode -ne 0 -or $refresh.TimedOut -or $refresh.InvalidLines -ne 0 -or $refresh.Result.data.status -ne 'refreshed' -or $refresh.Result.data.model_calls -ne 0) {
            throw "Metadata-only refresh failed before $Stage; no task started. Use a VCP build with setup provider-refresh. See $($refresh.StderrPath)."
        }
        $newSnapshot = Join-Path $generation 'snapshot.json'
        $newCatalog = Join-Path $generation 'endpoints.json'
        $snapshot = Get-Content -LiteralPath $newSnapshot -Raw | ConvertFrom-Json -AsHashtable -Depth 100
        Assert-That ($snapshot.compatibility.model -ceq $profile.provider.compatibility.model -and
            $snapshot.compatibility.endpoint -ceq $profile.provider.compatibility.endpoint) 'Refresh changed the selected model or endpoint'
        Assert-That ((Get-FileHash -LiteralPath $newCatalog -Algorithm SHA256).Hash -ieq $snapshot.raw_sha256) 'Refreshed catalog hash mismatch'
        Assert-That ([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until) -gt $neededUntil) 'Fresh metadata does not cover the next task deadline'
        $Ctx.Snapshot = $newSnapshot; $Ctx.Catalog = $newCatalog
        $Ctx.SnapshotText = Get-Content -LiteralPath $newSnapshot -Raw
        $Ctx.SnapshotValidUntil = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until).ToString('o')
    }
    $profile.provider = $snapshot
    $profile.catalog = $Ctx.Catalog
    $freshProfile = Join-Path $Ctx.Profiles ('profile-refreshed-' + [guid]::NewGuid().ToString('N') + '.json')
    Write-JsonFile $freshProfile $profile
    $check = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'setup-check-refreshed' -Config $freshProfile -DenyProviderCredentials -Arguments @('setup', 'check')
    if ($check.ExitCode -ne 0 -or $check.TimedOut -or $check.InvalidLines -ne 0) { throw "Refreshed profile rejected before $Stage; inspect $($check.StderrPath)." }
    Write-JsonFile (Join-Path $Ctx.Logs "$Stage/provider-refresh.json") @{
        original_profile = $Config; admitted_profile = $freshProfile; snapshot = $Ctx.Snapshot
        catalog = $Ctx.Catalog; model_calls = 0; valid_until = $Ctx.SnapshotValidUntil
    }
    return $freshProfile
}

function Invoke-PaidScenarioDispatch {
    <# Reserve before process dispatch, including exceptions while decoding its
       output or collecting canonical accounting. The evidence collector keeps
       its normal cumulative per-task accounting; remove this temporary hold
       only after that collector returns successfully. #>
    param($Ctx, [decimal]$AdditionalBudgetUsd, [scriptblock]$Dispatch)
    $priorSpent = [decimal]$Ctx.SpentUsd
    $Ctx.SpentUsd += $AdditionalBudgetUsd
    $Ctx.CostUnknown = $true
    $completed = $false
    try {
        $result = & $Dispatch
        $Ctx.SpentUsd -= $AdditionalBudgetUsd
        $Ctx.CostUnknown = [bool]$Ctx.UnscopedCostUnknown -or $Ctx.UnknownTaskCosts.Count -gt 0
        $completed = $true
        return $result
    }
    finally {
        # Evidence collection may have recorded some settlement before a later
        # operation threw. Retain at least the full possible spend, without
        # double-counting that settlement or prior spend on a resumed task.
        # Finally also covers pipeline cancellation, which may bypass catch.
        if (-not $completed) {
            $Ctx.SpentUsd = [math]::Max($priorSpent + $AdditionalBudgetUsd, $Ctx.SpentUsd - $AdditionalBudgetUsd)
            $Ctx.CostUnknown = $true
            $Ctx.UnscopedCostUnknown = $true
        }
    }
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
    Assert-ScenarioBudgetPrecision $BudgetUsd 'BudgetUsd'
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
    if (-not (Test-PaidExecutionAdmission $Ctx $stageRecord)) { return $null }
    $preflightFailures = @($Ctx.Gates | Where-Object { $_.stage -match '^(P0-|B0-|P1-|G0-)' -and $_.required -and $_.outcome -ne 'pass' })
    if ($preflightFailures.Count) { throw 'Required preflight, baseline, profile or guardrail checks failed; refusing paid execution.' }
    if (($Ctx.SpentUsd + $BudgetUsd) -gt $Ctx.MaxScenarioUsd) {
        $stageRecord.skipped = ('scenario ceiling {0} USD would be exceeded (spent {1})' -f (Format-Usd $Ctx.MaxScenarioUsd), (Format-Usd $Ctx.SpentUsd))
        $Ctx.Stages.Add([pscustomobject]$stageRecord)
        Write-Step $Ctx "Skipping $Stage; $($stageRecord.skipped)" 'warn'
        return $null
    }
    $Config = Get-FreshScenarioProfile $Ctx $Stage $Config
    $stageRecord.profile = $Config
    Write-Step $Ctx "$Stage :: $Title (autonomy $Autonomy, cap $(Format-Usd $BudgetUsd) USD)" 'phase'
    $stageDir = Join-Path $Ctx.Logs $Stage
    $promptPath = Join-Path $stageDir 'prompt.md'
    Write-Utf8File -Path $promptPath -Content $Prompt
    $before = Get-WorkspaceManifest $Ctx.Workspace
    $arguments = @('run', '--file', $promptPath, '--budget-usd', (Format-Usd $BudgetUsd), '--autonomy', $Autonomy)
    foreach ($id in $Skill) { $arguments += @('--skill', $id) }
    return Invoke-PaidScenarioDispatch $Ctx $BudgetUsd {
        $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label 'run' -Config $Config -Arguments $arguments `
            -TimeoutSeconds ($Ctx.DeadlineSeconds + 300) -Live
        Complete-VcpStageEvidence -Ctx $Ctx -Stage $Stage -Run $run -Before $before -Record $stageRecord
    }
}

function Test-VcpPreAdmissionRejection {
    <# Only a complete, unscoped configuration result proves that execution never started.
       Missing/truncated output, acceptance, timeouts and other conditions remain uncertain. #>
    param($Run)
    $frames = @($Run.Frames)
    if ($Run.TimedOut -or $Run.ExitCode -ne 2 -or $Run.InvalidLines -ne 0 -or
        $Run.Accepted -or $Run.Scope -or $frames.Count -ne 1) { return $false }
    $frame = $frames[0]
    $conditions = @(Get-VcpConditions $frame)
    return $frame.type -eq 'result' -and $frame.schema_version -eq 1 -and
        -not [string]::IsNullOrWhiteSpace($frame.correlation) -and $null -eq $frame.scope -and
        $null -eq $frame.receipt -and $frame.exit_code -eq 2 -and
        $conditions.Count -eq 1 -and $conditions[0] -eq 'invalid_configuration'
}

function Complete-VcpStageEvidence {
    <# Shared evidence sweep for run, resume and fork stages. #>
    param($Ctx, [string]$Stage, $Run, $Before, $Record)
    $Record.exit_code = $Run.ExitCode
    $Record.duration_seconds = $Run.DurationSeconds
    $Record.conditions = Get-VcpConditions $Run.Result
    $Record.event_counts = $Run.EventCounts
    Update-PaidExecutionBlock $Ctx $Stage $Run
    if ($Run.TimedOut) { $Ctx.Notes.Add("$Stage exceeded the harness timeout and was killed; inspect for unresolved effects.") }
    $task = if ($Run.Scope) { [string]$Run.Scope.task } else { $null }
    $Record.task = $task
    $Record.session = if ($Run.Scope) { [string]$Run.Scope.session } else { $null }
    $requiredInput = @($Run.Frames | Where-Object { $_.type -eq 'required_input' })
    if ($requiredInput.Count) {
        $Ctx.Notes.Add("$Stage reported $($requiredInput.Count) required input(s); inspect the saved policy and tool evidence for the cause.")
    }
    if ($task) {
      if ($Ctx.SupportsInspectionBundle) {
        $views = Get-VcpStageInspection $Ctx $Stage $task
        $costs = $views.costs; $tools = $views.tools; $outputs = $views.outputs
      }
      else {
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
      }
        $statusPath = Join-Path $Ctx.Logs "$Stage/tasks-status.json"
        if ($Ctx.PaidExecutionBlock -and $Ctx.PaidExecutionBlock.task -eq $task -and (Test-Path -LiteralPath $statusPath)) {
            $taskStatus = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json -Depth 100
            if ($taskStatus.data) { $taskStatus = $taskStatus.data }
            if ($taskStatus.reason) {
                $reason = [regex]::Replace([string]$taskStatus.reason, '[\x00-\x1f\x7f]', ' ')
                if ($reason.Length -gt 1000) { $reason = $reason.Substring(0, 1000) + ' ...' }
                $Record.task_reason = $reason
                $Ctx.PaidExecutionBlock | Add-Member -NotePropertyName task_reason -NotePropertyValue $reason -Force
                Write-JsonFile (Join-Path $Ctx.Results 'paid-execution.json') $Ctx.PaidExecutionBlock
                $Ctx.Notes.Add("$Stage VCP task reason: $reason")
                Write-Step $Ctx "$Stage VCP task reason: $reason" 'warn'
            }
        }
        $cost = Get-VcpTaskCost $costs
        $Record.cost_usd = $cost.Usd
        $Record.attempts = $cost.Attempts
        $Record.tool_items = (Get-InspectItems $tools).Count
        Update-ScenarioCost -Ctx $Ctx -Task $task -Cost $cost.Usd -Record $Record
        [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id 'cost-evidence' -Description 'canonical task cost is complete and settled at this checkpoint' -Advisory -Test { $null -ne $cost.Usd })
        # Optional final text is extracted on demand by its consuming gate.
        # Keep the complete outputs view and original response artifacts here.
    }
    elseif (Test-VcpPreAdmissionRejection $Run) {
        $Record.cost_usd = [decimal]0
        $Record.attempts = 0
        $Record.admission = 'rejected before task acceptance'
        $Ctx.Notes.Add("$Stage was rejected before task acceptance (complete unscoped configuration result); no new task spend or uncertainty reservation. See logs/$Stage/vcp.")
    }
    elseif ($Record.budget_usd) {
        # A missing scope can also mean truncated output or a killed process.
        # It does not prove that no provider request was dispatched.
        $Ctx.CostUnknown = $true
        $Ctx.UnscopedCostUnknown = $true
        $reservation = if ($Record.Contains('additional_budget_usd')) { [decimal]$Record.additional_budget_usd } else { [decimal]$Record.budget_usd }
        $Ctx.SpentUsd += $reservation
        $Ctx.Notes.Add("$Stage produced no task scope (exit $($Run.ExitCode)); reserved the possible additional spend and retained unknown accounting. See logs/$Stage/vcp.")
    }
    if ($null -ne $Before) {
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

function Update-ScenarioCost {
    <# Each inspect ledger is cumulative for its task, including after resume. #>
    param($Ctx, [string]$Task, $Cost, $Record)
    if ($null -eq $Ctx.UnknownTaskCosts) { $Ctx.UnknownTaskCosts = @{} }
    if ($null -eq $Ctx.TaskBudgetUsd) { $Ctx.TaskBudgetUsd = @{} }
    if (-not $Ctx.TaskBudgetUsd.ContainsKey($Task)) { $Ctx.TaskBudgetUsd[$Task] = [decimal]$Record.budget_usd }
    $priorAccounted = [decimal]$Ctx.AccountedTaskUsd[$Task]
    if ($null -eq $Cost) {
        $Ctx.UnknownTaskCosts[$Task] = $true
        $accounted = [math]::Max($priorAccounted, [decimal]$Ctx.TaskBudgetUsd[$Task])
        $Record.cost_usd = $null
        $Ctx.Notes.Add("$($Record.stage) cost evidence incomplete; budget guard reserves the full task cap.")
    }
    else {
        $accounted = [decimal]$Cost
        $Record.cost_usd = $accounted - [decimal]$Ctx.SettledTaskUsd[$Task]
        $Ctx.SettledTaskUsd[$Task] = $accounted
        [void]$Ctx.UnknownTaskCosts.Remove($Task)
    }
    $Record.task_cost_usd = $Cost
    $Ctx.SpentUsd += $accounted - $priorAccounted
    $Ctx.AccountedTaskUsd[$Task] = $accounted
    $Ctx.CostUnknown = [bool]$Ctx.UnscopedCostUnknown -or $Ctx.UnknownTaskCosts.Count -gt 0
}

function Get-ContinuationBudget {
    <# Resume shares a durable task cap; fork admits a new task at the selected profile cap. #>
    param($Ctx, [string[]]$Arguments, [string]$Config)
    $isFork = $Arguments[0] -eq 'sessions' -and $Arguments[1] -eq 'fork'
    $prior = @($Ctx.Stages | Where-Object { $_.task -and -not $_.skipped })
    $source = if ($Arguments[0] -eq 'sessions') {
        $prior | Where-Object { $_.session -eq $Arguments[2] } | Select-Object -Last 1
    }
    elseif ($Arguments[1] -eq '--last') { $prior | Select-Object -Last 1 }
    else { $prior | Where-Object { $_.task -eq $Arguments[1] } | Select-Object -Last 1 }
    if (-not $source) { throw 'Continuation has no matching recorded task; its durable budget cannot be bounded safely.' }
    $cap = [decimal]$source.budget_usd
    if ($isFork) {
        # CLI app::execute uses profile.budget_usd for a fork, which may differ
        # from the source review's smaller run --budget-usd override.
        if (-not $Config) { throw 'Fork requires a profile with an explicit budget_usd.' }
        $profile = [IO.File]::ReadAllText($Config) | ConvertFrom-Json -Depth 100
        $cap = [decimal]::Parse([string]$profile.budget_usd, [Globalization.CultureInfo]::InvariantCulture)
    }
    elseif ($Ctx.TaskBudgetUsd -and $Ctx.TaskBudgetUsd.ContainsKey([string]$source.task)) {
        $cap = [decimal]$Ctx.TaskBudgetUsd[[string]$source.task]
    }
    if ($cap -le 0) { throw 'Continuation budget must be positive.' }
    $alreadyAccounted = if ($isFork) { [decimal]0 } else { [decimal]$Ctx.AccountedTaskUsd[[string]$source.task] }
    return [pscustomobject]@{ Cap = $cap; Additional = [math]::Max([decimal]0, $cap - $alreadyAccounted) }
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
    if (-not (Test-PaidExecutionAdmission $Ctx $record $Arguments)) { return $null }
    $budget = Get-ContinuationBudget $Ctx $Arguments $Config
    $record.budget_usd = $budget.Cap
    $record.additional_budget_usd = $budget.Additional
    if (($Ctx.SpentUsd + $budget.Additional) -gt $Ctx.MaxScenarioUsd) {
        $record.skipped = ('scenario ceiling {0} USD would be exceeded (spent {1})' -f (Format-Usd $Ctx.MaxScenarioUsd), (Format-Usd $Ctx.SpentUsd))
        $Ctx.Stages.Add([pscustomobject]$record)
        Write-Step $Ctx "Skipping $Stage; $($record.skipped)" 'warn'
        return $null
    }
    # Resume and fork load native task-captured models. Supplying a refreshed
    # profile cannot replace that immutable snapshot; preserve native admission.
    $record.profile = $Config
    Write-Step $Ctx "$Stage :: $Title" 'phase'
    $before = Get-WorkspaceManifest $Ctx.Workspace
    return Invoke-PaidScenarioDispatch $Ctx $budget.Additional {
        $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label ($Arguments[0..1] -join '-') -Config $Config -Arguments $Arguments `
            -TimeoutSeconds ($Ctx.DeadlineSeconds + 300) -Live
        Complete-VcpStageEvidence -Ctx $Ctx -Stage $Stage -Run $run -Before $before -Record $record
    }
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
    for ($attempt = 1; $attempt -le $Ctx.MaxRepairTurns + 1; $attempt++) {
        $failed = Get-FailedGates $Ctx $current
        if ($Ctx.SkipPaidStages) { return $current }
        # A model cannot repair a missing canonical evidence sweep. Preserve its
        # original failure and full accounting hold for metadata-only recovery,
        # including failures discovered after an otherwise successful repair.
        $inspectionFailures = @($failed | Where-Object { $_.id -like 'inspect-*' })
        if ($Ctx.CostUnknown -or $inspectionFailures.Count -gt 0) {
            throw "Stage $current has incomplete canonical inspection or accounting; no paid application repair or dependent stage was started. Inspect retained evidence and reconcile accounting first."
        }
        if ($failed.Count -eq 0) { return $current }
        if ($attempt -gt $Ctx.MaxRepairTurns) {
            throw "Stage $current still has required failures after $($Ctx.MaxRepairTurns) allowed repair turns; dependent stages were not started."
        }
        $repairStage = "$Stage-repair$attempt"
        if ($Ctx.PaidExecutionBlock) {
            $skippedRepair = [ordered]@{ stage = $repairStage; title = "Repair failures from $current"; kind = 'run'; skipped = $null }
            [void](Test-PaidExecutionAdmission $Ctx $skippedRepair)
            throw "Stage $current has unresolved required gates and a paid-execution stopping condition; dependent stages were not started."
        }
        $originalPromptPath = Join-Path $Ctx.Logs "$Stage/prompt.md"
        if (-not (Test-Path -LiteralPath $originalPromptPath -PathType Leaf)) { throw "Cannot repair $Stage without its original task instructions: $originalPromptPath" }
        if ((Get-Item -LiteralPath $originalPromptPath).Length -gt 65536) { throw "Original task instructions exceed the 64 KiB repair context limit: $originalPromptPath" }
        $originalPrompt = [IO.File]::ReadAllText($originalPromptPath)
        $observed = $Ctx.Stages | Where-Object stage -eq $current | Select-Object -Last 1
        $changed = @($observed.workspace_diff.Added) + @($observed.workspace_diff.Modified) + @($observed.workspace_diff.Removed)
        $changedSummary = (@($changed | Where-Object { $_ } | Select-Object -First 80) -join ', ')
        if (-not $changedSummary) { $changedSummary = '(none recorded)' }
        $evidence = ($failed | ForEach-Object { "- [$($_.id)] $($_.description)`n  Failure: $($_.detail)" }) -join "`n"
        if ($evidence.Length -gt 12000) { $evidence = $evidence.Substring(0, 12000) + "`n... (truncated)" }
        $prompt = @"
# Repair request ($repairStage)

## Original task, environment and protected-file constraints

$originalPrompt

## Independent verification failures

An independent verification harness checked the work from stage $current and
these required checks failed. The checks are authoritative acceptance tests.

$evidence

Authored files changed during the preceding attempt: $changedSummary
Tool errors or a completion summary do not establish that a file was written.
Read back the required source files and inspect the actual test counts before claiming completion.

Fix the underlying causes in the project. Do not weaken, skip, or delete tests,
and do not edit files that the task describes as protected. Keep all previously
working behavior intact. Run the relevant build and test commands available to
you before finishing, then reply with a short summary of the root causes and fixes.
$Constraints
"@
        $result = Invoke-VcpTask -Ctx $Ctx -Stage $repairStage -Title "Repair failures from $current" -Prompt $prompt -Config $Config -AcceptExit @(0)
        if (-not $result) { throw "Repair $repairStage was not admitted; dependent stages were not started." }
        Test-StageExit $Ctx $result $repairStage
        & $GateScript $repairStage
        $current = $repairStage
    }
    return $current
}

function Get-DeadlineAccountingSnapshot {
    param($Bundle, $Scope, [switch]$Settled)
    Assert-That ($Bundle.schema_version -eq 1 -and $Bundle.kind -eq 'inspection_bundle' -and $Bundle.source_watermark -and $Bundle.task.state -eq 'paused') 'Expected a complete paused-task inspection bundle.'
    foreach ($field in 'workspace', 'session', 'task') {
        Assert-That ($Scope.$field -and $Bundle.task.scope.$field -eq $Scope.$field) "Reconciliation inspection has a different $field."
    }
    $agents = @($Bundle.agents)
    Assert-That ($agents.Count -eq 1 -and $agents[0].root -eq $Scope.task -and $agents[0].total -eq 0 -and @($agents[0].items).Count -eq 0 -and -not $agents[0].next_offset) 'Reconciliation requires no active agents or missing agent pages.'
    foreach ($view in 'costs', 'tools') {
        $pages = @($Bundle.views.$view)
        Assert-That ($pages.Count -gt 0 -and -not $pages[-1].next_cursor) "Incomplete reconciliation $view pages."
        foreach ($page in $pages) {
            Assert-That ($page.view -eq $view -and $page.source_watermark -eq $Bundle.source_watermark -and @($page.gaps).Count -eq 0 -and @($page.items | Where-Object visibility -ne 'available').Count -eq 0) "Untrusted or incomplete reconciliation $view evidence."
            foreach ($field in 'workspace', 'session', 'task') {
                Assert-That ($page.scope.$field -eq $Scope.$field) "Reconciliation $view page has a different $field."
            }
        }
    }
    foreach ($item in @(Get-InspectItems $Bundle.views.tools | Where-Object collection -eq 'effect')) {
        Assert-That ($item.record.scope.task -eq $Scope.task -and $item.record.state -in 'succeeded', 'failed', 'cancelled') 'A tool effect remains active or unknown; billing reconciliation cannot authorize resume.'
        foreach ($field in 'workspace', 'session') { Assert-That ($item.record.scope.$field -eq $Scope.$field) "Effect has a different $field." }
    }
    $items = @(Get-InspectItems $Bundle.views.costs)
    $ledgers = @($items | Where-Object collection -eq 'ledger')
    Assert-That ($ledgers.Count -eq 1 -and $ledgers[0].record.scope.task -eq $Scope.task) 'Expected exactly one same-task reconciliation ledger.'
    $ledger = $ledgers[0].record
    foreach ($field in 'workspace', 'session') { Assert-That ($ledger.scope.$field -eq $Scope.$field) "Ledger has a different $field." }
    foreach ($field in 'cap', 'settled', 'active', 'unresolved') { Assert-That ([string]$ledger.$field -match '^\d+$') "Invalid ledger $field." }
    Assert-That ($ledger.currency -eq 'USD' -and $ledger.overrun -is [bool] -and $ledger.overrun -eq $false -and [decimal]$ledger.active -eq 0 -and [decimal]$ledger.settled + [decimal]$ledger.unresolved -le [decimal]$ledger.cap) 'Ledger has active, overrun, or invalid liability.'
    if (-not $Settled) {
        Assert-That ([decimal]$ledger.unresolved -gt 0) 'The original stop did not retain unresolved provider billing.'
        $pending = @($items | Where-Object { $_.collection -eq 'reservation' -and $_.record.phase -eq 'reconciliation_pending' })
        $liability = [decimal]0
        foreach ($reservation in $pending) {
            $attempt = @($items | Where-Object { $_.collection -eq 'attempt' -and $_.id -eq $reservation.record.attempt })
            Assert-That ($reservation.record.scope.task -eq $Scope.task -and $attempt.Count -eq 1 -and $attempt[0].record.scope.task -eq $Scope.task -and
                $attempt[0].record.phase -eq 'reconciliation_pending' -and [string]$reservation.record.liability -match '^\d+$' -and [decimal]$reservation.record.liability -gt 0) 'Unresolved liability is not fully bound to provider attempts.'
            $liability += [decimal]$reservation.record.liability
        }
        Assert-That ($liability -eq [decimal]$ledger.unresolved) 'Pending provider receipts do not explain all unresolved liability.'
    }
    $cost = Get-VcpTaskCost $Bundle.views.costs
    if ($Settled) { Assert-That ($null -ne $cost.Usd -and [decimal]$ledger.unresolved -eq 0) 'Canonical accounting remains unresolved.' }
    return [pscustomobject]@{ Ledger = $ledger; Cost = $cost }
}

function Invoke-DeadlineCostReconciliation {
    <# Only A/B's deliberate T5 pause may resolve a billing-only exit 7 into
       same-task resume eligibility. This never launches inference or resumes. #>
    param($Ctx, $StageResult)
    $stage = [string]$StageResult.stage
    $metadataStage = "$stage-cost-reconciliation"
    $block = $Ctx.PaidExecutionBlock
    try {
        Assert-That (($Ctx.Name -eq 'a-vue-taskboard' -and $stage -eq 'T5-production') -or ($Ctx.Name -eq 'b-aspnet-inventory' -and $stage -eq 'T5-concurrency')) 'Billing recovery is limited to A/B short-deadline T5.'
        Assert-That ($StageResult.exit_code -eq 7 -and -not $StageResult.Run.TimedOut -and $StageResult.Run.InvalidLines -eq 0 -and
            $block -and $block.task -eq $StageResult.task -and $block.session -eq $StageResult.session -and
            @($block.reasons).Count -eq 1 -and $block.reasons[0] -eq 'unresolved_effect' -and @($block.approval_ids).Count -eq 0) 'Expected only the same-task unresolved-billing stop.'
        Assert-That ((Get-FailedGates $Ctx $stage).Count -eq 0 -and @($Ctx.Gates | Where-Object { $_.stage -eq $stage -and $_.id -eq 'jsonl' -and $_.outcome -eq 'pass' }).Count -eq 1) 'Original framing or mandatory inspection failed; do not poll or resume.'
        Assert-That (-not $Ctx.UnscopedCostUnknown -and @($Ctx.UnknownTaskCosts.Keys | Where-Object { $_ -ne $StageResult.task }).Count -eq 0) 'Unrelated or unscoped accounting remains unknown.'
        $scope = $StageResult.Run.Scope
        $originalPath = Join-Path $Ctx.Logs "$stage/inspection-bundle.json"
        $original = Get-Content -LiteralPath $originalPath -Raw | ConvertFrom-Json -Depth 100
        $originalState = Get-DeadlineAccountingSnapshot $original $scope
        Assert-That ([decimal]$originalState.Ledger.cap / 1000000 -eq [decimal]$Ctx.TaskBudgetUsd[$StageResult.task]) 'Original ledger cap differs from the admitted task cap.'
        $originalHashes = @($StageResult.Run.StdoutPath, $originalPath | ForEach-Object { @{ path = $_; sha256 = Get-Sha256 $_ } })
        $pollEvidence = @(); $settled = $false
        for ($poll = 1; $poll -le 3; $poll++) {
            # Credential is needed only for authenticated receipt metadata GET.
            # The native command holds the paused root lease and cannot infer.
            $run = Invoke-Vcp -Ctx $Ctx -Stage $metadataStage -Label "reconcile-$poll" -Arguments @('tasks', 'reconcile-cost', $StageResult.task) -TimeoutSeconds 1800
            $frames = @($run.Frames); $data = $run.Result.data
            Assert-That (-not $run.TimedOut -and $run.ExitCode -in 0, 7 -and $run.InvalidLines -eq 0 -and -not $run.Accepted -and
                @($frames | Where-Object type -eq 'accepted').Count -eq 0 -and @($frames | Where-Object type -eq 'result').Count -eq 1 -and
                $frames[-1].type -eq 'result' -and $run.Result.schema_version -eq 1 -and $run.Result.exit_code -eq $run.ExitCode -and $run.Result.correlation) 'Reconciliation command is unsupported or did not produce a complete metadata-only result.'
            Assert-That ($data.kind -eq 'provider_cost_reconciliation' -and $data.task -eq $StageResult.task -and $data.state -eq 'paused' -and
                $data.metadata_only -is [bool] -and $data.metadata_only -eq $true -and $data.resumed -is [bool] -and $data.resumed -eq $false) 'Invalid reconciliation receipt or unexpected task activation.'
            foreach ($field in 'workspace', 'session', 'task') {
                Assert-That ($data.scope.$field -eq $scope.$field -and $run.Result.scope.$field -eq $scope.$field -and $data.ledger.scope.$field -eq $scope.$field) "Receipt has a different $field."
            }
            foreach ($field in 'cap', 'settled', 'active', 'unresolved') { Assert-That ([string]$data.ledger.$field -match '^\d+$') "Invalid receipt ledger $field." }
            Assert-That ($data.ledger.currency -eq 'USD' -and $data.ledger.overrun -is [bool] -and $data.ledger.overrun -eq $false -and $data.ledger.cap -eq $originalState.Ledger.cap -and
                [decimal]$data.ledger.active -eq 0 -and [decimal]$data.ledger.settled + [decimal]$data.ledger.unresolved -le [decimal]$data.ledger.cap) 'Receipt ledger has active, overrun, or changed-cap liability.'
            $pollEvidence += @{ path = $run.StdoutPath; sha256 = Get-Sha256 $run.StdoutPath; exit_code = $run.ExitCode }
            if ($run.ExitCode -eq 0) {
                Assert-That ([decimal]$data.ledger.unresolved -eq 0 -and @($data.observations | Where-Object status -ne 'settled').Count -eq 0) 'Successful receipt still has unknown billing.'
                $settled = $true; break
            }
            Assert-That ([decimal]$data.ledger.unresolved -gt 0) 'Unresolved receipt has inconsistent exit status.'
            if ($poll -lt 3) { Start-Sleep -Seconds 10 }
        }
        Assert-That $settled 'Provider receipts remain unknown after three metadata polls; the full task hold remains and no resume is permitted.'
        $fresh = Invoke-Vcp -Ctx $Ctx -Stage $metadataStage -Label 'inspect-settled' -Arguments @('inspect-bundle', $StageResult.task) -TimeoutSeconds 1800 -DenyProviderCredentials
        Assert-That ($fresh.ExitCode -eq 0 -and -not $fresh.TimedOut -and $fresh.InvalidLines -eq 0 -and -not $fresh.Accepted -and
            @($fresh.Frames | Where-Object type -eq 'accepted').Count -eq 0 -and @($fresh.Frames | Where-Object type -eq 'result').Count -eq 1 -and
            $fresh.Frames[-1].type -eq 'result' -and $fresh.Result.schema_version -eq 1 -and $fresh.Result.exit_code -eq 0 -and $fresh.Result.correlation) 'Fresh canonical inspection failed; the full hold remains.'
        $freshState = Get-DeadlineAccountingSnapshot $fresh.Result.data $scope -Settled
        foreach ($field in 'cap', 'settled', 'active', 'unresolved') { Assert-That ($freshState.Ledger.$field -eq $data.ledger.$field) 'Canonical accounting differs from the reconciliation receipt.' }
        foreach ($file in $originalHashes) { Assert-That ((Get-Sha256 $file.path) -eq $file.sha256) 'Original stop evidence changed during reconciliation.' }
        $proof = @{ schema = 'vcp-scenario-deadline-reconciliation/1'; stage = $stage; scope = $scope; original_exit_code = 7
            original_evidence = $originalHashes; polls = $pollEvidence; inspection = @{ path = $fresh.StdoutPath; sha256 = Get-Sha256 $fresh.StdoutPath }
            settled_usd = $freshState.Cost.Usd; resume_same_task_only = $true; at = [DateTimeOffset]::UtcNow.ToString('o') }
        $proofPath = Join-Path $Ctx.Logs "$metadataStage/reconciliation.json"
        Write-JsonFile $proofPath $proof
        $accounting = [ordered]@{ stage = $metadataStage; budget_usd = $StageResult.budget_usd }
        Update-ScenarioCost -Ctx $Ctx -Task $StageResult.task -Cost $freshState.Cost.Usd -Record $accounting
        $StageResult | Add-Member -NotePropertyName deadline_reconciliation -NotePropertyValue @{ path = $proofPath; sha256 = Get-Sha256 $proofPath; accounting = $accounting } -Force
        # Keep the original stop record intact; only this new proof grants a
        # same-task continuation. Ordinary paid tasks/repairs remain blocked.
        $next = [ordered]@{}
        foreach ($property in $block.PSObject.Properties) { $next[$property.Name] = $property.Value }
        $next.reasons = @('durably_paused'); $next.resume_same_task = $true
        $next.reconciliation = @{ path = $proofPath; sha256 = Get-Sha256 $proofPath }
        $Ctx.PaidExecutionBlock = [pscustomobject]$next
        Write-JsonFile (Join-Path $Ctx.Results 'paid-execution.json') $Ctx.PaidExecutionBlock
        [void](Add-GateResult $Ctx $stage 'deadline-cost-reconciliation' 'original unresolved T5 stop has authoritative settled billing and only same-task resume eligibility' 'pass' $proofPath $true)
        return $proof
    }
    catch {
        [void](Add-GateResult $Ctx $stage 'deadline-cost-reconciliation' 'original unresolved T5 stop must be reconciled before same-task continuation' 'fail' $_.Exception.Message $true)
        throw
    }
}

function Test-StageExit {
    <# Gate: the VCP stage ended with an accepted exit code. #>
    param($Ctx, $StageResult, [string]$Stage, [switch]$DiagnosticUnresolvedDeadline)
    if (-not $StageResult) { return }
    if ($DiagnosticUnresolvedDeadline) {
        Assert-That ($StageResult.exit_code -eq 7 -and (($Ctx.Name -eq 'a-vue-taskboard' -and $Stage -eq 'T5-production') -or ($Ctx.Name -eq 'b-aspnet-inventory' -and $Stage -eq 'T5-concurrency'))) 'Diagnostic exit handling is limited to A/B T5 unresolved stops.'
    }
    $exitId = if ($DiagnosticUnresolvedDeadline) { 'original-unresolved-exit' } else { 'vcp-exit' }
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id $exitId -Description "vcp exit code in {$($StageResult.accepted_exit -join ',')}" -Advisory:$DiagnosticUnresolvedDeadline -Test {
            Assert-That ($StageResult.accepted_exit -contains $StageResult.exit_code) ("exit {0} conditions [{1}]; stderr: {2}" -f $StageResult.exit_code, ($StageResult.conditions -join ','), (Get-TextTail $StageResult.Run.StderrPath 15))
            $true
        })
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "jsonl" -Description 'JSONL stream has accepted and result frames and no invalid lines' -Test {
            Assert-That ($null -ne $StageResult.Run.Accepted) 'no accepted frame'
            Assert-That ($null -ne $StageResult.Run.Result) 'no final result frame'
            Assert-That ($StageResult.Run.InvalidLines -eq 0) "$($StageResult.Run.InvalidLines) invalid JSONL lines"
            $frames = @($StageResult.Run.Frames)
            Assert-That (@($frames | Where-Object type -eq 'accepted').Count -eq 1 -and
                @($frames | Where-Object type -eq 'result').Count -eq 1 -and $frames[-1].type -eq 'result') 'expected one accepted frame and one terminal result frame'
            $accepted = $StageResult.Run.Accepted
            $result = $StageResult.Run.Result
            Assert-That ($null -ne $result.exit_code -and $result.exit_code -eq $StageResult.exit_code) 'result exit code differs from process exit'
            foreach ($field in 'workspace', 'session', 'task') {
                Assert-That (-not [string]::IsNullOrWhiteSpace($accepted.scope.$field) -and $accepted.scope.$field -eq $result.scope.$field) "missing or inconsistent $field scope"
            }
            Assert-That (-not [string]::IsNullOrWhiteSpace($accepted.correlation) -and $accepted.correlation -eq $result.correlation) 'missing or inconsistent command correlation'
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
    $before = Get-WorkspaceManifest $Ctx.Workspace -IncludeGenerated
    $review = Invoke-VcpTask -Ctx $Ctx -Stage $Stage -Title 'Plan-mode review (read-only)' -Prompt $Prompt -Config $Config `
        -Autonomy 'plan' -BudgetUsd $Ctx.TurnBudgetUsd -AcceptExit @(0, 3, 4)
    if (-not $review) { return $null }
    Test-StageExit $Ctx $review $Stage
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "immutable" -Description 'plan autonomy left the workspace byte-identical' -Test {
            $diff = Compare-WorkspaceManifest $before (Get-WorkspaceManifest $Ctx.Workspace -IncludeGenerated)
            Assert-That ($diff.Changed -eq 0) ("changed: " + (($diff.Added + $diff.Modified + $diff.Removed) -join ', '))
            $true
        })
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "findings-json" -Description 'review ends with a parseable JSON findings block' -Advisory -Test {
            $text = Get-VcpStageFinalMessage -Ctx $Ctx -StageRecord $review
            Assert-That (-not [string]::IsNullOrEmpty($text)) 'final message unavailable from outputs view'
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

function Save-ScenarioProcessAuthorization {
    param($Ctx, [string]$Profile, [string]$Decision, [string[]]$ProcessNames = @(), [string[]]$Effects = @())
    if ($null -eq $Ctx.ProcessAuthorization) {
        $Ctx.ProcessAuthorization = [ordered]@{
            schema = 'vcp-scenario-process-authorization/1'
            allow_process_publish = [bool]$Ctx.AllowProcessPublish
            dry_run = [bool]$Ctx.SkipPaidStages
            scope = 'Only this scenario run: trusted execution profiles with registered processes and the execute effect. Reviews and guardrail profiles are excluded.'
            reason = 'The installed generic process tool classifies every process as read, write, execute, network, install, publish and opaque, including local builds/tests. Allowing publish permits effects beyond local validation; process arguments do not narrow that classification.'
            profiles = [Collections.Generic.List[object]]::new()
        }
    }
    if ($Profile) {
        $Ctx.ProcessAuthorization.profiles.Add([ordered]@{ profile = $Profile; decision = $Decision; processes = $ProcessNames; automatic_effects = $Effects })
    }
    Write-JsonFile (Join-Path $Ctx.Results 'process-authorization.json') $Ctx.ProcessAuthorization
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
        [switch]$Guardrail,
        [bool]$TrustWorkspace = $true
    )
    if ($DeadlineSeconds -le 0) { $DeadlineSeconds = $Ctx.DeadlineSeconds }
    if ($MaxRequests -le 0) { $MaxRequests = $Ctx.MaxRequests }
    $decision = 'unchanged: no execution-process authorization required'
    if ($Processes.Count -gt 0 -and $AutomaticEffects -contains 'execute' -and $MaximumAutonomy -ne 'plan' -and $TrustWorkspace -and -not $Guardrail) {
        if ($AutomaticEffects -contains 'publish') { $decision = 'publish already explicitly present in supplied profile effects' }
        elseif ($Ctx.AllowProcessPublish) {
            # The native generic process effect set includes publish even for
            # tests. Only explicit owner consent may expand this run's profiles.
            $AutomaticEffects = @($AutomaticEffects) + @('publish')
            $decision = 'publish added by explicit AllowProcessPublish consent for this run'
        }
        elseif ($Ctx.SkipPaidStages) { $decision = 'publish not granted; explicit consent required before Full execution' }
        else {
            Save-ScenarioProcessAuthorization $Ctx $Name 'refused: explicit AllowProcessPublish consent required' @($Processes | ForEach-Object { $_.name }) $AutomaticEffects
            throw 'Process execution requires explicit permission: this installed VCP classifies local builds/tests as including publish, network, install and opaque effects. No inference was started. Review the permission in run-cli-scenarios.ps1 or explicitly pass -AllowProcessPublish to authorize these scenario process profiles; DryRun remains available without that permission.'
        }
    }
    Save-ScenarioProcessAuthorization $Ctx $Name $decision @($Processes | ForEach-Object { $_.name }) $AutomaticEffects
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

function Update-ScenarioProviderMetadata {
    # Retained evidence identifies the owner's selection only. The installed
    # CLI constructs new evidence from its current adapter and a public GET.
    param($Ctx, [string]$SnapshotPath, [string]$CatalogPath,
        [string]$ExpectedModel = $Ctx.Model, [string]$ExpectedEndpoint = $Ctx.Endpoint)
    $prior = Get-Content -LiteralPath $SnapshotPath -Raw | ConvertFrom-Json -Depth 100
    $catalog = Get-Content -LiteralPath $CatalogPath -Raw | ConvertFrom-Json -Depth 100
    Assert-That ($ExpectedModel -and $ExpectedEndpoint -and $prior.compatibility.model -ceq $ExpectedModel -and
        $prior.compatibility.endpoint -ceq $ExpectedEndpoint) 'Retained evidence changed the selected model or endpoint'
    Assert-That ($prior.compatibility.id -clike 'openrouter-responses-adapter-contract/1/*') 'Metadata recovery requires a retained adapter selection'
    Assert-That ($prior.raw_sha256 -match '^[0-9a-fA-F]{64}$' -and
        (Get-FileHash -LiteralPath $CatalogPath -Algorithm SHA256).Hash -ieq $prior.raw_sha256) 'Retained catalog hash mismatch'
    foreach ($value in [string]$prior.compatibility.model, [string]$prior.compatibility.endpoint) {
        Assert-That ($value.Length -le 256 -and $value -cmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$' -and
            -not @($value.Split('/') | Where-Object { $_ -in '.', '..' }).Count) 'Metadata recovery requires exact model and endpoint identifiers'
    }
    Assert-That ($catalog.data.id -ceq $prior.compatibility.model -and
        @($catalog.data.endpoints | Where-Object { $_.tag -ceq $prior.compatibility.endpoint }).Count -eq 1) 'Retained catalog selection mismatch'
    $generation = Join-Path $Ctx.Root ('provider-current-' + [guid]::NewGuid().ToString('N'))
    $evidencePath = Join-Path $Ctx.Results 'provider-adapter-update.json'
    $evidence = [ordered]@{ status = 'starting'; prior_snapshot = $SnapshotPath; prior_catalog = $CatalogPath
        generation = $generation; model = $prior.compatibility.model; endpoint = $prior.compatibility.endpoint
        model_calls = 0; scope = 'Current compiled adapter and fresh public metadata; retained evidence supplies identity only.' }
    Write-JsonFile $evidencePath $evidence
    try {
        Write-Step $Ctx 'Recreating metadata for the selected model and endpoint with the installed adapter; no inference.' 'phase'
        $run = Invoke-Vcp -Ctx $Ctx -Stage 'provider' -Label 'current-adapter-metadata' -DenyProviderCredentials -TimeoutSeconds 180 -Arguments @(
            'setup', 'provider-metadata', '--model', $prior.compatibility.model, '--endpoint', $prior.compatibility.endpoint, '--output', $generation)
        $evidence.exit_code = $run.ExitCode; $evidence.stdout = $run.StdoutPath; $evidence.stderr = $run.StderrPath
        Assert-That ($run.ExitCode -eq 0 -and -not $run.TimedOut -and $run.InvalidLines -eq 0 -and
            $run.Result.data.status -eq 'created' -and $run.Result.data.model_calls -eq 0) "Current adapter metadata failed (exit $($run.ExitCode)); the installed CLI must support setup provider-metadata. $($run.Stderr)"
        $snapshotPath = Join-Path $generation 'snapshot.json'
        $catalogPath = Join-Path $generation 'endpoints.json'
        $snapshotText = Get-Content -LiteralPath $snapshotPath -Raw
        $snapshot = $snapshotText | ConvertFrom-Json -Depth 100
        Assert-That ($snapshot.compatibility.model -ceq $prior.compatibility.model -and
            $snapshot.compatibility.endpoint -ceq $prior.compatibility.endpoint) 'Current adapter metadata changed the selected model or endpoint'
        Assert-That ($snapshot.raw_sha256 -match '^[0-9a-fA-F]{64}$' -and
            (Get-FileHash -LiteralPath $catalogPath -Algorithm SHA256).Hash -ieq $snapshot.raw_sha256) 'Current adapter catalog hash mismatch'
        $validUntil = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$snapshot.valid_until)
        Assert-That ($validUntil -gt [DateTimeOffset]::UtcNow) 'Current adapter metadata is already expired'
        $evidence.status = 'created'; $evidence.valid_until = $snapshot.valid_until
        Write-JsonFile $evidencePath $evidence
        return @{ Generation = $generation; Snapshot = $snapshotPath; Catalog = $catalogPath
            SnapshotText = $snapshotText; SnapshotValidUntil = $validUntil.ToString('o'); MaxOutput = [int64]$snapshot.max_output }
    }
    catch {
        $evidence.status = 'failed'; $evidence.failure = $_.Exception.Message
        Write-JsonFile $evidencePath $evidence
        throw
    }
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
    $bundleHelp = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'inspect-bundle-help' -Arguments @('inspect-bundle', '--help') -NoGlobals
    $Ctx.SupportsInspectionBundle = $bundleHelp.ExitCode -eq 0 -and -not $bundleHelp.TimedOut
    if (-not $Ctx.SupportsInspectionBundle) { $Ctx.Notes.Add('Installed CLI has no inspect-bundle command; using individual evidence commands, which may be slow on retained history.') }
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'version' -Description 'vcp --version exits 0' -Test {
            Assert-That ($version.ExitCode -eq 0) "exit $($version.ExitCode)"; $true })
    $doctor = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'doctor' -Arguments @('doctor')
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'doctor' -Description 'vcp doctor reports healthy local paths' -Advisory -Test {
            Assert-That ($doctor.ExitCode -eq 0) "exit $($doctor.ExitCode): $($doctor.Stderr)"; $true })
    $credential = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'credential-status' -Arguments @('setup', 'credential', 'status')
    Write-JsonFile -Path (Join-Path $Ctx.Logs "$stage\credential-status.json") -Value $credential.Result
    $base = Join-Path $Ctx.Profiles ("base-setup-profile-{0}.json" -f $Ctx.RunId)
    $setup = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'setup-profile' -DenyProviderCredentials -Arguments @(
        'setup', 'profile', '--snapshot', $Ctx.Snapshot, '--catalog', $Ctx.Catalog, '--output', $base,
        '--trust-workspace', '--budget-usd', (Format-Usd $Ctx.TurnBudgetUsd), '--autonomy', 'ask', '--affected-path', $AffectedPath)
    $recoveryError = $null
    if ($setup.ExitCode -eq 2 -and $setup.Stderr -match 'dated compatibility record|adapter contract or fresh metadata window') {
        try {
            $current = Update-ScenarioProviderMetadata $Ctx $Ctx.Snapshot $Ctx.Catalog
            foreach ($key in 'Snapshot', 'Catalog', 'SnapshotText', 'SnapshotValidUntil') { $Ctx[$key] = $current[$key] }
            if ($current.MaxOutput -gt 0 -and $Ctx.OutputTokens -gt $current.MaxOutput) { $Ctx.OutputTokens = [int]$current.MaxOutput }
            $setup = Invoke-Vcp -Ctx $Ctx -Stage $stage -Label 'setup-profile-current-adapter' -DenyProviderCredentials -Arguments @(
                'setup', 'profile', '--snapshot', $Ctx.Snapshot, '--catalog', $Ctx.Catalog, '--output', $base,
                '--trust-workspace', '--budget-usd', (Format-Usd $Ctx.TurnBudgetUsd), '--autonomy', 'ask', '--affected-path', $AffectedPath)
        }
        catch { $recoveryError = $_.Exception.Message }
    }
    [void](Invoke-Gate -Ctx $Ctx -Stage $stage -Id 'setup-profile' -Description 'vcp setup profile creates a trusted base profile for this workspace' -Test {
            Assert-That (-not $recoveryError) "Provider adapter recovery failed: $recoveryError"
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

function Test-ProcessEnvironment {
    <# Exercise the configured executable/argv/environment before inference; not an isolation proof. #>
    param($Ctx, [string]$Stage, $Process, [string]$Id, [string[]]$Arguments)
    $gate = "process-environment.$Id"
    if ((Get-FailedGates $Ctx $Stage).Count) {
        return Skip-Gate $Ctx $Stage $gate 'Toolchain works with its explicit process environment' 'Earlier profile or process checks failed.'
    }
    return Invoke-Gate -Ctx $Ctx -Stage $Stage -Id $gate -Description 'Toolchain works with its explicit process environment' -Test {
        $environment = @{}
        foreach ($key in $Process.environment.Keys) { $environment[$key] = $Process.environment[$key] }
        $run = Invoke-Tool -Ctx $Ctx -Stage $Stage -Label $gate -FilePath $Process.executable -ArgumentList $Arguments `
            -Environment $environment -ClearEnvironment -TimeoutSeconds ([int][math]::Ceiling($Process.max_timeout_ms / 1000))
        Assert-That ($run.ExitCode -eq 0 -and -not $run.TimedOut) ("exit {0}; stdout: {1}; stderr: {2}" -f $run.ExitCode, (Get-TextTail $run.StdoutPath 25), (Get-TextTail $run.StderrPath 25))
        $true
    }
}

function Invoke-GuardrailRun {
    <#
    Zero-spend negative test: a run the CLI must reject before inference with
    exit 2 (invalid configuration). Asserts no task was accepted.
    #>
    param($Ctx, [string]$Stage, [string]$Id, [string]$Description, [string[]]$Arguments, [string]$Config, [string]$ExpectStderr)
    $run = Invoke-Vcp -Ctx $Ctx -Stage $Stage -Label "guardrail-$Id" -Config $Config -Arguments $Arguments -TimeoutSeconds 120 -DenyProviderCredentials
    [void](Invoke-Gate -Ctx $Ctx -Stage $Stage -Id "guardrail.$Id" -Description $Description -Test {
            Assert-That (Test-VcpPreAdmissionRejection $run) 'expected a complete unscoped invalid_configuration result with exit 2 and no task acceptance'
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
    if ($Ctx.ReuseProject) {
        $Ctx.Git = $null
        $Ctx.Notes.Add('Existing project: automatic git initialization, staging and checkpoint commits disabled to preserve user work.')
        return
    }
    $Ctx.Git = Find-Executable -Name 'git'
    if (-not $Ctx.Git) { $Ctx.Notes.Add('git not found; per-stage checkpoint commits disabled.'); return }
    [void](Invoke-Tool -Ctx $Ctx -Stage 'git' -Label 'init' -FilePath $Ctx.Git -ArgumentList @('init', '-q', '-b', 'main'))
    Save-Checkpoint $Ctx 'seed: scenario scaffold'
}

function Assert-CheckpointPhysicalPath([string]$Path) {
    $probe = [IO.Path]::GetFullPath($Path)
    while ($probe) {
        if (Test-Path -LiteralPath $probe) {
            if ((Get-Item -LiteralPath $probe -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Source checkpoint cannot traverse a link or junction: $probe"
            }
        }
        $parent = Split-Path -Parent $probe
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
}

function Save-SourceCheckpoint($Ctx, [string]$Message) {
    $workspace = [IO.Path]::GetFullPath($Ctx.Workspace).TrimEnd('\', '/')
    $checkpoints = [IO.Path]::GetFullPath((Join-Path $Ctx.Root 'checkpoints'))
    if ($checkpoints -ieq $workspace -or $checkpoints.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Source checkpoints must be outside the authored workspace.'
    }
    Assert-CheckpointPhysicalPath $workspace
    Assert-CheckpointPhysicalPath $checkpoints
    $before = Get-WorkspaceManifest $workspace -ForCheckpoint
    foreach ($relative in $before.Keys) {
        if ([string]$before[$relative] -notmatch '^[0-9a-f]{64}$') { throw "Source checkpoint contains a link or invalid hash: $relative" }
    }
    $directory = Join-Path $checkpoints ([guid]::NewGuid().ToString('N'))
    if (Test-Path -LiteralPath $directory) { throw 'Source checkpoint directory already exists.' }
    $filesRoot = Join-Path $directory 'files'
    [void][IO.Directory]::CreateDirectory($filesRoot)
    foreach ($relative in $before.Keys) {
        $source = [IO.Path]::GetFullPath((Join-Path $workspace $relative))
        $target = [IO.Path]::GetFullPath((Join-Path $filesRoot $relative))
        if (-not $source.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            -not $target.StartsWith($filesRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Source checkpoint path escapes its source or destination.' }
        Assert-CheckpointPhysicalPath $source
        Assert-CheckpointPhysicalPath $target
        [void][IO.Directory]::CreateDirectory((Split-Path -Parent $target))
        [IO.File]::Copy($source, $target, $false)
        Assert-CheckpointPhysicalPath $source
        Assert-CheckpointPhysicalPath $target
        if ((Get-Sha256 $target) -ne $before[$relative]) { throw "Source checkpoint copy differs from the captured source: $relative" }
    }
    $after = Get-WorkspaceManifest $workspace -ForCheckpoint
    $copied = Get-WorkspaceManifest $filesRoot -ForCheckpoint
    Assert-CheckpointPhysicalPath $workspace
    Assert-CheckpointPhysicalPath $filesRoot
    if ((Compare-WorkspaceManifest $before $after).Changed -ne 0 -or (Compare-WorkspaceManifest $before $copied).Changed -ne 0) {
        throw 'Authored source changed during checkpoint capture; no completed checkpoint was published.'
    }
    # Publish the completion manifest last. A failed capture has no manifest and
    # cannot be mistaken for a restorable checkpoint. Never overwrite a snapshot.
    $manifest = @{ schema = 'vcp-source-checkpoint/1'; message = $Message; at = [DateTimeOffset]::UtcNow.ToString('o')
        workspace = $workspace; files = $before; file_count = $before.Count
        exclusions = @{ directory_names = @($script:ManifestExclusions | Where-Object { $_ -notin 'models', 'models-repro', 'reports' })
            root_only_directory_names = @('models', 'models-repro', 'reports'); directory_suffixes = @('.egg-info'); links = 'rejected' } }
    $path = Join-Path $directory 'manifest.json'
    $pending = Join-Path $directory 'manifest.pending.json'
    $stream = [IO.File]::Open($pending, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    try {
        $bytes = $script:Utf8NoBom.GetBytes(($manifest | ConvertTo-Json -Depth 10))
        $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
    }
    finally { $stream.Dispose() }
    [IO.File]::Move($pending, $path, $false)
    Write-Step $Ctx "Source checkpoint saved: $path"
}

function Save-Checkpoint {
    param($Ctx, [string]$Message)
    Save-SourceCheckpoint $Ctx $Message
    if ($Ctx.ReuseProject -or -not $Ctx.Git) { return }
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
    if ($Ctx.PaidExecutionBlock) {
        $diagnostic = if ($Ctx.PaidExecutionBlock.task_reason) { "; VCP reason: $($Ctx.PaidExecutionBlock.task_reason)" } else { '' }
        [void](Add-GateResult $Ctx 'FINAL-execution' 'paid-execution-stopped' 'paid execution has no unresolved stopping condition' 'fail' `
            ("$($Ctx.PaidExecutionBlock.reasons -join ', '); task $($Ctx.PaidExecutionBlock.task)$diagnostic; inspect $($Ctx.PaidExecutionBlock.inspection)") $true)
    }
    if ($Ctx.AccountedTaskUsd.Count -gt 0 -or $Ctx.UnscopedCostUnknown) {
        [void](Invoke-Gate -Ctx $Ctx -Stage 'FINAL-accounting' -Id 'cost-evidence' -Description 'latest accounting for every paid task is complete and within budget' -Test {
                Assert-That (-not $Ctx.CostUnknown) 'Unsettled task accounting or an execution without task scope remains.'
                Assert-That ($Ctx.SpentUsd -le $Ctx.MaxScenarioUsd) 'Observed scenario spend exceeds the configured ceiling.'
                foreach ($task in $Ctx.TaskBudgetUsd.Keys) {
                    Assert-That ([decimal]$Ctx.AccountedTaskUsd[$task] -le [decimal]$Ctx.TaskBudgetUsd[$task]) "Task $task exceeded its admitted cap."
                }
                $true
            })
    }
    # Latest state: a repair stage's result for the same gate supersedes the
    # original stage's result. First-pass statistics keep the original misses.
    $required = @($Ctx.Gates | Where-Object { $_.required } |
            Group-Object { '{0}|{1}' -f ($_.stage -replace '-repair\d+$', ''), $_.id } |
            ForEach-Object { $_.Group[-1] })
    $passed = @($required | Where-Object { $_.outcome -eq 'pass' })
    $failed = @($required | Where-Object { $_.outcome -eq 'fail' })
    $advisoryFailed = @($Ctx.Gates | Where-Object { -not $_.required -and $_.outcome -eq 'fail' })
    $skippedStages = @($Ctx.Stages | Where-Object { $_.skipped })
    $verdict = if ($Ctx.Fatal -or $failed.Count -gt 0 -or $required.Count -eq 0) { 'fail' }
        elseif ($Ctx.SkipPaidStages) { 'dry-run-pass' }
        elseif ($skippedStages.Count -gt 0) { 'incomplete' }
        else { 'pass' }
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
        workspace                = $Ctx.Workspace
        reused_project           = [bool]$Ctx.ReuseProject
        model                    = $Ctx.Model
        endpoint                 = $Ctx.Endpoint
        snapshot_valid_until     = $Ctx.SnapshotValidUntil
        spend_usd                = [math]::Round($Ctx.SpentUsd, 6)
        spend_evidence_complete  = -not $Ctx.CostUnknown
        max_scenario_usd         = $Ctx.MaxScenarioUsd
        dry_run                  = [bool]$Ctx.SkipPaidStages
        skipped_stages           = $skippedStages.Count
        paid_execution_block     = $Ctx.PaidExecutionBlock
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
        verdict                  = $verdict
        stages                   = @($Ctx.Stages | ForEach-Object { $_ | Select-Object * -ExcludeProperty Run, workspace_diff })
        gates                    = $Ctx.Gates
        assets                   = $Ctx.Assets
        notes                    = $Ctx.Notes
    }
    Write-JsonFile -Path (Join-Path $Ctx.Results 'scorecard.json') -Value $scorecard

    $md = [System.Text.StringBuilder]::new()
    [void]$md.AppendLine("# VCP practical scenario: $($Ctx.Name)")
    [void]$md.AppendLine()
    [void]$md.AppendLine("Project: ``$($Ctx.Workspace)`` (existing files reused: $([bool]$Ctx.ReuseProject)).")
    [void]$md.AppendLine()
    $costSummary = if ($Ctx.CostUnknown) {
        "budget reservation $(Format-Usd $Ctx.SpentUsd) USD (actual spend unresolved)"
    } else {
        "spend $(Format-Usd $Ctx.SpentUsd) USD"
    }
    [void]$md.AppendLine("Run ``$($Ctx.RunId)`` - verdict **$($scorecard.verdict.ToUpperInvariant())** - $($passed.Count)/$($required.Count) required gates - $costSummary - $($scorecard.wall_minutes) min")
    [void]$md.AppendLine()
    [void]$md.AppendLine("VCP ``$($Ctx.VcpVersion)``, model ``$($Ctx.Model)`` endpoint ``$($Ctx.Endpoint)``. First-pass feature turns: $($firstPass.Count)/$($primary.Count); repair turns: $($repairs.Count).")
    $commandLogPath = if ($Ctx.CommandLog) { $Ctx.CommandLog } else { Join-Path $Ctx.Logs 'vcp-commands.log' }
    $auditLogPath = if ($Ctx.CommandAuditLog) { $Ctx.CommandAuditLog } else { Join-Path (Split-Path -Parent $commandLogPath) 'vcp-commands.jsonl' }
    $commandLogLink = [IO.Path]::GetRelativePath($Ctx.Results, $commandLogPath).Replace('\', '/')
    $auditLogLink = [IO.Path]::GetRelativePath($Ctx.Results, $auditLogPath).Replace('\', '/')
    [void]$md.AppendLine()
    [void]$md.AppendLine("Command execution evidence: [literal VCP commands and result summaries](<$commandLogLink>) and [structured argv/results](<$auditLogLink>). Each command entry points to its stdout, stderr, workspace and final result targets.")
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
    Write-Step $Ctx ("Scenario {0}: {1} ({2}/{3} required gates, {4}). Results: {5}" -f $Ctx.Name, $scorecard.verdict.ToUpperInvariant(), $passed.Count, $required.Count, $costSummary, $Ctx.Results) $(if ($scorecard.verdict -in 'pass', 'dry-run-pass') { 'ok' } else { 'fail' })
    if ($Ctx.Transcript) { try { Stop-Transcript | Out-Null } catch { } }
    return $(if ($scorecard.verdict -in 'pass', 'dry-run-pass') { 0 } else { 1 })
}

#endregion

Export-ModuleMember -Function @(
    'Write-Utf8File', 'Write-JsonFile', 'Write-SeedFiles', 'Get-TextTail', 'Get-Sha256', 'Assert-That', 'ConvertTo-Instant', 'Format-Usd',
    'ConvertFrom-JsonLines', 'Write-Step', 'Find-Executable', 'Invoke-NativeLogged', 'Invoke-Tool',
    'Start-BackgroundServer', 'Stop-BackgroundServer', 'Invoke-Http', 'Initialize-VcpScenario', 'Add-GateResult',
    'Invoke-Gate', 'Skip-Gate', 'Get-FailedGates', 'Invoke-Vcp', 'Invoke-VcpInspect', 'Get-InspectItems',
    'Get-VcpTaskCost', 'Get-VcpFinalMessage', 'Get-VcpStageFinalMessage', 'Get-CompletedTurnIds', 'Get-WorkspaceManifest',
    'Compare-WorkspaceManifest', 'Invoke-VcpTask', 'Invoke-VcpContinuation', 'Invoke-RepairLoop', 'Test-StageExit', 'Invoke-DeadlineCostReconciliation',
    'Invoke-PlanModeReview', 'New-ProcessProfile', 'New-ScenarioProfile', 'Invoke-CommonPreflight',
    'Update-ScenarioProviderMetadata', 'Test-ProfileCheck', 'Test-ProcessEnvironment', 'Invoke-GuardrailRun', 'Invoke-WorkspaceDiscover', 'Invoke-FinalEvidenceSweep',
    'Initialize-GitCheckpoint', 'Save-Checkpoint', 'Add-Asset', 'Complete-VcpScenario'
)
