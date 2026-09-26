// SPDX-License-Identifier: Apache-2.0
// P0-02 qualification support. Not the VCP product sandbox.
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

namespace Vcp.Qualification {
public sealed class AppContainerFixture : IDisposable {
    readonly string name = "iokaio.vcp.memory." + Guid.NewGuid().ToString("N");
    readonly string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
    IntPtr sid;
    bool created;
    public string Root { get; private set; }
    public string Sid { get; private set; }
    public string Name { get { return name; } }
    public static string CurrentIntegrity() {
        IntPtr token; Check(OpenProcessToken(GetCurrentProcess(), 8, out token));
        try {
            IntPtr info = TokenInfo(token, 25);
            try { return new SecurityIdentifier(Marshal.ReadIntPtr(info)).Value; }
            finally { Marshal.FreeHGlobal(info); }
        } finally { CloseHandle(token); }
    }

    public AppContainerFixture() {
        // The trusted broker must start with these two OS profile paths, since
        // Windows caches them before CreateAppContainerProfile is called.
        if (!String.Equals(Environment.GetEnvironmentVariable("LOCALAPPDATA"), localAppData, StringComparison.OrdinalIgnoreCase) ||
            !String.Equals(Environment.GetEnvironmentVariable("USERPROFILE"), Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), StringComparison.OrdinalIgnoreCase))
            throw new IOException("Windows broker profile environment mismatch");
        try {
            Marshal.ThrowExceptionForHR(CreateAppContainerProfile(name, name, "Disposable public local-memory fixture", IntPtr.Zero, 0, out sid));
            created = true;
            Sid = new SecurityIdentifier(sid).Value;
            IntPtr folder;
            Marshal.ThrowExceptionForHR(GetAppContainerFolderPath(Sid, out folder));
            try { Root = Path.GetFullPath(Marshal.PtrToStringUni(folder)); }
            finally { Marshal.FreeCoTaskMem(folder); }
            string expected = Path.Combine(localAppData, "Packages", name, "AC");
            if (!String.Equals(Root, expected, StringComparison.OrdinalIgnoreCase)) throw new IOException("Unexpected owned AppContainer folder");
            if ((File.GetAttributes(Root) & FileAttributes.ReparsePoint) != 0) throw new IOException("AppContainer folder is a reparse point");
            Directory.CreateDirectory(Path.Combine(Root, "temp"));
        } catch { Dispose(); throw; }
    }
    public void Dispose() {
        if (sid != IntPtr.Zero) { FreeSid(sid); sid = IntPtr.Zero; }
        if (created) {
            // Only the unique profile whose creation succeeded above is removed.
            int result = DeleteAppContainerProfile(name);
            Marshal.ThrowExceptionForHR(result);
            created = false;
            if (Root != null && Directory.Exists(Root)) throw new IOException("Owned AppContainer files remain after profile deletion");
        }
    }
    public sealed class Result {
        public uint ExitCode;
        public bool AppContainer;
        public int CapabilityCount;
        public bool TokenSidMatchesProfile;
        public bool RestrictedToken;
        public string IntegrityLevel;
        public ulong PeakJobCommittedBytes;
        public long WallMilliseconds;
    }
    static void Check(bool ok) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    static string Quote(string value) {
        if (value == null || value.IndexOf('\0') >= 0) throw new ArgumentException("Invalid process argument");
        var quoted = new StringBuilder("\""); int slashes = 0;
        foreach (char item in value) {
            if (item == '\\') { slashes++; continue; }
            quoted.Append('\\', item == '"' ? slashes * 2 + 1 : slashes);
            quoted.Append(item); slashes = 0;
        }
        quoted.Append('\\', slashes * 2); quoted.Append('"'); return quoted.ToString();
    }
    static IntPtr Structure<T>(T value) {
        IntPtr pointer = Marshal.AllocHGlobal(Marshal.SizeOf<T>());
        Marshal.StructureToPtr(value, pointer, false); return pointer;
    }
    static IntPtr TokenInfo(IntPtr token, int kind) {
        uint size;
        GetTokenInformation(token, kind, IntPtr.Zero, 0, out size);
        if (size == 0 || size > 65536) throw new IOException("Invalid token information size");
        IntPtr result = Marshal.AllocHGlobal((int)size);
        try { Check(GetTokenInformation(token, kind, result, size, out size)); return result; }
        catch { Marshal.FreeHGlobal(result); throw; }
    }
    public Result Run(string executable, string[] arguments, bool restricted, int timeoutMilliseconds) {
        return RunCore(executable, arguments, restricted, timeoutMilliseconds, null);
    }
    Result RunCore(string executable, string[] arguments, bool restricted, int timeoutMilliseconds, OwnedIo bounded) {
        if (!created || sid == IntPtr.Zero) throw new ObjectDisposedException(nameof(AppContainerFixture));
        executable = Path.GetFullPath(executable);
        if (!String.Equals(Path.GetDirectoryName(executable), Root, StringComparison.OrdinalIgnoreCase) || !File.Exists(executable) ||
            (File.GetAttributes(executable) & FileAttributes.ReparsePoint) != 0) throw new IOException("Executable must be a regular file directly in the owned profile");
        if (arguments.Length > 16 || timeoutMilliseconds < 1 || timeoutMilliseconds > 180000) throw new ArgumentException("Invalid bounded process request");
        string command = String.Join(" ", new[] { executable }.Concat(arguments).Select(Quote));
        if (command.Length > 30000) throw new ArgumentException("Process command is too long");
        IntPtr list = IntPtr.Zero, caps = IntPtr.Zero, handles = IntPtr.Zero, environment = IntPtr.Zero;
        IntPtr job = IntPtr.Zero, jobHandles = IntPtr.Zero, limits = IntPtr.Zero, token = IntPtr.Zero;
        IntPtr input = IntPtr.Zero, output = IntPtr.Zero, error = IntPtr.Zero;
        bool initialized = false;
        var process = new ProcessInfo();
        try {
            var attributes = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
            if (bounded == null) {
                input = CreateFile("NUL", 0x80000000, 3, ref attributes, 3, 0x80, IntPtr.Zero);
                if (input == new IntPtr(-1)) { input = IntPtr.Zero; throw new Win32Exception(Marshal.GetLastWin32Error()); }
                IntPtr current = GetCurrentProcess();
                Check(DuplicateHandle(current, GetStdHandle(-11), current, out output, 0, true, 2));
                Check(DuplicateHandle(current, GetStdHandle(-12), current, out error, 0, true, 2));
            } else bounded.Create(out input, out output, out error);
            handles = Marshal.AllocHGlobal(IntPtr.Size * 3);
            Marshal.Copy(new[] { input, output, error }, 0, handles, 3);
            IntPtr size = IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero, restricted ? 3 : 2, 0, ref size);
            if (size == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            list = Marshal.AllocHGlobal(size);
            Check(InitializeProcThreadAttributeList(list, restricted ? 3 : 2, 0, ref size)); initialized = true;
            Check(UpdateProcThreadAttribute(list, 0, new IntPtr(0x20002), handles, new IntPtr(IntPtr.Size * 3), IntPtr.Zero, IntPtr.Zero));
            if (restricted) {
                caps = Structure(new Capabilities { Sid = sid });
                Check(UpdateProcThreadAttribute(list, 0, new IntPtr(0x20009), caps, new IntPtr(Marshal.SizeOf<Capabilities>()), IntPtr.Zero, IntPtr.Zero));
            }
            string system = Environment.GetFolderPath(Environment.SpecialFolder.System);
            string windows = Directory.GetParent(system).FullName;
            string temp = Path.Combine(Root, "temp");
            // Explicit non-secret child environment; no inherited provider or proxy credentials.
            environment = Marshal.StringToHGlobalUni(String.Join("\0", new[] {
                "APPDATA="+Root, "LOCALAPPDATA="+localAppData, "PATH="+system, "SYSTEMROOT="+windows,
                "TEMP="+temp, "TMP="+temp, "USERPROFILE="+Root, "WINDIR="+windows })+"\0\0");
            var startup = new StartupEx { Startup = new Startup {
                Size = Marshal.SizeOf<StartupEx>(), Flags = 0x100, Input = input, Output = output, Error = error }, Attributes = list };
            job = CreateJobObject(IntPtr.Zero, null); Check(job != IntPtr.Zero);
            var settings = new ExtendedLimits { Basic = new BasicLimits { Flags = 0x2008, ActiveProcesses = 1 } };
            if (bounded != null) {
                settings.Basic.Flags |= 0x300; // Process and job committed-memory ceilings.
                settings.ProcessMemory = settings.JobMemory = new UIntPtr(bounded.MemoryBytes);
            }
            limits = Structure(settings);
            Check(SetInformationJobObject(job, 9, limits, (uint)Marshal.SizeOf<ExtendedLimits>()));
            // PROC_THREAD_ATTRIBUTE_JOB_LIST assigns the process during creation.
            // A separate CreateProcess/AssignProcessToJobObject sequence strands a
            // suspended child if the owner dies between those calls (Windows 10+).
            jobHandles = Marshal.AllocHGlobal(IntPtr.Size);
            Marshal.WriteIntPtr(jobHandles, job);
            Check(UpdateProcThreadAttribute(list, 0, new IntPtr(0x2000d), jobHandles, new IntPtr(IntPtr.Size), IntPtr.Zero, IntPtr.Zero));
            // Suspended until its token is checked; kill-on-close ownership is atomic.
            // Bounded pipe-only children need no console host process. DETACHED_PROCESS
            // avoids the conhost created for CREATE_NO_WINDOW on this Windows host.
            Check(CreateProcess(executable, new StringBuilder(command), IntPtr.Zero, IntPtr.Zero, true,
                bounded == null ? 0x08080404u : 0x0008040cu, environment, Root, ref startup, out process));
            bool assigned;
            Check(IsProcessInJob(process.Process, job, out assigned));
            if (!assigned) throw new IOException("Created process missing owned job");
            if (bounded != null) {
                // Only the child owns these ends now; EOF must not depend on parent cleanup.
                foreach (IntPtr handle in new[] { input, output, error }) CloseHandle(handle);
                input = output = error = IntPtr.Zero;
                bounded.Start(process.Pid);
            }
            Check(OpenProcessToken(process.Process, 8, out token));
            IntPtr containerInfo = TokenInfo(token, 29), capabilityInfo = IntPtr.Zero, sidInfo = IntPtr.Zero;
            bool isContainer; int count;
            try {
                isContainer = Marshal.ReadInt32(containerInfo) == 1;
                capabilityInfo = TokenInfo(token, 30); count = Marshal.ReadInt32(capabilityInfo);
                if (restricted) {
                    sidInfo = TokenInfo(token, 31);
                    if (!isContainer || count != 0 || new SecurityIdentifier(Marshal.ReadIntPtr(sidInfo)).Value != Sid)
                        throw new IOException("Restricted process token mismatch");
                } else if (isContainer) throw new IOException("Control process unexpectedly contained");
            } finally {
                Marshal.FreeHGlobal(containerInfo);
                if (capabilityInfo != IntPtr.Zero) Marshal.FreeHGlobal(capabilityInfo);
                if (sidInfo != IntPtr.Zero) Marshal.FreeHGlobal(sidInfo);
            }
            var watch = Stopwatch.StartNew();
            if (ResumeThread(process.Thread) == UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (bounded == null) {
                uint wait = WaitForSingleObject(process.Process, (uint)timeoutMilliseconds);
                if (wait != 0) throw new IOException(wait == 258 ? "Contained process timed out" : "Process wait failed");
            } else {
                while (true) {
                    bounded.ObserveJob(job);
                    uint wait = WaitForSingleObject(process.Process, 10);
                    if (wait == 0) break;
                    if (wait != 258) throw new IOException("Process wait failed");
                    string stop = bounded.StopReason(watch.ElapsedMilliseconds, timeoutMilliseconds);
                    if (stop != null) {
                        bounded.Termination = stop;
                        Check(TerminateJobObject(job, 1));
                        if (WaitForSingleObject(process.Process, 5000) != 0) throw new IOException("Owned process did not terminate");
                        break;
                    }
                }
                bounded.Complete();
            }
            uint code; Check(GetExitCodeProcess(process.Process, out code));
            bool restrictedToken = IsTokenRestricted(token);
            IntPtr integrityInfo = TokenInfo(token, 25);
            string integrityLevel;
            try { integrityLevel = new SecurityIdentifier(Marshal.ReadIntPtr(integrityInfo)).Value; }
            finally { Marshal.FreeHGlobal(integrityInfo); }
            Check(QueryInformationJobObject(job, 9, limits, (uint)Marshal.SizeOf<ExtendedLimits>(), IntPtr.Zero));
            var usage = Marshal.PtrToStructure<ExtendedLimits>(limits);
            return new Result { ExitCode = code, AppContainer = isContainer, CapabilityCount = count,
                TokenSidMatchesProfile = restricted,
                RestrictedToken = restrictedToken, IntegrityLevel = integrityLevel,
                PeakJobCommittedBytes = usage.PeakJobMemory.ToUInt64(), WallMilliseconds = watch.ElapsedMilliseconds };
        } finally {
            // Closing the job kills its process tree even if the managed caller exits.
            if (job != IntPtr.Zero) CloseHandle(job);
            bool terminationFailed = false;
            if (process.Process != IntPtr.Zero) {
                if (WaitForSingleObject(process.Process, 0) != 0) {
                    // Job close may already be terminating the process. A racing
                    // TerminateProcess failure is harmless only after observed exit.
                    TerminateProcess(process.Process, 1);
                    terminationFailed = WaitForSingleObject(process.Process, 5000) != 0;
                }
                CloseHandle(process.Process);
            }
            if (process.Thread != IntPtr.Zero) CloseHandle(process.Thread);
            if (token != IntPtr.Zero) CloseHandle(token);
            foreach (IntPtr handle in new[] { input, output, error }) if (handle != IntPtr.Zero) CloseHandle(handle);
            if (initialized) DeleteProcThreadAttributeList(list);
            foreach (IntPtr pointer in new[] { list, caps, handles, jobHandles, environment, limits }) if (pointer != IntPtr.Zero) Marshal.FreeHGlobal(pointer);
            if (terminationFailed) throw new IOException("Owned process did not terminate");
        }
    }
    // Qualification-only, one request per fresh process. No child assertion is trusted.
    public sealed class BoundedResult {
        public Result Process;
        public byte[] Output, Error;
        public long OutputBytes, ErrorBytes;
        public string Termination;
        public uint Pid;
        public ulong MemoryLimitBytes;
        public uint PeakActiveProcesses;
    }
    public BoundedResult RunBounded(string executable, string[] arguments, bool restricted,
        byte[] input, int outputLimit, ulong memoryBytes, int timeoutMilliseconds, CancellationToken cancellation) {
        if (input == null || input.Length > 65536 || outputLimit < 1 || outputLimit > 1048576 ||
            memoryBytes < 67108864 || memoryBytes > 1073741824) throw new ArgumentException("Invalid bounded IO request");
        cancellation.ThrowIfCancellationRequested();
        using (var bounded = new BoundedIo(input, outputLimit, memoryBytes, cancellation)) {
            var process = RunCore(executable, arguments, restricted, timeoutMilliseconds, bounded);
            return new BoundedResult { Process = process, Pid = bounded.Pid,
                Output = bounded.Output.ToArray(), Error = bounded.Error.ToArray(),
                OutputBytes = bounded.OutputBytes, ErrorBytes = bounded.ErrorBytes, Termination = bounded.Termination,
                MemoryLimitBytes = memoryBytes, PeakActiveProcesses = bounded.PeakActiveProcesses };
        }
    }
    // Pipe ownership and job observation shared by the one-shot and duplex modes.
    abstract class OwnedIo : IDisposable {
        public ulong MemoryBytes;
        public string Termination = "exited";
        public uint Pid;
        public uint PeakActiveProcesses;
        public abstract void Create(out IntPtr childInput, out IntPtr childOutput, out IntPtr childError);
        public abstract void Start(uint pid);
        public abstract string StopReason(long elapsed, int timeout);
        public abstract void Complete();
        public abstract void Dispose();
        protected static FileStream Pipe(bool parentWrites, out IntPtr child) {
            var attributes = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
            IntPtr read, write;
            Check(CreatePipe(out read, out write, ref attributes, 0));
            child = parentWrites ? read : write;
            IntPtr parent = parentWrites ? write : read;
            try {
                Check(SetHandleInformation(parent, 1, 0));
                return new FileStream(new SafeFileHandle(parent, true), parentWrites ? FileAccess.Write : FileAccess.Read, 4096, false);
            } catch { CloseHandle(parent); CloseHandle(child); child = IntPtr.Zero; throw; }
        }
        public void ObserveJob(IntPtr job) {
            IntPtr accounting = Marshal.AllocHGlobal(48);
            try {
                Check(QueryInformationJobObject(job, 1, accounting, 48, IntPtr.Zero));
                uint active = unchecked((uint)Marshal.ReadInt32(accounting, 40));
                PeakActiveProcesses = Math.Max(PeakActiveProcesses, active);
                if (active > 1) throw new IOException("One-process job containment failed: " + active);
            } finally { Marshal.FreeHGlobal(accounting); }
        }
    }
    sealed class BoundedIo : OwnedIo {
        readonly byte[] input;
        readonly int limit;
        readonly CancellationToken cancellation;
        readonly object captureLock = new object();
        FileStream inputStream, outputStream, errorStream;
        Task inputTask, outputTask, errorTask;
        public readonly MemoryStream Output = new MemoryStream(), Error = new MemoryStream();
        public long OutputBytes, ErrorBytes;
        int overflow;
        public BoundedIo(byte[] input, int limit, ulong memoryBytes, CancellationToken cancellation) {
            this.input = (byte[])input.Clone(); this.limit = limit; MemoryBytes = memoryBytes; this.cancellation = cancellation;
        }
        public override void Create(out IntPtr childInput, out IntPtr childOutput, out IntPtr childError) {
            childInput = childOutput = childError = IntPtr.Zero;
            inputStream = Pipe(true, out childInput);
            outputStream = Pipe(false, out childOutput);
            errorStream = Pipe(false, out childError);
        }
        void Read(FileStream stream, MemoryStream retained, bool error) {
            var buffer = new byte[4096]; int count;
            while ((count = stream.Read(buffer, 0, buffer.Length)) != 0) {
                if (error) Interlocked.Add(ref ErrorBytes, count); else Interlocked.Add(ref OutputBytes, count);
                lock (captureLock) {
                    int keep = Math.Min(count, limit - (int)(Output.Length + Error.Length));
                    if (keep > 0) retained.Write(buffer, 0, keep);
                }
                if (Interlocked.Read(ref OutputBytes) + Interlocked.Read(ref ErrorBytes) > limit) Interlocked.Exchange(ref overflow, 1);
            }
        }
        public override void Start(uint pid) {
            Pid = pid;
            outputTask = Task.Run(() => Read(outputStream, Output, false));
            errorTask = Task.Run(() => Read(errorStream, Error, true));
            inputTask = Task.Run(() => {
                try { inputStream.Write(input, 0, input.Length); inputStream.Flush(); }
                catch (IOException) { /* Early child exit is reported by its process receipt. */ }
                finally { inputStream.Dispose(); }
            });
        }
        public override string StopReason(long elapsed, int timeout) {
            if (Volatile.Read(ref overflow) != 0) return "output_limit";
            if (cancellation.IsCancellationRequested) return "cancelled";
            if (elapsed >= timeout) return "timeout";
            return null;
        }
        public override void Complete() {
            if (!Task.WaitAll(new[] { inputTask, outputTask, errorTask }, 5000)) throw new IOException("Owned pipes did not close");
            if (Volatile.Read(ref overflow) != 0) Termination = "output_limit";
        }
        public override void Dispose() {
            inputStream?.Dispose(); outputStream?.Dispose(); errorStream?.Dispose();
            Output.Dispose(); Error.Dispose();
        }
    }
    // Qualification-only duplex relay. Child stdout lines are forwarded to the trusted
    // parent as opaque base64 frames; the parent decides what they mean. Nothing the
    // child writes is interpreted here beyond newline framing and byte/count ceilings.
    public sealed class InteractiveResult {
        public Result Process;
        public byte[] Error;
        public long ErrorBytes;
        public string Termination;
        public uint Pid;
        public ulong MemoryLimitBytes;
        public uint PeakActiveProcesses;
        public int FramesToChild, FramesFromChild;
        public long BytesToChild, BytesFromChild, TrailingBytes;
    }
    public InteractiveResult RunInteractive(string executable, string[] arguments, bool restricted, Stream parentInput, Stream parentOutput,
        int maxFrames, int maxFrameBytes, int maxTotalBytes, int errorLimit, ulong memoryBytes,
        int timeoutMilliseconds, int idleMilliseconds, CancellationToken cancellation) {
        if (parentInput == null || parentOutput == null || maxFrames < 1 || maxFrames > 4096 || maxFrameBytes < 1 || maxFrameBytes > 65536 ||
            maxTotalBytes < 1 || maxTotalBytes > 1048576 || errorLimit < 0 || errorLimit > 65536 ||
            memoryBytes < 67108864 || memoryBytes > 1073741824 || idleMilliseconds < 1 || idleMilliseconds > timeoutMilliseconds)
            throw new ArgumentException("Invalid interactive IO request");
        cancellation.ThrowIfCancellationRequested();
        using (var duplex = new DuplexIo(name, parentInput, parentOutput, maxFrames, maxFrameBytes, maxTotalBytes, errorLimit,
            memoryBytes, idleMilliseconds, cancellation)) {
            var process = RunCore(executable, arguments, restricted, timeoutMilliseconds, duplex);
            return new InteractiveResult { Process = process, Pid = duplex.Pid, Error = duplex.Error.ToArray(),
                ErrorBytes = Interlocked.Read(ref duplex.ErrorBytes), Termination = duplex.Termination,
                MemoryLimitBytes = memoryBytes, PeakActiveProcesses = duplex.PeakActiveProcesses,
                FramesToChild = Volatile.Read(ref duplex.FramesToChild), FramesFromChild = Volatile.Read(ref duplex.FramesFromChild),
                BytesToChild = Interlocked.Read(ref duplex.BytesToChild), BytesFromChild = Interlocked.Read(ref duplex.BytesFromChild),
                TrailingBytes = duplex.TrailingBytes };
        }
    }
    sealed class DuplexIo : OwnedIo {
        readonly string profile;
        readonly Stream parentInput, parentOutput;
        readonly int maxFrames, maxFrameBytes, maxTotalBytes, errorLimit, idle;
        readonly CancellationToken cancellation;
        readonly object outputLock = new object();
        readonly Stopwatch clock = new Stopwatch();
        FileStream inputStream, outputStream, errorStream;
        Task outputTask, errorTask;
        public readonly MemoryStream Error = new MemoryStream();
        public long ErrorBytes, BytesToChild, BytesFromChild, TrailingBytes;
        public int FramesToChild, FramesFromChild;
        long lastActivity;
        string violation;
        public DuplexIo(string profile, Stream parentInput, Stream parentOutput, int maxFrames, int maxFrameBytes, int maxTotalBytes,
            int errorLimit, ulong memoryBytes, int idle, CancellationToken cancellation) {
            this.profile = profile; this.parentInput = parentInput; this.parentOutput = parentOutput; this.maxFrames = maxFrames;
            this.maxFrameBytes = maxFrameBytes; this.maxTotalBytes = maxTotalBytes; this.errorLimit = errorLimit;
            MemoryBytes = memoryBytes; this.idle = idle; this.cancellation = cancellation;
        }
        public override void Create(out IntPtr childInput, out IntPtr childOutput, out IntPtr childError) {
            childInput = childOutput = childError = IntPtr.Zero;
            inputStream = Pipe(true, out childInput);
            outputStream = Pipe(false, out childOutput);
            errorStream = Pipe(false, out childError);
        }
        void Touch() { Interlocked.Exchange(ref lastActivity, clock.ElapsedMilliseconds); }
        void Fail(string reason) { Interlocked.CompareExchange(ref violation, reason, null); }
        void Envelope(string json) {
            byte[] bytes = Encoding.UTF8.GetBytes(json + "\n");
            lock (outputLock) { parentOutput.Write(bytes, 0, bytes.Length); parentOutput.Flush(); }
        }
        // Frames longer than the ceiling stop the child; bytes are never truncated silently.
        void Frames(Stream source, bool toChild) {
            var line = new MemoryStream(); var buffer = new byte[4096]; int count;
            while ((count = source.Read(buffer, 0, buffer.Length)) != 0) {
                for (int i = 0; i < count; i++) {
                    if (buffer[i] != (byte)'\n') {
                        if (line.Length >= maxFrameBytes) { Fail(toChild ? "parent_frame_bytes" : "frame_bytes"); return; }
                        line.WriteByte(buffer[i]); continue;
                    }
                    byte[] frame = line.ToArray(); line.SetLength(0);
                    int frames = toChild ? Interlocked.Increment(ref FramesToChild) : Interlocked.Increment(ref FramesFromChild);
                    long total = toChild ? Interlocked.Add(ref BytesToChild, frame.Length + 1) : Interlocked.Add(ref BytesFromChild, frame.Length + 1);
                    if (frames > maxFrames) { Fail(toChild ? "parent_frame_limit" : "frame_limit"); return; }
                    if (total > maxTotalBytes) { Fail(toChild ? "parent_total_bytes" : "total_bytes"); return; }
                    Touch();
                    if (toChild) { inputStream.Write(frame, 0, frame.Length); inputStream.WriteByte((byte)'\n'); inputStream.Flush(); }
                    else Envelope("{\"frame\":\"" + Convert.ToBase64String(frame) + "\"}");
                }
            }
            if (!toChild) TrailingBytes = line.Length;
            else if (line.Length != 0) Fail("parent_partial_frame");
        }
        void ReadError() {
            var buffer = new byte[4096]; int count;
            while ((count = errorStream.Read(buffer, 0, buffer.Length)) != 0) {
                long total = Interlocked.Add(ref ErrorBytes, count);
                lock (Error) {
                    int keep = (int)Math.Min(count, Math.Max(0, errorLimit - Error.Length));
                    if (keep > 0) Error.Write(buffer, 0, keep);
                }
                if (total > errorLimit) Fail("output_limit");
            }
        }
        public override void Start(uint pid) {
            Pid = pid;
            clock.Start(); Touch();
            // The supervisor records this profile identity for reconciliation after owner loss.
            Envelope("{\"started\":{\"pid\":" + pid + ",\"profile\":\"" + profile + "\"}}");
            outputTask = Task.Run(() => Frames(outputStream, false));
            errorTask = Task.Run(ReadError);
            // Parent EOF closes the child's input. A parent that never closes cannot extend
            // the run: idle, wall and cancellation deadlines still terminate the job.
            Task.Run(() => {
                try { Frames(parentInput, true); }
                catch (Exception) { /* Child exit or disposal closes the forwarding pipe. */ }
                finally { try { inputStream.Dispose(); } catch (Exception) { } }
            });
        }
        public override string StopReason(long elapsed, int timeout) {
            string reason = Volatile.Read(ref violation);
            if (reason != null) return reason;
            if (cancellation.IsCancellationRequested) return "cancelled";
            if (elapsed >= timeout) return "timeout";
            if (clock.ElapsedMilliseconds - Interlocked.Read(ref lastActivity) >= idle) return "idle_timeout";
            return null;
        }
        public override void Complete() {
            if (!Task.WaitAll(new[] { outputTask, errorTask }, 5000)) throw new IOException("Owned pipes did not close");
            string reason = Volatile.Read(ref violation);
            if (reason != null) Termination = reason;
            lock (outputLock) parentOutput.Flush();
        }
        public override void Dispose() {
            try { inputStream?.Dispose(); } catch (Exception) { }
            outputStream?.Dispose(); errorStream?.Dispose(); Error.Dispose();
        }
    }
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool CreatePipe(out IntPtr read,out IntPtr write,ref SecurityAttributes attributes,uint size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetHandleInformation(IntPtr handle,uint mask,uint flags);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateJobObject(IntPtr job,uint code);
    [StructLayout(LayoutKind.Sequential)] struct Capabilities { public IntPtr Sid, Values; public uint Count, Reserved; }
    [StructLayout(LayoutKind.Sequential)] struct SecurityAttributes { public int Size; public IntPtr Descriptor; [MarshalAs(UnmanagedType.Bool)] public bool Inherit; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct Startup {
        public int Size; public string Reserved, Desktop, Title; public uint X,Y,XSize,YSize,XChars,YChars,Fill,Flags;
        public ushort Show, Reserved2; public IntPtr Reserved3, Input, Output, Error;
    }
    [StructLayout(LayoutKind.Sequential)] struct StartupEx { public Startup Startup; public IntPtr Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr Process, Thread; public uint Pid,Tid; }
    [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
        public long ProcessTime, JobTime; public uint Flags; public UIntPtr MinWorkingSet, MaxWorkingSet;
        public uint ActiveProcesses; public UIntPtr Affinity; public uint Priority, Scheduling;
    }
    [StructLayout(LayoutKind.Sequential)] struct IoCounters { public ulong ReadOps,WriteOps,OtherOps,ReadBytes,WriteBytes,OtherBytes; }
    [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
        public BasicLimits Basic; public IoCounters Io; public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
    }
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)] static extern int CreateAppContainerProfile(string name,string display,string description,IntPtr capabilities,uint count,out IntPtr sid);
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)] static extern int DeleteAppContainerProfile(string name);
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)] static extern int GetAppContainerFolderPath(string sid,out IntPtr path);
    [DllImport("advapi32.dll")] static extern IntPtr FreeSid(IntPtr sid);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,int flags,ref IntPtr size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr attribute,IntPtr value,IntPtr size,IntPtr previous,IntPtr returned);
    [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref StartupEx startup,out ProcessInfo process);
    [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle,uint timeout);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateProcess(IntPtr process,uint code);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll",SetLastError=true)] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr GetStdHandle(int kind);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool DuplicateHandle(IntPtr sourceProcess,IntPtr source,IntPtr targetProcess,out IntPtr target,uint access,bool inherit,uint options);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateFile(string file,uint access,uint share,ref SecurityAttributes attributes,uint mode,uint flags,IntPtr template);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attributes,string name);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job,int kind,IntPtr info,uint length);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job,int kind,IntPtr info,uint length,IntPtr returned);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool IsProcessInJob(IntPtr process,IntPtr job,[MarshalAs(UnmanagedType.Bool)] out bool assigned);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll")] static extern bool IsTokenRestricted(IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr info,uint size,out uint returned);
}
}
