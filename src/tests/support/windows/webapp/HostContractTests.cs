// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections;
using Vcp.Cs3WebViewDraft;
static class HostContractTests {
    static int checks;
    static void Check(bool ok) { if (!ok) throw new Exception("Offline assertion failed"); checks++; }
    static void Reject(Action action) { bool failed=false; try { action(); } catch { failed=true; } Check(failed); }
    static int Main() {
        string nonce=new string('a',64);
        string[] args={"--runtime",@"D:\runtime\154", "--profile",@"D:\private\new-profile", "--version","154.0.4258.37","--nonce",nonce};
        var parsed=HostInput.Parse(args); Check(parsed.Version=="154.0.4258.37");
        Reject(delegate { HostInput.Parse(new string[0]); });
        var more=(string[])args.Clone(); more[0]="--browser-arguments"; Reject(delegate { HostInput.Parse(more); });
        more=(string[])args.Clone(); more[3]=@"D:\runtime\154\profile"; Reject(delegate { HostInput.Parse(more); });
        Reject(delegate { HostInput.Absolute(@"\\server\share\profile"); });
        Reject(delegate { HostInput.Absolute("C:\\"); });
        Reject(delegate { HostInput.Absolute("C:\\temp\\p:stream"); });
        Reject(delegate { HostInput.Absolute("C:\\temp\\x\nsecret"); });
        more=(string[])args.Clone(); more[5]="154.0.4258.37 --no-sandbox"; Reject(delegate { HostInput.Parse(more); });
        HostInput.NoOverrides(new Hashtable { { "SystemRoot",@"C:\Windows" } }); checks++;
        foreach(string key in new[] {"WEBVIEW2_BROWSER_EXECUTABLE_FOLDER","WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS","WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER","webview2_unknown_future_override","COREWEBVIEW2_MAX_INSTANCES"}) { string name=key; Reject(delegate { HostInput.NoOverrides(new Hashtable {{name,""}}); }); }
        Check(HostInput.StopCommand("STOP "+nonce,nonce)); Check(!HostInput.StopCommand("STOP "+new string('b',64),nonce)); Check(!HostInput.StopCommand("STOP "+nonce+" ",nonce));
        Check(Evidence.Quote("a\n\"\\\u2028")=="\"a\\u000a\\\"\\\\\\u2028\"");
        string record=Evidence.Record(nonce,"creation_failed",25,unchecked((int)0x80070005),"154.0.4258.37",42,"browser");
        Check(record.Contains("0x80070005") && record.Contains("\"containment_attested\":false") && record.Contains("\"browser_qualified\":false"));
        Reject(delegate { Evidence.Record(nonce,"phase\nforged",0,0,"",0,""); });
        Reject(delegate { Evidence.Record(nonce,"phase",-1,0,"",0,""); });
        Reject(delegate { Evidence.Record(nonce,"phase",0,0,new string('x',257),0,""); });
        Check(Evidence.MaximumProcesses==32 && Evidence.StartupMilliseconds==20000);
        Check(HostProbeContract.CommandCount==11 && HostProbeContract.MaximumCumulativeResponseBytes==HostProbeContract.MaximumRecordBytes+20);
        var skippedSentinel=new HostProbeContract.CommandSequence();
        skippedSentinel.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,1);
        skippedSentinel.Complete("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{}",1);
        Reject(delegate { skippedSentinel.BeginRequest("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,2); });
        // The rejection callback runs while an earlier STOP is queued. Its
        // deferred cleanup callback must not be needed to prevent success.
        var rejected=new HostLifecycle();
        rejected.Fail("navigation_rejected",unchecked((int)0x80070005));
        Check(rejected.BeginStop("stop_received",0,true,100));
        Check(rejected.ExitCode==1 && rejected.FailureReason=="navigation_rejected");
        Check(!rejected.BeginStop("navigation_rejected",unchecked((int)0x80070005),false,101));
        Check(rejected.ExitCode==1);
        // STOP can be dispatched before the timer message at/after expiry.
        foreach(long elapsed in new long[] {Evidence.StartupMilliseconds,Evidence.StartupMilliseconds+1}) {
            var expired=new HostLifecycle();
            Check(expired.BeginStop("stop_received",0,true,elapsed));
            Check(expired.ExitCode==1 && expired.FailureReason=="deadline");
        }
        // A canceled request can arrive during Close, after cleanup began.
        var closing=new HostLifecycle();
        Check(closing.BeginStop("stop_received",0,true,100)); Check(closing.ExitCode==0);
        closing.Fail("permission_rejected",unchecked((int)0x80070005));
        Check(closing.ExitCode==1 && closing.FailureReason=="permission_rejected");
        Check(!closing.BeginStop("permission_rejected",unchecked((int)0x80070005),false,101));
        Check(closing.ExitCode==1);
        closing.Fail("close_failed",unchecked((int)0x80004005));
        Check(closing.FailureReason=="permission_rejected");
        var timely=new HostLifecycle();
        Check(timely.BeginStop("stop_received",0,true,Evidence.StartupMilliseconds-1));
        Check(timely.ExitCode==0 && !timely.Failed);
        var active=new HostLifecycle(); HostProbeContract.RequireActive(active,0); HostProbeContract.RequireActive(active,Evidence.StartupMilliseconds-1); checks+=2;
        Reject(delegate { HostProbeContract.RequireActive(active,-1); });
        Reject(delegate { HostProbeContract.RequireActive(active,Evidence.StartupMilliseconds); });
        var inactive=new HostLifecycle(); inactive.Fail("fixed_failure",unchecked((int)0x80004005));
        Reject(delegate { HostProbeContract.RequireActive(inactive,1); });
        var stopped=new HostLifecycle(); stopped.BeginStop("stop_received",0,true,1);
        Reject(delegate { HostProbeContract.RequireActive(stopped,1); });

        var sequence=new HostProbeContract.CommandSequence();
        Reject(delegate { sequence.BeginRequest("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,1); });
        sequence.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,1);
        Reject(delegate { sequence.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,1); });
        Reject(delegate { sequence.Complete("Input.dispatchKeyEvent",HostProbeContract.ReadinessDownParameters,"{}",1); });
        sequence.Complete("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{}",1);
        Action<string,string,string,long> command=(method,parameters,response,elapsed)=>{sequence.BeginRequest(method,parameters,elapsed);sequence.Complete(method,parameters,response,elapsed);};
        command("Input.dispatchKeyEvent",HostProbeContract.ReadinessDownParameters,"{}",2);
        command("Input.dispatchKeyEvent",HostProbeContract.ReadinessUpParameters,"{}",3);
        command("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,"{}",4);
        command("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,"{}",5);
        command("Input.insertText",HostProbeContract.InsertParameters,"{}",6);
        command("Input.dispatchKeyEvent",HostProbeContract.ReadinessDownParameters,"{}",7);
        command("Input.dispatchKeyEvent",HostProbeContract.ReadinessUpParameters,"{}",8);
        command("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,"{}",9);
        command("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,"{}",10);
        command("Accessibility.getFullAXTree",HostProbeContract.AxParameters,"{\"nodes\":[]}",11);
        sequence.RequireComplete(); Check(sequence.Completed==HostProbeContract.CommandCount);
        Reject(delegate { sequence.BeginRequest("Accessibility.getFullAXTree",HostProbeContract.AxParameters,12); });
        var late=new HostProbeContract.CommandSequence();
        Reject(delegate { late.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,Evidence.StartupMilliseconds); });
        var lateCompletion=new HostProbeContract.CommandSequence(); lateCompletion.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,1);
        Reject(delegate { lateCompletion.Complete("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{}",Evidence.StartupMilliseconds); });
        var badResponse=new HostProbeContract.CommandSequence(); badResponse.BeginRequest("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,1);
        Reject(delegate { badResponse.Complete("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{\"unexpected\":true}",2); });
        Reject(delegate { badResponse.RequireComplete(); });
        Reject(delegate { new HostProbeContract.CommandSequence().RequireComplete(); });

        var routing=new InputRoutingCommands();
        Reject(delegate { routing.BeginRequest("Input.insertText",HostProbeContract.InsertParameters,1); });
        Reject(delegate { routing.BeginRequest("Runtime.evaluate","{}",1); });
        Reject(delegate { routing.BeginRequest("Runtime.evaluate",InputRoutingCommands.TargetParameters,20000); });
        routing.BeginRequest("Runtime.evaluate",InputRoutingCommands.TargetParameters,1);
        Reject(delegate { routing.BeginRequest("Runtime.evaluate",InputRoutingCommands.TargetParameters,1); });
        routing.Complete("Runtime.evaluate",InputRoutingCommands.TargetParameters,"{\"result\":{}}",1);
        Action<string,string,string,long> route=(method,parameters,response,elapsed)=>{routing.BeginRequest(method,parameters,elapsed);routing.Complete(method,parameters,response,elapsed);};
        route("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{}",2);
        route("Input.insertText",HostProbeContract.InsertParameters,"{}",3);
        route("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,"{}",4);
        route("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,"{}",5);
        routing.RequireComplete(); checks++;
        Reject(delegate { routing.BeginRequest("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,6); });
        var badRouting=new InputRoutingCommands(); badRouting.BeginRequest("Runtime.evaluate",InputRoutingCommands.TargetParameters,1);
        Reject(delegate { badRouting.Complete("Runtime.evaluate",InputRoutingCommands.TargetParameters,new string('x',8193),1); });
        Reject(delegate { badRouting.RequireComplete(); });
        Reject(delegate { new InputRoutingCommands().RequireComplete(); });
        var noInsert=new InputRoutingCommands(false);
        Action<string,string,string,long> noInsertCommand=(method,parameters,response,elapsed)=>{noInsert.BeginRequest(method,parameters,elapsed);noInsert.Complete(method,parameters,response,elapsed);};
        noInsertCommand("Runtime.evaluate",InputRoutingCommands.TargetParameters,"{\"result\":{}}",1);
        noInsertCommand("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters,"{}",2);
        Reject(delegate { noInsert.BeginRequest("Input.insertText",HostProbeContract.InsertParameters,3); });
        Reject(delegate { noInsert.BeginRequest("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,3); });
        noInsertCommand("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters,"{}",3);
        noInsertCommand("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,"{}",4);
        noInsert.RequireComplete(); checks++;
        Reject(delegate { noInsert.BeginRequest("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters,5); });
        Reject(delegate { new InputRoutingCommands(false).RequireComplete(); });
        var budget=new HostProbeContract.EvidenceBudget();
        Check(budget.Add(new string('a',HostProbeContract.MaximumRecordBytes)).Length==HostProbeContract.MaximumRecordBytes);
        Check(budget.Add(new string('b',HostProbeContract.MaximumRecordBytes)).Length==HostProbeContract.MaximumRecordBytes);
        Check(budget.TotalBytes==HostProbeContract.MaximumAggregateBytes);
        Reject(delegate { budget.Add("x"); });
        Reject(delegate { new HostProbeContract.EvidenceBudget().Add(new string('x',HostProbeContract.MaximumRecordBytes+1)); });
        Reject(delegate { new HostProbeContract.EvidenceBudget().Add(""); });
        Console.WriteLine("PASS "+checks+" pure contract assertions; no host, Core assembly, native API or browser was loaded.");
        return 0;
    }
}
