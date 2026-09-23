// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const renewal=require('../../../scripts/evals/p8-profile-renewal.cjs');
const owner=require('../../../scripts/evals/p805-owner-runner.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const put=(file,value)=>fs.writeFileSync(file,JSON.stringify(value));
const budget=()=>({schema:'p7-p8-owner-campaign/1',cap_micros:100000000,settled_micros:1000,reserved_micros:20000000,models:['qwen/qwen3.8-max-0902'],runs:[{sha256:'old',status:'failed-unknown',cap_micros:20000000}]});
function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-renewal-controls-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  // A single observation keeps the maximum catalog window exact even when
  // fixture construction crosses a millisecond boundary under parallel load.
  const observed=Date.now();
  const spec={model:'qwen/qwen3.8-max-0902',observed_at:String(observed),valid_until:String(observed+86400000)},specFile=path.join(root,'spec.json');put(specFile,spec);
  const campaign=path.join(root,'campaign.json');put(campaign,budget());
  const plan={spec:{path:specFile,sha256:sha(fs.readFileSync(specFile))},executable:{path:'never-launched.exe',sha256:'bound'},output:path.join(root,'probe'),campaign,campaign_sha256:sha(fs.readFileSync(campaign))};
  const scope={task:'task',workspace:'workspace',session:'session'};
  const ledger={scope,cap:'12000000',currency:'USD',active:'0',unresolved:'0',overrun:false,settled:'7'};
  const report={schema:'p6-provider-conformance/1',status:'failed',scope,ledger,actual_cost_micros:'7'};
  const records=[{collection:'ledger',id:'task',workspace:'workspace',value:ledger},{collection:'attempt',id:'attempt',workspace:'workspace',value:{id:'attempt',reservation:'reservation',scope,root:'task',phase:'settled',charged:'7',role:'main'}}, {collection:'reservation',id:'reservation',workspace:'workspace',value:{attempt:'attempt',scope,root:'task',phase:'settled',liability:'0',charged:'7',role:'main'}}];
  const claim={binary_sha256:'bound',spec_sha256:plan.spec.sha256,spec};
  return {root,plan,report,records,claim};
}
test('reservation preserves old unknown liabilities and refuses collisions/over-budget',()=>{
  const b=budget(),old=structuredClone(b.runs[0]);renewal.reserve(b,{spec:{path:'fixed'}},'new');
  assert.equal(b.reserved_micros,32000000);assert.deepEqual(b.runs[0],old);assert.equal(b.settled_micros,1000);
  assert.throws(()=>renewal.reserve(b,{spec:{path:'fixed'}},'new'),/Duplicate/);
  for(const status of ['prepared','running','running-held']){const b=budget();b.runs.push({status});assert.throws(()=>renewal.reserve(b,{spec:{path:'fixed'}},'new'),/outstanding/);}
  const full=budget();full.reserved_micros=90000000;assert.throws(()=>renewal.reserve(full,{},'new'),/bound/);
});

