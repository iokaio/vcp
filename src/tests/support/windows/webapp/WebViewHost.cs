// SPDX-License-Identifier: Apache-2.0
// HOST DRAFT ONLY. Browser execution requires the separately reviewed supervisor.
using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using System.Web.Script.Serialization;
using Microsoft.Web.WebView2.Core;
using Vcp.Qualification.Webapp;

namespace Vcp.Cs3WebViewDraft {
    internal static class WebViewHost {
        [System.Runtime.InteropServices.DllImport("shell32.dll",CharSet=System.Runtime.InteropServices.CharSet.Unicode)]
        static extern int SetCurrentProcessExplicitAppUserModelID(string id);
        [System.Runtime.InteropServices.DllImport("user32.dll")]
        static extern IntPtr GetFocus();
        [System.Runtime.InteropServices.DllImport("user32.dll")]
        static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
        [System.Runtime.InteropServices.DllImport("kernel32.dll",SetLastError=true)]
        static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
        [System.Runtime.InteropServices.DllImport("kernel32.dll",CharSet=System.Runtime.InteropServices.CharSet.Unicode,SetLastError=true)]
        static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder name,ref uint size);
        [System.Runtime.InteropServices.DllImport("kernel32.dll",CharSet=System.Runtime.InteropServices.CharSet.Unicode,SetLastError=true)]
        static extern IntPtr CreateFile(string file,uint access,uint share,IntPtr security,uint mode,uint flags,IntPtr template);
        [System.Runtime.InteropServices.DllImport("kernel32.dll")]
        static extern bool CloseHandle(IntPtr handle);
        static HostInput input;
        static ApplicationContext pump;
        static SynchronizationContext sta;
        static CoreWebView2Environment environment;
        static CoreWebView2Controller controller;
        static CoreWebView2 core;
        static System.Windows.Forms.Timer timer;
        static readonly Stopwatch clock = new Stopwatch();
        static readonly HostLifecycle lifecycle = new HostLifecycle();
        static readonly WebDomSession dom = WebDomContract.CreateSession();
        static readonly HostProbeContract.CommandSequence commands = new HostProbeContract.CommandSequence();
        // Fixed source-bound diagnostic variant; never configurable by a page.
        static readonly bool InsertBeforeKey = false;
        static readonly int InputSettleMilliseconds = 100;
        static readonly int KeyObservationMilliseconds = 0;
        static readonly int ReadinessSettleMilliseconds = 100;
        static readonly InputRoutingCommands inputCommands = new InputRoutingCommands(InsertBeforeKey);
        static int nativeGotFocus, nativeLostFocus;
        static readonly HostProbeContract.EvidenceBudget domBudget = new HostProbeContract.EvidenceBudget();
        static readonly List<Stream> responseStreams = new List<Stream>();
        sealed class ReportedHandles { public IntPtr Process,Image; }
        static readonly Dictionary<uint,ReportedHandles> reportedProcessHandles = new Dictionary<uint,ReportedHandles>();
        static readonly BlockingCollection<string> brokerResponses = new BlockingCollection<string>(2);
        static readonly JavaScriptSerializer json = new JavaScriptSerializer { MaxJsonLength = 16384, RecursionLimit = 64 };
        static bool ready, begun;
        static bool initialNavigationSeen, negativeExpected, negativeSeen, probeRunning;
        static ulong navigationId;
        static int events, snapshots;
        static string phase = "startup", version = "";
        static TaskCompletionSource<bool> negativeCompletion;
        const int DomChunkBytes = 96;

