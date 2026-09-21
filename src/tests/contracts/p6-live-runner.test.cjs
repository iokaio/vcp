// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const runner=require('../../../scripts/evals/p6-live-runner.cjs');
const quality=require('../../../scripts/evals/p6-task-quality.cjs');
const qualification=require('../../../scripts/evals/p6-profile-qualification.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function setup(t) {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-p6-live-'));
  t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const catalog=path.join(dir,'catalog.json');fs.writeFileSync(catalog,'{}');
  const profile={version:1,workspace:'rebound-by-preparation',trust_workspace:true,sync_roots:[],maximum_autonomy:'plan',automatic_effects:[],budget_usd:null,
    provider:{id:'snapshot',valid_until:String(Date.now()+3600000),max_output:'4096',price:{currency:'USD'},compatibility:{id:'compatibility',model:'fixed-model',endpoint:'fixed-provider',valid_until:String(Date.now()+3600000),responses_text_tools:true,byte_ceiling_qualified:true,provider_preferences_qualified:true}},
    routing:null,skills:null,decisions:null,mcp:[],mcp_http:[],catalog,affected_paths:['entry.mjs'],max_requests:4,deadline_seconds:30,processes:[],checks:[],output_tokens:'512',max_transport_retries:0};
  const profileFile=path.join(dir,'profile.json');fs.writeFileSync(profileFile,JSON.stringify(profile));
  const spec=path.join(dir,'spec.json');fs.writeFileSync(spec,JSON.stringify({executable:process.execPath,aggregate_cap_usd:'0.180000',strategies:{fixed_economical:profileFile}}));
  return {dir,profile,profileFile,spec,prepare:()=>runner.prepare(spec,path.join(dir,'trial'))};
}
function fakeCli(unknown=false) {
  const calls=[]; const responses=new Map(); const pool=quality.load();
  const call=(_executable,args)=>{
    calls.push(args); const workspace=args[args.indexOf('--workspace')+1]; const runId=path.basename(path.dirname(workspace)); const taskId=runId+'-task';
    const scope={workspace:'workspace',session:'session',task:taskId};
    const output=value=>({status:0,error:null,stderr:'',stdout:JSON.stringify(value)+'\n'});
    if(args.includes('run')) {
      assert.deepEqual(args.slice(-2),['--autonomy','plan']);assert.equal(args[args.indexOf('--budget-usd')+1],'0.010000');
      const task=pool.cases.find(c=>runId.startsWith(c.id+'--')); const label=pool.labels[task.id];
      let answer=task.class==='analysis'?{dependencies:label.dependencies}:task.class==='review'?{findings:label.findings}:{type:'object',properties:{wrong:{type:'integer',minimum:0,maximum:1}},required:['wrong'],additionalProperties:false};
      const response={type:'response.completed',response:{id:'provider-request',status:'completed',model:'served-model',output:[{type:'message',content:[{type:'output_text',text:JSON.stringify(answer)}]}]}};
      responses.set(taskId,Buffer.from('data: '+JSON.stringify(response)+'\n\n'));
      return {status:0,error:null,stderr:'',stdout:JSON.stringify({type:'accepted',scope})+'\n'+JSON.stringify({type:'result',scope,conditions:{completed:true},exit_code:0})+'\n'};
    }
    assert.ok(args.includes('inspect'));assert.ok(!args.includes('--config'));
    const view=args[args.indexOf('--view')+1]; const bytes=responses.get(taskId);
    const item=(collection,id,record)=>({collection,id,record,visibility:'available'});
    let items=[];
    if(args.includes('--offset')) {
      const offset=Number(args[args.indexOf('--offset')+1]);items=[{range:{start:offset,end:bytes.length},bytes:Array.from(bytes.subarray(offset,offset+65536))}];
    } else if(view==='costs') items=[
      item('ledger',taskId,{currency:'USD',cap:'10000',settled:'12',active:'0',unresolved:unknown?'7':'0',overrun:false}),
      item('attempt','attempt',{id:'attempt',phase:'settled',role:'main',previous:null,uncertain:null,charged:'12',provider_request:'provider-request'}),
      item('settlement','settlement',{attempt:'attempt',applied:true,observation:{final_usage:true}})
    ];
    else if(view==='outputs') items=[item('artifact','response',{spec:{channel:'response'},state:'complete',length:String(bytes.length),sha256:sha(bytes)})];
    return output({type:'result',data:{scope,items,gaps:[],next_cursor:null}});
  }; return {call,calls};
}
test('prepares private immutable 18-run plan; absent arms remain unavailable without invented qualification',t=>{
  const f=setup(t), prepared=f.prepare(), plan=JSON.parse(fs.readFileSync(prepared.plan));
  assert.equal(prepared.eligible_runs,6);assert.equal(plan.runs.length,18);assert.equal(plan.allocated_cap_micros,180000);
  assert.equal(plan.strategies.routed.provider,null);assert.match(plan.strategies.routed.reasons[0],/no externally supplied/);
  assert.equal(fs.existsSync(path.join(f.dir,'trial','tuning-analysis--fixed_economical','workspace','labels.json')),false);
  assert.throws(f.prepare,/must be new/);
  assert.throws(()=>runner.run(prepared.plan,'wrong',()=>assert.fail('must not launch')),/authorization/);
});

