// SPDX-License-Identifier: Apache-2.0
// Parent-side reconstruction and independent assertions. No browser or native calls.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Vcp.Cs3Draft {
public class DomEvidence {
    string[] Names;
    Action<string,string> validator;
    string terminal;
    public DomEvidence() : this(false) { }
    protected DomEvidence(bool web) {
        Names=web?new[]{"web-initial","web-tab","web-invalid","web-success","web-ax","web-poll-error","web-poll-success","web-hostile"}:new[]{"initial","focused","invalid","filled","success","accessibility","origin"};
        validator=web?(Action<string,string>)FrozenWebEvidence.Validate:Validate; terminal=web?"frozen_web_v1":"form_and_origin";
    }
    protected void Configure(string[] names,Action<string,string> validate,string complete) { if(next!=0 || pending!=null) throw new IOException("Evidence already started"); Names=names;validator=validate;terminal=complete; }
    protected void FinishPrefix() { if(failed || completed || pending!=null) throw new IOException("Invalid partial evidence terminal");completed=true; }
    static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
    int next, total, lifetime; string hash; MemoryStream pending;
    bool completed, failed;
    public bool Complete { get { return completed && !failed; } }
    public int Documents { get { return next; } }

    public void Add(string kind) {
        try { AddCore(kind); } catch { failed = true; throw; }
    }
    void AddCore(string kind) {
        if (failed || completed || next >= Names.Length || kind == null || kind.Length > 256) throw new IOException("Unexpected DOM chunk");
        string[] fields = kind.Split(':');
        if (fields.Length != 5 || fields[0] != Names[next] || !Regex.IsMatch(fields[1], "\\A(0|[1-9][0-9]{0,4})\\z") || !Regex.IsMatch(fields[2], "\\A[1-9][0-9]{0,4}\\z") || !Regex.IsMatch(fields[3], "\\A[a-f0-9]{64}\\z")) throw new IOException("DOM chunk identity/offset malformed");
        int offset = Int32.Parse(fields[1]), length = Int32.Parse(fields[2]);
        if (length > 8192 || fields[4].Length > 128) throw new IOException("DOM chunk byte ceiling");
        byte[] bytes = Convert.FromBase64String(fields[4]);
        if (bytes.Length == 0 || bytes.Length > 96 || Convert.ToBase64String(bytes) != fields[4]) throw new IOException("Noncanonical DOM chunk encoding");
        if (pending == null) { if (offset != 0 || lifetime + length > 16384) throw new IOException("DOM start/lifetime ceiling"); total = length; hash = fields[3]; pending = new MemoryStream(); }
        if (length != total || fields[3] != hash || offset != pending.Length || offset + bytes.Length > total) throw new IOException("DOM chunk coverage mismatch");
        pending.Write(bytes, 0, bytes.Length);
        if (pending.Length == total) {
            byte[] all = pending.ToArray();
            if (Convert.ToHexString(SHA256.HashData(all)).ToLowerInvariant() != hash) throw new IOException("DOM snapshot hash differs");
            validator(Names[next],Utf8.GetString(all));
            lifetime += all.Length; next++; pending.Dispose(); pending = null;
        }
    }
    public void Finish(string kind) {
        if (failed || completed || pending != null || next != Names.Length || kind != terminal) { failed = true; throw new IOException("Incomplete DOM evidence"); }
        completed = true;
    }
    static void Exact(JsonElement value, params string[] names) {
        if (value.ValueKind != JsonValueKind.Object) throw new IOException("DOM object required");
        var expected = new HashSet<string>(names);
        foreach (var p in value.EnumerateObject()) if (!expected.Remove(p.Name)) throw new IOException("Duplicate/unknown DOM field");
        if (expected.Count != 0) throw new IOException("Missing DOM field");
    }
    static void Equal(JsonElement value, string name, string expected) {
        if (value.GetProperty(name).ValueKind != JsonValueKind.String || value.GetProperty(name).GetString() != expected) throw new IOException("DOM oracle mismatch: " + name);
    }
    static void Equal(JsonElement value, string name, int expected) {
        int actual;
        if (value.GetProperty(name).ValueKind != JsonValueKind.Number || !value.GetProperty(name).TryGetInt32(out actual) || actual != expected) throw new IOException("DOM oracle mismatch: " + name);
    }
    static void True(JsonElement value, string name) {
        if (value.GetProperty(name).ValueKind != JsonValueKind.True) throw new IOException("DOM oracle mismatch: " + name);
    }
    static void UniqueJson(JsonElement value) {
        if (value.ValueKind == JsonValueKind.Object) {
            var names = new HashSet<string>();
            foreach (var property in value.EnumerateObject()) {
                if (!names.Add(property.Name)) throw new IOException("Duplicate JSON field");
                UniqueJson(property.Value);
            }
        } else if (value.ValueKind == JsonValueKind.Array) {
            foreach (var item in value.EnumerateArray()) UniqueJson(item);
        }
    }
    public static void Validate(string name, string json) {
        if (json == null || Utf8.GetByteCount(json) > 8192) throw new IOException("DOM document ceiling");
        using (var doc = JsonDocument.Parse(json, new JsonDocumentOptions { MaxDepth = 16 })) {
            var value = doc.RootElement; UniqueJson(value);
            if (name == "origin") {
                Exact(value, "url", "origin"); Equal(value, "url", "https://cs3-fixture.invalid/form.html"); Equal(value, "origin", "https://cs3-fixture.invalid"); return;
            }
            if (name == "accessibility") { Accessibility(value); return; }
            if (name != "initial" && name != "focused" && name != "invalid" && name != "filled" && name != "success") throw new IOException("Unknown DOM oracle");
            Exact(value, "url", "readyState", "title", "name", "value", "required", "error", "status", "active",
                "scriptReady", "keyDowns", "keyPresses", "keyUps", "submits", "lastKey", "trustedKeys",
                "readinessDowns", "readinessKeyPresses", "readinessUps", "readinessRepeats", "readinessSequence", "readinessTrusted");
            Equal(value, "url", "https://cs3-fixture.invalid/form.html"); Equal(value, "readyState", "complete");
            Equal(value, "title", "CS-3 form fixture"); Equal(value, "name", "Name");
            if (value.GetProperty("required").ValueKind != JsonValueKind.True) throw new IOException("Required input missing");
            Equal(value, "value", name == "filled" || name == "success" ? "Ada" : "");
            Equal(value, "error", name == "invalid" ? "Name is required." : "");
            Equal(value, "status", name == "success" ? "Saved Ada." : "");
            Equal(value, "active", name == "initial" ? "BODY" : "name");
            True(value, "scriptReady"); True(value, "trustedKeys");
            int events = name == "success" ? 2 : (name == "invalid" || name == "filled" ? 1 : 0);
            Equal(value, "keyDowns", events); Equal(value, "keyPresses", events); Equal(value, "keyUps", events); Equal(value, "submits", events);
            Equal(value, "lastKey", events == 0 ? "" : "Enter");
            int readiness = name == "initial" ? 0 : (name == "filled" || name == "success" ? 2 : 1);
            Equal(value, "readinessDowns", readiness); Equal(value, "readinessKeyPresses", 0); Equal(value, "readinessUps", readiness); Equal(value, "readinessRepeats", 0);
            Equal(value, "readinessSequence", readiness == 0 ? "" : (readiness == 1 ? "DU" : "DUDU")); True(value, "readinessTrusted");
        }
    }
    static void Accessibility(JsonElement root) {
        Exact(root, "schema", "source_method", "raw_utf8_bytes", "raw_sha256", "raw_node_count", "nodes");
        Equal(root, "schema", "cs3-accessibility-projection/1"); Equal(root, "source_method", "Accessibility.getFullAXTree");
        int rawBytes, rawNodes;
        if (root.GetProperty("raw_utf8_bytes").ValueKind != JsonValueKind.Number || !root.GetProperty("raw_utf8_bytes").TryGetInt32(out rawBytes) || rawBytes <= 0 || rawBytes > 64*1024) throw new IOException("AX raw byte ceiling");
        if (root.GetProperty("raw_node_count").ValueKind != JsonValueKind.Number || !root.GetProperty("raw_node_count").TryGetInt32(out rawNodes) || rawNodes < 5 || rawNodes > 128) throw new IOException("AX raw node ceiling");
        var rawHash=root.GetProperty("raw_sha256");
        if (rawHash.ValueKind != JsonValueKind.String || !Regex.IsMatch(rawHash.GetString(),"\\A[a-f0-9]{64}\\z")) throw new IOException("AX raw hash differs");
        var nodes = root.GetProperty("nodes");
        if (nodes.ValueKind != JsonValueKind.Array || nodes.GetArrayLength() != 5) throw new IOException("AX projection node coverage differs");
        string[] roles={"RootWebArea","textbox","button","alert","status"};
        string[] names={"CS-3 form fixture","Name","Save"};
        string[] values={"","Ada","","",""};
        var nodeIds = new HashSet<string>();
        int index=0;
        foreach (var node in nodes.EnumerateArray()) {
            Exact(node,"node_id","role","name","value","required");
            var nodeId=node.GetProperty("node_id");
            if (nodeId.ValueKind != JsonValueKind.String || String.IsNullOrEmpty(nodeId.GetString()) || nodeId.GetString().Length > 128 || !nodeIds.Add(nodeId.GetString())) throw new IOException("AX projection node identity differs");
            Equal(node,"role",roles[index]);
            var name=node.GetProperty("name");
            if(name.ValueKind!=JsonValueKind.String || name.GetString().Length>256 || (index<names.Length && name.GetString()!=names[index])) throw new IOException("AX projection name differs");
            Equal(node,"value",values[index]);
            var required=node.GetProperty("required");
            if ((index==1 && required.ValueKind!=JsonValueKind.True) || (index!=1 && required.ValueKind!=JsonValueKind.False)) throw new IOException("AX projection required state differs");
            index++;
        }
    }
    public static int Test() {
        int checks = 0;
        Action<Action> reject = action => { bool failed = false; try { action(); } catch { failed = true; } if (!failed) throw new Exception("Expected DOM rejection"); checks++; };
        string initial = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"BODY\",\"scriptReady\":true,\"keyDowns\":0,\"keyPresses\":0,\"keyUps\":0,\"submits\":0,\"lastKey\":\"\",\"trustedKeys\":true,\"readinessDowns\":0,\"readinessKeyPresses\":0,\"readinessUps\":0,\"readinessRepeats\":0,\"readinessSequence\":\"\",\"readinessTrusted\":true}";
        string focused = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":0,\"keyPresses\":0,\"keyUps\":0,\"submits\":0,\"lastKey\":\"\",\"trustedKeys\":true,\"readinessDowns\":1,\"readinessKeyPresses\":0,\"readinessUps\":1,\"readinessRepeats\":0,\"readinessSequence\":\"DU\",\"readinessTrusted\":true}";
        string invalid = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"Name is required.\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":1,\"keyPresses\":1,\"keyUps\":1,\"submits\":1,\"lastKey\":\"Enter\",\"trustedKeys\":true,\"readinessDowns\":1,\"readinessKeyPresses\":0,\"readinessUps\":1,\"readinessRepeats\":0,\"readinessSequence\":\"DU\",\"readinessTrusted\":true}";
        string filled = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"Ada\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":1,\"keyPresses\":1,\"keyUps\":1,\"submits\":1,\"lastKey\":\"Enter\",\"trustedKeys\":true,\"readinessDowns\":2,\"readinessKeyPresses\":0,\"readinessUps\":2,\"readinessRepeats\":0,\"readinessSequence\":\"DUDU\",\"readinessTrusted\":true}";
        string success = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"Ada\",\"required\":true,\"error\":\"\",\"status\":\"Saved Ada.\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":2,\"keyPresses\":2,\"keyUps\":2,\"submits\":2,\"lastKey\":\"Enter\",\"trustedKeys\":true,\"readinessDowns\":2,\"readinessKeyPresses\":0,\"readinessUps\":2,\"readinessRepeats\":0,\"readinessSequence\":\"DUDU\",\"readinessTrusted\":true}";
        string origin = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"origin\":\"https://cs3-fixture.invalid\"}";
        string root = "{\"node_id\":\"1\",\"role\":\"RootWebArea\",\"name\":\"CS-3 form fixture\",\"value\":\"\",\"required\":false}";
        string textbox = "{\"node_id\":\"2\",\"role\":\"textbox\",\"name\":\"Name\",\"value\":\"Ada\",\"required\":true}";
        string button = "{\"node_id\":\"3\",\"role\":\"button\",\"name\":\"Save\",\"value\":\"\",\"required\":false}";
        string alert = "{\"node_id\":\"4\",\"role\":\"alert\",\"name\":\"\",\"value\":\"\",\"required\":false}";
        string status = "{\"node_id\":\"5\",\"role\":\"status\",\"name\":\"Saved Ada.\",\"value\":\"\",\"required\":false}";
        string accessibility = "{\"schema\":\"cs3-accessibility-projection/1\",\"source_method\":\"Accessibility.getFullAXTree\",\"raw_utf8_bytes\":12345,\"raw_sha256\":\"" + new string('a',64) + "\",\"raw_node_count\":9,\"nodes\":[" + root + "," + textbox + "," + button + "," + alert + "," + status + "]}";

        Action<DomEvidence,string,string> add = (receipt, name, text) => {
            byte[] data = Utf8.GetBytes(text); string hash = Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();
            for (int i = 0; i < data.Length; i += 96) receipt.Add(name + ":" + i + ":" + data.Length + ":" + hash + ":" + Convert.ToBase64String(data, i, Math.Min(96, data.Length-i)));
        };
        Func<string,int,string> pad = (text, bytes) => {
            int length = Utf8.GetByteCount(text); if (length > bytes) throw new Exception("Test document exceeds pad target");
            return text + new string(' ', bytes-length);
        };
        Validate("initial",initial); Validate("focused",focused); Validate("invalid",invalid); Validate("filled",filled); Validate("success",success);
        Validate("accessibility",accessibility); Validate("origin",origin); checks += 7;
        var complete = new DomEvidence();
        add(complete,"initial",initial); add(complete,"focused",focused); add(complete,"invalid",invalid); add(complete,"filled",filled); add(complete,"success",success);
        add(complete,"accessibility",accessibility); add(complete,"origin",origin);
        if(complete.Documents!=7 || complete.Complete) throw new Exception("Seven reconstructed documents must await terminal record"); checks++;
        complete.Finish("form_and_origin");
        if(!complete.Complete || complete.Documents!=7) throw new Exception("Full DOM evidence did not complete"); checks++;

        reject(() => Validate("initial",initial.Replace("\"required\":true","\"required\":false")));
        reject(() => Validate("initial",initial.Replace("\"value\":\"\"","\"value\":\"\",\"value\":\"Ada\"")));
        reject(() => Validate("initial",initial.Replace("\"scriptReady\":true","\"scriptReady\":false")));
        reject(() => Validate("invalid",invalid.Replace("\"trustedKeys\":true","\"trustedKeys\":false")));
        reject(() => Validate("focused",focused.Replace("\"readinessUps\":1","\"readinessUps\":0")));
        reject(() => Validate("focused",focused.Replace("\"readinessKeyPresses\":0","\"readinessKeyPresses\":1")));
        reject(() => Validate("focused",focused.Replace("\"readinessRepeats\":0","\"readinessRepeats\":1")));
        reject(() => Validate("focused",focused.Replace("\"readinessSequence\":\"DU\"","\"readinessSequence\":\"UD\"")));
        reject(() => Validate("focused",focused.Replace("\"readinessTrusted\":true","\"readinessTrusted\":false")));
        reject(() => Validate("invalid",invalid.Replace("\"readinessDowns\":1","\"readinessDowns\":2")));
        reject(() => Validate("filled",filled.Replace("\"submits\":1","\"submits\":2")));
        reject(() => Validate("filled",filled.Replace("\"status\":\"\"","\"status\":\"Saved Ada.\"")));
        reject(() => Validate("success",success.Replace("\"readinessSequence\":\"DUDU\"","\"readinessSequence\":\"DUDUDU\"")));
        reject(() => Validate("invalid",invalid.Replace("\"keyDowns\":1","\"keyDowns\":0")));
        reject(() => Validate("focused",focused.Replace("\"active\":\"name\"","\"active\":\"BODY\"")));
        reject(() => Validate("focused",focused.Replace("\"status\":\"\"","\"status\":\"Saved Ada.\"")));
        reject(() => Validate("success",initial));
        reject(() => Validate("origin","{\"url\":\"https://blocked.invalid/\",\"origin\":\"https://blocked.invalid\"}"));
        reject(() => Validate("accessibility","{\"nodes\":[]}"));

        string missingButton = accessibility.Replace(","+button,"");
        string falseRequired = accessibility.Replace("\"value\":\"Ada\",\"required\":true","\"value\":\"Ada\",\"required\":false");
        string wrongValue = accessibility.Replace("\"value\":\"Ada\"","\"value\":\"Eve\"");
        string nestedDuplicate = accessibility.Replace("\"role\":\"textbox\"","\"role\":\"button\",\"role\":\"textbox\"");
        string duplicateNodeId = accessibility.Replace("\"node_id\":\"3\"","\"node_id\":\"2\"");
        string missingNodeId = accessibility.Replace("\"node_id\":\"4\",","");
        reject(() => Validate("accessibility",missingButton));
        reject(() => Validate("accessibility",falseRequired));
        reject(() => Validate("accessibility",wrongValue));
        reject(() => Validate("accessibility",nestedDuplicate));
        reject(() => Validate("accessibility",duplicateNodeId));
        reject(() => Validate("accessibility",missingNodeId));
        reject(() => Validate("accessibility",accessibility.Replace("cs3-accessibility-projection/1","wrong")));
        reject(() => Validate("accessibility",accessibility.Replace("Accessibility.getFullAXTree","Accessibility.getPartialAXTree")));
        reject(() => Validate("accessibility",accessibility.Replace("12345","65537")));
        reject(() => Validate("accessibility",accessibility.Replace(new string('a',64),new string('A',64))));
        reject(() => Validate("accessibility",accessibility.Replace("\"raw_node_count\":9","\"raw_node_count\":129")));
        Validate("accessibility",accessibility.Replace("Saved Ada.","")); checks++;
        reject(() => Validate("accessibility",accessibility.Replace("Saved Ada.",new string('x',257))));

        string exact8k=pad(initial,8192); Validate("initial",exact8k); checks++;
        reject(() => Validate("initial",pad(initial,8193)));
        byte[] ninetySeven=Enumerable.Repeat((byte)'x',97).ToArray(); string ninetySevenHash=Convert.ToHexString(SHA256.HashData(ninetySeven)).ToLowerInvariant();
        reject(() => new DomEvidence().Add("initial:0:97:"+ninetySevenHash+":"+Convert.ToBase64String(ninetySeven)));

        byte[] initialBytes=Utf8.GetBytes(initial); string initialHash=Convert.ToHexString(SHA256.HashData(initialBytes)).ToLowerInvariant();
        string first=Convert.ToBase64String(initialBytes,0,Math.Min(96,initialBytes.Length));
        string badHash=(initialHash[0]=='a'?'b':'a')+initialHash.Substring(1);
        var tampered=new DomEvidence();
        for(int i=0;i<initialBytes.Length;i+=96) {
            string data=Convert.ToBase64String(initialBytes,i,Math.Min(96,initialBytes.Length-i));
            if(i+96>=initialBytes.Length) reject(() => tampered.Add("initial:"+i+":"+initialBytes.Length+":"+badHash+":"+data));
            else tampered.Add("initial:"+i+":"+initialBytes.Length+":"+badHash+":"+data);
        }
        reject(() => tampered.Finish("form_and_origin"));
        reject(() => new DomEvidence().Add("initial:1:"+initialBytes.Length+":"+initialHash+":"+first));
        byte[] invalidBytes=Utf8.GetBytes(invalid); string invalidHash=Convert.ToHexString(SHA256.HashData(invalidBytes)).ToLowerInvariant();
        reject(() => new DomEvidence().Add("invalid:0:"+invalidBytes.Length+":"+invalidHash+":"+Convert.ToBase64String(invalidBytes,0,Math.Min(96,invalidBytes.Length))));
        reject(() => new DomEvidence().Add("unknown:0:"+initialBytes.Length+":"+initialHash+":"+first));

        var latched=new DomEvidence();
        reject(() => latched.Add("initial:1:"+initialBytes.Length+":"+initialHash+":"+first));
        reject(() => latched.Add("initial:0:"+initialBytes.Length+":"+initialHash+":"+first));
        reject(() => latched.Finish("form_and_origin"));
        reject(() => complete.Add("origin:0:1:"+new string('a',64)+":eA=="));
        if(complete.Complete) throw new Exception("Post-terminal input must permanently invalidate completion"); checks++;
        reject(() => complete.Finish("form_and_origin"));

        var lifetime=new DomEvidence();
        add(lifetime,"initial",pad(initial,8192)); add(lifetime,"focused",pad(focused,8192));
        byte[] invalidBytesAtCeiling=Utf8.GetBytes(invalid); string invalidHashAtCeiling=Convert.ToHexString(SHA256.HashData(invalidBytesAtCeiling)).ToLowerInvariant();
        reject(() => lifetime.Add("invalid:0:"+invalidBytesAtCeiling.Length+":"+invalidHashAtCeiling+":"+Convert.ToBase64String(invalidBytesAtCeiling,0,Math.Min(96,invalidBytesAtCeiling.Length))));
        reject(() => lifetime.Finish("form_and_origin"));
        return checks;
    }
}
}
