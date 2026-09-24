// SPDX-License-Identifier: Apache-2.0
// Loaded by the actual VS Code extension host, not a mocked API.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL}=require('node:url');
const vscode = require('vscode');
let phase='activation';

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
      assert.equal(connected.taskCount,1);
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
    const result={trustVerified:true,movedVerified:true,ok:true,version:vscode.version,node:process.versions.node,extensionPath:extension.extensionPath,sdkPath,trustEnabled,actualTrusted:vscode.workspace.isTrusted,trustEvidence:vscode.workspace.isTrusted?'trusted development host; restricted mode not observed':'actual restricted workspace',remoteName:vscode.env.remoteName??null,statuses};
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

const run=exports.run;
exports.run=async function(){
  try{return await run();}
  catch(error){
    const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
    const failure={ok:false,phase,error:{name:String(error?.name??'Error'),message:String(error?.message??'qualification failed').slice(0,8192),stack:String(error?.stack??'').slice(0,16384)}};
    fs.writeFileSync(input.result,JSON.stringify(failure,null,2));
    fs.writeFileSync(path.join(path.dirname(input.stdout),'failure.json'),JSON.stringify(failure,null,2));
    throw error;
  }
};
