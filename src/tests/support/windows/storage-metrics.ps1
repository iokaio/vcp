# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([Parameter(Mandatory)][string]$Binary,[Parameter(Mandatory)][string]$Root,[Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Native Windows required' }
if (Test-Path -LiteralPath $Root) { throw 'Benchmark root must be new' }
$scratch=Join-Path (Split-Path -Parent $Root) 'benchmark-scratch'
New-Item -ItemType Directory -Path $scratch | Out-Null
$start=[Diagnostics.ProcessStartInfo]::new()
$start.FileName=$Binary; $start.ArgumentList.Add('benchmark'); $start.ArgumentList.Add($Root)
$start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
$start.Environment['TMP']=$scratch; $start.Environment['TEMP']=$scratch
$process=[Diagnostics.Process]::new();$process.StartInfo=$start
$watch=[Diagnostics.Stopwatch]::StartNew()
$record=[ordered]@{status='running';samples=0;requested_interval_ms=50;maximum_sample_gap_ms=0;peak_sampled_disk_bytes=0;peak_sampled_snapshot_bytes=@{};peak_sampled_private_bytes=0;peak_working_set_bytes=0;ciphertext_observations=0;plaintext_marker_observed=$false}
try {
    if (-not $process.Start()) {throw 'Cannot start native benchmark'}
    $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync();$previous=0
    do {
        $elapsed=$watch.ElapsedMilliseconds;$record.maximum_sample_gap_ms=[Math]::Max($record.maximum_sample_gap_ms,$elapsed-$previous);$previous=$elapsed
        $bytes=0L;$bySnapshot=@{}
        foreach ($directory in @($Root,$scratch)) {
            if(Test-Path -LiteralPath $directory){Get-ChildItem -LiteralPath $directory -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
                $bytes+=$_.Length
                $parts=[IO.Path]::GetRelativePath($Root,$_.FullName).Split([IO.Path]::DirectorySeparatorChar)
                if($parts.Length -ge 3 -and $parts[1] -match '^(stage-)?(full|incremental)$'){
                    $key=$parts[0]+'/'+($parts[1] -replace '^stage-','')
                    if(-not $bySnapshot.ContainsKey($key)){$bySnapshot[$key]=0L};$bySnapshot[$key]+=$_.Length
                }
                if($_.Extension -in @('.age','.partial')) {
                    try {
                        $observed=[IO.File]::ReadAllBytes($_.FullName);$record.ciphertext_observations++
                        if([Text.Encoding]::ASCII.GetString($observed).Contains('VCP_P0_PLAINTEXT_CANARY')){$record.plaintext_marker_observed=$true;throw 'Plaintext marker observed in vault'}
                    } catch [IO.IOException] { } # A concurrent create/rename may deny this sample; final closed files are also sampled.
                }
            }}
        }
        foreach($key in $bySnapshot.Keys){$record.peak_sampled_snapshot_bytes[$key]=[Math]::Max([long]$record.peak_sampled_snapshot_bytes[$key],$bySnapshot[$key])}
        $record.samples++;$record.peak_sampled_disk_bytes=[Math]::Max($record.peak_sampled_disk_bytes,$bytes)
        if(-not $process.HasExited){$process.Refresh();$record.peak_sampled_private_bytes=[Math]::Max($record.peak_sampled_private_bytes,$process.PrivateMemorySize64);$record.peak_working_set_bytes=[Math]::Max($record.peak_working_set_bytes,$process.PeakWorkingSet64)}
        if($watch.Elapsed.TotalSeconds -gt 600){throw 'Native benchmark timeout'}
        Start-Sleep -Milliseconds 50
    } while(-not $process.HasExited)
    $process.WaitForExit();$record.exit_code=$process.ExitCode;$record.cpu_ms=$process.TotalProcessorTime.TotalMilliseconds;$record.wall_ms=$watch.ElapsedMilliseconds
    [IO.File]::WriteAllText($Output+'.stdout.log',$stdout.GetAwaiter().GetResult());[IO.File]::WriteAllText($Output+'.stderr.log',$stderr.GetAwaiter().GetResult())
    if($record.exit_code -ne 0 -or $record.plaintext_marker_observed -or $record.ciphertext_observations -lt 1){throw 'Native benchmark or vault observation failed'}
    $record.status='pass'
} catch {$record.status='fail';$record.reason=$_.Exception.Message;if(-not $process.HasExited){$process.Kill($true)};throw}
finally {$record | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Output -Encoding utf8;$process.Dispose()}
