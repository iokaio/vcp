// SPDX-License-Identifier: Apache-2.0
// Loaded by the actual VS Code extension host, not a mocked API.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL}=require('node:url');
const vscode = require('vscode');
let phase='activation';
let restoreSdkCalls=()=>{};
const activeSnapshots=new Map();
const observedMethods=new Map();
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function taskState(api,predicate,label) {
  let state;
  for(let i=0;i<300;i++){state=api.getTaskState();if(state&&predicate(state))return state;await delay(100);}
  assert.fail(`${label}: ${JSON.stringify(state)}`);
}
function compareCli(state,fixture) {
  const cli=fixture.cli,detail=state.detail;
  assert.equal(detail.task.task,cli.task);assert.equal(detail.task.state,cli.state);
  assert.equal(detail.objective.text,cli.objective);assert.equal(detail.task.revision,String(cli.revision));
  assert.equal(state.rows.filter(row=>row.parent===fixture.rootTask).length,cli.child_count);
  assert.deepEqual(detail.questions.map(row=>({id:row.input.id,task:detail.task.task,actionable:row.actionable,digest:row.input.operation_digest})),cli.pending_questions.map(row=>({id:row.id,task:row.task,actionable:row.actionable,digest:row.operation_digest})));
  if(cli.cost===null){assert.equal(state.usage,undefined,'missing canonical ledger remains unavailable');} else {
  assert.equal(state.usage.currency,cli.cost.currency);assert.equal(state.usage.settled_micros,String(cli.cost.known));
  assert.equal(state.usage.reserved_micros,String(cli.cost.reserved));assert.equal(state.usage.unresolved_micros,String(cli.cost.uncertain));
  assert.equal(state.usage.cap_micros,String(cli.cost.cap));
  }
  assert.equal(detail.task.turn,cli.current_step?.id??null);
  // CLI receives a configured model label; editor must not claim it was observed.
  assert.equal(cli.model,'never-dispatched');assert.equal(detail.model.source,'unavailable');
}

