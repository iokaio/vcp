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

namespace Vcp.Qualification {
public sealed class AppContainerFixture : IDisposable {
    readonly string name = "iokaio.vcp.memory." + Guid.NewGuid().ToString("N");
    readonly string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
    IntPtr sid;
    bool created;
    public string Root { get; private set; }
    public string Sid { get; private set; }
    public string Name { get { return name; } }

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
        if (!created || sid == IntPtr.Zero) throw new ObjectDisposedException(nameof(AppContainerFixture));
        executable = Path.GetFullPath(executable);
        if (!String.Equals(Path.GetDirectoryName(executable), Root, StringComparison.OrdinalIgnoreCase) || !File.Exists(executable) ||
            (File.GetAttributes(executable) & FileAttributes.ReparsePoint) != 0) throw new IOException("Executable must be a regular file directly in the owned profile");
        if (arguments.Length > 16 || timeoutMilliseconds < 1 || timeoutMilliseconds > 180000) throw new ArgumentException("Invalid bounded process request");
        string command = String.Join(" ", new[] { executable }.Concat(arguments).Select(Quote));
        if (command.Length > 30000) throw new ArgumentException("Process command is too long");
        IntPtr list = IntPtr.Zero, caps = IntPtr.Zero, handles = IntPtr.Zero, environment = IntPtr.Zero;
        IntPtr job = IntPtr.Zero, limits = IntPtr.Zero, token = IntPtr.Zero;
        IntPtr input = IntPtr.Zero, output = IntPtr.Zero, error = IntPtr.Zero;
        bool initialized = false;
        var process = new ProcessInfo();
        try {
            var attributes = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
            input = CreateFile("NUL", 0x80000000, 3, ref attributes, 3, 0x80, IntPtr.Zero);
            if (input == new IntPtr(-1)) { input = IntPtr.Zero; throw new Win32Exception(Marshal.GetLastWin32Error()); }
            IntPtr current = GetCurrentProcess();
            Check(DuplicateHandle(current, GetStdHandle(-11), current, out output, 0, true, 2));
            Check(DuplicateHandle(current, GetStdHandle(-12), current, out error, 0, true, 2));
            handles = Marshal.AllocHGlobal(IntPtr.Size * 3);
            Marshal.Copy(new[] { input, output, error }, 0, handles, 3);
            IntPtr size = IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero, restricted ? 2 : 1, 0, ref size);
            if (size == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            list = Marshal.AllocHGlobal(size);
            Check(InitializeProcThreadAttributeList(list, restricted ? 2 : 1, 0, ref size)); initialized = true;
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
            limits = Structure(settings);
            Check(SetInformationJobObject(job, 9, limits, (uint)Marshal.SizeOf<ExtendedLimits>()));
            // Suspended until it belongs to our kill-on-close job and its token is checked.
            Check(CreateProcess(executable, new StringBuilder(command), IntPtr.Zero, IntPtr.Zero, true, 0x08080404, environment, Root, ref startup, out process));
            Check(AssignProcessToJobObject(job, process.Process));
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
            uint wait = WaitForSingleObject(process.Process, (uint)timeoutMilliseconds);
            if (wait != 0) throw new IOException(wait == 258 ? "Contained process timed out" : "Process wait failed");
            uint code; Check(GetExitCodeProcess(process.Process, out code));
            Check(QueryInformationJobObject(job, 9, limits, (uint)Marshal.SizeOf<ExtendedLimits>(), IntPtr.Zero));
            var usage = Marshal.PtrToStructure<ExtendedLimits>(limits);
            return new Result { ExitCode = code, AppContainer = isContainer, CapabilityCount = count,
                TokenSidMatchesProfile = restricted,
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
            foreach (IntPtr pointer in new[] { list, caps, handles, environment, limits }) if (pointer != IntPtr.Zero) Marshal.FreeHGlobal(pointer);
            if (terminationFailed) throw new IOException("Owned process did not terminate");
        }
    }
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
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job,IntPtr process);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
    [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr info,uint size,out uint returned);
}
}
