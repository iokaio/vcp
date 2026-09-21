// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const original=require('../../../scripts/evals/builtin-live-runner.cjs'),runner=require('../../../scripts/evals/builtin-live-continuation.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),put=(p,v)=>fs.writeFileSync(p,JSON.stringify(v));
function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-live-continuation-'));
  t.after(()=>{const resolved=fs.realpathSync(root);assert.equal(path.dirname(resolved),fs.realpathSync(os.tmpdir()));assert.ok(path.basename(resolved).startsWith('vcp-live-continuation-'));fs.rmSync(resolved,{recursive:true,force:true});});
  const executable=path.join(root,'vcp.exe');fs.writeFileSync(executable,'never execute');fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),path.join(root,'skills/builtin'),{recursive:true});
  const catalog=path.join(root,'catalog.json');put(catalog,{});
  const profile={version:1,trust_workspace:true,maximum_autonomy:'plan',automatic_effects:[],workspace:'rebound',sync_roots:[],provider:{valid_until:String(Date.now()+3600000),max_output:'512',price:{currency:'USD'},compatibility:{valid_until:String(Date.now()+3600000),responses_text_tools:true,provider_preferences_qualified:true}},catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'512',max_transport_retries:0,max_requests:8,deadline_seconds:60};
  const profileFile=path.join(root,'profile.json');put(profileFile,profile);const spec=path.join(root,'spec.json');put(spec,{executable,profile:profileFile,aggregate_cap_usd:'40.000000'});
  const prepared=original.prepare(spec,path.join(root,'original')),plan=JSON.parse(fs.readFileSync(prepared.plan));
  const scope={workspace:'workspace',session:'session',task:'task'},first={case_id:plan.runs[0].case_id,arm:'baseline',status:'failed',actual_cost_micros:null,answer:null,scope};
  const result={schema:'p7-builtin-live-result/1',plan_sha256:prepared.sha256,fixture_sha256:plan.fixture_sha256,stopped:true,actual_cost_micros:null,runs:plan.runs.map((r,i)=>i?{case_id:r.case_id,arm:r.arm,status:'not_run',actual_cost_micros:null}:first)};
  put(path.join(plan.directory,'execution-claim.json'),{plan_sha256:prepared.sha256});put(path.join(plan.directory,'result.json'),result);
  const base=path.join(plan.directory,plan.runs[0].id);put(path.join(base,'attempted.json'),{plan_sha256:prepared.sha256});put(path.join(base,'result.json'),first);fs.writeFileSync(path.join(base,'data','canonical-history'),'must remain unchanged');
  const costs=[{gaps:[],items:[{collection:'ledger',visibility:'available',record:{scope,currency:'USD',cap:'2500000',active:'0',settled:'0',unresolved:'201640',overrun:false}},{collection:'attempt',visibility:'available',record:{scope,phase:'reconciliation_pending',charged:'0',previous:null,role:'main',quote:{amount:{micros:'201640'}}}}]}];put(path.join(base,'costs.json'),costs);
  const bytes=Buffer.from(JSON.stringify({error:{message:'User not found.',code:401}}));put(path.join(base,'retained-response-inspection.json'),[{items:[{bytes:[...bytes],descriptor:{sha256:sha(bytes),length:String(bytes.length),spec:{channel:'response',scope}}}]}]);
  const receipt=path.join(root,'owner-authorization.json');put(receipt,{schema:'p7-owner-authorization/1',readonly_plan_sha256:prepared.sha256,generation_plan_sha256:'a'.repeat(64),aggregate_cap_usd:'45.000000',retries:0,permissions:'Exact reviewed plans, sole pinned verification launcher',authorization_source:'User response: Approved. Proceed.'});
  return {root,prepared,plan,base,costs,receipt,prepare:()=>runner.prepare(prepared.plan,prepared.sha256,path.join(root,'continuation'),receipt)};
}
test('prepares exact untouched fifteen, retains full failed allocation and binds evidence without dispatch',t=>{
  const f=fixture(t),before=fs.readFileSync(path.join(f.base,'data','canonical-history')),p=f.prepare(),v=runner.validate(p.plan,p.sha256);
  assert.equal(p.model_calls,0);assert.equal(p.additional_budget_micros,0);assert.deepEqual(v.plan.run_ids,f.plan.runs.slice(1).map(r=>r.id));assert.equal(v.plan.failure.unresolved_micros,201640);assert.equal(v.plan.failure.retained_allocation_micros,2500000);assert.equal(v.plan.remaining_allocation_micros,37500000);assert.deepEqual(fs.readFileSync(path.join(f.base,'data','canonical-history')),before);
  assert.throws(()=>runner.run(p.plan,'wrong',()=>assert.fail('must not dispatch')),/Integrity/);
});
test('tampered failed evidence and claimed rows reject before dispatch',t=>{
  const f=fixture(t),p=f.prepare(),history=path.join(f.base,'data','canonical-history');fs.appendFileSync(history,'tamper');
  assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/failure evidence/);fs.writeFileSync(history,'must remain unchanged');
  const next=path.join(f.plan.directory,f.plan.runs[1].id,'attempted.json');put(next,{});
  assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/already attempted/);
});
test('first continued unknown stops, preserves denominator, never retries original and bars continuation replay',t=>{
  const f=fixture(t),p=f.prepare();let calls=0;
  const result=runner.run(p.plan,p.sha256,(_exe,args)=>{calls++;assert.equal(args[args.indexOf('--workspace')+1],path.join(f.plan.directory,f.plan.runs[1].id,'workspace'));assert.equal(args[args.indexOf('--budget-usd')+1],'2.500000');assert.ok(args.includes('--skill'));return {status:null,error:'ETIMEDOUT',stdout:'',stderr:''};});
  assert.equal(calls,1);assert.equal(result.runs.length,16);assert.equal(result.runs.filter(r=>r.status==='not_run').length,14);assert.equal(result.actual_cost_micros,null);assert.equal(result.known_settled_micros,0);assert.equal(result.original_unresolved_micros,201640);assert.equal(result.new_unknown_liability,true);assert.equal(result.new_unknown_allocation_micros,2500000);
  assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/already claimed/);assert.equal(JSON.parse(fs.readFileSync(path.join(f.plan.directory,'result.json'))).runs[1].status,'not_run');
});
test('changed original allocation, evidence amount and continuation subset reject even with recomputed hash',t=>{
  const f=fixture(t);f.costs[0].items[0].record.unresolved='0';put(path.join(f.base,'costs.json'),f.costs);assert.throws(f.prepare,/liability differs/);
  f.costs[0].items[0].record.unresolved='201640';put(path.join(f.base,'costs.json'),f.costs);const p=f.prepare(),changed=JSON.parse(fs.readFileSync(p.plan));changed.run_ids.shift();put(p.plan,changed);assert.throws(()=>runner.run(p.plan,sha(fs.readFileSync(p.plan)),()=>assert.fail()),/remaining identities/);
});
test('settled failed row advances once; later unknown preserves known charges and stops',t=>{
  const f=fixture(t),p=f.prepare();let runs=0;
  const result=runner.run(p.plan,p.sha256,(_exe,args)=>{
    if(args.includes('run')){runs++;if(runs===2)return {status:null,error:'ETIMEDOUT',stdout:'',stderr:''};return {status:1,stdout:[{type:'accepted',scope:{task:'continued'}},{type:'result',conditions:{completed:false}}].map(v=>JSON.stringify(v)).join('\n'),stderr:''};}
    const costs=args[args.indexOf('--view')+1]==='costs';
    const items=costs?[{collection:'ledger',visibility:'available',record:{currency:'USD',cap:'2500000',active:'0',unresolved:'0',settled:'123',overrun:false}},{collection:'attempt',visibility:'available',record:{id:'attempt',phase:'settled',charged:'123',previous:null,role:'main',uncertain:null}},{collection:'settlement',visibility:'available',record:{attempt:'attempt',applied:true,observation:{final_usage:true}}}]:[];
    return {status:0,stdout:JSON.stringify({type:'result',data:{items,gaps:[],next_cursor:null}}),stderr:''};
  });
  assert.equal(runs,2);assert.equal(result.known_settled_micros,123);assert.equal(result.actual_cost_micros,null);assert.equal(result.runs[1].actual_cost_micros,123);assert.equal(result.runs[2].actual_cost_micros,null);assert.equal(result.runs.filter(r=>r.status==='not_run').length,13);assert.equal(result.new_unknown_allocation_micros,2500000);
});
test('owner receipt tampering rejects even when the plan and runtime are unchanged',t=>{
  const f=fixture(t),p=f.prepare(),receipt=JSON.parse(fs.readFileSync(f.receipt));receipt.retries=1;put(f.receipt,receipt);
  assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/authorization receipt differs/);
});
test('partial settled charges within an unresolved row remain visible and cannot authorize another row',t=>{
  const f=fixture(t),p=f.prepare();let runs=0;const scope={task:'continued'};
  const result=runner.run(p.plan,p.sha256,(_exe,args)=>{
    if(args.includes('run')){runs++;return {status:1,stdout:[{type:'accepted',scope},{type:'result',conditions:{completed:false}}].map(v=>JSON.stringify(v)).join('\n'),stderr:''};}
    const records=[['ledger',{scope,currency:'USD',cap:'2500000',active:'0',unresolved:'201640',settled:'6957',overrun:false}],['attempt',{scope,id:'known',phase:'settled',charged:'6957'}],['attempt',{scope,id:'unknown',phase:'reconciliation_pending',charged:'0'}],['settlement',{scope,attempt:'known',applied:true,total:'6957'}]];
    const items=args[args.indexOf('--view')+1]==='costs'?records.map(([collection,record])=>({collection,record,visibility:'available'})):[];
    return {status:0,stdout:JSON.stringify({type:'result',data:{items,gaps:[],next_cursor:null}}),stderr:''};
  });
  assert.equal(runs,1);assert.equal(result.actual_cost_micros,null);assert.equal(result.known_settled_micros,6957);assert.equal(result.known_settled_complete,true);assert.deepEqual(result.runs[1].cost_diagnostic,{known_settled_micros:6957,unresolved_micros:201640,active_micros:0,overrun:false});assert.equal(result.new_unknown_allocation_micros,2500000);
});
test('receipt raw-byte changes and original plan tampering are independently rejected',t=>{
  const f=fixture(t),p=f.prepare(),bytes=fs.readFileSync(f.receipt);fs.appendFileSync(f.receipt,'\n');assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/receipt changed/);fs.writeFileSync(f.receipt,bytes);
  fs.appendFileSync(f.prepared.plan,'\n');assert.throws(()=>runner.run(p.plan,p.sha256,()=>assert.fail()),/approved plan hash changed/);
});
test('expiry is checked immediately before later rows without dispatching an expired profile',t=>{
  const f=fixture(t),p=f.prepare();let runs=0;const realNow=Date.now;
  t.after(()=>{Date.now=realNow;});
  const result=runner.run(p.plan,p.sha256,(_exe,args)=>{
    if(args.includes('run')){runs++;Date.now=()=>realNow()+7200000;return {status:1,stdout:[{type:'accepted',scope:{task:'continued'}},{type:'result',conditions:{completed:false}}].map(v=>JSON.stringify(v)).join('\n'),stderr:''};}
    const items=args[args.indexOf('--view')+1]==='costs'?[{collection:'ledger',visibility:'available',record:{currency:'USD',cap:'2500000',active:'0',unresolved:'0',settled:'0',overrun:false}}]:[];
    return {status:0,stdout:JSON.stringify({type:'result',data:{items,gaps:[],next_cursor:null}}),stderr:''};
  });
  assert.equal(runs,1);assert.equal(result.stopped,true);assert.equal(result.runs[2].status,'not_run');assert.match(result.runs[2].reason,/expired/);assert.equal(result.new_unknown_liability,false);
});