exports.run = async function run() {
  const inputPath=process.env.VCP_EXTENSION_TEST_INPUT;
  assert(inputPath);
  const bytes=fs.readFileSync(inputPath);assert(bytes.length<=16*1024);
  const input=JSON.parse(bytes.toString('utf8'));
  const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);
  assert.equal(path.resolve(extension.extensionPath),path.resolve(input.extension));
  const sdkPath=fs.realpathSync(path.join(extension.extensionPath,'node_modules','@vcp','sdk','dist','index.js'));
  assert(sdkPath.startsWith(fs.realpathSync(extension.extensionPath)+path.sep),'SDK resolves inside staged package');
  assert.equal(typeof (await import(pathToFileURL(sdkPath).href)).launchLocal,'function');
  const schemaPath=fs.realpathSync(path.join(extension.extensionPath,'node_modules','@vcp','protocol','schema.json'));
  assert(schemaPath.startsWith(fs.realpathSync(extension.extensionPath)+path.sep),'canonical schema stays inside staged package');
  const api=await extension.activate();
  assert.equal(typeof api.getTaskState,'function');assert.equal(typeof api.dispatchTaskMessage,'function');
  const connectionPath=fs.realpathSync(path.join(extension.extensionPath,'dist','engine_connection.js')).toLowerCase();
  const connectionModule=Object.values(require.cache).find(module=>module.filename&&path.basename(module.filename)==='engine_connection.js'&&fs.realpathSync(module.filename).toLowerCase()===connectionPath);
  assert(connectionModule,'actual staged connection module is loaded');
  const prototype=connectionModule.exports.EngineConnection.prototype;
  const originalCurrent=prototype.currentClient;
  const wrapped=new Map();
  prototype.currentClient=function(){
    const client=originalCurrent.call(this);
    if(client&&!wrapped.has(client)){
      const originalCall=client.call;
      wrapped.set(client,originalCall);
      client.call=async function(method,params,...rest){
        observedMethods.set(method,(observedMethods.get(method)||0)+1);
        const reply=await originalCall.call(this,method,params,...rest);
        if(method==='session/snapshot'&&reply.value.complete)activeSnapshots.set(reply.value.subscription,{client:this,scope:{...params.scope},subscription:reply.value.subscription});
        if(method==='events/unsubscribe')activeSnapshots.delete(params.subscription);
        return reply;
      };
    }
    return client;
  };
  restoreSdkCalls=()=>{prototype.currentClient=originalCurrent;for(const [client,call] of wrapped)client.call=call;};
  if(input.mode==='tasks')return runTrustedTasks(api,input);
  const commands=await vscode.commands.getCommands(true);
  for(const command of ['vcp.connect','vcp.refreshConnection','vcp.disconnect'])assert(commands.includes(command));
  const configuration=vscode.workspace.getConfiguration('vcp');
  const trustEnabled=vscode.workspace.getConfiguration('security.workspace.trust').get('enabled');
  assert.notEqual(trustEnabled,false,'workspace trust must remain enabled');
  const folders=vscode.workspace.workspaceFolders;assert.equal(folders.length,5);
  assert.equal(folders[0].name,folders[1].name,'duplicate display names are intentional');
  const statuses=[];
  if(fs.existsSync(input.reloadMarker)) {
    const previous=JSON.parse(fs.readFileSync(input.reloadMarker,'utf8'));
    assert.notEqual(previous.pid,process.pid,'reload must replace the extension host process');
    let restored;
    for(let i=0;i<300;i++){restored=api.getConnectionState();if(restored.phase==='connected')break;await new Promise(resolve=>setTimeout(resolve,100));}
    assert.equal(restored.phase,'connected',JSON.stringify({restored,previousPaths:previous.paths,currentPaths:{executable:configuration.inspect('engineExecutable')?.globalValue,data:configuration.inspect('dataDirectory')?.globalValue}}));
    assert.equal(restored.role,'observer');assert.deepEqual(restored.scope,input.reloadFixture.scope);
    assert.equal(restored.pendingInputs,1);
    const sdk=await import(pathToFileURL(sdkPath).href);
    const observed=await sdk.reconnectObserverLocal({executable:input.executable,reference:input.reloadFixture.reference});
    try {
      const lease=(await observed.call('controller/read',{scope:observed.scope})).value;
      assert.equal(lease.ownership,'other_connection');
      assert.deepEqual(lease,previous.lease);
    }finally{await observed.dispose();}
    await vscode.commands.executeCommand('vcp.disconnect');
    fs.writeFileSync(input.result,JSON.stringify({...previous.result,reloadVerified:true,reloadHostPid:process.pid},null,2));
    return;
  }
  let reloading=false;
  try {
    phase='missing executable';
    await configuration.update('engineExecutable',undefined,vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.connect',folders[0].uri.toString());
    assert.equal(api.getConnectionState().phase,'unavailable','workspace executable cannot supply user authority');
    await configuration.update('engineExecutable',path.join(input.unselected,'missing-engine.exe'),vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.connect',folders[0].uri.toString());
    assert.equal(api.getConnectionState().phase,'unavailable','missing configured executable stays unavailable');
    await configuration.update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    for(const fixture of input.fixtures) {
      phase='ordinary observer '+fixture.scope.workspace;
      const uri=vscode.Uri.file(fixture.workspace).toString();
      assert(folders.some(folder=>folder.uri.toString()===uri));
      await configuration.update('dataDirectory',fixture.data,vscode.ConfigurationTarget.Global);
      const connected=await vscode.commands.executeCommand('vcp.connect',uri);
      assert.equal(connected.phase,'connected',JSON.stringify(connected));
      assert.equal(connected.role,'observer');
      assert.equal(connected.workspaceUri,uri);
      assert.deepEqual(connected.scope,fixture.scope);
      assert.equal(connected.editorTrusted,vscode.workspace.isTrusted);
      assert.equal(connected.protocolVersion,'1.0');
      assert.equal(connected.engineExecutable,input.executable);
      assert(connected.engineBuild && connected.host.id);
      assert.equal(connected.host.id,fixture.host);
      assert.equal(connected.host.platform,'windows');
      assert.equal(connected.workspaceRoot,fixture.canonicalRoot);
      assert.equal(connected.rootId,fixture.rootId);
      assert.equal(connected.bindingRevision,fixture.bindingRevision);
      assert.equal(connected.pendingInputs,1);
      assert.equal(connected.taskCount,2);
      phase='task child presentation '+fixture.scope.workspace;
      let tasks=await taskState(api,state=>state.phase==='current'&&state.total===2&&state.detail?.task?.scope.workspace===fixture.scope.workspace,'task snapshot');
      assert.equal(tasks.detail.task.task,fixture.rootTask);
      assert.equal(tasks.detail.questions.length,1);
      assert.equal(tasks.detail.questions[0].actionable,false,'historical question grants no current authority');
      compareCli(tasks,fixture);
      assert.equal(tasks.usage,undefined,'inspection fixture has no canonical ledger');
      assert(tasks.actions.every(action=>action.disabledReason),'observer never receives enabled mutation');
      const blocked=tasks.actions[0];
      if(blocked)await api.dispatchTaskMessage({action:'task',id:blocked.id});
      await api.dispatchTaskMessage({action:'task',id:'00000000-0000-0000-0000-000000000000',method:'task/cancel'});
      await api.dispatchTaskMessage({action:'task',id:'00000000-0000-0000-0000-000000000000'});
      await api.dispatchTaskMessage({action:'refresh'});
      tasks=await taskState(api,state=>state.phase==='current'&&state.detail?.task?.scope.workspace===fixture.scope.workspace,'observer action refresh');
      const child=tasks.rows.find(row=>row.task===fixture.childTask);assert(child);assert.equal(child.parent,fixture.rootTask);
      await api.dispatchTaskMessage({action:'select',id:child.actionId});
      tasks=await taskState(api,state=>state.detail?.task?.task===fixture.childTask,'child selection');
      assert.equal(tasks.detail.task.parent,fixture.rootTask);assert.equal(tasks.detail.task.root,fixture.rootTask);
      assert.equal(tasks.detail.objective.text,'child editor inspection <script>untrusted</script>');
      assert.deepEqual(tasks.detail.objective_constraints,['preserve child scope']);
      assert.deepEqual(tasks.detail.objective_acceptance,['explicit review']);
      assert.equal(tasks.commands.length,0,'observer blocked actions never create journal mutations');
      phase='dropped task subscription '+fixture.scope.workspace;
      const subscriptions=[...activeSnapshots.values()].filter(row=>row.scope.workspace===fixture.scope.workspace);
      assert.equal(subscriptions.length,1,'only the actual task subscription remains active: '+JSON.stringify([...observedMethods]));
      const subscription=subscriptions[0];
      await subscription.client.call('events/unsubscribe',{scope:subscription.scope,subscription:subscription.subscription});
      tasks=await taskState(api,state=>state.phase==='current'&&state.detail?.task.task===fixture.childTask&&[...activeSnapshots.values()].some(row=>row.scope.workspace===fixture.scope.workspace&&row.subscription!==subscription.subscription),'expired subscription automatically resynchronizes');
      assert.equal(tasks.rows.find(row=>row.task===fixture.rootTask).pendingInputs.length,1);
      assert.equal(tasks.commands.length,0);
      await vscode.commands.executeCommand('vcp.tasks.focus');
      await vscode.commands.executeCommand('vcp.connection.focus');
      const refreshed=await vscode.commands.executeCommand('vcp.refreshConnection');
      assert.deepEqual(refreshed.scope,fixture.scope);
      assert.equal(refreshed.pendingInputs,1);
      statuses.push(refreshed);
      await vscode.commands.executeCommand('vcp.disconnect');
      assert.equal(api.getConnectionState().phase,'disconnected');
    }
    // No chooser or arbitrary filesystem path may act as a selected folder.
    const beforeRejected=api.getConnectionState();
    await vscode.commands.executeCommand('vcp.connect',vscode.Uri.file(input.unselected).toString());
    assert.equal(api.getConnectionState().phase,'disconnected');
    assert.equal(api.getConnectionState().generation,beforeRejected.generation);
    phase='trust controller';
    await configuration.update('dataDirectory',input.trustFixture.data,vscode.ConfigurationTarget.Global);
    const controlled=await vscode.commands.executeCommand('vcp.connectController',vscode.Uri.file(input.trustFixture.workspace).toString());
    assert.equal(controlled.phase,'connected',JSON.stringify(controlled));assert.equal(controlled.role,'controller');
    assert.equal(controlled.engineTrust,'trusted');assert.equal(controlled.workspaceRevision,'1');
    assert.equal(vscode.workspace.isTrusted,false,'fixture must exercise actual restricted editor mode');
    phase='restricted grant';
    const denied=await vscode.commands.executeCommand('vcp.grantTrust');
    assert.equal(denied.workspaceRevision,'1');assert.equal(denied.role,'controller');
    phase='trust revoke';
    const revoked=await vscode.commands.executeCommand('vcp.revokeTrust');
    assert.equal(revoked.phase,'connected',JSON.stringify(revoked));assert.equal(revoked.role,'observer');
    assert.equal(revoked.engineTrust,'untrusted');assert.equal(revoked.workspaceRevision,'2');assert.equal(revoked.pendingInputs,1);
    await vscode.commands.executeCommand('vcp.disconnect');
    phase='moved root';
    const movedUri=vscode.Uri.file(input.movedFixture.workspace).toString();
    await configuration.update('dataDirectory',input.movedFixture.data,vscode.ConfigurationTarget.Global);
    const moved=await vscode.commands.executeCommand('vcp.reconcileRoot',movedUri,input.movedFixture.scope.workspace);
    assert.equal(moved.phase,'connected',JSON.stringify(moved));
    assert.deepEqual(moved.scope,input.movedFixture.scope);
    assert.equal(moved.rootId,input.movedFixture.rootId);
    assert.equal(moved.bindingRevision,input.movedFixture.bindingRevision);
    assert.equal(moved.pendingInputs,1);assert.equal(moved.engineTrust,'untrusted');
    await vscode.commands.executeCommand('vcp.disconnect');
    const result={droppedSubscriptionVerified:true,taskViewsVerified:true,cliParityVerified:true,trustVerified:true,movedVerified:true,ok:true,version:vscode.version,node:process.versions.node,extensionPath:extension.extensionPath,sdkPath,trustEnabled,actualTrusted:vscode.workspace.isTrusted,trustEvidence:vscode.workspace.isTrusted?'trusted development host; restricted mode not observed':'actual restricted workspace',remoteName:vscode.env.remoteName??null,statuses};
    phase='reload attach';
    await configuration.update('dataDirectory',input.reloadFixture.data,vscode.ConfigurationTarget.Global);
    const reload=await vscode.commands.executeCommand('vcp.attachObserver',vscode.Uri.file(input.reloadFixture.workspace).toString(),input.reloadFixture.reference);
    assert.equal(reload.phase,'connected',JSON.stringify(reload));assert.equal(reload.pendingInputs,1);
    const sdk=await import(pathToFileURL(sdkPath).href);
    const observed=await sdk.reconnectObserverLocal({executable:input.executable,reference:input.reloadFixture.reference});
    let lease;try{lease=(await observed.call('controller/read',{scope:observed.scope})).value;assert.equal(lease.ownership,'other_connection');}finally{await observed.dispose();}
    fs.writeFileSync(input.reloadMarker,JSON.stringify({pid:process.pid,lease,result,paths:{executable:configuration.inspect('engineExecutable')?.globalValue,data:configuration.inspect('dataDirectory')?.globalValue}}));
    assert.equal(api.getConnectionState().phase,'connected');
    phase='reload requested';
    reloading=true;
    await vscode.commands.executeCommand('workbench.action.reloadWindow').catch(error=>{if(error?.name!=='Canceled')throw error;});
    await new Promise(()=>{});
  }finally{
    if(!reloading)await vscode.commands.executeCommand('vcp.disconnect');
  }
};

async function runTrustedTasks(api,input) {
  assert.notEqual(vscode.workspace.getConfiguration('security.workspace.trust').get('enabled'),false);
  assert.equal(vscode.workspace.isTrusted,true,'isolated persisted trust preferences must produce genuine trusted editor mode');
  const fixture=input.fixture;
  if(fs.existsSync(input.reloadMarker)) {
    phase='trusted task reload';
    const previous=JSON.parse(fs.readFileSync(input.reloadMarker,'utf8'));
    assert.notEqual(previous.pid,process.pid);
    const tasks=await taskState(api,state=>state.phase==='current'&&state.detail?.task?.scope.workspace===fixture.scope.workspace&&state.commands.some(row=>row.commandId===previous.cancelCommand),'restored task command');
    assert.equal(api.getConnectionState().role,'observer');
    assert.equal(tasks.rows.find(row=>row.task===fixture.childTask).state,'cancelled');
    assert(tasks.commands.some(row=>row.commandId===previous.cancelCommand&&row.phase==='reconciled'));
    assert(tasks.commands.some(row=>row.commandId===previous.staleCommand&&row.phase==='rejected'));
    assert.equal(tasks.detail.questions[0].input.id,fixture.approval);
    await vscode.commands.executeCommand('vcp.disconnect');
    fs.writeFileSync(input.result,JSON.stringify({ok:true,actualTrusted:true,reloadVerified:true,cliParityVerified:true,duplicateVerified:true,staleVerified:true,cancelCommand:previous.cancelCommand,staleCommand:previous.staleCommand,version:vscode.version,hostPids:[previous.pid,process.pid]},null,2));
    return;
  }
  let reloading=false;
  try {
    phase='trusted task connect';
    const config=vscode.workspace.getConfiguration('vcp');
    await config.update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    await config.update('dataDirectory',fixture.data,vscode.ConfigurationTarget.Global);
    const connected=await vscode.commands.executeCommand('vcp.connectController',vscode.Uri.file(fixture.workspace).toString());
    assert.equal(connected.phase,'connected',JSON.stringify(connected));assert.equal(connected.role,'controller');assert.equal(connected.engineTrust,'trusted');
    let state=await taskState(api,value=>value.phase==='current'&&value.detail?.task.task===fixture.rootTask,'trusted task presentation');
    compareCli(state,fixture);
    await vscode.commands.executeCommand('vcp.tasks.focus');
    phase='stale owner approval';
    assert.equal(state.detail.questions[0].actionable,true,'freshness is explicitly separate from current process owner');
    const deny=state.actions.find(action=>action.label.startsWith('Deny'));assert(deny&&!deny.disabledReason);
    await Promise.all([api.dispatchTaskMessage({action:'task',id:deny.id}),api.dispatchTaskMessage({action:'task',id:deny.id})]);
    await api.dispatchTaskMessage({action:'refresh'});
    state=await taskState(api,value=>value.phase==='current'&&value.commands.some(row=>row.operation==='deny'),'stale decision journal');
    const stale=state.commands.filter(row=>row.operation==='deny');assert.equal(stale.length,1);assert.equal(stale[0].phase,'rejected');
    assert.equal(state.detail.questions[0].input.id,fixture.approval);
    phase='trusted child cancel';
    const selected=state.rows.find(row=>row.task===fixture.childTask);assert(selected);
    await api.dispatchTaskMessage({action:'select',id:selected.actionId});
    state=await taskState(api,value=>value.detail?.task.task===fixture.childTask&&value.phase==='current','trusted child selected');
    const cancel=state.actions.find(action=>action.label==='Cancel task');assert(cancel&&!cancel.disabledReason);
    await api.dispatchTaskMessage({action:'task',id:cancel.id,method:'task/cancel'});
    assert.equal(api.getTaskState().commands.filter(row=>row.operation==='cancel').length,0,'closed gateway rejects extra fields');
    await Promise.all([api.dispatchTaskMessage({action:'task',id:cancel.id}),api.dispatchTaskMessage({action:'task',id:cancel.id})]);
    state=await taskState(api,value=>value.phase==='current'&&value.rows.find(row=>row.task===fixture.childTask)?.state==='cancelled','cancelled child');
    const cancelled=state.commands.filter(row=>row.operation==='cancel');assert.equal(cancelled.length,1);assert.equal(cancelled[0].phase,'reconciled');
    await api.dispatchTaskMessage({action:'task',id:cancel.id});
    await api.dispatchTaskMessage({action:'refresh'});
    state=await taskState(api,value=>value.phase==='current'&&value.detail,'stale click refresh');
    assert.equal(state.commands.filter(row=>row.operation==='cancel').length,1);
    fs.writeFileSync(input.reloadMarker,JSON.stringify({pid:process.pid,cancelCommand:cancelled[0].commandId,staleCommand:stale[0].commandId}));
    phase='trusted task reload request';reloading=true;
    await vscode.commands.executeCommand('workbench.action.reloadWindow').catch(error=>{if(error?.name!=='Canceled')throw error;});
    await new Promise(()=>{});
  }finally{if(!reloading)await vscode.commands.executeCommand('vcp.disconnect');}
}

const run=exports.run;
exports.run=async function(){
  try{return await run();}
  catch(error){
    const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
    const failure={ok:false,phase,error:{name:String(error?.name??'Error'),message:String(error?.message??'qualification failed').slice(0,8192),stack:String(error?.stack??'').slice(0,16384)}};
    fs.writeFileSync(input.result,JSON.stringify(failure,null,2));
    fs.writeFileSync(path.join(path.dirname(input.stdout),'failure.json'),JSON.stringify(failure,null,2));
    throw error;
  }finally{restoreSdkCalls();}
};
