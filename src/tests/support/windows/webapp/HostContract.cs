// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;

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
        public const int MaximumAggregateBytes = 16 * 1024;
        public const int MaximumParameterBytes = 512;
        public const int MaximumCumulativeParameterBytes = 2048;
        public const int MaximumCumulativeResponseBytes = MaximumRecordBytes + 12;
        public const int CommandCount = 7;
        public const string FocusParameters = "{\"enabled\":true}";
        public const string EnterDownParameters = "{\"type\":\"keyDown\",\"key\":\"Enter\",\"code\":\"Enter\",\"text\":\"\\r\",\"unmodifiedText\":\"\\r\",\"windowsVirtualKeyCode\":13}";
        public const string EnterUpParameters = "{\"type\":\"keyUp\",\"key\":\"Enter\",\"code\":\"Enter\",\"windowsVirtualKeyCode\":13}";
        public const string InsertParameters = "{\"text\":\"Ada\"}";
        public const string AxParameters = "{}";

        static readonly string[] Methods = {
            "Emulation.setFocusEmulationEnabled",
            "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Input.insertText", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent",
            "Accessibility.getFullAXTree"
        };
        static readonly string[] Parameters = {
            FocusParameters, EnterDownParameters, EnterUpParameters,
            InsertParameters, EnterDownParameters, EnterUpParameters, AxParameters
        };

        public static void RequireActive(HostLifecycle lifecycle, long elapsed) {
            if (lifecycle == null || lifecycle.Stopping || lifecycle.Failed) throw new InvalidOperationException("DOM probe is no longer active");
            if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("DOM probe deadline");
        }
        public sealed class CommandSequence {
            int next, parameterBytes, responseBytes;
            public int Completed { get { return next; } }
            public void RequireRequest(string method, string parameters, long elapsed) {
                if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Protocol command deadline");
                if (next >= CommandCount || method != Methods[next] || parameters != Parameters[next]) throw new InvalidOperationException("Protocol command sequence differs");
                int bytes = Encoding.UTF8.GetByteCount(parameters ?? "");
                if (bytes == 0 || bytes > MaximumParameterBytes || parameterBytes > MaximumCumulativeParameterBytes - bytes) throw new InvalidDataException("Protocol parameter byte bound");
            }
            public void Complete(string method, string parameters, string response, long elapsed) {
                RequireRequest(method,parameters,elapsed);
                int parametersLength = Encoding.UTF8.GetByteCount(parameters);
                int responseLength = response == null ? 0 : Encoding.UTF8.GetByteCount(response);
                bool accessibility = method == "Accessibility.getFullAXTree";
                if ((!accessibility && response != "{}") || (accessibility && (responseLength == 0 || responseLength > MaximumRecordBytes)) ||
                    responseBytes > MaximumCumulativeResponseBytes - responseLength) throw new InvalidDataException("Protocol response contract differs");
                parameterBytes += parametersLength; responseBytes += responseLength; next++;
            }
            public void RequireComplete() {
                if (next != CommandCount) throw new InvalidOperationException("Protocol command sequence incomplete");
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
    // Separate diagnostic sequence: observations cannot satisfy the DOM oracle.
    public sealed class InputRoutingCommands {
        public const string TargetParameters = "{\"expression\":\"(() => ({url:location.href,title:document.title,scriptReady:window.__cs3InputEvidence && window.__cs3InputEvidence.scriptReady === true}))()\",\"returnByValue\":true}";
        static readonly string[] MethodsWithInsert = { "Runtime.evaluate", "Emulation.setFocusEmulationEnabled", "Input.insertText", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent" };
        static readonly string[] ParametersWithInsert = { TargetParameters, HostProbeContract.FocusParameters, HostProbeContract.InsertParameters, HostProbeContract.EnterDownParameters, HostProbeContract.EnterUpParameters };
        static readonly string[] MethodsWithoutInsert = { "Runtime.evaluate", "Emulation.setFocusEmulationEnabled", "Input.dispatchKeyEvent", "Input.dispatchKeyEvent" };
        static readonly string[] ParametersWithoutInsert = { TargetParameters, HostProbeContract.FocusParameters, HostProbeContract.EnterDownParameters, HostProbeContract.EnterUpParameters };
        readonly string[] methods, parameters;
        int next, responseBytes;
        public InputRoutingCommands() : this(true) { }
        public InputRoutingCommands(bool insertBeforeKey) {
            methods=insertBeforeKey ? MethodsWithInsert : MethodsWithoutInsert;
            parameters=insertBeforeKey ? ParametersWithInsert : ParametersWithoutInsert;
        }
        public void RequireRequest(string method, string parameters, long elapsed) {
            if (elapsed < 0 || elapsed >= Evidence.StartupMilliseconds) throw new TimeoutException("Input diagnostic deadline");
            if (next >= methods.Length || method != methods[next] || parameters != this.parameters[next]) throw new InvalidOperationException("Input diagnostic command sequence differs");
        }
        public void Complete(string method, string parameters, string response, long elapsed) {
            RequireRequest(method,parameters,elapsed);
            int length=response==null?0:Encoding.UTF8.GetByteCount(response);
            if (length==0 || length>HostProbeContract.MaximumRecordBytes || responseBytes>HostProbeContract.MaximumAggregateBytes-length || (next>0 && response!="{}")) throw new InvalidDataException("Input diagnostic response bound or shape differs");
            responseBytes+=length; next++;
        }
        public void RequireComplete() { if(next!=methods.Length) throw new InvalidOperationException("Incomplete input diagnostic commands"); }
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
