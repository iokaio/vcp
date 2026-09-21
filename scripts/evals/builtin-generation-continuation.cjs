// SPDX-License-Identifier: Apache-2.0
'use strict';
// Execute only the untouched, already authorized skill arm. Never replay baseline.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process'),same=require('node:util').isDeepStrictEqual;
const prior=require('./p6-live-runner.cjs'),paired=require('./builtin-live-runner.cjs');
const prep=require('./builtin-generation-prepare.cjs'),generation=require('./builtin-generation-runner.cjs');
const {plain,read,write,within,safeChild,filesUnder,noParentInstructions,privateDirectory,usd,frames,inspection,invoke}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),fixture=path.join(repo,'src/evals/skills/builtin/generation-v1');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const hash=file=>sha(read(file,1024*1024*1024)),json=file=>JSON.parse(read(file));
const inventory=directory=>Object.fromEntries(filesUnder(directory).map(file=>[file,hash(safeChild(directory,file))]));

function originalInputs(file,approvedHash){
  if(hash(file)!==approvedHash)throw Error('Original approved generation plan changed');
  const plan=json(file);noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-u03-generation-preparation/1'||!plan.runnable||plan.blockers?.length||!plan.runtime||plain(path.dirname(file))!==plan.directory)throw Error('Original runnable generation plan required');
  const bindings={runner_sha256:'builtin-generation-prepare.cjs',oracle_sha256:'builtin-generation-oracle.cjs',live_runner_sha256:'builtin-generation-runner.cjs',shared_runner_sha256:'p6-live-runner.cjs',skill_runner_sha256:'builtin-live-runner.cjs'};
  for(const [field,name]of Object.entries(bindings))if(plan[field]!==hash(path.join(__dirname,name)))throw Error('Original bound runner source changed');
  const runtimeInput={node:plan.runtime.node,launcher:plan.runtime.launcher};if(plan.runtime.build_receipt)runtimeInput.build_receipt=plan.runtime.build_receipt;
  if(!same(prep.qualifyRuntime(runtimeInput),plan.runtime)||plan.runtime.launcher_build_provenance!=='recorded_local_build')throw Error('Original recorded runtime build changed');
  const spec=json(plan.spec_source);
  if(hash(plan.spec_source)!==plan.spec_sha256||spec.propose_opaque_launcher_effects!==true||!same(plan.permission_review,prep.permissionReview(plan.runtime,true)))throw Error('Original exact permission proposal changed');
  if(plain(path.resolve(spec.executable))!==plan.executable||plain(path.resolve(spec.profile))!==plan.profile_source||prior.micros(spec.aggregate_cap_usd)!==plan.aggregate_cap_micros||!same(prep.qualifyRuntime(spec.runtime),plan.runtime))throw Error('Original plan differs from owner spec');
  const manifest=json(path.join(fixture,'manifest.json'));
  if(plan.fixture_sha256!==hash(path.join(fixture,'manifest.json'))||plan.fixture_revision!==manifest.revision||plan.executable_sha256!==hash(plan.executable)||plan.profile_sha256!==hash(plan.profile_source)||plan.catalog_sha256!==hash(plan.catalog))throw Error('Original fixture, executable or provider changed');
  for(const file of manifest.files){const bytes=read(safeChild(path.join(fixture,'project'),file.path));if(bytes.length!==file.bytes||sha(bytes)!==file.sha256)throw Error('Frozen generation source bytes changed');}
  if(!same(prep.inventory(path.join(path.dirname(plan.executable),'skills/builtin')),plan.assets)||!same(prep.inventory(path.join(repo,'src/skills/builtin')),plan.assets))throw Error('Original packaged skills changed');
  const source=json(plan.profile_source);
  if(prior.profileReasons(source,'fixed_economical').length||source.routing||source.maximum_autonomy!=='workspace'||!same([...source.automatic_effects].sort(),['read','write'])||source.max_requests!==8||source.deadline_seconds!==300||source.output_tokens!=='512'||source.max_transport_retries!==0)throw Error('Original bounded profile expired or changed');
  if(plan.aggregate_cap_micros!==5000000||plan.allocated_cap_micros!==5000000||plan.runs.length!==2)throw Error('Original two $2.50 allocations required');
  const files=Object.fromEntries(manifest.files.map(file=>[file.path,file.sha256]));
  for(const [index,arm]of ['baseline','skill'].entries()){
    const row=plan.runs[index],base=safeChild(plan.directory,arm),workspace=path.join(base,'workspace');
    if(row.arm!==arm||row.cap_micros!==2500000||row.skill!==(arm==='skill'?'vcp-builtin::javascript-typescript::javascript-typescript':null)||!same(row.files,files)||!same(row.editable,manifest.editable)||row.prompt_sha256!==sha(Buffer.from(manifest.prompt))||hash(path.join(base,'prompt.txt'))!==row.prompt_sha256)throw Error('Original frozen arm or prompt changed');
    if(hash(path.join(base,'profile.json'))!==row.profile_sha256||!same(json(path.join(base,'profile.json')),prep.qualifiedProfile(source,workspace,plan.catalog,2500000,plan.runtime,true)))throw Error('Original exact verification profile changed');
    if(arm==='skill'&&(!same(inventory(workspace),files)||fs.readdirSync(path.join(base,'data')).length||fs.existsSync(path.join(base,'attempted.json'))||fs.existsSync(path.join(base,'result.json'))))throw Error('Original skill arm is changed or already attempted');
  }
  return plan;
}

