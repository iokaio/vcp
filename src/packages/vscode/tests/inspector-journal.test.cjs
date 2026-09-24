// SPDX-License-Identifier: Apache-2.0
const test=require('node:test');
const assert=require('node:assert/strict');
const {randomUUID}=require('node:crypto');
const {InspectorJournal,inspectorCommandRecords,INSPECTOR_COMMAND_METHODS}=require('../dist/inspector_journal.js');
const scope={workspace:'workspace',session:'session'};
const record=extra=>({version:1,commandId:randomUUID(),scope,method:'backup/create',phase:'submitting',...extra});
const receipt=(command,extra)=>({scope:command.scope,command_id:command.commandId,task:command.task??null,outcome:'accepted',revision:'9007199254740993',watermark:'18446744073709551615',...extra});
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no});return {promise,resolve,reject}};

test('inspector journal accepts only bounded nonsecret metadata and turns interrupted submits unknown',()=>{
  for(const method of INSPECTOR_COMMAND_METHODS){const value=record({method});const restored=inspectorCommandRecords([value]);assert.equal(restored[0].phase,'unknown');assert.notEqual(restored[0].scope,scope)}
  for(const value of [record({payload:'secret'}),record({path:'private'}),record({method:'session/export'}),record({scope:{...scope,actor:'forged'}}),record({target:'file:/private'}),record({revision:'18446744073709551616'}),record({watermark:'01'}),record({commandId:'-'.repeat(36)}),Object.defineProperty(record(), 'phase',{enumerable:true,get(){throw Error('getter executed')}})])assert.deepEqual(inspectorCommandRecords([value]),[]);
  const repeated=record();assert.deepEqual(inspectorCommandRecords([repeated,repeated]),[]);
  const array=[];Object.defineProperty(array,'0',{enumerable:true,get(){throw Error('array getter')}});assert.deepEqual(inspectorCommandRecords(array),[]);
  assert.deepEqual(inspectorCommandRecords(Array.from({length:257},()=>record())),[]);
  assert.deepEqual(inspectorCommandRecords(new Proxy([], {ownKeys(){throw Error('proxy')}})),[]);
});
test('command identity is saved before send eligibility; concurrent writers are serialized',async()=>{
  const first=deferred(),writes=[];const journal=new InspectorJournal(async rows=>{writes.push(structuredClone(rows));if(writes.length===1)await first.promise});
  let ready=false;const a=journal.begin(scope,'routing/reportCapture').then(id=>{ready=true;return id});const b=journal.begin(scope,'backup/create');
  await new Promise(setImmediate);assert.equal(ready,false);assert.equal(writes.length,1);assert.equal(journal.records().length,0);first.resolve();const [aid,bid]=await Promise.all([a,b]);
  assert.equal(writes.length,2);assert.deepEqual(writes[1].map(row=>row.commandId),[aid,bid]);assert.ok(writes.every(rows=>rows.every(row=>row.phase==='submitting')));
  const fail=new InspectorJournal(async()=>{throw Error('disk unavailable')});await assert.rejects(fail.begin(scope,'backup/create'));assert.deepEqual(fail.records(),[]);
  await assert.rejects(journal.begin(scope,'backup/create',{method:'routing/apply'}));
});
test('correlated accepted receipt survives failed save and later unknown outcomes',async()=>{
  let fail=false;const journal=new InspectorJournal(async()=>{if(fail)throw Error('storage failed')});const id=await journal.begin(scope,'routing/apply',{target:'review'});const command=journal.records()[0];
  await assert.rejects(journal.settle(id,'accepted',receipt(command,{task:'foreign'})));assert.equal(journal.records()[0].phase,'submitting');
  fail=true;await assert.rejects(journal.settle(id,'accepted',receipt(command)));assert.equal(journal.records()[0].phase,'accepted');
  await journal.settle(id,'unknown');await journal.settle(id,'rejected');assert.equal(journal.records()[0].phase,'accepted');
  fail=false;await journal.reconcile(scope,async(method,request)=>{assert.equal(method,'command/read');assert.equal(request.command_id,id);return {kind:'acceptance',value:receipt(command)}});
  assert.equal(journal.records()[0].phase,'reconciled');assert.equal(journal.records()[0].revision,'9007199254740993');
});
test('reload reconciliation reads only same-scope original IDs, preserves known acceptance and never replays',async()=>{
  const unknown=record(),accepted=record({phase:'accepted'}),foreign=record({scope:{workspace:'other',session:'other'}});
  const writes=[],calls=[];const journal=new InspectorJournal(async rows=>writes.push(rows),[unknown,accepted,foreign]);
  await journal.reconcile(scope,async(method,request)=>{calls.push([method,request]);throw Error('not found or access revoked')});
  assert.deepEqual(calls.map(row=>row[0]),['command/read','command/read']);assert.equal(journal.records()[0].phase,'unknown');assert.equal(journal.records()[1].phase,'accepted');assert.equal(journal.records()[2].phase,'unknown');
  await journal.reconcile(scope,async(method,request)=>({kind:'acceptance',value:receipt(request.command_id===unknown.commandId?unknown:accepted,{scope:{workspace:'wrong',session:'wrong'}})}));assert.equal(journal.records()[1].phase,'accepted');
  assert.ok(writes.every(rows=>rows.every(row=>!Object.hasOwn(row,'payload'))));
});
test('full journals never evict unresolved commands; only terminal history makes space',async()=>{
  const full=Array.from({length:256},()=>record({phase:'unknown'}));const blocked=new InspectorJournal(async()=>{},full);await assert.rejects(blocked.begin(scope,'backup/create'),/full/);assert.equal(blocked.records().length,256);
  full[0]={...full[0],phase:'reconciled'};const journal=new InspectorJournal(async()=>{},full);await journal.begin(scope,'backup/create');assert.equal(journal.records().length,256);assert.ok(!journal.records().some(value=>value.commandId===full[0].commandId));
});

test('actual pruning job reference survives reload separately from command identity',async()=>{
 let saved;const journal=new InspectorJournal(async rows=>saved=structuredClone(rows));const command=await journal.begin(scope,'memory/forget',{task:'task'});const metadata=journal.records()[0];
 await journal.settle(command,'accepted',receipt(metadata),'actual-retention-job');assert.notEqual(command,journal.records()[0].target);
 const restored=new InspectorJournal(async()=>{},saved);assert.equal(restored.records()[0].target,'actual-retention-job');
 await assert.rejects(journal.settle(command,'accepted',receipt(metadata),'file:/private'));
 await assert.rejects(journal.settle(command,'accepted',receipt(metadata),'different-job'));
 const unknown=new InspectorJournal(async()=>{},[record({method:'memory/forget',task:'task',phase:'unknown'})]);const original=unknown.records()[0];await unknown.reconcile(scope,async()=>({kind:'acceptance',value:receipt(original)}));assert.equal(unknown.records()[0].phase,'reconciled');assert.equal(unknown.records()[0].target,undefined,'command receipt alone cannot invent a pruning job');
});
