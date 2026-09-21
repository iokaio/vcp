// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawnSync}=require('node:child_process');
const prep=require('../../../scripts/evals/builtin-debug-prepare.cjs'),runner=require('../../../scripts/evals/builtin-debug-runner.cjs'),oracle=require('../../../scripts/evals/builtin-debug-v2-oracle.cjs');
const {ownedRoot}=require('../support/experiments.cjs');
function sourceProfile(catalog){const expiry=String(Date.now()+3600000);return {version:1,trust_workspace:true,maximum_autonomy:'workspace',automatic_effects:['read','write'],workspace:'rebound',sync_roots:[],provider:{valid_until:expiry,max_output:'8192',price:{currency:'USD',valid_until:expiry},compatibility:{valid_until:expiry,responses_text_tools:true,provider_preferences_qualified:true}},catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'4096',max_transport_retries:0,max_requests:16,deadline_seconds:600};}
function fixture(t){const temp=ownedRoot(os.tmpdir());t.after(()=>temp.cleanup());const root=temp.root,executable=path.join(root,'vcp.exe'),catalog=path.join(root,'catalog.json'),profileFile=path.join(root,'profile.json'),spec=path.join(root,'spec.json');fs.writeFileSync(executable,'not executed');fs.writeFileSync(catalog,'{}');fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),path.join(root,'skills/builtin'),{recursive:true});const profile=sourceProfile(catalog);fs.writeFileSync(profileFile,JSON.stringify(profile));fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'0.400003'}));return {root,executable,catalog,profileFile,profile,spec};}
test('debug v2 adds canonical check inputs without modifying v1 scenarios or source',()=>{
  const v1=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../evals/skills/builtin/debug-v1/manifest.json'))),v2=prep.pool().manifest;
  assert.deepEqual(v2.cases,v1.cases);assert.equal(v1.files['shipping.test.cjs'],undefined);assert.equal(JSON.parse(v1.files['package.json']).scripts,undefined);
  assert.equal(v2.parent_revision,v1.revision);assert.equal(v2.files['notes.txt'],v1.files['notes.txt']);assert.equal(JSON.parse(v2.files['package.json']).scripts.test,'node --test shipping.test.cjs');
});
test('debug preparation binds four selected skill scenarios without calls or implicit process authority',t=>{
  const f=fixture(t),made=prep.prepare(f.spec,path.join(f.root,'trial')),plan=JSON.parse(fs.readFileSync(made.plan));
  assert.equal(made.model_calls,0);assert.equal(made.runnable,false);assert.equal(plan.authorization,false);assert.equal(plan.allocated_cap_micros,400000);assert.equal(plan.runs.length,4);
  for(const row of plan.runs){assert.equal(row.arm,'skill');assert.equal(row.skill,'review-debug');const base=path.join(plan.directory,row.id),profile=JSON.parse(fs.readFileSync(path.join(base,'profile.json')));assert.deepEqual(profile.processes,[]);assert.deepEqual(profile.checks,[]);assert.deepEqual(profile.automatic_effects,['read','write']);assert.equal(fs.existsSync(path.join(base,'workspace/manifest.json')),false);}
  assert.throws(()=>runner.run(made.plan,'wrong',()=>assert.fail('no dispatch')),/exact prepared plan hash/);
  assert.throws(()=>runner.run(made.plan,made.sha256,()=>assert.fail('no dispatch')),/Runnable qualified/);
  f.profile.processes=[{name:'broad'}];fs.writeFileSync(f.profileFile,JSON.stringify(f.profile));assert.throws(()=>prep.prepare(f.spec,path.join(f.root,'denied')),/outside the source profile/);assert.equal(fs.existsSync(path.join(f.root,'denied')),false);
});
test('debug available reproduction binds a current Node requirement while missing access has none',()=>{
  const profile=sourceProfile('catalog'),runtime={launcher:'launcher'};
  for(const scenario of prep.pool().manifest.cases){const derived=prep.qualifiedProfile(profile,'workspace','catalog',100000,runtime,true,scenario);if(scenario.reproduction==='available'){assert.equal(derived.processes.length,1);assert.equal(derived.checks[0].manifest,'package.json');assert.equal(derived.checks[0].timeout_ms,10000);assert.deepEqual(derived.checks[0].expected_tests,['shipping fee threshold includes 50']);}else{assert.deepEqual(derived.processes,[]);assert.deepEqual(derived.checks,[]);assert.equal(derived.maximum_autonomy,'workspace');assert.deepEqual(derived.automatic_effects,['read','write']);}}
});
function receipts(){
  const row={reproduction:'available',files:{'shipping.cjs':'original'}};
  const captures=[['before',1,'original','not ok 1 - shipping fee threshold includes 50'],['after',0,'current','ok 1 - shipping fee threshold includes 50']].map(([artifact,exit_code,sha256,tail])=>({artifact,scope:{task:'task'},receipt:{effect:artifact+'-effect',execution:artifact+'-execution',exit_code,output_complete:true,stop_reason:null,owned_processes_remaining:0,observed_workspace:{complete:true,sources:[{path:'shipping.cjs',sha256}]},presentation:{stdout:{omitted_bytes:0,replacement_characters:0,tail}}}}));
  const pages=[{gaps:[],items:captures.map(c=>({collection:'effect',visibility:'available',record:{id:c.receipt.effect,execution:c.receipt.execution,scope:{task:'task'},state:c.receipt.exit_code===0?'succeeded':'failed',exit_code:c.receipt.exit_code,observed_changes:[c.artifact]}}))}];return {row,captures,pages};
}
test('native debug evidence requires canonical original failure and current-source passing receipt',()=>{
  const f=receipts();assert.equal(runner.processEvidence(f.pages,f.captures,f.row,'task','current').status,'passed');
  for(const mutation of [x=>x.captures.pop(),x=>x.captures[1].receipt.observed_workspace.sources[0].sha256='stale',x=>x.captures[1].receipt.output_complete=false,x=>x.pages[0].items[1].record.observed_changes=[],x=>x.captures[0].receipt.presentation.stdout.tail='launcher rejected arguments',x=>x.captures[1].scope.task='different']){const bad=structuredClone(f);mutation(bad);assert.throws(()=>runner.processEvidence(bad.pages,bad.captures,bad.row,'task','current'),/Missing canonical/);}
  const missing={reproduction:'unavailable'};assert.equal(runner.processEvidence([{gaps:[],items:[]}],[],missing,'task','current').status,'not_run');assert.throws(()=>runner.processEvidence(f.pages,f.captures,missing,'task','current'),/executed/);
});
test('analysis verification cannot substitute for available native parent checks',()=>{
  const pages=[{gaps:[],items:[{id:'analysis',collection:'verification',visibility:'available',record:{scope:{task:'task'}}}]}];assert.equal(runner.verificationEvidence(pages,'task',false).records[0],'analysis');assert.throws(()=>runner.verificationEvidence(pages,'task',true),/No canonical successful/);
  pages[0].items[0].record.checks=[{outcome:{status:'passed'}}];assert.throws(()=>runner.verificationEvidence(pages,'task',false),/unexpectedly claims/);
});
test('qualified debug runtime executes frozen checks and exact plans remain one shot',t=>{
  const receiptPath=process.env.VCP_CR06_BUILD_RECEIPT;if(!receiptPath)return t.skip('Requires recorded native CR06 launcher build');
  const receipt=JSON.parse(fs.readFileSync(receiptPath)),runtime={node:receipt.node,launcher:receipt.launcher,build_receipt:receiptPath};assert.equal(prep.qualifyRuntime(runtime).launcher_build_provenance,'recorded_local_build');
  const f=fixture(t);fs.writeFileSync(f.spec,JSON.stringify({executable:f.executable,profile:f.profileFile,aggregate_cap_usd:'0.400003',runtime,propose_opaque_launcher_effects:true}));const made=prep.prepare(f.spec,path.join(f.root,'qualified')),plan=JSON.parse(fs.readFileSync(made.plan));runner.validate(plan,made.plan);
  const missingProfile=path.join(plan.directory,'missing-reproduction-access/profile.json'),original=fs.readFileSync(missingProfile),broadened=JSON.parse(original);broadened.processes=[{name:'not allowed'}];fs.writeFileSync(missingProfile,JSON.stringify(broadened));assert.throws(()=>runner.validate(plan,made.plan),/process authority changed/);fs.writeFileSync(missingProfile,original);
  for(const id of ['seeded-failure','interrupted-instrumentation','concurrent-human-edit']){
    const workspace=path.join(f.root,id);oracle.prepare(id,workspace);const args=['--test','--test-reporter=tap','--test-concurrency=1','shipping.test.cjs'];
    const launch=extra=>spawnSync(runtime.launcher,[...args,...extra],{cwd:workspace,env:{SystemRoot:process.env.SystemRoot,VCP_FAKE_PROVIDER_SECRET:'not-inherited'},encoding:'utf8',timeout:5000,maxBuffer:65536,windowsHide:true});
    const before=launch([]);assert.equal(before.status,1);assert.match(before.stdout,/not ok 1 - shipping fee threshold includes 50/);
    const source=path.join(workspace,'shipping.cjs');let fixed=fs.readFileSync(source,'utf8').replace('subtotal > 50','subtotal >= 50').replace("console.warn('CR06_OWNED_TRACE', subtotal); ",'');
    fixed="if(process.env.VCP_FAKE_PROVIDER_SECRET||['net','child','fs.write','worker','addons'].some(scope=>process.permission.has(scope)))throw Error('authority leak');\n"+fixed;fs.writeFileSync(source,fixed);
    const after=launch([]);assert.equal(after.status,0,after.stderr);assert.match(after.stdout,/ok 1 - shipping fee threshold includes 50/);assert.equal(oracle.grade(id,workspace).controls_pass,true);
    const extra=launch(['--allow-net']);assert.equal(extra.status,1);assert.match(extra.stderr,/only the frozen CR06/);
  }
  let calls=0;const stopped=runner.run(made.plan,made.sha256,()=>{calls++;return {error:'ETIMEDOUT',status:null,stdout:'',stderr:''};});assert.equal(calls,1);assert.equal(stopped.actual_cost_micros,null);assert.equal(stopped.stopped,true);assert.equal(stopped.runs[1].status,'not_run');assert.throws(()=>runner.run(made.plan,made.sha256,()=>assert.fail('never replay')),/EEXIST/);
});