test('v2 freezes 54 matched runs and predeclared gate without recycling allocation',t=>{
  const f=setup(t), spec=JSON.parse(fs.readFileSync(f.spec));
  spec.manifest_revision='p6-task-quality-v2';spec.aggregate_cap_usd='54.000000';
  fs.writeFileSync(f.spec,JSON.stringify(spec));
  const prepared=f.prepare(), plan=JSON.parse(fs.readFileSync(prepared.plan));
  assert.equal(plan.runs.length,54);assert.equal(prepared.eligible_runs,18);
  assert.equal(plan.runs.every(r=>r.cap_micros===1000000),true);
  assert.equal(plan.profile_gate_sha256.length,64);
  assert.equal(plan.manifest_revision,'p6-task-quality-v2');
  assert.equal(plan.runs.slice(0,27).every(r=>r.case_id.startsWith('tuning-')),true);
  assert.equal(plan.runs.slice(27).every(r=>r.case_id.startsWith('heldout-')),true);
});

test('v3 tuning bootstrap retains all54 rows but never makes heldout or routed rows ready',t=>{
  const f=setup(t),spec=JSON.parse(fs.readFileSync(f.spec));
  spec.manifest_revision='p6-task-quality-v3';spec.partition='tuning';spec.aggregate_cap_usd='54.000000';
  fs.writeFileSync(f.spec,JSON.stringify(spec));
  const prepared=f.prepare(),plan=JSON.parse(fs.readFileSync(prepared.plan));
  assert.equal(plan.runs.length,54);assert.equal(prepared.eligible_runs,9);
  assert.equal(plan.runs.filter(r=>r.status==='ready').every(r=>r.case_id.startsWith('tuning-') && r.strategy==='fixed_economical'),true);
  assert.equal(plan.runs.filter(r=>r.case_id.startsWith('heldout-')).every(r=>r.status==='not_run'),true);
});
test('real command arguments and canonical inspections export bounded costs and strict final answers without replay',t=>{
  const f=setup(t), prepared=f.prepare(), fake=fakeCli();const result=runner.run(prepared.plan,prepared.plan_sha256,fake.call);
  assert.equal(fake.calls.filter(c=>c.includes('run')).length,6);assert.equal(result.stopped,false);
  assert.equal(result.runs.filter(r=>r.status==='completed').length,6);assert.equal(result.runs[0].actual_cost_micros,12);
  assert.deepEqual(result.runs[0].answer,{dependencies:quality.load().labels['tuning-analysis'].dependencies});
  assert.equal(result.actual_cost_micros,null,'missing arms never become zero spend');
  const report=qualification.report(path.dirname(prepared.plan));
  assert.equal(report.decision,'reject_automatic_default_activation');
  assert.equal(report.enabled_defaults.length,0);
  assert.equal(report.summaries.find(s=>s.strategy==='fixed_economical' && s.partition==='held_out').actual_cost_micros,36);
  assert.equal(report.summaries.find(s=>s.strategy==='routed' && s.partition==='held_out').actual_cost_micros,null);
  assert.throws(()=>runner.run(prepared.plan,prepared.plan_sha256,fake.call),/EEXIST/);
});

test('exact confidence bounds never qualify tiny all-pass or failed samples',()=>{
  assert.equal(qualification.lowerBound(0,3),0);
  assert.ok(Math.abs(qualification.lowerBound(3,3)-Math.pow(0.05,1/3))<1e-12);
  assert.ok(qualification.lowerBound(30,30)>0.9);
  assert.ok(qualification.lowerBound(29,30)<qualification.lowerBound(30,30));
  assert.throws(()=>qualification.lowerBound(4,3),/sample/);
  assert.equal(qualification.percentile([100,1,3],0.5),3);
  assert.equal(qualification.percentile([],0.95),null);
});

