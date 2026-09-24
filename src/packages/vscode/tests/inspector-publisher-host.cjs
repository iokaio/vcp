// SPDX-License-Identifier: Apache-2.0
// Real installed renderer and native encrypted publisher. Only explicit host
// file selection and confirmation receive deterministic qualification answers.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module'),vscode=require('vscode');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.run=async()=>{
 const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
 const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);
 const target=createRequire(path.join(extension.extensionPath,'dist','extension.js'))('vscode');
 const restores=[],calls=[],errors=[];let api,renderer,phase='activate',reloading=false,pickers=0,confirmations=0;
 const replace=(object,key,value)=>{const previous=object[key];object[key]=value;restores.push(()=>{object[key]=previous;});};
 const wait=async(predicate,label)=>{for(let i=0;i<900;i++){const value=predicate();if(value)return value;await delay(50);}throw Error(`deadline: ${label}`);};
 const state=()=>api.getInspectorState();
 const last=method=>calls.findLast(call=>call.method===method&&call.reply);
 const click=async label=>{const action=await wait(()=>state()?.actions.find(action=>action.label===label&&!action.disabledReason),`action ${label}`);await renderer.click(`[data-action="${action.id}"]`);await wait(()=>!state()?.actions.some(a=>a.id===action.id),`handled ${label}`);};
 const publisher=async()=>{await vscode.commands.executeCommand('vcp.inspectors.focus');await renderer.click('[data-tab="publisher"]');await wait(()=>state()?.tab==='publisher'&&state().phase==='current','publisher display');};
 try{
  replace(target.window,'showOpenDialog',async options=>{assert.equal(options.title,'Select native encrypted publisher profile outside the workspace');pickers++;assert(!fs.existsSync(input.marker),'reload must not request native key/profile loading');return[vscode.Uri.file(input.profile)];});
  replace(target.window,'showWarningMessage',async(message,options,...choices)=>{assert(message.includes('encrypted backup'));assert(choices.includes('Confirm'));confirmations++;return'Confirm';});
  replace(target.window,'showErrorMessage',async message=>{errors.push(message);});
  api=await extension.activate();
  const expected=fs.realpathSync(path.join(extension.extensionPath,'dist','engine_connection.js')).toLowerCase();
  const module=Object.values(require.cache).find(row=>row.filename&&path.basename(row.filename)==='engine_connection.js'&&fs.realpathSync(row.filename).toLowerCase()===expected);assert(module);
  const prototype=module.exports.EngineConnection.prototype,original=prototype.currentClient,wrapped=new WeakSet();
  replace(prototype,'currentClient',function(){const client=original.call(this);if(client&&!wrapped.has(client)){wrapped.add(client);const call=client.call;replace(client,'call',async function(method,params,...rest){try{const reply=await call.call(this,method,params,...rest);calls.push({method,params,reply});return reply;}catch(error){calls.push({method,error:error.code});throw error;}});}return client;});
  renderer=await require('./inspector-cdp.cjs').connect(input.userData);
  if(fs.existsSync(input.marker)){
   phase='actual reload observer receipt';const marker=JSON.parse(fs.readFileSync(input.marker,'utf8'));assert.notEqual(marker.pid,process.pid);
   await wait(()=>api.getConnectionState().phase==='connected','restored observer');assert.equal(api.getConnectionState().role,'observer');
   await publisher();await click('Reconcile submitted commands');await wait(()=>last('command/read')?.params.command_id===marker.operation,'original receipt read');
   const receipt=last('command/read').reply;assert.deepEqual(receipt,marker.receipt);
   await click(`Open backup ${marker.operation}`);await wait(()=>last('backup/read')?.reply.value.operation===marker.operation,'recovered operation');
   const value=last('backup/read').reply.value;assert.equal(value.local_publication,'published');assert.equal(value.cloud_transfer,'unknown');assert.equal(value.restore_verification,'not_observed');
   await renderer.wait("document.getElementById('inspector-sections').textContent.includes('published')",'actual recovered published text');await renderer.inert();
   assert.equal(pickers,0);assert.equal(confirmations,0);assert(!calls.some(c=>['backup/create','backup/retry','backup/cancel','controller/acquire'].includes(c.method)));
   assert.equal(errors.length,0);await vscode.commands.executeCommand('vcp.disconnect');
   fs.writeFileSync(input.result,JSON.stringify({ok:true,operation:marker.operation,receipt:marker.receipt,version:vscode.version,installed:true,rendererCreate:true,localPublished:true,reloadObserver:true,originalReceipt:true,noAutomaticMutation:true,cloud:'unknown',restore:'not_observed',hostPids:[marker.pid,process.pid]}));return;
  }
  phase='explicit controller profile dialog';
  await target.workspace.getConfiguration('vcp').update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
  await target.workspace.getConfiguration('vcp').update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
  await vscode.commands.executeCommand('vcp.connectPublisherController',vscode.Uri.file(input.workspace).toString());
  await wait(()=>api.getConnectionState().phase==='connected','publisher controller');assert.equal(api.getConnectionState().role,'controller');assert.equal(pickers,1);
  await publisher();await wait(()=>last('backup/status')?.reply.value.capability.state==='loaded','loaded native publisher');
  phase='actual renderer publish';await click('Publish encrypted workspace backup');await wait(()=>last('backup/create'),'durable publisher receipt');
  const submitted=last('backup/create'),operation=submitted.params.mutation.command_id,receipt=submitted.reply;
  assert.equal(receipt.kind,'acceptance');assert.equal(receipt.value.command_id,operation);assert.equal(confirmations,1);
  await click(`Open backup ${operation}`);
  await wait(()=>last('backup/read')?.reply.value.operation===operation&&last('backup/read').reply.value.local_publication==='published'&&last('backup/read').reply.value.source_pins==='released','local encrypted publication and released pins');
  const job=last('backup/read').reply.value;assert.equal(job.cloud_transfer,'unknown');assert.equal(job.restore_verification,'not_observed');assert.equal(job.checkpoint,'matched');
  await renderer.wait("document.getElementById('inspector-sections').textContent.includes('published')",'actual published result');await renderer.inert();
  assert.equal(calls.filter(c=>c.method==='backup/create').length,1);assert.equal(errors.length,0);
  phase='reload after receipt';fs.writeFileSync(input.marker,JSON.stringify({pid:process.pid,operation,receipt}));reloading=true;
  await vscode.commands.executeCommand('workbench.action.reloadWindow');await new Promise(()=>{});
 }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,error:{message:String(error.message),stack:String(error.stack)},methods:calls.map(c=>({method:c.method,error:c.error})),state:api?.getInspectorState?.(),connection:api?.getConnectionState?.(),errors}));throw error;}
 finally{if(!reloading){renderer?.close();for(const restore of restores.reverse())restore();}}
};