test('P8 retained liabilities permit an independently settled renewal and bounded owner admission',async t=>{
  const f=fixture(t),old=process.env.OPENROUTER_API_KEY;
  process.env.OPENROUTER_API_KEY='synthetic-only';
  t.after(()=>{if(old===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=old;});
  const initial={...budget(),settled_micros:3637947,reserved_micros:78127269,runs:[
    {sha256:'historical',status:'failed-unknown',cap_micros:66127269},
    {sha256:'consumed-renewal',status:'failed-unknown',cap_micros:12000000,actual_cost_micros:null}
  ]};
  put(f.plan.campaign,initial);f.plan.campaign_sha256=sha(fs.readFileSync(f.plan.campaign));
  // Synthetic, independently joined two-attempt accounting exercises the coordinator;
  // these records provide no live provider qualification or paid execution authority.
  f.report.status='observed';f.report.responses=[{},{}];f.report.responses_text_tools=true;
  f.records[1].value.charged='3';f.records[2].value.charged='3';
  const attempt=structuredClone(f.records[1]),reservation=structuredClone(f.records[2]);
  attempt.id='attempt-2';Object.assign(attempt.value,{id:'attempt-2',reservation:'reservation-2',charged:'4'});
  reservation.id='reservation-2';Object.assign(reservation.value,{attempt:'attempt-2',charged:'4'});
  f.records.push(attempt,reservation);
  let executions=0;
  const result=await renewal.run('synthetic-proposal','fresh-renewal',{
    platform:'win32',validate:()=>({plan:f.plan,root:f.root}),inputs:()=>{},execute:async()=>{
      executions++;
      const held=fs.readFileSync(f.plan.campaign);
      assert.equal(JSON.parse(held).reserved_micros,90127269);
      assert.throws(()=>owner.changeCampaign(f.plan.campaign,b=>owner.reserve(b,{directory:f.root},'owner-cohort',{id:'u01-sqlite'})),/outstanding/);
      assert.deepEqual(fs.readFileSync(f.plan.campaign),held);
      fs.mkdirSync(f.plan.output);
      put(path.join(f.plan.output,'result.json'),f.report);
      put(path.join(f.plan.output,'canonical-records.json'),f.records);
      put(path.join(f.plan.output,'claim.json'),f.claim);
      return {status:0,error:null,process_reaped:true,stdout:'',stderr:''};
    }
  });
  assert.equal(executions,1);assert.equal(result.status,'observed-awaiting-qualification');
  const settled=JSON.parse(fs.readFileSync(f.plan.campaign));
  assert.equal(settled.reserved_micros,78127269);assert.equal(settled.settled_micros,3637954);
  assert.deepEqual(settled.runs.slice(0,2),initial.runs);
  // Admission at the exact remaining-budget boundary succeeds; one micro-dollar
  // over refuses atomically. No test rewrites the real campaign or its receipts.
  for(const extra of [0,1]){
    const boundary=structuredClone(settled);boundary.settled_micros=13872731+extra;
    put(f.plan.campaign,boundary);const before=fs.readFileSync(f.plan.campaign);
    const admit=()=>owner.changeCampaign(f.plan.campaign,b=>owner.reserve(b,{directory:f.root},'owner-cohort',{id:'u01-sqlite'}));
    if(extra){assert.throws(admit,/ceiling/);assert.deepEqual(fs.readFileSync(f.plan.campaign),before);}
    else {admit();const admitted=JSON.parse(fs.readFileSync(f.plan.campaign));assert.equal(admitted.settled_micros+admitted.reserved_micros,100000000);assert.deepEqual(admitted.runs.slice(0,2),initial.runs);}
  }
});
test('canonical accounting independently joins claims, ledger, attempts and charge sum',t=>{
  const f=fixture(t);assert.equal(renewal.accounting(f.report,f.records,f.claim,f.plan),7);
  const corruptions=[x=>x.report.ledger.unresolved='1',x=>x.report.actual_cost_micros='8',x=>x.records[2].value.charged='8',x=>x.records[2].value.phase='unknown',x=>x.records[2].value.scope.task='foreign',x=>x.claim.binary_sha256='changed',x=>x.report.status='observed',x=>x.records.push({...x.records[1]})];
  for(const change of corruptions){const x=structuredClone(f);change(x);assert.throws(()=>renewal.accounting(x.report,x.records,x.claim,x.plan));}
});
test('bounded fake execution reserves before launch, redacts output, settles known failure and blocks replay',async t=>{
  const f=fixture(t),old=process.env.OPENROUTER_API_KEY;process.env.OPENROUTER_API_KEY='synthetic-secret';t.after(()=>{if(old===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=old;});
  const controls={platform:'win32',validate:()=>({plan:f.plan,root:f.root}),inputs:()=>{},execute:async(program,args,deadline,env)=>{
    assert.equal(JSON.parse(fs.readFileSync(f.plan.campaign)).reserved_micros,32000000);
    assert.deepEqual(args,[f.plan.spec.path,f.plan.output,f.plan.spec.sha256]);assert.equal(deadline,300000);assert.equal(env.OPENROUTER_API_KEY,'synthetic-secret');
    fs.mkdirSync(f.plan.output);put(path.join(f.plan.output,'result.json'),f.report);put(path.join(f.plan.output,'canonical-records.json'),f.records);put(path.join(f.plan.output,'claim.json'),f.claim);
    return {status:1,error:null,process_reaped:true,stdout:'synthetic-secret',stderr:'Bearer synthetic-secret'};
  }};
  const result=await renewal.run('proposal','new',controls);assert.equal(result.status,'failed-reconciled');
  assert.equal(result.sensitive_output_detected,true);
  const b=JSON.parse(fs.readFileSync(f.plan.campaign));assert.equal(b.reserved_micros,20000000);assert.equal(b.settled_micros,1007);assert.deepEqual(b.runs[0],budget().runs[0]);
  assert.ok(!fs.readFileSync(path.join(f.root,'execution.json'),'utf8').includes('synthetic-secret'));
  await assert.rejects(renewal.run('proposal','new',controls),/EEXIST/);
});
test('timeout, unreaped process and invalid report retain full cap and no later launch',async t=>{
  const old=process.env.OPENROUTER_API_KEY;process.env.OPENROUTER_API_KEY='synthetic';t.after(()=>{if(old===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=old;});
  for(const mode of ['timeout','unreaped','bad-report']){
    const f=fixture(t);let calls=0;
    const controls={platform:'win32',validate:()=>({plan:f.plan,root:f.root}),inputs:()=>{},execute:async()=>{calls++;return {status:0,error:mode==='timeout'?'deadline_exceeded':null,process_reaped:mode!=='unreaped',stdout:'',stderr:''};}};
    const result=await renewal.run('proposal','new',controls);assert.equal(result.actual_cost_micros,null);assert.equal(result.status,'failed');
    const b=JSON.parse(fs.readFileSync(f.plan.campaign));assert.equal(b.reserved_micros,32000000);assert.equal(b.settled_micros,1000);
    assert.equal(b.runs[1].status,mode==='unreaped'?'running-held':'failed-unknown');
    await assert.rejects(renewal.run('proposal','new',controls),/EEXIST/);assert.equal(calls,1);
  }
});
test('freshness refuses future, expired, insufficient-margin and extended evidence',()=>{
  const now=1000000,good={observed_at:'900000',valid_until:'1400000'};
  assert.doesNotThrow(()=>renewal.freshness(good,now));
  for(const spec of [{...good,observed_at:'1000001'},{...good,valid_until:'1300000'},{...good,valid_until:'1'},{...good,valid_until:String(900000+86400001)},{...good,observed_at:900000}])assert.throws(()=>renewal.freshness(spec,now),/Fresh/);
});
test('admission-time expiry stops before execution and permanently holds claimed reservation',async t=>{
  const f=fixture(t),old=process.env.OPENROUTER_API_KEY;process.env.OPENROUTER_API_KEY='synthetic';t.after(()=>{if(old===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=old;});
  let calls=0;
  const result=await renewal.run('proposal','new',{platform:'win32',validate:()=>({plan:f.plan,root:f.root}),inputs:()=>{},changeCampaign:(file,change)=>{
    const b=JSON.parse(fs.readFileSync(file));change(b);put(file,b);
    const spec=JSON.parse(fs.readFileSync(f.plan.spec.path));spec.valid_until=String(Date.now()+1000);put(f.plan.spec.path,spec);
  },execute:async()=>{calls++;throw Error('Must not execute');}});
  assert.equal(calls,0);assert.equal(result.reserved,true);assert.match(result.failures[0],/Fresh/);assert.equal(JSON.parse(fs.readFileSync(f.plan.campaign)).reserved_micros,32000000);
});
test('malformed provider JSON cannot leak credential through result or diagnostics',async t=>{
  const f=fixture(t),old=process.env.OPENROUTER_API_KEY;process.env.OPENROUTER_API_KEY='synthetic-secret';t.after(()=>{if(old===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=old;});
  const result=await renewal.run('proposal','new',{platform:'win32',validate:()=>({plan:f.plan,root:f.root}),inputs:()=>{},execute:async()=>{
    fs.mkdirSync(f.plan.output);fs.writeFileSync(path.join(f.plan.output,'result.json'),'{"secret":"synthetic-secret", BROKEN');
    return {status:0,error:null,process_reaped:true,stdout:'synthetic-secret',stderr:'synthetic-secret'};
  }});
  assert.deepEqual(result.failures,['Invalid bounded JSON input']);
  for(const file of ['renewal-result.json','execution.json'])assert.ok(!fs.readFileSync(path.join(f.root,file),'utf8').includes('synthetic-secret'));
  assert.equal(JSON.parse(fs.readFileSync(f.plan.campaign)).reserved_micros,32000000);
});
test('pinned inputs and every loaded repository helper are checked independently',t=>{
  const f=fixture(t),exe=path.join(f.root,'fake.exe'),catalog=path.join(f.root,'endpoints.json'),proposal=path.join(f.root,'proposal.json');fs.writeFileSync(exe,'fake');put(catalog,{});
  f.plan.executable={path:exe,sha256:sha(fs.readFileSync(exe))};f.plan.catalog={path:catalog,sha256:sha(fs.readFileSync(catalog))};
  f.plan.runner_hashes=Object.fromEntries(renewal.runnerFiles().map(file=>[file,sha(fs.readFileSync(file))]));put(proposal,f.plan);const digest=sha(fs.readFileSync(proposal));
  renewal.inputs(f.plan,proposal,digest);
  fs.appendFileSync(catalog,' ');assert.throws(()=>renewal.inputs(f.plan,proposal,digest),/Pinned/);put(catalog,{});
  assert.ok(renewal.runnerFiles().includes(__filename),'loaded repository code outside scripts and src/evals included');delete f.plan.runner_hashes[__filename];assert.throws(()=>renewal.inputs(f.plan,proposal,digest),/dependency/);
});
