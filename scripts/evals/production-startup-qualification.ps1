# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PackageResult,
    [string]$FixtureManifest,
    [string[]]$FixtureResults = @(),
    [string]$OutputRoot,
    [ValidateRange(1,10)][int]$Repeats = 3,
    [ValidateRange(1,600)][int]$DeadlineSeconds = 120
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p8-production-startup' }
if ($FixtureManifest) {
    if ($FixtureResults.Count) { throw 'Select fixture manifest or explicit receipt paths' }
    $fixtureSpec = Get-Content -LiteralPath $FixtureManifest -Raw | ConvertFrom-Json
    if ($fixtureSpec.schema -ne 'vcp-retained-startup-fixtures/1') { throw 'Unknown fixture manifest' }
    $FixtureResults = @($fixtureSpec.receipts | ForEach-Object {
        if ([IO.Path]::IsPathFullyQualified($_)) { $_ } else { Join-Path $repository $_ }
    })
}
if (-not $FixtureResults.Count) { throw 'Explicit retained fixture manifest or receipt list required' }
$root = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $root | Out-Null
$privateRoot = Join-Path ([IO.Path]::GetTempPath()) ('vcp-production-startup-' + [IO.Path]::GetFileName($root))
New-Item -ItemType Directory -Path $privateRoot | Out-Null
function Save-Json($Path, $Value) { $Value | ConvertTo-Json -Depth 45 | Set-Content -LiteralPath $Path -Encoding utf8 }
function Hash($Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Inventory($Directory) {
    $directoryPath = (Get-Item -LiteralPath $Directory).FullName
    if ((Get-Item -LiteralPath $Directory).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse fixture root' }
    $entries = @(Get-ChildItem -LiteralPath $directoryPath -Recurse -Force)
    if (@($entries | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) { throw 'Reparse fixture entry' }
    @($entries | Where-Object { -not $_.PSIsContainer } | Sort-Object FullName | ForEach-Object {
        [ordered]@{ path = [IO.Path]::GetRelativePath($directoryPath,$_.FullName).Replace('\','/'); bytes = $_.Length; sha256 = Hash $_.FullName }
    })
}
function Same-Inventory($Left,$Right) { ($Left | ConvertTo-Json -Depth 6 -Compress) -ceq ($Right | ConvertTo-Json -Depth 6 -Compress) }
$report = [ordered]@{
    schema = 'vcp-production-startup-qualification/1'; status = 'running'; started_at = [DateTime]::UtcNow.ToString('o')
    runner_sha256 = Hash $PSCommandPath; counter_sha256 = Hash (Join-Path $PSScriptRoot 'production-startup-counts.py')
    repeats = $Repeats; deadline_seconds = $DeadlineSeconds; fixtures = @(); measurements = @(); preserved_originals = $null
    pipe_drain_deadline_seconds = 10; pipe_cancel_deadline_seconds = 5; process_reap_deadline_seconds = 10
    private_data_root = $privateRoot
    limitations = @('Fresh process, OS cache not flushed; descriptive small sample, no stable p95 or minimum-hardware claim.',
        'Retained failed and passing MCP campaign states differ in contents as well as size; these are not controlled scaling clones.',
        'Only inspection and history startup are timed; no model request, task resumption or full matrix.',
        'Inspection/history are expected to spawn no children. A pipe holder surviving parent exit is an unresolved external process condition; the entire campaign stops without claiming descendant termination.',
        'Workspace binding stays at the preserved original fixture; canonical data is copied before every measured invocation.')
}
$snapshots = @()
$executable = $null
if ($FixtureManifest) { $report.fixture_manifest_sha256 = Hash $FixtureManifest }
try {
    $resultPath = (Get-Item -LiteralPath $PackageResult).FullName
    $distribution = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($distribution.schema -ne 'vcp-distribution-result/1' -or $distribution.package -ne 'vcp-windows-unsigned.zip') { throw 'Exact distribution result required' }
    $archive = Join-Path (Split-Path $resultPath) $distribution.package
    if ((Hash $archive) -cne $distribution.archive_sha256) { throw 'Archive identity mismatch' }
    $package = Join-Path $root 'package'
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName -match '(^/|\\|:|(^|/)\.\.?(/|$))') { throw 'Unsafe archive entry' }
        }
        $zipNames = @($zip.Entries | ForEach-Object FullName | Sort-Object)
        $expectedNames = @($distribution.manifest.files.path + 'manifest.json' | Sort-Object)
        if (($zipNames | ConvertTo-Json -Compress) -cne ($expectedNames | ConvertTo-Json -Compress)) { throw 'Archive inventory mismatch' }
    } finally { $zip.Dispose() }
    Expand-Archive -LiteralPath $archive -DestinationPath $package
    foreach ($file in $distribution.manifest.files) {
        $payload = Get-Item -LiteralPath (Join-Path $package $file.path)
        if ($payload.Length -ne $file.bytes -or (Hash $payload.FullName) -cne $file.sha256) { throw 'Extracted package identity mismatch' }
    }
    $executable = Join-Path $package 'vcp.exe'
    $report.package_result = $resultPath; $report.package_result_sha256 = Hash $resultPath
    $report.archive_sha256 = Hash $archive; $report.executable_sha256 = Hash $executable
    $report.host = [ordered]@{ os = [Environment]::OSVersion.VersionString; architecture = $env:PROCESSOR_ARCHITECTURE; processors = [Environment]::ProcessorCount; powershell = $PSVersionTable.PSVersion.ToString() }
    try {
        $osInfo=Get-CimInstance Win32_OperatingSystem; $systemInfo=Get-CimInstance Win32_ComputerSystem
        $report.host.os_caption=$osInfo.Caption; $report.host.os_build=$osInfo.BuildNumber
        $report.host.cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
        $report.host.physical_memory_bytes=$systemInfo.TotalPhysicalMemory
        $report.host.logical_disks=@(Get-CimInstance Win32_LogicalDisk | Select-Object DeviceID,FileSystem,DriveType,Size,FreeSpace)
    } catch { $report.host.metadata_limitation=$_.Exception.Message }
    $python = (Get-Command python -CommandType Application | Select-Object -First 1).Source
    $report.counter_runtime = [ordered]@{ path = $python; sha256 = Hash $python }
    foreach ($fixtureReceiptPath in $FixtureResults) {
        $receiptPath = (Get-Item -LiteralPath $fixtureReceiptPath).FullName
        $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
        $logPath = Join-Path (Split-Path $receiptPath) 'stderr.log'
        $expectedLog = @($receipt.files | Where-Object path -EQ 'stderr.log')
        if ($expectedLog.Count -ne 1 -or (Hash $logPath) -cne $expectedLog[0].sha256) { throw 'Fixture receipt log identity mismatch' }
        $matches = [regex]::Matches((Get-Content -LiteralPath $logPath -Raw), '(?m)^p803-mcp-history backend=(sqlite|files) retained_root=([^\r\n]+)')
        if ($matches.Count -ne 1) { throw 'Exactly one retained backend root required' }
        $source = (Get-Item -LiteralPath $matches[0].Groups[2].Value).FullName
        $backend = $matches[0].Groups[1].Value
        $sourceData = Join-Path $source 'data'; $workspace = Join-Path $source 'workspace'
        $dataBefore = @(Inventory $sourceData); $workspaceBefore = @(Inventory $workspace)
        $fixtureId = '{0}-{1}' -f $backend,([IO.Path]::GetFileName((Split-Path $receiptPath)))
        Save-Json (Join-Path $root "$fixtureId-original-data-before.json") $dataBefore
        Save-Json (Join-Path $root "$fixtureId-original-workspace-before.json") $workspaceBefore
        $snapshots += @{ id=$fixtureId; data=$sourceData; workspace=$workspace; data_before=$dataBefore; workspace_before=$workspaceBefore }
        $descriptors = @(Get-ChildItem -LiteralPath (Join-Path $sourceData 'workspaces') -Filter workspace.json -Recurse -File)
        if ($descriptors.Count -ne 1) { throw 'Expected one retained workspace descriptor' }
        $descriptor = Get-Content -LiteralPath $descriptors[0].FullName -Raw | ConvertFrom-Json
        if ($descriptor.version -ne 1 -or $descriptor.config.backend -ne $backend -or $descriptor.rebind_pending) { throw 'Unsupported retained descriptor' }
        $relativeDescriptor = [IO.Path]::GetRelativePath($sourceData,$descriptors[0].FullName)
        # Count only a copied root: even read-only SQLite connections must not touch original sidecars.
        $seed = Join-Path $privateRoot "$fixtureId-seed"
        Copy-Item -LiteralPath $sourceData -Destination $seed -Recurse
        if (-not (Same-Inventory $dataBefore @(Inventory $seed))) { throw 'Fixture copy differs from source inventory' }
        $copiedDescriptorPath = Join-Path $seed $relativeDescriptor
        $canonical = Join-Path (Split-Path $copiedDescriptorPath) 'canonical'
        $countsText = & $python (Join-Path $PSScriptRoot 'production-startup-counts.py') $canonical $backend
        if ($LASTEXITCODE -ne 0) { throw 'Independent fixture count failed' }
        $counts = $countsText | ConvertFrom-Json
        $fixture = [ordered]@{ id=$fixtureId; source_root=$source; source_receipt=$receiptPath; source_receipt_sha256=Hash $receiptPath
            source_status=$receipt.status; backend=$backend; counts=$counts; files=$dataBefore.Count
            data_inventory_sha256=Hash (Join-Path $root "$fixtureId-original-data-before.json")
            workspace_inventory_sha256=Hash (Join-Path $root "$fixtureId-original-workspace-before.json")
            bytes=[long](($dataBefore | ForEach-Object { [long]$_['bytes'] } | Measure-Object -Sum).Sum); workspace=$workspace; seed=$seed; descriptor_relative=$relativeDescriptor
            task=[string]$descriptor.config.root_task; workspace_id=[string]$descriptor.config.workspace }
        $report.fixtures += $fixture
    }
    Save-Json (Join-Path $root 'predeclared.json') $report
    $stoppedBackends = @{}
    foreach ($fixture in @($report.fixtures | Sort-Object backend,@{Expression={[long]$_.counts.commit_count}})) {
        if ($stoppedBackends.ContainsKey($fixture.backend)) { continue }
        foreach ($operation in @('inspect','history')) {
            for ($repeat = 1; $repeat -le $Repeats; $repeat++) {
                $id = "$($fixture.id)-$operation-$repeat"
                $runDirectory = Join-Path $root $id
                New-Item -ItemType Directory -Path $runDirectory | Out-Null
                $copy = Join-Path $privateRoot $id
                Copy-Item -LiteralPath $fixture.seed -Destination $copy -Recurse
                $descriptorPath = Join-Path $copy $fixture.descriptor_relative
                $descriptor = Get-Content -LiteralPath $descriptorPath -Raw | ConvertFrom-Json
                # Rust canonical paths on Windows carry the extended path prefix.
                $descriptor.config.canonical_root = '\\?\' + [IO.Path]::GetFullPath((Join-Path (Split-Path $descriptorPath) 'canonical'))
                Save-Json $descriptorPath $descriptor
                Save-Json (Join-Path $runDirectory 'data-before.json') @(Inventory $copy)
                $arguments = @('--format','jsonl','--non-interactive','--workspace',$fixture.workspace,'--data-dir',$copy)
                if ($operation -eq 'inspect') { $arguments += @('inspect',$fixture.task,'--view','costs','--limit','1') }
                else { $arguments += @('history','list','--limit','1') }
                Save-Json (Join-Path $runDirectory 'command.json') @{program=$executable;arguments=$arguments;deadline_seconds=$DeadlineSeconds}
                $start = [Diagnostics.ProcessStartInfo]::new($executable)
                $start.UseShellExecute=$false; $start.CreateNoWindow=$true
                $start.WorkingDirectory=$runDirectory; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
                foreach ($argument in $arguments) { $start.ArgumentList.Add($argument) }
                foreach ($name in @('OPENROUTER_API_KEY','RUST_MIN_STACK')) { $null=$start.Environment.Remove($name) }
                $process=[Diagnostics.Process]::new(); $process.StartInfo=$start
                $stdoutPath=Join-Path $runDirectory 'stdout.jsonl'; $stderrPath=Join-Path $runDirectory 'stderr.log'
                $stdout=[IO.File]::Create($stdoutPath); $stderr=[IO.File]::Create($stderrPath)
                $timer=[Diagnostics.Stopwatch]::StartNew(); $timedOut=$false; $outputBound=$false; $peak=0L; $cpu=$null; $exitCode=$null
                $started=$false; $reaped=$false; $drainTimedOut=$false; $drainError=$null; $supervisionError=$null
                $outCopy=$null; $errCopy=$null; $drainTimer=[Diagnostics.Stopwatch]::new()
                $pipeCancellation=[Threading.CancellationTokenSource]::new(); $pipeCleanupComplete=$true
                try {
                    $started=$process.Start()
                    if (-not $started) { throw 'Process start failed' }
                    $outCopy=$process.StandardOutput.BaseStream.CopyToAsync($stdout,81920,$pipeCancellation.Token)
                    $errCopy=$process.StandardError.BaseStream.CopyToAsync($stderr,81920,$pipeCancellation.Token)
                    while (-not $process.WaitForExit(100)) {
                        $process.Refresh(); $peak=[Math]::Max($peak,$process.PeakWorkingSet64)
                        $cpu=$process.TotalProcessorTime.TotalMilliseconds
                        $outputBound=($stdout.Length -gt 4MB -or $stderr.Length -gt 4MB)
                        if ($timer.Elapsed.TotalSeconds -ge $DeadlineSeconds -or $outputBound) {
                            $timedOut= -not $outputBound; $process.Kill($true)
                            if (-not $process.WaitForExit(10000)) { throw 'Timed-out process did not reap' }
                            break
                        }
                    }
                    $timer.Stop(); $reaped=$process.HasExited; $drainTimer.Start()
                    try {
                        $copies=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($outCopy,$errCopy))
                        $null=$copies.WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
                    } catch [TimeoutException] { $drainTimedOut=$true; $drainError='Output pipes did not finish within the 10-second drain deadline' }
                    catch { $drainError=$_.Exception.Message }
                    $drainTimer.Stop()
                    $process.Refresh(); $peak=[Math]::Max($peak,$process.PeakWorkingSet64)
                    $cpu=$process.TotalProcessorTime.TotalMilliseconds; $exitCode=$process.ExitCode
                } catch { $supervisionError=$_.Exception.Message }
                finally {
                    $timer.Stop(); $drainTimer.Stop()
                    if ($started) {
                        try {
                            if (-not $process.HasExited) { $process.Kill($true); $reaped=$process.WaitForExit(10000) }
                            else { $reaped=$true }
                        } catch { $supervisionError=$_.Exception.Message; $reaped=$false }
                    }
                    $copyTasks=[Threading.Tasks.Task[]]@(@($outCopy,$errCopy) | Where-Object { $null -ne $_ })
                    if (@($copyTasks | Where-Object { -not $_.IsCompleted }).Count) {
                        $pipeCancellation.Cancel()
                        if ($started) { $process.StandardOutput.Dispose(); $process.StandardError.Dispose() }
                        try { $null=[Threading.Tasks.Task]::WhenAll($copyTasks).WaitAsync([TimeSpan]::FromSeconds(5)).GetAwaiter().GetResult() }
                        catch { if (@($copyTasks | Where-Object { -not $_.IsCompleted }).Count) { $pipeCleanupComplete=$false } }
                    }
                    # Observe copy failures even after cancellation; no unbounded pipe await remains.
                    foreach ($copyTask in $copyTasks) { if ($copyTask.IsFaulted) { $null=$copyTask.Exception } }
                    $stdout.Dispose(); $stderr.Dispose(); $pipeCancellation.Dispose(); $process.Dispose()
                }
                $semantic=$false; $semanticError=$null; $observedWatermark=$null
                try {
                    $frames=@(Get-Content -LiteralPath $stdoutPath | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json })
                    $resultFrames=@($frames | Where-Object type -EQ 'result')
                    if ($exitCode -ne 0 -or $timedOut -or $outputBound -or $drainTimedOut -or $drainError -or $supervisionError -or -not $reaped -or -not $pipeCleanupComplete -or $resultFrames.Count -ne 1 -or $resultFrames[0].exit_code -ne 0) { throw 'Successful single result or bounded supervisor cleanup missing' }
                    $value=$resultFrames[0].data; $observedWatermark=[string]$value.source_watermark
                    if ($observedWatermark -cne $fixture.counts.watermark) { throw 'Retained watermark changed' }
                    if ($operation -eq 'inspect') {
                        if ($value.view -ne 'costs' -or $value.scope.task -ne $fixture.task -or $value.scope.workspace -ne $fixture.workspace_id -or @($value.items).Count -ne 1) { throw 'Cost inspection scope or item missing' }
                    } elseif (@($value.rows).Count -ne 1 -or $value.rows[0].event.event.workspace -ne $fixture.workspace_id) { throw 'History row scope mismatch' }
                    $semantic=$true
                } catch { $semanticError=$_.Exception.Message }
                Save-Json (Join-Path $runDirectory 'data-after.json') @(Inventory $copy)
                $afterCounts=$null
                if ($reaped -and $pipeCleanupComplete) {
                    $afterCountsText=& $python (Join-Path $PSScriptRoot 'production-startup-counts.py') (Join-Path (Split-Path $descriptorPath) 'canonical') $fixture.backend
                    if ($LASTEXITCODE -eq 0) { $afterCounts=$afterCountsText | ConvertFrom-Json }
                }
                if ($null -eq $afterCounts) { $semantic=$false; $semanticError='Post-command canonical count unavailable' }
                else {
                    foreach ($field in @('commit_count','event_count','record_count','watermark')) {
                        if ($afterCounts.$field -ne $fixture.counts.$field) { $semantic=$false; $semanticError="Post-command canonical $field changed"; break }
                    }
                }
                $measurement=[ordered]@{ fixture=$fixture.id; backend=$fixture.backend; operation=$operation; repeat=$repeat
                    wall_ms=$timer.Elapsed.TotalMilliseconds; cpu_ms=$cpu; peak_working_set_bytes=if ($peak -gt 0) { $peak } else { $null }; exit_code=$exitCode
                    timed_out=$timedOut; output_bound=$outputBound; semantic_pass=$semantic; semantic_error=$semanticError
                    process_reaped=$reaped; supervision_error=$supervisionError; drain_timed_out=$drainTimedOut; drain_error=$drainError
                    unresolved_pipe_holder=($drainTimedOut -and $reaped)
                    drain_ms=$drainTimer.Elapsed.TotalMilliseconds; pipe_cleanup_complete=$pipeCleanupComplete
                    source_watermark=$observedWatermark; command_sha256=Hash (Join-Path $runDirectory 'command.json')
                    counts_after=$afterCounts; data_before_sha256=Hash (Join-Path $runDirectory 'data-before.json'); data_after_sha256=Hash (Join-Path $runDirectory 'data-after.json')
                    stdout_sha256=Hash $stdoutPath; stderr_sha256=Hash $stderrPath; directory=$runDirectory }
                $report.measurements += $measurement
                Save-Json (Join-Path $root 'result.json') $report
                if (-not $reaped -or -not $pipeCleanupComplete -or $drainTimedOut -or $drainError -or $supervisionError) { throw 'Process or output supervision failed; stop the entire campaign and retain unresolved ownership evidence' }
                Write-Host "$($fixture.backend) commits=$($fixture.counts.commit_count) $operation repeat=$repeat wall_ms=$([Math]::Round($timer.Elapsed.TotalMilliseconds)) pass=$semantic"
                if (-not $semantic) { $stoppedBackends[$fixture.backend]=$true; break }
            }
            if ($stoppedBackends.ContainsKey($fixture.backend)) { break }
        }
    }
    $report.status = if ($stoppedBackends.Count) { 'failed' } else { 'passed' }
} catch {
    $report.status='failed'; $report.error=$_.Exception.Message
} finally {
    $preserved=$true
    foreach ($snapshot in $snapshots) {
        try {
            $dataAfter=@(Inventory $snapshot.data); $workspaceAfter=@(Inventory $snapshot.workspace)
            Save-Json (Join-Path $root "$($snapshot.id)-original-data-after.json") $dataAfter
            Save-Json (Join-Path $root "$($snapshot.id)-original-workspace-after.json") $workspaceAfter
            if (-not (Same-Inventory $snapshot.data_before $dataAfter) -or -not (Same-Inventory $snapshot.workspace_before $workspaceAfter)) { $preserved=$false }
        } catch { $preserved=$false; $report.preservation_error=$_.Exception.Message }
    }
    $report.preserved_originals=$preserved
    if (-not $preserved) { $report.status='failed' }
    try {
        if ($report.executable_sha256) {
            foreach ($file in $distribution.manifest.files) {
                $payload=Get-Item -LiteralPath (Join-Path $package $file.path)
                if ($payload.Length -ne $file.bytes -or (Hash $payload.FullName) -cne $file.sha256) { $report.status='failed'; $report.package_changed=$true }
            }
        }
        if ($report.archive_sha256 -and (Hash $archive) -cne $report.archive_sha256) { $report.status='failed'; $report.archive_changed=$true }
        if ($report.package_result_sha256 -and (Hash $resultPath) -cne $report.package_result_sha256) { $report.status='failed'; $report.package_result_changed=$true }
        if ((Hash $PSCommandPath) -cne $report.runner_sha256 -or (Hash (Join-Path $PSScriptRoot 'production-startup-counts.py')) -cne $report.counter_sha256) { $report.status='failed'; $report.runner_changed=$true }
    } catch { $report.status='failed'; $report.artifact_verification_error=$_.Exception.Message }
    $report.ended_at=[DateTime]::UtcNow.ToString('o')
    Save-Json (Join-Path $root 'result.json') $report
}
Write-Output (Join-Path $root 'result.json')
if ($report.status -ne 'passed') { exit 1 }
