// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const oracle=require('../../../scripts/evals/builtin-generation-oracle.cjs');
const prepare=require('../../../scripts/evals/builtin-generation-prepare.cjs').prepare;
const fixture=path.resolve(__dirname,'../../evals/skills/builtin/generation-v1');
function observe(fn){return oracle.cases().map(({args})=>{args=structuredClone(args);const before=JSON.stringify(args);try{return {value:fn(...args),unchanged:before===JSON.stringify(args)};}catch(error){return {error:error.name,unchanged:before===JSON.stringify(args)};}});}
function reference(items,options={}){
  if(!Array.isArray(items)||!options||typeof options!=='object'||Array.isArray(options))throw TypeError();
  const bps=options.discountBps===undefined?0:options.discountBps;
  if(!Number.isInteger(bps)||bps<0||bps>10000)throw TypeError();
  let sum=0n;for(const item of items){if(!item||!Number.isSafeInteger(item.unitCents)||item.unitCents<0||!Number.isSafeInteger(item.quantity)||item.quantity<0)throw TypeError();sum+=BigInt(item.unitCents)*BigInt(item.quantity);}
  if(sum>BigInt(Number.MAX_SAFE_INTEGER))throw TypeError();
  const discount=(sum*BigInt(bps)+5000n)/10000n;
  return {totalCents:Number(sum-discount),discountCents:Number(discount),subtotalCents:Number(sum)};
}
test('independent executable oracle rejects seeded missing feature and accepts exact reference semantics',()=>{
  const original=require(path.join(fixture,'project/src/cart.cjs'));
  assert.equal(oracle.evaluate(observe(original.quoteCart)).pass,false);
  const measured=oracle.evaluate(observe(reference));
  assert.equal(measured.pass,true);assert.equal(measured.total,44);
  // The public contract rejects arrays and strings even when their missing
  // discountBps field could otherwise look like the default options object.
  assert.equal(oracle.evaluate(observe((items,options)=>reference(items,
    Array.isArray(options)||typeof options==='string'?{}:options))).pass,false);
  // An invalid price must still be rejected when quantity zero masks overflow.
  assert.equal(oracle.evaluate(observe((items,options)=>reference(
    items?.map?.(item=>item?.quantity===0?{...item,unitCents:0}:item)??items,options))).pass,false);
  assert.equal(oracle.evaluate(observe((items,options)=>{const result=reference(items,options);if(items.length)items[0].quantity++;return result;})).pass,false);
  assert.equal(oracle.evaluate(observe(reference),['package-lock.json']).pass,false);
  assert.equal(oracle.evaluate([]).pass,false);
});
test('preparation freezes matched arms without executable authority, expected answers or calls',t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-u03-prep-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const executable=path.join(root,'vcp.exe');fs.writeFileSync(executable,'fixture; never executed');
  fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),path.join(root,'skills/builtin'),{recursive:true});
  const catalog=path.join(root,'catalog.json');fs.writeFileSync(catalog,'{}');
  const profile={version:1,trust_workspace:true,maximum_autonomy:'workspace',automatic_effects:['read','write'],workspace:'rebound',sync_roots:[],provider:{valid_until:String(Date.now()+3600000),max_output:'8192',price:{currency:'USD',valid_until:String(Date.now()+3600000)},compatibility:{valid_until:String(Date.now()+3600000),responses_text_tools:true,provider_preferences_qualified:true}},catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'4096',max_transport_retries:0,max_requests:16,deadline_seconds:60};
  const profileFile=path.join(root,'profile.json');fs.writeFileSync(profileFile,JSON.stringify(profile));
  const spec=path.join(root,'spec.json');fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'0.200001'}));
  const result=prepare(spec,path.join(root,'trial')),plan=JSON.parse(fs.readFileSync(result.plan));
  assert.equal(result.model_calls,0);assert.equal(result.runnable,false);assert.equal(plan.allocated_cap_micros,200000);
  assert.equal(plan.authorization,false);assert.equal(plan.blockers.length,3);assert.equal(plan.runs.length,2);
  assert.deepEqual(plan.runs[0].files,plan.runs[1].files);assert.equal(plan.runs[0].prompt_sha256,plan.runs[1].prompt_sha256);
  assert.equal(plan.runs[0].skill,null);assert.equal(plan.runs[1].skill,'vcp-builtin::javascript-typescript::javascript-typescript');
  for(const row of plan.runs){const workspace=path.join(plan.directory,row.arm,'workspace');assert.equal(fs.existsSync(path.join(workspace,'manifest.json')),false);assert.equal(fs.existsSync(path.join(workspace,'oracle.cjs')),false);const derived=JSON.parse(fs.readFileSync(path.join(plan.directory,row.arm,'profile.json')));assert.deepEqual(derived.affected_paths,['src/cart.cjs']);assert.deepEqual(derived.processes,[]);}
  assert.throws(()=>prepare(spec,plan.directory),/New private directory/);
  const generation=require('../../../scripts/evals/builtin-generation-runner.cjs');
  assert.throws(()=>generation.run(result.plan,result.sha256,()=>{throw Error('must not dispatch');}),/Runnable qualified/);
  if(process.env.VCP_U03_LAUNCHER){
    const runtime={node:process.execPath,launcher:process.env.VCP_U03_LAUNCHER};
    if(process.env.VCP_U03_BUILD_RECEIPT)runtime.build_receipt=process.env.VCP_U03_BUILD_RECEIPT;
    fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'0.200001',runtime}));
    const defaultPlan=prepare(spec,path.join(root,'runtime-without-proposal'));
    assert.equal(defaultPlan.runnable,false);
    const blocked=JSON.parse(fs.readFileSync(defaultPlan.plan));
    assert.deepEqual(blocked.blockers,['explicit_opaque_launcher_permission_proposal_required']);
    const limited=JSON.parse(fs.readFileSync(path.join(path.dirname(defaultPlan.plan),'baseline/profile.json')));
    assert.deepEqual(limited.automatic_effects,['read','write']);assert.deepEqual(limited.processes,[]);
    assert.throws(()=>generation.validate(blocked,defaultPlan.plan),/Runnable qualified/);
    fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'0.200001',runtime,propose_opaque_launcher_effects:true}));
    assert.throws(()=>prepare(spec,path.join(root,'short-deadline')),/deadline above the default 120-second/);
    assert.equal(fs.existsSync(path.join(root,'short-deadline')),false);
    profile.deadline_seconds=600;fs.writeFileSync(profileFile,JSON.stringify(profile));
    const qualified=prepare(spec,path.join(root,'qualified')),qualifiedPlan=JSON.parse(fs.readFileSync(qualified.plan));
    assert.equal(qualified.runnable,true);generation.validate(qualifiedPlan,qualified.plan);
    const provenance=runtime.build_receipt?'recorded_local_build':'owner_supplied_unverified';
    assert.equal(qualified.launcher_build_provenance,provenance);
    assert.equal(qualifiedPlan.runtime.launcher_build_provenance,provenance);
    assert.match(qualifiedPlan.runtime.launcher_reference_source_sha256,/^[a-f0-9]{64}$/);
    assert.equal(qualifiedPlan.runtime.launcher_source_sha256,undefined);
    assert.equal(qualifiedPlan.authorization,false);
    assert.equal(qualifiedPlan.permission_review.approval,'pending_exact_plan_authorization');
    assert.deepEqual(qualifiedPlan.permission_review.automatic_effects,['read','write','execute','network','install','publish','opaque']);
    const altered=structuredClone(qualifiedPlan);altered.permission_review.automatic_effects.pop();
    assert.throws(()=>generation.validate(altered,qualified.plan),/permission proposal changed/);
    const injectedProfile=path.join(path.dirname(qualified.plan),'baseline/profile.json'),originalProfile=fs.readFileSync(injectedProfile);
    const broadened=JSON.parse(originalProfile);broadened.processes.push({...broadened.processes[0],name:'unscoped'});
    fs.writeFileSync(injectedProfile,JSON.stringify(broadened));
    assert.throws(()=>generation.validate(qualifiedPlan,qualified.plan),/verification profile changed/);
    fs.writeFileSync(injectedProfile,originalProfile);
    if(runtime.build_receipt){
      const receipt=JSON.parse(fs.readFileSync(runtime.build_receipt));receipt.embedded.VCP_U03_NODE='changed';
      const tampered=path.join(root,'tampered-build.json');fs.writeFileSync(tampered,JSON.stringify(receipt));
      assert.throws(()=>require('../../../scripts/evals/builtin-generation-prepare.cjs').qualifyRuntime({...runtime,build_receipt:tampered}),/build receipt does not bind/);
    }
    let calls=0;
    const stopped=generation.run(qualified.plan,qualified.sha256,()=>{calls++;return {error:'ETIMEDOUT',status:null,stdout:'',stderr:''};});
    assert.equal(calls,1);assert.equal(stopped.stopped,true);assert.equal(stopped.actual_cost_micros,null);assert.equal(stopped.runs[1].status,'not_run');
    assert.throws(()=>generation.run(qualified.plan,qualified.sha256,()=>{throw Error('never replay');}),/EEXIST/);
    fs.writeFileSync(spec,JSON.stringify({executable,profile:profileFile,aggregate_cap_usd:'0.200001'}));
  }
  profile.automatic_effects.push('execute');fs.writeFileSync(profileFile,JSON.stringify(profile));
  assert.throws(()=>prepare(spec,path.join(root,'denied')),/no execution authority/);assert.equal(fs.existsSync(path.join(root,'denied')),false);
});

