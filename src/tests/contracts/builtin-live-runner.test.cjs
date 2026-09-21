// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const runner=require('../../../scripts/evals/builtin-live-runner.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function setup(t) {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-builtin-live-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const executable=path.join(root,'vcp.exe');fs.writeFileSync(executable,'test executable; never launched');
  fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),path.join(root,'skills/builtin'),{recursive:true});
  const catalog=path.join(root,'catalog.json');fs.writeFileSync(catalog,'{}');
  const profile={version:1,trust_workspace:true,maximum_autonomy:'plan',automatic_effects:[],workspace:'rebound',sync_roots:[],
    provider:{valid_until:String(Date.now()+3600000),max_output:'8192',price:{currency:'USD',valid_until:String(Date.now()+3600000)},compatibility:{valid_until:String(Date.now()+3600000),responses_text_tools:true,provider_preferences_qualified:true}},
    catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'4096',max_transport_retries:0,max_requests:16,deadline_seconds:900};
  const profileFile=path.join(root,'profile.json');fs.writeFileSync(profileFile,JSON.stringify(profile));
  const spec=path.join(root,'spec.json');fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'1.600000'}));
  return {root,executable,profile,profileFile,spec,prepare:()=>runner.prepare(spec,path.join(root,'trial'))};
}
test('prepares paired frozen copies without answers, model calls or authority expansion',t=>{
  const f=setup(t), prepared=f.prepare(), plan=JSON.parse(fs.readFileSync(prepared.plan));
  assert.equal(prepared.model_calls,0);assert.equal(plan.runs.length,16);assert.equal(plan.allocated_cap_micros,1600000);
  for(let i=0;i<16;i+=2) {
    const baseline=plan.runs[i], skill=plan.runs[i+1];
    assert.deepEqual(baseline.files,skill.files);assert.equal(baseline.prompt_sha256,skill.prompt_sha256);assert.equal(skill.cap_micros,100000);
    const base=path.join(plan.directory,skill.id);
    assert.equal(fs.existsSync(path.join(base,'workspace','manifest.json')),false);
    const profile=JSON.parse(fs.readFileSync(path.join(base,'profile.json')));
    assert.equal(profile.maximum_autonomy,'plan');assert.deepEqual(profile.processes,[]);
  }
  runner.validate(plan,prepared.plan);
  assert.throws(f.prepare,/New private/);
  assert.throws(()=>runner.run(prepared.plan,'incorrect',()=>assert.fail('must not dispatch')),/Authorization/);
});
test('rejects changed fixtures, package bytes, profiles and reused stores before dispatch',t=>{
  const f=setup(t), prepared=f.prepare(), plan=JSON.parse(fs.readFileSync(prepared.plan));
  const file=path.join(plan.directory,plan.runs[0].id,'workspace','architecture.md');
  const original=fs.readFileSync(file);fs.appendFileSync(file,'changed');
  assert.throws(()=>runner.run(prepared.plan,prepared.sha256,()=>assert.fail()),/inputs changed/);
  fs.writeFileSync(file,original);
  const asset=path.join(f.root,'skills/builtin/architecture/SKILL.md');const body=fs.readFileSync(asset);fs.appendFileSync(asset,'changed');
  assert.throws(()=>runner.run(prepared.plan,prepared.sha256,()=>assert.fail()),/packaged skill/);fs.writeFileSync(asset,body);
  fs.writeFileSync(path.join(plan.directory,plan.runs[0].id,'data','old'),'old');
  assert.throws(()=>runner.run(prepared.plan,prepared.sha256,()=>assert.fail()),/store is not fresh/);
});
test('unknown first-run liability stops dispatch, retains denominator and prevents replay',t=>{
  const f=setup(t), prepared=f.prepare();let calls=0;
  const result=runner.run(prepared.plan,prepared.sha256,(_exe,args)=>{
    calls++;assert.ok(!args.includes('--skill'));
    return {status:null,error:'ETIMEDOUT',stdout:'',stderr:''};
  });
  assert.equal(calls,1);assert.equal(result.stopped,true);assert.equal(result.actual_cost_micros,null);
  assert.equal(result.runs.length,16);assert.equal(result.runs.filter(r=>r.status==='not_run').length,15);
  assert.throws(()=>runner.run(prepared.plan,prepared.sha256,()=>assert.fail()),/EEXIST/);
});
test('unsafe provider profiles are rejected without creating trial state',t=>{
  const f=setup(t);f.profile.processes=[{name:'unauthorized'}];fs.writeFileSync(f.profileFile,JSON.stringify(f.profile));
  assert.throws(f.prepare,/external tools/);assert.equal(fs.existsSync(path.join(f.root,'trial')),false);
});

