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
        [Nullable[double]]$JobCpuSeconds=$null,[Nullable[ulong]]$JobPeakCommittedMemoryBytes=$null,
        $BeforeCleanup=$null,$MsvcServices=$null)
    $record=[ordered]@{schema='vcp-build-progress/1';phase=$Phase;status=$Status;captured_at=[DateTime]::UtcNow.ToString('o');
        elapsed_seconds=$ElapsedSeconds;output_bytes=$OutputBytes;idle_seconds=$IdleSeconds;compiler_artifacts=$CompilerArtifacts;
        job_cpu_seconds=$JobCpuSeconds;job_peak_committed_memory_bytes=$JobPeakCommittedMemoryBytes;logical_processor_count=[Environment]::ProcessorCount;
        resource_measurement='Windows Job cumulative user+kernel CPU seconds and peak committed memory; includes the owned broker, Cargo and descendants. Null means unavailable; no percentage or stall inference.'}
    if ($null -ne $BeforeCleanup) { $record.before_cleanup=$BeforeCleanup }
    if ($null -ne $MsvcServices) { $record.msvc_services=$MsvcServices }
    $temporary=$ProgressPath+'.tmp'
    [IO.File]::WriteAllText($temporary,($record | ConvertTo-Json -Depth 8 -Compress),[Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary,$ProgressPath,$true)
}

function Read-VcpBuildChildExit {
    param([IO.FileStream]$Observation,[string]$Nonce)
    # Read the held handle, including before any optional service termination.
    # An empty observation is unknown, never a fabricated successful exit.
    if ($Observation.Length -eq 0) { return $null }
    if ($Observation.Length -gt 1024) { throw 'Oversized child-exit observation' }
    $null=$Observation.Seek(0,[IO.SeekOrigin]::Begin)
    $reader=[IO.StreamReader]::new($Observation,[Text.UTF8Encoding]::new($false,$true),$false,1024,$true)
    try { $record=$reader.ReadToEnd() | ConvertFrom-Json -AsHashtable -ErrorAction Stop } finally { $reader.Dispose() }
    if ($record -isnot [Collections.IDictionary] -or $record.Count -ne 3 -or
        $record.schema -isnot [string] -or $record.schema -cne 'vcp-build-child-exit/1' -or
        $record.nonce -isnot [string] -or $record.nonce -cne $Nonce -or
        ($record.exit_code -isnot [int] -and $record.exit_code -isnot [long]) -or
        $record.exit_code -lt [int]::MinValue -or $record.exit_code -gt [int]::MaxValue) { throw 'Invalid child-exit observation' }
    return [int]$record.exit_code
}

function Assert-VcpBuildOrdinaryTool {
    param([string]$Path)
    if (-not [IO.Path]::IsPathFullyQualified($Path) -or $Path -notmatch '^[A-Za-z]:[\\/]') { throw 'MSVC service policy requires a normal absolute tool path' }
    $item=Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or $item.FullName -ine [IO.Path]::GetFullPath($Path)) { throw 'MSVC service policy requires an ordinary tool file' }
    for ($part=$item;$part;$part=if($part -is [IO.FileInfo]){$part.Directory}else{$part.Parent}) {
        if ($part.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'MSVC service policy refuses redirected tools or ancestors' }
    }
    return $item.FullName
}

