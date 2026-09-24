// SPDX-License-Identifier: Apache-2.0
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const Module=require('node:module');
class Range{constructor(sl,sc,el,ec){this.start={line:sl,character:sc};this.end={line:el,character:ec};}}
const languages={getDiagnostics:()=>[]};
const load=Module._load;
let EditorBuffers;
try{Module._load=function(name,...args){if(name==='vscode')return{Range,languages};return load.call(this,name,...args);};({EditorBuffers}=require('../dist/editor_buffers.js'));}finally{Module._load=load;}
const zero={line:0,character:0};
const insert=text=>[{range:{start:zero,end:zero},text}];
function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-buffer-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const file=path.join(root,'same.txt');fs.writeFileSync(file,'base\r\n');
  const document={uri:{scheme:'file',authority:'',fsPath:file,toString:()=>`file://${file}`},version:1,languageId:'plaintext',eol:2,isDirty:false,isClosed:false,text:'base\r\n',getText(){return this.text;}};
  const adapter=new EditorBuffers();
  const binding={host:'host',rootId:'root',rootPath:root,bindingRevision:'3',generation:4,trusted:true};
  const state={calls:0,options:undefined,before:undefined,after:undefined,returnFalse:false,throwAfter:false,throwBefore:false};
  const position=(position)=>document.text.split('\r\n').slice(0,position.line).reduce((n,line)=>n+line.length+2,0)+position.character;
  const editor={document,selections:[new Range(0,0,0,0)],edit(callback,options){
    state.calls++;state.options=options;if(state.throwBefore)throw Error('lost transport');state.before?.();
    const edits=[];callback({replace:(range,text)=>edits.push({range,text})});
    if(state.returnFalse)return Promise.resolve(false);
    for(const edit of edits.reverse()){const start=position(edit.range.start),end=position(edit.range.end);document.text=document.text.slice(0,start)+edit.text+document.text.slice(end);}
    document.version++;document.isDirty=true;adapter.invalidate(document);state.after?.();
    if(state.throwAfter)return Promise.reject(Error('lost reply'));return Promise.resolve(true);
  }};
  return {adapter,editor,document,binding,state,capture:()=>adapter.capture(editor,binding),apply:(expected,edits=insert('new\n'))=>adapter.apply(editor,expected,edits,()=>binding)};
}
test('capture binds open identity, canonical root and logical UTF8 independently from disk',t=>{
  const f=fixture(t);const first=f.capture(),second=f.capture();assert.equal(first.document.open_id,second.document.open_id);
  assert.equal(first.document.relative_path,'same.txt');assert.equal(first.document.eol,'crlf');assert.equal(first.document.encoding,'unknown');assert.equal(first.document.disk_sha256,null);assert.equal(first.document.capture,false);
  assert.equal(f.adapter.capture(f.editor,f.binding,false).document.content,null);assert(Object.isFrozen(first.document));assert.doesNotThrow(()=>JSON.stringify(first));
  f.editor.document={...f.document};assert.notEqual(f.capture().document.open_id,first.document.open_id);
});
test('successful application reports actual before/after with undo stops and normalized CRLF',async t=>{
  const f=fixture(t),expected=f.capture();const result=await f.apply(expected);assert.equal(result.outcome,'applied');assert.equal(result.after.document.content,'new\r\nbase\r\n');assert.equal(result.before.document.version,'1');assert.equal(result.after.document.version,'2');assert.equal(result.after.document.dirty,true);assert(result.after.epoch>expected.epoch);assert.deepEqual(f.state.options,{undoStopBefore:true,undoStopAfter:true});assert.equal(fs.readFileSync(f.document.uri.fsPath,'utf8'),'base\r\n');
});
test('typing and same-version save invalidation reject before dispatch',async t=>{
  for(const mutate of [f=>{f.document.text='human';f.document.version++;},f=>f.adapter.invalidate(f.document)]){
    const f=fixture(t),expected=f.capture();mutate(f);assert.equal((await f.apply(expected)).outcome,'rejected');assert.equal(f.state.calls,0);
  }
});
test('reopened document at same URI/version cannot reuse the original open ID',async t=>{
  const f=fixture(t),expected=f.capture();f.editor.document={...f.document};assert.equal((await f.apply(expected)).outcome,'rejected');assert.equal(f.state.calls,0);
});
test('binding generation, revision, root and trust changes reject stale intent',async t=>{
  for(const patch of [{generation:5},{bindingRevision:'4'},{rootId:'other'},{host:'other'},{trusted:false}]){
    const f=fixture(t),expected=f.capture();Object.assign(f.binding,patch);assert.equal((await f.apply(expected)).outcome,'rejected');assert.equal(f.state.calls,0);
  }
});
test('callback synchronously rechecks state after builder captures its version',async t=>{
  const f=fixture(t),expected=f.capture();f.state.before=()=>{f.document.version++;f.document.text='typing';};const result=await f.apply(expected);assert.equal(result.outcome,'rejected');assert.equal(f.document.text,'typing');
});
test('native version rejection observes current human text',async t=>{
  const f=fixture(t),expected=f.capture();f.state.returnFalse=true;const result=await f.apply(expected);assert.equal(result.outcome,'rejected');assert.equal(result.after.document.content,'base\r\n');
});
test('lost edit response never reports rejection or authorizes replay',async t=>{
  const f=fixture(t),expected=f.capture();f.state.throwAfter=true;const result=await f.apply(expected);assert.equal(result.outcome,'unknown');assert.equal(result.after.document.content,'new\r\nbase\r\n');assert.equal(f.state.calls,1);
});
test('editor throw without a known validation rejection is unknown',async t=>{
  const f=fixture(t),expected=f.capture();f.state.throwBefore=true;assert.equal((await f.apply(expected)).outcome,'unknown');
});
test('typing or undo/redo before receipt observation remains unknown',async t=>{
  for(const mutate of [f=>{f.document.text='human';f.document.version++;},f=>{f.document.version+=2;}]){
    const f=fixture(t),expected=f.capture();f.state.after=()=>mutate(f);assert.equal((await f.apply(expected)).outcome,'unknown');
  }
});
test('trust revocation after successful application leaves outcome unknown',async t=>{
  const f=fixture(t),expected=f.capture();f.state.after=()=>{f.binding.trusted=false;};const result=await f.apply(expected);assert.equal(result.outcome,'unknown');assert.equal(result.after,undefined);
});
test('deleted, closed and remote documents cannot be captured',t=>{
  for(const mutate of [f=>fs.unlinkSync(f.document.uri.fsPath),f=>{f.document.isClosed=true;},f=>{f.document.uri.scheme='vscode-remote';},f=>{f.document.uri.query='alternate';},f=>{f.document.uri.fragment='alternate';}]){const f=fixture(t);mutate(f);assert.throws(()=>f.capture());}
});
test('documents outside the selected root and disposed adapters fail closed',t=>{
  const f=fixture(t);fs.mkdirSync(path.join(f.binding.rootPath,'nested'));f.binding.rootPath=path.join(f.binding.rootPath,'nested');assert.throws(()=>f.capture());f.adapter.dispose();assert.throws(()=>f.capture());
});
test('bounds, malformed and overlapping UTF16 edits reject before invoking editor',async t=>{
  const bad=[[],[{range:{start:{line:9,character:0},end:{line:9,character:0}},text:'x'}],[...insert('x'),...insert('y')],insert('x'.repeat(65537))];
  for(const edits of bad){const f=fixture(t);assert.equal((await f.apply(f.capture(),edits)).outcome,'rejected');assert.equal(f.state.calls,0);}
  const f=fixture(t);f.document.text='😀';const expected=f.capture();assert.equal((await f.apply(expected,[{range:{start:{line:0,character:1},end:{line:0,character:1}},text:'x'}])).outcome,'rejected');
});
test('capture does not truncate oversized content into an authoritative observation',t=>{
  const f=fixture(t);f.document.text='é'.repeat(32769);assert.throws(()=>f.capture());
});
test('unpaired surrogates cannot masquerade as authoritative UTF8 text',async t=>{
  const f=fixture(t);assert.equal((await f.apply(f.capture(),insert('\ud800'))).outcome,'rejected');f.document.text='\udc00';assert.throws(()=>f.capture());
});
test('successful buffer observation reports actual clean state after external auto-save',async t=>{
  const f=fixture(t);f.state.after=()=>{f.document.isDirty=false;};const result=await f.apply(f.capture());assert.equal(result.outcome,'applied');assert.equal(result.after.document.dirty,false);assert(!result.reason.includes('unsaved'));
});
test('native Position getters normalize to wire UTF16 fields without private properties',t=>{
  const f=fixture(t);class Position{constructor(line,character){this._line=line;this._character=character;}get line(){return this._line;}get character(){return this._character;}}
  f.editor.selections=[{start:new Position(0,1),end:new Position(0,3)}];assert.deepEqual(f.capture().document.selections,[{start:{line:0,character:1},end:{line:0,character:3}}]);
});
test('diagnostics retain bounded provenance and hash but no messages or claimed producer revision',t=>{
  const f=fixture(t);const diagnostic={range:new Range(0,0,0,1),severity:1,source:'test-producer',code:{value:'E1'},message:'private diagnostic message'};
  languages.getDiagnostics=()=>[diagnostic];t.after(()=>{languages.getDiagnostics=()=>[];});
  const first=f.adapter.capture({document:f.document,selections:[]},f.binding).document.diagnostics;
  assert.equal(first.count,1);assert.equal(first.truncated,false);assert.equal(first.producer_document_version,null);assert.equal(first.observed_document_version,'1');assert.match(first.sha256,/^[0-9a-f]{64}$/);assert(!JSON.stringify(first).includes(diagnostic.message));
  assert.equal(f.capture().document.diagnostics.sha256,first.sha256);
  diagnostic.message='changed';assert.notEqual(f.capture().document.diagnostics.sha256,first.sha256);
});
test('diagnostics enforce both sample count and total UTF8 bytes without persisting raw text',t=>{
  const f=fixture(t);const row={range:new Range(0,0,0,1),severity:1,message:'x'};
  languages.getDiagnostics=()=>Array(65).fill(row);t.after(()=>{languages.getDiagnostics=()=>[];});
  let captured=f.capture().document.diagnostics;assert.equal(captured.count,64);assert.equal(captured.truncated,true);
  languages.getDiagnostics=()=>[{...row,message:'é'.repeat(32768)}];captured=f.capture().document.diagnostics;assert.equal(captured.count,0);assert.equal(captured.truncated,true);
  languages.getDiagnostics=()=>{throw Error('producer unavailable');};assert.equal(f.capture().document.diagnostics,null);
});
test('diagnostic order does not change digest or falsely advance document version',t=>{
  const f=fixture(t);const rows=[{range:new Range(0,0,0,1),severity:1,message:'b'},{range:new Range(0,1,0,2),severity:2,message:'a'}];
  languages.getDiagnostics=()=>rows;t.after(()=>{languages.getDiagnostics=()=>[];});const before=f.capture();rows.reverse();f.adapter.invalidate(f.document);const after=f.capture();assert.equal(before.document.diagnostics.sha256,after.document.diagnostics.sha256);assert.equal(before.document.version,after.document.version);assert(after.epoch>before.epoch);
});
