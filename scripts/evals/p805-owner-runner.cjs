// SPDX-License-Identifier: Apache-2.0
'use strict';
// Exact six-slot owner cohort. Human scoring and final acceptance stay pending.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawn,spawnSync}=require('node:child_process');
const prior=require('./p6-live-runner.cjs'),profiles=require('./builtin-live-runner.cjs');
const {plain,read,write,privateDirectory,noParentInstructions,frames,inspection}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),fixture=path.join(repo,'src/evals/release/p8-owner-v3');
const materializer=require(path.join(fixture,'tools/prepare.cjs'));
const inventory=require('../package-inventory.cjs');
const CAP=8000000,AGGREGATE=48000000;
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const hash=file=>sha(read(plain(file),1024*1024*1024));
const parse=file=>JSON.parse(read(file));
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const common=row=>['--format','jsonl','--non-interactive','--workspace',row.workspace,'--data-dir',row.data];

function preservation(row,gitExecutable,before=false) {
  const observed=materializer.inventory(row.workspace),expected=row.workspace_files;
  const allowed=before||row.case!=='U03'?new Set():new Set(['src/domain/window.cjs','src/api/page.cjs']);
  if(!same(observed.map(r=>r.path),expected.map(r=>r.path)))throw Error('Workspace file set changed');
  for(let i=0;i<expected.length;i++)if(!allowed.has(expected[i].path)&&!same(observed[i],expected[i]))throw Error('Protected workspace bytes changed: '+expected[i].path);
  if(row.git){
    if(hash(path.join(row.workspace,'.git/index'))!==row.git.index_sha256||hash(path.join(row.workspace,'.git/config'))!==row.git.config_sha256)throw Error('Prepared Git index or configuration changed');
    const result=spawnSync(gitExecutable,['-c','safe.directory='+row.workspace.replaceAll('\\','/'),'rev-parse','HEAD'],{cwd:row.workspace,env:{SystemRoot:process.env.SystemRoot||'',GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:process.platform==='win32'?'NUL':'/dev/null',GIT_OPTIONAL_LOCKS:'0'},encoding:'utf8',windowsHide:true,timeout:10000,maxBuffer:65536});
    if(result.error||result.status!==0||result.stdout.trim()!==row.git.head)throw Error('Prepared Git HEAD changed or unavailable');
  }
  return {passed:true,allowed_changes:observed.filter((r,i)=>!same(r,expected[i])).map(r=>r.path),git_index_preserved:!!row.git};
}

// Recheck immutable execution inputs without requiring already completed U03
// workspaces to retain their initial bytes. The campaign and canonical stores
// are mutable evidence, not frozen inputs.
function frozenInputs(plan,file,expected) {
  if(hash(file)!==expected)throw Error('Exact frozen plan hash required');
  for(const [name,digest]of Object.entries(plan.runner_hashes))if(hash(name)!==digest)throw Error('Bound runner changed');
  for(const [name,digest]of Object.entries(plan.input_hashes))if(hash(name)!==digest)throw Error('Frozen input changed');
  const manifestFile=path.join(fixture,'manifest.json');
  if(hash(manifestFile)!==plan.fixture_manifest_sha256)throw Error('Frozen fixture manifest changed');
  for(const row of parse(manifestFile).files)if(hash(path.join(fixture,row.path))!==row.sha256)throw Error('Frozen fixture file changed');
  const preparation=parse(plan.preparation_file),pkg=parse(plan.package_file);
  if(hash(preparation.git_executable)!==preparation.git_sha256)throw Error('Pinned native Git changed');
  inventory.verifyManifest(path.join(path.dirname(plan.package_file),'package'),pkg.manifest);
  for(const row of plan.runs)if(hash(row.profile)!==row.profile_sha256||hash(row.prompt)!==row.prompt_sha256)throw Error('Profile or prompt changed');
  return {passed:true};
}

