// SPDX-License-Identifier: Apache-2.0
using System;
using System.Collections.Generic;
using System.Collections.Concurrent;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Runtime.ExceptionServices;
namespace Vcp.Cs3Draft {
public sealed class HostObservation {
    public string Phase, Version, Kind; public uint Pid; public long Elapsed;
}
public static class ProbeContract {
    public const string Version = "154.0.4258.37";
    public const string Argument = "--edge-webview-no-dpi-workaround";
    // The caller supplies only queried process metadata here. No file access,
    // hash read or approval occurs for an image outside the exact inventory.
    public static string RequireImage(string queriedPath, string normalizedPath, uint pid, long creation,
        Dictionary<string,string> images, Action<object> diagnostic) {
        string expected;
        if(images.TryGetValue(normalizedPath,out expected)) return expected;
        // QueryFullProcessImageName is bounded to 4096 characters. Chunk longer
        // paths so JSON escaping cannot exceed the controller's line ceiling.
        try { diagnostic(new { type="rejected_process_image",pid,creation_filetime=creation,
            queried_image_path=queriedPath.Length<=1024?queriedPath:null,
            path_characters=queriedPath.Length,path_in_chunks=queriedPath.Length>1024,
            image_approved=false,image_bytes_read=false }); } catch { }
        if(queriedPath.Length>1024 && queriedPath.Length<=4096) {
            for(int offset=0;offset<queriedPath.Length;offset+=1024) {
                try { diagnostic(new { type="rejected_process_image_path_chunk",pid,creation_filetime=creation,
                    offset,path_chunk=queriedPath.Substring(offset,Math.Min(1024,queriedPath.Length-offset)) }); } catch { }
            }
        }
        throw new IOException("Process image outside exact host/runtime inventory");
    }
    // Diagnostic-only failure drainage: never parse, attest, accept readiness,
    // or alter the primary exception. The producer queue is bounded to 32.
    public static void FailedHostDiagnostics(BlockingCollection<string> lines, Action<object> diagnostic) {
        int drained=0,retained=0; string line;
        while(drained<32 && lines.TryTake(out line)) {
            drained++;
            if(retained>=8) continue;
            retained++;
            try { diagnostic(new { type="host_diagnostic_untrusted",accepted_observation=false,
                line=line.Length<=1024?line:line.Substring(0,1024),truncated=line.Length>1024 }); } catch { }
        }
        try { diagnostic(new { type="host_diagnostic_drain",untrusted=true,drained_lines=drained,
            retained_lines=retained,omitted_lines=drained-retained,remaining_queued_lines=lines.Count }); } catch { }
    }
    public static void Coverage(uint cumulative, int verified) {
        if (cumulative > 128 || cumulative != verified) throw new IOException("Job census incomplete: cumulative process count differs from verified identities");
    }
    // A browser can create a child between the PID snapshot and accounting
    // query. Exact coverage is authoritative only after job termination; the
    // live check must not manufacture a failure from that non-atomic pair.
    public static void LiveCoverage(uint cumulative, int verified) {
        if (verified < 0 || cumulative > 128 || cumulative < verified) throw new IOException("Invalid live job process accounting");
    }
    public static HostObservation Parse(string line, string nonce) {
        if (line == null || Encoding.UTF8.GetByteCount(line)>2048) throw new IOException("Host line byte bound");
        using (var doc = JsonDocument.Parse(line, new JsonDocumentOptions { MaxDepth=4 })) {
            var root=doc.RootElement;
            var expected=new HashSet<string>(new[]{"schema","nonce","phase","elapsed_ms","hresult","version","pid","kind","containment_attested","browser_qualified"});
            foreach(var property in root.EnumerateObject()) if(!expected.Remove(property.Name)) throw new IOException("Unexpected or duplicate host field");
            if(expected.Count!=0 || root.GetProperty("schema").GetString()!="cs3-webview2-host/1" || root.GetProperty("nonce").GetString()!=nonce || !Regex.IsMatch(nonce,"\\A[a-f0-9]{64}\\z") || root.GetProperty("containment_attested").GetBoolean() || root.GetProperty("browser_qualified").GetBoolean()) throw new IOException("Host identity or honesty differs");
            var result=new HostObservation { Phase=root.GetProperty("phase").GetString(), Version=root.GetProperty("version").GetString(), Pid=root.GetProperty("pid").GetUInt32(),Elapsed=root.GetProperty("elapsed_ms").GetInt64() };
            if(result.Elapsed<0 || result.Elapsed>20000 || root.GetProperty("hresult").GetString()!="0x00000000" || (result.Version!="" && result.Version!=Version)) throw new IOException("Host failed or expired");
            string kind=root.GetProperty("kind").GetString();
            result.Kind = kind;
            bool chunk=result.Phase=="dom_chunk" || result.Phase=="input_chunk";
            if(kind==null || kind.Length>(chunk?256:64) || !new[]{"host_started","environment_create","environment_created","process_snapshot","reported_process","controller_create","controller_created","controller_ready","stop_received","controller_closed","dom_chunk","dom_complete","input_chunk","input_complete"}.Contains(result.Phase)) throw new IOException("Unaccepted host phase");
            if ((chunk || result.Phase=="dom_complete" || result.Phase=="input_complete") && result.Pid!=0) throw new IOException("Unexpected evidence PID");
            if ((result.Phase=="reported_process" || result.Phase=="controller_ready") && result.Pid==0) throw new IOException("Missing reported PID");
            if(result.Phase=="controller_ready" && ((kind!="browser" && kind!="input_diagnostic") || result.Version!=Version)) throw new IOException("Unbound readiness");
            return result;
        }
    }
    // A rejected host line is untrusted diagnostics only. Failure to publish
    // that diagnostic must never replace the original parser/contract failure.
    public static HostObservation ParseRetainingRejected(string line, string nonce, Action<object> diagnostic) {
        try { return Parse(line,nonce); }
        catch(Exception primary) {
            string retained=Utf8Prefix(line,1024);
            int bytes=line==null?0:Encoding.UTF8.GetByteCount(line);
            try { diagnostic(new { type="host_observation_rejected",untrusted=true,accepted_observation=false,
                line=retained,line_utf8_bytes=bytes,retained_utf8_bytes=Encoding.UTF8.GetByteCount(retained),
                truncated=line!=null && retained.Length!=line.Length }); } catch { }
            ExceptionDispatchInfo.Capture(primary).Throw();
            throw;
        }
    }
    static string Utf8Prefix(string value, int byteLimit) {
        if(value==null) return null;
        if(Encoding.UTF8.GetByteCount(value)<=byteLimit) return value;
        int low=0,high=value.Length;
        while(low<high) {
            int middle=low+(high-low+1)/2;
            if(middle<value.Length && middle>0 && Char.IsHighSurrogate(value[middle-1]) && Char.IsLowSurrogate(value[middle])) middle--;
            if(Encoding.UTF8.GetByteCount(value.Substring(0,middle))<=byteLimit) low=Math.Max(low+1,middle);
            else high=middle-1;
        }
        while(low>0 && Encoding.UTF8.GetByteCount(value.Substring(0,low))>byteLimit) low--;
        if(low<value.Length && low>0 && Char.IsHighSurrogate(value[low-1]) && Char.IsLowSurrogate(value[low])) low--;
        return value.Substring(0,low);
    }
    // No native calls: runs under compile-only.
    public static int Test() {
        int checks=0; Action<bool> check = ok => { if(!ok) throw new Exception("Pure probe assertion failed"); checks++; };
        Action<Action> reject = action => { bool failed=false; try{action();}catch{failed=true;}check(failed); };
        Coverage(2,2); checks++; reject(()=>Coverage(2,1)); reject(()=>Coverage(129,129));
        LiveCoverage(2,1); checks++; LiveCoverage(2,2); checks++;
        reject(()=>LiveCoverage(1,2)); reject(()=>LiveCoverage(129,1)); reject(()=>LiveCoverage(1,-1));
        string nonce=new string('a',64);
        string line="{\"schema\":\"cs3-webview2-host/1\",\"nonce\":\""+nonce+"\",\"phase\":\"controller_ready\",\"elapsed_ms\":100,\"hresult\":\"0x00000000\",\"version\":\"154.0.4258.37\",\"pid\":123,\"kind\":\"browser\",\"containment_attested\":false,\"browser_qualified\":false}";
        check(Parse(line,nonce).Pid==123); reject(()=>Parse(line,new string('b',64))); reject(()=>Parse(line.Replace("100,","20001,"),nonce)); reject(()=>Parse(line.Replace("123,","0,"),nonce)); reject(()=>Parse(line.Replace("0x00000000","0x80070005"),nonce)); reject(()=>Parse(line.Replace("\"containment_attested\":false","\"containment_attested\":true"),nonce)); reject(()=>Parse(line.Replace("\"pid\":123","\"pid\":123,\"pid\":456"),nonce)); reject(()=>Parse(line.Replace("controller_ready","navigation_rejected"),nonce));
        check(Parse(line.Replace("\"kind\":\"browser\"","\"kind\":\"input_diagnostic\""),nonce).Kind=="input_diagnostic");
        reject(()=>Parse(line.Replace("\"kind\":\"browser\"","\"kind\":\"routing_only\""),nonce));
        string inputChunk=line.Replace("\"phase\":\"controller_ready\"","\"phase\":\"input_chunk\"").Replace("\"pid\":123","\"pid\":0").Replace("\"kind\":\"browser\"","\"kind\":\"target:0:1:"+new string('a',64)+":eA==\"");
        check(Parse(inputChunk,nonce).Phase=="input_chunk"); reject(()=>Parse(inputChunk.Replace("\"pid\":0","\"pid\":1"),nonce));
        string inputComplete=inputChunk.Replace("\"phase\":\"input_chunk\"","\"phase\":\"input_complete\"").Replace("\"kind\":\"target:0:1:"+new string('a',64)+":eA==\"","\"kind\":\"routing_only\"");
        check(Parse(inputComplete,nonce).Kind=="routing_only");
        var inventory=new Dictionary<string,string>(StringComparer.OrdinalIgnoreCase) { {@"C:\\approved.exe",new string('f',64)} };
        var diagnostics=new List<object>();
        check(RequireImage(@"C:\\approved.exe",@"C:\\approved.exe",17,123,inventory,diagnostics.Add)==new string('f',64));
        check(diagnostics.Count==0);
        reject(()=>RequireImage(@"C:\\outside.exe",@"C:\\outside.exe",19,456,inventory,diagnostics.Add));
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[0]))) {
            var r=d.RootElement; check(r.GetProperty("queried_image_path").GetString()==@"C:\\outside.exe" && r.GetProperty("pid").GetUInt32()==19 && r.GetProperty("creation_filetime").GetInt64()==456);
            check(!r.GetProperty("image_approved").GetBoolean() && !r.GetProperty("image_bytes_read").GetBoolean());
        }
        check(inventory.Count==1 && !inventory.ContainsKey(@"C:\\outside.exe"));
        diagnostics.Clear(); string longPath=new string('\u2028',4096);
        reject(()=>RequireImage(longPath,longPath,20,457,inventory,diagnostics.Add));
        check(diagnostics.Count==5); string restored="";
        for(int i=1;i<diagnostics.Count;i++) {
            string encoded=JsonSerializer.Serialize(diagnostics[i]); check(Encoding.UTF8.GetByteCount(encoded)<16384);
            using(var d=JsonDocument.Parse(encoded)) { restored+=d.RootElement.GetProperty("path_chunk").GetString(); }
        }
        check(restored==longPath);
        bool original=false; try { RequireImage(@"C:\\outside.exe",@"C:\\outside.exe",19,456,inventory,_=>{throw new Exception("transport");}); }
        catch(IOException error) { original=error.Message=="Process image outside exact host/runtime inventory"; } check(original);
        var queue=new BlockingCollection<string>(32); queue.Add(line); queue.Add("malformed\\nraw-host-text"); queue.Add(new string('x',2048));
        for(int i=3;i<32;i++) queue.Add("queued"); queue.CompleteAdding(); diagnostics.Clear();
        FailedHostDiagnostics(queue,diagnostics.Add); check(queue.Count==0 && diagnostics.Count==9);
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[0]))) { var r=d.RootElement; check(r.GetProperty("type").GetString()=="host_diagnostic_untrusted" && !r.GetProperty("accepted_observation").GetBoolean() && r.GetProperty("line").GetString()==line); }
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[2]))) { var r=d.RootElement; check(r.GetProperty("truncated").GetBoolean() && r.GetProperty("line").GetString().Length==1024); }
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[8]))) { var r=d.RootElement; check(r.GetProperty("drained_lines").GetInt32()==32 && r.GetProperty("retained_lines").GetInt32()==8 && r.GetProperty("omitted_lines").GetInt32()==24); }
        queue=new BlockingCollection<string>(32); queue.Add("raw"); queue.CompleteAdding(); FailedHostDiagnostics(queue,_=>{throw new Exception("transport");}); check(queue.Count==0);
        diagnostics.Clear(); string malformed="{malformed"; bool malformedPrimary=false;
        try { ParseRetainingRejected(malformed,nonce,diagnostics.Add); } catch(JsonException) { malformedPrimary=true; }
        check(malformedPrimary && diagnostics.Count==1);
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[0]))) { var r=d.RootElement; check(r.GetProperty("type").GetString()=="host_observation_rejected" && r.GetProperty("untrusted").GetBoolean() && !r.GetProperty("accepted_observation").GetBoolean() && r.GetProperty("line").GetString()==malformed); }
        diagnostics.Clear(); string failed=line.Replace("0x00000000","0x80070005"); bool failedPrimary=false;
        try { ParseRetainingRejected(failed,nonce,diagnostics.Add); } catch(IOException error) { failedPrimary=error.Message=="Host failed or expired"; }
        check(failedPrimary && diagnostics.Count==1);
        bool callbackPrimary=false;
        try { ParseRetainingRejected(failed,nonce,_=>{throw new Exception("diagnostic transport");}); } catch(IOException error) { callbackPrimary=error.Message=="Host failed or expired"; }
        check(callbackPrimary);
        diagnostics.Clear(); string multibyte=new string('\u2028',900);
        try { ParseRetainingRejected(multibyte,nonce,diagnostics.Add); } catch(IOException) { }
        using(var d=JsonDocument.Parse(JsonSerializer.Serialize(diagnostics[0]))) { var r=d.RootElement; check(r.GetProperty("truncated").GetBoolean() && r.GetProperty("retained_utf8_bytes").GetInt32()<=1024 && r.GetProperty("line_utf8_bytes").GetInt32()==2700); }
        reject(()=>Coverage(2,1));
        return checks;
    }
}
}
