// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const runner=require('../../../scripts/evals/p6-live-runner.cjs'), quality=require('../../../scripts/evals/p6-task-quality.cjs'), builder=require('../../../scripts/evals/p6-bootstrap-profiles.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function setup(t,revision='p6-task-quality-v1') {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-p6-bootstrap-'));t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const now=Date.now(), until=now+3600000, strategies={}, refs={};
  const write=(name,obj)=>{const file=path.join(dir,name);fs.writeFileSync(file,JSON.stringify(obj));return {path:file,sha256:sha(fs.readFileSync(file))};};
  for(const id of ['fixed_economical','fixed_stronger']) {
    const catalog=write(id+'-catalog.json',{synthetic:true,model:id});
    const probe=write(id+'-probe.json',{schema:'p6-provider-qualification/1',authorized_sources_sha256:sha(id),attribution:[0,1].map(n=>({response_id:id+n,requested_model:id,observed_model_revision:id+'-rev',catalog_endpoint:'provider',provider_name:'Provider',observed_endpoint_id:'region',generation_sha256:sha(String(n)),method:'synthetic-test-only'}))});
    const profile=write(id+'.json',{version:1,trust_workspace:true,catalog:catalog.path,max_requests:4,deadline_seconds:300,max_transport_retries:0,output_tokens:'512',provider:{id:id+'-snapshot',raw_sha256:catalog.sha256,observed_at:String(now),valid_until:String(until),max_input:'200000',max_output:'4096',price:{currency:'USD'},compatibility:{id:'p6-generation-qualified/'+sha(id),model:id,endpoint:'provider',valid_until:String(until),responses_text_tools:true,provider_preferences_qualified:true,byte_ceiling_qualified:false,require_zdr:false}},routing:null});
    strategies[id]=profile.path;refs[id]={profile,probe};
  }
  const spec=write('runner-spec.json',{executable:process.execPath,aggregate_cap_usd:'18.000000',strategies,...(revision==='p6-task-quality-v3'?{manifest_revision:revision,partition:'tuning'}:{})});
  const prepared=runner.prepare(spec.path,path.join(dir,'trial')), pool=quality.load(revision);
  const runs=quality.prepare(pool).runs.map(r=>({case_id:r.case_id,strategy:r.strategy,start_state_sha256:r.start_state_sha256,status:r.strategy==='routed'?'not_run':'completed',answer:r.task_class==='analysis'?{dependencies:pool.labels[r.case_id].dependencies}:r.task_class==='review'?{findings:pool.labels[r.case_id].findings}:{type:'object',properties:{[r.partition==='tuning'?'count':'offset']:{type:'integer',minimum:r.partition==='tuning'?0:-2,maximum:r.partition==='tuning'?3:2}},required:[r.partition==='tuning'?'count':'offset'],additionalProperties:false},actual_cost_micros:r.strategy==='routed'?null:10,latency_ms:r.strategy==='routed'?null:100}));
  const result={schema:'p6-live-result/1',plan_sha256:prepared.plan_sha256,manifest_sha256:pool.manifest_sha256,authorization:true,stopped:false,runs};
  fs.writeFileSync(path.join(dir,'trial','result.json'),JSON.stringify(result));
  return {dir,result,spec:{bootstrap_directory:path.join(dir,'trial'),observed_at:String(now+1000),valid_until:String(until-1),deadline:String(now+300000),strategies:refs}};
}
test('bootstrap uses tuning only and preserves exact full-input snapshot bounds and strict pins',t=>{
  const f=setup(t), bundle=builder.build(f.spec);
  assert.equal(bundle.shipping_defaults,false);assert.equal(bundle.training.every(x=>x.samples===3&&x.quality_bps===10000),true);
  const routed=bundle.profiles.routed.routing;
  assert.equal(routed.policy.pin,null);assert.equal(routed.policy.broader_task_class,null);assert.equal(routed.policy.minimum_samples,3);
  assert.equal(routed.catalog.entries.every(e=>e.snapshot.compatibility.byte_ceiling_qualified===false),true);
  assert.equal(routed.estimates.every(e=>e.first_attempt.input==='200000' && e.first_attempt.cache_read==='0'),true);
  assert.equal(bundle.profiles.fixed_stronger.routing,null);
  for(const r of f.result.runs)if(r.case_id.startsWith('heldout'))r.answer=null;
  fs.writeFileSync(path.join(f.spec.bootstrap_directory,'result.json'),JSON.stringify(f.result));
  const changed=builder.build(f.spec);
  assert.deepEqual(changed.training.map(x=>x.quality_bps),bundle.training.map(x=>x.quality_bps));
});
test('failed tuning remains in denominator and missing charge or forged probe cannot qualify',t=>{
  const f=setup(t);f.result.runs[0].answer=null;
  fs.writeFileSync(path.join(f.spec.bootstrap_directory,'result.json'),JSON.stringify(f.result));
  const rejected=builder.build(f.spec);
  assert.equal(rejected.training[0].eligible,false);
  assert.equal(rejected.profiles.fixed_economical.routing,null,'failed baseline remains independently measurable');
  f.result.runs[0].actual_cost_micros=null;fs.writeFileSync(path.join(f.spec.bootstrap_directory,'result.json'),JSON.stringify(f.result));
  assert.throws(()=>builder.build(f.spec),/known costs/);
  f.spec.strategies.fixed_economical.probe.sha256='0'.repeat(64);
  assert.throws(()=>builder.build(f.spec),/hash changed/);
});
test('canonical seals ignore object insertion order and bind policy changes',()=>{
  assert.equal(builder.seal({id:'',b:2,a:{z:1,c:3}}).id,builder.seal({a:{c:3,z:1},b:2,id:''}).id);
  assert.notEqual(builder.seal({id:'',quality_floor_bps:8000}).id,builder.seal({id:'',quality_floor_bps:7999}).id);
});
test('v3 membership requires all nine tuning rows and keeps minimum9 without heldout fallback',t=>{
  const f=setup(t,'p6-task-quality-v3'),bundle=builder.build(f.spec);
  assert.equal(bundle.training.every(r=>r.samples===9),true);
  assert.equal(bundle.profiles.routed.routing.policy.minimum_samples,9);
  assert.equal(bundle.profiles.routed.routing.policy.broader_task_class,null);
  assert.equal(bundle.training.every(r=>r.case_ids.every(id=>id.startsWith('tuning-'))),true);
  assert.equal(bundle.profiles.fixed_economical.routing,null);
});
