// SPDX-License-Identifier: Apache-2.0
// Actual renderer actions against a configured native execution host. Synthetic
// loopback responses cost no provider spend. The competing policy publication
// is a real RPC on the same controller, not a second canonical writer or CLI.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module'),{randomUUID}=require('node:crypto');
const vscode=require('vscode');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.run=async()=>{
 const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
 const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);
 const target=createRequire(path.join(extension.extensionPath,'dist','extension.js'))('vscode');
 const calls=[],restores=[],errors=[],coverage={};let phase='activate',api,renderer,client,holdEvents=false,releaseEvents;
 const eventGate=new Promise(resolve=>releaseEvents=resolve);
 const replace=(object,key,value)=>{const prior=object[key];object[key]=value;restores.push(()=>object[key]=prior)};
 const wait=async(predicate,label)=>{for(let i=0;i<300;i++){const value=predicate();if(value)return value;await delay(50)}throw Error(`deadline: ${label}`)};
 const state=()=>api.getInspectorState();
 const latest=method=>calls.findLast(row=>row.method===method&&row.reply)?.reply;
 const current=async tab=>{await wait(()=>state()?.tab===tab&&state().phase==='current',`current ${tab}`);await renderer.wait(`document.getElementById('inspector-phase').textContent==='current'`,`render ${tab}`)};
 const click=async label=>{const action=await wait(()=>state()?.actions.find(row=>row.label===label&&!row.disabledReason),`action ${label}`);await renderer.click(`[data-action="${action.id}"]`);return action.id};
 const tab=async name=>{await renderer.click(`[data-tab="${name}"]`);await current(name)};
 try{
  replace(target.window,'showQuickPick',async(items,options)=>{
   const rows=await items;
   if(options?.title==='Start an execution-backed VCP task')return rows.find(row=>row.folder.uri.fsPath.toLowerCase()===input.workspace.toLowerCase());
   if(options?.title==='Report coverage')return 'session';
   if(options?.title==='Policy field')return 'input_tokens';
   if(options?.title==='Pruning action for selected task')return 'exclude';
   throw Error(`unexpected pick ${options?.title}`);
  });
  replace(target.window,'showOpenDialog',async()=>[vscode.Uri.file(input.profile)]);
  const answers={'Task objective':'Qualify optimizer inspector actions and exact cost','Configured task cost cap (USD)':'1','Configured maximum requests':'8','Configured task deadline (seconds)':'300','Provider credential':'synthetic-cli-qualification','Window start in Unix milliseconds; blank includes all retained history':'','New limit (nonnegative integer)':'4000','Historical policy revision to restore':'1'};
  replace(target.window,'showInputBox',async options=>{if(options.title==='Window end in Unix milliseconds')return Date.now().toString();assert(Object.hasOwn(answers,options.title),options.title);return answers[options.title]});
  replace(target.window,'showWarningMessage',async(_message,_options,...choices)=>choices.includes('Confirm')?'Confirm':choices.includes('Apply')?'Apply':undefined);
  replace(target.window,'showInformationMessage',async()=>undefined);replace(target.window,'showErrorMessage',async message=>errors.push(message));
  api=await extension.activate();
  const source=fs.realpathSync(path.join(extension.extensionPath,'dist','engine_connection.js')).toLowerCase();
  const module=Object.values(require.cache).find(row=>row.filename&&path.basename(row.filename)==='engine_connection.js'&&fs.realpathSync(row.filename).toLowerCase()===source);assert(module);
  const prototype=module.exports.EngineConnection.prototype,original=prototype.currentClient,wrapped=new WeakSet();
  replace(prototype,'currentClient',function(){const currentClient=original.call(this);if(currentClient){client=currentClient;if(!wrapped.has(client)){wrapped.add(client);const call=client.call;replace(client,'call',async function(method,params,...rest){try{const reply=await call.call(this,method,params,...rest);calls.push({method,params,reply});if(method==='events/next'&&holdEvents)await eventGate;return reply}catch(error){calls.push({method,params,error:{code:error.code,classification:error.classification}});throw error}})}}return currentClient});
  await target.workspace.getConfiguration('vcp').update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
  await target.workspace.getConfiguration('vcp').update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
  phase='start configured execution';await vscode.commands.executeCommand('vcp.startEditorTask');assert.equal(errors.length,0,JSON.stringify(errors));
  const task=calls.find(row=>row.method==='turn/start').params.task;
  const execution=await wait(()=>api.getTaskState()?.rows.find(row=>row.task===task),'actual accepted execution task');
  await api.dispatchTaskMessage({action:'select',id:execution.actionId});
  await wait(()=>api.getTaskState()?.detail?.task.task===task,'selected actual execution task');
  await wait(()=>api.getTaskState()?.phase==='current'&&api.getTaskState()?.usage&&BigInt(api.getTaskState().usage.settled_micros)>0n,'settled synthetic cost');
  await delay(750);await wait(()=>api.getTaskState()?.phase==='current'&&api.getTaskState()?.detail,'settled task projection');
  // Delay only delivery of actual event replies to keep the old review visible
  // across a competing canonical publication. No engine result is substituted.
  holdEvents=true;
  renderer=await require('./inspector-cdp.cjs').connect(input.userData);
  await vscode.commands.executeCommand('vcp.inspectors.focus');await current('history');
  phase='exact root cost';await tab('cost');const usage=latest('usage/read').value;assert.equal(usage.currency,'USD');assert(BigInt(usage.settled_micros)>0n);assert.equal(usage.cap_micros,'1000000');assert((await renderer.text()).includes(usage.settled_micros));coverage.costLedger=true;
  phase='report capture';await tab('optimizer');await click('Capture optimization report');await wait(()=>latest('routing/reportRead'),'captured report read');await current('optimizer');const report=latest('routing/reportRead').value.report;assert(latest('routing/reportCapture'));coverage.reportCapture=true;
  phase='positive apply';await click('Preview policy edit');await wait(()=>latest('routing/preview'),'review response');await current('optimizer');const first=latest('routing/preview').value;assert.equal(first.operation,'apply');assert((await renderer.text()).includes('4000'));await click('Apply reviewed policy');await wait(()=>calls.some(row=>row.method==='routing/apply'&&row.reply),'applied policy receipt');await current('optimizer');coverage.apply=true;
  phase='stale review after concurrent real policy RPC';await click(`Open report ${report}`);await current('optimizer');const priorPreview=latest('routing/preview');await click('Preview policy edit');await wait(()=>latest('routing/preview')!==priorPreview,'second preview');await current('optimizer');const stale=latest('routing/preview').value;
  const outOfBand=await client.call('routing/preview',{scope:client.scope,expected_policy_revision:'1',proposal:{kind:'rollback',target_revision:'0'}});
  const publication=await client.call('routing/rollback',{scope:client.scope,mutation:{command_id:randomUUID(),expected_revision:api.getConnectionState().workspaceRevision,steering_revision:'0'},expected_binding_revision:api.getConnectionState().bindingRevision,preview_id:outOfBand.value.preview_id,preview_sha256:outOfBand.value.preview_sha256});assert.equal(publication.kind,'acceptance');
  await click('Apply reviewed policy');await wait(()=>calls.some(row=>row.method==='routing/apply'&&row.params.preview_id===stale.preview_id&&row.error),'stale rejection');
  const rejected=calls.findLast(row=>row.method==='routing/apply'&&row.error);assert.equal(rejected.error.classification.applicationCode,'VERSION_CONFLICT');assert.equal(calls.filter(row=>row.method==='routing/apply'&&row.params.preview_id===stale.preview_id).length,1);coverage.stalePreview=true;
  phase='positive rollback';await wait(()=>!state()?.actions.some(row=>row.label==='Apply reviewed policy'),'rejected review controls removed');await current('optimizer');await renderer.click('#inspector-refresh');await current('optimizer');const beforeRollback=latest('routing/preview');await click('Preview policy rollback');await wait(()=>latest('routing/preview')!==beforeRollback,'rollback review');await current('optimizer');const rollback=latest('routing/preview').value;assert.equal(rollback.operation,'rollback');assert.equal(rollback.target_policy_revision,'1');await click('Apply reviewed rollback');await wait(()=>calls.some(row=>row.method==='routing/rollback'&&row.params.preview_id===rollback.preview_id&&row.reply),'rollback receipt');coverage.rollback=true;
  // Execution-backed source dependencies can make this session-scoped pruning
  // selector unavailable. Qualify that denial without widening its authority;
  // positive protected-target previews use the separate retention fixture.
  phase='pruning protected metadata review';await current('optimizer');await tab('pruning');await click('Preview pruning for selected task');await wait(()=>calls.some(row=>row.method==='memory/forgetPreview'&&(row.reply||row.error)),'pruning preview outcome');const pruning=calls.findLast(row=>row.method==='memory/forgetPreview');if(pruning.reply){await current('pruning');assert.equal(pruning.reply.value.task,task);assert(/^\d+$/.test(pruning.reply.value.protected_count));assert((await renderer.text()).includes('protected_count'));coverage.pruningPreview=true;}else{assert.equal(pruning.error.classification.applicationCode,'POLICY_DENIED');await wait(()=>state()?.phase==='unavailable','scoped pruning denial');assert.equal(state().actions.length,0);coverage.pruningDenied=true;}assert(!calls.some(row=>row.method==='memory/forget'));
  await renderer.inert();coverage.actualRenderer=true;coverage.noReplay=true;
  const methods=calls.filter(row=>['routing/reportCapture','routing/apply','routing/rollback','memory/forgetPreview'].includes(row.method)).map(row=>({method:row.method,command:row.params.mutation?.command_id,kind:row.reply?.kind,error:row.error?.classification?.applicationCode}));
  releaseEvents();await vscode.commands.executeCommand('vcp.disconnect');
  fs.writeFileSync(input.result,JSON.stringify({ok:true,version:vscode.version,installed:true,...coverage,policyMutationSource:'same authenticated controller real RPC; no second writer',eventDeliveryDelay:true,usage,methods}));
 }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,error:{message:String(error.message),stack:String(error.stack)},methods:calls.map(row=>({method:row.method,error:row.error})),taskState:api?.getTaskState?.(),usage:latest("usage/read"),presentation:latest("task/presentation"),state:api?.getInspectorState?.(),connection:api?.getConnectionState?.(),errors}));throw error}
 finally{releaseEvents();renderer?.close();for(const restore of restores.reverse())restore()}
};
