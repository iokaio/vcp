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
public sealed class DomEvidence {
    static readonly string[] Names = { "initial", "focused", "invalid", "success", "accessibility", "origin" };
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
            Validate(Names[next], Utf8.GetString(all));
            lifetime += all.Length; next++; pending.Dispose(); pending = null;
        }
    }
    public void Finish(string kind) {
        if (failed || completed || pending != null || next != Names.Length || kind != "form_and_origin") { failed = true; throw new IOException("Incomplete DOM evidence"); }
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
            if (name != "initial" && name != "focused" && name != "invalid" && name != "success") throw new IOException("Unknown DOM oracle");
            Exact(value, "url", "readyState", "title", "name", "value", "required", "error", "status", "active",
                "scriptReady", "keyDowns", "keyPresses", "keyUps", "submits", "lastKey", "trustedKeys");
            Equal(value, "url", "https://cs3-fixture.invalid/form.html"); Equal(value, "readyState", "complete");
            Equal(value, "title", "CS-3 form fixture"); Equal(value, "name", "Name");
            if (value.GetProperty("required").ValueKind != JsonValueKind.True) throw new IOException("Required input missing");
            Equal(value, "value", name == "success" ? "Ada" : "");
            Equal(value, "error", name == "invalid" ? "Name is required." : "");
            Equal(value, "status", name == "success" ? "Saved Ada." : "");
            Equal(value, "active", name == "initial" ? "BODY" : "name");
            True(value, "scriptReady"); True(value, "trustedKeys");
            int events = name == "success" ? 2 : (name == "invalid" ? 1 : 0);
            Equal(value, "keyDowns", events); Equal(value, "keyPresses", events); Equal(value, "keyUps", events); Equal(value, "submits", events);
            Equal(value, "lastKey", events == 0 ? "" : "Enter");
        }
    }
    static string AxText(JsonElement node, string field, params string[] allowedTypes) {
        JsonElement outer, type, text;
        if (!node.TryGetProperty(field, out outer) || outer.ValueKind != JsonValueKind.Object ||
            !outer.TryGetProperty("type", out type) || type.ValueKind != JsonValueKind.String || !allowedTypes.Contains(type.GetString()) ||
            !outer.TryGetProperty("value", out text) || text.ValueKind != JsonValueKind.String) return null;
        return text.GetString();
    }
    static void Accessibility(JsonElement root) {
        Exact(root, "nodes"); var nodes = root.GetProperty("nodes");
        if (nodes.ValueKind != JsonValueKind.Array || nodes.GetArrayLength() == 0 || nodes.GetArrayLength() > 128) throw new IOException("AX node ceiling");
        bool document = false, textbox = false, button = false, alert = false, status = false;
        var nodeIds = new HashSet<string>();
        foreach (var node in nodes.EnumerateArray()) {
            if (node.ValueKind != JsonValueKind.Object) throw new IOException("AX node object required");
            var keys = new HashSet<string>(); foreach (var p in node.EnumerateObject()) if (!keys.Add(p.Name)) throw new IOException("Duplicate AX field");
            JsonElement nodeId;
            if (!node.TryGetProperty("nodeId", out nodeId) || nodeId.ValueKind != JsonValueKind.String ||
                String.IsNullOrEmpty(nodeId.GetString()) || !nodeIds.Add(nodeId.GetString())) throw new IOException("AX node identity differs");
            JsonElement ignored;
            if (!node.TryGetProperty("ignored", out ignored) || (ignored.ValueKind != JsonValueKind.True && ignored.ValueKind != JsonValueKind.False)) throw new IOException("AX ignored state missing");
            if (ignored.GetBoolean()) continue;
            string role = AxText(node, "role", "role", "internalRole"), name = AxText(node, "name", "computedString");
            if (role == "RootWebArea" && name == "CS-3 form fixture") document = true;
            if (role == "textbox" && name == "Name") {
                JsonElement properties; bool required = false;
                var propertyNames = new HashSet<string>();
                if (node.TryGetProperty("properties", out properties) && properties.ValueKind == JsonValueKind.Array) foreach (var property in properties.EnumerateArray()) {
                    JsonElement propertyName;
                    if (property.ValueKind != JsonValueKind.Object || !property.TryGetProperty("name",out propertyName) || propertyName.ValueKind != JsonValueKind.String || !propertyNames.Add(propertyName.GetString())) throw new IOException("AX property identity differs");
                    if (propertyName.GetString() == "required") {
                        JsonElement wrapped, wrappedType, wrappedValue;
                        if (!property.TryGetProperty("value",out wrapped) || wrapped.ValueKind != JsonValueKind.Object ||
                            !wrapped.TryGetProperty("type",out wrappedType) || wrappedType.ValueKind != JsonValueKind.String || wrappedType.GetString() != "booleanOrUndefined" ||
                            !wrapped.TryGetProperty("value",out wrappedValue) || (wrappedValue.ValueKind != JsonValueKind.True && wrappedValue.ValueKind != JsonValueKind.False)) throw new IOException("AX required wrapper differs");
                        required = wrappedValue.GetBoolean();
                    }
                }
                textbox = required && AxText(node, "value", "string") == "Ada";
            }
            if (role == "button" && name == "Save") button = true;
            if (role == "alert") alert = true;
            if (role == "status") status = true;
        }
        if (!document || !textbox || !button || !alert || !status) throw new IOException("Accessible form roles, name, value or required state missing");
    }
    public static int Test() {
        int checks = 0;
        Action<Action> reject = action => { bool failed = false; try { action(); } catch { failed = true; } if (!failed) throw new Exception("Expected DOM rejection"); checks++; };
        string initial = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"BODY\",\"scriptReady\":true,\"keyDowns\":0,\"keyPresses\":0,\"keyUps\":0,\"submits\":0,\"lastKey\":\"\",\"trustedKeys\":true}";
        string focused = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":0,\"keyPresses\":0,\"keyUps\":0,\"submits\":0,\"lastKey\":\"\",\"trustedKeys\":true}";
        string invalid = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"\",\"required\":true,\"error\":\"Name is required.\",\"status\":\"\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":1,\"keyPresses\":1,\"keyUps\":1,\"submits\":1,\"lastKey\":\"Enter\",\"trustedKeys\":true}";
        string success = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"readyState\":\"complete\",\"title\":\"CS-3 form fixture\",\"name\":\"Name\",\"value\":\"Ada\",\"required\":true,\"error\":\"\",\"status\":\"Saved Ada.\",\"active\":\"name\",\"scriptReady\":true,\"keyDowns\":2,\"keyPresses\":2,\"keyUps\":2,\"submits\":2,\"lastKey\":\"Enter\",\"trustedKeys\":true}";
        string origin = "{\"url\":\"https://cs3-fixture.invalid/form.html\",\"origin\":\"https://cs3-fixture.invalid\"}";
        string root = "{\"nodeId\":\"1\",\"ignored\":false,\"role\":{\"type\":\"internalRole\",\"value\":\"RootWebArea\"},\"name\":{\"type\":\"computedString\",\"value\":\"CS-3 form fixture\"},\"properties\":[]}";
        string textbox = "{\"nodeId\":\"2\",\"ignored\":false,\"role\":{\"type\":\"role\",\"value\":\"textbox\"},\"name\":{\"type\":\"computedString\",\"value\":\"Name\"},\"value\":{\"type\":\"string\",\"value\":\"Ada\"},\"properties\":[{\"name\":\"required\",\"value\":{\"type\":\"booleanOrUndefined\",\"value\":true}}]}";
        string button = "{\"nodeId\":\"3\",\"ignored\":false,\"role\":{\"type\":\"role\",\"value\":\"button\"},\"name\":{\"type\":\"computedString\",\"value\":\"Save\"},\"properties\":[]}";
        string alert = "{\"nodeId\":\"4\",\"ignored\":false,\"role\":{\"type\":\"role\",\"value\":\"alert\"},\"name\":{\"type\":\"computedString\",\"value\":\"\"},\"properties\":[]}";
        string status = "{\"nodeId\":\"5\",\"ignored\":false,\"role\":{\"type\":\"role\",\"value\":\"status\"},\"name\":{\"type\":\"computedString\",\"value\":\"Saved Ada.\"},\"properties\":[]}";
        string accessibility = "{\"nodes\":[" + root + "," + textbox + "," + button + "," + alert + "," + status + "]}";

        Action<DomEvidence,string,string> add = (receipt, name, text) => {
            byte[] data = Utf8.GetBytes(text); string hash = Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();
            for (int i = 0; i < data.Length; i += 96) receipt.Add(name + ":" + i + ":" + data.Length + ":" + hash + ":" + Convert.ToBase64String(data, i, Math.Min(96, data.Length-i)));
        };
        Func<string,int,string> pad = (text, bytes) => {
            int length = Utf8.GetByteCount(text); if (length > bytes) throw new Exception("Test document exceeds pad target");
            return text + new string(' ', bytes-length);
        };
        Func<int,string> axWithIgnored = count => {
            var nodes = new List<string> { root, textbox, button, alert, status };
            for (int i=0; i<count; i++) nodes.Add("{\"nodeId\":\"ignored-"+i+"\",\"ignored\":true}");
            return "{\"nodes\":["+String.Join(",",nodes)+"]}";
        };

        Validate("initial",initial); Validate("focused",focused); Validate("invalid",invalid); Validate("success",success);
        Validate("accessibility",accessibility); Validate("origin",origin); checks += 6;
        var complete = new DomEvidence();
        add(complete,"initial",initial); add(complete,"focused",focused); add(complete,"invalid",invalid); add(complete,"success",success);
        add(complete,"accessibility",accessibility); add(complete,"origin",origin);
        if(complete.Documents!=6 || complete.Complete) throw new Exception("Six reconstructed documents must await terminal record"); checks++;
        complete.Finish("form_and_origin");
        if(!complete.Complete || complete.Documents!=6) throw new Exception("Full DOM evidence did not complete"); checks++;

        reject(() => Validate("initial",initial.Replace("\"required\":true","\"required\":false")));
        reject(() => Validate("initial",initial.Replace("\"value\":\"\"","\"value\":\"\",\"value\":\"Ada\"")));
        reject(() => Validate("initial",initial.Replace("\"scriptReady\":true","\"scriptReady\":false")));
        reject(() => Validate("invalid",invalid.Replace("\"trustedKeys\":true","\"trustedKeys\":false")));
        reject(() => Validate("invalid",invalid.Replace("\"keyDowns\":1","\"keyDowns\":0")));
        reject(() => Validate("focused",focused.Replace("\"active\":\"name\"","\"active\":\"BODY\"")));
        reject(() => Validate("focused",focused.Replace("\"status\":\"\"","\"status\":\"Saved Ada.\"")));
        reject(() => Validate("success",initial));
        reject(() => Validate("origin","{\"url\":\"https://blocked.invalid/\",\"origin\":\"https://blocked.invalid\"}"));
        reject(() => Validate("accessibility","{\"nodes\":[]}"));

        string missingButton = "{\"nodes\":["+root+","+textbox+","+alert+","+status+"]}";
        string ignoredTextbox = accessibility.Replace("\"nodeId\":\"2\",\"ignored\":false","\"nodeId\":\"2\",\"ignored\":true");
        string falseRequired = accessibility.Replace("\"type\":\"booleanOrUndefined\",\"value\":true","\"type\":\"booleanOrUndefined\",\"value\":false");
        string wrongValue = accessibility.Replace("\"type\":\"string\",\"value\":\"Ada\"","\"type\":\"string\",\"value\":\"Eve\"");
        string nestedDuplicate = accessibility.Replace("\"role\":{\"type\":\"role\",\"value\":\"textbox\"}","\"role\":{\"type\":\"role\",\"value\":\"button\",\"value\":\"textbox\"}");
        string contradictoryRequired = accessibility.Replace("{\"name\":\"required\",\"value\":{\"type\":\"booleanOrUndefined\",\"value\":true}}","{\"name\":\"required\",\"value\":{\"type\":\"booleanOrUndefined\",\"value\":false}},{\"name\":\"required\",\"value\":{\"type\":\"booleanOrUndefined\",\"value\":true}}");
        string missingWrapperType = accessibility.Replace("\"name\":{\"type\":\"computedString\",\"value\":\"Name\"}","\"name\":{\"value\":\"Name\"}");
        string wrongValueType = accessibility.Replace("\"value\":{\"type\":\"string\",\"value\":\"Ada\"}","\"value\":{\"type\":\"computedString\",\"value\":\"Ada\"}");
        string wrongRequiredType = accessibility.Replace("\"type\":\"booleanOrUndefined\",\"value\":true","\"type\":\"boolean\",\"value\":true");
        string duplicateNodeId = accessibility.Replace("\"nodeId\":\"3\"","\"nodeId\":\"2\"");
        string missingNodeId = accessibility.Replace("\"nodeId\":\"4\",","");
        reject(() => Validate("accessibility",missingButton));
        reject(() => Validate("accessibility",ignoredTextbox));
        reject(() => Validate("accessibility",falseRequired));
        reject(() => Validate("accessibility",wrongValue));
        reject(() => Validate("accessibility",nestedDuplicate));
        reject(() => Validate("accessibility",contradictoryRequired));
        reject(() => Validate("accessibility",missingWrapperType));
        reject(() => Validate("accessibility",wrongValueType));
        reject(() => Validate("accessibility",wrongRequiredType));
        reject(() => Validate("accessibility",duplicateNodeId));
        reject(() => Validate("accessibility",missingNodeId));
        Validate("accessibility",axWithIgnored(123)); checks++;
        reject(() => Validate("accessibility",axWithIgnored(124)));

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
