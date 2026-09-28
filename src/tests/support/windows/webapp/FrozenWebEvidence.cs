// SPDX-License-Identifier: Apache-2.0
// Independently reconstructs bounded host chunks and grades exact WEB v1 states.
using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Text;
using System.Security.Cryptography;
namespace Vcp.Cs3Draft {
public sealed class FrozenWebEvidence : DomEvidence {
    public FrozenWebEvidence() : base(true) { }
    static void Exact(JsonElement r,params string[] keys) { var remaining=new HashSet<string>(keys); foreach(var p in r.EnumerateObject()) if(!remaining.Remove(p.Name)) throw new IOException("WEB unknown/duplicate field"); if(remaining.Count!=0) throw new IOException("WEB missing field"); }
    static void Equal(JsonElement r,string field,string expected) { if(r.GetProperty(field).GetString()!=expected) throw new IOException("WEB oracle mismatch: "+field); }
    static void Flag(JsonElement r,string field,bool expected) { if(r.GetProperty(field).GetBoolean()!=expected) throw new IOException("WEB boolean mismatch: "+field); }
    public new static void Validate(string name,string raw) {
        using(var doc=JsonDocument.Parse(raw,new JsonDocumentOptions {MaxDepth=8})) {
            var r=doc.RootElement;
            if(name=="web-hostile") { Exact(r,"url","origin"); Equal(r,"url","https://cs3-fixture.invalid/web-hostile/index.html"); Equal(r,"origin","https://cs3-fixture.invalid"); return; }
            if(name=="web-ax") {
                Exact(r,"raw_sha256","raw_bytes","nodes"); if(!Regex.IsMatch(r.GetProperty("raw_sha256").GetString(),"\\A[a-f0-9]{64}\\z") || r.GetProperty("raw_bytes").GetInt32()<1 || r.GetProperty("raw_bytes").GetInt32()>65536) throw new IOException("WEB AX source identity");
                var nodes=r.GetProperty("nodes"); if(nodes.GetArrayLength()!=4) throw new IOException("WEB AX role coverage");
                var roles=new HashSet<string>(new[]{"textbox","button","alert","status"});
                foreach(var node in nodes.EnumerateArray()) { Exact(node,"role","name","required"); string role=node.GetProperty("role").GetString(); if(!roles.Remove(role)) throw new IOException("WEB AX duplicate role"); if(role=="textbox") { Equal(node,"name","Name"); Flag(node,"required",true); } else { Flag(node,"required",false); if(role=="button") Equal(node,"name","Save"); } }
                if(roles.Count!=0) throw new IOException("WEB AX missing role"); return;
            }
            if(name=="web-poll-error" || name=="web-poll-success") {
                Exact(r,"url","status","retryHidden","items"); Equal(r,"url","https://cs3-fixture.invalid/web-poll/index.html"); bool success=name=="web-poll-success"; Equal(r,"status",success?"2 items":"Unable to load"); Flag(r,"retryHidden",success); var items=r.GetProperty("items"); if(items.GetArrayLength()!=(success?2:0)) throw new IOException("WEB polling row count"); if(success && (items[0].GetString()!="Alpha" || items[1].GetString()!="Beta")) throw new IOException("WEB polling row order"); return;
            }
            if(name!="web-initial" && name!="web-tab" && name!="web-invalid" && name!="web-success") throw new IOException("Unknown WEB oracle");
            Exact(r,"url","title","value","required","error","status","active"); Equal(r,"url","https://cs3-fixture.invalid/web-form/index.html"); Equal(r,"title","Contact"); Flag(r,"required",true); Equal(r,"value",name=="web-success"?"Ada":""); Equal(r,"error",name=="web-invalid"?"Name is required.":""); Equal(r,"status",name=="web-success"?"Saved Ada.":""); Equal(r,"active",name=="web-initial"?"BODY":"name");
        }
    }
    public new static int Test() {
        int checks=0;
        Action<Action> reject=action=>{bool rejected=false;try {action();} catch {rejected=true;} if(!rejected) throw new Exception("Expected WEB oracle rejection");checks++;};
        var documents=new Dictionary<string,string>();
        foreach(string name in new[]{"web-initial","web-tab","web-invalid","web-success"}) documents.Add(name,JsonSerializer.Serialize(new {url="https://cs3-fixture.invalid/web-form/index.html",title="Contact",value=name=="web-success"?"Ada":"",required=true,error=name=="web-invalid"?"Name is required.":"",status=name=="web-success"?"Saved Ada.":"",active=name=="web-initial"?"BODY":"name"}));
        documents.Add("web-ax",JsonSerializer.Serialize(new {raw_sha256=new string('a',64),raw_bytes=1000,nodes=new[]{new{role="textbox",name="Name",required=true},new{role="button",name="Save",required=false},new{role="alert",name="",required=false},new{role="status",name="",required=false}}}));
        documents.Add("web-poll-error",JsonSerializer.Serialize(new {url="https://cs3-fixture.invalid/web-poll/index.html",status="Unable to load",retryHidden=false,items=new string[0]}));
        documents.Add("web-poll-success",JsonSerializer.Serialize(new {url="https://cs3-fixture.invalid/web-poll/index.html",status="2 items",retryHidden=true,items=new[]{"Alpha","Beta"}}));
        documents.Add("web-hostile",JsonSerializer.Serialize(new {url="https://cs3-fixture.invalid/web-hostile/index.html",origin="https://cs3-fixture.invalid"}));
        var receipt=new FrozenWebEvidence();
        foreach(var pair in documents) {
            Validate(pair.Key,pair.Value);checks++;
            reject(()=>Validate(pair.Key,pair.Value.Substring(0,pair.Value.Length-1)+",\"extra\":true}"));
            byte[] bytes=Encoding.UTF8.GetBytes(pair.Value);string hash=Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
            for(int offset=0;offset<bytes.Length;offset+=96) receipt.Add(pair.Key+":"+offset+":"+bytes.Length+":"+hash+":"+Convert.ToBase64String(bytes,offset,Math.Min(96,bytes.Length-offset)));
        }
        receipt.Finish("frozen_web_v1");if(!receipt.Complete || receipt.Documents!=8) throw new Exception("WEB evidence incomplete");checks++;
        reject(()=>receipt.Finish("frozen_web_v1"));reject(()=>new FrozenWebEvidence().Finish("frozen_web_v1"));
        reject(()=>Validate("web-tab",documents["web-tab"].Replace("\"active\":\"name\"","\"active\":\"BODY\"")));
        reject(()=>Validate("web-invalid",documents["web-invalid"].Replace("Name is required.","")));
        reject(()=>Validate("web-success",documents["web-success"].Replace("Saved Ada.","Saved Bob.")));
        reject(()=>Validate("web-ax",documents["web-ax"].Replace("\"required\":true","\"required\":false")));
        reject(()=>Validate("web-poll-success",documents["web-poll-success"].Replace("\"Alpha\",\"Beta\"","\"Beta\",\"Alpha\"")));
        reject(()=>Validate("web-poll-success",documents["web-poll-success"].Replace("\"Alpha\",\"Beta\"","\"Alpha\",\"Beta\",\"Alpha\"")));
        reject(()=>Validate("web-hostile",documents["web-hostile"].Replace("https://cs3-fixture.invalid/web-hostile/index.html","https://blocked.invalid/")));
        return checks;
    }
}
}
