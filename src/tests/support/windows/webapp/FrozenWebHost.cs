// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Web.WebView2.Core;
using Vcp.Qualification.Webapp;
namespace Vcp.Cs3WebViewDraft {
    internal static partial class WebViewHost {
        static bool webCasesRunning, webNavigationStarted, webDenyExpected;
        static int webResourceIndex, webDenied, webCommands, webDeniedResources, webDeniedNavigations;
        static string webExpectedUrl;
        static TaskCompletionSource<bool> webNavigation;
        static readonly string[] WebDocuments={"web-initial","web-tab","web-invalid","web-success","web-ax","web-poll-error","web-poll-success","web-hostile"};
        static int webDocumentIndex;
        static void ServeWebCase(CoreWebView2WebResourceRequestedEventArgs e) {
            if(webDenyExpected && e.Request.Method=="GET" && e.Request.Uri==WebDomContract.BlockedUrl) {
                // WebView2 may raise the denied resource callback before the
                // navigation callback for this single fixed control action.
                if(++webDeniedResources!=1) throw new InvalidDataException("Repeated blocked WEB resource");
                webDenied=1; return;
            }
            if(e.Request.Method!="GET" || webResourceIndex>=FrozenWebResources.Sequence.Length) throw new InvalidDataException("WEB resource request boundary");
            var r=FrozenWebResources.Find(FrozenWebResources.Sequence[webResourceIndex]);
            if(e.Request.Uri!=r.Url || r.Id=="web-ready") throw new InvalidDataException("WEB exact route/order differs");
            RelayWebResource(r); webResourceIndex++;
            var stream=new MemoryStream(r.Bytes,false); responseStreams.Add(stream);
            e.Response=environment.CreateWebResourceResponse(stream,r.Status,r.Reason,"Content-Type: "+r.ContentType+"\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'none'; img-src 'none'; form-action 'none'");
        }
        static void RelayWebResource(FrozenWebResource resource) {
            Emit("broker_request",0,0,resource.Id); string acknowledgement;
            if(!brokerResponses.TryTake(out acknowledgement,3000) || acknowledgement!="RESOURCE "+input.Nonce+" "+resource.Id+" "+Hash(resource.Bytes)) throw new InvalidDataException("WEB server relay acknowledgement differs");
        }
        static void StartWebNavigation(CoreWebView2NavigationStartingEventArgs e) {
            if(webDenyExpected && e.Uri==WebDomContract.BlockedUrl) {
                e.Cancel=true;
                if(++webDeniedNavigations!=1) throw new InvalidDataException("Repeated blocked WEB navigation");
                webDenied=1;
                return;
            }
            if(webNavigation!=null && !webNavigationStarted && e.Uri==webExpectedUrl) { webNavigationStarted=true; e.Cancel=false; return; }
            e.Cancel=true; Reject("web_navigation_rejected");
        }
        static async Task NavigateWeb(string id) {
            webExpectedUrl=FrozenWebResources.Find(id).Url; webNavigationStarted=false;
            webNavigation=new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            core.Navigate(webExpectedUrl); await webNavigation.Task; RequireProbeActive(); webNavigation=null;
            string focused=await core.CallDevToolsProtocolMethodAsync("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters);
            RequireProbeActive(); if(focused!="{}") throw new InvalidDataException("WEB focus-emulation response");
            await WaitWeb("({focus:document.hasFocus()?'yes':'no'})","focus","yes");
        }
        static void CompleteWebNavigation(object sender,CoreWebView2NavigationCompletedEventArgs e) {
            if(lifecycle.Stopping) return;
            if(webDenyExpected && !e.IsSuccess) return;
            if(webNavigation==null || !webNavigationStarted || !e.IsSuccess || core.Source!=webExpectedUrl) { Reject("web_navigation_completion_rejected"); return; }
            webNavigation.TrySetResult(true);
        }
        static async Task WebCommand(string method,string parameters) {
            // This method is private; every caller below supplies a source-frozen
            // constant. No page, model, pipe or configuration selects a command.
            RequireProbeActive(); if(++webCommands>16) throw new InvalidDataException("WEB command count");
            bool allowed=method=="Input.dispatchKeyEvent" && (parameters==HostProbeContract.EnterDownParameters || parameters==HostProbeContract.EnterUpParameters || parameters=="{\"type\":\"rawKeyDown\",\"key\":\"Tab\",\"code\":\"Tab\",\"windowsVirtualKeyCode\":9}" || parameters=="{\"type\":\"keyUp\",\"key\":\"Tab\",\"code\":\"Tab\",\"windowsVirtualKeyCode\":9}") || method=="Input.insertText" && parameters==HostProbeContract.InsertParameters;
            if(!allowed) throw new InvalidDataException("WEB command not fixed");
            var response=await core.CallDevToolsProtocolMethodAsync(method,parameters);
            RequireProbeActive(); if(response!="{}") throw new InvalidDataException("WEB command response differs");
        }
        static async Task TabWeb() {
            await WebCommand("Input.dispatchKeyEvent","{\"type\":\"rawKeyDown\",\"key\":\"Tab\",\"code\":\"Tab\",\"windowsVirtualKeyCode\":9}");
            await WebCommand("Input.dispatchKeyEvent","{\"type\":\"keyUp\",\"key\":\"Tab\",\"code\":\"Tab\",\"windowsVirtualKeyCode\":9}");
        }
        static async Task EnterWeb() { await WebCommand("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters); await WebCommand("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters); }
        static async Task ClickWebControl(string id) {
            if(id!="retry" && id!="continue") throw new InvalidDataException("WEB click selector differs");
            string frame=await core.CallDevToolsProtocolMethodAsync("Runtime.evaluate","{\"expression\":\"new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(document.readyState==='complete'))))\",\"awaitPromise\":true,\"returnByValue\":true}");
            BoundJson(frame,512);var frameResult=Object(frame);var frameValue=frameResult.ContainsKey("result")?frameResult["result"] as Dictionary<string,object>:null;
            if(frameValue==null||!frameValue.ContainsKey("value")||!(frameValue["value"] is bool)||!(bool)frameValue["value"])throw new InvalidDataException("WEB render-frame readiness");
            string script=id=="retry"?"(() => {const r=document.getElementById('retry').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height};})()":"(() => {const r=document.getElementById('continue').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height};})()";
            var rect=Object(await ReadScript(script)); double x=Convert.ToDouble(rect["x"]),y=Convert.ToDouble(rect["y"]);
            if(Double.IsNaN(x) || Double.IsNaN(y) || x<=0 || x>=800 || y<=0 || y>=600 || Convert.ToDouble(rect["width"])<=0 || Convert.ToDouble(rect["height"])<=0) throw new InvalidDataException("WEB click is outside bounded viewport");
            foreach(string type in new[]{"mousePressed","mouseReleased"}) {
                RequireProbeActive(); if(++webCommands>11) throw new InvalidDataException("WEB command count");
                string response=await core.CallDevToolsProtocolMethodAsync("Input.dispatchMouseEvent",json.Serialize(new {type=type,x=x,y=y,button="left",clickCount=1}));
                RequireProbeActive(); if(response!="{}") throw new InvalidDataException("WEB click response differs");
            }
        }
        const string FormWebScript="(() => {const n=document.getElementById('name'); return {url:location.href,title:document.title,value:n.value,required:n.required,error:document.getElementById('name-error').textContent,status:document.getElementById('status').textContent,active:document.activeElement.id||document.activeElement.tagName};})()";
        const string PollWebScript="(() => ({url:location.href,status:document.getElementById('state').textContent,retryHidden:document.getElementById('retry').hidden,items:Array.from(document.querySelectorAll('#items li'),x=>x.textContent)}))()";
        static async Task<string> WaitWeb(string script,string field,string expected) {
            string last="";
            for(int turn=0;turn<50;turn++) { string value=await ReadScript(script); object observed; if(Object(value).TryGetValue(field,out observed) && observed is string) {last=(string)observed; if(last==expected) return value;} await Task.Delay(20); RequireProbeActive(); }
            throw new InvalidDataException("WEB observation "+field+"="+(last.Length<32?last:"overbound"));
        }
        static void EmitWeb(string id,string value) {
            if(webDocumentIndex>=WebDocuments.Length || id!=WebDocuments[webDocumentIndex++]) throw new InvalidDataException("WEB document sequence");
            byte[] bytes=domBudget.Add(value); string hash=Hash(bytes);
            for(int offset=0;offset<bytes.Length;offset+=DomChunkBytes) Emit("web_chunk",0,0,id+":"+offset+":"+bytes.Length+":"+hash+":"+Convert.ToBase64String(bytes,offset,Math.Min(DomChunkBytes,bytes.Length-offset)));
        }
        static async Task RunFrozenWebCases() {
            phase="frozen_web_cases"; webCasesRunning=true;
            core.NavigationCompleted+=CompleteWebNavigation;
            await NavigateWeb("web-form-html");
            EmitWeb("web-initial",await ReadScript(FormWebScript));
            // The existing successful focus-emulation sequence stays active.
            // One real Tab pair is dispatched; bounded reads never retry input.
            await Task.Delay(100); await TabWeb();
            EmitWeb("web-tab",await WaitWeb(FormWebScript,"active","name"));
            await EnterWeb(); EmitWeb("web-invalid",await WaitWeb(FormWebScript,"error","Name is required."));
            await WebCommand("Input.insertText",HostProbeContract.InsertParameters);
            await WaitWeb(FormWebScript,"value","Ada");
            await EnterWeb(); EmitWeb("web-success",await WaitWeb(FormWebScript,"status","Saved Ada."));
            var ax=await core.CallDevToolsProtocolMethodAsync("Accessibility.getFullAXTree","{}");
            BoundJson(ax,HostProbeContract.MaximumAccessibilityResponseBytes);
            var root=Object(ax); var nodes=root["nodes"] as object[];
            if(nodes==null || nodes.Length>128) throw new InvalidDataException("WEB AX node bound");
            var selected=new List<object>();
            foreach(var item in nodes) { var node=item as Dictionary<string,object>; if(node==null) throw new InvalidDataException("WEB AX shape");
                var role=node["role"] as Dictionary<string,object>; if(role==null) continue; var name=node.ContainsKey("name")?node["name"] as Dictionary<string,object>:null;
                string r=role["value"] as string; if(r!="textbox" && r!="button" && r!="alert" && r!="status") continue;
                bool required=false; var props=node.ContainsKey("properties")?node["properties"] as object[]:null;
                if(props!=null) foreach(var p in props) { var property=p as Dictionary<string,object>; if(property!=null && (string)property["name"]=="required") required=(bool)((Dictionary<string,object>)property["value"])["value"]; }
                selected.Add(new {role=r,name=name==null?"":name["value"],required=required});
            }
            EmitWeb("web-ax",json.Serialize(new {raw_sha256=Hash(Encoding.UTF8.GetBytes(ax)),raw_bytes=Encoding.UTF8.GetByteCount(ax),nodes=selected}));
            controller.Bounds=new System.Drawing.Rectangle(0,0,800,600);
            // HWND_MESSAGE remains invisible on the desktop. Enabling its child
            // rendering widget allows Chromium to perform pointer hit testing.
            controller.IsVisible=true;
            if(FrozenWebResources.Sequence[webResourceIndex]!="web-ready") throw new InvalidDataException("WEB readiness sequence");
            RelayWebResource(FrozenWebResources.Find("web-ready")); webResourceIndex++;
            await NavigateWeb("web-poll-html");
            EmitWeb("web-poll-error",await WaitWeb(PollWebScript,"status","Unable to load"));
            phase="web_poll_retry_input"; await ClickWebControl("retry"); EmitWeb("web-poll-success",await WaitWeb(PollWebScript,"status","2 items"));
            await NavigateWeb("web-hostile-html");
            webDenyExpected=true; await ClickWebControl("continue");
            for(int turn=0;turn<50 && webDenied==0;turn++) { await Task.Delay(20); RequireProbeActive(); }
            if(webDenied!=1) throw new InvalidDataException("WEB denial not observed");
            // Retain only origin identity, never hostile text or attributes.
            EmitWeb("web-hostile",await ReadScript("({url:location.href,origin:location.origin})"));
            if(webResourceIndex!=8 || webDocumentIndex!=8 || webCommands!=11) throw new InvalidDataException("WEB execution coverage differs");
            Emit("web_complete",0,0,"frozen_web_v1");
        }
    }
}