test('runtime verification profile cannot be derived with an impossible task deadline',()=>{
  const {qualifiedProfile}=require('../../../scripts/evals/builtin-generation-prepare.cjs');
  const defaults=qualifiedProfile({deadline_seconds:600,maximum_autonomy:'workspace',automatic_effects:['read','write'],processes:[]},'workspace','catalog',100,{launcher:'unused'});
  assert.equal(defaults.maximum_autonomy,'workspace');assert.deepEqual(defaults.automatic_effects,['read','write']);assert.deepEqual(defaults.processes,[]);
  for(const deadline_seconds of [60,120])assert.throws(()=>qualifiedProfile({deadline_seconds},'workspace','catalog',100,{launcher:'unused'},true),/deadline above the default 120-second/);
  assert.equal(qualifiedProfile({deadline_seconds:600},'workspace','catalog',100,{launcher:'unused'},true).deadline_seconds,600);
});
test('oracle refuses current runtime before loading candidate when network permission is absent',()=>{
  const help=require('node:child_process').execFileSync(process.execPath,['--help'],{encoding:'utf8'});
  if(!help.includes('--allow-net'))assert.throws(()=>oracle.grade(path.join(fixture,'project')),/candidate was not loaded/);
});
test('generation requires independent canonical passed parent verification',()=>{
  const {verificationEvidence}=require('../../../scripts/evals/builtin-generation-runner.cjs');
  const record={id:'check-1',scope:{task:'task-1'},outputs:['output-1'],unresolved_effects:[],outstanding_issues:[],cost:{certainty:'known'},checks:[{specification:'package.json#test',outcome:{status:'passed'},exit_code:0}]};
  const pages=[{gaps:[],items:[{collection:'verification',visibility:'available',record}]}];
  assert.deepEqual(verificationEvidence(pages,'task-1'),['check-1']);
  assert.throws(()=>verificationEvidence(pages,'other-task'),/No canonical/);
  record.checks[0].outcome.status='not_run';assert.throws(()=>verificationEvidence(pages,'task-1'),/No canonical/);
  record.checks[0].outcome.status='passed';record.unresolved_effects=['pending'];assert.throws(()=>verificationEvidence(pages,'task-1'),/No canonical/);
  pages[0].gaps.push({reason:'unavailable'});assert.throws(()=>verificationEvidence(pages,'task-1'),/gaps/);
});
test('qualified runtime executes oracle and denies candidate filesystem, network and process authority',t=>{
  const {spawnSync}=require('node:child_process');
  const help=spawnSync(process.execPath,['--help'],{env:{},encoding:'utf8'});
  if(!help.stdout.includes('--allow-net'))return t.skip('Requires qualified network-denying Node runtime');
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-u03-oracle-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const workspace=path.join(root,'workspace');fs.cpSync(path.join(fixture,'project'),workspace,{recursive:true});
  assert.equal(oracle.grade(workspace).pass,false);
  fs.writeFileSync(path.join(workspace,'src/cart.cjs'),`exports.quoteCart=${reference.toString()};`);
  assert.equal(oracle.grade(workspace).pass,true);
  if(process.env.VCP_U03_LAUNCHER){
    const launcher=process.env.VCP_U03_LAUNCHER;
    const args=['--test','--test-reporter=tap','--test-concurrency=1','test/cart.test.cjs'];
    const run=extra=>spawnSync(launcher,[...args,...extra],{cwd:workspace,env:{SystemRoot:process.env.SystemRoot,VCP_FAKE_PROVIDER_SECRET:'must-not-be-inherited'},encoding:'utf8',timeout:5000,maxBuffer:65536,windowsHide:true});
    fs.writeFileSync(path.join(workspace,'src/cart.cjs'),`if(process.env.VCP_FAKE_PROVIDER_SECRET||process.permission.has('net')||process.permission.has('child')||process.permission.has('fs.write'))throw Error('launcher authority leak');exports.quoteCart=${reference.toString()};`);
    const accepted=run([]);assert.equal(accepted.status,0,accepted.stderr);assert.match(accepted.stdout,/# pass 2/);
    const denied=run(['--allow-net']);assert.equal(denied.status,1);assert.match(denied.stderr,/only the frozen/);
  }
  const secret=path.join(root,'outside.txt');fs.writeFileSync(secret,'not-readable');
  const environment=process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{};
  const probe=`
    for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];
    const assert=require('node:assert/strict');
    for(const scope of ['net','fs.write','child','worker','addons'])assert.equal(process.permission.has(scope),false,scope);
    assert.throws(()=>require('node:fs').readFileSync(process.argv[1]),{code:'ERR_ACCESS_DENIED'});
    assert.throws(()=>require('node:fs').writeFileSync(process.argv[2],'bad'),{code:'ERR_ACCESS_DENIED'});
    assert.throws(()=>require('node:child_process').spawnSync(process.execPath,['--version']),{code:'ERR_ACCESS_DENIED'});
    assert.throws(()=>new(require('node:worker_threads').Worker)('0',{eval:true}),{code:'ERR_ACCESS_DENIED'});
    assert.deepEqual(Object.keys(process.env).map(k=>k.toUpperCase()).sort(),process.platform==='win32'?['SYSTEMROOT']:[]);
    const socket=require('node:net').connect({host:'127.0.0.1',port:9});
    socket.once('connect',()=>{throw Error('Unexpected network connection');});
    socket.once('error',error=>{assert.equal(error.code,'ERR_ACCESS_DENIED');console.log('denial-controls-pass');});
  `;
  const measured=spawnSync(process.execPath,['--permission',`--allow-fs-read=${workspace}`,'-e',probe,secret,path.join(workspace,'unexpected.txt')],{cwd:workspace,env:environment,encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});
  assert.equal(measured.status,0,measured.stderr);assert.match(measured.stdout,/denial-controls-pass/);
  assert.equal(fs.readFileSync(secret,'utf8'),'not-readable');assert.equal(fs.existsSync(path.join(workspace,'unexpected.txt')),false);
  fs.writeFileSync(path.join(workspace,'notes.txt'),'changed');assert.equal(oracle.grade(workspace).pass,false);
});
