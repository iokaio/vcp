// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const runner=require('../../../scripts/evals/model-rotation-live-runner.cjs');
function fixture(t) {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-model-rotation-offline-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  for(const name of ['vcp.exe','node.exe','endpoints.json'])fs.writeFileSync(path.join(root,name),'synthetic offline bytes');
  const p={version:1,trust_workspace:true,max_requests:12,deadline_seconds:180,output_tokens:'1024',max_transport_retries:2,provider_timeout_seconds:45,maximum_autonomy:'autonomous',automatic_effects:['read','write','execute','opaque'],canonical_tools:['vcp_read','vcp_patch','vcp_verify'],provider:{price:{currency:'USD'},valid_until:Date.now()+3600000,compatibility:{model:'qwen/qwen3-coder',endpoint:'google-vertex/us-south1',valid_until:Date.now()+3600000}},catalog:path.join(root,'endpoints.json')};
  fs.writeFileSync(path.join(root,'baseline.json'),JSON.stringify(p));fs.writeFileSync(path.join(root,'rotation.json'),JSON.stringify({...p,routing:{rotation:{synthetic_offline_test:true}}}));
  const spec=path.join(root,'spec.json');fs.writeFileSync(spec,JSON.stringify({executable:path.join(root,'vcp.exe'),node:path.join(root,'node.exe'),baseline_profile:path.join(root,'baseline.json'),rotation_profile:path.join(root,'rotation.json')}));
  return {root,spec,directory:path.join(root,'campaign')};
}
test('preparation binds four permanent $3 caps and distinct canonical roots without dispatch',t=>{
  const f=fixture(t),prepared=runner.prepare(f.spec,f.directory),p=JSON.parse(fs.readFileSync(prepared.plan));assert.equal(prepared.model_calls,0);assert.equal(p.aggregate_cap_micros,25000000);assert.equal(p.allocated_cap_micros,12000000);assert.equal(p.unused_cap_micros,13000000);assert.equal(new Set(p.runs.map(r=>JSON.parse(fs.readFileSync(path.join(f.directory,r.id,'profile.json'))).workspace)).size,4);runner.bounds(p);assert.throws(()=>runner.bounds({...p,runs:[...p.runs,{id:'extra',cap_micros:3000000}]}));assert.throws(()=>runner.bounds({...p,aggregate_cap_micros:26000000}));
});
test('unknown liabilities stop the next pair and execution claims prohibit allocation replay',async t=>{
  const f=fixture(t),prepared=runner.prepare(f.spec,f.directory);let calls=0,active=0,maximum=0;
  async function call(_exe,args){if(args.includes('run')){calls++;active++;maximum=Math.max(maximum,active);await new Promise(resolve=>setTimeout(resolve,20));active--;return {status:0,error:null,stdout:JSON.stringify({type:'accepted',scope:{task:'root'}})+'\n'+JSON.stringify({type:'result',conditions:{completed:false}})+'\n',stderr:''};}return {status:0,error:null,stdout:JSON.stringify({type:'result',data:{items:args.includes('costs')?[{visibility:'available',collection:'ledger',record:{currency:'USD',cap:'3000000',settled:'10',active:'0',unresolved:'100',overrun:false}}]:[],gaps:[],next_cursor:null}})+'\n',stderr:''};}
  const result=await runner.run(prepared.plan,prepared.plan_sha256,call);assert.equal(calls,2);assert.equal(maximum,2);assert.equal(result.stopped,true);assert.equal(result.reserved_cap_micros,12000000);assert.equal(result.runs.length,2);assert.ok(result.runs.every(r=>r.status==='unknown'));await assert.rejects(runner.run(prepared.plan,prepared.plan_sha256,call));assert.equal(calls,2);
});
test('source hashes and exact authorization fail before any model dispatch',async t=>{
  const f=fixture(t),prepared=runner.prepare(f.spec,f.directory);let calls=0;const call=async()=>{calls++;throw Error('unexpected dispatch');};await assert.rejects(runner.run(prepared.plan,'0'.repeat(64),call));fs.appendFileSync(path.join(f.directory,'baseline-A','workspace','value.cjs'),'// altered');await assert.rejects(runner.run(prepared.plan,prepared.plan_sha256,call));assert.equal(calls,0);
});
test('money remains conservative and verifier authority cannot broaden silently',t=>{
  const f=fixture(t),p=JSON.parse(fs.readFileSync(path.join(f.root,'baseline.json')));runner.gate(p,'baseline');assert.throws(()=>runner.gate({...p,automatic_effects:[...p.automatic_effects,'network']},'baseline'));assert.throws(()=>runner.gate({...p,canonical_tools:[...p.canonical_tools,'vcp_exec']},'baseline'));const pages=[{gaps:[],items:[{visibility:'available',collection:'ledger',record:{cap:'3000000',currency:'USD',settled:'1000000',active:'1000000',unresolved:'1000000',overrun:false}}]}];assert.deepEqual(runner.accounting(pages,3000000),{settled:1000000,active:1000000,unresolved:1000000});pages[0].items[0].record.unresolved='1000001';assert.throws(()=>runner.accounting(pages,3000000));
});
test('diagnostics redact credential echoes without changing safe text',()=>{
  const key='VCP_OFFLINE_SECRET';const old=process.env[key];process.env[key]='private-test-secret';try{assert.equal(runner.redact('safe private-test-secret Bearer hidden-key sk-or-v1-abcdef'),'safe [REDACTED] Bearer [REDACTED] [REDACTED]');}finally{if(old===undefined)delete process.env[key];else process.env[key]=old;}
});