for(const invalid of ['foreign ledger and attempts','foreign settlement','missing settlement scope'])test(`partial diagnostic rejects ${invalid} without attributing its charges`,t=>{
  const f=fixture(t),p=f.prepare();let runs=0;
  const scope={workspace:'workspace',session:'session',task:'continued'};
  const foreign={...scope,task:'other-task'};
  const ledgerScope=invalid==='foreign ledger and attempts'?foreign:scope;
  const settlementScope=invalid==='missing settlement scope'?undefined:foreign;
  const result=runner.run(p.plan,p.sha256,(_exe,args)=>{
    if(args.includes('run')){runs++;return {status:1,stdout:[{type:'accepted',scope},{type:'result',conditions:{completed:false}}].map(v=>JSON.stringify(v)).join('\n'),stderr:''};}
    const records=[['ledger',{scope:ledgerScope,currency:'USD',cap:'2500000',active:'0',unresolved:'201640',settled:'37',overrun:false}],['attempt',{scope:ledgerScope,id:'known',phase:'settled',charged:'37'}],['attempt',{scope:ledgerScope,id:'unknown',phase:'reconciliation_pending',charged:'0'}],['settlement',{scope:settlementScope,attempt:'known',applied:true,total:'37'}]];
    const items=args[args.indexOf('--view')+1]==='costs'?records.map(([collection,record])=>({collection,record,visibility:'available'})):[];
    return {status:0,stdout:JSON.stringify({type:'result',data:{items,gaps:[],next_cursor:null}}),stderr:''};
  });
  assert.equal(runs,1);assert.equal(result.stopped,true);assert.equal(result.runs[1].cost_diagnostic,null);
  assert.equal(result.known_settled_micros,0);assert.equal(result.known_settled_complete,false);
  assert.equal(result.actual_cost_micros,null);assert.equal(result.new_unknown_allocation_micros,2500000);
  assert.equal(result.runs.filter(row=>row.status==='not_run').length,14);
});
