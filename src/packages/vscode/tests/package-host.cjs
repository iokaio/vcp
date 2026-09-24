// SPDX-License-Identifier: Apache-2.0
// Runs only from an external private qualification directory with an installed VSIX.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process'),vscode=require('vscode');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.run=async()=>{
 const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
 let phase='activate',api,unexpectedPath=[];
 const wait=async(fn,label)=>{for(let i=0;i<600;i++){const result=fn();if(result)return result;await delay(50)}throw Error(`deadline ${label}`)};
 try{
  const normalize=value=>path.resolve(value).toLowerCase();
  const runtime=[input.editorRoot,...fs.readdirSync(input.editorRoot,{withFileTypes:true}).filter(entry=>entry.isDirectory()).map(entry=>path.join(input.editorRoot,entry.name))].filter(root=>{try{return JSON.parse(fs.readFileSync(path.join(root,'resources/app/package.json'),'utf8')).version==='1.138.0'}catch{return false}});
  assert.equal(runtime.length,1);const allowedPath=new Set([process.env.SystemRoot,path.join(process.env.SystemRoot,'System32'),runtime[0]].map(normalize));
  unexpectedPath=process.env.PATH.split(path.delimiter).filter(entry=>!entry||!allowedPath.has(normalize(entry)));const developmentPathAbsent=unexpectedPath.length===0;assert(developmentPathAbsent);
  const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);assert.equal(extension.packageJSON.version,input.version);
  assert(!extension.extensionPath.toLowerCase().startsWith(input.checkout.toLowerCase()));
  api=await extension.activate();
  const restoring=input.mode==='install'&&fs.existsSync(input.reloadMarker);
  if(!restoring){
   const config=vscode.workspace.getConfiguration('vcp');
   if(config.inspect('engineExecutable')?.globalValue!==input.executable)await config.update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
   if(config.inspect('dataDirectory')?.globalValue!==input.data)await config.update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
   phase='connect';await wait(()=>api.getConnectionState().phase!=='connecting','prior discovery settled');
   if(api.getConnectionState().phase!=='connected')await vscode.commands.executeCommand('vcp.connect',vscode.Uri.file(input.workspace).toString());
  }else phase='authenticated observer restoration';
  if(input.mode==='missing'){
   await wait(()=>api.getConnectionState().phase==='unavailable','missing engine');assert(!fs.existsSync(input.executable));
   fs.writeFileSync(input.result,JSON.stringify({ok:true,mode:input.mode,missingEngine:true,version:extension.packageJSON.version}));return;
  }
  const state=await wait(()=>api.getConnectionState().phase==='connected'?api.getConnectionState():undefined,'packaged engine');
  assert.equal(state.role,'observer');assert.deepEqual(state.scope,input.scope);assert.equal(state.rootId,input.scope.workspace);
  const view=await wait(()=>api.getTaskState()?.phase==='current'?api.getTaskState():undefined,'packaged task snapshot');assert(view.rows.some(row=>row.task===input.task&&row.state==='paused'));
  if(input.mode==='install'&&!fs.existsSync(input.reloadMarker)){
   fs.writeFileSync(input.reloadMarker,JSON.stringify({pid:process.pid,scope:state.scope}));
   void vscode.commands.executeCommand('workbench.action.reloadWindow');await new Promise(()=>{});return;
  }
  let reloaded=false;if(input.mode==='install'){const marker=JSON.parse(fs.readFileSync(input.reloadMarker,'utf8'));assert.notEqual(marker.pid,process.pid);assert.deepEqual(marker.scope,state.scope);reloaded=true;}
  // Unsupported negotiation is sent to the actual native authenticated bridge,
  // not a mock engine. It qualifies protocol rejection, not arbitrary downgrades.
  let rejected;
  if(input.mode==='install'){
   // A separate native stdio observer opens only after the prior owner drains.
   await vscode.commands.executeCommand('vcp.disconnect');
   await delay(31000);
   rejected=await new Promise((resolve,reject)=>{
    const child=spawn(input.executable,['local-bridge'],{shell:false,windowsHide:true,stdio:['pipe','pipe','pipe']});let pending='',step=0,value,failure;
    const timer=setTimeout(()=>{failure=Error('native incompatible negotiation deadline');child.kill()},40000);
    const stop=error=>{failure=error;child.kill()};child.stderr.on('data',()=>{});child.on('error',stop);
    child.on('close',()=>{clearTimeout(timer);failure?reject(failure):value?resolve(value):reject(Error('native reply unavailable'))});
    child.stdout.on('data',chunk=>{try{pending+=chunk.toString('utf8');assert(pending.length<=1024*1024);let newline;
     while((newline=pending.indexOf('\n'))>=0){const frame=JSON.parse(pending.slice(0,newline));pending=pending.slice(newline+1);
      if(step++===0){assert.equal(frame.schema,'vcp-local-ready/1');child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocol_version:'99.0',client:{name:'package-incompatibility-probe',version:'1'},capabilities:[],required_capabilities:[]}})+'\n')}
      else{assert.equal(frame.error?.data?.kind,'unsupported_version');value={code:frame.error.code,data:frame.error.data};child.stdin.end()}
     }}catch(error){stop(error)}});
    child.stdin.on('error',stop);child.stdin.write(JSON.stringify({schema:'vcp-local-bootstrap/1',workspace:input.workspace,data:input.data,role:'observer'})+'\n');
   });
  }else await vscode.commands.executeCommand('vcp.disconnect');
  assert(developmentPathAbsent);
  fs.writeFileSync(input.result,JSON.stringify({ok:true,mode:input.mode,version:extension.packageJSON.version,reloaded,scope:state.scope,task:input.task,unsupportedNegotiation:rejected??null,developmentPathAbsent:developmentPathAbsent}));
 }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,unexpectedPath,error:String(error.stack),state:api?.getConnectionState?.()}));throw error}
};
