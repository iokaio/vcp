// SPDX-License-Identifier: Apache-2.0
// Fixture scenarios adapt Gemini CLI (Apache-2.0), Copyright 2025–2026 Google LLC.
'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {pathToFileURL}=require('node:url');
const {spawnSync}=require('node:child_process');
const {verifySource}=require('../../src/tests/support/gemini-baseline.cjs');
const {outside}=require('../../src/tests/support/model-assets.cjs');
const {digest,writeManifest}=require('../../src/tests/support/harness.cjs');
async function main(args){
  const options={};for(let i=0;i<args.length;i+=2){if(!['--source','--output-root'].includes(args[i])||!args[i+1]||options[args[i]])throw Error('Supply --source and --output-root once');options[args[i]]=args[i+1];}
  if(Object.keys(options).length!==2)throw Error('Supply both paths');
  const root=path.resolve(__dirname,'../..'),source=path.resolve(options['--source']);
  const output=path.join(outside(options['--output-root'],[source,path.join(root,'src')]),crypto.randomUUID());fs.mkdirSync(output,{recursive:true});
  const record={schema_version:1,task_id:'P0-09',status:'running',started_at:new Date().toISOString(),node:process.version,platform:process.platform,observations:{}};
  const file=path.join(output,'manifest.json');
  try{
    const TOML=require('../../src/tests/node_modules/@iarna/toml');
    const selection=TOML.parse(fs.readFileSync(path.join(root,'src/third_party/upstreams.toml'),'utf8')).upstream.find(p=>p.id==='gemini-cli');
    record.source=verifySource(source,selection);
    const fixturePath=path.join(root,'src/tests/fixtures/gemini/ports.json');
    const f=JSON.parse(fs.readFileSync(fixturePath));assert.equal(f.revision,selection.commit);record.fixture_sha256=digest(fs.readFileSync(fixturePath));
    // Compile the pinned TypeScript before importing its JS. No SDK request is made.
    const core=path.join(source,'packages/core');
    const build=spawnSync(process.execPath,[path.join(source,'node_modules/typescript/bin/tsc'),'--build'],{cwd:core,encoding:'utf8',windowsHide:true,timeout:300000});
    fs.writeFileSync(path.join(output,'build.log'),(build.stdout||'')+(build.stderr||''));if(build.error||build.status!==0)throw Error('Pinned TypeScript build failed');
    const modulePaths=['policy/stable-stringify','policy/policy-engine','scheduler/state-manager','scheduler/policy'];
    record.modules=modulePaths.map(p=>({path:p,source_sha256:digest(fs.readFileSync(path.join(core,'src',p+'.ts'))),compiled_sha256:digest(fs.readFileSync(path.join(core,'dist/src',p+'.js')))}));
    const load=p=>import(pathToFileURL(path.join(core,'dist/src',p+'.js')).href);
    const {stableStringify}=await load(modulePaths[0]);const {PolicyEngine}=await load(modulePaths[1]);const {SchedulerStateManager}=await load(modulePaths[2]);const {checkPolicy}=await load(modulePaths[3]);
    const canonical=stableStringify(f.canonical.input);assert.equal(canonical,f.canonical.expected);record.observations.canonical=canonical;
    const bus={publish:async()=>{}};
    const invocation={getDescription:()=> 'synthetic fixture'};
    const tool={name:'fixture',displayName:'fixture'};
    const call=(id,args={})=>({request:{callId:id,name:'fixture',args,isClientInitiated:false,prompt_id:'p0'},status:'scheduled',tool,invocation});
    const scheduler=new SchedulerStateManager(bus);scheduler.addToolCalls(f.out_of_order.arrival.map(id=>call(id)));
    scheduler.dequeue();scheduler.dequeue();assert.equal(scheduler.activeCallCount,f.resource_conflict.upstream_state_manager_active);
    for(const id of f.out_of_order.completion){scheduler.updateStatus(id,'success',{callId:id,responseParts:[{text:id}]});scheduler.finalizeCall(id);}
    const order=scheduler.completedBatch.map(c=>c.request.callId);assert.deepEqual(order,f.out_of_order.upstream);record.observations.out_of_order=order;
    const rewritten=new SchedulerStateManager(bus);rewritten.addToolCalls([call('rewrite',f.rewrite.before)]);rewritten.dequeue();rewritten.updateArgs('rewrite',f.rewrite.after,invocation);rewritten.setOutcome('rewrite','proceed_once');
    assert.deepEqual(rewritten.getToolCall('rewrite').request.args,f.rewrite.after);assert.equal(rewritten.getToolCall('rewrite').outcome,'proceed_once');record.observations.rewrite={args:rewritten.getToolCall('rewrite').request.args,outcome:'proceed_once'};
    const cancelled=new SchedulerStateManager(bus);cancelled.addToolCalls([call('cancel')]);cancelled.cancelAllQueued(f.cancellation.reason);cancelled.updateStatus('cancel','success',{callId:'cancel',responseParts:[]});
    assert.equal(cancelled.completedBatch[0].status,f.cancellation.upstream);record.observations.cancellation=cancelled.completedBatch[0].status;
    const engine=new PolicyEngine({defaultDecision:'ask_user'});
    const config={getPolicyEngine:()=>engine,isInteractive:()=>true};const request=call('policy');request.request.isClientInitiated=true;
    const policy=await checkPolicy(request,config);assert.equal(policy.decision,f.client_initiated.upstream);record.observations.client_initiated=policy.decision;
    record.observations.resource_conflict={state_manager_active:2,scope:'State-manager boundary permits two active calls; upstream Scheduler owns execution arbitration.'};
    record.status='pass';record.exit_code=0;
  }catch(error){record.status='fail';record.reason=error.message;record.exit_code=1;}
  record.runner_sha256=digest(fs.readFileSync(__filename));record.ended_at=new Date().toISOString();writeManifest(file,record);console.log(JSON.stringify({status:record.status,reason:record.reason,manifest:file}));process.exitCode=record.exit_code;
}
main(process.argv.slice(2)).catch(e=>{console.error(e.message);process.exitCode=1;});
