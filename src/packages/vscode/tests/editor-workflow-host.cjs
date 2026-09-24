// SPDX-License-Identifier: Apache-2.0
// Real extension, SDK, engine, and editor. UI answers are deterministic; timing
// hooks issue real typing/undo/reload commands without forging any operation result.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module');const {createHash}=require('node:crypto');const vscode=require('vscode');
const hash=text=>createHash('sha256').update(text).digest('hex');const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let phase='activate',reloading=false;
exports.run=async()=>{
  const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));const extension=vscode.extensions.getExtension('vcp.vcp-local');assert(extension);
  const target=createRequire(path.join(extension.extensionPath,'dist','extension.js'))('vscode');
  const restores=[];const replace=(object,key,value)=>{const old=object[key];object[key]=value;assert.equal(object[key],value);restores.push(()=>{object[key]=old;});};
  const renamed=[],deleted=[];
  const renameEvents=vscode.workspace.onDidRenameFiles(event=>renamed.push(...event.files.map(file=>file.oldUri.fsPath.toLowerCase())));
  const deleteEvents=vscode.workspace.onDidDeleteFiles(event=>deleted.push(...event.files.map(uri=>uri.fsPath.toLowerCase())));
  const calls=[],errors=[],prompts=[],coverage={};let api,current,latest,partialB,interrupt=false;const views=[];
  const wait=async(predicate,label)=>{for(let i=0;i<200;i++){const value=predicate();if(value)return value;await delay(50);}throw Error(`deadline: ${label}`);};
  const loaded=name=>{const expected=fs.realpathSync(path.join(extension.extensionPath,'dist',name)).toLowerCase();const entry=Object.values(require.cache).find(entry=>entry.filename&&path.basename(entry.filename)===name&&fs.realpathSync(entry.filename).toLowerCase()===expected);assert(entry,`loaded ${name}`);return entry.exports;};
  try{
    replace(target.window,'showQuickPick',async(items,options)=>{
      prompts.push(options?.title??'quickPick');const rows=await items;if(options?.title==='Start an execution-backed VCP task')return rows.find(row=>row.folder.uri.fsPath.toLowerCase()===input.workspace.toLowerCase());
      if(options?.title==='Review editor drafts')return rows;
      if(options?.title==='Apply reviewed editor changes')return rows.at(-1);
      return undefined;
    });
    replace(target.window,'showOpenDialog',async()=>[vscode.Uri.file(input.profile)]);
    const answers={'Task objective':'Qualify editor versioned buffer changes','Configured task cost cap (USD)':'1','Configured maximum requests':'8','Configured task deadline (seconds)':'300','Provider credential':'synthetic-cli-qualification'};
    replace(target.window,'showInputBox',async options=>{assert(Object.hasOwn(answers,options.title));return answers[options.title];});
    replace(target.window,'showWarningMessage',async(message,options,...choices)=>choices.includes('Apply')?'Apply':undefined);
    replace(target.window,'showInformationMessage',async()=>undefined);
    replace(target.window,'showErrorMessage',async message=>{errors.push(message);});
    api=await extension.activate();
    const prototype=loaded('engine_connection.js').EngineConnection.prototype,original=prototype.currentClient;const wrapped=new Set();
    replace(prototype,'currentClient',function(){const client=original.call(this);if(client&&!wrapped.has(client)){wrapped.add(client);const call=client.call;replace(client,'call',async function(method,params,...rest){let reply;try{reply=await call.call(this,method,params,...rest);}catch(error){calls.push({method,error:{code:error.code,message:String(error.message).slice(0,1000),classification:error.classification}});throw error;}calls.push({method,params,reply});
      if(method==='editor/prepare'){latest=reply.value;views.push(latest);}
      if(method==='editor/changeResult'&&partialB&&params.file===0){const document=partialB;partialB=undefined;const editor=await vscode.window.showTextDocument(document,{preview:false});editor.selection=new vscode.Selection(0,0,0,0);await vscode.commands.executeCommand('type',{text:'human '});}
      return reply;});}if(client)current=client;return client;});
    const bufferPrototype=loaded('editor_buffers.js').EditorBuffers.prototype,apply=bufferPrototype.apply;
    replace(bufferPrototype,'apply',async function(editor,...rest){const result=await apply.call(this,editor,...rest);if(interrupt&&result.outcome==='applied'){
      interrupt=false;assert.equal(editor.document.getText(),'44\n');await vscode.window.showTextDocument(editor.document,{preview:false});await vscode.commands.executeCommand('undo');
      assert(editor.document.isDirty);assert.equal(editor.document.getText(),'43\n');
      const change=latest;fs.writeFileSync(input.marker,JSON.stringify({pid:process.pid,change:change.change,task:change.task,scope:change.scope,uri:editor.document.uri.toString(),buffer:hash(editor.document.getText()),dirty:editor.document.isDirty,disk:hash(fs.readFileSync(editor.document.uri.fsPath)),changes:views.map(view=>view.change),coverage}));
      phase='reload after apply and undo before receipt';reloading=true;await vscode.commands.executeCommand('workbench.action.reloadWindow').catch(error=>{if(error?.name!=='Canceled')throw error;});await new Promise(()=>{});
    }return result;});
    if(fs.existsSync(input.marker)){
      phase='observer reload';const marker=JSON.parse(fs.readFileSync(input.marker,'utf8'));assert.notEqual(marker.pid,process.pid);
      await wait(()=>api.getConnectionState().phase==='connected','restored observer');assert.equal(api.getConnectionState().role,'observer');
      await wait(()=>current,'restored actual client');const view=(await current.call('editor/changeRead',{scope:marker.scope,task:marker.task,change:marker.change})).value;
      assert(['dispatched','unknown'].includes(view.files[0].state));assert.equal(view.buffers_unverified,true);
      const document=await vscode.workspace.openTextDocument(vscode.Uri.parse(marker.uri));await vscode.window.showTextDocument(document,{preview:false});await wait(()=>hash(document.getText())===marker.buffer,'restored dirty buffer after reload');assert.equal(hash(document.getText()),marker.buffer);assert.equal(hash(fs.readFileSync(document.uri.fsPath)),marker.disk);
      await vscode.commands.executeCommand('vcp.inspectEditorChanges');assert(!calls.some(row=>row.method==='editor/dispatch'||row.method==='editor/changeResult'));
      for(const document of vscode.workspace.textDocuments.filter(document=>document.uri.scheme==='file'&&document.isDirty)){await vscode.window.showTextDocument(document,{preview:false});await vscode.commands.executeCommand('workbench.action.files.revert');}
      await vscode.commands.executeCommand('vcp.disconnect');
      fs.writeFileSync(input.result,JSON.stringify({...marker.coverage,ok:true,version:vscode.version,installed:true,extensionPath:extension.extensionPath,reload:true,undoBeforeReceipt:true,noReplay:true,unknown:view.files[0].state,restoredBufferSha256:marker.buffer,diskSha256:marker.disk,dirtyBeforeReload:marker.dirty,changes:marker.changes,interrupted:marker.change,task:marker.task,scope:marker.scope,hostPids:[marker.pid,process.pid]},null,2));return;
    }
    phase='trusted task start';assert.equal(vscode.workspace.isTrusted,true);
    await target.workspace.getConfiguration('vcp').update('engineExecutable',input.executable,vscode.ConfigurationTarget.Global);
    await target.workspace.getConfiguration('vcp').update('dataDirectory',input.data,vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.startEditorTask');assert.equal(errors.length,0,JSON.stringify(errors));
    assert(calls.some(row=>row.method==='turn/start'),'explicit start command reached engine');const running=await wait(()=>api.getTaskState()?.rows.find(row=>row.state==='running'),'running execution-backed task');await api.dispatchTaskMessage({action:'select',id:running.actionId});await wait(()=>api.getTaskState()?.detail?.task.task===running.task,'selected running task');assert.equal(api.getConnectionState().role,'controller');
    const open=async file=>{const uri=vscode.Uri.file(file);await vscode.commands.executeCommand('vscode.open',uri,{preview:false});return (await wait(()=>vscode.window.activeTextEditor?.document.uri.toString()===uri.toString()?vscode.window.activeTextEditor:undefined,'opened source editor')).document;};
    const draft=async(document,text)=>{await vscode.window.showTextDocument(document,{preview:false});await wait(()=>api.getTaskState()?.phase==='current'&&api.getTaskState()?.detail?.task.task===running.task,'current selected task for draft');await vscode.commands.executeCommand('vcp.createEditDraft');const editor=vscode.window.activeTextEditor;assert(editor.document.isUntitled);assert.equal(await editor.edit(builder=>builder.replace(new vscode.Range(editor.document.positionAt(0),editor.document.positionAt(editor.document.getText().length)),text)),true);return editor.document;};
    const prepare=async()=>{const count=views.length;await vscode.commands.executeCommand('vcp.reviewEditorDrafts');assert.equal(views.length,count+1,JSON.stringify(errors));return latest;};
    phase='outside selected root';const outside=await open(input.outside);await vscode.window.showTextDocument(outside,{preview:false});const before=calls.length;await vscode.commands.executeCommand('vcp.createEditDraft');assert.equal(calls.slice(before).some(row=>row.method==='editor/context'),false);assert(errors.length>0);coverage.rootMismatch=true;errors.length=0;
    const discardDraft=async document=>{if(!document.isClosed){await vscode.window.showTextDocument(document,{preview:false});await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');}};
    for(const action of ['reopen','rename','delete']){
      phase=`actual source ${action}`;const file=path.join(input.workspace,`${action}.txt`);fs.writeFileSync(file,'source\n');const source=await open(file);const oldDraft=await draft(source,'candidate\n');
      const before=calls.filter(row=>row.method==='editor/dispatch').length;
      if(action==='reopen'){
        await vscode.window.showTextDocument(source,{preview:false});await vscode.commands.executeCommand('workbench.action.closeActiveEditor');await wait(()=>source.isClosed,'source really closed');
        const reopened=await open(file);assert.notEqual(reopened,source);assert.equal(reopened.getText(),'source\n');
      }else if(action==='rename'){
        const edit=new vscode.WorkspaceEdit();edit.renameFile(vscode.Uri.file(file),vscode.Uri.file(`${file}.renamed`));assert.equal(await vscode.workspace.applyEdit(edit),true);await wait(()=>renamed.includes(file.toLowerCase()),'actual rename event');
      }else{const edit=new vscode.WorkspaceEdit();edit.deleteFile(vscode.Uri.file(file));assert.equal(await vscode.workspace.applyEdit(edit),true);await wait(()=>deleted.includes(file.toLowerCase()),'actual delete event');}
      await vscode.commands.executeCommand('vcp.reviewEditorDrafts');assert.equal(calls.filter(row=>row.method==='editor/dispatch').length,before);assert.equal(views.length,0);
      if(action==='delete')assert.equal(fs.existsSync(file),false);else assert.equal(fs.readFileSync(action==='rename'?`${file}.renamed`:file,'utf8'),'source\n');
      await discardDraft(oldDraft);errors.length=0;coverage[action]=true;
    }
    phase='positive receipt and independent disk';let a=await open(path.join(input.workspace,'value.txt'));
    await draft(a,'42\n');const positive=await prepare();await vscode.commands.executeCommand('vcp.applyEditorChanges');assert.equal(errors.length,0,JSON.stringify(errors));
    let view=(await current.call('editor/changeRead',{scope:positive.scope,task:positive.task,change:positive.change})).value;assert.equal(view.files[0].state,'applied');assert.equal(view.files[0].observed_sha256,hash(a.getText()));assert.equal(a.getText(),'42\n');assert.equal(fs.readFileSync(a.uri.fsPath,'utf8'),'41\n');assert(a.isDirty);
    const saveCallIndex=calls.length;await a.save();const saved=await wait(()=>calls.slice(saveCallIndex).findLast(row=>row.method==='editor/context'&&row.reply&&row.params.documents?.some(document=>document.uri===a.uri.toString()&&!document.dirty)),'saved observation');
    const savedId=saved.reply.value.observations.find(observation=>observation.document.uri===a.uri.toString()).id;
    await vscode.window.showTextDocument(a,{preview:false});await vscode.commands.executeCommand('workbench.action.closeActiveEditor');await wait(()=>a.isClosed,'saved source really closed');await wait(()=>calls.some(row=>row.method==='editor/context'&&row.reply&&row.params.closed?.includes(savedId)),'exact saved observation retirement');coverage.appliedReceipt=true;coverage.saveClose=true;
    phase='stale preview';a=await open(path.join(input.workspace,'value.txt'));await draft(a,'43\n');const stale=await prepare();const sourceEditor=await vscode.window.showTextDocument(a,{preview:false});await wait(()=>vscode.window.activeTextEditor?.document===a,'source editor focused for typing');sourceEditor.selection=new vscode.Selection(0,0,0,0);const beforeTypingVersion=a.version,typedSource='human '+a.getText();await vscode.commands.executeCommand('type',{text:'human '});await wait(()=>a.version>beforeTypingVersion&&a.getText()===typedSource,'typed source observed before stale apply');const dispatched=calls.filter(row=>row.method==='editor/dispatch').length;await vscode.commands.executeCommand('vcp.applyEditorChanges');assert.equal(calls.filter(row=>row.method==='editor/dispatch').length,dispatched);assert.equal(a.getText(),typedSource);coverage.stalePreview=true;errors.length=0;await vscode.commands.executeCommand('workbench.action.files.revert');
    phase='per-file partial';const b=await open(path.join(input.workspace,'second.txt'));await draft(a,'43\n');await draft(b,'changed second\n');const partial=await prepare();partialB=b;await vscode.commands.executeCommand('vcp.applyEditorChanges');view=(await current.call('editor/changeRead',{scope:partial.scope,task:partial.task,change:partial.change})).value;assert.equal(view.files[0].state,'applied');assert(['prepared','rejected'].includes(view.files[1].state));assert.equal(view.files[1].execution,null);assert.equal(a.getText(),'43\n');assert.equal(b.getText(),'human second\n');await vscode.commands.executeCommand('vcp.refreshEditorObservations');view=(await current.call('editor/changeRead',{scope:partial.scope,task:partial.task,change:partial.change})).value;assert.equal(view.files[0].state,'applied');assert.equal(view.files[1].state,'rejected',JSON.stringify(view.files));assert.equal(view.files[1].execution,null);coverage.partialFiles=true;errors.length=0;
    phase='interrupted final receipt';await draft(a,'44\n');await prepare();interrupt=true;await vscode.commands.executeCommand('vcp.applyEditorChanges');throw Error('reload should terminate old extension host');
  }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,error:{message:String(error.message),stack:String(error.stack)},errors,prompts,methods:calls.map(row=>row.error?{method:row.method,error:row.error}:row.method),connection:api?.getConnectionState(),tasks:api?.getTaskState()},null,2));throw error;}
  finally{if(!reloading){renameEvents.dispose();deleteEvents.dispose();for(const restore of restores.reverse())restore();}}
};
