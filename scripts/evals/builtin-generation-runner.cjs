// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{spawnSync}=require('node:child_process');
const prior=require('./p6-live-runner.cjs'),paired=require('./builtin-live-runner.cjs'),prep=require('./builtin-generation-prepare.cjs');
const {plain,read,write,safeChild,noParentInstructions,privateDirectory,usd,frames,inspection,invoke}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),fixture=path.join(repo,'src/evals/skills/builtin/generation-v1');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function validate(plan,file){
  noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-u03-generation-preparation/1'||!plan.runnable||plan.blockers.length||!plan.runtime||plain(path.dirname(path.resolve(file)))!==plan.directory)throw Error('Runnable qualified generation plan required');
  const bindings={runner_sha256:'builtin-generation-prepare.cjs',oracle_sha256:'builtin-generation-oracle.cjs',live_runner_sha256:'builtin-generation-runner.cjs',shared_runner_sha256:'p6-live-runner.cjs',skill_runner_sha256:'builtin-live-runner.cjs'};
  for(const [field,name]of Object.entries(bindings))if(plan[field]!==sha(read(path.join(__dirname,name))))throw Error('Prepared runner source changed');
  const runtimeInput={node:plan.runtime.node,launcher:plan.runtime.launcher};if(plan.runtime.build_receipt)runtimeInput.build_receipt=plan.runtime.build_receipt;
  if(JSON.stringify(prep.qualifyRuntime(runtimeInput))!==JSON.stringify(plan.runtime))throw Error('Qualified runtime changed');
  const specBytes=read(plan.spec_source);
  const spec=JSON.parse(specBytes);
  if(sha(specBytes)!==plan.spec_sha256||spec.propose_opaque_launcher_effects!==true||JSON.stringify(plan.permission_review)!==JSON.stringify(prep.permissionReview(plan.runtime,true)))throw Error('Exact opaque launcher permission proposal changed');
  if(plain(path.resolve(spec.executable))!==plan.executable||plain(path.resolve(spec.profile))!==plan.profile_source||prior.micros(spec.aggregate_cap_usd)!==plan.aggregate_cap_micros||JSON.stringify(prep.qualifyRuntime(spec.runtime))!==JSON.stringify(plan.runtime))throw Error('Prepared plan differs from exact owner spec');
  const manifestBytes=read(path.join(fixture,'manifest.json')),manifest=JSON.parse(manifestBytes);
  const executableBytes=read(plan.executable,1024*1024*1024);
  if(plan.fixture_sha256!==sha(manifestBytes)||plan.fixture_revision!==manifest.revision||plan.executable_sha256!==sha(executableBytes)||plan.profile_sha256!==sha(read(plan.profile_source))||plan.catalog_sha256!==sha(read(plan.catalog)))throw Error('Prepared fixture, executable or provider changed');
  if(JSON.stringify(prep.inventory(path.join(path.dirname(plan.executable),'skills/builtin')))!==JSON.stringify(plan.assets)||JSON.stringify(prep.inventory(path.join(repo,'src/skills/builtin')))!==JSON.stringify(plan.assets))throw Error('Packaged skills changed');
  prep.requireEmbeddedCatalog(executableBytes,read(path.join(path.dirname(plan.executable),'skills/builtin/catalog.json')));
  const source=JSON.parse(read(plan.profile_source));
  if(paired.fixedProfileReasons(source).length||source.maximum_autonomy!=='workspace'||JSON.stringify([...source.automatic_effects].sort())!==JSON.stringify(['read','write']))throw Error('Source profile qualification expired or changed');
  const allocation=Math.floor(plan.aggregate_cap_micros/2),files=Object.fromEntries(manifest.files.map(f=>[f.path,f.sha256]));
  if(!Number.isSafeInteger(plan.aggregate_cap_micros)||allocation<1||plan.allocated_cap_micros!==allocation*2||plan.runs.length!==2)throw Error('Paired cap allocation changed');
  for(const [index,arm]of ['baseline','skill'].entries()){
    const row=plan.runs[index],base=safeChild(plan.directory,arm),workspace=path.join(base,'workspace');
    if(row.arm!==arm||row.cap_micros!==allocation||row.skill!==(arm==='skill'?'vcp-builtin::javascript-typescript::javascript-typescript':null)||JSON.stringify(row.files)!==JSON.stringify(files)||JSON.stringify(row.editable)!==JSON.stringify(manifest.editable))throw Error('Frozen paired cohort changed');
    if(JSON.stringify(prep.inventory(workspace))!==JSON.stringify(files)||fs.readdirSync(path.join(base,'data')).length||row.prompt_sha256!==sha(Buffer.from(manifest.prompt))||sha(read(path.join(base,'prompt.txt')))!==row.prompt_sha256)throw Error('Prepared workspace, prompt or fresh store changed');
    const profileBytes=read(path.join(base,'profile.json'));
    if(sha(profileBytes)!==row.profile_sha256||JSON.stringify(JSON.parse(profileBytes))!==JSON.stringify(prep.qualifiedProfile(source,workspace,plan.catalog,allocation,plan.runtime,true)))throw Error('Exact trusted verification profile changed');
  }
}
function verificationEvidence(pages,task){
  if(pages.some(p=>p.gaps.length))throw Error('Canonical verification has gaps');
  const records=pages.flatMap(p=>p.items).filter(i=>i.collection==='verification'&&i.visibility==='available').map(i=>i.record);
  const passed=records.filter(v=>v.scope?.task===task&&!v.redaction&&v.outputs?.length&&!v.unresolved_effects?.length&&!v.outstanding_issues?.length&&v.cost?.certainty==='known'&&v.checks?.length===1&&v.checks[0].specification==='package.json#test'&&v.checks[0].outcome?.status==='passed'&&v.checks[0].exit_code===0);
  if(!passed.length)throw Error('No canonical successful parent executable verification');
  return passed.map(v=>v.id);
}
function run(file,authorization,call=invoke){
  const bytes=read(file);if(sha(bytes)!==authorization)throw Error('Authorization must name exact prepared plan hash');
  const plan=JSON.parse(bytes);validate(plan,file);
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,at:new Date().toISOString(),meaning:'One shot; inspect interrupted liability, never replay'});
  const result={schema:'p7-u03-generation-result/1',plan_sha256:authorization,actual_cost_micros:0,stopped:false,runs:[]};
  for(const row of plan.runs){
    const report={arm:row.arm,status:'not_run',actual_cost_micros:null};result.runs.push(report);if(result.stopped)continue;
    const base=safeChild(plan.directory,row.arm),profile=JSON.parse(read(path.join(base,'profile.json')));
    const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy','autonomous'];
    if(row.arm==='skill')args.push('--skill',row.skill);
    write(path.join(base,'attempted.json'),{plan_sha256:authorization,args,at:new Date().toISOString()});
    const start=Date.now();
    try{
      const execution=call(plan.executable,args,(profile.deadline_seconds+180)*1000);report.latency_ms=Date.now()-start;
      write(path.join(base,'stdout.jsonl'),execution.stdout);write(path.join(base,'stderr.txt'),execution.stderr);
      if(execution.error)throw Error('CLI interrupted; reconcile liability before another trial');
      const output=frames(execution.stdout),accepted=output.find(f=>f.type==='accepted'),final=output.findLast(f=>f.type==='result');
      if(!accepted?.scope?.task||!final?.conditions)throw Error('Durable task result missing');report.scope=accepted.scope;
      const evidence={};for(const view of ['costs','routing','outputs','context','verification']){evidence[view]=inspection(plan,base,accepted.scope.task,view,call);write(path.join(base,view+'.json'),evidence[view]);}
      const money=prior.accounting(evidence.costs,row.cap_micros);report.actual_cost_micros=money.actual_cost_micros;result.actual_cost_micros+=money.actual_cost_micros;
      report.skill_evidence=paired.skillEvidence(plan,base,{...row,skill:'javascript-typescript'},evidence.context,money.attempts,call);
      report.status='failed';
      if(execution.status===0&&final.conditions.completed===true){
        report.verification=verificationEvidence(evidence.verification,accepted.scope.task);
        const env=process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{};
        const observed=spawnSync(plan.runtime.node,[path.join(__dirname,'builtin-generation-oracle.cjs'),path.join(base,'workspace')],{env,encoding:'utf8',timeout:10000,maxBuffer:65536,windowsHide:true});
        if(observed.error||![0,1].includes(observed.status))throw Error('Independent oracle unavailable');
        report.oracle=JSON.parse(observed.stdout);if(observed.status===0&&report.oracle.pass===true)report.status='completed';
      }else report.reason='CLI task did not complete; oracle cannot override';
    }catch(error){report.status='failed';report.reason=error.message;result.actual_cost_micros=null;result.stopped=true;}
    write(path.join(base,'result.json'),report);
  }
  write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={run,validate,verificationEvidence};
if(require.main===module){try{const [command,file,authorization,...rest]=process.argv.slice(2);if(command!=='run'||!file||!authorization||rest.length)throw Error('Usage: builtin-generation-runner.cjs run <plan.json> <authorized-plan-sha256>');const result=run(file,authorization);console.log(JSON.stringify(result));if(result.stopped||result.runs.some(r=>r.status!=='completed'))process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
