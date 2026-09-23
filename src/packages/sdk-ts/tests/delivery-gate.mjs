// SPDX-License-Identifier: Apache-2.0
// A bounded test-only delivery gate. Every response comes from the actual native
// server. This models client delivery delay, not a native crash-before-send.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { LineDecoder } from '../dist/codec.js';
import { Client } from '../dist/client.js';

export async function connectGated(input,initialize) {
  const child=spawn(input.executable,['local-bridge'],{shell:false,windowsHide:true,stdio:['pipe','pipe','pipe']});
  const decoder=new LineDecoder(16*1024);
  const ready=Promise.withResolvers(),exited=Promise.withResolvers();
  let boot=true,dead=false,frame,failed,watched,requestId,heldFrame,latch,closing;
  let diagnostics=0;
  child.stderr.on('data',chunk=>{diagnostics+=chunk.length;if(diagnostics>64*1024)child.kill();});
  child.once('error',()=>{ready.reject(new Error('native gated bridge failed'));failed?.();});
  child.once('exit',()=>{dead=true;exited.resolve();if(boot)ready.reject(new Error('native bootstrap exited'));failed?.();});
  child.stdout.on('data',chunk=>{
    try {
      for(const value of decoder.push(chunk)) {
        if(boot){boot=false;assert.equal(value.schema,'vcp-local-ready/1');assert.deepEqual(value.scope,input.scope);decoder.setMaximum(1024*1024);ready.resolve();continue;}
        if(value.id===requestId){assert.equal(heldFrame,undefined);heldFrame=value;latch.resolve(value);}
        else {assert(frame);frame(value);}
      }
    }catch(error){ready.reject(error);failed?.();child.kill();}
  });
  const transport={
    send(bytes){
      const request=JSON.parse(bytes.toString('utf8'));
      if(watched!==undefined && request.params?.mutation?.command_id===watched)requestId=request.id;
      return new Promise((resolve,reject)=>child.stdin.write(bytes,error=>error?reject(error):resolve()));
    },
    listen(onFrame,onFailure){frame=onFrame;failed=onFailure;return()=>{frame=undefined;failed=undefined;};},
    setMaximum(bytes){decoder.setMaximum(bytes);},
    close(){return closing??=(async()=>{
      child.stdin.end();let timer;
      await Promise.race([exited.promise,new Promise(resolve=>timer=setTimeout(resolve,5000))]);clearTimeout(timer);
      if(!dead)child.kill();
      await Promise.race([exited.promise,new Promise(resolve=>timer=setTimeout(resolve,2000))]);clearTimeout(timer);assert(dead,'owned gated bridge must exit');
    })();}
  };
  child.stdin.write(JSON.stringify({schema:'vcp-local-bootstrap/1',workspace:input.workspace,data:input.data,role:'controller'})+'\n');
  let timer;
  try {
    await Promise.race([ready.promise,new Promise((_,reject)=>timer=setTimeout(()=>reject(new Error('bounded native bootstrap')),10000))]);
    const client=await Client.connect(transport,input.scope,'controller',initialize);
    return {client,hold(command){assert.equal(watched,undefined);watched=command;latch=Promise.withResolvers();return latch.promise;},release(){assert(heldFrame);const value=heldFrame;heldFrame=undefined;watched=undefined;requestId=undefined;frame(value);}};
  }catch(error){await transport.close();throw error;}finally{clearTimeout(timer);}
}