function validate(file,expected) {
  if(!/^[a-f0-9]{64}$/.test(expected)||hash(file)!==expected)throw Error('Exact frozen plan hash required');
  const plan=parse(file);
  if(plan.schema!=='p805-owner-execution-binding/1'||plan.runnable!==true||plan.aggregate_cap_micros!==AGGREGATE||plain(path.dirname(path.resolve(file)))!==plan.directory)throw Error('Runnable six-slot binding required');
  privateDirectory(plan.directory);noParentInstructions(path.dirname(plan.directory));
  if(plan.campaign!==path.join(repo,'artifacts/p7-p8-owner-campaign.json'))throw Error('Existing owner campaign required');
  const required=['p805-owner-runner.cjs','p805-owner-prepare.cjs','p6-live-runner.cjs','builtin-live-runner.cjs','package-inventory.cjs'].map(name=>path.join(repo,name==='package-inventory.cjs'?'scripts':'scripts/evals',name));
  for(const name of required)if(plan.runner_hashes?.[name]!==hash(name))throw Error('Bound runner changed');
  for(const [name,digest]of Object.entries(plan.input_hashes))if(hash(name)!==digest)throw Error('Frozen input changed');
  for(const file of [plan.preparation_file,plan.package_file,plan.source_profile,plan.launcher_file])if(!plan.input_hashes[file])throw Error('Missing exact prerequisite binding');
  const manifest=parse(path.join(fixture,'manifest.json'));
  if(hash(path.join(fixture,'manifest.json'))!==plan.fixture_manifest_sha256||manifest.revision!==plan.fixture_revision||!same(plan.limits,manifest.proposed_limits))throw Error('Fixture or proposed thresholds changed');
  for(const row of manifest.files)if(hash(path.join(fixture,row.path))!==row.sha256)throw Error('Frozen fixture file changed');
  const preparation=parse(plan.preparation_file),pkg=parse(plan.package_file),source=parse(plan.source_profile),launcher=parse(plan.launcher_file);
  if(preparation.fixture_manifest_sha256!==plan.fixture_manifest_sha256||hash(preparation.git_executable)!==preparation.git_sha256)throw Error('Preparation or native Git changed');
  const reasons=profiles.fixedProfileReasons(source,Date.now()+900000);if(reasons.length)throw Error(reasons.join('; '));
  if(source.provider.compatibility.model!=='qwen/qwen3.8-max-0902'||source.output_tokens!=='16384')throw Error('Qualified frozen Qwen allowance required');
  const packageRoot=path.join(path.dirname(plan.package_file),'package');inventory.verifyManifest(packageRoot,pkg.manifest);
  if(hash(path.join(path.dirname(plan.package_file),pkg.package))!==plan.package_sha256||pkg.archive_sha256!==plan.package_sha256||plan.executable!==path.join(packageRoot,'vcp.exe')||hash(plan.executable)!==plan.executable_sha256)throw Error('Exact production package required');
  const schedule=['u01-sqlite','u01-files','u02-sqlite','u02-files','u03-sqlite','u03-files'];
  if(plan.runs.length!==6||preparation.runs.length!==6)throw Error('Six first attempts required');
  for(const [index,row]of plan.runs.entries()){
    const original=preparation.runs[index],generation=row.case==='U03',base=path.join(plan.directory,row.id);
    if(row.id!==schedule[index]||row.cap_micros!==CAP||row.workspace!==path.join(base,'workspace')||row.data!==path.join(base,'data')||row.profile!==path.join(base,'owner-profile.json'))throw Error('Frozen schedule, paths or cap changed');
    for(const key of Object.keys(original))if(!same(row[key],original[key]))throw Error('Materialized slot changed');
    const profile=parse(row.profile);
    if(hash(row.profile)!==row.profile_sha256||hash(row.prompt)!==row.prompt_sha256)throw Error('Profile or prompt changed');
    const derived={...source,workspace:row.workspace,budget_usd:'8.000000',max_requests:16,deadline_seconds:900,provider_timeout_seconds:360,max_transport_retries:0,output_tokens:source.output_tokens,maximum_autonomy:generation?'autonomous':'plan',automatic_effects:generation?['read','write','execute','network','install','publish','opaque']:[],affected_paths:generation?['src/domain/window.cjs','src/api/page.cjs']:row.workspace_files.map(f=>f.path),processes:[],checks:[]};
    if(generation){derived.processes=[{name:'p805-page-node',executable:launcher.launcher,environment:{SystemRoot:process.env.SystemRoot},required_isolation:[],reduced_isolation:true,inputs:[]}];derived.checks=[{manifest:'package.json',runner:'node',profile:'p805-page-node',expected_tests:['empty array','small default page is a copy','invalid array'],rationale:'Frozen P8-05 pagination integrated-parent check'}];}
    if(!same(profile,derived))throw Error('Exact reviewed task authority changed');
    const args=[...common(row),'--config',row.profile,'run','--file',row.prompt,'--budget-usd','8.000000','--autonomy',generation?'autonomous':'plan'];
    if(row.paid_command.program!==plan.executable||!same(row.paid_command.args,args)||row.backend_command.program!==plan.executable||!same(row.backend_command.args,[...common(row),'storage','configure','--backend',row.backend]))throw Error('Bound command changed');
    const grader=generation?{program:launcher.node,args:['--permission','--max-old-space-size=64','--allow-fs-read='+row.workspace,'--allow-fs-read='+path.join(fixture,'u03/hidden/oracle.cjs'),path.join(fixture,'u03/hidden/oracle.cjs'),row.workspace],clear_environment:true,timeout_ms:10000,expected_exit:0}:null;
    if(!same(row.hidden_grader,grader))throw Error('Hidden oracle permission fence changed');
    preservation(row,preparation.git_executable,true);
    if(fs.existsSync(path.join(row.data,'workspaces'))&&fs.readdirSync(path.join(row.data,'workspaces')).some(name=>fs.existsSync(path.join(row.data,'workspaces',name,'workspace.json'))))throw Error('Slot already has accepted canonical work');
  }
  return {plan,preparation};
}

