// SPDX-License-Identifier: Apache-2.0
// Actual VS Code primitive qualification; no engine or mocked editor responses.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const crypto=require('node:crypto');
const vscode=require('vscode');
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
let phase='startup';
exports.run=async()=>{
  const input=JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT,'utf8'));
  const evidence={version:vscode.version,node:process.versions.node,files:[],events:[]};
  const listen=vscode.workspace.onDidChangeTextDocument(event=>{
    const index=input.files.findIndex(file=>file.toLowerCase()===event.document.uri.fsPath.toLowerCase());
    if(index>=0)evidence.events.push({file:index,version:event.document.version,reason:event.reason??null,dirty:event.document.isDirty});
  });
  try{
    assert.equal(vscode.version,'1.138.0');
    const [a,b]=await Promise.all(input.files.map(file=>vscode.workspace.openTextDocument(vscode.Uri.file(file))));
    const original='base α😀\r\nsecond\r\n';
    for(const document of [a,b]){
      assert.equal(document.getText(),original);assert.equal(document.eol,vscode.EndOfLine.CRLF);
      const disk=fs.readFileSync(document.uri.fsPath);assert.equal(disk.subarray(0,3).toString('hex'),'efbbbf');
      evidence.files.push({beforeVersion:document.version,beforeBuffer:hash(document.getText()),beforeDisk:hash(disk)});
    }
    let editor=await vscode.window.showTextDocument(a,{preview:false});
    editor.selection=new vscode.Selection(0,0,0,0);
    phase='typing during review';
    const reviewed=a.version;
    await vscode.commands.executeCommand('type',{text:'review-human '});
    assert(a.version>reviewed);assert(a.isDirty);assert(a.getText().startsWith('review-human '));
    let invoked=false;
    const staleApply=()=>{if(a.version!==reviewed)return false;invoked=true;return editor.edit(edit=>edit.insert(new vscode.Position(0,0),'must-not-apply'));};
    assert.equal(staleApply(),false);assert.equal(invoked,false);
    assert.equal(hash(fs.readFileSync(input.files[0])),evidence.files[0].beforeDisk);
    await vscode.commands.executeCommand('workbench.action.files.revert');assert.equal(a.getText(),original);
    phase='first file applies';
    const candidate='prepared α😀\r\nsecond\r\n';
    const beforeApply=a.version;
    assert.equal(await editor.edit(edit=>edit.replace(new vscode.Range(a.positionAt(0),a.positionAt(a.getText().length)),candidate)),true);
    assert(a.version>beforeApply);assert.equal(a.getText(),candidate);assert(a.isDirty);
    assert.equal(hash(fs.readFileSync(input.files[0])),evidence.files[0].beforeDisk);
    evidence.files[0].applied={beforeVersion:beforeApply,afterVersion:a.version,buffer:hash(a.getText()),disk:hash(fs.readFileSync(input.files[0])),dirty:a.isDirty};
    phase='renderer typing race';
    editor=await vscode.window.showTextDocument(b,{preview:false});editor.selection=new vscode.Selection(0,0,0,0);
    const expected=b.version;let typing;let callbackVersion;
    const outcome=editor.edit(edit=>{
      callbackVersion=b.version;assert.equal(callbackVersion,expected);
      // The builder already captured expected. Enqueue actual renderer typing
      // before the prepared edit's RPC, without letting the host observe it first.
      typing=vscode.commands.executeCommand('type',{text:'racing-human '});
      edit.replace(new vscode.Range(b.positionAt(0),b.positionAt(b.getText().length)),'must-not-apply\r\n');
    });
    const [applied]=await Promise.all([outcome,typing]);
    assert.equal(applied,false,'renderer must reject the captured stale document version');
    assert.equal(b.getText(),'racing-human '+original);assert(b.version>expected);assert(b.isDirty);
    assert.equal(hash(fs.readFileSync(input.files[1])),evidence.files[1].beforeDisk);
    assert.equal(a.getText(),candidate,'partial application preserves the successful first file');
    evidence.files[1].conflict={expected,callbackVersion,afterVersion:b.version,applied,buffer:hash(b.getText()),disk:hash(fs.readFileSync(input.files[1])),dirty:b.isDirty};
    phase='undo redo and save';
    editor=await vscode.window.showTextDocument(a,{preview:false});
    const appliedVersion=a.version;
    await vscode.commands.executeCommand('undo');assert.equal(a.getText(),original);assert(a.version>appliedVersion);
    evidence.files[0].undo={version:a.version,buffer:hash(a.getText())};
    assert.notEqual(a.version,beforeApply,'same content does not restore the old version');
    await vscode.commands.executeCommand('redo');assert.equal(a.getText(),candidate);
    evidence.files[0].redo={version:a.version,buffer:hash(a.getText())};
    assert.equal(await a.save(),true);assert.equal(a.isDirty,false);
    const saved=fs.readFileSync(input.files[0]);assert.deepEqual(saved,Buffer.concat([Buffer.from([0xef,0xbb,0xbf]),Buffer.from(candidate,'utf8')]));
    assert.notEqual(hash(saved),evidence.files[0].beforeDisk);
    evidence.files[0].saved={version:a.version,buffer:hash(a.getText()),disk:hash(saved),dirty:a.isDirty,bom:true,eol:'CRLF'};
    await vscode.commands.executeCommand('undo');assert.equal(a.getText(),original);assert(a.isDirty);
    assert.equal(hash(fs.readFileSync(input.files[0])),hash(saved),'undo changes the buffer, not saved disk');
    evidence.files[0].undoAfterSave={version:a.version,buffer:hash(a.getText()),disk:hash(fs.readFileSync(input.files[0])),dirty:a.isDirty};
    assert(evidence.events.some(event=>event.reason===vscode.TextDocumentChangeReason.Undo));
    assert(evidence.events.some(event=>event.reason===vscode.TextDocumentChangeReason.Redo));
    for(const document of [a,b]){await vscode.window.showTextDocument(document,{preview:false});await vscode.commands.executeCommand('workbench.action.files.revert');assert.equal(document.isDirty,false);}
    evidence.ok=true;evidence.typingRace=true;evidence.partial=true;evidence.undoRedo=true;evidence.bomCrlf=true;
    fs.writeFileSync(input.result,JSON.stringify(evidence,null,2));
  }catch(error){fs.writeFileSync(input.result,JSON.stringify({ok:false,phase,error:{message:String(error.message),stack:String(error.stack)},evidence},null,2));throw error;}
  finally{listen.dispose();}
};
