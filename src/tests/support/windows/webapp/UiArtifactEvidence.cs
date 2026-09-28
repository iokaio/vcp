// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using Vcp.Qualification.Webapp;
namespace Vcp.Cs3Draft {
public sealed class UiArtifactEvidence : DomEvidence {
    readonly Dictionary<string,bool> checks=new Dictionary<string,bool>();
    readonly bool picker;
    public bool Passed { get { return Complete && checks.Values.All(x=>x); } }
    public object[] Assertions { get { return checks.Select(x=>(object)new {name=x.Key,passed=x.Value}).ToArray(); } }
    public UiArtifactEvidence() : this(UiArtifactResource.CaseId) { }
    UiArtifactEvidence(string caseId) {
        picker=caseId=="UI-cs3-filter-selection-v1";
        string[] names=picker?new[]{"ui-initial","ui-keyboard","ui-filter","ui-selected","ui-hidden","ui-cleared","ui-no-match","ui-layout-320","ui-layout-1024","ui-motion"}:new[]{"ui-initial","ui-keyboard","ui-disclosed","ui-invalid","ui-whitespace","ui-success","ui-layout-320","ui-layout-1024","ui-motion"};
        Configure(names,Grade,"candidate_observed");
        foreach(string key in new[]{"keyboard_reachability","focus_visible","layout_320","layout_1024","reduced_motion","no_outbound_request","initial_state"})checks.Add(key,false);
        foreach(string key in picker?new[]{"filter_case_insensitive","selection_persists_hidden","clear_hidden_selection","no_match"}:new[]{"disclosure_relationship","empty_validation","whitespace_validation","trimmed_success_no_reload"})checks.Add(key,false);
    }
    public new void Finish(string kind) {
        if(kind=="candidate_failed") {FinishPrefix();return;}
        base.Finish(kind); checks["no_outbound_request"]=true;
    }
    static void Exact(JsonElement r,params string[] keys) {var left=new HashSet<string>(keys);foreach(var p in r.EnumerateObject())if(!left.Remove(p.Name))throw new IOException("UI evidence fields differ");if(left.Count!=0)throw new IOException("UI evidence fields absent");}
    static string S(JsonElement r,string k) {var v=r.GetProperty(k);if(v.ValueKind!=JsonValueKind.String || v.GetString().Length>256)throw new IOException("UI string bound");return v.GetString();}
    static bool B(JsonElement r,string k) {return r.GetProperty(k).GetBoolean();}
    static string[] A(JsonElement r,string k) {var a=r.GetProperty(k);if(a.ValueKind!=JsonValueKind.Array || a.GetArrayLength()>32)throw new IOException("UI array bound");return a.EnumerateArray().Select(x=>{if(x.ValueKind!=JsonValueKind.String || x.GetString().Length>128)throw new IOException("UI array string bound");return x.GetString();}).ToArray();}
    void Grade(string id,string json) {
        using(var doc=JsonDocument.Parse(json,new JsonDocumentOptions{MaxDepth=8})) {
            var r=doc.RootElement;
            if(id=="ui-keyboard") {Exact(r,"targets","allFocusVisible");var expected=picker?new[]{"Search","Cedar","Birch","Elm","Clear selection"}:new[]{"Details","Location","Send request"};var targets=A(r,"targets");checks["keyboard_reachability"]=expected.All(targets.Contains);checks["focus_visible"]=B(r,"allFocusVisible")&&checks["keyboard_reachability"];return;}
            if(id=="ui-layout-320"||id=="ui-layout-1024") {Exact(r,"viewport","clientWidth","scrollWidth");int expected=id=="ui-layout-320"?320:1024;int viewport=r.GetProperty("viewport").GetInt32(),client=r.GetProperty("clientWidth").GetInt32(),scroll=r.GetProperty("scrollWidth").GetInt32();checks[expected==320?"layout_320":"layout_1024"]=viewport==expected&&client>0&&client<=expected&&scroll<=client+1;return;}
            if(id=="ui-motion") {Exact(r,"reduced","animationMs","transitionMs");double animation=r.GetProperty("animationMs").GetDouble(),transition=r.GetProperty("transitionMs").GetDouble();if(Double.IsNaN(animation)||Double.IsNaN(transition)||animation<0||transition<0)throw new IOException("UI motion number");checks["reduced_motion"]=B(r,"reduced")&&animation<=0.01&&transition<=0.01;return;}
            Exact(r,"url","title","heading","searchLabel","visibleTasks","checkedTasks","status","noMatch","detailsLabel","expanded","controls","detailsVisible","locationLabel","locationValue","alert","active");
            foreach(string key in new[]{"url","title","heading","searchLabel","status","detailsLabel","expanded","locationLabel","locationValue","alert","active"})S(r,key);
            foreach(string key in new[]{"noMatch","controls","detailsVisible"})B(r,key);
            A(r,"visibleTasks");A(r,"checkedTasks");
            bool origin=S(r,"url")==UiArtifactResource.Resource.Url;
            if(picker) {
                string[] visible=A(r,"visibleTasks"),selected=A(r,"checkedTasks");string status=S(r,"status");
                var integers=System.Text.RegularExpressions.Regex.Matches(status,"-?[0-9]+");
                Func<int,bool> count=n=>integers.Count==1&&integers[0].Value==n.ToString(System.Globalization.CultureInfo.InvariantCulture);
                if(id=="ui-initial")checks["initial_state"]=origin&&S(r,"title")=="Task picker"&&S(r,"searchLabel")=="Search"&&visible.SequenceEqual(new[]{"Cedar","Birch","Elm"})&&selected.Length==0&&count(0);
                else if(id=="ui-filter")checks["filter_case_insensitive"]=origin&&visible.SequenceEqual(new[]{"Birch"})&&selected.Length==0&&count(0);
                else if(id=="ui-selected")checks["selection_persists_hidden"]=origin&&selected.SequenceEqual(new[]{"Birch"})&&count(1);
                else if(id=="ui-hidden")checks["selection_persists_hidden"]&=origin&&selected.SequenceEqual(new[]{"Birch"})&&visible.SequenceEqual(new[]{"Cedar"})&&count(1);
                else if(id=="ui-cleared")checks["clear_hidden_selection"]=origin&&selected.Length==0&&count(0);
                else if(id=="ui-no-match")checks["no_match"]=origin&&visible.Length==0&&B(r,"noMatch");
                else throw new IOException("Unknown UI picker stage");
            } else {
                if(id=="ui-initial")checks["initial_state"]=origin&&S(r,"heading")=="Maintenance request"&&S(r,"detailsLabel")=="Details"&&S(r,"expanded")=="false"&&B(r,"controls")&&!B(r,"detailsVisible")&&S(r,"locationLabel")=="Location"&&S(r,"status")==""&&S(r,"alert")=="";
                else if(id=="ui-disclosed")checks["disclosure_relationship"]=origin&&S(r,"expanded")=="true"&&B(r,"controls")&&B(r,"detailsVisible");
                else if(id=="ui-invalid"||id=="ui-whitespace")checks[id=="ui-invalid"?"empty_validation":"whitespace_validation"]=origin&&S(r,"alert")=="Location required"&&S(r,"status")==""&&S(r,"active")=="Location";
                else if(id=="ui-success")checks["trimmed_success_no_reload"]=origin&&S(r,"status")=="Request queued for Pump room"&&S(r,"locationValue")=="  Pump room  "&&S(r,"alert")=="";
                else throw new IOException("Unknown UI form stage");
            }
        }
    }
    public new static int Test() {
        int checks=0;Action<bool> check=ok=>{if(!ok)throw new Exception("UI parent oracle assertion failed");checks++;};
        Func<string[],string[],string,string> state=(visible,selected,status)=>JsonSerializer.Serialize(new {url=UiArtifactResource.Resource.Url,title="Task picker",heading="",searchLabel="Search",visibleTasks=visible,checkedTasks=selected,status=status,noMatch=false,detailsLabel="",expanded="",controls=false,detailsVisible=false,locationLabel="",locationValue="",alert="",active="Search"});
        var picker=new UiArtifactEvidence("UI-cs3-filter-selection-v1");
        string initial=state(new[]{"Cedar","Birch","Elm"},new string[0],"0 selected");picker.Grade("ui-initial",initial);check(picker.checks["initial_state"]);
        picker.Grade("ui-initial",initial.Replace("Task picker","Wrong title"));check(!picker.checks["initial_state"]);
        picker.Grade("ui-filter",state(new[]{"Birch"},new string[0],"0 selected"));check(picker.checks["filter_case_insensitive"]);
        picker.Grade("ui-filter",state(new[]{"Cedar","Birch"},new string[0],"0 selected"));check(!picker.checks["filter_case_insensitive"]);
        picker.Grade("ui-selected",state(new[]{"Birch"},new[]{"Birch"},"1 selected"));picker.Grade("ui-hidden",state(new[]{"Cedar"},new[]{"Birch"},"1 selected"));check(picker.checks["selection_persists_hidden"]);
        picker.Grade("ui-hidden",state(new[]{"Cedar"},new string[0],"0 selected"));check(!picker.checks["selection_persists_hidden"]);
        picker.Grade("ui-cleared",state(new[]{"Cedar"},new string[0],"0 selected"));check(picker.checks["clear_hidden_selection"]);
        picker.Grade("ui-keyboard","{\"targets\":[\"Search\",\"Cedar\",\"Birch\",\"Elm\",\"Clear selection\"],\"allFocusVisible\":true}");check(picker.checks["keyboard_reachability"]&&picker.checks["focus_visible"]);
        picker.Grade("ui-layout-320","{\"viewport\":320,\"clientWidth\":320,\"scrollWidth\":900}");check(!picker.checks["layout_320"]);
        picker.Grade("ui-layout-320","{\"viewport\":320,\"clientWidth\":320,\"scrollWidth\":320}");check(picker.checks["layout_320"]);
        picker.Grade("ui-motion","{\"reduced\":true,\"animationMs\":1000,\"transitionMs\":0}");check(!picker.checks["reduced_motion"]);
        picker.Grade("ui-motion","{\"reduced\":true,\"animationMs\":0,\"transitionMs\":0}");check(picker.checks["reduced_motion"]);
        var missing=new UiArtifactEvidence("UI-cs3-disclosure-form-v1");missing.Finish("candidate_failed");check(missing.Complete&&!missing.Passed&&missing.Assertions.Length==11);
        bool rejected=false;try{new UiArtifactEvidence().Finish("candidate_observed");}catch{rejected=true;}check(rejected);
        return checks;
    }
}
}
