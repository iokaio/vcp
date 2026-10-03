#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<# Offline command-audit regressions: no VCP, providers, network or spend. #>
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -PassThru -DisableNameChecking
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$testRoot = Join-Path $tempBase ('vcp-command-log-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
try {
    $logs = Join-Path $testRoot "log's directory"
    $ctx = @{
        Logs = $logs; CommandLog = Join-Path $logs 'vcp-commands.log'
        Vcp = Join-Path $testRoot "executable's directory/vcp.exe"
        Workspace = $testRoot; Data = Join-Path $testRoot 'data'; Results = Join-Path $testRoot 'results'
        SkipPaidStages = $false
    }
    & $module {
        $script:CommandTestMode = 'nonzero'
        $script:CommandTestArgs = @()
        function script:Invoke-NativeLogged {
            param($FilePath, $ArgumentList, $WorkingDirectory, $StdoutPath, $StderrPath, $TimeoutSeconds, $OnLine, $HeartbeatLabel, $Ctx, $Environment)
            $startedLog = Get-Content -LiteralPath $Ctx.CommandLog -Raw
            if ($startedLog -notmatch 'START id=' -or $startedLog -notmatch 'command: & ') { throw 'START/command must be durable before launch' }
            $script:CommandTestArgs = @($ArgumentList)
            if ($script:CommandTestMode -eq 'launch-failure') { throw [ComponentModel.Win32Exception]::new('fixture-sensitive-error-must-not-be-logged') }
            $output = @(
                '{"type":"accepted","scope":{"task":"task-1","session":"session-1"}}',
                '{"type":"result","conditions":{"budget_exceeded":true,"completed":false}}'
            ) -join "`n"
            if ($script:CommandTestMode -eq 'text') { $output = 'vcp 1.2.3' }
            Write-Utf8File $StdoutPath $output
            Write-Utf8File $StderrPath 'fixture stderr'
            $exitCode = switch ($script:CommandTestMode) { 'success' { 0 }; 'text' { 0 }; 'timeout' { -1 }; default { 3 } }
            return [pscustomobject]@{ ExitCode = $exitCode; TimedOut = $script:CommandTestMode -eq 'timeout'; DurationSeconds = 1.25 }
        }
    }

    $arguments = @('run', 'two words', "O'Brien", 'double"quote', '$literal; $(no-execution)', '`literal', '', "line1`nline2", 'C:\tail\')
    $run = Invoke-Vcp -Ctx $ctx -Stage 'T1' -Label 'quoted arguments' -Arguments $arguments -NoGlobals
    $auditPath = Join-Path $logs 'vcp-commands.jsonl'
    $records = @(Get-Content -LiteralPath $auditPath | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    Check ($records.Count -eq 1) 'Expected one JSONL completion per invocation'
    $record = $records[0]
    Check ($record.schema_version -eq 1 -and $record.status -eq 'nonzero_exit' -and $record.exit_code -eq 3) 'Nonzero exit result was not preserved'
    Check ($record.executable -ceq $ctx.Vcp) 'Executable was replaced with a generic vcp name'
    Check (($record.argv | ConvertTo-Json -Compress) -ceq ($arguments | ConvertTo-Json -Compress)) 'Exact argv was not preserved'
    $actualArguments = & $module { , $script:CommandTestArgs }
    Check (($actualArguments | ConvertTo-Json -Compress) -ceq ($arguments | ConvertTo-Json -Compress)) 'Logged argv differs from dispatched argv'
    Check ($record.command.StartsWith("& '" + $ctx.Vcp.Replace("'", "''") + "' ")) 'Executable is not a PowerShell single-quoted literal'
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseInput($record.command, [ref]$tokens, [ref]$errors)
    Check ($errors.Count -eq 0) 'Replay command does not parse'
    $commandAst = $ast.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] }, $true)
    $literals = @($commandAst.CommandElements | ForEach-Object Value)
    Check (($literals | ConvertTo-Json -Compress) -ceq (@($ctx.Vcp) + $arguments | ConvertTo-Json -Compress)) 'Replay command does not decode to the original executable and arguments'
    Check (@($commandAst.CommandElements | Where-Object { $_ -isnot [Management.Automation.Language.StringConstantExpressionAst] -or $_.StringConstantType -ne 'SingleQuoted' }).Count -eq 0) 'Replay command contains expandable arguments'
    Check ($record.accepted -and $record.task -eq 'task-1' -and $record.session -eq 'session-1') 'Accepted scope was not summarized'
    Check (($record.conditions -join ',') -eq 'budget_exceeded') 'Enabled condition names were not summarized'
    Check ($record.duration_seconds -eq 1.25 -and -not $record.timed_out) 'Duration/timeout metadata was not preserved'
    foreach ($field in 'stdout_path', 'stderr_path', 'workspace', 'results_directory', 'summary_path', 'scorecard_path') {
        Check ([IO.Path]::IsPathFullyQualified($record.$field)) "$field is not an absolute reference"
    }
    Check ($record.stdout_exists -and $record.stderr_exists -and (Test-Path -LiteralPath $record.stdout_path) -and (Test-Path -LiteralPath $record.stderr_path)) 'Captured output references do not exist'
    Check (-not (Test-Path -LiteralPath $record.scorecard_path)) 'Test fixture unexpectedly created a final scorecard'
    $readable = Get-Content -LiteralPath $ctx.CommandLog -Raw
    Check ($readable.Contains($record.command) -and $readable.Contains('exit=3') -and $readable.Contains($record.stdout_path) -and $readable.Contains($record.results_directory)) 'Readable command/result/evidence log is incomplete'
    Check ($readable.Contains('written on scenario completion')) 'Readable log incorrectly implies final evidence already exists'

    & $module { $script:CommandTestMode = 'success' }
    [void](Invoke-Vcp -Ctx $ctx -Stage 'T2' -Label 'globals' -Arguments @('version') -Config (Join-Path $testRoot "profile's file.json"))
    $record = (Get-Content -LiteralPath $auditPath | Select-Object -Last 1) | ConvertFrom-Json
    Check ($record.status -eq 'succeeded' -and $record.exit_code -eq 0) 'Successful command result missing'
    Check (($record.argv[0..3] -join ',') -eq '--format,jsonl,--non-interactive,--workspace' -and $record.argv -contains $ctx.Workspace -and $record.argv -contains '--config') 'Global arguments were omitted from command audit'

    & $module { $script:CommandTestMode = 'timeout' }
    [void](Invoke-Vcp -Ctx $ctx -Stage 'T3' -Label 'timeout' -Arguments @('version') -NoGlobals)
    $record = (Get-Content -LiteralPath $auditPath | Select-Object -Last 1) | ConvertFrom-Json
    Check ($record.status -eq 'timed_out' -and $record.timed_out -and $record.exit_code -eq -1) 'Timeout result missing'

    & $module { $script:CommandTestMode = 'launch-failure' }
    $threw = $false
    try { [void](Invoke-Vcp -Ctx $ctx -Stage 'T4' -Label 'launch failure' -Arguments @('version') -NoGlobals) } catch { $threw = $true }
    Check $threw 'Launch failure must be rethrown'
    $records = @(Get-Content -LiteralPath $auditPath | ForEach-Object { $_ | ConvertFrom-Json })
    Check ($records.Count -eq 4) 'Launch failure did not produce exactly one completion record'
    $record = $records[-1]
    Check ($record.status -eq 'invocation_failed' -and $null -eq $record.exit_code -and $record.error_type -match 'Win32Exception') 'Launch failure was misreported as a process exit'
    Check (-not $record.stdout_exists -and -not $record.stderr_exists) 'Launch failure claimed missing output files exist'
    Check ($record.summary_path -eq (Join-Path $ctx.Results 'summary.md') -and $record.scorecard_path -eq (Join-Path $ctx.Results 'scorecard.json')) 'Launch failure omitted final evidence targets'
    $readable = Get-Content -LiteralPath $ctx.CommandLog -Raw
    Check ($readable.Contains('END invocation_failed') -and $readable.Contains('exit=unavailable')) 'Launch failure missing from readable log'
    Check (-not $readable.Contains('fixture-sensitive-error') -and -not (Get-Content -LiteralPath $auditPath -Raw).Contains('fixture-sensitive-error')) 'Exception details leaked into command audit'
    & $module { $script:CommandTestMode = 'text' }
    [void](Invoke-Vcp -Ctx $ctx -Stage 'T5' -Label 'version' -Arguments @('--version') -NoGlobals)
    $record = (Get-Content -LiteralPath $auditPath | Select-Object -Last 1) | ConvertFrom-Json
    Check ($record.status -eq 'succeeded' -and $record.output_format -eq 'text' -and $null -eq $record.invalid_jsonl_lines) 'Plain-text --version was summarized as invalid JSONL'
    Write-Host "Command log regressions passed ($checks checks)."
}
finally {
    Remove-Module $module -Force
    $resolved = [IO.Path]::GetFullPath($testRoot)
    if (-not $resolved.StartsWith($tempBase.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup target' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
