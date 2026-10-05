// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const {verify,encoding,read} = require('../../../scripts/evals/verify-invocation-stream.cjs');
const evaluator = path.resolve(__dirname,'../../../scripts/evals/verify-invocation-stream.cjs');
function frames(task,offset) {
  const scope={workspace:'w',session:'s',task}, command=`create-${task}`, finish=`finish-${task}`;
  const receipt=(id,start,end) => ({version:1,workspace:'w',command:id,first_event:String(offset+start),last_event:String(offset+end)});
  const base={schema_version:1,correlation:command,scope};
  const event=(seq,cause) => ({...base,type:'event',event:{sequence:String(offset+seq),event:{...scope,correlation:cause}}});
  return [{...base,type:'accepted',receipt:receipt(command,1,2)},event(1,command),event(2,command),event(3,finish),
    {...base,type:'result',exit_code:0,conditions:{completed:true,unresolved_effect:false},receipt:receipt(finish,3,3)}];
}
function bundle(scope) {
  const row=purpose => ({scope,turn:'turn',attempt:null,purpose,catalog:'a'.repeat(64),reasoning:'high',request_sha256:'b'.repeat(64),started_micros:1,elapsed_micros:10,
    work:{encode_calls:1,encode_failures:0,encoded_bytes:100,encode_micros:2,validation_calls:purpose==='sealed_validation'?1:0,validation_failures:0,validation_micros:purpose==='sealed_validation'?3:0}});
  return {schema_version:1,kind:'inspection_bundle',source_watermark:'20',task:{scope,state:'completed'},views:{},
    history:[{rows:[{event:{event:{...scope,id:'diagnostic',kind:'diagnostic'}},artifact_links:[]}],next_cursor:null}],
    retained_lifecycle_diagnostics:[{event:'diagnostic',watermark:'20',capture_boundary:'owner_drained',snapshot:{schema_version:1,available:true,
      owner:`owner-${scope.task}`,window:'current_owner_only',complete_history:false,snapshot_micros:20,capacity:256,dropped:0,observations:[],
      encoding_available:true,encodings_dropped:0,encodings:[row('final_assembly'),row('sealed_validation')]}}]};
}
test('joins distinct same-session tasks and both full receipt ranges, allowing authorized taskless events', () => {
  const a=frames('first',0),b=frames('second',10);
  b.splice(1,0,{...b[1],scope:null,event:{sequence:'10',event:{workspace:'w',session:'s',task:null,correlation:'policy'}}});
  const report=verify(a,b);
  assert.equal(report.second_event_count,4);
  assert.equal(report.second_first_sequence,'10');
  assert.equal(report.final_receipt.command,'finish-second');
  b[1].event.event.task='first';
  assert.equal(verify(a,b).second_first_sequence,'10','a new authorized observation about the prior task is not replay');
});
test('rejects old-prefix flooding, duplicate or out-of-order delivery', () => {
  for (const change of [
    b=>b.splice(1,0,frames('first',0)[1]),
    b=>b.splice(2,0,structuredClone(b[1])),
    b=>[b[1],b[2]]=[b[2],b[1]],
  ]) { const b=frames('second',10);change(b);assert.throws(()=>verify(frames('first',0),b)); }
});
test('rejects incomplete receipts, foreign session, reused task and noncompleted or gapped framing', () => {
  for (const change of [
    b=>b.splice(2,1), b=>b[3].event.event.correlation='wrong',
    b=>b[0].scope.session='foreign', b=>b.at(-1).conditions.completed=false,
    b=>b.splice(1,0,{...b[0],type:'cursor_gap'}), b=>b.pop(),
    b=>b[0].receipt.last_event='18446744073709551615',
  ]) { const b=frames('second',10);change(b);assert.throws(()=>verify(frames('first',0),b)); }
  assert.throws(()=>verify(frames('first',0),frames('first',10)));
});
test('keeps u64 sequences exact above JavaScript safe integer range', () => {
  const a=frames('first',0), b=frames('second',10);
  for (const list of [a,b]) for (const frame of list) {
    if(frame.event) frame.event.sequence=String(BigInt(frame.event.sequence)+9007199254740993n);
    if(frame.receipt) for(const key of ['first_event','last_event']) frame.receipt[key]=String(BigInt(frame.receipt[key])+9007199254740993n);
  }
  assert.equal(verify(a,b).second_first_sequence,'9007199254741004');
});
test('rejects a dropped interior event even when both receipt ranges remain complete', () => {
  const b=frames('second',10);
  b[3].event.sequence='14';
  b.at(-1).receipt.first_event='14';b.at(-1).receipt.last_event='14';
  assert.throws(()=>verify(frames('first',0),b),/Missing interior/);
  b.splice(3,0,{...b[1],event:{sequence:'13',event:{workspace:'w',session:'s',task:'second',correlation:'diagnostic'}}});
  assert.equal(verify(frames('first',0),b).second_event_count,4);
});
test('requires actual retained owner encoding work and keeps overlapping measures separate', () => {
  const scope={workspace:'w',session:'s',task:'second'}, input=bundle(scope);
  const result=encoding(input,scope);
  assert.equal(result.length,1);assert.equal(result[0].groups.length,2);assert.equal(result[0].partial_window,true);
  const legacy=structuredClone(input);delete legacy.retained_lifecycle_diagnostics[0].snapshot.encoding_available;
  delete legacy.retained_lifecycle_diagnostics[0].snapshot.encodings;
  assert.throws(()=>encoding(legacy,scope),/Missing retained/);
  assert.throws(()=>encoding(input,{...scope,task:'foreign'}),/scope/);
});
test('real evaluator CLI reads completed files, binds hashes, refuses overwrite and partial tails', t => {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-invocation-stream-'));
  t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const a=frames('first',0),b=frames('second',10);
  const files=['first.jsonl','second.jsonl','first.json','second.json'].map(file=>path.join(dir,file));
  fs.writeFileSync(files[0],a.map(JSON.stringify).join('\n')+'\n');fs.writeFileSync(files[1],b.map(JSON.stringify).join('\n')+'\n');
  fs.writeFileSync(files[2],JSON.stringify(bundle(a[0].scope)));fs.writeFileSync(files[3],JSON.stringify(bundle(b[0].scope)));
  const output=path.join(dir,'report.json');
  let run=spawnSync(process.execPath,[evaluator,...files,output],{encoding:'utf8'});
  assert.equal(run.status,0,run.stderr);
  const report=JSON.parse(fs.readFileSync(output,'utf8'));assert.equal(report.inputs.length,4);assert.equal(report.evaluators.length,3);
  assert.ok(report.inputs.every(row=>/^[a-f0-9]{64}$/.test(row.sha256)));
  run=spawnSync(process.execPath,[evaluator,...files,output],{encoding:'utf8'});assert.equal(run.status,1);
  fs.writeFileSync(files[0],JSON.stringify(a[0]));assert.throws(()=>read(files[0],true),/tail/);
});
