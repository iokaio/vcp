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
    provider:{valid_until:String(Date.now()+3600000),max_output:'512',price:{currency:'USD'},compatibility:{valid_until:String(Date.now()+3600000),responses_text_tools:true,provider_preferences_qualified:true}},
    catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'512',max_transport_retries:0,max_requests:8,deadline_seconds:60};
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
