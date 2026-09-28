// SPDX-License-Identifier: Apache-2.0
// Startup-only adaptation of the corrected independent CS-3 worker.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
namespace Vcp.Cs3Draft {
public static partial class NativeProbe {
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder name,ref uint size);
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
    public static void CheckLayouts() {
        if(IntPtr.Size!=8 || Marshal.SizeOf<StartupEx>()!=112 || Marshal.SizeOf<Capabilities>()!=24 || Marshal.SizeOf<ExtendedLimits>()!=144 || Marshal.SizeOf<Accounting>()!=48) throw new IOException("Unexpected x64 layouts");
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
        var process=new ProcessInfo(); bool initialized=false; Timer deadline=null; BoundedCapture stderr=null;
        var observed=new Dictionary<uint,IntPtr>(); var reported=new HashSet<uint>();
        var lines=new BlockingCollection<string>(32); Task reader=null; ExceptionDispatchInfo primary=null; var cleanupFailures=new List<string>();
        string phase="prepare_native_launch"; bool ready=false,stopSent=false,hostClosed=false,hostStopped=false; uint readyPid=0;
        var dom = new DomEvidence();
        var inputDiagnostic = new InputDiagnosticEvidence();
        string evidenceRoute = null;
        var clock=new Stopwatch();
        Action observe=()=> {
            if(WaitForSingleObject(owner,0)!=258) throw new IOException("Controller owner lost");
            Scratch(profile,"profile"); Scratch(temp,"probe-temp");
            foreach(uint pid in JobPids(job)) {
                if(observed.ContainsKey(pid)) { if(WaitForSingleObject(observed[pid],0)==0) throw new IOException("Numeric PID reused within job census"); continue; }
                IntPtr held=OpenProcess(0x100400,false,pid); Check(held!=IntPtr.Zero);
                try {
                    VerifyToken(held,job,expectedSid,pid); string image=Image(held,pid,images);
                    observed.Add(pid,held); Event(new { type="owned_process",pid,creation_filetime=Creation(held),image,token_verified=true });
                } catch { CloseHandle(held); throw; }
            }
            ProbeContract.LiveCoverage(Accounts(job).Total,observed.Count);
        };
        try {
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
                    if(item.Pid!=0) reported.Add(item.Pid);
                    if(item.Phase=="controller_ready") {
                        if(ready) throw new IOException("Duplicate readiness");
                        if(item.Kind=="browser" && (evidenceRoute!="dom" || !dom.Complete)) throw new IOException("Incomplete independent DOM evidence");
                        if(item.Kind=="input_diagnostic" && (evidenceRoute!="input_diagnostic" || !inputDiagnostic.Complete)) throw new IOException("Incomplete independent input diagnostic evidence");
                        ready=true; readyPid=item.Pid;
                    }
                    if(item.Phase=="stop_received") { if(!stopSent) throw new IOException("Host STOP without supervisor command"); hostStopped=true; }
                    if(item.Phase=="controller_closed") { if(!hostStopped) throw new IOException("Host closed before STOP"); hostClosed=true; }
                }
                if(ready && !stopSent) {
                    observe();
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
            if(evidenceRoute=="dom") Event(new { type="dom_observed",browser_qualification=false,production_profile_qualified=false,prototype_only=true,documents=dom.Documents,version=ProbeContract.Version,verified_processes=observed.Count });
            else if(evidenceRoute=="input_diagnostic") Event(new { type="input_diagnostic_observed",target_verified=inputDiagnostic.TargetVerified,text_inserted=inputDiagnostic.TextInserted,focus_value=inputDiagnostic.FocusValue,text_value=inputDiagnostic.TextValue,key_value=inputDiagnostic.KeyValue,key_delivered=inputDiagnostic.KeyDelivered,key_downs=inputDiagnostic.KeyDowns,key_presses=inputDiagnostic.KeyPresses,key_ups=inputDiagnostic.KeyUps,submits=inputDiagnostic.Submits,submission_observed=inputDiagnostic.SubmissionObserved,status=inputDiagnostic.Status,browser_qualification=false,production_profile_qualified=false,prototype_only=true,documents=inputDiagnostic.Documents,version=ProbeContract.Version,verified_processes=observed.Count });
            else throw new IOException("No completed host evidence route");
        } catch(Exception errorValue) {
            primary=ExceptionDispatchInfo.Capture(errorValue); Diagnostic(new { type="primary_failure",phase,exception=Bounded(errorValue.ToString(),2048),hresult=errorValue.HResult });
        } finally {
            if(deadline!=null) Cleanup("deadline_drain",()=>{using(var done=new ManualResetEvent(false)){deadline.Dispose(done); if(!done.WaitOne(5000)) throw new IOException("Deadline callback drain failed");}},cleanupFailures);
            if(job!=IntPtr.Zero) {
                Cleanup("terminate_and_drain_job",()=> {
                    Check(TerminateJobObject(job,1)); var wait=Stopwatch.StartNew();
                    while(JobPids(job).Length!=0 && wait.ElapsedMilliseconds<10000) Thread.Sleep(10);
                    if(JobPids(job).Length!=0) throw new IOException("Job remains active");
                    var counts=Accounts(job);
                    Cleanup("final_process_coverage",()=>ProbeContract.Coverage(counts.Total,observed.Count),cleanupFailures);
                    Diagnostic(new { type="job_process_coverage",total_processes=counts.Total,verified_identities=observed.Count,complete=counts.Total==observed.Count });
                    Event(new { type="job_empty_waiting_for_ack" }); WaitForDrainAcknowledgement(owner);
                    Diagnostic(new { type="job_empty",independent_notification_acknowledged=true });
                },cleanupFailures);
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