function changeCampaign(file,change) {
  // Same lock as the separately authorized interactive runner. A stale lock blocks.
  const lock=file+'.production-interactive.lock',fd=fs.openSync(lock,'wx');
  try{
    const budget=parse(file);
    if(budget.schema!=='p7-p8-owner-campaign/1'||budget.cap_micros!==100000000||![budget.settled_micros,budget.reserved_micros].every(n=>Number.isSafeInteger(n)&&n>=0))throw Error('Existing campaign amounts changed');
    change(budget);
    if(budget.settled_micros+budget.reserved_micros>budget.cap_micros)throw Error('Campaign ceiling exceeded');
    const temporary=file+'.'+crypto.randomUUID()+'.tmp';write(temporary,budget);fs.renameSync(temporary,file);
  }finally{fs.closeSync(fd);fs.unlinkSync(lock);}
}

function reserve(budget,plan,expected,row) {
  if(!budget.models.includes('qwen/qwen3.8-max-0902')||budget.runs.some(r=>(r.sha256===expected&&r.slot===row.id)||['prepared','running','running-held'].includes(r.status)))throw Error('Duplicate slot or outstanding launch reservation');
  const cohort=budget.runs.filter(r=>r.sha256===expected);
  if(cohort.reduce((sum,r)=>sum+r.cap_micros,0)+CAP>AGGREGATE)throw Error('Owner cohort exceeds $48');
  budget.reserved_micros+=CAP;budget.runs.push({model:'qwen/qwen3.8-max-0902',stage:'p805-owner',slot:row.id,cap_micros:CAP,plan:plan.directory,sha256:expected,status:'running-held',actual_cost_micros:null});
}

