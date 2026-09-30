# SPDX-License-Identifier: Apache-2.0
# Invoked by the ignored Rust qualification test inside a no-breakaway Job Object.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageResult,
    [Parameter(Mandatory)][string]$InstalledExecutable,
    [Parameter(Mandatory)][string]$FixtureInput,
    [Parameter(Mandatory)][string]$QualificationExecutable,
    [Parameter(Mandatory)][string]$NodeExecutable,
    [Parameter(Mandatory)][string]$OutputRoot
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'production-package.ps1')
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Save([string]$Path,$Value) { $Value | ConvertTo-Json -Depth 50 | Set-Content -LiteralPath $Path -Encoding utf8 }
$root=[IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $root) { throw 'Fresh startup130 output directory required' }
New-Item -ItemType Directory -Path $root | Out-Null
$report=[ordered]@{schema='vcp-production-startup130-observations/1';status='running';started_at=[DateTime]::UtcNow.ToString('o');
    runner_sha256=(Hash $PSCommandPath);fixture_input_sha256=(Hash $FixtureInput);qualification_executable_sha256=(Hash $QualificationExecutable);deadline_seconds=300;
    drain_deadline_seconds=10;cleanup_deadline_seconds=10;resources=@();
    limitations=@('Synthetic governed 130-version history, current host, uncontrolled OS cache. No provider/model calls or real paid task.',
        'One sample per backend/operation. No population quantiles or hardware floor.',
        'Native CPU and peak working sets are 100ms samples of observed bridge/server PIDs, not complete process-tree totals; short-lived processes may have zero samples.',
        'Rust qualification wrapper separately verifies canonical state, paused accounting and kernel-confirmed descendant termination. This observation receipt alone cannot establish those checks.')}
