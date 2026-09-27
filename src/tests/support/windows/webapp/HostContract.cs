// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Web.Script.Serialization;

namespace Vcp.Cs3WebViewDraft {
    public sealed class HostInput {
        public string Runtime, Profile, Version, Nonce;
        public static HostInput Parse(string[] args) {
            if (args.Length != 8 || args[0] != "--runtime" || args[2] != "--profile" || args[4] != "--version" || args[6] != "--nonce") throw new ArgumentException("Exact four input pairs required");
            var result = new HostInput { Runtime = Absolute(args[1]), Profile = Absolute(args[3]), Version = args[5], Nonce = args[7] };
            if (!Regex.IsMatch(result.Version, @"\A[0-9]{1,5}(\.[0-9]{1,5}){3}\z") || !Regex.IsMatch(result.Nonce, @"\A[a-f0-9]{64}\z")) throw new ArgumentException("Bounded exact version and nonce required");
            if (Within(result.Runtime, result.Profile) || Within(result.Profile, result.Runtime)) throw new ArgumentException("Runtime and profile cannot overlap");
            return result;
        }
        public static string Absolute(string value) {
            if (String.IsNullOrEmpty(value) || value.Length > 2048 || value.IndexOfAny(new[] {'\r','\n','\0','"'}) >= 0 || !Regex.IsMatch(value, @"\A[A-Za-z]:[\\/]") || value.Substring(2).Contains(":")) throw new ArgumentException("Absolute local drive path required");
            string full = Path.GetFullPath(value).TrimEnd('\\','/');
            if (full.Length <= 3) throw new ArgumentException("Drive root prohibited");
            return full;
        }
        static bool Within(string parent, string child) { return String.Equals(parent,child,StringComparison.OrdinalIgnoreCase) || child.StartsWith(parent+"\\",StringComparison.OrdinalIgnoreCase); }
        public static void NoOverrides(IDictionary environment) {
            foreach (object key in environment.Keys) {
                string name = Convert.ToString(key, CultureInfo.InvariantCulture);
                if (name.StartsWith("WEBVIEW2_", StringComparison.OrdinalIgnoreCase) || String.Equals(name,"COREWEBVIEW2_MAX_INSTANCES",StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("WebView2 environment override present");
            }
        }
        public static bool StopCommand(string text, string nonce) { return text == "STOP " + nonce; }
    }
    // STA-owned bookkeeping, separated so callback ordering can be tested
    // without constructing a host, loading Core, or running a message pump.
    public sealed class HostLifecycle {
        public bool Stopping { get; private set; }
        public bool Failed { get; private set; }
        public int ExitCode { get; private set; }
        public string FailureReason { get; private set; }
        public int FailureHResult { get; private set; }
        public HostLifecycle() { ExitCode = 1; }
        public void Fail(string reason, int hresult) {
            if (!Failed) { FailureReason = reason; FailureHResult = hresult; }
            Failed = true;
            ExitCode = 1;
        }
        public bool BeginStop(string reason, int hresult, bool success, long elapsed) {
            if (!success) Fail(reason,hresult);
            if (success && elapsed >= Evidence.StartupMilliseconds) Fail("deadline",unchecked((int)0x800705b4));
            if (Stopping) return false;
            Stopping = true;
            ExitCode = success && !Failed ? 0 : 1;
            return true;
        }
    }
    // Pure, fixed protocol-input contract shared by the host and offline tests.
    public static class HostProbeContract {
        public const int MaximumRecordBytes = 8 * 1024;
        public const int MaximumAccessibilityResponseBytes = 64 * 1024;
        public const int MaximumAggregateBytes = 16 * 1024;
        public const int MaximumParameterBytes = 512;
        public const int MaximumCumulativeParameterBytes = 2048;
        public const int MaximumCumulativeResponseBytes = MaximumAccessibilityResponseBytes + 20;
        public const int CommandCount = 11;
        public const string FocusParameters = "{\"enabled\":true}";
        public const string ReadinessDownParameters = "{\"type\":\"keyDown\",\"key\":\"F24\",\"code\":\"F24\",\"windowsVirtualKeyCode\":135}";
        public const string ReadinessUpParameters = "{\"type\":\"keyUp\",\"key\":\"F24\",\"code\":\"F24\",\"windowsVirtualKeyCode\":135}";
        public const string EnterDownParameters = "{\"type\":\"keyDown\",\"key\":\"Enter\",\"code\":\"Enter\",\"text\":\"\\r\",\"unmodifiedText\":\"\\r\",\"windowsVirtualKeyCode\":13}";
        public const string EnterUpParameters = "{\"type\":\"keyUp\",\"key\":\"Enter\",\"code\":\"Enter\",\"windowsVirtualKeyCode\":13}";
        public const string InsertParameters = "{\"text\":\"Ada\"}";
        public const string AxParameters = "{}";

        static readonly string[] Methods = {
            "Emulation.setFocusEmulationEnabled",
            "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Input.insertText", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Accessibility.getFullAXTree"
        };
        static readonly string[] Parameters = {
            FocusParameters, ReadinessDownParameters, ReadinessUpParameters,
            EnterDownParameters, EnterUpParameters, InsertParameters,
            ReadinessDownParameters, ReadinessUpParameters,
            EnterDownParameters, EnterUpParameters, AxParameters
        };

        public static void RequireActive(HostLifecycle lifecycle, long elapsed) {
            if (lifecycle == null || lifecycle.Stopping || lifecycle.Failed) throw new InvalidOperationException("DOM probe is no longer active");
            if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("DOM probe deadline");
        }
        public sealed class CommandSequence {
            int next, parameterBytes, responseBytes;
            bool inFlight;
            string pendingMethod, pendingParameters;
            public int Completed { get { return next; } }
            public void BeginRequest(string method, string parameters, long elapsed) {
                if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Protocol command deadline");
                if (inFlight) throw new InvalidOperationException("Protocol command already in flight");
                if (next >= CommandCount || method != Methods[next] || parameters != Parameters[next]) throw new InvalidOperationException("Protocol command sequence differs");
                int bytes = Encoding.UTF8.GetByteCount(parameters ?? "");
                if (bytes == 0 || bytes > MaximumParameterBytes || parameterBytes > MaximumCumulativeParameterBytes - bytes) throw new InvalidDataException("Protocol parameter byte bound");
                pendingMethod = method; pendingParameters = parameters; inFlight = true;
            }
            public void Complete(string method, string parameters, string response, long elapsed) {
                if (!inFlight || method != pendingMethod || parameters != pendingParameters) throw new InvalidOperationException("Protocol completion differs from the in-flight command");
                if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Protocol command deadline");
                int parametersLength = Encoding.UTF8.GetByteCount(parameters);
                int responseLength = response == null ? 0 : Encoding.UTF8.GetByteCount(response);
                bool accessibility = method == "Accessibility.getFullAXTree";
                if ((!accessibility && response != "{}") || (accessibility && (responseLength == 0 || responseLength > MaximumAccessibilityResponseBytes)) ||
                    responseBytes > MaximumCumulativeResponseBytes - responseLength) throw new InvalidDataException("Protocol response contract differs");
                parameterBytes += parametersLength; responseBytes += responseLength; next++;
                inFlight = false; pendingMethod = null; pendingParameters = null;
            }
            public void RequireComplete() {
                if (inFlight || next != CommandCount) throw new InvalidOperationException("Protocol command sequence incomplete");
            }
        }
        public sealed class EvidenceBudget {
            int total;
            public int TotalBytes { get { return total; } }
            public byte[] Add(string raw) {
                byte[] bytes = raw == null ? new byte[0] : Encoding.UTF8.GetBytes(raw);
                if (bytes.Length == 0 || bytes.Length > MaximumRecordBytes || total > MaximumAggregateBytes - bytes.Length) throw new InvalidDataException("DOM evidence aggregate bound exceeded");
                total += bytes.Length;
                return bytes;
            }
        }
    }
    // Converts the verbose, bounded CDP tree to the exact semantic evidence the
    // parent validates. Raw bytes remain identity-bound without consuming the
    // host event budget or weakening ordinary DOM-document ceilings.
    public static class HostAccessibilityProjection {
        static readonly JavaScriptSerializer Json = new JavaScriptSerializer();
        static Dictionary<string,object> Object(string raw) {
            var value=Json.DeserializeObject(raw) as Dictionary<string,object>;
            if(value==null) throw new InvalidDataException("Expected accessibility JSON object");
            return value;
        }
        static string AxValue(Dictionary<string,object> node, string field, params string[] allowedTypes) {
            object property, type, value;
            var item=node.TryGetValue(field,out property) ? property as Dictionary<string,object> : null;
            if(item==null || !item.TryGetValue("type",out type) || !(type is string) || !item.TryGetValue("value",out value) || !(value is string)) return "";
            bool allowed=false; foreach(string candidate in allowedTypes) if((string)type==candidate) allowed=true;
            return allowed ? (string)value : "";
        }
        static bool AxRequired(Dictionary<string,object> node) {
            object propertiesValue;
            var properties=node.TryGetValue("properties",out propertiesValue) ? propertiesValue as object[] : null;
            if(properties==null) return false;
            var names=new HashSet<string>(StringComparer.Ordinal); bool found=false, required=false;
            foreach(object propertyValue in properties) {
                var property=propertyValue as Dictionary<string,object>; object name, wrapped, type, value;
                if(property==null || !property.TryGetValue("name",out name) || !(name is string) || !names.Add((string)name)) throw new InvalidDataException("Accessibility property identity differs");
                if((string)name=="required" && property.TryGetValue("value",out wrapped)) {
                    var item=wrapped as Dictionary<string,object>;
                    if(item==null || !item.TryGetValue("type",out type) || !(type is string) || (string)type!="booleanOrUndefined" || !item.TryGetValue("value",out value) || !(value is bool)) throw new InvalidDataException("Accessibility required state differs");
                    found=true; required=(bool)value;
                }
            }
            return found && required;
        }
        static string Hash(byte[] bytes) {
            using(var algorithm=SHA256.Create()) {
                var text=new StringBuilder(64); foreach(byte value in algorithm.ComputeHash(bytes)) text.Append(value.ToString("x2",CultureInfo.InvariantCulture)); return text.ToString();
            }
        }
        public static string Create(string raw) {
            byte[] rawBytes=Encoding.UTF8.GetBytes(raw ?? "");
            if(rawBytes.Length==0 || rawBytes.Length>HostProbeContract.MaximumAccessibilityResponseBytes) throw new InvalidDataException("Accessibility response byte bound differs");
            var root=Object(raw); object nodesValue;
            var nodes=root.TryGetValue("nodes",out nodesValue) ? nodesValue as object[] : null;
            if(root.Count!=1 || nodes==null || nodes.Length==0 || nodes.Length>128) throw new InvalidDataException("Accessibility node bound or shape differs");
            var nodeIds=new HashSet<string>(StringComparer.Ordinal);
            var selected=new Dictionary<string,string[]>(StringComparer.Ordinal);
            foreach(object nodeValue in nodes) {
                var node=nodeValue as Dictionary<string,object>; if(node==null) throw new InvalidDataException("Accessibility node shape differs");
                object nodeIdValue, ignored;
                string nodeId=node.TryGetValue("nodeId",out nodeIdValue) ? nodeIdValue as string : null;
                if(String.IsNullOrEmpty(nodeId) || nodeId.Length>128 || !nodeIds.Add(nodeId)) throw new InvalidDataException("Accessibility node identity differs");
                if(!node.TryGetValue("ignored",out ignored) || !(ignored is bool)) throw new InvalidDataException("Accessibility ignored state differs");
                if((bool)ignored) continue;
                string role=AxValue(node,"role","role","internalRole"), name=AxValue(node,"name","computedString"), key=null, value=""; bool required=false;
                if(role=="RootWebArea" && name=="CS-3 form fixture") key="document";
                else if(role=="textbox" && name=="Name") { key="textbox"; value=AxValue(node,"value","string"); required=AxRequired(node); if(value!="Ada" || !required) throw new InvalidDataException("Accessible textbox value or required state differs"); }
                else if(role=="button" && name=="Save") key="button";
                else if(role=="alert") key="alert";
                else if(role=="status") key="status";
                if(key!=null) {
                    if(name.Length>256) throw new InvalidDataException("Accessibility name exceeds the evidence ceiling");
                    if(selected.ContainsKey(key)) throw new InvalidDataException("Accessibility projection role is ambiguous");
                    selected.Add(key,new[]{nodeId,role,name,value,required ? "true" : "false"});
                }
            }
            string[] order={"document","textbox","button","alert","status"};
            if(selected.Count!=order.Length) throw new InvalidDataException("Required accessibility roles, names or state absent");
            var output=new StringBuilder();
            output.Append("{\"schema\":\"cs3-accessibility-projection/1\",\"source_method\":\"Accessibility.getFullAXTree\",\"raw_utf8_bytes\":")
                .Append(rawBytes.Length.ToString(CultureInfo.InvariantCulture)).Append(",\"raw_sha256\":").Append(Evidence.Quote(Hash(rawBytes)))
                .Append(",\"raw_node_count\":").Append(nodes.Length.ToString(CultureInfo.InvariantCulture)).Append(",\"nodes\":[");
            for(int index=0;index<order.Length;index++) {
                if(index>0) output.Append(','); string[] item=selected[order[index]];
                output.Append("{\"node_id\":").Append(Evidence.Quote(item[0])).Append(",\"role\":").Append(Evidence.Quote(item[1]))
                    .Append(",\"name\":").Append(Evidence.Quote(item[2])).Append(",\"value\":").Append(Evidence.Quote(item[3]))
                    .Append(",\"required\":").Append(item[4]).Append('}');
            }
            string projection=output.Append("]}").ToString();
            if(Encoding.UTF8.GetByteCount(projection)>HostProbeContract.MaximumRecordBytes) throw new InvalidDataException("Accessibility projection exceeds the DOM evidence ceiling");
            return projection;
        }
    }
    // Separate diagnostic sequence: observations cannot satisfy the DOM oracle.
    public sealed class InputRoutingCommands {
        public const string TargetParameters = "{\"expression\":\"(() => ({url:location.href,title:document.title,scriptReady:window.__cs3InputEvidence && window.__cs3InputEvidence.scriptReady === true}))()\",\"returnByValue\":true}";
        static readonly string[] MethodsWithInsert = { "Runtime.evaluate", "Emulation.setFocusEmulationEnabled", "Input.insertText", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent" };
        static readonly string[] ParametersWithInsert = { TargetParameters, HostProbeContract.FocusParameters, HostProbeContract.InsertParameters, HostProbeContract.EnterDownParameters, HostProbeContract.EnterUpParameters };
        static readonly string[] MethodsWithoutInsert = { "Runtime.evaluate", "Emulation.setFocusEmulationEnabled", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent" };
        static readonly string[] ParametersWithoutInsert = { TargetParameters, HostProbeContract.FocusParameters, HostProbeContract.EnterDownParameters, HostProbeContract.EnterUpParameters };
        readonly string[] methods, parameters;
        int next, responseBytes;
        bool inFlight;
        string pendingMethod, pendingParameters;
        public InputRoutingCommands() : this(true) { }
        public InputRoutingCommands(bool insertBeforeKey) {
            methods=insertBeforeKey ? MethodsWithInsert : MethodsWithoutInsert;
            parameters=insertBeforeKey ? ParametersWithInsert : ParametersWithoutInsert;
        }
        public void BeginRequest(string method, string parameters, long elapsed) {
            if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Input diagnostic deadline");
            if (inFlight) throw new InvalidOperationException("Input diagnostic command already in flight");
            if (next >= methods.Length || method != methods[next] || parameters != this.parameters[next]) throw new InvalidOperationException("Input diagnostic command sequence differs");
            pendingMethod=method; pendingParameters=parameters; inFlight=true;
        }
        public void Complete(string method, string parameters, string response, long elapsed) {
            if(!inFlight || method!=pendingMethod || parameters!=pendingParameters) throw new InvalidOperationException("Input diagnostic completion differs from the in-flight command");
            if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Input diagnostic deadline");
            int length=response==null?0:Encoding.UTF8.GetByteCount(response);
            if (length==0 || length>HostProbeContract.MaximumRecordBytes || responseBytes>HostProbeContract.MaximumAggregateBytes-length || (next>0 && response!="{}")) throw new InvalidDataException("Input diagnostic response bound or shape differs");
            responseBytes+=length; next++; inFlight=false; pendingMethod=null; pendingParameters=null;
        }
        public void RequireComplete() { if(inFlight || next!=methods.Length) throw new InvalidOperationException("Incomplete input diagnostic commands"); }
    }
    public static class Evidence {
        public const int MaximumEvents = 384, MaximumProcesses = 32, StartupMilliseconds = 20000;
        public static string Quote(string value) {
            if (value == null || value.Length > 256) throw new ArgumentException("Evidence text exceeds bound");
            var b = new StringBuilder("\"");
            foreach (char c in value) {
                if (c == '"' || c == '\\') b.Append('\\').Append(c);
                else if (c < 32 || c > 126) b.Append("\\u").Append(((int)c).ToString("x4",CultureInfo.InvariantCulture));
                else b.Append(c);
            }
            return b.Append('"').ToString();
        }
        public static string Record(string nonce, string phase, long elapsed, int hresult, string version, uint pid, string kind) {
            if (!Regex.IsMatch(nonce,@"\A[a-f0-9]{64}\z") || !Regex.IsMatch(phase,@"\A[a-z_]{1,64}\z") || elapsed < 0 || elapsed > 120000) throw new ArgumentException("Invalid evidence fields");
            string json = "{\"schema\":\"cs3-webview2-host/1\",\"nonce\":"+Quote(nonce)+",\"phase\":"+Quote(phase)+",\"elapsed_ms\":"+elapsed.ToString(CultureInfo.InvariantCulture)+",\"hresult\":"+Quote("0x"+unchecked((uint)hresult).ToString("x8",CultureInfo.InvariantCulture))+",\"version\":"+Quote(version)+",\"pid\":"+pid.ToString(CultureInfo.InvariantCulture)+",\"kind\":"+Quote(kind)+",\"containment_attested\":false,\"browser_qualified\":false}";
            if (Encoding.UTF8.GetByteCount(json)>2048) throw new ArgumentException("Evidence line exceeds bound");
            return json;
        }
    }
}