function bounded(program,args,timeout,env=process.env) {
  return new Promise(resolve=>{
    let child,stdout='',stderr='',size=0,error=null,done=false,timer,reapTimer,drainTimer;
    const finish=status=>{if(done)return;done=true;clearTimeout(timer);clearTimeout(reapTimer);clearTimeout(drainTimer);resolve({status,stdout,stderr,error,process_reaped:!!child&&(child.exitCode!==null||child.signalCode!==null)});};
    const abandon=()=>{child?.stdout?.destroy();child?.stderr?.destroy();child?.unref();finish(child?.exitCode??null);};
    const terminate=reason=>{if(error)return;error=reason;if(child?.pid){if(process.platform==='win32')spawnSync(path.join(process.env.SystemRoot,'System32/taskkill.exe'),['/PID',String(child.pid),'/T','/F'],{windowsHide:true,timeout:5000,stdio:'ignore'});try{child.kill('SIGKILL');}catch{}}reapTimer=setTimeout(abandon,5000);};
    try{child=spawn(program,args,{shell:false,windowsHide:true,env,stdio:['ignore','pipe','pipe']});}catch(e){error=e.code||'spawn_failed';finish(null);return;}
    for(const stream of ['stdout','stderr'])child[stream].setEncoding('utf8').on('data',chunk=>{size+=Buffer.byteLength(chunk);if(size>16*1024*1024){terminate('output_limit');return;}if(stream==='stdout')stdout+=chunk;else stderr+=chunk;});
    child.on('error',e=>{error=e.code||'spawn_failed';finish(null);});child.on('close',finish);
    child.on('exit',()=>{drainTimer=setTimeout(()=>{error||='pipe_drain_deadline';abandon();},5000);});
    timer=setTimeout(()=>terminate('deadline_exceeded'),timeout);
  });
}

function artifactBytes(plan,base,item,call) {
    const descriptor=item.record,length=Number(descriptor.length);
    if(item.visibility!=='available'||descriptor.state!=='complete'||!Number.isSafeInteger(length)||length<0||length>1024*1024)throw Error('Incomplete or unbounded response capture');
    const chunks=[];
    for(let offset=0;offset<length;offset+=65536){
      const ranged=inspection(plan,base,item.id,'outputs',call,['--offset',String(offset),'--length','65536']),row=ranged[0]?.items?.[0],end=Math.min(offset+65536,length);
      if(ranged.length!==1||ranged[0].items.length!==1||ranged.some(p=>p.gaps.some(g=>!prior.privacyGap(g,item.id)))||row?.range?.start!==offset||row.range.end!==end||!Array.isArray(row.bytes)||row.bytes.length!==end-offset||row.bytes.some(b=>!Number.isInteger(b)||b<0||b>255)||(row.artifact!==undefined&&row.artifact!==item.id)||(row.visibility!==undefined&&row.visibility!=='available'))throw Error('Response range has gaps');
      chunks.push(Buffer.from(row.bytes));
    }
    const bytes=Buffer.concat(chunks);if(bytes.length!==length||sha(bytes)!==descriptor.sha256)throw Error('Response identity mismatch');
    return bytes;
}
function naturalAnswer(plan,base,pages,attempts,call,capture) {
  const answers=[];
  for(const item of pages.flatMap(p=>p.items).filter(i=>i.collection==='artifact'&&i.record?.spec?.channel==='response')){
    const bytes=artifactBytes(plan,base,item,call);
    capture('response-'+sha(Buffer.from(item.id))+'.sse',bytes.toString('utf8'));
    for(const block of bytes.toString('utf8').split(/\r?\n\r?\n/)){
      const data=block.split(/\r?\n/).filter(l=>l.startsWith('data:')).map(l=>l.slice(5).trimStart()).join('\n');if(!data||data==='[DONE]')continue;
      const event=JSON.parse(data);if(event.type!=='response.completed'||event.response?.status!=='completed')continue;
      const response=event.response;if(!attempts.some(a=>a.provider_request===response.id)||response.output?.some(o=>o.type==='function_call'))continue;
      const text=(response.output||[]).flatMap(o=>(o.content||[]).filter(c=>c.type==='output_text').map(c=>c.text)).join('');
      if(text)answers.push({text,artifact:item.id,provider_request:response.id,served_model:response.model??null});
    }
  }
  if(answers.length!==1)throw Error('Expected one unambiguous canonical natural-language final answer');
  return answers[0];
}

