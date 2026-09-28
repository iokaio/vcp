// SPDX-License-Identifier: Apache-2.0
// Independent CS-3 draft. Win32 layouts/quoting follow the existing repository
// AppContainerFixture patterns; its frozen source is not modified or loaded.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Runtime.ExceptionServices;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

namespace Vcp.Cs3Draft {
public static partial class NativeProbe {
    static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
    static void Check(bool ok) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    static void Name(string name) { if (!System.Text.RegularExpressions.Regex.IsMatch(name ?? "", "\\Aiokaio\\.vcp\\.cs3\\.[a-f0-9]{32}\\z")) throw new ArgumentException("Invalid exact profile name"); }
    public static string DeriveSid(string name) {
        Name(name); IntPtr sid; Marshal.ThrowExceptionForHR(DeriveAppContainerSidFromAppContainerName(name, out sid));
        try { return new SecurityIdentifier(sid).Value; } finally { FreeSid(sid); }
    }
    public static string CreateProfile(string name) {
        Name(name); IntPtr sid;
        Marshal.ThrowExceptionForHR(CreateAppContainerProfile(name, name, "CS-3 disposable browser compatibility draft", IntPtr.Zero, 0, out sid));
        try { return new SecurityIdentifier(sid).Value; } finally { FreeSid(sid); }
    }
    public static void DeleteProfile(string name, string expectedSid) {
        Name(name); if (DeriveSid(name) != expectedSid) throw new IOException("Cleanup SID differs");
        Marshal.ThrowExceptionForHR(DeleteAppContainerProfile(name));
    }
    public static void RegularTree(string root) {
        string full = Path.GetFullPath(root);
        for (string part = full; part != null; part = Path.GetDirectoryName(part)) {
            if ((File.GetAttributes(part) & FileAttributes.ReparsePoint) != 0) throw new IOException("Reparse ancestor rejected");
        }
        foreach (string item in Directory.EnumerateFileSystemEntries(full)) {
            var attr = File.GetAttributes(item);
            if ((attr & FileAttributes.ReparsePoint) != 0) throw new IOException("Reparse child rejected");
            if ((attr & FileAttributes.Directory) != 0) RegularTree(item);
        }
    }
    static string Quote(string value) {
        if (value == null || value.IndexOf('\0') >= 0) throw new ArgumentException("Invalid argument");
        var quoted = new StringBuilder("\""); int slashes = 0;
        foreach (char item in value) { if (item == '\\') { slashes++; continue; } quoted.Append('\\', item == '"' ? slashes * 2 + 1 : slashes); quoted.Append(item); slashes = 0; }
        quoted.Append('\\', slashes * 2); return quoted.Append('"').ToString();
    }
    static IntPtr Structure<T>(T value) { IntPtr p = Marshal.AllocHGlobal(Marshal.SizeOf<T>()); Marshal.StructureToPtr(value, p, false); return p; }
    static IntPtr TokenInfo(IntPtr token, int kind) {
        uint size; GetTokenInformation(token, kind, IntPtr.Zero, 0, out size);
        if (size == 0 || size > 65536) throw new IOException("Invalid token information size");
        IntPtr p = Marshal.AllocHGlobal((int)size);
        try { Check(GetTokenInformation(token, kind, p, size, out size)); return p; } catch { Marshal.FreeHGlobal(p); throw; }
    }
    static void Event(object value) { Console.WriteLine(JsonSerializer.Serialize(value)); Console.Out.Flush(); }
    static string Bounded(string value, int limit) { return value == null || value.Length <= limit ? value : value.Substring(0, limit); }
    static void Diagnostic(object value) { try { Event(value); } catch (Exception) { /* Broken controller transport must not replace the original error. */ } }
    static void Cleanup(string operation, Action action, List<string> failures) {
        try { action(); } catch (Exception error) {
            string detail = Bounded(error.ToString(), 2048); failures.Add(operation+": "+detail);
            Diagnostic(new { type = "cleanup_error", operation, exception = detail });
        }
    }
    static void ProcessExit(IntPtr process, string phase) {
        if (process == IntPtr.Zero) return;
        uint code; Check(GetExitCodeProcess(process, out code));
        Diagnostic(new { type = "browser_exit", phase, exit_code = code, exit_code_hex = "0x"+code.ToString("X8"), still_active = code == 259 });
    }
    sealed class BoundedCapture {
        readonly MemoryStream retained = new MemoryStream(); readonly string channel; long observed;
        public readonly Task Reading;
        public BoundedCapture(Stream source, string channel = "browser_stderr") { this.channel=channel; Reading = Task.Run(() => {
            var buffer = new byte[4096]; int count;
            while ((count = source.Read(buffer, 0, buffer.Length)) != 0) {
                lock (retained) { observed += count; int keep = (int)Math.Min(count, Math.Max(0, 65536-retained.Length)); if (keep > 0) retained.Write(buffer,0,keep); }
                if (Interlocked.Read(ref observed) > 65536) throw new IOException(channel+" ceiling");
            }
        }); }
        public void Publish() {
            byte[] bytes; long total;
            lock (retained) { bytes = retained.ToArray(); total = observed; }
            Diagnostic(new { type = channel, observed_bytes = total, retained_bytes = bytes.Length, truncated = total > bytes.Length });
            for (int offset = 0; offset < bytes.Length; offset += 8192) Diagnostic(new { type = channel+"_chunk", offset, base64 = Convert.ToBase64String(bytes,offset,Math.Min(8192,bytes.Length-offset)) });
        }
    }
    static long Creation(IntPtr handle) { long created, exited, kernel, user; Check(GetProcessTimes(handle, out created, out exited, out kernel, out user)); return created; }
    public static IntPtr CompletionPort() { IntPtr port = CreateIoCompletionPort(new IntPtr(-1), IntPtr.Zero, IntPtr.Zero, 1); Check(port != IntPtr.Zero); return port; }
    public static long DuplicatePort(IntPtr port, IntPtr worker) { IntPtr copy; Check(DuplicateHandle(GetCurrentProcess(), port, worker, out copy, 0, false, 2)); return copy.ToInt64(); }
    public static bool WaitForEmpty(IntPtr port) {
        var clock = Stopwatch.StartNew();
        while (clock.ElapsedMilliseconds < 10000) { uint code; IntPtr key, process; bool received = GetQueuedCompletionStatus(port, out code, out key, out process, 100);
            if (!received) { if (Marshal.GetLastWin32Error() == 258) continue; throw new Win32Exception(Marshal.GetLastWin32Error()); }
            if (key != new IntPtr(1)) throw new IOException("Unexpected completion identity");
            if (code == 4) return true; // JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO.
        }
        return false; // Notifications are advisory: absent evidence never means empty.
    }
    public static void ClosePort(IntPtr port) { Check(CloseHandle(port)); }
    static void WaitForDrainAcknowledgement(IntPtr owner) {
        var reply=Task.Run(()=> {
            var text=new StringBuilder(); int value;
            while((value=Console.In.Read())!=-1) {
                if(value==10) return text.ToString().TrimEnd('\r');
                if(text.Length>=32) throw new IOException("Drain acknowledgement length ceiling");
                text.Append((char)value);
            }
            return (string)null;
        });
        var clock=Stopwatch.StartNew();
        while(!reply.Wait(50)) {
            if(WaitForSingleObject(owner,0)!=258) throw new IOException("Controller lost before drain acknowledgement");
            if(clock.ElapsedMilliseconds>=15000) throw new TimeoutException("Drain acknowledgement deadline");
        }
        if(WaitForSingleObject(owner,0)!=258 || reply.GetAwaiter().GetResult()!="DRAINED") throw new IOException("Missing exact controller drain acknowledgement");
    }
    public static Task<string> ReadBounded(Stream stream, int limit, bool line) { return Task.Run(() => {
        var bytes = new MemoryStream(); int value;
        while ((value = stream.ReadByte()) != -1) { if (line && value == 10) return Utf8.GetString(bytes.ToArray()).TrimEnd('\r'); if (bytes.Length >= limit) throw new IOException("Controller pipe byte ceiling"); bytes.WriteByte((byte)value); }
        return bytes.Length == 0 && line ? null : Utf8.GetString(bytes.ToArray());
    }); }
    public sealed class DiagnosticPrefix { public byte[] Bytes; public bool Truncated; }
    public static Task<DiagnosticPrefix> ReadDiagnosticPrefix(Stream stream) { return Task.Run(() => {
        var bytes = new MemoryStream(); int value;
        while ((value=stream.ReadByte()) != -1) {
            if (bytes.Length >= 65536) return new DiagnosticPrefix { Bytes=bytes.ToArray(), Truncated=true };
            bytes.WriteByte((byte)value);
        }
        return new DiagnosticPrefix { Bytes=bytes.ToArray(), Truncated=false };
    }); }
    static void VerifyToken(IntPtr process, IntPtr job, string sid, uint pid) {
        bool member; Check(IsProcessInJob(process, job, out member)); if (!member) throw new IOException("Process outside owned job");
        IntPtr token; Check(OpenProcessToken(process, 8, out token));
        try {
            IntPtr ac = TokenInfo(token, 29), caps = IntPtr.Zero, info = IntPtr.Zero;
            try {
                caps = TokenInfo(token, 30); info = TokenInfo(token, 31);
                bool contained = Marshal.ReadInt32(ac) == 1; int count = Marshal.ReadInt32(caps);
                IntPtr rawSid = Marshal.ReadIntPtr(info); string actual = rawSid == IntPtr.Zero ? null : new SecurityIdentifier(rawSid).Value;
                Event(new { type = "token", pid, appcontainer = contained, capabilities = count, sid = actual, owned_job = member });
                if (!contained || count != 0 || actual != sid) throw new IOException("Required process changed outer AppContainer token identity or capabilities");
            } finally { Marshal.FreeHGlobal(ac); if (caps != IntPtr.Zero) Marshal.FreeHGlobal(caps); if (info != IntPtr.Zero) Marshal.FreeHGlobal(info); }
        } finally { CloseHandle(token); }
    }
    static FileStream Pipe(bool parentWrites, out IntPtr child) {
        var attributes = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
        IntPtr read, write; Check(CreatePipe(out read, out write, ref attributes, 65536));
        child = parentWrites ? read : write; IntPtr parent = parentWrites ? write : read;
        try { Check(SetHandleInformation(parent, 1, 0)); return new FileStream(new SafeFileHandle(parent, true), parentWrites ? FileAccess.Write : FileAccess.Read, 4096, false); }
        catch { CloseHandle(child); CloseHandle(parent); child = IntPtr.Zero; throw; }
    }
    static uint[] JobPids(IntPtr job) {
        IntPtr p = Marshal.AllocHGlobal(8 + 64 * IntPtr.Size);
        try {
            Check(QueryInformationJobObject(job, 3, p, (uint)(8 + 64 * IntPtr.Size), IntPtr.Zero));
            int total = Marshal.ReadInt32(p), count = Marshal.ReadInt32(p, 4);
            if (total != count || count < 0 || count > 32) throw new IOException("Incomplete or excessive job process inventory");
            return Enumerable.Range(0, count).Select(i => checked((uint)Marshal.ReadIntPtr(p, 8 + i * IntPtr.Size).ToInt64())).ToArray();
        } finally { Marshal.FreeHGlobal(p); }
    }
    // Live browser-writable paths must never be recursively followed by pathname.
    // Hold each directory without write/delete sharing while traversing its children,
    // open final components as reparse points, and inspect metadata through the held
    // handle. A sharing conflict is an inconclusive probe, not permission to retry
    // with weaker sharing. This is a bounded observation, not a filesystem quota.
    const uint ScratchReadAttributes = 0x80;
    const uint ScratchDirectoryShare = 1;
    const uint ScratchFileShare = 3;
    const uint ScratchOpenExisting = 3;
    const uint ScratchFlags = 0x02200000;
    sealed class ScratchOpenFailureEvidence {
        public string type { get; set; }
        public string root_tag { get; set; }
        public string relative_path { get; set; }
        public bool relative_path_truncated { get; set; }
        public string relative_path_sha256 { get; set; }
        public int depth { get; set; }
        public bool directory_hint { get; set; }
        public string operation { get; set; }
        public int native_error { get; set; }
        public uint desired_access { get; set; }
        public uint share_mode { get; set; }
        public uint creation_disposition { get; set; }
        public uint flags_and_attributes { get; set; }
        public bool inherit_handle { get; set; }
    }
    static void ScratchTag(string value) {
        if (value != "profile" && value != "probe-temp") throw new ArgumentException("Invalid scratch root tag");
    }
    static ScratchOpenFailureEvidence ScratchFailure(string root, string tag, string path, int depth, bool directory, int error) {
        ScratchTag(tag);
        if (depth < 0 || depth > 16 || error <= 0) throw new ArgumentException("Invalid scratch failure context");
        if (string.IsNullOrEmpty(root) || string.IsNullOrEmpty(path) || !Path.IsPathRooted(root) || !Path.IsPathRooted(path) || root.Length<3 || path.Length<3 || root[1]!=':' || path[1]!=':' || root[2]!='\\' || path[2]!='\\' || root.IndexOf('/') >= 0 || path.IndexOf('/') >= 0 || root.Any(value=>value<' ') || path.Any(value=>value<' ') || root.StartsWith("\\\\",StringComparison.Ordinal) || path.StartsWith("\\\\",StringComparison.Ordinal)) throw new ArgumentException("Invalid scratch path context");
        foreach (string part in path.Split('\\')) if (part == "." || part == ".." || part.IndexOf(':') >= 0 && part != path.Substring(0,2)) throw new ArgumentException("Unsafe scratch path context");
        string boundary=Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar);
        string full=Path.GetFullPath(path);
        string prefix=boundary+Path.DirectorySeparatorChar;
        if (!full.StartsWith(prefix,StringComparison.OrdinalIgnoreCase)) throw new ArgumentException("Scratch failure path escaped its root");
        string relative=full.Substring(prefix.Length);
        if (relative.Length == 0 || relative.IndexOfAny(new[]{'\0','\r','\n'}) >= 0) throw new ArgumentException("Invalid scratch relative path");
        byte[] relativeBytes=Utf8.GetBytes(relative);
        string digest;
        using (var hash=SHA256.Create()) digest=BitConverter.ToString(hash.ComputeHash(relativeBytes)).Replace("-","").ToLowerInvariant();
        bool truncated=relative.Length>512;
        string retained=truncated?relative.Substring(0,512):relative;
        if (truncated && char.IsHighSurrogate(retained[retained.Length-1]) && relative.Length>retained.Length && char.IsLowSurrogate(relative[retained.Length])) retained=retained.Substring(0,retained.Length-1);
        return new ScratchOpenFailureEvidence {
            type="scratch_open_failure",root_tag=tag,relative_path=retained,relative_path_truncated=truncated,relative_path_sha256=digest,
            depth=depth,directory_hint=directory,operation="CreateFile",native_error=error,desired_access=ScratchReadAttributes,
            share_mode=directory?ScratchDirectoryShare:ScratchFileShare,creation_disposition=ScratchOpenExisting,flags_and_attributes=ScratchFlags,inherit_handle=false
        };
    }
    static void PreserveScratchFailure(Win32Exception error, Action report) {
        try { report(); } catch (Exception) { /* Context must never replace the original native failure. */ }
        ExceptionDispatchInfo.Capture(error).Throw();
    }
    static long Scratch(string root, string tag) {
        ScratchTag(tag);
        root=Path.GetFullPath(root);
        var clock = Stopwatch.StartNew(); int entries = 0; long bytes = 0;
        var ancestors = new List<IntPtr>();
        try {
            var names = new Stack<string>();
            for (string part = Path.GetFullPath(root); part != null; part = Path.GetDirectoryName(part)) names.Push(part);
            while (names.Count != 0) ancestors.Add(OpenScratchEntry(names.Pop(), true));
            WalkScratch(root, tag, root, 0, clock, ref entries, ref bytes);
            return bytes;
        } finally { foreach (IntPtr held in ancestors) CloseHandle(held); }
    }
    static IntPtr CreateScratchEntryHandle(string path, bool directory) {
        var attributes = new SecurityAttributes { Size = Marshal.SizeOf<SecurityAttributes>(), Inherit = false };
        // FILE_READ_ATTRIBUTES, FILE_SHARE_READ (files also SHARE_WRITE),
        // OPEN_EXISTING, BACKUP_SEMANTICS | OPEN_REPARSE_POINT.
        IntPtr held = CreateFile(path, ScratchReadAttributes, directory ? ScratchDirectoryShare : ScratchFileShare, ref attributes, ScratchOpenExisting, ScratchFlags, IntPtr.Zero);
        int error=Marshal.GetLastWin32Error();
        if (held == new IntPtr(-1)) throw new Win32Exception(error);
        return held;
    }
    static void ValidateScratchEntry(IntPtr held, bool directory) {
        var info = ScratchMetadata(held);
        if ((info.Attributes & 0x400) != 0 || ((info.Attributes & 0x10) != 0) != directory) throw new IOException("Scratch reparse/type change rejected");
    }
    static IntPtr OpenScratchEntry(string path, bool directory) {
        IntPtr held=CreateScratchEntryHandle(path,directory);
        try {
            ValidateScratchEntry(held,directory);
            return held;
        } catch { CloseHandle(held); throw; }
    }
    struct ScratchInfo { public uint Attributes; public long Bytes; }
    static ScratchInfo ScratchMetadata(IntPtr handle) {
        IntPtr p = Marshal.AllocHGlobal(24);
        try {
            Check(GetFileInformationByHandleEx(handle, 9, p, 8));
            uint attr = unchecked((uint)Marshal.ReadInt32(p));
            Check(GetFileInformationByHandleEx(handle, 1, p, 24));
            long bytes = Marshal.ReadInt64(p, 8);
            if (bytes < 0) throw new IOException("Invalid scratch size");
            return new ScratchInfo { Attributes = attr, Bytes = bytes };
        } finally { Marshal.FreeHGlobal(p); }
    }
    static void WalkScratch(string boundary, string tag, string root, int depth, Stopwatch clock, ref int count, ref long bytes) {
        if (depth > 16) throw new IOException("Scratch depth ceiling");
        foreach (string path in Directory.EnumerateFileSystemEntries(root)) {
            if (++count > 8192 || clock.ElapsedMilliseconds > 2000) throw new IOException("Scratch entry/time ceiling");
            bool directory; IntPtr held;
            // A direct child may already be delete-pending when its name is
            // returned. Skip only ERROR_FILE_NOT_FOUND/ERROR_PATH_NOT_FOUND at
            // this hint/open boundary. An access-denied open may additionally
            // be confirmed vanished by one metadata query under held ancestors.
            // No live inaccessible entry or sharing/reparse failure is accepted.
            if (!TryOpenScratchChild(boundary,tag,path,depth,out directory,out held)) continue;
            try {
                if (directory) WalkScratch(boundary,tag,path,depth + 1,clock,ref count,ref bytes);
                else { bytes = checked(bytes + ScratchMetadata(held).Bytes); if (bytes > 67108864) throw new IOException("Observed scratch byte ceiling"); }
            } finally { CloseHandle(held); }
        }
    }
    static bool TryOpenScratchChild(string boundary, string tag, string path, int depth, out bool directory, out IntPtr held) {
        directory = false; held = IntPtr.Zero;
        try {
            // This metadata hint chooses sharing only. Held-handle validation
            // rejects a replacement or reparse point before any traversal.
            directory = (File.GetAttributes(path) & FileAttributes.Directory) != 0;
        } catch(Exception error) when(ScratchEntryVanished(error)) { return false; }
        try { held=CreateScratchEntryHandle(path,directory); }
        catch(Exception error) when(ScratchEntryVanished(error)) { return false; }
        catch(Win32Exception error) {
            bool directoryHint=directory;
            if(ScratchConfirmedVanished(error,()=>File.GetAttributes(path))) {
                Diagnostic(new {type="scratch_disappeared_after_open_denied",open_failure=ScratchFailure(boundary,tag,path,depth,directoryHint,error.NativeErrorCode),confirmation="FILE_OR_PATH_NOT_FOUND"});
                return false;
            }
            PreserveScratchFailure(error,()=>Diagnostic(ScratchFailure(boundary,tag,path,depth,directoryHint,error.NativeErrorCode)));
            throw;
        }
        try { ValidateScratchEntry(held,directory); return true; }
        catch { CloseHandle(held); held=IntPtr.Zero; throw; }
    }
    static bool ScratchEntryVanished(Exception error) {
        var native=error as Win32Exception;
        return error is FileNotFoundException || error is DirectoryNotFoundException || (native!=null && (native.NativeErrorCode==2 || native.NativeErrorCode==3));
    }
    static bool ScratchConfirmedVanished(Win32Exception error,Action confirm) {
        // CreateFile may return ACCESS_DENIED for a delete-pending file. This
        // does not classify denial as absence: only a subsequent explicit
        // FILE/PATH_NOT_FOUND does. Never reopen, wait, follow a link or change
        // sharing/permissions; every other result preserves the original error.
        if(error.NativeErrorCode!=5) return false;
        try { confirm(); } catch(Exception confirmation) { return ScratchEntryVanished(confirmation); }
        return false;
    }
    // Pure classification plus one read-only missing-path check; no native
    // browser/profile/ACL/registry operation is performed.
    public static int TestScratchVanishedContract() {
        int checks=0; Action<bool> check=value=>{if(!value) throw new Exception("Scratch vanished contract assertion failed"); checks++;};
        check(ScratchEntryVanished(new FileNotFoundException()));
        check(ScratchEntryVanished(new DirectoryNotFoundException()));
        check(ScratchEntryVanished(new Win32Exception(2)));
        check(ScratchEntryVanished(new Win32Exception(3)));
        check(!ScratchEntryVanished(new Win32Exception(5)));
        check(!ScratchEntryVanished(new Win32Exception(32)));
        check(ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new FileNotFoundException();}));
        check(ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new DirectoryNotFoundException();}));
        check(ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new Win32Exception(2);}));
        check(ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new Win32Exception(3);}));
        check(!ScratchConfirmedVanished(new Win32Exception(5),()=>{}));
        check(!ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new Win32Exception(5);}));
        check(!ScratchConfirmedVanished(new Win32Exception(5),()=>{throw new Win32Exception(32);}));
        bool queried=false;
        check(!ScratchConfirmedVanished(new Win32Exception(32),()=>{queried=true;throw new FileNotFoundException();}) && !queried);
        bool directory; IntPtr held;
        string root=Path.GetTempPath().TrimEnd(Path.DirectorySeparatorChar);
        check(!TryOpenScratchChild(root,"probe-temp",Path.Combine(root,"vcp-cs3-absent-"+Guid.NewGuid().ToString("N")),0,out directory,out held) && held==IntPtr.Zero);
        return checks;
    }
    // Pure diagnostic-shape and exception-preservation checks. No path is opened.
    public static int TestScratchDiagnosticContract() {
        int checks=0; Action<bool> check=value=>{if(!value) throw new Exception("Scratch diagnostic contract assertion failed"); checks++;};
        Action<Action> reject=action=>{try{action();throw new Exception("Expected scratch diagnostic rejection");}catch(Exception error){if(error.Message=="Expected scratch diagnostic rejection")throw;}checks++;};
        string root=Path.Combine(Path.GetTempPath(),"vcp-cs3-context-root");
        string child=Path.Combine(root,"quoted-\"-unicode-\u2028-\ud83d\ude80");
        var file=ScratchFailure(root,"profile",child,0,false,5);
        check(file.type=="scratch_open_failure" && file.root_tag=="profile" && file.relative_path==Path.GetFileName(child) && !file.relative_path_truncated && file.relative_path_sha256.Length==64);
        check(file.depth==0 && !file.directory_hint && file.operation=="CreateFile" && file.native_error==5);
        check(file.desired_access==0x80 && file.share_mode==3 && file.creation_disposition==3 && file.flags_and_attributes==0x02200000 && !file.inherit_handle);
        var directory=ScratchFailure(root,"probe-temp",Path.Combine(root,"directory"),16,true,32);
        check(directory.root_tag=="probe-temp" && directory.depth==16 && directory.directory_hint && directory.share_mode==1);
        string serialized=JsonSerializer.Serialize(file);
        check(Utf8.GetByteCount(serialized)<16384 && !serialized.Contains(root) && !serialized.Contains(Path.GetDirectoryName(root)));
        string longRelative=string.Join("\\",Enumerable.Repeat(new string('x',120),6));
        var longPath=ScratchFailure(root,"profile",Path.Combine(root,longRelative),1,false,5);
        check(longPath.relative_path_truncated && longPath.relative_path.Length<=512 && longPath.relative_path_sha256.Length==64 && Utf8.GetByteCount(JsonSerializer.Serialize(longPath))<16384);
        reject(()=>ScratchFailure(root,"other",child,0,false,5));
        reject(()=>ScratchFailure(root,"profile",child,-1,false,5));
        reject(()=>ScratchFailure(root,"profile",child,17,false,5));
        reject(()=>ScratchFailure(root,"profile",Path.Combine(Path.GetDirectoryName(root),"vcp-cs3-context-root-sibling","item"),0,false,5));
        reject(()=>ScratchFailure(root,"profile",root+"\\..\\outside",0,false,5));
        reject(()=>ScratchFailure(root,"profile",root+"\\file:stream",0,false,5));
        reject(()=>ScratchFailure(root,"profile",root+"\\control\nname",0,false,5));
        reject(()=>ScratchFailure(root,"profile",root.Replace('\\','/')+"/item",0,false,5));
        reject(()=>ScratchFailure(root,"profile","relative\\item",0,false,5));
        reject(()=>ScratchFailure(root,"profile","\\\\server\\share\\item",0,false,5));
        reject(()=>ScratchFailure(root,"profile","\\\\?\\C:\\item",0,false,5));
        var original=new Win32Exception(5); bool same=false;
        try { PreserveScratchFailure(original,()=>{throw new IOException("formatter failure");}); }
        catch(Win32Exception caught) { same=Object.ReferenceEquals(original,caught) && caught.NativeErrorCode==5; }
        check(same);
        return checks;
    }
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetFileInformationByHandleEx(IntPtr file,int kind,IntPtr info,uint bytes);

    // Only the worker calls Run. No other process receives or opens its job handle.
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)] static extern int DeriveAppContainerSidFromAppContainerName(string name,out IntPtr sid);
    [StructLayout(LayoutKind.Sequential)] struct JobCompletion { public IntPtr Key, Port; }
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr CreateIoCompletionPort(IntPtr file,IntPtr existing,IntPtr key,uint concurrency);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetQueuedCompletionStatus(IntPtr port,out uint bytes,out IntPtr key,out IntPtr overlapped,uint timeout);
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool ConvertStringSidToSid(string value,out IntPtr sid);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr pointer);
    [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint pid);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long creation,out long exit,out long kernel,out long user);
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
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentThread();
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool CancelSynchronousIo(IntPtr thread);
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
