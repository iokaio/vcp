// SPDX-License-Identifier: Apache-2.0
// Real native replay of a 130-version governed history, with no GUI or provider.
import assert from 'node:assert/strict';
import {spawn,execFileSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {performance} from 'node:perf_hooks';
import {launchLocal} from '../dist/index.js';
let raw='';for await(const chunk of process.stdin)raw+=chunk;
const input=JSON.parse(raw);assert.equal(input.versions.length,130);
const initialize={protocol_version:'1.0',client:{name:'native-startup',version:'1'},capabilities:['task/read'],required_capabilities:['task/read']};
const bootstrap={schema:'vcp-local-bootstrap/1',workspace:input.workspace,data:input.data,role:'observer',transport:'stdio',observer_reconnect:true};
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const alive=pid=>{try{process.kill(pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;throw error;}};
const ownerPID=()=>{try{return JSON.parse(fs.readFileSync(path.join(input.canonical_root,'owner.lock'),'utf8')).pid;}catch{return undefined;}};
const ownedServer=parent=>{
  assert(Number.isInteger(parent)&&parent>0);
  const script='$rows=@(Get-CimInstance Win32_Process -Filter ("ParentProcessId = " + $env:VCP_STARTUP_PARENT) | Select-Object ProcessId,ExecutablePath); ConvertTo-Json -InputObject $rows -Compress';
  const rows=JSON.parse(execFileSync('powershell.exe',['-NoProfile','-Command',script],{windowsHide:true,timeout:5000,encoding:'utf8',env:{...process.env,VCP_STARTUP_PARENT:String(parent)}}));
  const normalize=value=>path.resolve(value).replace(/^\\\\\?\\/,'').toLowerCase();
  return rows.find(row=>row.ExecutablePath&&normalize(row.ExecutablePath)===normalize(input.executable))?.ProcessId;
};
async function probe(cancel){
  const priorOwner=ownerPID(),began=performance.now(),result={};
  const child=spawn(input.executable,['local-bridge'],{windowsHide:true,stdio:['pipe','pipe','pipe']});
  const closed=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
  let buffered='',ready=false,initStart,parseError,server;
  child.stderr.on('data',()=>{});child.stdin.on('error',()=>{});
  const guard=setTimeout(()=>child.kill(),75000);
  child.stdout.on('data',bytes=>{
    try{
      buffered+=bytes.toString();assert(buffered.length<=2*1024*1024);
      let newline;
      while((newline=buffered.indexOf('\n'))>=0){
        const row=JSON.parse(buffered.slice(0,newline));buffered=buffered.slice(newline+1);
        if(!ready){assert.equal(cancel,undefined,'cancel while actual store replay is pending');ready=true;result.ready_ms=Math.round(performance.now()-began);initStart=performance.now();child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:initialize})+'\n');}
        else {result.initialize_ms=Math.round(performance.now()-initStart);assert(row.result);child.stdin.end();}
      }
    }catch(error){parseError=error;child.stdin.end();}
  });
  child.stdin.write(JSON.stringify(bootstrap)+'\n');
  try{
    if(cancel){
      // Resolve only the new bridge's child with the exact native executable;
      // Windows byte-range locking can hide owner.lock during replay. No process
      // is killed by this PID: it is used only for independent liveness checks.
      await delay(250);server=ownedServer(child.pid);
      assert(Number.isInteger(server)&&server>0&&server!==priorOwner&&server!==process.pid,'new bridge owns this native server');assert(alive(server));assert.equal(ready,false);
      const cancelled=performance.now();
      if(cancel==='eof')child.stdin.end();else child.stdin.write('{}\n');
      const exit=await closed;result.cancel_ms=Math.round(performance.now()-cancelled);
      assert(result.cancel_ms<5000,'native startup cancellation precedes SDK forced bridge termination');
      assert.equal(exit.signal,null);assert.notEqual(exit.code,0);assert.equal(ready,false);
      assert.equal(alive(server),false,'owned native server terminated and released its writer');result.owned_server_stopped=true;
    }else{
      const exit=await closed;assert.equal(exit.code,0);assert.equal(exit.signal,null);assert(ready);assert(result.ready_ms<60000);assert(result.initialize_ms<10000);
    }
    if(parseError)throw parseError;
    return result;
  }finally{clearTimeout(guard);if(child.exitCode===null&&child.signalCode===null){child.stdin.end();await Promise.race([closed,delay(5000)]);if(child.exitCode===null&&child.signalCode===null)child.kill();await closed;}}
}
const result={ok:true,backend:input.backend,versions:input.versions.length};
result.eof=await probe('eof');result.early_frame=await probe('early-frame');result.native=await probe();
const began=performance.now();let client;
try{client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,role:'observer',transport:'stdio',initialize});result.sdk_ready_initialize_ms=Math.round(performance.now()-began);assert(result.sdk_ready_initialize_ms<75000);const task=await client.call('task/read',{scope:input.scope,task:input.task});assert.equal(task.value.task,input.task);result.sdk_ok=true;}finally{await client?.dispose();}
process.stdout.write(JSON.stringify(result));
