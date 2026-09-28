// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Web.WebView2.Core;
using Vcp.Qualification.Webapp;
namespace Vcp.Cs3WebViewDraft {
internal static partial class WebViewHost {
    sealed class UiCandidateFailure : Exception { public UiCandidateFailure(string code):base(code){} }
    static bool uiRunning,uiNavigationStarted,uiBoundaryViolation;
    static int uiResources,uiCommands,uiDocuments,uiWorld;
    static TaskCompletionSource<bool> uiNavigation;
    static string uiStage="ui-initial";
    const string UiHelpers="const label=e=>{if(!e)return '';const refs=(e.getAttribute('aria-labelledby')||'').trim().split(/\\s+/).filter(Boolean);if(refs.length>8)return '';const linked=refs.map(id=>document.getElementById(id)).filter(Boolean).map(n=>n.textContent).join(' ');return String(linked||e.getAttribute('aria-label')||(e.labels&&Array.from(e.labels,l=>l.textContent).join(' '))||e.textContent||'').trim().replace(/\\s+/g,' ').slice(0,128);};const shown=e=>{if(!e||!e.getClientRects().length)return false;const r=e.getBoundingClientRect();if(r.width<=0||r.height<=0||r.bottom<=0||r.right<=0||r.top>=innerHeight||r.left>=innerWidth)return false;for(let n=e;n;n=n.parentElement){const s=getComputedStyle(n);if(s.visibility==='hidden'||s.visibility==='collapse'||parseFloat(s.opacity)===0)return false;}return true;};const inputs=Array.from(document.querySelectorAll('input'));const buttons=Array.from(document.querySelectorAll('button'));const search=inputs.find(e=>(e.type==='search'||e.type==='text')&&label(e)==='Search'),locationInput=inputs.find(e=>e.type==='text'&&label(e)==='Location'),details=buttons.find(e=>label(e)==='Details');const pick=n=>n==='Search'?search:n==='Location'?locationInput:n==='Details'?details:['Cedar','Birch','Elm'].includes(n)?inputs.find(e=>e.type==='checkbox'&&label(e)===n):buttons.find(e=>label(e)===n);const target=details?document.getElementById(details.getAttribute('aria-controls')||''):null;";
    const string UiStateScript="(() => {"+UiHelpers+"const region=role=>{const e=document.querySelector('[role='+role+']');return shown(e)?e.textContent.trim().slice(0,256):'';};return {url:location.href,title:document.title.slice(0,128),heading:(Array.from(document.querySelectorAll('h1,h2')).filter(shown).map(e=>e.textContent.trim()).find(s=>s==='Maintenance request')||'').slice(0,128),searchLabel:label(search),visibleTasks:inputs.filter(e=>e.type==='checkbox'&&shown(e)).map(label).slice(0,32),checkedTasks:inputs.filter(e=>e.type==='checkbox'&&e.checked).map(label).slice(0,32),status:region('status'),noMatch:document.body.innerText.includes('No tasks match'),detailsLabel:label(details),expanded:details?details.getAttribute('aria-expanded')||'':'',controls:!!target,detailsVisible:shown(target),locationLabel:label(locationInput),locationValue:locationInput?locationInput.value.slice(0,256):'',alert:region('alert'),active:label(document.activeElement)};})()";
    const string UiMotionScript="(() => {const list=value=>{if(typeof value!=='string'||value.length>8192)throw Error('CSS list bound');const out=[];let part='',quote='',escape=false;for(const c of value){if(escape){part+=c;escape=false;continue;}if(c==='\\\\'){part+=c;escape=true;continue;}if(quote){part+=c;if(c===quote)quote='';continue;}if(c==='\"'||c===\"'\"){part+=c;quote=c;continue;}if(c===','){out.push(part.trim());part='';}else part+=c;}if(quote||escape)throw Error('CSS list shape');out.push(part.trim());if(out.length>128||out.some(x=>!x))throw Error('CSS list bound');return out;};const ms=value=>{const n=parseFloat(value);if(!Number.isFinite(n)||n<0||(!value.endsWith('ms')&&!value.endsWith('s')))throw Error('CSS duration shape');return n*(value.endsWith('ms')?1:1000);};const animation=style=>{const names=list(style.animationName),durations=list(style.animationDuration),states=list(style.animationPlayState),iterations=list(style.animationIterationCount);let maximum=0;for(let i=0;i<names.length;i++){const state=states[i%states.length],raw=iterations[i%iterations.length],count=raw==='infinite'?Infinity:Number(raw);if((state!=='paused'&&state!=='running')||Number.isNaN(count)||count<0)throw Error('CSS animation shape');if(names[i]==='none'||state==='paused'||count===0)continue;maximum=Math.max(maximum,ms(durations[i%durations.length]));}return maximum;};const transition=style=>{const properties=list(style.transitionProperty),durations=list(style.transitionDuration);if(properties.length===1&&properties[0]==='none')return 0;const seen=new Set();let maximum=0;for(let i=properties.length-1;i>=0;i--){const property=properties[i];if(property==='none')throw Error('CSS transition shape');if(seen.has(property))continue;seen.add(property);maximum=Math.max(maximum,ms(durations[i%durations.length]));if(property==='all')break;}return maximum;};const styles=Array.from(document.querySelectorAll('*')).flatMap(e=>[getComputedStyle(e),getComputedStyle(e,'::before'),getComputedStyle(e,'::after')]);return {reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,animationMs:Math.max(0,...styles.map(animation)),transitionMs:Math.max(0,...styles.map(transition))};})()";
    static void ServeUi(CoreWebView2WebResourceRequestedEventArgs e) {
        if(e.Request.Method!="GET"||e.Request.Uri!=UiArtifactResource.Resource.Url||uiResources!=0){uiBoundaryViolation=true;return;}
        uiResources++;RelayWebResource(UiArtifactResource.Resource);
        var stream=new MemoryStream(UiArtifactResource.Resource.Bytes,false);responseStreams.Add(stream);
        // Only this immutable self-contained HTML can execute inline code/style.
        // Every subresource/network attempt remains a denied candidate failure.
        e.Response=environment.CreateWebResourceResponse(stream,200,"OK","Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'none'; img-src 'none'; form-action 'none'; frame-src 'none'; object-src 'none'");
    }
    static void StartUiNavigation(CoreWebView2NavigationStartingEventArgs e) {
        e.Cancel=true;
        if(!uiNavigationStarted&&e.Uri==UiArtifactResource.Resource.Url){uiNavigationStarted=true;e.Cancel=false;return;}
        uiBoundaryViolation=true;
    }
    static void CompleteUiNavigation(object sender,CoreWebView2NavigationCompletedEventArgs e) {
        if(lifecycle.Stopping)return;
        if(uiNavigation!=null&&uiNavigationStarted&&e.IsSuccess&&core.Source==UiArtifactResource.Resource.Url)uiNavigation.TrySetResult(true);
        else {uiBoundaryViolation=true;if(uiNavigation!=null)uiNavigation.TrySetException(new UiCandidateFailure("navigation"));}
    }
    static void RequireUi() {RequireProbeActive();if(uiBoundaryViolation)throw new UiCandidateFailure("boundary");}
    static async Task CreateUiWorld() {
        var tree=Object(await core.CallDevToolsProtocolMethodAsync("Page.getFrameTree","{}"));
        var frameTree=tree["frameTree"] as Dictionary<string,object>;var frame=frameTree==null?null:frameTree["frame"] as Dictionary<string,object>;
        string id=frame==null?null:frame["id"] as string;
        if(String.IsNullOrEmpty(id)||id.Length>128||!frame.ContainsKey("url")||(string)frame["url"]!=UiArtifactResource.Resource.Url)throw new InvalidDataException("UI main frame identity");
        var world=Object(await core.CallDevToolsProtocolMethodAsync("Page.createIsolatedWorld",json.Serialize(new{frameId=id,worldName="vcp-cs3-ui-oracle",grantUniveralAccess=false})));
        if(world.Count!=1||!(world["executionContextId"] is int)||(int)world["executionContextId"]<=0)throw new InvalidDataException("UI isolated world identity");uiWorld=(int)world["executionContextId"];
        RequireUi();
    }
    static async Task<string> ReadUiScript(string script) {
        RequireUi();if(uiWorld<=0||Encoding.UTF8.GetByteCount(script)>8192)throw new InvalidDataException("UI isolated read bound");
        string expression="document.querySelectorAll('*').length>512?({vcpNodeBound:false}):("+script+")";
        string raw=await core.CallDevToolsProtocolMethodAsync("Runtime.evaluate",json.Serialize(new{expression=expression,contextId=uiWorld,returnByValue=true,awaitPromise=false}));
        RequireUi();if(raw!=null&&Encoding.UTF8.GetByteCount(raw)>16384)throw new UiCandidateFailure("observation_bound");BoundJson(raw,16384);var envelope=Object(raw);
        if(envelope.ContainsKey("exceptionDetails"))throw new UiCandidateFailure("observation_exception");
        var result=envelope.ContainsKey("result")?envelope["result"] as Dictionary<string,object>:null;
        if(result==null||!result.ContainsKey("type")||(string)result["type"]!="object"||!result.ContainsKey("value"))throw new InvalidDataException("UI isolated response shape");
        var objectValue=result["value"] as Dictionary<string,object>;
        if(objectValue!=null&&objectValue.ContainsKey("vcpNodeBound"))throw new UiCandidateFailure("node_bound");
        string value=json.Serialize(result["value"]);if(Encoding.UTF8.GetByteCount(value)>HostProbeContract.MaximumRecordBytes)throw new UiCandidateFailure("observation_bound");BoundJson(value);return value;
    }
    static async Task<string> UiProtocol(string method,string parameters) {
        RequireUi();if(++uiCommands>160||Encoding.UTF8.GetByteCount(parameters)>1024)throw new InvalidDataException("UI command bound");
        var result=await core.CallDevToolsProtocolMethodAsync(method,parameters);RequireUi();if(result!="{}")throw new InvalidDataException("UI command response");return result;
    }
    static async Task UiKey(string key) {
        if(key!="Tab"&&key!="Enter"&&key!="SelectAll")throw new InvalidDataException("UI key contract");
        if(key=="Enter"){await UiProtocol("Input.dispatchKeyEvent",HostProbeContract.EnterDownParameters);await UiProtocol("Input.dispatchKeyEvent",HostProbeContract.EnterUpParameters);return;}
        string name=key=="Tab"?"Tab":"a",code=key=="Tab"?"Tab":"KeyA";int vk=key=="Tab"?9:65,modifiers=key=="Tab"?0:2;
        await UiProtocol("Input.dispatchKeyEvent",json.Serialize(new{type="rawKeyDown",key=name,code=code,windowsVirtualKeyCode=vk,modifiers=modifiers}));
        await UiProtocol("Input.dispatchKeyEvent",json.Serialize(new{type="keyUp",key=name,code=code,windowsVirtualKeyCode=vk,modifiers=modifiers}));
    }
    static async Task UiClick(string name) {
        if(!new[]{"Search","Birch","Clear selection","Details","Location","Send request"}.Contains(name))throw new InvalidDataException("UI click name contract");
        string script="(() => {"+UiHelpers+"const e=pick("+json.Serialize(name)+");if(!e||!shown(e)||e.disabled)return {found:false};const r=e.getBoundingClientRect();return {found:true,x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height};})()";
        var value=Object(await ReadUiScript(script));if(!value.ContainsKey("found")||!(bool)value["found"])throw new UiCandidateFailure("control_missing");
        double x=Convert.ToDouble(value["x"]),y=Convert.ToDouble(value["y"]);
        if(Double.IsNaN(x)||Double.IsNaN(y)||x<=0||x>=800||y<=0||y>=600||Convert.ToDouble(value["width"])<=0||Convert.ToDouble(value["height"])<=0)throw new UiCandidateFailure("control_bounds");
        foreach(string type in new[]{"mousePressed","mouseReleased"})await UiProtocol("Input.dispatchMouseEvent",json.Serialize(new{type=type,x=x,y=y,button="left",clickCount=1}));
    }
    static async Task UiText(string field,string value) {
        if(!new[]{"bIr","cedar","zzzz","   ","  Pump room  "}.Contains(value))throw new InvalidDataException("UI text contract");
        await UiClick(field);await UiKey("SelectAll");await UiProtocol("Input.insertText",json.Serialize(new{text=value}));
    }
    static void EmitUi(string name,string value) {
        RequireUi();if(++uiDocuments>10)throw new InvalidDataException("UI document count");
        byte[] bytes=domBudget.Add(value);string hash=Hash(bytes);
        for(int offset=0;offset<bytes.Length;offset+=DomChunkBytes)Emit("ui_chunk",0,0,name+":"+offset+":"+bytes.Length+":"+hash+":"+Convert.ToBase64String(bytes,offset,Math.Min(DomChunkBytes,bytes.Length-offset)));
    }
    static async Task UiSnapshot(string name) {uiStage=name;await Task.Delay(50);RequireUi();EmitUi(name,await ReadUiScript(UiStateScript));}
    static async Task UiKeyboard() {
        uiStage="ui-keyboard";var targets=new List<string>();bool visible=true;
        var baseline=new Dictionary<string,HashSet<string>>();var focused=new Dictionary<string,HashSet<string>>();
        string script="(() => {"+UiHelpers+"const e=document.activeElement,s=getComputedStyle(e),name=label(e);const transparent=c=>c==='transparent'||/rgba\\([^)]*,\\s*0(?:\\.0+)?\\)/.test(c);const outline=s=>s.outlineStyle!=='none'&&parseFloat(s.outlineWidth)>0&&!transparent(s.outlineColor),shadow=s=>s.boxShadow!=='none'&&!transparent(s.boxShadow);const signature=s=>JSON.stringify([outline(s)?[s.outlineStyle,s.outlineWidth,s.outlineColor,s.outlineOffset]:null,shadow(s)?s.boxShadow:null]);const unfocused={};for(const n of ['Search','Cedar','Birch','Elm','Clear selection','Details','Location','Send request']){const control=pick(n);if(control&&control!==e&&shown(control)&&!control.disabled)unfocused[n]=signature(getComputedStyle(control));}return {name:e===pick(name)&&shown(e)&&!e.disabled?name:'',visible:e.matches(':focus-visible')&&(outline(s)||shadow(s)),signature:signature(s),unfocused:unfocused};})()";
        for(int i=0;i<12;i++){
            await UiKey("Tab");var value=Object(await ReadUiScript(script));
            var rest=value["unfocused"] as Dictionary<string,object>;if(rest==null)throw new InvalidDataException("UI focus baseline shape");
            foreach(var pair in rest){if(!baseline.ContainsKey(pair.Key))baseline.Add(pair.Key,new HashSet<string>());baseline[pair.Key].Add((string)pair.Value);}
            string name=(string)value["name"];
            if(name=="Search"||name=="Cedar"||name=="Birch"||name=="Elm"||name=="Clear selection"||name=="Details"||name=="Location"||name=="Send request"){
                if(!targets.Contains(name))targets.Add(name);visible&=(bool)value["visible"];
                if(!focused.ContainsKey(name))focused.Add(name,new HashSet<string>());focused[name].Add((string)value["signature"]);
            }
        }
        visible&=targets.All(name=>baseline.ContainsKey(name)&&focused[name].All(signature=>!baseline[name].Contains(signature)));
        EmitUi("ui-keyboard",json.Serialize(new{targets=targets,allFocusVisible=visible}));
    }
    static async Task UiLayout(int width) {
        uiStage="ui-layout-"+width;
        await UiProtocol("Emulation.setDeviceMetricsOverride",json.Serialize(new{width=width,height=768,deviceScaleFactor=1,mobile=false}));
        await Task.Delay(50);
        EmitUi(uiStage,await ReadUiScript("({viewport:innerWidth,clientWidth:document.documentElement.clientWidth,scrollWidth:Math.max(document.documentElement.scrollWidth,document.body.scrollWidth)})"));
    }
    static async Task RunUiArtifact() {
        if(!UiArtifactResource.Enabled)return;
        if(UiArtifactResource.CaseId!="UI-cs3-filter-selection-v1"&&UiArtifactResource.CaseId!="UI-cs3-disclosure-form-v1")throw new InvalidDataException("UI case identity");
        uiRunning=true;core.NavigationCompleted-=CompleteWebNavigation;core.NavigationCompleted+=CompleteUiNavigation;
        try {
            uiNavigation=new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);core.Navigate(UiArtifactResource.Resource.Url);await uiNavigation.Task;uiNavigation=null;
            await CreateUiWorld();
            await UiProtocol("Emulation.setFocusEmulationEnabled",HostProbeContract.FocusParameters);
            await UiSnapshot("ui-initial");await UiKeyboard();
            if(UiArtifactResource.CaseId=="UI-cs3-filter-selection-v1") {
                await UiText("Search","bIr");await UiSnapshot("ui-filter");await UiClick("Birch");await UiSnapshot("ui-selected");
                await UiText("Search","cedar");await UiSnapshot("ui-hidden");await UiClick("Clear selection");await UiSnapshot("ui-cleared");await UiText("Search","zzzz");await UiSnapshot("ui-no-match");
            } else {
                await UiClick("Details");await UiSnapshot("ui-disclosed");await UiClick("Send request");await UiSnapshot("ui-invalid");
                await UiText("Location","   ");await UiKey("Enter");await UiSnapshot("ui-whitespace");await UiText("Location","  Pump room  ");await UiKey("Enter");await UiSnapshot("ui-success");
            }
            await UiLayout(320);await UiLayout(1024);uiStage="ui-motion";
            await UiProtocol("Emulation.setEmulatedMedia","{\"features\":[{\"name\":\"prefers-reduced-motion\",\"value\":\"reduce\"}]}");await Task.Delay(50);
            EmitUi("ui-motion",await ReadUiScript(UiMotionScript));
            Emit("ui_complete",0,0,"candidate_observed");
        } catch(UiCandidateFailure error) {Emit("ui_failure",0,0,uiStage+":"+error.Message);Emit("ui_complete",0,0,"candidate_failed");}
    }
}
}
