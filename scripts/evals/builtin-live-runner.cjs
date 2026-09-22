// SPDX-License-Identifier: Apache-2.0
'use strict';
// P7-02 paired, read-only usefulness observations through the real CLI.
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const prior=require('./p6-live-runner.cjs');
const {plain,read,write,within,safeChild,filesUnder,noParentInstructions,privateDirectory,noSecrets,usd,frames,inspection,invoke}=prior.boundaries;
const repo=path.resolve(__dirname,'../..');
const fixtures=path.join(repo,'src/evals/skills/builtin');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const families=['architecture','review-debug','testing','javascript-typescript'];
const promptFor=task=>task.prompt+'\nRead the relevant project files. Return only a concise JSON object as the final answer, without XML, Markdown fences or surrounding prose, with fields findings (array), evidence (array of file paths and observations), recommended_checks (array), not_run (array with reasons), and recommendation (string). Do not change files. Do not claim any check ran without a receipt. Run vcp_verify as required by the host.\n';
function pool() {
  const bytes=read(path.join(fixtures,'manifest.json'));
  const manifest=JSON.parse(bytes);
  const cases=manifest.cases.filter(c=>families.includes(c.skill));
  if(manifest.revision!=='p7-02-builtin-fixtures-v1'||cases.length!==8) throw Error('Frozen usefulness cohort changed');
  return {cases,sha256:sha(bytes),revision:manifest.revision};
}
function inventory(directory) {
  return Object.fromEntries(filesUnder(directory).map(p=>[p,sha(read(safeChild(directory,p)))]));
}
function packaged(executable) {
  const directory=path.join(path.dirname(executable),'skills/builtin');
  const observed=inventory(directory), expected=inventory(path.join(repo,'src/skills/builtin'));
  if(JSON.stringify(observed)!==JSON.stringify(expected)) throw Error('Exact current packaged skill assets required beside executable');
  return observed;
}
// P7 coding observations need explicit room for reasoning and complete edits.
// The allowance counts all output, including reasoning; it is not an answer quota.
// Keep this separate from P6's immutable 512-token smoke cohort; plan hashes bind it.
function fixedProfileReasons(profile,now=Date.now()) {
  const reasons=[];
  const decimal=value=>typeof value==='string'&&/^(0|[1-9][0-9]*)$/.test(value)&&Number.isSafeInteger(Number(value))?Number(value):NaN;
  if(profile.version!==1||profile.trust_workspace!==true)reasons.push('explicit trusted profile required');
  if(!Number.isSafeInteger(profile.max_requests)||profile.max_requests<1||profile.max_requests>16||!Number.isSafeInteger(profile.deadline_seconds)||profile.deadline_seconds<1||profile.deadline_seconds>1800)reasons.push('P7 trial needs 1..16 requests and 1..1800 second deadline');
  if(profile.provider_timeout_seconds!==undefined&&(!Number.isSafeInteger(profile.provider_timeout_seconds)||profile.provider_timeout_seconds<1||profile.provider_timeout_seconds>180||profile.provider_timeout_seconds>profile.deadline_seconds))reasons.push('Explicit provider timeout must be 1..180 seconds within the task deadline');
  if(profile.processes?.length||profile.checks?.length||profile.mcp?.length||profile.mcp_http?.length||profile.skills||profile.decisions||profile.qualification_endpoint)reasons.push('external tools, skills, evaluators, executable checks and endpoint overrides are outside the source profile');
  if(profile.routing||profile.max_transport_retries!==0)reasons.push('Use one fixed qualified provider, without routing or retries');
  const snapshot=profile.provider;
  if(!snapshot||!(decimal(snapshot.valid_until)>now)||!(decimal(snapshot.compatibility?.valid_until)>now)||!(decimal(snapshot.price?.valid_until)>now)||snapshot.price?.currency!=='USD'||snapshot.compatibility?.responses_text_tools!==true||snapshot.compatibility?.provider_preferences_qualified!==true)reasons.push('current qualified USD provider snapshot and price required');
  const requested=decimal(profile.output_tokens),available=decimal(snapshot?.max_output);
  if(!(requested>=1&&requested<=16384&&available>=requested))reasons.push('P7 profile must explicitly request 1..16384 total output tokens (including reasoning) within the CLI startup ceiling and qualified provider maximum');
  return reasons;
}
function profileReasons(profile,now=Date.now()) {
  const reasons=fixedProfileReasons(profile,now);
  if(profile.maximum_autonomy!=='plan'||profile.automatic_effects?.length) reasons.push('Read-only plan authority required');
  return reasons;
}
function prepare(specFile,destination) {
  const specBytes=read(specFile), spec=JSON.parse(specBytes); noSecrets(spec);
  if(Object.keys(spec).sort().join(',')!=='aggregate_cap_usd,executable,profile') throw Error('Spec requires only executable, profile and aggregate_cap_usd');
  destination=plain(path.resolve(destination));
  if(within(repo,destination)||within(destination,repo)||fs.existsSync(destination)) throw Error('New private directory outside repository required');
  noParentInstructions(path.dirname(destination)); privateDirectory(destination);
  const executable=plain(path.resolve(spec.executable));
  const assets=packaged(executable), executableHash=sha(read(executable,1024*1024*1024));
  const profileFile=plain(path.resolve(spec.profile)), profileBytes=read(profileFile), profile=JSON.parse(profileBytes); noSecrets(profile);
  const reasons=profileReasons(profile); if(reasons.length) throw Error(reasons.join('; '));
  const catalog=plain(path.resolve(profile.catalog)), catalogHash=sha(read(catalog));
  const selected=pool(), cap=prior.micros(spec.aggregate_cap_usd), allocation=Math.floor(cap/(selected.cases.length*2));
  if(allocation<1) throw Error('Positive per-run allocations required');
  fs.mkdirSync(destination,{mode:0o700});
  const plan={schema:'p7-builtin-live-plan/1',directory:destination,executable,executable_sha256:executableHash,
    assets,fixture_sha256:selected.sha256,fixture_revision:selected.revision,runner_sha256:sha(read(__filename)),
    shared_runner_sha256:sha(read(path.join(__dirname,'p6-live-runner.cjs'))),spec_sha256:sha(specBytes),
    profile_source:profileFile,profile_sha256:sha(profileBytes),catalog,catalog_sha256:catalogHash,
    aggregate_cap_micros:cap,allocated_cap_micros:allocation*selected.cases.length*2,authorization:false,runs:[]};
  for(const task of selected.cases) for(const arm of ['baseline','skill']) {
    const id=task.id+'--'+arm, base=safeChild(destination,id), workspace=path.join(base,'workspace');
    fs.mkdirSync(workspace,{recursive:true,mode:0o700}); fs.mkdirSync(path.join(base,'data'));
    const files={};
    for(const item of task.expected.preserve_files) {
      const bytes=read(safeChild(path.join(fixtures,task.project),item.path));
      if(bytes.length!==item.bytes||sha(bytes)!==item.sha256) throw Error('Frozen fixture bytes changed');
      const target=safeChild(workspace,item.path); fs.mkdirSync(path.dirname(target),{recursive:true}); fs.writeFileSync(target,bytes,{flag:'wx'}); files[item.path]=item.sha256;
    }
    const prompt=promptFor(task);
    write(path.join(base,'prompt.txt'),prompt);
    const derived={...profile,workspace,catalog,budget_usd:usd(allocation),affected_paths:Object.keys(files)};
    write(path.join(base,'profile.json'),derived);
    plan.runs.push({id,case_id:task.id,arm,skill:task.skill,cap_micros:allocation,files,
      prompt_sha256:sha(Buffer.from(prompt)),profile_sha256:sha(read(path.join(base,'profile.json')))});
  }
  write(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'),sha256:sha(read(path.join(destination,'plan.json'))),runs:plan.runs.length,aggregate_cap_micros:cap,model_calls:0};
}
function unchanged(base,row) {
  const actual=inventory(path.join(base,'workspace'));
  return JSON.stringify(Object.entries(actual).sort())===JSON.stringify(Object.entries(row.files).sort());
}
function capturedJson(plan,base,item,call) {
  const descriptor=item.record, length=Number(descriptor.length);
  if(descriptor.state!=='complete'||!Number.isSafeInteger(length)||length<1||length>1024*1024) throw Error('Incomplete bounded context capture');
  const chunks=[];
  for(let offset=0;offset<length;offset+=65536) {
    const pages=inspection(plan,base,item.id,'context',call,['--offset',String(offset),'--length','65536']);
    const value=pages[0]?.items[0], end=Math.min(offset+65536,length);
    if(pages.length!==1||pages[0].items.length!==1||pages.some(p=>p.gaps.some(g=>!prior.privacyGap(g,item.id)))||value?.range?.start!==offset||value.range.end!==end||(value.artifact!==undefined&&value.artifact!==item.id)||(value.visibility!==undefined&&value.visibility!=='available')||!Array.isArray(value.bytes)||value.bytes.length!==end-offset||value.bytes.some(b=>!Number.isInteger(b)||b<0||b>255)) throw Error('Context capture range unavailable');
    chunks.push(Buffer.from(value.bytes));
  }
  const bytes=Buffer.concat(chunks); if(sha(bytes)!==descriptor.sha256) throw Error('Context capture digest mismatch');
  return JSON.parse(bytes);
}
function skillEvidence(plan,base,row,pages,attempts,call) {
  if(pages.some(p=>p.gaps.some(g=>!prior.privacyGap(g,g.artifact)))) throw Error('Context evidence incomplete');
  const items=pages.flatMap(p=>p.items);
  const captures=items.filter(i=>i.collection==='artifact'&&i.record?.spec?.schema==='context-manifest/1');
  const manifests=captures.map(item=>({artifact:item.id,manifest:capturedJson(plan,base,item,call)}));
  const qualified=`vcp-builtin::${row.skill}::${row.skill}`, partId='skill-'+sha(Buffer.from(qualified))+'-0';
  const catalog=JSON.parse(read(path.join(repo,'src/skills/builtin/catalog.json')));
  const expected=catalog.skills.find(s=>s.id===row.skill).body.sha256;
  const dispatched=attempts.filter(a=>a.phase==='settled');
  if(!dispatched.length) throw Error('No observed settled model attempt');
  for(const attempt of dispatched) {
    const matched=manifests.filter(m=>m.manifest.request_sha256===attempt.request_digest);
    if(!matched.length) throw Error('No canonical context for dispatched request');
    for(const {manifest} of matched) {
      const active=manifest.included.filter(p=>p.kind==='skill');
      if(row.arm==='baseline' ? active.length!==0 : active.length!==1||active[0].id!==partId||active[0].source_hash!==expected||active[0].trust!=='active_skill') throw Error('Dispatched skill context differs from paired arm');
    }
  }
  return {qualified_id:row.arm==='skill'?qualified:null,checked_attempts:dispatched.length,manifests:manifests.map(m=>m.artifact)};
}
function validate(plan,planFile) {
  const selected=pool();
  noParentInstructions(plan.directory); privateDirectory(plan.directory);
  if(plan.schema!=='p7-builtin-live-plan/1'||plain(path.dirname(path.resolve(planFile)))!==plan.directory||plan.fixture_sha256!==selected.sha256||plan.runner_sha256!==sha(read(__filename))||plan.shared_runner_sha256!==sha(read(path.join(__dirname,'p6-live-runner.cjs')))||plan.executable_sha256!==sha(read(plan.executable,1024*1024*1024))||JSON.stringify(packaged(plan.executable))!==JSON.stringify(plan.assets)) throw Error('Prepared source, assets or executable changed');
  if(sha(read(plan.profile_source))!==plan.profile_sha256||sha(read(plan.catalog))!==plan.catalog_sha256) throw Error('Provider profile or catalog changed');
  const expected=selected.cases.flatMap(c=>['baseline','skill'].map(arm=>({id:c.id+'--'+arm,case_id:c.id,skill:c.skill,arm})));
  const allocation=Math.floor(plan.aggregate_cap_micros/expected.length);
  if(!Number.isSafeInteger(plan.aggregate_cap_micros)||allocation<1||plan.runs.length!==expected.length||plan.allocated_cap_micros!==allocation*expected.length) throw Error('Frozen cap allocation changed');
  for(let index=0;index<expected.length;index++) {
    const row=plan.runs[index], wanted=expected[index], base=safeChild(plan.directory,row.id);
    if(Object.entries(wanted).some(([k,v])=>row[k]!==v)||row.cap_micros!==allocation) throw Error('Frozen cohort or allocation changed');
    const task=selected.cases.find(c=>c.id===row.case_id);
    const frozen=Object.fromEntries(task.expected.preserve_files.map(f=>[f.path,f.sha256]));
    if(JSON.stringify(row.files)!==JSON.stringify(frozen)||row.prompt_sha256!==sha(Buffer.from(promptFor(task)))) throw Error('Frozen fixture or prompt contract differs');
    if(!unchanged(base,row)||sha(read(path.join(base,'prompt.txt')))!==row.prompt_sha256||sha(read(path.join(base,'profile.json')))!==row.profile_sha256||fs.readdirSync(path.join(base,'data')).length) throw Error('Prepared run inputs changed or data store is not fresh');
    const profile=JSON.parse(read(path.join(base,'profile.json')));
    const derived={...JSON.parse(read(plan.profile_source)),workspace:path.join(base,'workspace'),catalog:plan.catalog,budget_usd:usd(allocation),affected_paths:Object.keys(frozen)};
    if(JSON.stringify(profile)!==JSON.stringify(derived)||profileReasons(profile).length) throw Error('Profile qualification expired or invalid');
  }
}
function run(planFile,authorization,call=invoke) {
  const bytes=read(planFile); if(sha(bytes)!==authorization) throw Error('Authorization must name the exact prepared plan hash');
  const plan=JSON.parse(bytes); validate(plan,planFile);
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,at:new Date().toISOString(),meaning:'One shot; interruption requires canonical inspection, never replay.'});
  const result={schema:'p7-builtin-live-result/1',plan_sha256:authorization,fixture_sha256:plan.fixture_sha256,quality:'pending_independent_review',actual_cost_micros:0,stopped:false,runs:[]};
  for(const row of plan.runs) {
    const report={case_id:row.case_id,arm:row.arm,status:'not_run',answer:null,actual_cost_micros:null}; result.runs.push(report);
    if(result.stopped) continue;
    const base=safeChild(plan.directory,row.id), profile=JSON.parse(read(path.join(base,'profile.json')));
    const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy','plan'];
    if(row.arm==='skill') args.push('--skill',`vcp-builtin::${row.skill}::${row.skill}`);
    write(path.join(base,'attempted.json'),{plan_sha256:authorization,args,at:new Date().toISOString()});
    const start=Date.now();
    try {
      const execution=call(plan.executable,args,(profile.deadline_seconds+180)*1000); report.latency_ms=Date.now()-start;
      write(path.join(base,'stdout.jsonl'),execution.stdout); write(path.join(base,'stderr.txt'),execution.stderr);
      if(execution.error) throw Error('CLI interrupted; reconcile liability before another trial');
      const output=frames(execution.stdout), accepted=output.find(f=>f.type==='accepted'), final=output.findLast(f=>f.type==='result');
      if(!accepted?.scope?.task||!final?.conditions) throw Error('Missing durable task result; reconcile before continuing');
      report.scope=accepted.scope;
      const evidence={};
      for(const view of ['costs','routing','outputs','context']) {evidence[view]=inspection(plan,base,accepted.scope.task,view,call);write(path.join(base,view+'.json'),evidence[view]);}
      const money=prior.accounting(evidence.costs,row.cap_micros); report.actual_cost_micros=money.actual_cost_micros;result.actual_cost_micros+=money.actual_cost_micros;
      report.preserved=unchanged(base,row);
      report.status=final.conditions.completed&&execution.status===0&&report.preserved?'completed':'failed';
      if(report.status==='completed') {
        report.skill_evidence=skillEvidence(plan,base,row,evidence.context,money.attempts,call);
        try {report.answer_source=prior.responseAnswer(plan,base,evidence.outputs,money.attempts,call);report.answer=report.answer_source.answer;}
        catch {report.status='failed';report.reason='canonical_json_answer_unavailable';}
      }
    } catch(error) {report.status='failed';report.reason=error.message;result.actual_cost_micros=null;result.stopped=true;}
    write(path.join(base,'result.json'),report);
  }
  write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={prepare,run,pool,fixedProfileReasons,profileReasons,validate,skillEvidence};
if(require.main===module) {
  try {
    const [command,file,extra,...rest]=process.argv.slice(2);
    if(rest.length||!file||!extra||!['prepare','run'].includes(command)) throw Error('Usage: builtin-live-runner.cjs prepare <spec.json> <new-private-directory> | run <plan.json> <authorized-plan-sha256>');
    const result=command==='prepare'?prepare(file,extra):run(file,extra);
    console.log(JSON.stringify(command==='prepare'?result:{result:path.join(path.dirname(file),'result.json'),stopped:result.stopped,actual_cost_micros:result.actual_cost_micros}));
    if(command==='run'&&(result.stopped||result.runs.some(r=>r.status!=='completed'))) process.exitCode=1;
  } catch(error) {console.error(error.message);process.exitCode=1;}
}