$resourceRows=@{}; $candidate=$null; $process=$null; $started=$false; $outCopy=$null; $errCopy=$null; $cancellation=$null
$outFile=$null; $errFile=$null
try {
    $candidate=Read-ProductionPackage -PackageResult $PackageResult -ExtractionRoot (Join-Path $root 'package') -SelectedExecutable $InstalledExecutable -NodeExecutable $NodeExecutable
    $report.candidate=$candidate
    $fixture=Get-Content -LiteralPath $FixtureInput -Raw | ConvertFrom-Json -Depth 50
    if ($fixture.versions.Count -ne 130 -or $fixture.backend -cnotin @('Files','Sqlite')) { throw 'Synthetic 130-version backend fixture required' }
    $report.backend=$fixture.backend
    $node=(Resolve-Path -LiteralPath $NodeExecutable).Path
    $driver=Join-Path $PSScriptRoot 'production-startup130.mjs'
    $report.node_sha256=Hash $node; $report.driver_sha256=Hash $driver
    $sdk=Join-Path $PSScriptRoot '../../src/packages/sdk-ts/dist'
    $report.sdk=@(Get-ChildItem -LiteralPath $sdk -File -Recurse | Sort-Object FullName | ForEach-Object { @{path=[IO.Path]::GetRelativePath($sdk,$_.FullName);sha256=(Hash $_.FullName)} })
    $report.fixture_generator=@('local_inspector_queries.rs','support/local_fixture.rs') | ForEach-Object { @{path=$_;sha256=(Hash (Join-Path $PSScriptRoot "../../src/crates/vcp-cli/tests/$_"))} }
    $report.host=@{os=[Environment]::OSVersion.VersionString;processors=[Environment]::ProcessorCount;architecture=$env:PROCESSOR_ARCHITECTURE;cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);physical_memory_bytes=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory}
    $events=Join-Path $root 'process-events.jsonl'
    $fixture.executable=$candidate.executable
    $fixture | Add-Member -NotePropertyName executable_sha256 -NotePropertyValue $candidate.executable_sha256
    $fixture | Add-Member -NotePropertyName package_result -NotePropertyValue $candidate.receipt
    $fixture | Add-Member -NotePropertyName process_events -NotePropertyValue $events
    $inputPath=Join-Path $root 'bound-input.json'; Save $inputPath $fixture
    $report.bound_input_sha256=Hash $inputPath
    $report.command=@{program=$node;arguments=@($driver,$inputPath);working_directory=$root;environment='explicit Windows identity/system directories only; no credential or provider variables'}
    Save (Join-Path $root 'predeclared.json') $report
    $info=[Diagnostics.ProcessStartInfo]::new($node); $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.WorkingDirectory=$root
    $info.ArgumentList.Add($driver); $info.ArgumentList.Add($inputPath); $info.Environment.Clear()
    foreach($name in @('SystemRoot','WINDIR','USERPROFILE','LOCALAPPDATA','APPDATA','TEMP','TMP','ProgramFiles','ProgramFiles(x86)')) {
        $value=[Environment]::GetEnvironmentVariable($name); if($value){$info.Environment[$name]=$value}
    }
    $info.Environment['PATH']="$env:SystemRoot\System32;$env:SystemRoot"
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
    $outPath=Join-Path $root 'stdout.json'; $errPath=Join-Path $root 'stderr.log'
    $outFile=[IO.File]::Create($outPath); $errFile=[IO.File]::Create($errPath)
    $cancellation=[Threading.CancellationTokenSource]::new(); $watch=[Diagnostics.Stopwatch]::StartNew()
    $started=$process.Start(); if(-not $started){throw 'Startup driver did not start'}
    $outCopy=$process.StandardOutput.BaseStream.CopyToAsync($outFile,81920,$cancellation.Token)
    $errCopy=$process.StandardError.BaseStream.CopyToAsync($errFile,81920,$cancellation.Token)
    while(-not $process.WaitForExit(100)) {
        if($watch.Elapsed.TotalSeconds -ge 300 -or $outFile.Length+$errFile.Length -gt 4MB){throw 'Startup driver exceeded time/output bounds'}
        if(Test-Path -LiteralPath $events) {
            if((Get-Item -LiteralPath $events).Length -gt 64KB){throw 'Process evidence exceeded bound'}
            $lines=[IO.File]::ReadAllText($events).Split("`n")
            foreach($line in @($lines | Select-Object -SkipLast 1)) {
                if(-not $line){continue}; $row=$line|ConvertFrom-Json
                if($row.pid -lt 1 -or $row.pid -gt [int]::MaxValue){throw 'Invalid observed process identity'}
                if(-not $resourceRows.ContainsKey([string]$row.pid)){$resourceRows[[string]$row.pid]=@{pid=$row.pid;role=$row.role;samples=0;peak_working_set_bytes=0L;cpu_ms=0.0;start_time=$null}}
            }
            foreach($row in $resourceRows.Values) {
                try {
                    $observed=[Diagnostics.Process]::GetProcessById([int]$row.pid)
                    try {
                        if([IO.Path]::GetFullPath($observed.MainModule.FileName).Replace('\\?\','') -ine [IO.Path]::GetFullPath($candidate.executable).Replace('\\?\','')){continue}
                        $birth=$observed.StartTime.ToUniversalTime().ToString('o')
                        if($row.start_time -and $row.start_time -cne $birth){continue}
                        $row.start_time=$birth; $row.samples++; $row.peak_working_set_bytes=[Math]::Max($row.peak_working_set_bytes,$observed.PeakWorkingSet64); $row.cpu_ms=[Math]::Max($row.cpu_ms,$observed.TotalProcessorTime.TotalMilliseconds)
                    } finally {$observed.Dispose()}
                } catch [ArgumentException] {} catch [InvalidOperationException] {} catch [ComponentModel.Win32Exception] {}
            }
        }
    }
    $report.wall_ms=$watch.Elapsed.TotalMilliseconds; $report.exit_code=$process.ExitCode
    $null=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($outCopy,$errCopy)).WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
    if($outFile.Length+$errFile.Length -gt 4MB){throw 'Startup driver output exceeded bound'}
    $outFile.Dispose(); $errFile.Dispose()
    $report.stdout_sha256=Hash $outPath; $report.stderr_sha256=Hash $errPath
    if($process.ExitCode -ne 0){throw 'Startup driver reported failure; private output retained'}
    $report.observations=Get-Content -LiteralPath $outPath -Raw|ConvertFrom-Json -Depth 30
    if($report.observations.ok -ne $true -or $report.observations.backend -cne $fixture.backend){throw 'Startup semantic result absent'}
    & $node (Join-Path $PSScriptRoot 'production-package.cjs') $candidate.receipt (Split-Path -Parent $candidate.executable) | Out-Null
    if($LASTEXITCODE -ne 0 -or (Hash $candidate.receipt) -cne $candidate.receipt_sha256 -or (Hash $candidate.archive) -cne $candidate.archive_sha256 -or (Hash $FixtureInput) -cne $report.fixture_input_sha256 -or (Hash $driver) -cne $report.driver_sha256 -or (Hash $node) -cne $report.node_sha256){throw 'Candidate or qualification input changed during startup checks'}
    foreach($file in $report.sdk){if((Hash (Join-Path $sdk $file.path)) -cne $file.sha256){throw 'SDK helper changed during startup checks'}}
    $report.status='observations-passed'
} catch {$report.status='failed';$report.error=$_.Exception.Message}
finally {
    $cleanup=$true
    try { if($started -and -not $process.HasExited){$process.Kill($true);$cleanup=$process.WaitForExit(10000)} }
    catch { $cleanup=$false }
    $copies=[Threading.Tasks.Task[]]@(@($outCopy,$errCopy)|Where-Object {$null -ne $_})
    if(@($copies|Where-Object {-not $_.IsCompleted}).Count){
        $cancellation.Cancel();$process.StandardOutput.Dispose();$process.StandardError.Dispose()
        try{$null=[Threading.Tasks.Task]::WhenAll($copies).WaitAsync([TimeSpan]::FromSeconds(5)).GetAwaiter().GetResult()}
        catch{if(@($copies|Where-Object {-not $_.IsCompleted}).Count){$cleanup=$false}}
    }
    foreach($copy in $copies){if($copy.IsFaulted){$null=$copy.Exception}}
    if($outFile){$outFile.Dispose()};if($errFile){$errFile.Dispose()};if($cancellation){$cancellation.Dispose()};if($process){$process.Dispose()}
    $report.process_cleanup_complete=$cleanup
    if(-not $cleanup){$report.status='failed';$report.error='Startup supervision did not complete'}
    $report.resources=@($resourceRows.Values|Sort-Object pid)
    $report.ended_at=[DateTime]::UtcNow.ToString('o'); Save (Join-Path $root 'result.json') $report
}
if($report.status -cne 'observations-passed'){throw "Startup130 failed: $($report.error)"}
Write-Output (Join-Path $root 'result.json')
