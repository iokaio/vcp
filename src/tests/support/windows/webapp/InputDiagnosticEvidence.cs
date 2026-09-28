// SPDX-License-Identifier: Apache-2.0
// Parent-side reconstruction of the routing-only input diagnostic. No browser or native calls.
using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Vcp.Cs3Draft {
public sealed class InputDiagnosticEvidence {
    static readonly string[] Names = { "target", "focus", "text", "key" };
    static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
    int next, total, lifetime; string hash; MemoryStream pending;
    bool completed, failed, targetVerified; Snapshot focus, text, key;

    public bool Complete { get { return completed && !failed; } }
    public int Documents { get { return next; } }
    public bool TargetVerified { get { return targetVerified; } }
    public bool TextInserted { get { return Complete && focus.Value == "" && text.Value == "Ada"; } }
    public bool KeyDelivered { get { return Complete && (key.KeyDowns > text.KeyDowns || key.KeyPresses > text.KeyPresses || key.KeyUps > text.KeyUps); } }
    public bool SubmissionObserved { get { return Complete && key.Submits > text.Submits; } }
    public int KeyDowns { get { return Complete ? key.KeyDowns : 0; } }
    public int KeyPresses { get { return Complete ? key.KeyPresses : 0; } }
    public int KeyUps { get { return Complete ? key.KeyUps : 0; } }
    public int Submits { get { return Complete ? key.Submits : 0; } }
    public string Status { get { return Complete ? key.Status : null; } }
    public string FocusValue { get { return Complete ? focus.Value : null; } }
    public string TextValue { get { return Complete ? text.Value : null; } }
    public string KeyValue { get { return Complete ? key.Value : null; } }

    struct Snapshot {
        public string Value, Status;
        public int KeyDowns, KeyPresses, KeyUps, Submits, NativeGotFocus, NativeLostFocus;
    }

    public void Add(string kind) {
        try { AddCore(kind); } catch { failed = true; throw; }
    }
    void AddCore(string kind) {
        if (failed || completed || next >= Names.Length || kind == null || kind.Length > 256) throw new IOException("Unexpected input diagnostic chunk");
        string[] fields = kind.Split(':');
        if (fields.Length != 5 || fields[0] != Names[next] || !Regex.IsMatch(fields[1], "\\A(0|[1-9][0-9]{0,4})\\z") ||
            !Regex.IsMatch(fields[2], "\\A[1-9][0-9]{0,4}\\z") || !Regex.IsMatch(fields[3], "\\A[a-f0-9]{64}\\z")) throw new IOException("Input diagnostic chunk identity/offset malformed");
        int offset = Int32.Parse(fields[1]), length = Int32.Parse(fields[2]);
        if (length > 8192 || fields[4].Length > 128) throw new IOException("Input diagnostic chunk byte ceiling");
        byte[] bytes = Convert.FromBase64String(fields[4]);
        if (bytes.Length == 0 || bytes.Length > 96 || Convert.ToBase64String(bytes) != fields[4]) throw new IOException("Noncanonical input diagnostic chunk encoding");
        if (pending == null) { if (offset != 0 || lifetime + length > 16384) throw new IOException("Input diagnostic start/lifetime ceiling"); total = length; hash = fields[3]; pending = new MemoryStream(); }
        if (length != total || fields[3] != hash || offset != pending.Length || offset + bytes.Length > total) throw new IOException("Input diagnostic chunk coverage mismatch");
        pending.Write(bytes, 0, bytes.Length);
        if (pending.Length == total) {
            byte[] all = pending.ToArray();
            if (Convert.ToHexString(SHA256.HashData(all)).ToLowerInvariant() != hash) throw new IOException("Input diagnostic document hash differs");
            Validate(Names[next], Utf8.GetString(all));
            lifetime += all.Length; next++; pending.Dispose(); pending = null;
        }
    }

    public void Finish(string kind) {
        try {
            if (failed || completed || pending != null || next != Names.Length || kind != "routing_only") throw new IOException("Incomplete input diagnostic evidence");
            if (!targetVerified) throw new IOException("Input diagnostic target was not verified");
            completed = true;
        } catch { failed = true; throw; }
    }

    void Validate(string name, string json) {
        if (json == null || Utf8.GetByteCount(json) > 8192) throw new IOException("Input diagnostic document ceiling");
        using (var doc = JsonDocument.Parse(json, new JsonDocumentOptions { MaxDepth = 16 })) {
            JsonElement value = doc.RootElement; UniqueJson(value);
            if (name == "target") { ValidateTarget(value); targetVerified = true; return; }
            Snapshot observation = ValidateSnapshot(value);
            if (name == "focus") focus = observation;
            else if (name == "text") { Monotonic(focus, observation); text = observation; }
            else if (name == "key") { Monotonic(text, observation); key = observation; }
            else throw new IOException("Unknown input diagnostic document");
        }
    }

    static void ValidateTarget(JsonElement root) {
        Exact(root, "result"); JsonElement result = root.GetProperty("result"); Exact(result, "type", "value");
        Equal(result, "type", "object"); JsonElement value = result.GetProperty("value"); Exact(value, "url", "title", "scriptReady");
        Equal(value, "url", "https://cs3-fixture.invalid/form.html"); Equal(value, "title", "CS-3 form fixture"); Boolean(value, "scriptReady", true);
    }

    static Snapshot ValidateSnapshot(JsonElement value) {
        Exact(value, "url", "readyState", "title", "name", "value", "required", "error", "status", "active", "scriptReady",
            "keyDowns", "keyPresses", "keyUps", "submits", "lastKey", "trustedKeys", "native_got_focus", "native_lost_focus", "native_focus_pid", "native_focus_present");
        Equal(value, "url", "https://cs3-fixture.invalid/form.html"); Equal(value, "readyState", "complete");
        Equal(value, "title", "CS-3 form fixture"); Equal(value, "name", "Name"); Boolean(value, "required", true); Boolean(value, "scriptReady", true);
        string observedValue = BoundedString(value, "value"), error = BoundedString(value, "error"), status = BoundedString(value, "status");
        BoundedString(value, "active"); BoundedString(value, "lastKey"); Boolean(value, "trustedKeys", null);
        int downs = BoundedCount(value, "keyDowns"), presses = BoundedCount(value, "keyPresses"), ups = BoundedCount(value, "keyUps"), submits = BoundedCount(value, "submits");
        int got = BoundedCount(value, "native_got_focus"), lost = BoundedCount(value, "native_lost_focus");
        uint focusPid; JsonElement pid = value.GetProperty("native_focus_pid");
        if (pid.ValueKind != JsonValueKind.Number || !pid.TryGetUInt32(out focusPid)) throw new IOException("Input diagnostic focus PID malformed");
        Boolean(value, "native_focus_present", null);
        return new Snapshot { Value=observedValue, Status=status, KeyDowns=downs, KeyPresses=presses, KeyUps=ups, Submits=submits, NativeGotFocus=got, NativeLostFocus=lost };
    }

    static void Monotonic(Snapshot before, Snapshot after) {
        if (after.KeyDowns < before.KeyDowns || after.KeyPresses < before.KeyPresses || after.KeyUps < before.KeyUps || after.Submits < before.Submits ||
            after.NativeGotFocus < before.NativeGotFocus || after.NativeLostFocus < before.NativeLostFocus) throw new IOException("Input diagnostic counters regressed");
    }
    static void Exact(JsonElement value, params string[] names) {
        if (value.ValueKind != JsonValueKind.Object) throw new IOException("Input diagnostic object required");
        var expected = new HashSet<string>(names);
        foreach (var property in value.EnumerateObject()) if (!expected.Remove(property.Name)) throw new IOException("Duplicate/unknown input diagnostic field");
        if (expected.Count != 0) throw new IOException("Missing input diagnostic field");
    }
    static void Equal(JsonElement value, string name, string expected) {
        if (value.GetProperty(name).ValueKind != JsonValueKind.String || value.GetProperty(name).GetString() != expected) throw new IOException("Input diagnostic invariant differs: " + name);
    }
    static string BoundedString(JsonElement value, string name) {
        JsonElement item = value.GetProperty(name); string text;
        if (item.ValueKind != JsonValueKind.String || (text=item.GetString()) == null || Utf8.GetByteCount(text) > 256) throw new IOException("Input diagnostic string bound: " + name);
        return text;
    }
    static int BoundedCount(JsonElement value, string name) {
        int count; JsonElement item = value.GetProperty(name);
        if (item.ValueKind != JsonValueKind.Number || !item.TryGetInt32(out count) || count < 0 || count > 16) throw new IOException("Input diagnostic counter bound: " + name);
        return count;
    }
    static bool Boolean(JsonElement value, string name, bool? expected) {
        JsonElement item = value.GetProperty(name);
        if (item.ValueKind != JsonValueKind.True && item.ValueKind != JsonValueKind.False) throw new IOException("Input diagnostic boolean required: " + name);
        bool actual = item.GetBoolean(); if (expected.HasValue && actual != expected.Value) throw new IOException("Input diagnostic invariant differs: " + name); return actual;
    }
    static void UniqueJson(JsonElement value) {
        if (value.ValueKind == JsonValueKind.Object) {
            var names = new HashSet<string>(); foreach (var property in value.EnumerateObject()) { if (!names.Add(property.Name)) throw new IOException("Duplicate JSON field"); UniqueJson(property.Value); }
        } else if (value.ValueKind == JsonValueKind.Array) foreach (var item in value.EnumerateArray()) UniqueJson(item);
    }

    public static int Test() {
        int checks=0; Action<bool> check=ok=>{if(!ok)throw new Exception("Pure input diagnostic assertion failed");checks++;};
        Action<Action> reject=action=>{bool failed=false;try{action();}catch{failed=true;}check(failed);};
        string target="{\"result\":{\"type\":\"object\",\"value\":{\"url\":\"https://cs3-fixture.invalid/form.html\",\"title\":\"CS-3 form fixture\",\"scriptReady\":true}}}";
        Func<string,int,int,int,int,int,int,uint,bool,string> snapshot=(value,downs,presses,ups,submits,got,lost,pid,present)=>
            "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\""+value+"\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":"+downs+",\"keyPresses\":"+presses+",\"keyUps\":"+ups+",\"submits\":"+submits+",\"lastKey\":\"\",\"trustedKeys\":true,\"native_got_focus\":"+got+",\"native_lost_focus\":"+lost+",\"native_focus_pid\":"+pid+",\"native_focus_present\":"+(present?"true":"false")+"}";
        Action<InputDiagnosticEvidence,string,string> add=(receipt,name,text)=>{byte[] data=Utf8.GetBytes(text);string digest=Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();for(int i=0;i<data.Length;i+=96)receipt.Add(name+":"+i+":"+data.Length+":"+digest+":"+Convert.ToBase64String(data,i,Math.Min(96,data.Length-i)));};
        string focus=snapshot("",0,0,0,0,1,0,0,false), text=snapshot("Ada",0,0,0,0,1,0,321,true), key=snapshot("Ada",1,1,1,1,1,0,321,true);
        var complete=new InputDiagnosticEvidence(); add(complete,"target",target); add(complete,"focus",focus); add(complete,"text",text); add(complete,"key",key);
        check(complete.Documents==4 && !complete.Complete); complete.Finish("routing_only");
        check(complete.Complete && complete.TargetVerified && complete.TextInserted && complete.KeyDelivered && complete.SubmissionObserved && complete.KeyDowns==1 && complete.Status=="");
        reject(()=>complete.Add("key:0:1:"+new string('a',64)+":eA==")); reject(()=>complete.Finish("routing_only"));
        var zero=new InputDiagnosticEvidence(); add(zero,"target",target); add(zero,"focus",focus); add(zero,"text",snapshot("",0,0,0,0,1,0,0,false)); add(zero,"key",snapshot("",0,0,0,0,1,0,0,false)); zero.Finish("routing_only");
        check(zero.Complete && !zero.TextInserted && !zero.KeyDelivered && !zero.SubmissionObserved);
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target.Replace("CS-3 form fixture","wrong"));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target.Replace("{\"result\":","{\"exceptionDetails\":{},\"result\":"));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target.Replace("{\"result\":{","{\"result\":{},\"result\":{"));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target","{}");});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"focus",focus);});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target);add(x,"focus",focus.Replace("\"keyDowns\":0","\"keyDowns\":17"));});
        var unknownFocusOwner=new InputDiagnosticEvidence(); add(unknownFocusOwner,"target",target); add(unknownFocusOwner,"focus",focus.Replace("\"native_focus_present\":false","\"native_focus_present\":true")); checks++;
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target);add(x,"focus",focus.Replace("\"status\":\"\"","\"status\":\"\",\"status\":\"again\""));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target);add(x,"focus",focus.Replace(",\"status\":\"\"",""));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target);add(x,"focus",focus);add(x,"text",text);add(x,"key",key.Replace("\"keyDowns\":1","\"keyDowns\":0").Replace("\"keyPresses\":1","\"keyPresses\":0").Replace("\"keyUps\":1","\"keyUps\":0").Replace("\"submits\":1","\"submits\":0").Replace("\"native_got_focus\":1","\"native_got_focus\":0"));});
        reject(()=>{var x=new InputDiagnosticEvidence();add(x,"target",target);add(x,"focus",focus);add(x,"text",text);add(x,"key",key);x.Finish("form_and_origin");});
        reject(()=>new InputDiagnosticEvidence().Finish("routing_only"));

        byte[] targetBytes=Utf8.GetBytes(target); string targetHash=Convert.ToHexString(SHA256.HashData(targetBytes)).ToLowerInvariant();
        string first=Convert.ToBase64String(targetBytes,0,Math.Min(96,targetBytes.Length));
        reject(()=>new InputDiagnosticEvidence().Add("target:1:"+targetBytes.Length+":"+targetHash+":"+first));
        var duplicateOffset=new InputDiagnosticEvidence(); duplicateOffset.Add("target:0:"+targetBytes.Length+":"+targetHash+":"+first);
        reject(()=>duplicateOffset.Add("target:0:"+targetBytes.Length+":"+targetHash+":"+first));
        var changedTotal=new InputDiagnosticEvidence(); changedTotal.Add("target:0:"+targetBytes.Length+":"+targetHash+":"+first);
        reject(()=>changedTotal.Add("target:96:"+(targetBytes.Length+1)+":"+targetHash+":"+Convert.ToBase64String(targetBytes,96,targetBytes.Length-96)));
        reject(()=>new InputDiagnosticEvidence().Add("target:0:8193:"+targetHash+":eA=="));
        reject(()=>new InputDiagnosticEvidence().Add("target:0:1:"+targetHash+":eA== "));
        string wrongHash=(targetHash[0]=='a'?'b':'a')+targetHash.Substring(1); var hashMismatch=new InputDiagnosticEvidence();
        for(int i=0;i<targetBytes.Length;i+=96) {
            string chunk="target:"+i+":"+targetBytes.Length+":"+wrongHash+":"+Convert.ToBase64String(targetBytes,i,Math.Min(96,targetBytes.Length-i));
            if(i+96>=targetBytes.Length) reject(()=>hashMismatch.Add(chunk)); else hashMismatch.Add(chunk);
        }
        var latched=new InputDiagnosticEvidence(); reject(()=>latched.Add("target:1:"+targetBytes.Length+":"+targetHash+":"+first));
        reject(()=>latched.Add("target:0:"+targetBytes.Length+":"+targetHash+":"+first));
        return checks;
    }
}
}