        [STAThread]
        static int Main(string[] args) {
            try {
                input = HostInput.Parse(args);
                HostInput.NoOverrides(Environment.GetEnvironmentVariables());
                System.Runtime.InteropServices.Marshal.ThrowExceptionForHR(SetCurrentProcessExplicitAppUserModelID("iokaio.vcp.cs3.webview2.probe"));
                // The supervisor owns staging, ACLs, hashes, policy and job/token checks.
                // This host creates no profile, server, network grant or child authority.
                if (!Directory.Exists(input.Runtime) || !Directory.Exists(input.Profile)) throw new DirectoryNotFoundException("Supervisor must stage exact directories");
                Application.SetUnhandledExceptionMode(UnhandledExceptionMode.ThrowException);
                pump = new ApplicationContext();
                sta = new WindowsFormsSynchronizationContext();
                SynchronizationContext.SetSynchronizationContext(sta);
                clock.Start();
                Emit("host_started",0,0,"");
                var reader = new Thread(ReadControl) { IsBackground = true };
                reader.Start();
                timer = new System.Windows.Forms.Timer { Interval = 100 };
                timer.Tick += delegate {
                    if (clock.ElapsedMilliseconds >= Evidence.StartupMilliseconds) { Stop("deadline",unchecked((int)0x800705b4),false); return; }
                    if (!begun) { begun = true; Begin(); }
                };
                timer.Start();
                Application.Run(pump);
            } catch (Exception error) {
                if (input != null) { try { Stop("host_failed",error.HResult,false); } catch { } }
                lifecycle.Fail("host_failed",error.HResult);
            } finally {
                if (controller != null) { try { controller.Close(); } catch (Exception error) { lifecycle.Fail("close_failed",error.HResult); } controller = null; }
                foreach (Stream stream in responseStreams) { try { stream.Dispose(); } catch (Exception error) { lifecycle.Fail("stream_close_failed",error.HResult); } }
                foreach (ReportedHandles handles in reportedProcessHandles.Values) foreach(IntPtr handle in new[]{handles.Process,handles.Image}) { try { if(!CloseHandle(handle)) lifecycle.Fail("process_handle_close_failed",System.Runtime.InteropServices.Marshal.GetHRForLastWin32Error()); } catch (Exception error) { lifecycle.Fail("process_handle_close_failed",error.HResult); } }
                if (timer != null) timer.Dispose();
                if (pump != null) pump.Dispose();
            }
            return lifecycle.ExitCode;
        }
        static async void Begin() {
            try {
                phase = "environment_create"; Emit(phase,0,0,"");
                HostInput.NoOverrides(Environment.GetEnvironmentVariables());
                var options = new CoreWebView2EnvironmentOptions();
                // Production-profile qualification permits no extra browser
                // argument. Keep the assignment conditional so an empty exact
                // contract cannot be serialized into a synthetic command line.
                if(!String.IsNullOrEmpty(HostProbeContract.BrowserArgument)) options.AdditionalBrowserArguments = HostProbeContract.BrowserArgument;
                options.ExclusiveUserDataFolderAccess = true;
                options.IsCustomCrashReportingEnabled = true;
                environment = await CoreWebView2Environment.CreateAsync(input.Runtime,input.Profile,options);
                if (lifecycle.Stopping) return;
                version = environment.BrowserVersionString;
                if (version != input.Version || !String.Equals(HostInput.Absolute(environment.UserDataFolder),input.Profile,StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("Actual version or profile differs");
                Emit("environment_created",0,0,"");
                environment.ProcessInfosChanged += ProcessInfosChanged;
                Census();
                phase = "controller_create"; Emit(phase,0,0,"");
                // HWND_MESSAGE (-3), as documented for an invisible WebView.
                controller = await environment.CreateCoreWebView2ControllerAsync(new IntPtr(-3));
                if (lifecycle.Stopping) { controller.Close(); controller = null; return; }
                controller.IsVisible = false;
                controller.Bounds = new System.Drawing.Rectangle(0,0,1,1);
                controller.GotFocus += GotNativeFocus;
                controller.LostFocus += LostNativeFocus;
                core = controller.CoreWebView2;
                core.Settings.AreDevToolsEnabled = false;
                core.Settings.AreDefaultContextMenusEnabled = false;
                core.Settings.AreBrowserAcceleratorKeysEnabled = false;
                core.Settings.IsStatusBarEnabled = false;
                core.Settings.IsZoomControlEnabled = false;
                core.Settings.IsBuiltInErrorPageEnabled = false;
                core.Settings.IsPasswordAutosaveEnabled = false;
                core.Settings.IsGeneralAutofillEnabled = false;
                core.Settings.AreHostObjectsAllowed = false;
                core.Settings.IsWebMessageEnabled = false;
                core.Settings.IsScriptEnabled = true;
                core.AddWebResourceRequestedFilter("*",CoreWebView2WebResourceContext.All);
                core.WebResourceRequested += WebResourceRequested;
                core.NavigationStarting += NavigationStarting;
                core.FrameNavigationStarting += FrameNavigationStarting;
                core.NavigationCompleted += NavigationCompleted;
                core.NewWindowRequested += NewWindowRequested;
                core.DownloadStarting += DownloadStarting;
                core.PermissionRequested += PermissionRequested;
                core.LaunchingExternalUriScheme += LaunchingExternalUriScheme;
                phase = "fixture_navigation";
                core.Navigate(WebDomContract.FormUrl);
                // NavigationCompleted drives the bounded probe. Only its final
                // evidence record may make the controller ready for supervisor STOP.
            } catch (Exception error) { Stop("creation_failed",error.HResult,false); }
        }
        static void WebResourceRequested(object sender, CoreWebView2WebResourceRequestedEventArgs e) {
            try {
                // Install a denial before any inspection; callback exceptions and
                // shutdown must not leave an implicit network fallback.
                var denied = new MemoryStream(new byte[0],false);
                responseStreams.Add(denied);
                e.Response = environment.CreateWebResourceResponse(denied,403,"Forbidden","Content-Type: text/plain; charset=utf-8\r\nCache-Control: no-store");
                if (lifecycle.Stopping) return;
                // Handle the one exact off-origin test before consulting the
                // normal-phase fixture router, which intentionally latches any
                // request made after its resource phase has closed.
                if (negativeExpected && String.Equals(e.Request.Uri,WebDomContract.BlockedUrl,StringComparison.Ordinal)) {
                    CompleteNegativeNavigation(e.Request.Uri);
                    return;
                }
                WebDomResource resource;
                if (!dom.TryServe(e.Request.Method,e.Request.Uri,out resource)) {
                    Reject("resource_rejected");
                    return;
                }
                byte[] bytes = Encoding.UTF8.GetBytes(resource.Body);
                string resourceId = String.Equals(resource.Url,WebDomContract.FormUrl,StringComparison.Ordinal) ? "form" :
                    String.Equals(resource.Url,WebDomContract.ScriptUrl,StringComparison.Ordinal) ? "script" : null;
                if(resourceId==null) throw new InvalidDataException("Unknown broker resource identity");
                Emit("broker_request",0,0,resourceId);
                string acknowledgement;
                if(!brokerResponses.TryTake(out acknowledgement,3000)) throw new TimeoutException("Owned server broker deadline");
                string expectedAck="RESOURCE "+input.Nonce+" "+resourceId+" "+Hash(bytes);
                if(!String.Equals(acknowledgement,expectedAck,StringComparison.Ordinal)) throw new InvalidDataException("Owned server broker acknowledgement differs");
                var stream = new MemoryStream(bytes,false);
                responseStreams.Add(stream);
                string headers = "Content-Type: " + resource.ContentType + "\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; connect-src 'none'; img-src 'none'; style-src 'none'; form-action 'none'";
                e.Response = environment.CreateWebResourceResponse(stream,200,"OK",headers);
            } catch (Exception error) { Reject("resource_callback_failed",error.HResult); }
        }
        static void NavigationStarting(object sender, CoreWebView2NavigationStartingEventArgs e) {
            if (lifecycle.Stopping) { e.Cancel = true; return; }
            try {
                if (!initialNavigationSeen && !negativeExpected && String.Equals(e.Uri,WebDomContract.FormUrl,StringComparison.Ordinal)) {
                    initialNavigationSeen = true;
                    navigationId = e.NavigationId;
                    return;
                }
                if (negativeExpected && String.Equals(e.Uri,WebDomContract.BlockedUrl,StringComparison.Ordinal)) {
                    e.Cancel = true;
                    CompleteNegativeNavigation(e.Uri);
                    return;
                }
                e.Cancel = true;
                Reject("navigation_rejected");
            } catch (Exception error) { e.Cancel = true; Reject("navigation_callback_failed",error.HResult); }
        }
        static void CompleteNegativeNavigation(string uri) {
            if(negativeSeen) return;
            if(!negativeExpected || negativeCompletion==null || !String.Equals(uri,WebDomContract.BlockedUrl,StringComparison.Ordinal)) throw new InvalidOperationException("Unexpected negative navigation completion");
            if(!dom.ObserveDeniedNavigation(uri,true)) throw new InvalidOperationException(dom.Failure);
            negativeSeen=true;
            negativeCompletion.TrySetResult(true);
        }
        static void FrameNavigationStarting(object sender, CoreWebView2NavigationStartingEventArgs e) {
            e.Cancel = true;
            if (!lifecycle.Stopping) Reject("frame_navigation_rejected");
        }
        static void NewWindowRequested(object sender, CoreWebView2NewWindowRequestedEventArgs e) {
            e.Handled = true;
            if (!lifecycle.Stopping) Reject("popup_rejected");
        }
        static void DownloadStarting(object sender, CoreWebView2DownloadStartingEventArgs e) {
            e.Cancel = true;
            if (!lifecycle.Stopping) Reject("download_rejected");
        }
        static void PermissionRequested(object sender, CoreWebView2PermissionRequestedEventArgs e) {
            e.State = CoreWebView2PermissionState.Deny; e.Handled = true;
            if (!lifecycle.Stopping) Reject("permission_rejected");
        }
        static void LaunchingExternalUriScheme(object sender, CoreWebView2LaunchingExternalUriSchemeEventArgs e) {
            e.Cancel = true;
            if (!lifecycle.Stopping) Reject("external_uri_rejected");
        }
        static async void NavigationCompleted(object sender, CoreWebView2NavigationCompletedEventArgs e) {
            if (lifecycle.Stopping) return;
            if (probeRunning) { Reject("duplicate_navigation_complete"); return; }
            probeRunning = true;
            core.NavigationCompleted -= NavigationCompleted;
            try {
                if (!e.IsSuccess || !initialNavigationSeen || e.NavigationId != navigationId || dom.ResourceCount != WebDomContract.MaxResourceCount) throw new InvalidOperationException("Initial fixture navigation did not complete exactly");
                await RunDomProbe();
            } catch (Exception error) { phase=Evidence.FailureKind(phase,error); Stop("dom_probe_failed",error.HResult,false); }
        }
        static void GotNativeFocus(object sender, object args) {
            if(lifecycle.Stopping) return;
            if(++nativeGotFocus>16) Reject("native_focus_event_bound");
        }
        static void LostNativeFocus(object sender, object args) {
            if(lifecycle.Stopping) return;
            if(++nativeLostFocus>16) Reject("native_focus_event_bound");
        }
        static async Task<string> InputCommand(string method, string parameters) {
            RequireProbeActive(); inputCommands.BeginRequest(method,parameters,clock.ElapsedMilliseconds);
            string result=await core.CallDevToolsProtocolMethodAsync(method,parameters);
            RequireProbeActive(); inputCommands.Complete(method,parameters,result,clock.ElapsedMilliseconds);
            BoundJson(result); return result;
        }
        static async Task InputSnapshot(string id) {
            // Preserve the frozen routing-only evidence shape even though the
            // selected full-DOM route records additional readiness fields.
            const string script="(() => { const n=document.getElementById('name'),e=document.getElementById('name-error'),s=document.getElementById('status'),a=document.activeElement,p=window.__cs3InputEvidence||{}; return {url:location.href,readyState:document.readyState,title:document.title,name:n.labels[0].textContent,value:n.value,required:n.required,error:e.textContent,status:s.textContent,active:a&&a.id?a.id:(a?a.tagName:''),scriptReady:p.scriptReady===true,keyDowns:Number.isInteger(p.keyDowns)?p.keyDowns:-1,keyPresses:Number.isInteger(p.keyPresses)?p.keyPresses:-1,keyUps:Number.isInteger(p.keyUps)?p.keyUps:-1,submits:Number.isInteger(p.submits)?p.submits:-1,lastKey:typeof p.lastKey==='string'?p.lastKey:'',trustedKeys:p.trustedKeys===true}; })()";
            string raw=await ReadScript(script); RequireProbeActive();
            var value=Object(raw); IntPtr window=GetFocus(); uint pid=0;
            if(window!=IntPtr.Zero && GetWindowThreadProcessId(window,out pid)==0) throw new InvalidOperationException("Cannot identify native focus window");
            value.Add("native_got_focus",nativeGotFocus); value.Add("native_lost_focus",nativeLostFocus);
            value.Add("native_focus_present",window!=IntPtr.Zero); value.Add("native_focus_pid",pid);
            EmitInput(id,json.Serialize(value));
        }
        static async Task RunInputDiagnostic() {
            // This run records routing observations only. It cannot emit DOM
            // completion or claim keyboard, AX, origin or browser qualification.
            phase="cdp_target_control";
            string target=await InputCommand("Runtime.evaluate",InputRoutingCommands.TargetParameters);
            EmitInput("target",target);
            var result=Object(target); object remoteObject;
            if(result.Count!=1 || !result.TryGetValue("result",out remoteObject)) throw new InvalidDataException("CDP target returned exception or unexpected fields");
            var remote=remoteObject as Dictionary<string,object>; object byValue;
            if(remote==null || !remote.TryGetValue("value",out byValue)) throw new InvalidDataException("CDP target returned no value");
            EqualString(remote,"type","object");
            var identity=byValue as Dictionary<string,object>;
            if(identity==null || identity.Count!=3) throw new InvalidDataException("CDP page identity shape differs");
            EqualString(identity,"url",WebDomContract.FormUrl); EqualString(identity,"title","CS-3 form fixture"); EqualBoolean(identity,"scriptReady",true);
            await InputCommand("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters);
            phase="input_native_focus";
            controller.MoveFocus(CoreWebView2MoveFocusReason.Next);
            await Task.Delay(InputSettleMilliseconds); RequireProbeActive();
            await InputSnapshot("focus");
            phase=InsertBeforeKey?"independent_text_insertion":"no_insertion_control";
            if(InsertBeforeKey) await InputCommand("Input.insertText",HostProbeContract.InsertParameters);
            await Task.Delay(InputSettleMilliseconds); RequireProbeActive();
            await InputSnapshot("text");
            phase="independent_enter_delivery";
            await InputCommand("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters);
            await InputCommand("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters);
            await Task.Delay(KeyObservationMilliseconds); RequireProbeActive();
            await InputSnapshot("key");
            inputCommands.RequireComplete(); RequireProbeActive();
            Emit("input_complete",0,0,"routing_only");
            Census(); RequireProbeActive(); ready=true; phase="controller_ready";
            Emit(phase,0,checked((uint)core.BrowserProcessId),"input_diagnostic");
        }
        static async Task RunDomProbe() {
            phase = "focus_emulation";
            await DevTools("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters);
            RequireProbeActive();
            string initial = await Snapshot();
            RequireProbeActive();
            EmitDom("initial",initial);
            AssertSnapshot(initial,"", "", "", "BODY",0,0,0,0,"",0);

            phase = "native_forward_focus";
            // This is WebView2's documented forward-traversal primitive for
            // entering the first page element. It does not claim CDP Tab input.
            controller.MoveFocus(CoreWebView2MoveFocusReason.Next);
            RequireProbeActive();
            await Task.Delay(ReadinessSettleMilliseconds);
            RequireProbeActive();
            await ReadinessSentinel();
            phase = "native_forward_focus";
            string focused = await Snapshot();
            RequireProbeActive();
            EmitDom("focused",focused);
            AssertSnapshot(focused,"", "", "", "name",0,0,0,0,"",1);

            phase = "invalid_form_interaction";
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters);
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters);
            string invalid = await Snapshot();
            RequireProbeActive();
            EmitDom("invalid",invalid);
            AssertSnapshot(invalid,"", "Name is required.", "", "name",1,1,1,1,"Enter",1);

            phase = "valid_form_interaction";
            await DevTools("Input.insertText",HostProbeContract.InsertParameters);
            await Task.Delay(ReadinessSettleMilliseconds);
            RequireProbeActive();
            await ReadinessSentinel();
            phase = "valid_form_interaction";
            string filled = await Snapshot();
            RequireProbeActive();
            AssertSnapshot(filled,"Ada", "", "", "name",1,1,1,1,"Enter",2);
            EmitDom("filled",filled);
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters);
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters);
            string success = await Snapshot();
            RequireProbeActive();
            EmitDom("success",success);
            AssertSnapshot(success,"Ada", "", "Saved Ada.", "name",2,2,2,2,"Enter",2);

            phase = "accessibility_snapshot";
            string accessibilityRaw = await DevTools("Accessibility.getFullAXTree",HostProbeContract.AxParameters);
            RequireProbeActive();
            string accessibility = HostAccessibilityProjection.Create(accessibilityRaw);
            EmitDom("accessibility",accessibility);
            AssertSnapshot(await Snapshot(),"Ada", "", "Saved Ada.", "name",2,2,2,2,"Enter",2);

            phase = "negative_origin";
            if (!dom.BeginNegativePhase()) throw new InvalidOperationException(dom.Failure);
            negativeExpected = true;
            negativeCompletion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            core.Navigate(WebDomContract.BlockedUrl);
            await negativeCompletion.Task;
            if (lifecycle.Stopping || !negativeSeen) return;
            AssertSnapshot(await Snapshot(),"Ada", "", "Saved Ada.", "name",2,2,2,2,"Enter",2);
            string origin = await OriginSnapshot();
            RequireProbeActive();
            AssertOrigin(origin);
            EmitDom("origin",origin);
            if (!dom.Complete()) throw new InvalidOperationException(dom.Failure);
            commands.RequireComplete();
            RequireProbeActive();

            Emit("dom_complete",0,0,"form_and_origin");
            Census();
            RequireProbeActive();
            ready = true;
            phase = "controller_ready";
            Emit(phase,0,checked((uint)core.BrowserProcessId),"browser");
            // Wait only for exact supervisor STOP or the fixed total watchdog.
        }
        static async Task ReadinessSentinel() {
            phase="input_readiness_sentinel";
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.ReadinessDownParameters);
            await DevTools("Input.dispatchKeyEvent",HostProbeContract.ReadinessUpParameters);
            // The current Evergreen runtime can complete dispatch before the
            // page's key handlers are observable through ExecuteScriptAsync.
            // One awaited, bounded read-only barrier samples renderer turns
            // until the already-dispatched pair is observed or its fixed
            // ceiling expires. It never retries Escape or the following form
            // action, and the command contract rejects missing, late, repeated
            // or untrusted evidence.
            await DevTools("Runtime.evaluate",HostProbeContract.ReadinessBarrierParameters);
        }
        static Task<string> Snapshot() {
            const string script = "(() => { const n=document.getElementById('name'),e=document.getElementById('name-error'),s=document.getElementById('status'),a=document.activeElement,p=window.__cs3InputEvidence||{}; return {url:location.href,readyState:document.readyState,title:document.title,name:n.labels[0].textContent,value:n.value,required:n.required,error:e.textContent,status:s.textContent,active:a&&a.id?a.id:(a?a.tagName:''),scriptReady:p.scriptReady===true,keyDowns:Number.isInteger(p.keyDowns)?p.keyDowns:-1,keyPresses:Number.isInteger(p.keyPresses)?p.keyPresses:-1,keyUps:Number.isInteger(p.keyUps)?p.keyUps:-1,submits:Number.isInteger(p.submits)?p.submits:-1,lastKey:typeof p.lastKey==='string'?p.lastKey:'',trustedKeys:p.trustedKeys===true,readinessDowns:Number.isInteger(p.readinessDowns)?p.readinessDowns:-1,readinessKeyPresses:Number.isInteger(p.readinessKeyPresses)?p.readinessKeyPresses:-1,readinessUps:Number.isInteger(p.readinessUps)?p.readinessUps:-1,readinessRepeats:Number.isInteger(p.readinessRepeats)?p.readinessRepeats:-1,readinessSequence:typeof p.readinessSequence==='string'?p.readinessSequence:'',readinessTrusted:p.readinessTrusted===true}; })()";
            return ReadScript(script);
        }
        static Task<string> OriginSnapshot() {
            return ReadScript("(() => ({url:location.href,origin:location.origin}))()");
        }
        static async Task<string> ReadScript(string script) {
            // This path is read-only: scripts return snapshots and never assign,
            // click controls, dispatch DOM events or navigate.
            RequireProbeActive();
            string result = await core.ExecuteScriptAsync(script);
            RequireProbeActive();
            BoundJson(result);
            return result;
        }
        static async Task<string> DevTools(string method, string parameters) {
            RequireProbeActive();
            commands.BeginRequest(method,parameters,clock.ElapsedMilliseconds);
            string result = await core.CallDevToolsProtocolMethodAsync(method,parameters);
            RequireProbeActive();
            commands.Complete(method,parameters,result,clock.ElapsedMilliseconds);
            if (method == "Accessibility.getFullAXTree") BoundJson(result,HostProbeContract.MaximumAccessibilityResponseBytes);
            return result;
        }
        static void RequireProbeActive() { HostProbeContract.RequireActive(lifecycle,clock.ElapsedMilliseconds); }
        static void BoundJson(string value) { BoundJson(value,HostProbeContract.MaximumRecordBytes); }
        static void BoundJson(string value, int maximumBytes) {
            if (maximumBytes<=0 || String.IsNullOrEmpty(value) || Encoding.UTF8.GetByteCount(value)>maximumBytes) throw new InvalidDataException("DOM evidence exceeds its per-record bound");
            json.DeserializeObject(value);
        }
        static Dictionary<string,object> Object(string raw) {
            var value=json.DeserializeObject(raw) as Dictionary<string,object>;
            if(value==null) throw new InvalidDataException("Expected JSON object");
            return value;
        }
        static void AssertSnapshot(string raw, string value, string error, string status, string active, int keyDowns, int keyPresses, int keyUps, int submits, string lastKey, int readinessPairs) {
            var item=Object(raw);
            string[] expected={"url","readyState","title","name","value","required","error","status","active","scriptReady","keyDowns","keyPresses","keyUps","submits","lastKey","trustedKeys","readinessDowns","readinessKeyPresses","readinessUps","readinessRepeats","readinessSequence","readinessTrusted"};
            if(item.Count!=expected.Length) throw new InvalidDataException("Snapshot shape differs");
            foreach(string name in expected) if(!item.ContainsKey(name)) throw new InvalidDataException("Snapshot field absent");
            EqualString(item,"url",WebDomContract.FormUrl);
            EqualString(item,"readyState","complete");
            EqualString(item,"title","CS-3 form fixture");
            EqualString(item,"name","Name");
            EqualString(item,"value",value);
            EqualString(item,"error",error);
            EqualString(item,"status",status);
            EqualString(item,"active",active);
            object required; if(!item.TryGetValue("required",out required) || !(required is bool) || !(bool)required) throw new InvalidDataException("Required state differs");
            EqualBoolean(item,"scriptReady",true); EqualBoolean(item,"trustedKeys",true);
            EqualInteger(item,"keyDowns",keyDowns); EqualInteger(item,"keyPresses",keyPresses); EqualInteger(item,"keyUps",keyUps); EqualInteger(item,"submits",submits);
            EqualString(item,"lastKey",lastKey);
            EqualInteger(item,"readinessDowns",readinessPairs); EqualInteger(item,"readinessKeyPresses",0); EqualInteger(item,"readinessUps",readinessPairs); EqualInteger(item,"readinessRepeats",0);
            EqualString(item,"readinessSequence",new StringBuilder(readinessPairs*2).Insert(0,"DU",readinessPairs).ToString());
            EqualBoolean(item,"readinessTrusted",true);
        }
        static void AssertOrigin(string raw) {
            var item=Object(raw);
            if(item.Count!=2 || !item.ContainsKey("url") || !item.ContainsKey("origin")) throw new InvalidDataException("Origin snapshot shape differs");
            EqualString(item,"url",WebDomContract.FormUrl);
            EqualString(item,"origin","https://cs3-fixture.invalid");
        }
        static void EqualString(Dictionary<string,object> item, string field, string expected) {
            object value;
            if(!item.TryGetValue(field,out value) || !(value is string) || !String.Equals((string)value,expected,StringComparison.Ordinal)) throw new InvalidDataException("Snapshot value differs: "+field);
        }
        static void EqualInteger(Dictionary<string,object> item, string field, int expected) {
            object value;
            if(!item.TryGetValue(field,out value) || !(value is int) || (int)value!=expected || (int)value<0 || (int)value>16) throw new InvalidDataException("Snapshot integer differs: "+field);
        }
        static void EqualBoolean(Dictionary<string,object> item, string field, bool expected) {
            object value;
            if(!item.TryGetValue(field,out value) || !(value is bool) || (bool)value!=expected) throw new InvalidDataException("Snapshot boolean differs: "+field);
        }
        static void EmitDom(string id, string raw) {
            if(id!="initial" && id!="focused" && id!="invalid" && id!="filled" && id!="success" && id!="accessibility" && id!="origin") throw new InvalidOperationException("Unknown DOM evidence id");
            RequireProbeActive();
            byte[] bytes=domBudget.Add(raw);
            string hash;
            using(var algorithm=SHA256.Create()) hash=Hex(algorithm.ComputeHash(bytes));
            for(int offset=0;offset<bytes.Length;offset+=DomChunkBytes) {
                int count=Math.Min(DomChunkBytes,bytes.Length-offset);
                string data=Convert.ToBase64String(bytes,offset,count);
                if(data.Length>128) throw new InvalidDataException("DOM evidence chunk exceeds bound");
                Emit("dom_chunk",0,0,id+":"+offset+":"+bytes.Length+":"+hash+":"+data);
            }
        }
        static void EmitInput(string id, string raw) {
            byte[] bytes=domBudget.Add(raw); string hash;
            using(var algorithm=SHA256.Create()) hash=Hex(algorithm.ComputeHash(bytes));
            for(int offset=0;offset<bytes.Length;offset+=DomChunkBytes) {
                int count=Math.Min(DomChunkBytes,bytes.Length-offset);
                Emit("input_chunk",0,0,id+":"+offset+":"+bytes.Length+":"+hash+":"+Convert.ToBase64String(bytes,offset,count));
            }
        }
        static string Hex(byte[] bytes) {
            var text=new StringBuilder(bytes.Length*2);
            foreach(byte value in bytes) text.Append(value.ToString("x2",System.Globalization.CultureInfo.InvariantCulture));
            return text.ToString();
        }
        static void ProcessInfosChanged(object sender, object args) {
            if (lifecycle.Stopping) return;
            try { Census(); } catch (Exception error) { Stop("census_failed",error.HResult,false); }
        }
        static void Census() {
            // ProcessInfosChanged is an advisory, coalescible signal. Retain a
            // bounded prefix; the supervisor's job census remains authoritative
            // and must still verify every cumulative process identity exactly.
            if (snapshots >= 8) return;
            snapshots++;
            var processes = environment.GetProcessInfos();
            if (processes.Count > Evidence.MaximumProcesses) throw new InvalidOperationException("Process count bound exceeded");
            Emit("process_snapshot",0,0,"snapshot_"+snapshots);
            foreach (var process in processes) {
                uint pid=checked((uint)process.ProcessId); ReportedHandles handles;
                if(!reportedProcessHandles.TryGetValue(pid,out handles)) {
                    IntPtr processHandle=OpenProcess(0x101000,false,pid);
                    if(processHandle==IntPtr.Zero) throw new System.ComponentModel.Win32Exception(System.Runtime.InteropServices.Marshal.GetLastWin32Error());
                    var imageName=new StringBuilder(4096); uint imageLength=4096;
                    if(!QueryFullProcessImageName(processHandle,0,imageName,ref imageLength)) { int error=System.Runtime.InteropServices.Marshal.GetLastWin32Error(); CloseHandle(processHandle); throw new System.ComponentModel.Win32Exception(error); }
                    IntPtr imageHandle=CreateFile(imageName.ToString(),0x80000000,7,IntPtr.Zero,3,0,IntPtr.Zero);
                    if(imageHandle==new IntPtr(-1)) { int error=System.Runtime.InteropServices.Marshal.GetLastWin32Error(); CloseHandle(processHandle); throw new System.ComponentModel.Win32Exception(error); }
                    handles=new ReportedHandles { Process=processHandle,Image=imageHandle };
                    reportedProcessHandles.Add(pid,handles);
                }
                Emit("reported_process",0,pid,process.Kind.ToString()+":"+handles.Process.ToInt64().ToString("x",System.Globalization.CultureInfo.InvariantCulture)+":"+handles.Image.ToInt64().ToString("x",System.Globalization.CultureInfo.InvariantCulture));
            }
        }
        static void ReadControl() {
            try {
                while(true) {
                    var text = new System.Text.StringBuilder();
                    int c;
                    while ((c = Console.In.Read()) != -1 && c != '\n') {
                        if (text.Length >= 160) throw new InvalidDataException("Control bound");
                        text.Append((char)c);
                    }
                    string command = text.ToString();
                    if (command.EndsWith("\r",StringComparison.Ordinal)) command = command.Substring(0,command.Length-1);
                    if(c=='\n' && command.StartsWith("RESOURCE "+input.Nonce+" ",StringComparison.Ordinal)) {
                        if(!brokerResponses.TryAdd(command,1000)) throw new InvalidDataException("Broker response queue bound");
                        continue;
                    }
                    bool accepted = c == '\n' && HostInput.StopCommand(command,input.Nonce);
                    sta.Post(delegate { Stop(accepted ? "stop_received" : "control_closed",accepted ? 0 : unchecked((int)0x80070057),accepted && ready); },null);
                    return;
                }
            } catch { try { sta.Post(delegate { Stop("control_failed",unchecked((int)0x80070057),false); },null); } catch { } }
        }
        static string Hash(byte[] bytes) {
            using(var algorithm=SHA256.Create()) return Hex(algorithm.ComputeHash(bytes));
        }
        static void Reject(string reason) { Reject(reason,unchecked((int)0x80070005)); }
        static void Reject(string reason, int hresult) {
            // Cancellation handlers run on the STA. Latch before posting cleanup
            // so an already queued STOP, or a callback during Close, cannot win.
            lifecycle.Fail(reason,hresult);
            sta.Post(delegate { Stop(reason,hresult,false); },null);
        }
        static void Emit(string state, int hresult, uint pid, string kind) {
            if (++events > Evidence.MaximumEvents) throw new InvalidOperationException("Event bound exceeded");
            Console.Out.WriteLine(Evidence.Record(input.Nonce,state,Math.Min(clock.ElapsedMilliseconds,120000),hresult,version,pid,kind)); Console.Out.Flush();
        }
        static void Stop(string reason, int hresult, bool success) {
            if (!lifecycle.BeginStop(reason,hresult,success,clock.ElapsedMilliseconds)) return;
            if (lifecycle.Failed) { reason = lifecycle.FailureReason; hresult = lifecycle.FailureHResult; }
            if (timer != null) timer.Stop();
            try {
                Emit(reason,hresult,0,phase);
                if (environment != null) environment.ProcessInfosChanged -= ProcessInfosChanged;
                if (core != null) {
                    core.WebResourceRequested -= WebResourceRequested;
                    core.NavigationStarting -= NavigationStarting;
                    core.FrameNavigationStarting -= FrameNavigationStarting;
                    core.NavigationCompleted -= NavigationCompleted;
                    core.NewWindowRequested -= NewWindowRequested;
                    core.DownloadStarting -= DownloadStarting;
                    core.PermissionRequested -= PermissionRequested;
                    core.LaunchingExternalUriScheme -= LaunchingExternalUriScheme;
                }
                if (controller != null) { controller.GotFocus-=GotNativeFocus; controller.LostFocus-=LostNativeFocus; controller.Close(); controller = null; core = null; Emit("controller_closed",0,0,"job_drain_not_attested"); }
                else Emit("controller_absent",0,0,"job_drain_not_attested");
            } catch (Exception error) { lifecycle.Fail("close_failed",error.HResult); try { Emit("close_failed",error.HResult,0,phase); } catch { } }
            finally { if (pump != null) pump.ExitThread(); }
        }
    }
}