function verificationEvidence(pages,task) {
  if(pages.some(p=>p.gaps.length))throw Error('Canonical verification has gaps');
  if(task?.state!=='completed'||task.parent||task.editing!==true||!same(task.required_checks,['package.json#test']))throw Error('Completed integrated parent required');
  const passed=pages.flatMap(p=>p.items).filter(i=>i.collection==='verification'&&i.visibility==='available').map(i=>i.record).filter(v=>same(v.scope,task.scope)&&v.steering===task.steering&&same(v.fingerprint,task.fingerprint)&&!v.redaction&&v.outputs?.length&&!v.unresolved_effects?.length&&!v.outstanding_issues?.length&&v.cost?.certainty==='known'&&v.checks?.length===1&&v.checks[0].specification==='package.json#test'&&v.checks[0].outcome?.status==='passed'&&v.checks[0].exit_code===0);
  if(!passed.length)throw Error('No canonical successful current-parent verification');return passed.map(v=>v.id);
}

function sourceManifest(manifest,workspace) {
  if(manifest?.bounded_scan_complete!==true||!Array.isArray(manifest.files)||manifest.files.length>256)throw Error('Complete bounded source manifest required');
  const seen=new Set();
  for(const file of manifest.files){
    const source=prior.boundaries.safeChild(workspace,file.path);
    if(seen.has(file.path)||hash(source)!==file.sha256||fs.statSync(source).size!==Number(file.bytes))throw Error('Verified source differs from current parent');seen.add(file.path);
  }
  if(!['src/domain/window.cjs','src/api/page.cjs','package.json','test/page.test.cjs'].every(file=>seen.has(file)))throw Error('Integrated source/check manifest incomplete');
  return {passed:true,files:seen.size};
}

function currentSources(plan,base,row,outputs,verification,ids,call,capture) {
  const selected=verification.flatMap(p=>p.items).filter(i=>ids.includes(i.id)||ids.includes(i.record?.id)).map(i=>i.record);
  const artifacts=outputs.flatMap(p=>p.items).filter(i=>i.collection==='artifact'&&i.record?.spec?.schema==='verification-result/1'&&selected.some(v=>v.outputs.includes(i.id)));
  const matched=[];
  for(const item of artifacts){
    const text=artifactBytes(plan,base,item,call).toString('utf8');capture('verification-source-'+sha(Buffer.from(item.id))+'.json',text);
    const evidence=JSON.parse(text);
    if(evidence.applicability!=='current'||evidence.observation_error||!same(evidence.before,evidence.after))continue;
    matched.push({artifact:item.id,...sourceManifest(evidence.after,row.workspace)});
  }
  if(!matched.length)throw Error('Current integrated source evidence unavailable');return matched;
}

