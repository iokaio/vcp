# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
param([switch]$BuildProgressBroker)

if ($BuildProgressBroker) {
    # The owner assigns this trusted, profile-free broker to its Job before
    # sending this line. No child can start before containment is established.
    $ErrorActionPreference='Stop'
    $request=[Console]::ReadLine()
    if (-not $request) { exit 1 }
    $request=$request | ConvertFrom-Json
    $info=[Diagnostics.ProcessStartInfo]::new($request.executable)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    $info.WorkingDirectory=$request.directory
    foreach ($argument in $request.arguments) { $info.ArgumentList.Add($argument) }
    $child=[Diagnostics.Process]::new(); $child.StartInfo=$info
    try {
        if (-not $child.Start()) { throw 'Build child did not start' }
        $stdout=$child.StandardOutput.BaseStream.CopyToAsync([Console]::OpenStandardOutput())
        $stderr=$child.StandardError.BaseStream.CopyToAsync([Console]::OpenStandardError())
        $child.WaitForExit()
        $code=$child.ExitCode
        # Record the real child exit before waiting for inherited output pipes.
        # The owner holds this fresh file with DeleteOnClose, including when the
        # owner is terminated before its PowerShell finally block can execute.
        $bytes=[Text.UTF8Encoding]::new($false).GetBytes((@{schema='vcp-build-child-exit/1';nonce=$request.exit_nonce;exit_code=$code} | ConvertTo-Json -Compress))
        $observation=[IO.FileStream]::new($request.exit_observation,[IO.FileMode]::Open,[IO.FileAccess]::Write,
            ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
        try { $observation.Write($bytes,0,$bytes.Length); $observation.Flush($true) } finally { $observation.Dispose() }
        $null=[Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout,$stderr)).WaitAsync([TimeSpan]::FromSeconds(10)).GetAwaiter().GetResult()
    } finally { $child.Dispose() }
    exit $code
}

function Write-VcpBuildPhase {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ProgressPath,
        [ValidateSet('build-input-verification','cargo','post-verification')][string]$Phase,
        [ValidateSet('running','pass','fail','interrupted')][string]$Status,
        [long]$ElapsedSeconds=0,[long]$OutputBytes=0,[long]$IdleSeconds=0,[long]$CompilerArtifacts=0,
        [Nullable[double]]$JobCpuSeconds=$null,[Nullable[ulong]]$JobPeakCommittedMemoryBytes=$null)
    $record=[ordered]@{schema='vcp-build-progress/1';phase=$Phase;status=$Status;captured_at=[DateTime]::UtcNow.ToString('o');
        elapsed_seconds=$ElapsedSeconds;output_bytes=$OutputBytes;idle_seconds=$IdleSeconds;compiler_artifacts=$CompilerArtifacts;
        job_cpu_seconds=$JobCpuSeconds;job_peak_committed_memory_bytes=$JobPeakCommittedMemoryBytes;logical_processor_count=[Environment]::ProcessorCount;
        resource_measurement='Windows Job cumulative user+kernel CPU seconds and peak committed memory; includes the owned broker, Cargo and descendants. Null means unavailable; no percentage or stall inference.'}
    $temporary=$ProgressPath+'.tmp'
    [IO.File]::WriteAllText($temporary,($record | ConvertTo-Json -Compress),[Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary,$ProgressPath,$true)
}

