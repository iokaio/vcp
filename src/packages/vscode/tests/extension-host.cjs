// SPDX-License-Identifier: Apache-2.0
// Loaded by the actual VS Code extension host, not a mocked API.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL}=require('node:url');
const vscode = require('vscode');

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
  const folders=vscode.workspace.workspaceFolders;assert.equal(folders.length,2);
  assert.equal(folders[0].name,folders[1].name,'duplicate display names are intentional');
  const statuses=[];
  try {
    await configuration.update('engineExecutable',undefined,vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.connect',folders[0].uri.toString());
    assert.equal(api.getConnectionState().phase,'unavailable','workspace executable cannot supply user authority');
    await configuration.update('engineExecutable',path.join(input.unselected,'missing-engine.exe'),vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.connect',folders[0].uri.toString());
    assert.equal(api.getConnectionState().phase,'unavailable','missing configured executable stays unavailable');
    await configuration.update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    for(const fixture of input.fixtures) {
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
    fs.writeFileSync(input.result,JSON.stringify({ok:true,version:vscode.version,node:process.versions.node,extensionPath:extension.extensionPath,sdkPath,trustEnabled,actualTrusted:vscode.workspace.isTrusted,trustEvidence:vscode.workspace.isTrusted?'trusted development host; restricted mode not observed':'actual restricted workspace',remoteName:vscode.env.remoteName??null,statuses},null,2));
  }finally{
    await vscode.commands.executeCommand('vcp.disconnect');
  }
};
