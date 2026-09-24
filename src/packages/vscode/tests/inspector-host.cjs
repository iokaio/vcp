// SPDX-License-Identifier: Apache-2.0
// Installed-editor qualification: actual renderer clicks and actual engine
// replies. Only native host query prompts receive deterministic test answers.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {createRequire}=require('node:module');
const vscode=require('vscode');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.run=async()=>{
  const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
  assert.equal(typeof input.expirePreview,'boolean');
  const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);
  const target=createRequire(path.join(extension.extensionPath,'dist','extension.js'))('vscode');
  let phase='activate',api,renderer,reloading=false,previewReference;const calls=[],starts=[],restores=[],coverage={},dispatches=[];
  const replace=(object,key,value)=>{const prior=object[key];object[key]=value;restores.push(()=>{object[key]=prior;});};
  const wait=async(predicate,label)=>{for(let i=0;i<200;i++){const value=predicate();if(value)return value;await delay(50);}throw Error(`deadline: ${label}`);};
  const owner=async action=>{fs.writeFileSync(input.ownerRequest+'.partial',JSON.stringify(action));fs.renameSync(input.ownerRequest+'.partial',input.ownerRequest);return wait(()=>{if(!fs.existsSync(input.ownerAck))return false;const reply=JSON.parse(fs.readFileSync(input.ownerAck,'utf8'));return reply.action===action?reply.result:false;},`external owner ${action}`);};
  const state=()=>api.getInspectorState();
  const latest=method=>calls.findLast(row=>row.method===method&&row.reply)?.reply;
  const current=async tab=>{await wait(()=>state()?.tab===tab&&['current','unavailable'].includes(state().phase),`current ${tab}`);await renderer.wait(`document.getElementById('inspector-phase').textContent===${JSON.stringify(state().phase)}`,`render ${tab}`);return state();};
  const click=async label=>{const action=await wait(()=>state()?.actions.find(action=>action.label===label&&!action.disabledReason),`action ${label}`);await renderer.click(`[data-action="${action.id}"]`);await wait(()=>!state()?.actions.some(row=>row.id===action.id),`handled ${label}`);};
  const tab=async name=>{await renderer.click(`[data-tab="${name}"]`);await current(name);};
  const rows=()=>state().sections.flatMap(section=>(section.rows??[]).map(row=>JSON.parse(row.fields.find(field=>field.label==='Details').value)));
  try{
    replace(target.window,'showInputBox',async options=>{if(options.title==='Open pruning preview'){assert(previewReference);return previewReference;}assert.equal(options.title,'Search memory');return 'Parser';});
    replace(target.window,'showQuickPick',async(items,options)=>{assert.equal(options.title,'Pruning action for selected task');assert(items.includes('purge'));return 'purge';});
    api=await extension.activate();assert.equal(typeof api.getInspectorState,'function');
    const source=fs.realpathSync(path.join(extension.extensionPath,'dist','engine_connection.js')).toLowerCase();
    const module=Object.values(require.cache).find(row=>row.filename&&path.basename(row.filename)==='engine_connection.js'&&fs.realpathSync(row.filename).toLowerCase()===source);assert(module);
    const prototype=module.exports.EngineConnection.prototype,original=prototype.currentClient,wrapped=new WeakSet();
    replace(prototype,'currentClient',function(){const client=original.call(this);if(client&&!wrapped.has(client)){wrapped.add(client);const call=client.call;replace(client,'call',async function(method,params,...rest){starts.push({method,cleared:api?.getInspectorState?.()?.sections.length===0});try{const reply=await call.call(this,method,params,...rest);calls.push({method,params,reply});return reply;}catch(error){calls.push({method,params,error:error.code});throw error;}});}return client;});
    const inspectorSource=fs.realpathSync(path.join(extension.extensionPath,'dist','inspector_session.js')).toLowerCase();
    const inspectorModule=Object.values(require.cache).find(row=>row.filename&&path.basename(row.filename)==='inspector_session.js'&&fs.realpathSync(row.filename).toLowerCase()===inspectorSource);assert(inspectorModule);
    const inspectorPrototype=inspectorModule.exports.InspectorSession.prototype,dispatch=inspectorPrototype.dispatch;
    replace(inspectorPrototype,'dispatch',async function(message){const result=await dispatch.call(this,message);dispatches.push(message);return result;});
    renderer=await require('./inspector-cdp.cjs').connect(input.userData);
    if(fs.existsSync(input.marker)){
      phase='reload observer';const marker=JSON.parse(fs.readFileSync(input.marker,'utf8'));assert.notEqual(marker.pid,process.pid);
      await wait(()=>api.getConnectionState().phase==='connected','restored observer');assert.equal(api.getConnectionState().role,'observer');
      await wait(()=>api.getTaskState()?.phase==='current'&&api.getTaskState()?.detail,'restored selected task');
      await vscode.commands.executeCommand('vcp.inspectors.focus');await current('history');
      assert(calls.some(row=>row.method==='history/query'&&row.reply),'reload freshly reauthorizes');
      assert(!calls.some(row=>['controller/acquire','turn/start','memory/forget','backup/create','routing/apply'].includes(row.method)));
      await renderer.inert();
      phase='actual authority revocation after observer reload';
      assert((await renderer.text()).includes(input.expected.scope.workspace));
      const oldAction=state().actions.find(action=>!action.disabledReason);assert(oldAction);
      assert.equal(await renderer.inspector(`(()=>{globalThis.__vcpAuthorityButton=document.querySelector('[data-action="${oldAction.id}"]');return !!globalThis.__vcpAuthorityButton;})()`),true);
      const authorityStarts=starts.length,authorityCalls=calls.length;
      assert.equal((await owner('revoke')).prior_external_ownership,true);
      await wait(()=>!state().actions.some(action=>action.id===oldAction.id)&&((state().sections.length===0&&state().actions.every(action=>action.disabledReason!==undefined))||calls.slice(authorityCalls).some(row=>row.method==='history/query'&&row.reply)),'authority invalidated or freshly authorized content');
      if(state().sections.length){assert(starts.slice(authorityStarts).some(row=>row.method==='history/query'&&row.cleared),'old sections cleared before native reauthorization');await current('history');}
      else await renderer.wait("document.getElementById('inspector-sections').textContent.length===0",'revoked DOM cleared');
      const rejectedCalls=calls.length;await renderer.inspector('globalThis.__vcpAuthorityButton.click()');
      await wait(()=>dispatches.some(message=>message.action==='invoke'&&message.id===oldAction.id),'prior authority DOM handle rejected');
      assert(!calls.slice(rejectedCalls).some(row=>row.method==='history/query'));
      await vscode.commands.executeCommand('vcp.disconnect');
      fs.writeFileSync(input.result,JSON.stringify({ok:true,version:vscode.version,installed:true,...marker.coverage,reload:true,authorityInvalidation:true,observerOwnershipPreserved:true,noAutomaticMutation:true,hostPids:[marker.pid,process.pid]}));return;
    }
    phase='attach external controller observer';
    await target.workspace.getConfiguration('vcp').update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    await target.workspace.getConfiguration('vcp').update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.attachObserver',vscode.Uri.file(input.workspace).toString(),input.reference);
    await wait(()=>api.getConnectionState().phase==='connected','observer connected');assert.equal(api.getConnectionState().role,'observer');
    await wait(()=>api.getTaskState()?.phase==='current'&&api.getTaskState()?.detail?.task.task===input.expected.task,'selected canonical task');
    await vscode.commands.executeCommand('vcp.inspectors.focus');await current('history');
    phase='history pages';await click('Session history');await current('history');
    const history=[];for(let page=0;page<32;page++){
      const found=rows();history.push(...found);
      const text=await renderer.text();for(const row of found)assert(text.includes(row.id));
      if(!state().actions.some(action=>action.label==='Next page'))break;
      const before=latest('history/query');await click('Next page');await wait(()=>latest('history/query')!==before,'next history reply');await current('history');
    }
    const byId=new Map(history.map(row=>[row.id,row]));assert.equal(byId.size,history.length);
    for(const expected of input.expected.history){const row=byId.get(expected.id);assert(row,`CLI event ${expected.id}`);assert.equal(row.sequence,expected.sequence);assert.equal(row.timestamp_ms,expected.timestamp_ms);assert.deepEqual(row.task,expected.task);}
    assert(history.length>16);coverage.historyPaged=true;coverage.cliHistoryParity=true;
    phase='memory versions';await tab('memory');await click('Search memory');await wait(()=>latest('memory/query'),'memory query reply');await current('memory');
    await click(`Claim versions ${input.expected.claim}`);await wait(()=>latest('memory/history'),'memory history reply');await current('memory');
    const versions=[];for(let page=0;page<8;page++){
      versions.push(...latest('memory/history').value.versions.map(row=>row.finding.version));
      assert((await renderer.text()).includes('Parser'));
      if(!state().actions.some(action=>action.label==='Next page'))break;
      const before=latest('memory/history');await click('Next page');await wait(()=>latest('memory/history')!==before,'next memory reply');await current('memory');
    }
    assert.deepEqual(versions,input.expected.versions);coverage.memoryPaged=true;
    phase='context and actual hostile bytes';await tab('evidence');await wait(()=>latest('context/inspect'),'context reply');await current('evidence');
    await click(`Read ${input.expected.artifact}`);await wait(()=>latest('artifact/read'),'artifact reply');await current('evidence');
    assert((await renderer.text()).includes('inspector-native-sentinel'));assert((await renderer.text()).includes('<script>hostile()</script>'));await renderer.inert();
    const first=latest('artifact/read');await click('Next byte range');await wait(()=>latest('artifact/read')!==first,'next byte range');await current('evidence');
    assert.equal(latest('artifact/read').value.offset,'16384');await renderer.inert();coverage.hostileInert=true;coverage.artifactRange=true;
    phase='policy routing cost';await tab('policy');await wait(()=>latest('policy/read'),'policy reply');await current('policy');assert((await renderer.text()).includes('operation_not_evaluated'));
    assert.equal(latest('policy/read').value.persisted.mode,input.expected.inspection.policy.mode);await click('Show grants');await current('policy');
    await tab('routing');await wait(()=>latest('routing/status'),'routing reply');await current('routing');assert((await renderer.text()).includes('9007199254740993'));assert((await renderer.text()).includes('host_unconfigured'));
    await tab('cost');assert.equal(state().phase,'unavailable','no canonical ledger remains unavailable');coverage.policyRoutingParity=true;coverage.costUnavailable=true;
    phase='actual webview recreation';await tab('evidence');await click(`Read ${input.expected.artifact}`);await current('evidence');
    await vscode.commands.executeCommand('workbench.view.explorer');await wait(()=>state()?.sections.length===0,'hidden content cleared');
    const count=calls.length;await vscode.commands.executeCommand('vcp.inspectors.focus');await current('evidence');assert(calls.slice(count).some(row=>row.reply),'reveal reauthorizes');coverage.recreate=true;
    phase='actual retention change while bytes are visible';
    previewReference=(await owner('preview')).preview;assert.equal(typeof previewReference,'string');
    await tab('pruning');const ownerPreviewCalls=calls.length;await click('Open pruning preview');await current('pruning');
    assert(calls.slice(ownerPreviewCalls).some(row=>row.method==='memory/forgetPreviewRead'&&row.error));
    assert.equal(state().phase,'unavailable');assert.equal(state().sections.length,0);coverage.connectionLocalPreviewDenied=true;
    await tab('history');await tab('pruning');
    const ownPreview=async()=>{const before=calls.length;await click('Preview pruning for selected task');await wait(()=>calls.slice(before).some(row=>row.method==='memory/forgetPreview'&&row.reply),'observer own preview reply');await current('pruning');const page=latest('memory/forgetPreview').value;assert.notEqual(page.selected_count,'0');assert.equal(page.protected_count,'0');assert((await renderer.text()).includes(page.digest));const apply=state().actions.find(action=>action.label==='Apply reviewed pruning');assert(apply?.disabledReason);return page;};
    let own=await ownPreview();coverage.pruningPreview=true;
    if(input.expirePreview){
      phase='actual native pruning preview expiry';assert(own.expires_in_ms>0&&own.expires_in_ms<=60000);
      previewReference=own.preview;await delay(own.expires_in_ms+100);
      await tab('history');await tab('pruning');
      const expiredCalls=calls.length;await click('Open pruning preview');await current('pruning');
      assert(calls.slice(expiredCalls).some(row=>row.method==='memory/forgetPreviewRead'&&row.error));assert.equal(state().phase,'unavailable');assert.equal(state().sections.length,0);coverage.pruningExpiry=true;
      await tab('history');await tab('pruning');own=await ownPreview();
    }
    previewReference=own.preview;
    await tab('evidence');
    await renderer.click('#inspector-refresh');await current('evidence');
    // Return through actual context navigation rather than forging an artifact action.
    await tab('history');await tab('evidence');await click(`Read ${input.expected.retention_artifact}`);await current('evidence');
    assert((await renderer.text()).includes('retention-inspector-sentinel'));
    const stale=state().actions.find(action=>action.label==='Next byte range');assert(stale);
    assert.equal(await renderer.inspector(`(()=>{globalThis.__vcpStaleButton=document.querySelector('[data-action="${stale.id}"]');return !!globalThis.__vcpStaleButton;})()`),true);
    assert.equal((await owner('purge')).logical_unavailable,true);
    await wait(()=>!JSON.stringify(state()?.sections).includes('retention-inspector-sentinel')&&!state()?.actions.some(action=>action.label===`Read ${input.expected.retention_artifact}`),'purged visible bytes and navigation cleared');
    await renderer.wait("!document.body.innerText.includes('retention-inspector-sentinel')",'purged DOM cleared');
    const staleCalls=calls.length;
    await renderer.inspector('globalThis.__vcpStaleButton.click()');
    await wait(()=>dispatches.some(message=>message.action==='invoke'&&message.id===stale.id),'stale actual renderer click rejected by host');
    assert(!calls.slice(staleCalls).some(row=>row.method==='artifact/read'),'obsolete DOM action cannot fetch bytes');
    await vscode.commands.executeCommand('workbench.view.explorer');await vscode.commands.executeCommand('vcp.inspectors.focus');
    await current('evidence');assert(!(await renderer.text()).includes('retention-inspector-sentinel'));
    assert(!state().actions.some(action=>action.label===`Read ${input.expected.retention_artifact}`));coverage.retentionInvalidation=true;
    phase='own pruning preview becomes stale after purge';await tab('pruning');
    const stalePreviewCalls=calls.length;await click('Open pruning preview');await current('pruning');
    assert(calls.slice(stalePreviewCalls).some(row=>row.method==='memory/forgetPreviewRead'&&row.error));
    assert.equal(state().phase,'unavailable');assert.equal(state().sections.length,0);coverage.stalePruningPreview=true;
    phase='reload after purge';fs.writeFileSync(input.marker,JSON.stringify({pid:process.pid,coverage}));reloading=true;
    await vscode.commands.executeCommand('workbench.action.reloadWindow');await new Promise(()=>{});
  }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,error:{message:String(error.message),stack:String(error.stack)},methods:calls.map(row=>({method:row.method,error:row.error})),state:api?.getInspectorState?.(),connection:api?.getConnectionState?.()}));throw error;}
  finally{if(!reloading){renderer?.close();for(const restore of restores.reverse())restore();}}
};
