// SPDX-License-Identifier: Apache-2.0
// Copied into a separate qualification-only driver extension, never the VCP VSIX.
'use strict';
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict'), vscode=require('vscode');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.activate=()=>setImmediate(async()=>{
  const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
  let api;
  const wait=async fn=>{for(let attempt=0;attempt<600;attempt++){const value=fn();if(value)return value;await delay(50)}throw Error('Candidate observer deadline');};
  try {
    const normalize=value=>path.resolve(value).toLowerCase();
    assert.equal(typeof input.editorRuntime,'string');
    const allowed=new Set([input.editorRuntime,process.env.SystemRoot,path.join(process.env.SystemRoot,'System32')].map(normalize));
    assert(process.env.PATH.split(path.delimiter).every(entry=>entry && allowed.has(normalize(entry))));
    const extension=vscode.extensions.getExtension('iokaio.vcp');assert(extension);
    assert.equal(extension.packageJSON.version,input.version);
    assert(!normalize(extension.extensionPath).startsWith(normalize(input.checkout)+path.sep));
    api=await extension.activate();
    const config=vscode.workspace.getConfiguration('vcp');
    await config.update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    await config.update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
    await wait(()=>api.getConnectionState().phase!=='connecting');
    await vscode.commands.executeCommand('vcp.connect',vscode.Uri.file(input.workspace).toString());
    const state=await wait(()=>api.getConnectionState().phase==='connected' && api.getConnectionState());
    assert.equal(state.role,'observer');
    const view=await wait(()=>api.getTaskState()?.phase==='current' && api.getTaskState());
    const paused=view.rows.filter(row=>row.state==='paused');assert.equal(paused.length,1);
    await vscode.commands.executeCommand('vcp.disconnect');
    fs.writeFileSync(input.result,JSON.stringify({status:'pass',observer:true,task:paused[0].task,version:extension.packageJSON.version,developmentPathAbsent:true,model_calls:0}));
  } catch(error) {
    fs.writeFileSync(input.result,JSON.stringify({status:'fail',error:String(error.stack),connection:api?.getConnectionState?.()}));
  } finally {void vscode.commands.executeCommand('workbench.action.quit');}
});