function Invoke-VcpBuildProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$Arguments,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$LogPath,
        [string]$ProgressPath,
        [switch]$MirrorOutput,
        [ValidateRange(1,300)][int]$ProgressSeconds=30,
        [ValidateRange(1,86400)][int]$TimeoutSeconds=14400
    )
    if (-not $IsWindows) { throw 'Native Windows build supervisor required' }
    if (-not [IO.Path]::IsPathFullyQualified($Executable)) { throw 'Absolute build executable required' }
    if (-not $ProgressPath) { $ProgressPath=Join-Path (Split-Path -Parent $LogPath) 'build-progress.json' }
    $observationDirectory=Get-Item -LiteralPath ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($LogPath))) -Force -ErrorAction Stop
    if (-not $observationDirectory.PSIsContainer) { throw 'Ordinary build output directory required' }
    for ($ancestor=$observationDirectory; $ancestor; $ancestor=$ancestor.Parent) {
        if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Build exit observation refuses redirected output ancestors' }
    }
    if (-not ('VcpBuildProgress.Job' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
namespace VcpBuildProgress {
    public sealed class Job : IDisposable {
        [StructLayout(LayoutKind.Sequential)] struct Basic {
            public long processTime, jobTime; public uint flags;
            public UIntPtr min, max; public uint active; public UIntPtr affinity; public uint priority, scheduling;
        }
        [StructLayout(LayoutKind.Sequential)] struct Limits {
            public Basic basic; public ulong readOps, writeOps, otherOps, readBytes, writeBytes, otherBytes;
            public UIntPtr processMemory, jobMemory, peakProcessMemory, peakJobMemory;
        }
        [StructLayout(LayoutKind.Sequential)] struct Accounting {
            public long user, kernel, periodUser, periodKernel; public uint faults, total, active, terminated;
        }
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateJobObjectW(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref Limits value, uint size);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting value, uint size, IntPtr returned);
        [DllImport("kernel32.dll", EntryPoint="QueryInformationJobObject", SetLastError=true)] static extern bool QueryLimits(IntPtr job, int kind, out Limits value, uint size, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateJobObject(IntPtr job, uint code);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        IntPtr handle;
        static void Check(bool ok) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
        public Job() {
            handle=CreateJobObjectW(IntPtr.Zero,null); Check(handle!=IntPtr.Zero);
            var limits=new Limits(); limits.basic.flags=0x2000; // KILL_ON_JOB_CLOSE, no breakaway.
            try { Check(SetInformationJobObject(handle,9,ref limits,(uint)Marshal.SizeOf<Limits>())); }
            catch { Dispose(); throw; }
        }
        public void Assign(IntPtr process) { Check(AssignProcessToJobObject(handle,process)); }
        public uint Active { get { Accounting value; Check(QueryInformationJobObject(handle,1,out value,(uint)Marshal.SizeOf<Accounting>(),IntPtr.Zero)); return value.active; } }
        public sealed class Measurement { public double CpuSeconds; public ulong PeakCommittedMemoryBytes; }
        public Measurement Measure() {
            Accounting value; Limits limits;
            Check(QueryInformationJobObject(handle,1,out value,(uint)Marshal.SizeOf<Accounting>(),IntPtr.Zero));
            Check(QueryLimits(handle,9,out limits,(uint)Marshal.SizeOf<Limits>(),IntPtr.Zero));
            return new Measurement { CpuSeconds=(value.user+value.kernel)/10000000.0, PeakCommittedMemoryBytes=limits.peakJobMemory.ToUInt64() };
        }
        public void Terminate() { Check(TerminateJobObject(handle,1)); }
        public void Dispose() { if(handle!=IntPtr.Zero) { CloseHandle(handle); handle=IntPtr.Zero; } GC.SuppressFinalize(this); }
        ~Job() { Dispose(); }
    }
    // Count bytes when the pipe is read, including a partial line. Progress
    // therefore does not falsely report silence while a long line is arriving.
    public sealed class CountedPipe : Stream {
        readonly Stream inner; long count;
        public CountedPipe(Stream stream) { inner=stream; }
        public long Count { get { return Interlocked.Read(ref count); } }
        public override bool CanRead=>true; public override bool CanSeek=>false; public override bool CanWrite=>false;
        public override long Length=>throw new NotSupportedException();
        public override long Position { get=>throw new NotSupportedException(); set=>throw new NotSupportedException(); }
        public override int Read(byte[] b,int o,int n) { int got=inner.Read(b,o,n); Interlocked.Add(ref count,got); return got; }
        public override async Task<int> ReadAsync(byte[] b,int o,int n,CancellationToken token) { int got=await inner.ReadAsync(b,o,n,token).ConfigureAwait(false); Interlocked.Add(ref count,got); return got; }
        public override async ValueTask<int> ReadAsync(Memory<byte> b,CancellationToken token=default) { int got=await inner.ReadAsync(b,token).ConfigureAwait(false); Interlocked.Add(ref count,got); return got; }
        public override void Flush() {} public override long Seek(long o,SeekOrigin s)=>throw new NotSupportedException();
        public override void SetLength(long n)=>throw new NotSupportedException(); public override void Write(byte[] b,int o,int n)=>throw new NotSupportedException();
        protected override void Dispose(bool disposing) { if(disposing) inner.Dispose(); base.Dispose(disposing); }
    }
}
'@
    }
    $logStream=[IO.File]::Open($LogPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $writer=[IO.StreamWriter]::new($logStream,[Text.UTF8Encoding]::new($false)); $writer.NewLine="`n"; $writer.AutoFlush=$true
    $job=$null; $process=$null; $pipes=@(); $started=$false; $forced=$false; $timedOut=$false
    $watch=[Diagnostics.Stopwatch]::StartNew(); $lastOutput=0L; $lastBytes=0L; $nextProgress=0L
    $artifacts=0; $processCode=$null; $brokerCode=$null; $failure=$null; $zero=$false; $exitedAt=$null
    $observation=$null; $observationRemoved=$null; $exitNonce=[Guid]::NewGuid().ToString('N')
    $observationPath=Join-Path $observationDirectory.FullName ('.vcp-child-exit-'+$exitNonce+'.json')
    $metrics=@{JobCpuSeconds=$null;JobPeakCommittedMemoryBytes=$null}
    try {
        $observation=[IO.FileStream]::new($observationPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,
            ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete),4096,[IO.FileOptions]::DeleteOnClose)
        $job=[VcpBuildProgress.Job]::new()
        $info=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $info.RedirectStandardInput=$true; $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        $info.WorkingDirectory=$WorkingDirectory
        foreach ($argument in @('-NoProfile','-NonInteractive','-File',(Join-Path $PSScriptRoot 'build-progress.ps1'),'-BuildProgressBroker')) { $info.ArgumentList.Add($argument) }
        $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
        $started=$process.Start(); if (-not $started) { throw 'Build broker did not start' }
        $job.Assign($process.Handle)
        foreach ($stream in @($process.StandardOutput.BaseStream,$process.StandardError.BaseStream)) {
            $counter=[VcpBuildProgress.CountedPipe]::new($stream)
            $reader=[IO.StreamReader]::new($counter,[Text.UTF8Encoding]::new($false,$true),$false,4096)
            $pipes+=@{counter=$counter;reader=$reader;pending=$reader.ReadLineAsync();done=$false}
        }
        $request=@{executable=$Executable;arguments=@($Arguments);directory=$WorkingDirectory;
            exit_observation=$observationPath;exit_nonce=$exitNonce} | ConvertTo-Json -Compress -Depth 5
        $process.StandardInput.WriteLine($request); $process.StandardInput.Close()
        while ($true) {
            # Only this owner writes the log. Never splice concurrent stderr
            # bytes into compiler JSON, even when the producer splits a line.
            foreach ($pipe in $pipes) {
                $batch=0
                while (-not $pipe.done -and $pipe.pending.IsCompleted -and $batch++ -lt 256) {
                    $line=$pipe.pending.GetAwaiter().GetResult()
                    if ($null -eq $line) { $pipe.done=$true }
                    else {
                        $writer.WriteLine($line)
                        if ($line.StartsWith('{')) {
                            try { $row=$line | ConvertFrom-Json -ErrorAction Stop; if ($row.reason -ceq 'compiler-artifact') { $artifacts++ } } catch { }
                        }
                        if ($MirrorOutput) { Write-Information $line -InformationAction Continue }
                        $pipe.pending=$pipe.reader.ReadLineAsync()
                    }
                }
            }
            $bytes=0L; foreach ($pipe in $pipes) { $bytes+=$pipe.counter.Count }
            if ($bytes -ne $lastBytes) { $lastOutput=$watch.ElapsedMilliseconds; $lastBytes=$bytes }
            if ($watch.ElapsedMilliseconds -ge $nextProgress) {
                $writer.Flush()
                $elapsed=[long][Math]::Floor($watch.Elapsed.TotalSeconds)
                $idle=[long][Math]::Floor(($watch.ElapsedMilliseconds-$lastOutput)/1000)
                Write-Information "VCP_BUILD_PROGRESS elapsed_seconds=$elapsed output_bytes=$bytes idle_seconds=$idle compiler_artifacts=$artifacts" -InformationAction Continue
                try { $sample=$job.Measure(); $metrics=@{JobCpuSeconds=$sample.CpuSeconds;JobPeakCommittedMemoryBytes=$sample.PeakCommittedMemoryBytes} }
                catch { $metrics=@{JobCpuSeconds=$null;JobPeakCommittedMemoryBytes=$null} }
                Write-VcpBuildPhase -ProgressPath $ProgressPath -Phase cargo -Status running -ElapsedSeconds $elapsed -OutputBytes $bytes -IdleSeconds $idle -CompilerArtifacts $artifacts @metrics
                $nextProgress=$watch.ElapsedMilliseconds+($ProgressSeconds*1000)
            }
            if ($process.HasExited) {
                if ($null -eq $exitedAt) { $exitedAt=$watch.ElapsedMilliseconds; $brokerCode=$process.ExitCode }
                if (@($pipes | Where-Object { -not $_.done }).Count -eq 0 -and $job.Active -eq 0) { break }
                if ($watch.ElapsedMilliseconds-$exitedAt -ge 10000) { throw 'Build descendants or output pipes did not complete within 10 seconds' }
            }
            if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) { $timedOut=$true; throw 'Build process deadline exceeded' }
            Start-Sleep -Milliseconds 10
        }
    } catch { $failure=$_.Exception.Message }
    finally {
        try {
        if ($job) {
            if ($job.Active -ne 0) { $forced=$true; $job.Terminate() }
            $cleanup=[Diagnostics.Stopwatch]::StartNew()
            while ($job.Active -ne 0 -and $cleanup.ElapsedMilliseconds -lt 10000) { Start-Sleep -Milliseconds 10 }
            $zero=($job.Active -eq 0)
        }
        # If assignment failed, the broker is still gated and has no children.
        if ($started -and -not $process.HasExited) { $forced=$true; $process.Kill($true); $null=$process.WaitForExit(10000) }
        # Preserve any final partial lines released by termination too. The Job
        # is already empty; a stuck drain is an explicit supervision failure.
        $drain=[Diagnostics.Stopwatch]::StartNew()
        while (@($pipes | Where-Object { -not $_.done }).Count -and $drain.ElapsedMilliseconds -lt 5000) {
            foreach ($pipe in $pipes) {
                if (-not $pipe.done -and $pipe.pending.IsCompleted) {
                    try {
                        $line=$pipe.pending.GetAwaiter().GetResult()
                        if ($null -eq $line) { $pipe.done=$true }
                        else { $writer.WriteLine($line); $pipe.pending=$pipe.reader.ReadLineAsync() }
                    } catch { $pipe.done=$true; $failure='Build output drain failed: '+$_.Exception.Message }
                }
            }
            Start-Sleep -Milliseconds 1
        }
        if (@($pipes | Where-Object { -not $_.done }).Count) { $failure='Build output drain did not complete within 5 seconds' }
        $lastBytes=0L; foreach ($pipe in $pipes) { $lastBytes+=$pipe.counter.Count }
        } finally {
        foreach ($pipe in $pipes) {
            try { $pipe.reader.Dispose() } catch { }
            if ($pipe.pending.IsFaulted) { $null=$pipe.pending.Exception }
        }
        if ($process) {
            if ($started -and $process.HasExited) { $brokerCode=$process.ExitCode }
            $process.Dispose()
        }
        if ($job) {
            try { $sample=$job.Measure(); $metrics=@{JobCpuSeconds=$sample.CpuSeconds;JobPeakCommittedMemoryBytes=$sample.PeakCommittedMemoryBytes} }
            catch { $metrics=@{JobCpuSeconds=$null;JobPeakCommittedMemoryBytes=$null} }
            $job.Dispose()
        }
        if ($observation) {
            try {
                # Read the held handle, never an independently replaced path.
                # Empty means the child exit was not observed (for example a
                # failed spawn or an interrupted build), not a fabricated code.
                if ($observation.Length -gt 0) {
                    if ($observation.Length -gt 1024) { throw 'Oversized child-exit observation' }
                    $null=$observation.Seek(0,[IO.SeekOrigin]::Begin)
                    $observationReader=[IO.StreamReader]::new($observation,[Text.UTF8Encoding]::new($false,$true),$false,1024,$true)
                    try { $record=$observationReader.ReadToEnd() | ConvertFrom-Json -AsHashtable -ErrorAction Stop } finally { $observationReader.Dispose() }
                    if ($record -isnot [Collections.IDictionary] -or $record.Count -ne 3 -or
                        $record.schema -isnot [string] -or $record.schema -cne 'vcp-build-child-exit/1' -or
                        $record.nonce -isnot [string] -or $record.nonce -cne $exitNonce -or
                        ($record.exit_code -isnot [int] -and $record.exit_code -isnot [long]) -or
                        $record.exit_code -lt [int]::MinValue -or $record.exit_code -gt [int]::MaxValue) { throw 'Invalid child-exit observation' }
                    $processCode=[int]$record.exit_code
                }
            } catch { $failure='Build child-exit observation failed: '+$_.Exception.Message }
            finally { $observation.Dispose() }
            $observationRemoved=-not [IO.File]::Exists($observationPath)
            if (-not $observationRemoved) { $failure='Build child-exit observation cleanup failed' }
        }
        $writer.Flush(); $writer.Dispose()
        }
    }
    $code=if ($failure -or $forced -or -not $zero -or $null -eq $processCode -or $null -eq $brokerCode) { 1 }
        elseif ($brokerCode -ne 0) { $brokerCode } else { $processCode }
    $status=if ($code -eq 0) { 'pass' } else { 'fail' }
    $elapsed=[long][Math]::Floor($watch.Elapsed.TotalSeconds)
    $idle=[long][Math]::Floor(($watch.ElapsedMilliseconds-$lastOutput)/1000)
    Write-VcpBuildPhase -ProgressPath $ProgressPath -Phase cargo -Status $status -ElapsedSeconds $elapsed -OutputBytes $lastBytes -IdleSeconds $idle -CompilerArtifacts $artifacts @metrics
    Write-Information "VCP_BUILD_PROGRESS elapsed_seconds=$elapsed output_bytes=$lastBytes idle_seconds=$idle compiler_artifacts=$artifacts" -InformationAction Continue
    return [pscustomobject]@{exit_code=$code;process_exit_code=$processCode;broker_exit_code=$brokerCode;
        child_exit_observation_removed=$observationRemoved;timed_out=$timedOut;forced_cleanup=$forced;job_active_processes_zero=$zero;
        output_bytes=$lastBytes;compiler_artifacts=$artifacts;elapsed_seconds=$elapsed;idle_seconds=$idle;failure=$failure;
        job_cpu_seconds=$metrics.JobCpuSeconds;job_peak_committed_memory_bytes=$metrics.JobPeakCommittedMemoryBytes}
}