async function run(file,expected) {
  const {plan,preparation}=validate(file,expected),secret=process.env.OPENROUTER_API_KEY;
  if(process.platform!=='win32'||!secret)throw Error('Native Windows and existing credential environment required');
  write(path.join(plan.directory,'owner-execution-claim.json'),{plan_sha256:expected,at:new Date().toISOString(),maximum_allocation_micros:AGGREGATE,meaning:'One shot; no retries or human acceptance implied'});
  const result={schema:'p805-owner-result/1',plan_sha256:expected,status:'running',stopped:false,actual_cost_micros:0,unknown_upper_bound_micros:0,human_acceptance:'pending',runs:[]};
  let leaked=false;
  const redact=value=>{const text=typeof value==='string'?value:JSON.stringify(value,null,2)+'\n';if(text.includes(secret))leaked=true;return text.split(secret).join('[REDACTED_PROVIDER_CREDENTIAL]');};
  const save=()=>fs.writeFileSync(path.join(plan.directory,'owner-result.json'),redact(result),{mode:0o600});
  save();
  for(const row of plan.runs){
    const report={id:row.id,case:row.case,backend:row.backend,status:'not_run',actual_cost_micros:null,human_rubric:'pending',failures:[]};result.runs.push(report);if(result.stopped){save();continue;}
    const base=path.dirname(row.workspace),capture=(name,value)=>write(path.join(base,name),redact(value));let reserved=false,accounted=false;
    const call=(program,args,timeout)=>{if(program!==plan.executable||hash(program)!==plan.executable_sha256)throw Error('Frozen inspection executable changed');const output=prior.boundaries.invoke(program,args,timeout);redact(output);if(leaked)throw Error('Provider credential appeared before redaction');return output;};
    try{
      frozenInputs(plan,file,expected);
      const reasons=profiles.fixedProfileReasons(parse(plan.source_profile),Date.now()+900000);if(reasons.length)throw Error(reasons.join('; '));
      preservation(row,preparation.git_executable,true);
      const withoutCredential={...process.env};delete withoutCredential.OPENROUTER_API_KEY;
      const backend=await bounded(row.backend_command.program,row.backend_command.args,10000,withoutCredential);capture('backend.json',backend);
      if(backend.status!==0||backend.error)throw Error('Backend selection failed before launch');
      const preflight=await bounded(row.paid_command.program,row.paid_command.args,10000,withoutCredential);capture('credential-free.json',preflight);
      if(preflight.status!==2||preflight.error||!preflight.stderr.includes('OPENROUTER_API_KEY is required'))throw Error('Credential-free exact profile preflight failed');
      const workspaces=path.join(row.data,'workspaces');
      if(fs.existsSync(workspaces)&&fs.readdirSync(workspaces).some(name=>fs.existsSync(path.join(workspaces,name,'workspace.json'))))throw Error('Credential-free preflight unexpectedly accepted canonical work');
      if(leaked)throw Error('Credential exposure detected before paid launch');
      frozenInputs(plan,file,expected);
      changeCampaign(plan.campaign,budget=>reserve(budget,plan,expected,row));reserved=true;
      report.status='running';capture('attempted.json',{plan_sha256:expected,slot:row.id,cap_micros:CAP,arguments:row.paid_command.args,at:new Date().toISOString()});save();
      const began=Date.now(),execution=await bounded(row.paid_command.program,row.paid_command.args,900000);
      report.latency_ms=Date.now()-began;report.exit_code=execution.status;report.process_error=execution.error;report.process_reaped=execution.process_reaped;capture('stdout.jsonl',execution.stdout);capture('stderr.txt',execution.stderr);
      if(execution.error||!execution.process_reaped)result.stopped=true;
      if(!execution.process_reaped)throw Error('CLI owner not reaped; retain full reservation and stop');
      const output=frames(execution.stdout),accepted=output.find(f=>f.type==='accepted'),final=output.findLast(f=>f.type==='result');
      if(!accepted?.scope?.task)throw Error('Accepted task identity missing; retain full reservation');report.scope=accepted.scope;
      // Always reconcile costs first, even when the task or quality gate failed.
      const costs=inspection(plan,base,accepted.scope.task,'costs',call);capture('costs.json',costs);
      const money=prior.accounting(costs,CAP);report.actual_cost_micros=money.actual_cost_micros;accounted=true;
      const evidence={};for(const view of ['outputs','context','verification','routing']){evidence[view]=inspection(plan,base,accepted.scope.task,view,call);capture(view+'.json',evidence[view]);}
      report.preservation=preservation(row,preparation.git_executable);
      if(execution.error||execution.status!==0||final?.conditions?.completed!==true)throw Error('CLI did not complete within the declared bound');
      report.answer=naturalAnswer(plan,base,evidence.outputs,money.attempts,call,capture);capture('answer.txt',report.answer.text);
      if(row.case==='U03'){
        const status=call(plan.executable,[...common(row),'tasks','status',accepted.scope.task],30000);capture('task-status.json',status);
        const data=status.status===0&&!status.error?frames(status.stdout).find(f=>f.type==='result')?.data:null;
        if(!data||data.truncated||data.records?.length!==1||data.records[0].scope.task!==accepted.scope.task)throw Error('Fresh canonical parent status unavailable');
        report.verification=verificationEvidence(evidence.verification,data.records[0]);
        report.current_sources=currentSources(plan,base,row,evidence.outputs,evidence.verification,report.verification,call,capture);
        const grade=await bounded(row.hidden_grader.program,row.hidden_grader.args,10000,{SystemRoot:process.env.SystemRoot});capture('hidden-oracle.json',grade);
        if(grade.error||!grade.process_reaped)result.stopped=true;
        if(grade.error||!grade.process_reaped||grade.status!==0)throw Error('Hidden feature oracle failed');report.oracle=JSON.parse(grade.stdout);
        if(report.oracle.pass!==true||report.oracle.passed!==37||report.oracle.total!==37)throw Error('Hidden feature assertions incomplete');
        report.preservation=preservation(row,preparation.git_executable);
      }
      if(leaked)throw Error('Provider credential appeared before redaction');
      report.status='automated-checks-passed-human-review-pending';
    }catch(error){report.status='failed';report.failures.push(String(error.message));if(!reserved||!accounted||leaked)result.stopped=true;}
    finally{
      try{report.frozen_inputs=frozenInputs(plan,file,expected);}catch(error){report.frozen_inputs={passed:false,reason:String(error.message)};report.failures.push(String(error.message));report.status='failed';result.stopped=true;}
      // Never launch a changed Git binary after an integrity failure. Known
      // canonical costs still settle below; unknown costs retain the full cap.
      if(report.frozen_inputs.passed){try{report.preservation=preservation(row,preparation.git_executable);}catch(error){report.preservation={passed:false,reason:String(error.message)};report.failures.push(String(error.message));report.status='failed';result.stopped=true;}}
      if(reserved){
        try{changeCampaign(plan.campaign,budget=>{const ledger=budget.runs.find(r=>r.sha256===expected&&r.slot===row.id);if(!ledger||ledger.status!=='running-held')throw Error('Campaign slot ownership changed');if(accounted){budget.reserved_micros-=CAP;budget.settled_micros+=report.actual_cost_micros;ledger.actual_cost_micros=report.actual_cost_micros;ledger.status=report.status==='failed'?'failed-reconciled':'observed-human-pending';}else{ledger.status='failed-unknown';ledger.unresolved_upper_bound_micros=CAP;}});}catch(error){report.failures.push('Campaign reconciliation failed: '+error.message);result.stopped=true;accounted=false;}
        if(accounted)result.actual_cost_micros+=report.actual_cost_micros;else result.unknown_upper_bound_micros+=CAP;
      }
      report.credential_exposure=leaked;capture('owner-slot-result.json',report);save();
    }
  }
  result.status=result.stopped?'stopped':result.runs.every(r=>r.status==='automated-checks-passed-human-review-pending')?'human-review-pending':'failed';save();return result;
}

module.exports={validate,frozenInputs,preservation,changeCampaign,reserve,bounded,naturalAnswer,verificationEvidence,sourceManifest,run};
if(require.main===module){(async()=>{const [command,file,digest,...extra]=process.argv.slice(2);if(!['validate','run'].includes(command)||!file||!digest||extra.length)throw Error('Usage: p805-owner-runner.cjs validate|run <plan.json> <exact-plan-sha256>');if(command==='validate'){validate(path.resolve(file),digest);console.log(JSON.stringify({status:'validated',model_calls:0}));}else{const result=await run(path.resolve(file),digest);console.log(JSON.stringify({directory:path.dirname(file),status:result.status,runs:result.runs.length,actual_cost_micros:result.actual_cost_micros,unknown_upper_bound_micros:result.unknown_upper_bound_micros,human_acceptance:'pending'}));if(result.status!=='human-review-pending')process.exitCode=1;}})().catch(error=>{console.error(error.message);process.exitCode=1;});}
