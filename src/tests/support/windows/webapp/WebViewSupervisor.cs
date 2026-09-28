// SPDX-License-Identifier: Apache-2.0
// Startup-only adaptation of the corrected independent CS-3 worker.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
using Vcp.Qualification.Webapp;
namespace Vcp.Cs3Draft {
public static partial class NativeProbe {
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder name,ref uint size);
    [DllImport("kernel32.dll")] static extern uint GetProcessId(IntPtr process);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern uint GetFinalPathNameByHandle(IntPtr file,StringBuilder path,uint length,uint flags);
    [StructLayout(LayoutKind.Sequential)] struct Accounting {
        public long User,Kernel,PeriodUser,PeriodKernel;
        public uint Faults,Total,Active,Terminated;
    }
    static Accounting Accounts(IntPtr job) {
        IntPtr p=Marshal.AllocHGlobal(Marshal.SizeOf<Accounting>());
        try { Check(QueryInformationJobObject(job,1,p,(uint)Marshal.SizeOf<Accounting>(),IntPtr.Zero)); return Marshal.PtrToStructure<Accounting>(p); }
        finally { Marshal.FreeHGlobal(p); }
    }
    static string Image(IntPtr process,uint pid,Dictionary<string,string> images) {
        var name=new StringBuilder(4096); uint length=4096;
        Check(QueryFullProcessImageName(process,0,name,ref length));
        string file=Path.GetFullPath(name.ToString());
        string expected=ProbeContract.RequireImage(name.ToString(),file,pid,Creation(process),images,Diagnostic);
        if((File.GetAttributes(file)&FileAttributes.ReparsePoint)!=0) throw new IOException("Process image redirected");
        using(var stream=File.OpenRead(file)) if(Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant()!=expected) throw new IOException("Process image bytes differ");
        return file;
    }
    static string HeldImage(IntPtr image,uint pid,long creation,Dictionary<string,string> images) {
        var name=new StringBuilder(4096); uint length=GetFinalPathNameByHandle(image,name,4096,0);
        if(length==0 || length>=4096) throw new Win32Exception(Marshal.GetLastWin32Error());
        string raw=name.ToString(), file=raw.StartsWith(@"\\?\",StringComparison.Ordinal)?raw.Substring(4):raw;
        file=Path.GetFullPath(file); string expected=ProbeContract.RequireImage(raw,file,pid,creation,images,Diagnostic);
        if((File.GetAttributes(file)&FileAttributes.ReparsePoint)!=0) throw new IOException("Process image redirected");
        using(var stream=new FileStream(new SafeFileHandle(image,false),FileAccess.Read,4096,false)) {
            stream.Position=0;
            if(Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant()!=expected) throw new IOException("Process image bytes differ");
        }
        return file;
    }
    // Capture handles independently of expensive scratch traversal, image hashing
    // and DOM parsing. No identity is accepted by this thread: the supervisor
    // subsequently checks every held process token, job, creation time and image.
    sealed class ProcessCollector : IDisposable {
        internal sealed class Held {
            internal uint Pid;
            internal IntPtr Process,Image;
            internal long Created;
        }
        readonly object gate=new object();
        readonly Dictionary<uint,Held> held=new Dictionary<uint,Held>();
        readonly ManualResetEventSlim stopped=new ManualResetEventSlim(false);
        readonly Thread thread;
        readonly IntPtr job;
        readonly Dictionary<string,string> images;
        ExceptionDispatchInfo failure;
        internal ProcessCollector(IntPtr ownedJob,Dictionary<string,string> approvedImages) {
            images=new Dictionary<string,string>(approvedImages,approvedImages.Comparer);
            Check(DuplicateHandle(GetCurrentProcess(),ownedJob,GetCurrentProcess(),out job,0,false,2));
            thread=new Thread(Collect) { IsBackground=true,Name="cs3-owned-process-handles" };
            try { thread.Start(); } catch { CloseHandle(job); stopped.Dispose(); throw; }
        }
        void Collect() {
            try {
                while(!stopped.IsSet) {
                    foreach(uint pid in JobPids(job)) {
                        lock(gate) {
                            Held prior;
                            if(held.TryGetValue(pid,out prior)) {
                                if(WaitForSingleObject(prior.Process,0)==0) throw new IOException("Numeric PID reused during handle collection");
                                continue;
                            }
                            if(held.Count>=128) throw new IOException("Process handle collection ceiling");
                        }
                        IntPtr process=IntPtr.Zero,image=IntPtr.Zero;
                        long created=0; string stage="open_process";
                        try {
                            process=OpenProcess(0x100400,false,pid); Check(process!=IntPtr.Zero);
                            stage="job_membership";
                            bool member; Check(IsProcessInJob(process,job,out member));
                            if(!member || GetProcessId(process)!=pid) throw new IOException("Collected process outside owned job");
                            stage="creation"; created=Creation(process);
                            var name=new StringBuilder(4096); uint length=4096;
                            stage="image_query";
                            Check(QueryFullProcessImageName(process,0,name,ref length));
                            string file=Path.GetFullPath(name.ToString());
                            ProbeContract.RequireImage(name.ToString(),file,pid,created,images,Diagnostic);
                            var attributes=new SecurityAttributes { Size=Marshal.SizeOf<SecurityAttributes>(),Inherit=false };
                            // Read-only, shared read/delete, never an executable load.
                            stage="image_open";
                            image=CreateFile(file,0x80000000,5,ref attributes,3,0,IntPtr.Zero);
                            if(image==new IntPtr(-1)) { image=IntPtr.Zero; throw new Win32Exception(Marshal.GetLastWin32Error()); }
                            var item=new Held { Pid=pid,Process=process,Image=image,Created=created };
                            lock(gate) held.Add(pid,item);
                            process=image=IntPtr.Zero;
                        } catch(Win32Exception error) {
                            // Preserve the original native error before any other
                            // API call. Only an already-held, exact exited process
                            // may defer image querying; no open failure is ignored.
                            int nativeError=error.NativeErrorCode;
                            uint wait=process==IntPtr.Zero?UInt32.MaxValue:WaitForSingleObject(process,stage=="image_query" && nativeError==5?50u:0u);
                            bool defer=ProbeContract.CanDeferCollectedImage(stage,nativeError,wait,created);
                            Diagnostic(new {type="collector_capture_failure",stage,pid,creation_filetime=created,native_error=nativeError,process_handle_held=process!=IntPtr.Zero,process_signaled=wait==0,image_deferred=defer});
                            if(!defer) throw;
                            lock(gate) held.Add(pid,new Held {Pid=pid,Process=process,Image=IntPtr.Zero,Created=created});
                            process=IntPtr.Zero;
                        } finally { if(image!=IntPtr.Zero) CloseHandle(image); if(process!=IntPtr.Zero) CloseHandle(process); }
                    }
                    stopped.Wait(1);
                }
            } catch(Exception error) { lock(gate) failure=ExceptionDispatchInfo.Capture(error); }
            finally { CloseHandle(job); }
        }
        internal Held[] Snapshot() {
            lock(gate) { if(failure!=null) failure.Throw(); return held.Values.ToArray(); }
        }
        internal void Stop() {
            stopped.Set();
            if(!thread.Join(5000)) throw new IOException("Process collector did not drain");
        }
        public void Dispose() {
            Stop();
            lock(gate) {
                foreach(var item in held.Values) { if(item.Image!=IntPtr.Zero) CloseHandle(item.Image); CloseHandle(item.Process); }
                held.Clear();
            }
            stopped.Dispose();
        }
    }
    public static void CheckLayouts() {
        if(IntPtr.Size!=8 || Marshal.SizeOf<StartupEx>()!=112 || Marshal.SizeOf<Capabilities>()!=24 || Marshal.SizeOf<ExtendedLimits>()!=144 || Marshal.SizeOf<Accounting>()!=48) throw new IOException("Unexpected x64 layouts");
    }
    sealed class OwnedServer : IDisposable {
        readonly TcpListener listener;
        readonly TcpClient idleClient, idleAccepted;
        readonly Task serving;
        readonly object gate=new object();
        TcpClient activeClient;
        int requests; bool disposed;
        public string Origin { get; private set; }
        public int Requests { get { lock(gate) return requests; } }
        public OwnedServer() {
            listener=new TcpListener(IPAddress.Loopback,0);
            try {
            listener.Start(2);
            var endpoint=(IPEndPoint)listener.LocalEndpoint;
            if(!endpoint.Address.Equals(IPAddress.Loopback) || endpoint.Port<=0) throw new IOException("Owned server endpoint differs");
            Origin="http://127.0.0.1:"+endpoint.Port.ToString(System.Globalization.CultureInfo.InvariantCulture);
            // One intentionally idle, separately held connection demonstrates
            // that exact /ready and DOM conditions do not require network idle.
            idleClient=new TcpClient(AddressFamily.InterNetwork); idleClient.Connect(IPAddress.Loopback,endpoint.Port);
            idleAccepted=listener.AcceptTcpClient();
            serving=Task.Run((Action)Serve);
            } catch {
                if(idleAccepted!=null) idleAccepted.Dispose();
                if(idleClient!=null) idleClient.Dispose();
                listener.Stop();
                throw;
            }
        }
        void Serve() {
            try {
                for(int index=0;index<(UiArtifactResource.Enabled?11:10);index++) using(var client=listener.AcceptTcpClient()) using(var stream=client.GetStream()) {
                    lock(gate) { if(disposed) return; activeClient=client; }
                    client.ReceiveTimeout=3000; client.SendTimeout=3000;
                    var bytes=new List<byte>(); int state=0;
                    while(bytes.Count<4096 && state<4) {
                        int value=stream.ReadByte(); if(value<0) throw new IOException("Owned server request ended early");
                        bytes.Add((byte)value);
                        state=(state==0&&value==13)?1:(state==1&&value==10)?2:(state==2&&value==13)?3:(state==3&&value==10)?4:0;
                    }
                    if(state!=4) throw new IOException("Owned server header bound");
                    string header=Encoding.ASCII.GetString(bytes.ToArray());
                    var frozen=index<2?null:index==10?UiArtifactResource.Resource:FrozenWebResources.Find(FrozenWebResources.Sequence[index-2]);
                    string route=index==0?"/form.html":index==1?"/form.js":frozen.Route;
                    string expected="GET "+route+" HTTP/1.1\r\nHost: 127.0.0.1:"+((IPEndPoint)listener.LocalEndpoint).Port.ToString(System.Globalization.CultureInfo.InvariantCulture)+"\r\nConnection: close\r\n\r\n";
                    if(!String.Equals(header,expected,StringComparison.Ordinal)) throw new IOException("Owned server request boundary differs");
                    byte[] body=frozen==null?Encoding.UTF8.GetBytes(index==0?WebDomContract.FormHtml:WebDomContract.FormJavaScript):frozen.Bytes;
                    string type=frozen==null?(index==0?"text/html; charset=utf-8":"text/javascript; charset=utf-8"):frozen.ContentType;
                    string status=frozen==null?"200 OK":frozen.Status+" "+frozen.Reason;
                    byte[] response=Encoding.ASCII.GetBytes("HTTP/1.1 "+status+"\r\nConnection: close\r\nContent-Length: "+body.Length.ToString(System.Globalization.CultureInfo.InvariantCulture)+"\r\nContent-Type: "+type+"\r\nX-Content-Type-Options: nosniff\r\n\r\n");
                    stream.Write(response,0,response.Length); stream.Write(body,0,body.Length); stream.Flush();
                    lock(gate) requests++;
                }
            } catch(Exception error) { lock(gate) { if(!disposed) throw new IOException("Owned server failed",error); } }
        }
        public byte[] Fetch(string id) {
            var frozen=id=="form" || id=="script"?null:id=="ui-artifact"&&UiArtifactResource.Enabled?UiArtifactResource.Resource:FrozenWebResources.Find(id);
            string route=id=="form"?"/form.html":id=="script"?"/form.js":frozen.Route;
            if(route==null) throw new IOException("Unknown broker resource");
            var endpoint=(IPEndPoint)listener.LocalEndpoint;
            using(var client=new TcpClient(AddressFamily.InterNetwork)) {
                client.ReceiveTimeout=3000; client.SendTimeout=3000; client.Connect(IPAddress.Loopback,endpoint.Port);
                using(var stream=client.GetStream()) {
                    byte[] request=Encoding.ASCII.GetBytes("GET "+route+" HTTP/1.1\r\nHost: 127.0.0.1:"+endpoint.Port.ToString(System.Globalization.CultureInfo.InvariantCulture)+"\r\nConnection: close\r\n\r\n");
                    stream.Write(request,0,request.Length); stream.Flush();
                    var response=new MemoryStream(); var buffer=new byte[4096]; int count;
                    while((count=stream.Read(buffer,0,buffer.Length))!=0) { if(response.Length+count>WebDomContract.MaxResourceBytes+4096) throw new IOException("Owned server response bound"); response.Write(buffer,0,count); }
                    byte[] all=response.ToArray(); byte[] marker=Encoding.ASCII.GetBytes("\r\n\r\n"); int split=-1;
                    for(int i=0;i<=all.Length-marker.Length;i++) if(all[i]==13&&all[i+1]==10&&all[i+2]==13&&all[i+3]==10){split=i;break;}
                    if(split<0) throw new IOException("Owned server response headers absent");
                    string headers=Encoding.ASCII.GetString(all,0,split+4);
                    byte[] body=new byte[all.Length-split-4]; Buffer.BlockCopy(all,split+4,body,0,body.Length);
                    byte[] expected=frozen==null?Encoding.UTF8.GetBytes(id=="form"?WebDomContract.FormHtml:WebDomContract.FormJavaScript):frozen.Bytes;
                    string contentType=frozen==null?(id=="form"?"text/html; charset=utf-8":"text/javascript; charset=utf-8"):frozen.ContentType;
                    string status=frozen==null?"200 OK":frozen.Status+" "+frozen.Reason;
                    string exact="HTTP/1.1 "+status+"\r\nConnection: close\r\nContent-Length: "+expected.Length.ToString(System.Globalization.CultureInfo.InvariantCulture)+"\r\nContent-Type: "+contentType+"\r\nX-Content-Type-Options: nosniff\r\n\r\n";
                    if(!String.Equals(headers,exact,StringComparison.Ordinal) || !body.SequenceEqual(expected)) throw new IOException("Owned server response differs from frozen resource");
                    return body;
                }
            }
        }
        public void RequireComplete(int expected) {
            if(expected==2) { if(Requests!=2) throw new IOException("Diagnostic server request count differs"); Dispose(); return; }
            if(expected!=(UiArtifactResource.Enabled?11:10) || !serving.Wait(3000)) throw new TimeoutException("Owned server completion deadline");
            if(serving.IsFaulted) serving.GetAwaiter().GetResult();
            if(Requests!=expected || !idleClient.Connected || !idleAccepted.Connected || idleClient.Client.Poll(0,SelectMode.SelectRead) || idleAccepted.Client.Poll(0,SelectMode.SelectRead)) throw new IOException("Owned server request/idle-connection contract differs");
        }
        public void Dispose() {
            lock(gate) { disposed=true; if(activeClient!=null) activeClient.Dispose(); }
            idleClient.Dispose(); idleAccepted.Dispose(); listener.Stop();
            if(!serving.Wait(3000)) throw new IOException("Owned server task did not drain");
            serving.GetAwaiter().GetResult();
        }
    }
    public static void Run(string root,string profileName,string expectedSid,string runtime,string nonce,string[] imagePaths,string[] imageHashes,long completionPort,IntPtr owner) {
        Name(profileName); CheckLayouts();
        string expectedRoot=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"Packages",profileName,"AC");
        if(!String.Equals(Path.GetFullPath(root),expectedRoot,StringComparison.OrdinalIgnoreCase) || expectedSid!=DeriveSid(profileName) || !System.Text.RegularExpressions.Regex.IsMatch(nonce,"\\A[a-f0-9]{64}\\z")) throw new IOException("Owned root/SID/nonce mismatch");
        if(imagePaths.Length!=imageHashes.Length || imagePaths.Length>4096) throw new IOException("Image inventory bound");
        var images=new Dictionary<string,string>(StringComparer.OrdinalIgnoreCase);
        for(int i=0;i<imagePaths.Length;i++) { if(!System.Text.RegularExpressions.Regex.IsMatch(imageHashes[i],"\\A[a-f0-9]{64}\\z")) throw new IOException("Image hash malformed"); images.Add(Path.GetFullPath(imagePaths[i]),imageHashes[i]); }
        RegularTree(root);
        string executable=Path.Combine(root,"host","WebViewHost.exe"), profile=Path.Combine(root,"profile"), temp=Path.Combine(root,"probe-temp");
        if(WaitForSingleObject(owner,0)!=258) throw new IOException("Controller lost before launch");
        IntPtr sid=IntPtr.Zero,job=IntPtr.Zero,list=IntPtr.Zero,handles=IntPtr.Zero,caps=IntPtr.Zero,jobList=IntPtr.Zero,limits=IntPtr.Zero,env=IntPtr.Zero,association=IntPtr.Zero;
        IntPtr port=new IntPtr(completionPort); if(port==IntPtr.Zero || port==new IntPtr(-1)) throw new IOException("No independent completion port");
        IntPtr childIn=IntPtr.Zero,childOut=IntPtr.Zero,childErr=IntPtr.Zero;
        FileStream input=null,output=null,error=null; SafeFileHandle inputHandle=null,outputHandle=null,errorHandle=null;
        var process=new ProcessInfo(); bool initialized=false; Timer deadline=null; BoundedCapture stderr=null; OwnedServer server=null; ProcessCollector collector=null;
        var observed=new Dictionary<uint,IntPtr>(); var reported=new HashSet<uint>();
        var lines=new BlockingCollection<string>(32); Task reader=null; ExceptionDispatchInfo primary=null; var cleanupFailures=new List<string>();
        string phase="prepare_native_launch"; bool ready=false,stopSent=false,hostClosed=false,hostStopped=false; uint readyPid=0;
        var dom = new DomEvidence();
        var web = new FrozenWebEvidence();
        var ui = new UiArtifactEvidence();
        var inputDiagnostic = new InputDiagnosticEvidence();
        string evidenceRoute = null;
        var deferredExitedImages=new Dictionary<uint,long>();
        var clock=new Stopwatch();
        Action collect=()=> {
            if(collector==null) return;
            foreach(var item in collector.Snapshot()) {
                IntPtr known;
                if(observed.TryGetValue(item.Pid,out known)) {
                    if(Creation(known)!=item.Created) throw new IOException("Collected process creation identity differs");
                    continue;
                }
                long deferredCreation;
                if(deferredExitedImages.TryGetValue(item.Pid,out deferredCreation) && deferredCreation!=item.Created) throw new IOException("Deferred collected identity differs");
                if(item.Image==IntPtr.Zero) {
                    if(!deferredExitedImages.ContainsKey(item.Pid)) {
                        VerifyToken(item.Process,job,expectedSid,item.Pid);
                        deferredExitedImages.Add(item.Pid,item.Created);
                        Event(new {type="exited_image_query_deferred",pid=item.Pid,creation_filetime=item.Created,verified=false,source="independent_handle_collector"});
                    }
                    continue;
                }
                VerifyToken(item.Process,job,expectedSid,item.Pid);
                string image=HeldImage(item.Image,item.Pid,item.Created,images);
                IntPtr copy; Check(DuplicateHandle(GetCurrentProcess(),item.Process,GetCurrentProcess(),out copy,0,false,2));
                observed.Add(item.Pid,copy); deferredExitedImages.Remove(item.Pid);
                Event(new {type="owned_process",pid=item.Pid,creation_filetime=item.Created,image,token_verified=true,source="independent_handle_collector"});
            }
        };
        Action observe=()=> {
            if(WaitForSingleObject(owner,0)!=258) throw new IOException("Controller owner lost");
            collect();
            Scratch(profile,"profile"); Scratch(temp,"probe-temp");
            foreach(uint pid in JobPids(job)) {
                if(observed.ContainsKey(pid)) { if(WaitForSingleObject(observed[pid],0)==0) throw new IOException("Numeric PID reused within job census"); continue; }
                if(deferredExitedImages.ContainsKey(pid)) continue;
                IntPtr held=OpenProcess(0x100400,false,pid); Check(held!=IntPtr.Zero);
                try {
                    VerifyToken(held,job,expectedSid,pid); string image;
                    try { image=Image(held,pid,images); }
                    catch(Win32Exception errorValue) {
                        // Image-query access can disappear during process teardown
                        // just before its process object becomes signaled. Wait
                        // once for at most 50 ms; a still-live identity fails.
                        if(!ProbeContract.CanDeferExitedImage(errorValue.NativeErrorCode,WaitForSingleObject(held,errorValue.NativeErrorCode==5?50u:0u))) throw;
                        long creation=Creation(held); deferredExitedImages.Add(pid,creation);
                        Event(new {type="exited_image_query_deferred",pid,creation_filetime=creation,verified=false});
                        CloseHandle(held); continue;
                    }
                    observed.Add(pid,held); Event(new { type="owned_process",pid,creation_filetime=Creation(held),image,token_verified=true });
                } catch { CloseHandle(held); throw; }
            }
            ProbeContract.LiveCoverage(Accounts(job).Total,observed.Count);
        };
        try {
            server=new OwnedServer(); Event(new { type="owned_server_started",origin=server.Origin,network_scope="exact_ipv4_loopback_endpoint",browser_network_capabilities=0 });
            Check(ConvertStringSidToSid(expectedSid,out sid));
            input=Pipe(true,out childIn); output=Pipe(false,out childOut); error=Pipe(false,out childErr);
            inputHandle=input.SafeFileHandle; outputHandle=output.SafeFileHandle; errorHandle=error.SafeFileHandle;
            stderr=new BoundedCapture(error,"host_stderr");
            reader=Task.Run(()=> {
                try {
                    using(var text=new StreamReader(output,Utf8,false,4096,true)) {
                        var line=new StringBuilder(); int value,count=0; long total=0;
                        while((value=text.Read())!=-1) {
                            if(value=='\n') {
                                if(++count>384 || !lines.TryAdd(line.ToString().TrimEnd('\r'),1000)) throw new IOException("Host event count/queue ceiling");
                                line.Clear();
                            } else { line.Append((char)value); if(line.Length>2048) throw new IOException("Host line ceiling"); }
                            if(++total>196608) throw new IOException("Host transport ceiling");
                        }
                        if(line.Length!=0) throw new IOException("Incomplete host line");
                    }
                } finally { lines.CompleteAdding(); }
            });
            job=CreateJobObject(IntPtr.Zero,null); Check(job!=IntPtr.Zero);
            association=Structure(new JobCompletion { Key=new IntPtr(1),Port=port }); Check(SetInformationJobObject(job,7,association,(uint)Marshal.SizeOf<JobCompletion>()));
            limits=Structure(new ExtendedLimits { Basic=new BasicLimits { Flags=0x2308,ActiveProcesses=32 },ProcessMemory=new UIntPtr(1073741824UL),JobMemory=new UIntPtr(2147483648UL) });
            Check(SetInformationJobObject(job,9,limits,(uint)Marshal.SizeOf<ExtendedLimits>()));
            IntPtr size=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,3,0,ref size); if(size==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            list=Marshal.AllocHGlobal(size); Check(InitializeProcThreadAttributeList(list,3,0,ref size)); initialized=true;
            handles=Marshal.AllocHGlobal(3*IntPtr.Size); Marshal.Copy(new[]{childIn,childOut,childErr},0,handles,3);
            Check(UpdateProcThreadAttribute(list,0,new IntPtr(0x20002),handles,new IntPtr(3*IntPtr.Size),IntPtr.Zero,IntPtr.Zero));
            caps=Structure(new Capabilities { Sid=sid }); Check(UpdateProcThreadAttribute(list,0,new IntPtr(0x20009),caps,new IntPtr(Marshal.SizeOf<Capabilities>()),IntPtr.Zero,IntPtr.Zero));
            jobList=Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobList,job); Check(UpdateProcThreadAttribute(list,0,new IntPtr(0x2000d),jobList,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero));
            string system=Environment.GetFolderPath(Environment.SpecialFolder.System),windows=Directory.GetParent(system).FullName;
            env=Marshal.StringToHGlobalUni(String.Join("\0",new[]{"APPDATA="+profile,"LOCALAPPDATA="+profile,"PATH="+system,"SYSTEMROOT="+windows,"TEMP="+temp,"TMP="+temp,"USERPROFILE="+profile,"WINDIR="+windows})+"\0\0");
            string[] args={executable,"--runtime",runtime,"--profile",profile,"--version",ProbeContract.Version,"--nonce",nonce};
            var startup=new StartupEx { Startup=new Startup { Size=Marshal.SizeOf<StartupEx>(),Flags=0x100,Input=childIn,Output=childOut,Error=childErr },Attributes=list };
            IntPtr ownedJob=job; deadline=new Timer(_=>TerminateJobObject(ownedJob,90),null,90000,Timeout.Infinite);
            phase="CreateProcess";
            Check(CreateProcess(executable,new StringBuilder(String.Join(" ",args.Select(Quote))),IntPtr.Zero,IntPtr.Zero,true,0x08080404u,env,profile,ref startup,out process));
            foreach(IntPtr h in new[]{childIn,childOut,childErr}) CloseHandle(h); childIn=childOut=childErr=IntPtr.Zero;
            VerifyToken(process.Process,job,expectedSid,process.Pid); Image(process.Process,process.Pid,images);
            collector=new ProcessCollector(job,images);
            // Hold a separate process handle for census identity; initial handle
            // remains independently owned by ProcessInfo and controller handshake.
            observe();
            Event(new { type="created_suspended",pid=process.Pid,creation_filetime=Creation(process.Process),atomic_job_assignment=true });
            phase="controller_resume_handshake";
            if(Console.ReadLine()!="RESUME") throw new IOException("Controller closed before resume");
            if(ResumeThread(process.Thread)==UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error()); clock.Start();
            phase="startup_observation";
            while(true) {
                if(clock.ElapsedMilliseconds>=20000) throw new TimeoutException("Startup ceiling");
                if(stderr.Reading.IsFaulted) stderr.Reading.GetAwaiter().GetResult();
                if(reader.IsFaulted) reader.GetAwaiter().GetResult();
                observe();
                string line;
                while(lines.TryTake(out line)) {
                    var item=ProbeContract.ParseRetainingRejected(line,nonce,Diagnostic); Event(new { type="host_observation",line });
                    // Draining the bounded host queue can take much longer than
                    // the outer 1 ms census interval when DOM evidence arrives
                    // in chunks. Re-census for every bounded host event so a
                    // short-lived WebView helper cannot disappear while the
                    // supervisor is busy parsing evidence.
                    // The host report remains corroborating evidence only: the
                    // independently opened handle, token, job membership, image
                    // path and image hash are still authoritative.
                    observe();
                    if(item.Phase=="reported_process" && !observed.ContainsKey(item.Pid)) {
                        string[] identity=item.Kind.Split(':'); long remoteValue,remoteImage;
                        if(identity.Length!=3 || !new[]{"Browser","Renderer","Gpu","Utility"}.Contains(identity[0]) || !Int64.TryParse(identity[1],System.Globalization.NumberStyles.AllowHexSpecifier,System.Globalization.CultureInfo.InvariantCulture,out remoteValue) || !Int64.TryParse(identity[2],System.Globalization.NumberStyles.AllowHexSpecifier,System.Globalization.CultureInfo.InvariantCulture,out remoteImage) || remoteValue==0 || remoteImage==0) throw new IOException("Host-held process handle identity differs");
                        IntPtr held,imageHandle; Check(DuplicateHandle(process.Process,new IntPtr(remoteValue),GetCurrentProcess(),out held,0,false,2));
                        try { Check(DuplicateHandle(process.Process,new IntPtr(remoteImage),GetCurrentProcess(),out imageHandle,0,false,2)); } catch { CloseHandle(held); throw; }
                        try {
                            if(GetProcessId(held)!=item.Pid) throw new IOException("Host-held process handle resolves to another PID");
                            long deferredCreation;
                            if(deferredExitedImages.TryGetValue(item.Pid,out deferredCreation) && Creation(held)!=deferredCreation) throw new IOException("Deferred process creation identity differs");
                            VerifyToken(held,job,expectedSid,item.Pid); string image=HeldImage(imageHandle,item.Pid,Creation(held),images);
                            observed.Add(item.Pid,held); held=IntPtr.Zero;
                            deferredExitedImages.Remove(item.Pid);
                            Event(new { type="owned_process",pid=item.Pid,creation_filetime=Creation(observed[item.Pid]),image,token_verified=true,source="duplicated_host_hold" });
                        } finally { if(held!=IntPtr.Zero) CloseHandle(held); CloseHandle(imageHandle); }
                    }
                    if(item.Phase=="broker_request") {
                        byte[] served=server.Fetch(item.Kind); string hash;
                        using(var algorithm=SHA256.Create()) hash=Convert.ToHexString(algorithm.ComputeHash(served)).ToLowerInvariant();
                        byte[] response=Utf8.GetBytes("RESOURCE "+nonce+" "+item.Kind+" "+hash+"\n");
                        input.Write(response,0,response.Length); input.Flush();
                        Event(new { type="owned_server_relay",resource=item.Kind,bytes=served.Length,sha256=hash });
                    }
                    if (item.Phase=="dom_chunk" || item.Phase=="dom_complete") {
                        if (evidenceRoute!=null && evidenceRoute!="dom") throw new IOException("Mixed host evidence routes");
                        evidenceRoute="dom";
                        if(item.Phase=="dom_chunk") dom.Add(item.Kind); else dom.Finish(item.Kind);
                    }
                    if (item.Phase=="input_chunk" || item.Phase=="input_complete") {
                        if (evidenceRoute!=null && evidenceRoute!="input_diagnostic") throw new IOException("Mixed host evidence routes");
                        evidenceRoute="input_diagnostic";
                        if(item.Phase=="input_chunk") inputDiagnostic.Add(item.Kind); else inputDiagnostic.Finish(item.Kind);
                    }
                    if(item.Phase=="web_chunk" || item.Phase=="web_complete") {
                        if(!dom.Complete) throw new IOException("WEB preceded synthetic qualification");
                        if(item.Phase=="web_chunk") web.Add(item.Kind); else web.Finish(item.Kind);
                    }
                    if(item.Phase=="ui_chunk" || item.Phase=="ui_complete" || item.Phase=="ui_failure") {
                        if(!UiArtifactResource.Enabled || !web.Complete)throw new IOException("Unexpected UI artifact evidence");
                        if(item.Phase=="ui_chunk")ui.Add(item.Kind);else if(item.Phase=="ui_complete")ui.Finish(item.Kind);
                    }
                    if(item.Pid!=0) reported.Add(item.Pid);
                    if(item.Phase=="controller_ready") {
                        if(ready) throw new IOException("Duplicate readiness");
                        if(item.Kind=="browser" && (evidenceRoute!="dom" || !dom.Complete || !web.Complete)) throw new IOException("Incomplete independent DOM/WEB evidence");
                        if(UiArtifactResource.Enabled&&!ui.Complete)throw new IOException("Incomplete UI candidate evidence");
                        if(item.Kind=="input_diagnostic" && (evidenceRoute!="input_diagnostic" || !inputDiagnostic.Complete)) throw new IOException("Incomplete independent input diagnostic evidence");
                        ready=true; readyPid=item.Pid;
                    }
                    if(item.Phase=="stop_received") { if(!stopSent) throw new IOException("Host STOP without supervisor command"); hostStopped=true; }
                    if(item.Phase=="controller_closed") { if(!hostStopped) throw new IOException("Host closed before STOP"); hostClosed=true; }
                }
                if(ready && !stopSent) {
                    observe();
                    if(deferredExitedImages.Count!=0) throw new IOException("Exited process image identity remains unresolved");
                    server.RequireComplete(evidenceRoute=="dom"?(UiArtifactResource.Enabled?11:10):2);
                    if(evidenceRoute=="dom") Event(new { type="owned_server_complete",requests=server.Requests,unexpected_requests=0,unrelated_connection_open=true,readiness_path="/ready",readiness_status=204 });
                    else Event(new { type="owned_server_complete",requests=server.Requests,unexpected_requests=0 });
                    foreach(uint pid in reported) if(!observed.ContainsKey(pid)) throw new IOException("Host-reported PID lacks independent held identity");
                    if(!observed.ContainsKey(readyPid) || WaitForSingleObject(observed[readyPid],0)!=258) throw new IOException("Ready browser is not independently live");
                    byte[] stop=Utf8.GetBytes("STOP "+nonce+"\n"); input.Write(stop,0,stop.Length); input.Flush(); stopSent=true;
                }
                if(WaitForSingleObject(process.Process,0)==0 && reader.IsCompleted && lines.Count==0) break;
                // Short-lived WebView2 helpers can complete inside a 10 ms
                // polling gap. Keep the controller responsive while retaining
                // the fixed outer deadline and exact final process accounting.
                Thread.Sleep(1);
            }
            uint exit; Check(GetExitCodeProcess(process.Process,out exit));
            if(exit!=0 || !ready || !hostStopped || !hostClosed) throw new IOException("Incomplete host startup/STOP outcome");
            foreach(uint pid in reported) if(!observed.ContainsKey(pid)) throw new IOException("Reported helper not independently verified");
            if(evidenceRoute=="dom") { Event(new { type="frozen_web_observed",manifest_sha256=FrozenWebResources.ManifestSha256,documents=web.Documents,denied_navigation_attempts=1,visual_review="not_run" }); Event(new { type="dom_observed",browser_qualification=false,production_profile_qualified=false,prototype_only=true,documents=dom.Documents,version=ProbeContract.Version,verified_processes=observed.Count }); }
            else if(evidenceRoute=="input_diagnostic") Event(new { type="input_diagnostic_observed",target_verified=inputDiagnostic.TargetVerified,text_inserted=inputDiagnostic.TextInserted,focus_value=inputDiagnostic.FocusValue,text_value=inputDiagnostic.TextValue,key_value=inputDiagnostic.KeyValue,key_delivered=inputDiagnostic.KeyDelivered,key_downs=inputDiagnostic.KeyDowns,key_presses=inputDiagnostic.KeyPresses,key_ups=inputDiagnostic.KeyUps,submits=inputDiagnostic.Submits,submission_observed=inputDiagnostic.SubmissionObserved,status=inputDiagnostic.Status,browser_qualification=false,production_profile_qualified=false,prototype_only=true,documents=inputDiagnostic.Documents,version=ProbeContract.Version,verified_processes=observed.Count });
            else throw new IOException("No completed host evidence route");
            if(UiArtifactResource.Enabled)Event(new {type="ui_artifact_observed",case_id=UiArtifactResource.CaseId,artifact_sha256=UiArtifactResource.ArtifactSha256,status=ui.Passed?"passed":"failed",documents=ui.Documents,assertions=ui.Assertions,visual_review="not_run"});
        } catch(Exception errorValue) {
            primary=ExceptionDispatchInfo.Capture(errorValue); Diagnostic(new { type="primary_failure",phase,exception=Bounded(errorValue.ToString(),2048),hresult=errorValue.HResult });
        } finally {
            if(server!=null) Cleanup("owned_server_stop",()=>{server.Dispose(); Event(new { type="owned_server_stopped",requests=server.Requests,listener_closed=true });},cleanupFailures);
            if(deadline!=null) Cleanup("deadline_drain",()=>{using(var done=new ManualResetEvent(false)){deadline.Dispose(done); if(!done.WaitOne(5000)) throw new IOException("Deadline callback drain failed");}},cleanupFailures);
            if(job!=IntPtr.Zero) {
                Cleanup("terminate_and_drain_job",()=> {
                    Check(TerminateJobObject(job,1)); var wait=Stopwatch.StartNew();
                    while(JobPids(job).Length!=0 && wait.ElapsedMilliseconds<10000) Thread.Sleep(10);
                    if(JobPids(job).Length!=0) throw new IOException("Job remains active");
                    if(collector!=null) collector.Stop();
                    Cleanup("final_collected_identities",collect,cleanupFailures);
                    var counts=Accounts(job);
                    Cleanup("final_process_coverage",()=>ProbeContract.Coverage(counts.Total,observed.Count),cleanupFailures);
                    Diagnostic(new { type="job_process_coverage",total_processes=counts.Total,verified_identities=observed.Count,complete=counts.Total==observed.Count });
                    Event(new { type="job_empty_waiting_for_ack" }); WaitForDrainAcknowledgement(owner);
                    Diagnostic(new { type="job_empty",independent_notification_acknowledged=true });
                },cleanupFailures);
                if(collector!=null) Cleanup("process_collector_dispose",()=>collector.Dispose(),cleanupFailures);
                Cleanup("close_job",()=>Check(CloseHandle(job)),cleanupFailures); job=IntPtr.Zero;
            }
            Cleanup("host_exit",()=>ProcessExit(process.Process,"after_cleanup"),cleanupFailures);
            foreach(IntPtr h in new[]{childIn,childOut,childErr}) if(h!=IntPtr.Zero) Cleanup("child_pipe",()=>Check(CloseHandle(h)),cleanupFailures);
            if(reader!=null) Cleanup("host_output_drain",()=>{if(!reader.Wait(5000)) throw new IOException("Host reader drain deadline");},cleanupFailures);
            if(primary!=null || cleanupFailures.Count!=0) ProbeContract.FailedHostDiagnostics(lines,Diagnostic);
            if(stderr!=null) { Cleanup("host_stderr_drain",()=>{if(!stderr.Reading.Wait(5000)) throw new IOException("Host stderr drain deadline");},cleanupFailures); stderr.Publish(); }
            foreach(FileStream stream in new[]{input,output,error}) if(stream!=null) Cleanup("dispose_stream",()=>stream.Dispose(),cleanupFailures);
            foreach(SafeFileHandle h in new[]{inputHandle,outputHandle,errorHandle}) if(h!=null) Cleanup("dispose_pipe_handle",()=>h.Dispose(),cleanupFailures);
            foreach(IntPtr h in observed.Values) Cleanup("close_observed",()=>Check(CloseHandle(h)),cleanupFailures);
            foreach(IntPtr h in new[]{process.Process,process.Thread}) if(h!=IntPtr.Zero) Cleanup("close_initial",()=>Check(CloseHandle(h)),cleanupFailures);
            if(initialized) Cleanup("delete_attributes",()=>DeleteProcThreadAttributeList(list),cleanupFailures);
            foreach(IntPtr p in new[]{list,handles,caps,jobList,limits,env,association}) if(p!=IntPtr.Zero) Cleanup("free_storage",()=>Marshal.FreeHGlobal(p),cleanupFailures);
            if(sid!=IntPtr.Zero) Cleanup("free_sid",()=>{if(LocalFree(sid)!=IntPtr.Zero) throw new IOException("SID release failed");},cleanupFailures);
            Cleanup("close_completion_port",()=>Check(CloseHandle(port)),cleanupFailures);
        }
        if(primary!=null) primary.Throw();
        if(cleanupFailures.Count!=0) throw new IOException("Cleanup/coverage incomplete: "+String.Join("; ",cleanupFailures));
    }
}
}