function evidenceRows(pages){
  if(!Array.isArray(pages)||!pages.length||pages.some(p=>!Array.isArray(p.items)||!Array.isArray(p.gaps)||p.gaps.length||p.next_cursor))throw Error('Canonical evidence incomplete');
  const items=pages.flatMap(p=>p.items);if(items.some(item=>item.visibility!=='available'))throw Error('Canonical evidence unavailable');return items;
}
function costDiagnostic(pages,scope){
  const items=evidenceRows(pages),ledgers=items.filter(i=>i.collection==='ledger').map(i=>i.record);
  const attempts=items.filter(i=>i.collection==='attempt').map(i=>i.record),settlements=items.filter(i=>i.collection==='settlement').map(i=>i.record);
  const counter=value=>{if(typeof value!=='string'||!/^(0|[1-9][0-9]*)$/.test(value)||!Number.isSafeInteger(Number(value)))throw Error('Invalid cost diagnostic counter');return Number(value);};
  if(ledgers.length!==1||ledgers[0].currency!=='USD'||ledgers[0].cap!=='2500000'||!same(ledgers[0].scope,scope))throw Error('Cost diagnostic ledger mismatch');
  const ids=new Set();let charged=0;
  for(const attempt of attempts){if(!attempt.id||ids.has(attempt.id)||!same(attempt.scope,scope))throw Error('Cost diagnostic attempt mismatch');ids.add(attempt.id);const value=counter(attempt.charged);charged+=value;if(value&&!settlements.some(s=>s.attempt===attempt.id&&s.applied===true&&same(s.scope,scope)&&counter(s.total)===value))throw Error('Cost diagnostic settlement missing');}
  const ledger=ledgers[0];if(charged!==counter(ledger.settled))throw Error('Cost diagnostic charge sum mismatch');
  return {known_settled_micros:charged,unresolved_micros:counter(ledger.unresolved),active_micros:counter(ledger.active),overrun:ledger.overrun===true};
}
function retainedAccounting(pages,scope){
  const items=evidenceRows(pages),rows=kind=>items.filter(i=>i.collection===kind).map(i=>i.record);
  const ledgers=rows('ledger'),attempts=rows('attempt'),settlements=rows('settlement');
  if(ledgers.length!==1||attempts.length!==6||settlements.length!==5)throw Error('Reviewed baseline six-attempt denominator changed');
  const ledger=ledgers[0];
  if(!same(ledger.scope,scope)||ledger.currency!=='USD'||ledger.cap!=='2500000'||ledger.active!=='0'||ledger.settled!=='6957'||ledger.unresolved!=='201640'||ledger.overrun)throw Error('Reviewed baseline ledger changed');
  const ids=new Set(),settledIds=new Set();let charged=0,unresolved=0;
  for(const attempt of attempts){
    if(!attempt.id||ids.has(attempt.id)||!same(attempt.scope,scope)||attempt.previous||attempt.role!=='main'||attempt.quote?.amount?.currency!=='USD'||attempt.quote.amount.micros!=='201640')throw Error('Baseline attempt identity or reserve changed');ids.add(attempt.id);
    if(attempt.phase==='settled'){
      if(typeof attempt.charged!=='string'||!/^\d+$/.test(attempt.charged))throw Error('Invalid baseline charge');
      const matches=settlements.filter(s=>s.attempt===attempt.id&&s.applied===true&&same(s.scope,scope)&&s.total===attempt.charged);
      if(matches.length!==1||settledIds.has(matches[0].id))throw Error('Baseline settlement missing or duplicated');settledIds.add(matches[0].id);charged+=Number(attempt.charged);
    }else if(attempt.phase==='reconciliation_pending'&&attempt.charged==='0')unresolved++;
    else throw Error('Unexpected baseline attempt state');
  }
  if(charged!==6957||unresolved!==1||settledIds.size!==5)throw Error('Baseline settled and unresolved amounts changed');
  return {known_settled_micros:6957,unresolved_micros:201640,retained_allocation_micros:2500000};
}
function failureEvidence(plan,approvedHash){
  const claimFile=path.join(plan.directory,'execution-claim.json'),resultFile=path.join(plan.directory,'result.json');
  const claim=json(claimFile),result=json(resultFile),base=safeChild(plan.directory,'baseline');
  if(claim.plan_sha256!==approvedHash||result.schema!=='p7-u03-generation-result/1'||result.plan_sha256!==approvedHash||result.stopped!==true||result.actual_cost_micros!==null||result.runs?.length!==2||result.runs[0].arm!=='baseline'||result.runs[0].status!=='failed'||result.runs[0].actual_cost_micros!==null||!same(result.runs[1],{arm:'skill',status:'not_run',actual_cost_micros:null}))throw Error('Original stopped result changed');
  const attempted=json(path.join(base,'attempted.json'));
  const baselineArgs=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd','2.500000','--autonomy','autonomous'];
  if(attempted.plan_sha256!==approvedHash||!same(attempted.args,baselineArgs)||!same(json(path.join(base,'result.json')),result.runs[0]))throw Error('Original baseline attempt/result mismatch');
  const accounting=retainedAccounting(json(path.join(base,'costs.json')),result.runs[0].scope);
  const responseFile=path.join(base,'response-25ab587f-inspected.sse'),bytes=read(responseFile);
  const candidates=evidenceRows(json(path.join(base,'outputs.json'))).filter(i=>i.collection==='artifact'&&i.record?.spec?.channel==='response'&&i.record.sha256===sha(bytes));
  if(candidates.length!==1||candidates[0].record.length!==String(bytes.length)||!same(candidates[0].record.spec.scope,result.runs[0].scope)||candidates[0].record.spec.schema!=='responses-sse-observed-through-terminal/1')throw Error('Retained incomplete response descriptor mismatch');
  const terminal=bytes.toString('utf8').split(/\r?\n/).filter(line=>line.startsWith('data:')).flatMap(line=>{try{return [JSON.parse(line.slice(5))];}catch{return [];}}).filter(frame=>frame.type==='response.incomplete');
  if(terminal.length!==1||terminal[0].response?.status!=='incomplete'||terminal[0].response?.incomplete_details?.reason!=='max_output_tokens')throw Error('Only the reviewed terminal output-limit failure can continue');
  return {...accounting,baseline_inventory:inventory(base),claim_sha256:hash(claimFile),result_sha256:hash(resultFile),retained:result.runs[0],response_sha256:sha(bytes)};
}
function ownerReceipt(file,approvedHash){
  const receipt=json(file);
  if(receipt.schema!=='p7-owner-authorization/1'||receipt.generation_plan_sha256!==approvedHash||receipt.aggregate_cap_usd!=='45.000000'||receipt.retries!==0||receipt.permissions!=='Exact reviewed plans, sole pinned verification launcher'||receipt.authorization_source!=='User response: Approved. Proceed.'||!/^[a-f0-9]{64}$/.test(receipt.readonly_plan_sha256))throw Error('Original owner authorization receipt changed');
  return {file:plain(path.resolve(file)),sha256:hash(file),generation_plan_sha256:approvedHash,readonly_plan_sha256:receipt.readonly_plan_sha256,source:'existing_exact_owner_approval_no_new_attempt_or_allocation'};
}
function prepare(originalFile,approvedHash,destination,receiptFile){
  originalFile=plain(path.resolve(originalFile));destination=plain(path.resolve(destination));
  if(fs.existsSync(destination)||within(repo,destination)||within(destination,repo))throw Error('Fresh private continuation directory required');noParentInstructions(destination);privateDirectory(destination);
  const original=originalInputs(originalFile,approvedHash),failure=failureEvidence(original,approvedHash),owner_authorization=ownerReceipt(receiptFile,approvedHash);
  const plan={schema:'p7-u03-generation-continuation/1',directory:destination,original_plan:originalFile,original_plan_sha256:approvedHash,runner_sha256:hash(__filename),owner_authorization,failure,arm:'skill',remaining_allocation_micros:2500000,aggregate_cap_micros:5000000,integrity_purpose:'continuation_identity_not_new_owner_approval'};
  fs.mkdirSync(destination,{mode:0o700});write(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'),sha256:hash(path.join(destination,'plan.json')),attempts:1,additional_budget_micros:0,model_calls:0};
}
function validate(file,integrity){
  if(hash(file)!==integrity)throw Error('Exact continuation integrity hash required');
  const plan=json(file);noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-u03-generation-continuation/1'||plain(path.dirname(path.resolve(file)))!==plan.directory||plan.runner_sha256!==hash(__filename)||plan.arm!=='skill'||plan.remaining_allocation_micros!==2500000||plan.aggregate_cap_micros!==5000000)throw Error('Continuation contract changed');
  if(fs.existsSync(path.join(plan.directory,'execution-claim.json')))throw Error('Continuation already claimed; no replay');
  const original=originalInputs(plan.original_plan,plan.original_plan_sha256);
  if(!same(plan.owner_authorization,ownerReceipt(plan.owner_authorization.file,plan.original_plan_sha256))||!same(plan.failure,failureEvidence(original,plan.original_plan_sha256)))throw Error('Retained original owner/failure evidence changed');
  return {plan,original};
}
function run(file,integrity,call=invoke){
  const {plan,original}=validate(file,integrity),row=original.runs[1],base=safeChild(original.directory,'skill');
  write(path.join(plan.directory,'execution-claim.json'),{continuation_sha256:integrity,original_plan_sha256:plan.original_plan_sha256,owner_authorization_sha256:plan.owner_authorization.sha256,at:new Date().toISOString(),meaning:'Only original untouched skill allocation; no baseline retry.'});
  const profile=json(path.join(base,'profile.json'));
  const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy','autonomous','--skill',row.skill];
  write(path.join(base,'attempted.json'),{plan_sha256:plan.original_plan_sha256,continuation_sha256:integrity,args,at:new Date().toISOString()});
  const report={arm:'skill',status:'failed',actual_cost_micros:null};
  const result={schema:'p7-u03-generation-continuation-result/1',plan_sha256:integrity,original_plan_sha256:plan.original_plan_sha256,actual_cost_micros:null,known_settled_micros:6957,known_settled_complete:true,original_unresolved_micros:201640,retained_baseline_allocation_micros:2500000,new_unknown_allocation_micros:0,stopped:false,runs:[plan.failure.retained,report]};
  let costPages;
  try{
    const execution=call(original.executable,args,(profile.deadline_seconds+180)*1000);
    write(path.join(base,'stdout.jsonl'),execution.stdout);write(path.join(base,'stderr.txt'),execution.stderr);
    if(execution.error)throw Error('Skill CLI interrupted; inspect new liability before further work');
    const output=frames(execution.stdout),accepted=output.find(frame=>frame.type==='accepted'),final=output.findLast(frame=>frame.type==='result');
    if(!accepted?.scope?.task||!final?.conditions)throw Error('Durable skill task result missing');report.scope=accepted.scope;
    const evidence={};for(const view of ['costs','routing','outputs','context','verification']){evidence[view]=inspection(original,base,accepted.scope.task,view,call);if(view==='costs')costPages=evidence[view];write(path.join(base,view+'.json'),evidence[view]);}
    const money=prior.accounting(evidence.costs,row.cap_micros);report.actual_cost_micros=money.actual_cost_micros;result.known_settled_micros+=money.actual_cost_micros;
    report.skill_evidence=paired.skillEvidence(original,base,{...row,skill:'javascript-typescript'},evidence.context,money.attempts,call);
    if(execution.status===0&&final.conditions.completed===true){
      report.verification=generation.verificationEvidence(evidence.verification,accepted.scope.task);
      const observed=spawnSync(original.runtime.node,[path.join(__dirname,'builtin-generation-oracle.cjs'),path.join(base,'workspace')],{env:process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{},encoding:'utf8',timeout:10000,maxBuffer:65536,windowsHide:true});
      if(observed.error||![0,1].includes(observed.status))throw Error('Independent generation oracle unavailable');report.oracle=JSON.parse(observed.stdout);
      if(observed.status===0&&report.oracle.pass===true)report.status='completed';
    }else report.reason='CLI did not complete; oracle cannot override';
  }catch(error){report.reason=error.message;result.stopped=true;if(report.actual_cost_micros===null){result.new_unknown_allocation_micros=2500000;try{report.cost_diagnostic=costDiagnostic(costPages,report.scope);result.known_settled_micros+=report.cost_diagnostic.known_settled_micros;}catch{report.cost_diagnostic=null;result.known_settled_complete=false;}}}
  try{if(!same(plan.failure,failureEvidence(original,plan.original_plan_sha256)))throw Error('changed');result.baseline_preserved=true;}catch{result.baseline_preserved=false;result.stopped=true;report.status='failed';report.baseline_preservation_error='Retained baseline evidence changed during continuation';}
  write(path.join(base,'result.json'),report);write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={prepare,validate,run,retainedAccounting,failureEvidence,ownerReceipt,costDiagnostic};
if(require.main===module){try{const [command,file,digest,destination,receipt,...rest]=process.argv.slice(2);if(rest.length||!file||!digest||!['prepare','run'].includes(command)||(command==='prepare'?(!destination||!receipt):(destination||receipt)))throw Error('Usage: prepare <original-plan> <approved-hash> <new-directory> <owner-receipt> | run <continuation-plan> <integrity-hash>');const result=command==='prepare'?prepare(file,digest,destination,receipt):run(file,digest);console.log(JSON.stringify(result));if(command==='run'&&(result.stopped||result.runs[1].status!=='completed'))process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
