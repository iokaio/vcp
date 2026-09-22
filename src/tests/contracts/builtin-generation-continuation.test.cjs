// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const continuation=require('../../../scripts/evals/builtin-generation-continuation.cjs');
const prep=require('../../../scripts/evals/builtin-generation-prepare.cjs');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scope={workspace:'fixture-workspace',session:'fixture-session',task:'fixture-task'};
function costs(){
  const records=[{collection:'ledger',record:{scope,currency:'USD',cap:'2500000',active:'0',settled:'6957',unresolved:'201640',overrun:false}}];
  [1057,1117,1231,1722,1830,0].forEach((charge,index)=>{
    const id='attempt-'+index;
    records.push({collection:'attempt',record:{id,scope,role:'main',previous:null,phase:charge?'settled':'reconciliation_pending',charged:String(charge),quote:{amount:{currency:'USD',micros:'201640'}}}});
    if(charge)records.push({collection:'settlement',record:{id:'settlement-'+index,scope,attempt:id,applied:true,total:String(charge)}});
  });
  return [{gaps:[],items:records.map(record=>({...record,visibility:'available'}))}];
}
test('retained baseline accounting keeps known charges and unknown liability distinct',()=>{
  assert.deepEqual(continuation.retainedAccounting(costs(),scope),{known_settled_micros:6957,unresolved_micros:201640,retained_allocation_micros:2500000});
  assert.deepEqual(continuation.costDiagnostic(costs(),scope),{known_settled_micros:6957,unresolved_micros:201640,active_micros:0,overrun:false});
  const changed=costs();changed[0].items.find(i=>i.collection==='ledger').record.unresolved='0';
  assert.throws(()=>continuation.retainedAccounting(changed,scope),/ledger changed/);
  const wrongScope=costs();wrongScope[0].items.find(i=>i.collection==='ledger').record.scope={...scope,task:'other-task'};
  assert.throws(()=>continuation.retainedAccounting(wrongScope,scope),/ledger changed/);
  const missing=costs();missing[0].items=missing[0].items.filter(i=>i.collection!=='settlement');
  assert.throws(()=>continuation.retainedAccounting(missing,scope),/denominator changed/);
  const unavailable=costs();unavailable[0].gaps.push({reason:'missing'});
  assert.throws(()=>continuation.retainedAccounting(unavailable,scope),/evidence incomplete/);
  const duplicate=costs();const settled=duplicate[0].items.filter(i=>i.collection==='settlement');settled[1].record.id=settled[0].record.id;
  assert.throws(()=>continuation.retainedAccounting(duplicate,scope),/duplicated/);
});
function original(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-u03-continuation-'));
  t.after(()=>{const resolved=path.resolve(root),relative=path.relative(path.resolve(os.tmpdir()),resolved);assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative)&&path.dirname(resolved)===path.resolve(os.tmpdir()));fs.rmSync(resolved,{recursive:true,force:true});});
  const write=(file,value)=>fs.writeFileSync(file,typeof value==='string'?value:JSON.stringify(value));
  const executable=path.join(root,'vcp.exe');write(executable,'synthetic executable; injected dispatch only');
  fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),path.join(root,'skills/builtin'),{recursive:true});
  fs.appendFileSync(executable,fs.readFileSync(path.join(root,'skills/builtin/catalog.json')));
  const catalog=path.join(root,'catalog.json');write(catalog,{});
  const profileFile=path.join(root,'source-profile.json');
  write(profileFile,{version:1,trust_workspace:true,maximum_autonomy:'workspace',automatic_effects:['read','write'],workspace:'rebound',sync_roots:[],provider:{valid_until:String(Date.now()+3600000),max_output:'512',price:{currency:'USD',valid_until:String(Date.now()+3600000)},compatibility:{valid_until:String(Date.now()+3600000),responses_text_tools:true,provider_preferences_qualified:true}},catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'512',max_transport_retries:0,max_requests:8,deadline_seconds:300});
  const spec=path.join(root,'spec.json');write(spec,{executable,profile:profileFile,aggregate_cap_usd:'5.000000',propose_opaque_launcher_effects:true,runtime:{node:process.execPath,launcher:process.env.VCP_U03_LAUNCHER,build_receipt:process.env.VCP_U03_BUILD_RECEIPT}});
  const prepared=prep.prepare(spec,path.join(root,'original')),plan=JSON.parse(fs.readFileSync(prepared.plan)),base=path.join(plan.directory,'baseline');
  const baseline={arm:'baseline',status:'failed',actual_cost_micros:null,scope,reason:'retained incomplete response'};
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:prepared.sha256});
  write(path.join(plan.directory,'result.json'),{schema:'p7-u03-generation-result/1',plan_sha256:prepared.sha256,actual_cost_micros:null,stopped:true,runs:[baseline,{arm:'skill',status:'not_run',actual_cost_micros:null}]});
  const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd','2.500000','--autonomy','autonomous'];
  write(path.join(base,'result.json'),baseline);write(path.join(base,'attempted.json'),{plan_sha256:prepared.sha256,args});write(path.join(base,'costs.json'),costs());
  const response='data: '+JSON.stringify({type:'response.incomplete',response:{status:'incomplete',incomplete_details:{reason:'max_output_tokens'}}})+'\n\n';
  write(path.join(base,'response-25ab587f-inspected.sse'),response);
  write(path.join(base,'outputs.json'),[{gaps:[],items:[{collection:'artifact',visibility:'available',record:{length:String(Buffer.byteLength(response)),sha256:sha(response),spec:{channel:'response',schema:'responses-sse-observed-through-terminal/1',scope}}}]}]);
  write(path.join(base,'data/history.json'),'historical baseline store, must remain unchanged');
  write(path.join(base,'workspace/src/cart.cjs'),'// preserved attempted baseline changes\n');
  const receipt=path.join(root,'owner.json');write(receipt,{schema:'p7-owner-authorization/1',generation_plan_sha256:prepared.sha256,readonly_plan_sha256:'a'.repeat(64),aggregate_cap_usd:'45.000000',retries:0,permissions:'Exact reviewed plans, sole pinned verification launcher',authorization_source:'User response: Approved. Proceed.'});
  return {root,prepared,plan,base,receipt,write};
}
test('only pristine original skill arm continues once with unchanged permissions and retained baseline',t=>{
  if(!process.env.VCP_U03_LAUNCHER||!process.env.VCP_U03_BUILD_RECEIPT)return t.skip('Requires recorded pinned runtime build; no live dispatch');
  const setup=original(t),{root,prepared,plan,receipt,base,write}=setup;
  const frozen=continuation.failureEvidence(plan,prepared.sha256);
  const first=continuation.prepare(prepared.plan,prepared.sha256,path.join(root,'continuation'),receipt);
  const second=continuation.prepare(prepared.plan,prepared.sha256,path.join(root,'other-continuation'),receipt);
  assert.equal(first.additional_budget_micros,0);assert.equal(first.attempts,1);
  continuation.validate(first.plan,first.sha256);
  const originalPlan=fs.readFileSync(prepared.plan),alteredPermission=JSON.parse(originalPlan);alteredPermission.permission_review.automatic_effects.pop();write(prepared.plan,alteredPermission);
  assert.throws(()=>continuation.prepare(prepared.plan,sha(fs.readFileSync(prepared.plan)),path.join(root,'changed-permission'),receipt),/exact permission proposal changed/);fs.writeFileSync(prepared.plan,originalPlan);
  const originalReceipt=fs.readFileSync(receipt);const changed=JSON.parse(originalReceipt);changed.aggregate_cap_usd='46.000000';write(receipt,changed);
  assert.throws(()=>continuation.validate(first.plan,first.sha256),/authorization receipt changed/);fs.writeFileSync(receipt,originalReceipt);
  const pending=path.join(plan.directory,'skill/data/pending');write(pending,'already used');
  assert.throws(()=>continuation.validate(first.plan,first.sha256),/already attempted/);fs.unlinkSync(pending);
  const terminal=path.join(base,'response-25ab587f-inspected.sse'),originalTerminal=fs.readFileSync(terminal);write(terminal,'altered');
  assert.throws(()=>continuation.validate(first.plan,first.sha256),/descriptor mismatch/);fs.writeFileSync(terminal,originalTerminal);
  const profile=path.join(plan.directory,'skill/profile.json'),originalProfile=fs.readFileSync(profile);const broadened=JSON.parse(originalProfile);broadened.max_requests=9;write(profile,broadened);
  assert.throws(()=>continuation.validate(first.plan,first.sha256),/verification profile changed/);fs.writeFileSync(profile,originalProfile);
  let calls=0;const result=continuation.run(first.plan,first.sha256,(_executable,args)=>{
    calls++;assert.equal(args[args.indexOf('--workspace')+1],path.join(plan.directory,'skill/workspace'));assert.equal(args[args.indexOf('--budget-usd')+1],'2.500000');assert.equal(args.at(-1),'vcp-builtin::javascript-typescript::javascript-typescript');
    return {error:'ETIMEDOUT',status:null,stdout:'',stderr:''};
  });
  assert.equal(calls,1);assert.equal(result.actual_cost_micros,null);assert.equal(result.known_settled_micros,6957);assert.equal(result.original_unresolved_micros,201640);assert.equal(result.new_unknown_allocation_micros,2500000);assert.equal(result.runs.length,2);
  assert.equal(result.known_settled_complete,false);
  assert.deepEqual(continuation.failureEvidence(plan,prepared.sha256),frozen);
  assert.throws(()=>continuation.run(first.plan,first.sha256,()=>{throw Error('replay');}),/already claimed/);
  assert.throws(()=>continuation.run(second.plan,second.sha256,()=>{throw Error('duplicate');}),/already attempted/);
});
test('new incomplete skill accounting retains observed charges without running verification or oracle',t=>{
  if(!process.env.VCP_U03_LAUNCHER||!process.env.VCP_U03_BUILD_RECEIPT)return t.skip('Requires recorded pinned runtime build; no live dispatch');
  const {root,prepared,plan,receipt}=original(t);
  const next=continuation.prepare(prepared.plan,prepared.sha256,path.join(root,'continuation'),receipt);
  const skillScope={...scope,task:'skill-task'};
  const partial={gaps:[],items:[
    {collection:'ledger',record:{scope:skillScope,currency:'USD',cap:'2500000',settled:'37',unresolved:'201640',active:'0',overrun:false}},
    {collection:'attempt',record:{id:'skill-settled',scope:skillScope,phase:'settled',charged:'37'}},
    {collection:'attempt',record:{id:'skill-unknown',scope:skillScope,phase:'reconciliation_pending',charged:'0'}},
    {collection:'settlement',record:{id:'skill-settlement',attempt:'skill-settled',scope:skillScope,total:'37',applied:true}},
  ].map(item=>({...item,visibility:'available'}))};
  let taskCalls=0,inspections=0;
  const result=continuation.run(next.plan,next.sha256,(_executable,args)=>{
    if(args.includes('run')){taskCalls++;return {status:0,stdout:JSON.stringify({type:'accepted',scope:skillScope})+'\n'+JSON.stringify({type:'result',conditions:{completed:true}})+'\n',stderr:''};}
    assert.ok(args.includes('inspect'));inspections++;
    const view=args[args.indexOf('--view')+1];return {status:0,stdout:JSON.stringify({type:'result',data:view==='costs'?partial:{gaps:[],items:[]}})+'\n',stderr:''};
  });
  assert.equal(taskCalls,1);assert.equal(inspections,5);assert.equal(result.stopped,true);assert.equal(result.known_settled_micros,6994);assert.equal(result.known_settled_complete,true);assert.equal(result.actual_cost_micros,null);
  assert.deepEqual(result.runs[1].cost_diagnostic,{known_settled_micros:37,unresolved_micros:201640,active_micros:0,overrun:false});assert.equal(result.runs[1].status,'failed');assert.equal(result.runs[1].oracle,undefined);assert.equal(result.runs[1].verification,undefined);assert.equal(result.baseline_preserved,true);
});