test('P7 coding bounds are independent of frozen P6 and reject invalid capacity before preparation',t=>{
  const f=setup(t),now=Date.now(),valid=f.profile;
  assert.deepEqual(runner.fixedProfileReasons(valid,now),[]);
  assert.deepEqual(runner.profileReasons(valid,now),[]);
  const p6=require('../../../scripts/evals/p6-live-runner.cjs');
  assert.ok(p6.profileReasons(valid,'fixed_economical',now).some(reason=>reason.includes('512-token')));
  assert.deepEqual(p6.profileReasons({...valid,output_tokens:'512',max_requests:8,deadline_seconds:600},'fixed_economical',now),[]);
  for(const overrides of [
    {max_requests:0},{max_requests:17},{max_requests:1.5},
    {deadline_seconds:0},{deadline_seconds:1801},{deadline_seconds:NaN},
    {output_tokens:undefined},{output_tokens:'0'},{output_tokens:'8193'},
    {output_tokens:'04096'},{output_tokens:4096},{output_tokens:'Infinity'},
    {output_tokens:'9007199254740992'},
    {provider:{...valid.provider,max_output:'4095'}},
    {provider:{...valid.provider,valid_until:undefined}},
    {provider:{...valid.provider,valid_until:'NaN'}},
    {provider:{...valid.provider,valid_until:String(now)}},
    {provider:{...valid.provider,compatibility:{...valid.provider.compatibility,valid_until:String(now)}}},
    {provider:{...valid.provider,price:{...valid.provider.price,valid_until:String(now)}}},
    {provider:{...valid.provider,price:{currency:'USD'}}},
    {provider:{...valid.provider,price:{currency:'EUR'}}},
    {provider:{...valid.provider,compatibility:{...valid.provider.compatibility,responses_text_tools:false}}},
    {provider:{...valid.provider,compatibility:{...valid.provider.compatibility,provider_preferences_qualified:false}}},
    {max_requests:1,max_transport_retries:undefined},{max_transport_retries:1},
    {routing:{}},{checks:[{}]},{processes:[{}]},{mcp:[{}]},{mcp_http:[{}]},
    {skills:{}},{decisions:{}},{qualification_endpoint:'https://not-authorized.invalid'},
    {maximum_autonomy:'workspace'},{automatic_effects:['read','write']},
  ]) {
    const changed={...valid,...overrides};
    assert.ok(runner.profileReasons(changed,now).length,JSON.stringify(overrides));
    fs.writeFileSync(f.profileFile,JSON.stringify(changed));
    assert.throws(f.prepare);
    assert.equal(fs.existsSync(path.join(f.root,'trial')),false);
  }
  for(const [max_requests,deadline_seconds,output_tokens]of [[1,1,'1'],[16,1800,'8192']])assert.deepEqual(runner.profileReasons({...valid,max_requests,deadline_seconds,output_tokens},now),[]);
});

test('runner revision changes require a fresh exact plan before any dispatch',t=>{
  const f=setup(t),prepared=f.prepare(),plan=JSON.parse(fs.readFileSync(prepared.plan));
  plan.runner_sha256='0'.repeat(64);fs.writeFileSync(prepared.plan,JSON.stringify(plan));
  assert.throws(()=>runner.run(prepared.plan,sha(fs.readFileSync(prepared.plan)),()=>assert.fail('must not dispatch historical plan')),/Prepared source/);
  assert.equal(fs.existsSync(path.join(plan.directory,'execution-claim.json')),false);
});
test('all allocated attempts have frozen identities even if a new plan hash is supplied',t=>{
  const f=setup(t), prepared=f.prepare(), plan=JSON.parse(fs.readFileSync(prepared.plan));
  plan.runs[0].cap_micros++;
  fs.writeFileSync(prepared.plan,JSON.stringify(plan));
  assert.throws(()=>runner.run(prepared.plan,sha(fs.readFileSync(prepared.plan)),()=>assert.fail()),/allocation changed/);
});
test('skill evidence binds exact body to each dispatched request and rejects baseline contamination',()=>{
  const body=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../skills/builtin/catalog.json'))).skills.find(s=>s.id==='architecture').body.sha256;
  const manifest={request_sha256:'request-digest',included:[{kind:'skill',trust:'active_skill',source_hash:body,id:'skill-'+sha(Buffer.from('vcp-builtin::architecture::architecture'))+'-0'}]};
  const bytes=Buffer.from(JSON.stringify(manifest));
  const pages=[{gaps:[],items:[{id:'context',collection:'artifact',record:{state:'complete',length:String(bytes.length),sha256:sha(bytes),spec:{schema:'context-manifest/1'}}}]}];
  const call=()=>({status:0,stdout:JSON.stringify({type:'result',data:{items:[{range:{start:0,end:bytes.length},bytes:[...bytes]}],gaps:[],next_cursor:null}})});
  const attempt={phase:'settled',request_digest:'request-digest'};
  assert.equal(runner.skillEvidence({executable:'unused'},os.tmpdir(),{skill:'architecture',arm:'skill'},pages,[attempt],call).checked_attempts,1);
  assert.throws(()=>runner.skillEvidence({executable:'unused'},os.tmpdir(),{skill:'architecture',arm:'baseline'},pages,[attempt],call),/differs from paired arm/);
  assert.throws(()=>runner.skillEvidence({executable:'unused'},os.tmpdir(),{skill:'architecture',arm:'skill'},pages,[{...attempt,request_digest:'other-request'}],call),/No canonical context/);
});