function Invoke-VcpBuildProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$Arguments,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$LogPath,
        [string]$ProgressPath,
        [string]$MsvcTelemetryExecutable,
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
using System.Text;
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
        [DllImport("kernel32.dll", EntryPoint="QueryInformationJobObject", SetLastError=true)] static extern bool QueryProcessList(IntPtr job, int kind, IntPtr value, uint size, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool IsProcessInJob(IntPtr process, IntPtr job, out bool result);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder image, ref uint size);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetProcessTimes(IntPtr process, out long created, out long exited, out long kernel, out long user);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateJobObject(IntPtr job, uint code);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateProcess(IntPtr process, uint code);
        [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr process, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
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
        public sealed class ProcessObservation {
            public uint process_id;
            public string status, image, name, started_at;
            public int? query_error;
        }
        public sealed class ProcessSnapshot {
            public uint assigned, listed;
            public bool truncated;
            public ProcessObservation[] processes;
        }
        ProcessObservation Observe(uint id) {
            var row=new ProcessObservation { process_id=id, status="unavailable" };
            // No command lines, environment, memory reads or mutation access.
            IntPtr process=OpenProcess(0x1000,false,id); // QUERY_LIMITED_INFORMATION
            if(process==IntPtr.Zero) { row.query_error=Marshal.GetLastWin32Error(); return row; }
            try {
                bool owned;
                if(!IsProcessInJob(process,handle,out owned)) { row.query_error=Marshal.GetLastWin32Error(); return row; }
                if(!owned) { row.status="no-longer-in-job"; return row; }
                var image=new StringBuilder(32768); uint length=(uint)image.Capacity;
                if(QueryFullProcessImageNameW(process,0,image,ref length)) {
                    row.image=image.ToString(); row.name=Path.GetFileName(row.image); row.status="observed";
                } else { row.query_error=Marshal.GetLastWin32Error(); }
                long created, exited, kernel, user;
                if(GetProcessTimes(process,out created,out exited,out kernel,out user)) row.started_at=DateTime.FromFileTimeUtc(created).ToString("o");
                return row;
            } finally { CloseHandle(process); }
        }
        public ProcessSnapshot Snapshot() {
            for(int capacity=16;capacity<=1024;capacity*=2) {
                int size=checked(8+IntPtr.Size*capacity);
                IntPtr buffer=Marshal.AllocHGlobal(size);
                try {
                    bool ok=QueryProcessList(handle,3,buffer,(uint)size,IntPtr.Zero);
                    int error=ok?0:Marshal.GetLastWin32Error();
                    if(!ok && error!=234) throw new Win32Exception(error);
                    if(!ok && capacity<1024) continue;
                    uint assigned=unchecked((uint)Marshal.ReadInt32(buffer,0));
                    uint listed=unchecked((uint)Marshal.ReadInt32(buffer,4));
                    int count=(int)Math.Min(listed,(uint)capacity);
                    var rows=new ProcessObservation[count];
                    for(int index=0;index<count;index++) {
                        long id=Marshal.ReadIntPtr(buffer,8+index*IntPtr.Size).ToInt64();
                        if(id<=0 || id>uint.MaxValue) throw new InvalidOperationException("Invalid owned process identifier");
                        rows[index]=Observe((uint)id);
                    }
                    return new ProcessSnapshot { assigned=assigned, listed=listed, truncated=!ok || listed>capacity || assigned>listed, processes=rows };
                } finally { Marshal.FreeHGlobal(buffer); }
            }
            throw new InvalidOperationException("Owned process diagnostic bound exceeded");
        }
        public sealed class ServiceCleanup {
            public uint process_id, termination_exit_code;
            public string image, name, started_at, sha256, result;
        }
        public bool StopDeclaredService(string expectedImage, string expectedHash, FileStream pinnedImage, uint waitMilliseconds, ServiceCleanup record) {
            var before=Snapshot();
            if(before.truncated || before.assigned!=1 || before.listed!=1 || before.processes.Length!=1)
                return false; // Ordinary descendants retain the natural-drain grace.
            var selected=before.processes[0];
            if(selected.status!="observed" || selected.started_at==null || !String.Equals(selected.image,expectedImage,StringComparison.OrdinalIgnoreCase))
                return false;
            // Hold the process handle through revalidation and termination: a
            // reused numeric PID can never redirect the termination request.
            IntPtr process=OpenProcess(0x101001,false,selected.process_id); // QUERY_LIMITED_INFORMATION | SYNCHRONIZE | TERMINATE
            if(process==IntPtr.Zero && Active==0) return false;
            Check(process!=IntPtr.Zero);
            try {
                if(WaitForSingleObject(process,0)==0) return false;
                bool owned; Check(IsProcessInJob(process,handle,out owned));
                if(!owned) throw new InvalidOperationException("Declared MSVC service left the owned Job");
                long created, exited, kernel, user; Check(GetProcessTimes(process,out created,out exited,out kernel,out user));
                if(DateTime.FromFileTimeUtc(created).ToString("o")!=selected.started_at)
                    throw new InvalidOperationException("Declared MSVC service process identity changed");
                var image=new StringBuilder(32768); uint length=(uint)image.Capacity;
                Check(QueryFullProcessImageNameW(process,0,image,ref length));
                if(!String.Equals(image.ToString(),expectedImage,StringComparison.OrdinalIgnoreCase))
                    throw new InvalidOperationException("Declared MSVC service image changed");
                pinnedImage.Position=0;
                string actualHash=Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(pinnedImage)).ToLowerInvariant();
                if(actualHash!=expectedHash) throw new InvalidOperationException("Declared MSVC service bytes changed");
                var current=Snapshot();
                if(current.truncated || current.assigned!=1 || current.listed!=1 || current.processes.Length!=1 || current.processes[0].process_id!=selected.process_id)
                    return false;
                Check(IsProcessInJob(process,handle,out owned));
                if(!owned) throw new InvalidOperationException("Declared MSVC service is no longer owned");
                if(WaitForSingleObject(process,0)==0) return false;
                record.process_id=selected.process_id; record.image=expectedImage; record.name=Path.GetFileName(expectedImage);
                record.started_at=selected.started_at; record.sha256=expectedHash; record.result="termination-requested";
                if(!TerminateProcess(process,1)) {
                    int error=Marshal.GetLastWin32Error();
                    if(WaitForSingleObject(process,0)==0) { record.result=null; return false; }
                    throw new Win32Exception(error);
                }
                if(WaitForSingleObject(process,waitMilliseconds)!=0) throw new InvalidOperationException("Declared MSVC service termination did not complete");
                uint code; Check(GetExitCodeProcess(process,out code));
                if(code!=1) throw new InvalidOperationException("Declared MSVC service termination exit differs");
                record.result="terminated"; record.termination_exit_code=code;
                return true;
            } finally { CloseHandle(process); }
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
    $artifacts=0; $processCode=$null; $brokerCode=$null; $failure=$null; $zero=$false; $exitedAt=$null;$beforeCleanup=$null
    $observation=$null; $observationRemoved=$null; $exitNonce=[Guid]::NewGuid().ToString('N')
    $observationPath=Join-Path $observationDirectory.FullName ('.vcp-child-exit-'+$exitNonce+'.json')
    $metrics=@{JobCpuSeconds=$null;JobPeakCommittedMemoryBytes=$null}
    $policy=$null; $pinnedTools=@(); $plannedCleanup=@(); $serviceAttempted=$false
    try {
        if ($MsvcTelemetryExecutable) {
            $telemetry=Assert-VcpBuildOrdinaryTool $MsvcTelemetryExecutable
            if ([IO.Path]::GetFileName($telemetry) -ine 'vctip.exe') { throw 'MSVC service policy requires the selected cl.exe sibling vctip.exe' }
            $compiler=Assert-VcpBuildOrdinaryTool (Join-Path ([IO.Path]::GetDirectoryName($telemetry)) 'cl.exe')
            foreach ($toolPath in @($telemetry,$compiler)) {
                # Deny writing and deletion for the entire invocation. Hash
                # the held bytes, then verify their path binding again below.
                $stream=[IO.FileStream]::new($toolPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
                $pinnedTools+=@{path=$toolPath;stream=$stream;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}
            }
            $policy=@{telemetry_executable=$telemetry;telemetry_sha256=$pinnedTools[0].sha256;
                compiler_executable=$compiler;compiler_sha256=$pinnedTools[1].sha256;
                pdb_endpoint='vcp-build-'+[Guid]::NewGuid().ToString('N');pdb_service_options='-shutdowntime 0';
                scope='child-process-environment';maximum_survivors=1}
        }
        $observation=[IO.FileStream]::new($observationPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,
            ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete),4096,[IO.FileOptions]::DeleteOnClose)
        $job=[VcpBuildProgress.Job]::new()
        $info=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'))
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $info.RedirectStandardInput=$true; $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        $info.WorkingDirectory=$WorkingDirectory
        if ($policy) {
            $info.Environment['_MSPDBSRV_ENDPOINT_']=$policy.pdb_endpoint
            $info.Environment['_MSPDBSRV_']=$policy.pdb_service_options
        }
        foreach ($argument in @('-NoProfile','-NonInteractive','-File',(Join-Path $PSScriptRoot 'build-progress.ps1'),'-BuildProgressBroker')) { $info.ArgumentList.Add($argument) }
        $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
        $started=$process.Start(); if (-not $started) { throw 'Build broker did not start' }
        $job.Assign($process.Handle)
        foreach ($stream in @($process.StandardOutput.BaseStream,$process.StandardError.BaseStream)) {
            $counter=[VcpBuildProgress.CountedPipe]::new($stream)
            $reader=[IO.StreamReader]::new($counter,[Text.UTF8Encoding]::new($false,$true),$false,4096)
            $pipes+=@{name=if($pipes.Count -eq 0){'stdout'}else{'stderr'};counter=$counter;reader=$reader;pending=$reader.ReadLineAsync();done=$false}
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
                if ($null -eq $exitedAt) {
                    $exitedAt=$watch.ElapsedMilliseconds; $brokerCode=$process.ExitCode
                    $processCode=Read-VcpBuildChildExit $observation $exitNonce
                }
                $pipesDone=@($pipes | Where-Object { -not $_.done }).Count -eq 0
                if ($pipesDone -and $job.Active -eq 0) { break }
                if ($policy -and -not $serviceAttempted -and $pipesDone -and $null -ne $processCode -and $processCode -eq 0 -and $brokerCode -eq 0 -and
                    $watch.ElapsedMilliseconds-$exitedAt -lt 9000 -and $watch.Elapsed.TotalSeconds -lt ($TimeoutSeconds-1)) {
                    $selection=$job.Snapshot()
                    if (-not $selection.truncated -and $selection.assigned -eq 1 -and $selection.listed -eq 1 -and $selection.processes.Count -eq 1 -and
                        $selection.processes[0].status -ceq 'observed' -and $selection.processes[0].image -ieq $telemetry) {
                        foreach ($tool in $pinnedTools) {
                            $null=Assert-VcpBuildOrdinaryTool $tool.path
                            if ((Get-FileHash -LiteralPath $tool.path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $tool.sha256) { throw 'MSVC service policy tool identity changed' }
                        }
                        $serviceRecord=[VcpBuildProgress.Job+ServiceCleanup]::new()
                        try { $serviceAttempted=$job.StopDeclaredService($telemetry,$policy.telemetry_sha256,$pinnedTools[0].stream,1000,$serviceRecord) }
                        finally { if ($serviceRecord.result) { $plannedCleanup+=,$serviceRecord } }
                    }
                }
                if ($watch.ElapsedMilliseconds-$exitedAt -ge 10000) { throw 'Build descendants or output pipes did not complete within 10 seconds' }
            }
            if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) { $timedOut=$true; throw 'Build process deadline exceeded' }
            Start-Sleep -Milliseconds 10
        }
    } catch { $failure=$_.Exception.Message }
    finally {
        try {
        if ($job) {
            # Capture the actual state before TerminateJobObject removes the
            # evidence. A failed diagnostic query never relaxes supervision.
            $beforeCleanup=@{captured_at=[DateTime]::UtcNow.ToString('o');broker_exited=($started -and $process.HasExited);
                broker_exit_observed=$brokerCode;pipes=@($pipes | ForEach-Object {@{name=$_.name;done=$_.done;read_completed=$_.pending.IsCompleted;read_faulted=$_.pending.IsFaulted;bytes=$_.counter.Count}})}
            try { $beforeCleanup.active_processes=$job.Active;$beforeCleanup.job=$job.Snapshot() }
            catch { $beforeCleanup.diagnostic_error=$_.Exception.Message }
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
                $processCode=Read-VcpBuildChildExit $observation $exitNonce
            } catch { $failure='Build child-exit observation failed: '+$_.Exception.Message }
            finally { $observation.Dispose() }
            $observationRemoved=-not [IO.File]::Exists($observationPath)
            if (-not $observationRemoved) { $failure='Build child-exit observation cleanup failed' }
        }
        foreach ($tool in $pinnedTools) {
            try {
                $null=Assert-VcpBuildOrdinaryTool $tool.path
                if ((Get-FileHash -LiteralPath $tool.path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $tool.sha256) { throw 'MSVC service policy tool identity changed' }
            } catch { $failure='Build MSVC service policy verification failed: '+$_.Exception.Message }
            finally { $tool.stream.Dispose() }
        }
        $writer.Flush(); $writer.Dispose()
        }
    }
    $code=if ($failure -or $forced -or -not $zero -or $null -eq $processCode -or $null -eq $brokerCode) { 1 }
        elseif ($brokerCode -ne 0) { $brokerCode } else { $processCode }
    $status=if ($code -eq 0) { 'pass' } else { 'fail' }
    $completion=if ($code -ne 0) { 'failed' } elseif ($plannedCleanup.Count) { 'planned-service-cleanup' } else { 'natural' }
    $services=if ($policy) { @{policy=$policy;planned_cleanup=@($plannedCleanup);completion=$completion} } else { $null }
    $elapsed=[long][Math]::Floor($watch.Elapsed.TotalSeconds)
    $idle=[long][Math]::Floor(($watch.ElapsedMilliseconds-$lastOutput)/1000)
    Write-VcpBuildPhase -ProgressPath $ProgressPath -Phase cargo -Status $status -ElapsedSeconds $elapsed -OutputBytes $lastBytes -IdleSeconds $idle -CompilerArtifacts $artifacts -BeforeCleanup $beforeCleanup -MsvcServices $services @metrics
    Write-Information "VCP_BUILD_PROGRESS elapsed_seconds=$elapsed output_bytes=$lastBytes idle_seconds=$idle compiler_artifacts=$artifacts" -InformationAction Continue
    return [pscustomobject]@{exit_code=$code;process_exit_code=$processCode;broker_exit_code=$brokerCode;
        child_exit_observation_removed=$observationRemoved;timed_out=$timedOut;forced_cleanup=$forced;job_active_processes_zero=$zero;
        before_cleanup=$beforeCleanup;completion=$completion;planned_service_cleanup=@($plannedCleanup);msvc_service_policy=$policy;
        output_bytes=$lastBytes;compiler_artifacts=$artifacts;elapsed_seconds=$elapsed;idle_seconds=$idle;failure=$failure;
        job_cpu_seconds=$metrics.JobCpuSeconds;job_peak_committed_memory_bytes=$metrics.JobPeakCommittedMemoryBytes}
}
