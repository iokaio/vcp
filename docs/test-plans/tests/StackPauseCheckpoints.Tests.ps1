#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
# Isolated T5 control flow; -RunStackHelpers also exercises installed Java/Python.
# The CI scenario batch provisions Node only, so native stack smoke is explicit.
param([switch]$RunStackHelpers)
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$root = Join-Path ([IO.Path]::GetTempPath()) ('vcp-stack-pause-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$checks = 0
function Check($Value, $Message) { if (-not $Value) { throw $Message }; $script:checks++ }
try {
    foreach ($runtime in $(if ($RunStackHelpers) { @('java', 'python') } else { @() })) {
        $ctx = @{ Workspace = $root; Logs = $root }
        $instruction = New-ScenarioPauseCheckpoint $ctx -Profile $runtime -Runtime $runtime
        Check ($instruction.Contains('"profile":"' + $runtime + '"')) "$runtime checkpoint chose another process profile"
        $executable = Find-Executable -Name $runtime
        Check ([bool]$executable) "$runtime is required for the real checkpoint helper smoke test"
        $start = [Diagnostics.ProcessStartInfo]::new($executable)
        $start.WorkingDirectory = $root
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $start.ArgumentList.Add((Split-Path -Leaf $ctx.PauseCheckpoint.script))
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {
            [void]$process.Start()
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            $until = [DateTimeOffset]::UtcNow.AddSeconds(20)
            while (-not (Test-Path -LiteralPath $ctx.PauseCheckpoint.marker) -and -not $process.HasExited -and [DateTimeOffset]::UtcNow -lt $until) { Start-Sleep -Milliseconds 50 }
            Check (-not $process.HasExited -and (Test-Path -LiteralPath $ctx.PauseCheckpoint.marker)) "$runtime did not reach its live process checkpoint"
            # The child writes then closes the marker before waiting. Allow the small write window.
            $marker = $null
            while (-not $marker -and [DateTimeOffset]::UtcNow -lt $until) {
                try { $marker = Get-Content -LiteralPath $ctx.PauseCheckpoint.marker -Raw | ConvertFrom-Json } catch { Start-Sleep -Milliseconds 20 }
            }
            Check ($marker.version -eq 1 -and $marker.token -ceq $ctx.PauseCheckpoint.token -and $marker.pid -eq $process.Id) "$runtime checkpoint marker did not identify the launched child"
            Check ((Get-Sha256 $ctx.PauseCheckpoint.script) -ceq $ctx.PauseCheckpoint.sha256) "$runtime changed its protected checkpoint script"
        }
        finally {
            # Stop only the exact child launched by this fixture, never an existing scenario process.
            if ($process.Id -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
            $process.Dispose()
        }
        $resumed = Invoke-NativeLogged -FilePath $executable -ArgumentList @((Split-Path -Leaf $ctx.PauseCheckpoint.script)) -WorkingDirectory $root `
            -StdoutPath (Join-Path $root "$runtime-resume.out") -StderrPath (Join-Path $root "$runtime-resume.err") -TimeoutSeconds 20
        Check ($resumed.ExitCode -eq 0 -and (Get-Content (Join-Path $root "$runtime-resume.out") -Raw).Contains('Checkpoint already reached')) "$runtime did not continue from its existing checkpoint"
        Write-Utf8File $ctx.PauseCheckpoint.marker '{"version":1,"token":"foreign","pid":1}'
        $invalid = Invoke-NativeLogged -FilePath $executable -ArgumentList @((Split-Path -Leaf $ctx.PauseCheckpoint.script)) -WorkingDirectory $root `
            -StdoutPath (Join-Path $root "$runtime-invalid.out") -StderrPath (Join-Path $root "$runtime-invalid.err") -TimeoutSeconds 20
        Check ($invalid.ExitCode -ne 0) "$runtime accepted another checkpoint's marker"
    }

    # Run each scenario's actual T5 block against explicit control outcomes.
    foreach ($scenario in 'c-java-ledger-cli', 'd-python-textlab') {
        $source = Get-Content -LiteralPath (Join-Path $scenarioRoot "scenario-$scenario.ps1") -Raw
        $tokens = $null; $errors = $null
        [void][Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
        Check ($errors.Count -eq 0) "$scenario failed PowerShell parsing"
        $from = $source.IndexOf('    # --- T5:')
        $to = $source.IndexOf('    # --- T6', $from)
        Check ($from -ge 0 -and $to -gt $from) "$scenario T5 block missing"
        $block = [scriptblock]::Create($source.Substring($from, $to - $from))
        foreach ($case in 'success', 'unacknowledged', 'not-paused', 'fenced', 'foreign-resume', 'missing-discovery', 'stale-accepted') {
            if ($scenario.StartsWith('c-') -and $case -in 'missing-discovery', 'stale-accepted') { continue }
            & {
                $ctx = @{ PaidExecutionBlock = @{ resume_same_task = $case -ne 'fenced' }; PauseCheckpoint = @{script='checkpoint';sha256='digest'} }
                $protected = @{}; $promptT5 = 'owner task'; $profileMain = 'full'; $profiles = @{T5='full'}
                $failed = [Collections.Generic.List[string]]::new()
                $observed = @{ resumed = 0; gates = 0 }
                function New-ScenarioPauseCheckpoint { param($Ctx, $Profile, $Runtime) "checkpoint-$Runtime" }
                function Save-Checkpoint { }
                function Test-StageExit { }
                function Test-MavenVerify { }; function Test-ExportAndRange { }; function Test-BankExport { }
                function Test-ProtectedUnchanged { }; function Test-Pytest { }; function Test-Model { }; function Test-Report { }
                function Invoke-RepairLoop { }
                function Get-FailedGates { $failed.ToArray() }
                function Invoke-Gate {
                    param($Ctx, $Stage, $Id, $Description, $Test)
                    try { [void](& $Test); $observed.gates++ } catch { $failed.Add($Id) }
                }
                function Invoke-VcpTask {
                    param($Ctx, $Stage, $Title, $Prompt, $Config, [switch]$PauseAfterProgress, $AcceptExit)
                    Check ($PauseAfterProgress -and ($AcceptExit -join ',') -eq '8' -and $Config -eq 'full' -and $Prompt.StartsWith('checkpoint-')) 'T5 did not require explicit checkpoint pause with the normal profile'
                    @{ task='task'; session='session'; exit_code=8; explicit_pause=@{acknowledged=$case -ne 'unacknowledged'}; conditions=$(if ($case -eq 'not-paused') { @() } else { @('durably_paused') }) }
                }
                function Invoke-WorkspaceDiscover {
                    @{Result=@{data=@{candidates=$(if ($case -eq 'missing-discovery') { @() } else { @(@{task='task';expected_revision=4}) })}}}
                }
                function Invoke-Vcp {
                    param($Ctx, $Stage, $Label, $Config, $Arguments)
                    Check (($Arguments -join ' ') -eq 'resume task --expected-revision 0') 'Stale-revision probe used unexpected task or revision'
                    $errorPath = Join-Path $root 'stale.err'
                    Write-Utf8File $errorPath 'selection revision mismatch'
                    @{ExitCode=$(if ($case -eq 'stale-accepted') {0} else {2}); StderrPath=$errorPath; Frames=@()}
                }
                function Invoke-VcpContinuation {
                    param($Ctx, $Stage, $Title, $Arguments, $Config, $AcceptExit)
                    $observed.resumed++
                    $expected = if ($scenario.StartsWith('c-')) {'sessions resume session'} else {'resume task --expected-revision 4'}
                    Check (($Arguments -join ' ') -eq $expected -and $Config -eq 'full') 'Resume lost exact task/session selection or full profile'
                    @{task=$(if ($case -eq 'foreign-resume') {'foreign'} else {'task'})}
                }
                $caught = $false
                try { . $block } catch { $caught = $true }
                if ($case -eq 'success') { Check (-not $caught -and -not $failed.Count -and $observed.resumed -eq 1) "$scenario success did not resume exactly once" }
                elseif ($case -eq 'foreign-resume') { Check ($failed.Contains('resume-same-task')) "$scenario accepted a different resumed task" }
                else { Check ($caught -and $observed.resumed -eq 0) "$scenario $case continued without qualified pause/selection evidence" }
            }
        }
    }
}
finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $prefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolved) -notlike 'vcp-stack-pause-*') { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Host "Stack checkpoint and C/D explicit pause checks passed: $checks"
if (-not $RunStackHelpers) { Write-Host 'Native Java/Python checkpoint helpers not run; use -RunStackHelpers with those runtimes installed.' }