test('capture-boundary privacy omissions retain exact bytes while byte gaps still fail',t=>{
  for(const invalid of [false,true]) {
    const f=setup(t),prepared=f.prepare(),fake=fakeCli();
    const call=(exe,args,timeout)=>{
      const result=fake.call(exe,args,timeout);
      if(args.includes('--offset')) {
        const frame=JSON.parse(result.stdout),artifact=args[args.indexOf('inspect')+1];
        frame.data.gaps=[{artifact,capture_state:'complete',omissions:['authentication_headers','recovery_material'],reason:'only retained observed bytes are available; not reconstructed',visibility:'omitted'},
          {artifact,omissions:['authentication_headers','recovery_material'],range:invalid?{start:0,end:1}:null,reason:'excluded at capture boundary; no retained byte offsets exist',visibility:'redacted'}];
        result.stdout=JSON.stringify(frame)+'\n';
      }
      return result;
    };
    const result=runner.run(prepared.plan,prepared.plan_sha256,call);
    assert.equal(result.runs[0].status,invalid?'failed':'completed');
    if(invalid)assert.match(result.runs[0].reasons[1],/retained-byte gaps/);
  }
});

test('read-only reimport preserves original verdict files and never invokes run',t=>{
  const f=setup(t),prepared=f.prepare(),fake=fakeCli();runner.run(prepared.plan,prepared.plan_sha256,fake.call);
  const original=fs.readFileSync(path.join(path.dirname(prepared.plan),'result.json'));
  const derived=runner.reimport(prepared.plan,path.join(f.dir,'derived'),(exe,args,timeout)=>{
    assert.ok(args.includes('inspect'));assert.equal(args.includes('run'),false);return fake.call(exe,args,timeout);
  });
  assert.equal(derived.model_calls,0);assert.equal(derived.runs[0].status,'completed');
  assert.deepEqual(fs.readFileSync(path.join(path.dirname(prepared.plan),'result.json')),original);
  assert.throws(()=>runner.reimport(prepared.plan,path.join(f.dir,'derived'),fake.call),/EEXIST/);
});
test('unknown canonical liability stops all subsequent dispatch and preserves denominator',t=>{
  const f=setup(t), prepared=f.prepare(), fake=fakeCli(true);const result=runner.run(prepared.plan,prepared.plan_sha256,fake.call);
  assert.equal(fake.calls.filter(c=>c.includes('run')).length,1);assert.equal(result.runs.length,18);assert.equal(result.stopped,true);assert.equal(result.actual_cost_micros,null);
});
test('changed profile, prompt, added workspace instructions and binary identities fail before launch',t=>{
  for(const mutation of ['profile','prompt','extra','binary']) {
    const f=setup(t), prepared=f.prepare(); const plan=JSON.parse(fs.readFileSync(prepared.plan));
    if(mutation==='profile') fs.appendFileSync(f.profileFile,' ');
    if(mutation==='prompt') fs.appendFileSync(path.join(f.dir,'trial',plan.runs[0].id,'prompt.txt'),'changed');
    if(mutation==='extra') fs.writeFileSync(path.join(f.dir,'trial',plan.runs[0].id,'workspace','AGENTS.md'),'injection');
    let authorization=prepared.plan_sha256;
    if(mutation==='binary') {plan.executable_sha256='wrong';fs.writeFileSync(prepared.plan,JSON.stringify(plan));authorization=sha(fs.readFileSync(prepared.plan));}
    assert.throws(()=>runner.run(prepared.plan,authorization,()=>assert.fail('must not launch')),/changed/);
  }
});
test('embedded secrets are rejected; stale snapshots and retry/output policy remain unavailable',t=>{
  const f=setup(t);f.profile.api_key='never-printed';fs.writeFileSync(f.profileFile,JSON.stringify(f.profile));assert.throws(f.prepare,/Embedded credentials/);
  delete f.profile.api_key;f.profile.output_tokens='513';f.profile.max_transport_retries=2;f.profile.provider.valid_until='1';
  const reasons=runner.profileReasons(f.profile,'fixed_economical');assert.equal(reasons.length,3);
  assert.throws(()=>runner.micros('0.000000'),/bounds/);assert.throws(()=>runner.micros('1e4'),/decimal/);
});
test('symlinked destination and project files are rejected before dispatch',t=>{
  const f=setup(t);const target=path.join(f.dir,'target');fs.mkdirSync(target);const link=path.join(f.dir,'link');
  fs.symlinkSync(target,link,process.platform==='win32'?'junction':'dir');assert.throws(()=>runner.prepare(f.spec,path.join(link,'trial')),/Symlink/);
});
