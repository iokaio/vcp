// SPDX-License-Identifier: Apache-2.0
'use strict';
// Continuation within the original owner-authorized rows, never task replay.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const paired=require('./builtin-live-runner.cjs'),prior=require('./p6-live-runner.cjs');
const {plain,read,write,within,safeChild,filesUnder,noParentInstructions,privateDirectory,usd,frames,inspection,invoke}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const json=file=>JSON.parse(read(file)),hash=file=>sha(read(file,1024*1024*1024));
const same=require('node:util').isDeepStrictEqual;
const inventory=directory=>Object.fromEntries(filesUnder(directory).map(p=>[p,hash(safeChild(directory,p))]));
function originalInputs(file,expectedHash) {
  if(hash(file)!==expectedHash)throw Error('Original approved plan hash changed');
  const plan=json(file),selected=paired.pool();
  noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-builtin-live-plan/1'||plain(path.dirname(file))!==plan.directory||plan.fixture_sha256!==selected.sha256||plan.fixture_revision!==selected.revision||plan.runner_sha256!==hash(path.join(__dirname,'builtin-live-runner.cjs'))||plan.shared_runner_sha256!==hash(path.join(__dirname,'p6-live-runner.cjs'))||plan.executable_sha256!==hash(plan.executable))throw Error('Original source or executable changed');
  if(!same(inventory(path.join(path.dirname(plan.executable),'skills/builtin')),plan.assets)||!same(inventory(path.join(repo,'src/skills/builtin')),plan.assets))throw Error('Packaged assets changed');
  if(hash(plan.profile_source)!==plan.profile_sha256||hash(plan.catalog)!==plan.catalog_sha256)throw Error('Original provider source changed');
  if(plan.aggregate_cap_micros!==40000000||plan.allocated_cap_micros!==40000000||plan.runs.length!==16)throw Error('Original $40 allocation required');
  const source=json(plan.profile_source);
  if(paired.profileReasons(source).length)throw Error('Original profile expired or invalid');
  for(let i=0;i<16;i++) {
    const row=plan.runs[i],task=selected.cases[Math.floor(i/2)],arm=i%2?'skill':'baseline';
    const frozen=Object.fromEntries(task.expected.preserve_files.map(f=>[f.path,f.sha256]));
    const prompt=task.prompt+'\nRead the relevant project files. Give a concise JSON final answer with fields findings (array), evidence (array of file paths and observations), recommended_checks (array), not_run (array with reasons), and recommendation (string). Do not change files. Do not claim any check ran without a receipt. Run vcp_verify as required by the host.\n';
    if(row.id!==task.id+'--'+arm||row.case_id!==task.id||row.skill!==task.skill||row.arm!==arm||row.cap_micros!==2500000||!same(row.files,frozen)||row.prompt_sha256!==sha(Buffer.from(prompt)))throw Error('Original cohort or allocation changed');
    const base=safeChild(plan.directory,row.id),profile=json(path.join(base,'profile.json'));
    if(!same(inventory(path.join(base,'workspace')),Object.fromEntries(Object.entries(frozen).sort()))||hash(path.join(base,'prompt.txt'))!==row.prompt_sha256||hash(path.join(base,'profile.json'))!==row.profile_sha256)throw Error('Original run inputs changed');
    const derived={...source,workspace:path.join(base,'workspace'),catalog:plan.catalog,budget_usd:usd(row.cap_micros),affected_paths:Object.keys(frozen)};
    if(!same(profile,derived)||paired.profileReasons(profile).length)throw Error('Derived profile expired or invalid');
    if(i>0&&(fs.readdirSync(path.join(base,'data')).length||fs.existsSync(path.join(base,'attempted.json'))||fs.existsSync(path.join(base,'result.json'))))throw Error('Continuation row was already attempted');
  }
  return plan;
}
function failureEvidence(plan,approvedHash) {
  const claim=json(path.join(plan.directory,'execution-claim.json')),result=json(path.join(plan.directory,'result.json'));
  if(claim.plan_sha256!==approvedHash||result.plan_sha256!==approvedHash||result.fixture_sha256!==plan.fixture_sha256||result.schema!=='p7-builtin-live-result/1'||result.stopped!==true||result.actual_cost_micros!==null||result.runs?.length!==16)throw Error('Original stopped claim/result missing');
  result.runs.forEach((r,i)=>{if(r.case_id!==plan.runs[i].case_id||r.arm!==plan.runs[i].arm||r.status!==(i?'not_run':'failed')||r.actual_cost_micros!==null)throw Error('Original result denominator changed');});
  const base=safeChild(plan.directory,plan.runs[0].id),attempted=json(path.join(base,'attempted.json'));
  if(attempted.plan_sha256!==approvedHash||!same(json(path.join(base,'result.json')),result.runs[0]))throw Error('Original attempt/result mismatch');
  const pages=json(path.join(base,'costs.json'));
  if(!Array.isArray(pages)||!pages.length||pages.some(p=>!Array.isArray(p.items)||p.gaps?.length||p.next_cursor))throw Error('Original cost evidence incomplete');
  const items=pages.flatMap(p=>p.items);if(items.some(i=>i.visibility!=='available'))throw Error('Original cost evidence unavailable');
  const rows=kind=>items.filter(i=>i.collection===kind).map(i=>i.record),ledgers=rows('ledger'),attempts=rows('attempt');
  if(ledgers.length!==1||attempts.length!==1||rows('settlement').length)throw Error('Exactly one unresolved original attempt required');
  const ledger=ledgers[0],attempt=attempts[0];
  if(ledger.currency!=='USD'||ledger.cap!=='2500000'||ledger.active!=='0'||ledger.settled!=='0'||ledger.unresolved!=='201640'||ledger.overrun||attempt.phase!=='reconciliation_pending'||attempt.charged!=='0'||attempt.previous||attempt.role!=='main'||attempt.quote?.amount?.micros!=='201640'||!same(attempt.scope,result.runs[0].scope)||!same(ledger.scope,attempt.scope))throw Error('Original unresolved liability differs');
  const response=json(path.join(base,'retained-response-inspection.json'));
  if(response.length!==1||response[0].items?.length!==1)throw Error('Original response evidence missing');
  const capture=response[0].items[0],bytes=Buffer.from(capture.bytes||[]);
  if(!Array.isArray(capture.bytes)||capture.bytes.some(b=>!Number.isInteger(b)||b<0||b>255)||capture.descriptor?.sha256!==sha(bytes)||capture.descriptor?.length!==String(bytes.length)||capture.descriptor?.spec?.channel!=='response'||!same(capture.descriptor?.spec?.scope,attempt.scope))throw Error('Original response capture mismatch');
  const error=JSON.parse(bytes).error;if(error?.code!==401||error.message!=='User not found.')throw Error('Only the reviewed authentication failure can continue');
  return {inventory:inventory(base),claim_sha256:hash(path.join(plan.directory,'execution-claim.json')),result_sha256:hash(path.join(plan.directory,'result.json')),retained:result.runs[0],known_settled_micros:0,unresolved_micros:201640,retained_allocation_micros:2500000};
}
function ownerReceipt(file,originalHash) {
  const receipt=json(file);
  if(receipt.schema!=='p7-owner-authorization/1'||receipt.readonly_plan_sha256!==originalHash||receipt.aggregate_cap_usd!=='45.000000'||receipt.retries!==0||receipt.permissions!=='Exact reviewed plans, sole pinned verification launcher'||receipt.authorization_source!=='User response: Approved. Proceed.'||!(/^[a-f0-9]{64}$/).test(receipt.generation_plan_sha256))throw Error('Original owner authorization receipt differs');
  return {file:plain(path.resolve(file)),sha256:hash(file),readonly_plan_sha256:originalHash,generation_plan_sha256:receipt.generation_plan_sha256,source:'existing_owner_approval_no_new_attempts_or_allocation'};
}
function prepare(originalFile,originalHash,destination,receiptFile) {
  originalFile=plain(path.resolve(originalFile));destination=plain(path.resolve(destination));
  if(fs.existsSync(destination)||within(repo,destination)||within(destination,repo))throw Error('Fresh private continuation directory required');
  noParentInstructions(destination);privateDirectory(destination);
  const original=originalInputs(originalFile,originalHash),failure=failureEvidence(original,originalHash),owner_authorization=ownerReceipt(receiptFile,originalHash);
  const plan={schema:'p7-builtin-live-continuation/1',directory:destination,original_plan:originalFile,original_plan_sha256:originalHash,runner_sha256:hash(__filename),owner_authorization,failure,run_ids:original.runs.slice(1).map(r=>r.id),remaining_allocation_micros:37500000,aggregate_cap_micros:40000000,continuation_hash_purpose:'execution_integrity_not_new_owner_approval'};
  fs.mkdirSync(destination,{mode:0o700});write(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'),sha256:hash(path.join(destination,'plan.json')),runs:15,additional_budget_micros:0,model_calls:0};
}
function validate(file,integrity) {
  if(hash(file)!==integrity)throw Error('Integrity must name exact continuation hash');
  const plan=json(file);noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-builtin-live-continuation/1'||plain(path.dirname(path.resolve(file)))!==plan.directory||plan.runner_sha256!==hash(__filename)||plan.remaining_allocation_micros!==37500000||plan.aggregate_cap_micros!==40000000)throw Error('Continuation contract changed');
  if(fs.existsSync(path.join(plan.directory,'execution-claim.json')))throw Error('Continuation already claimed; no replay');
  const original=originalInputs(plan.original_plan,plan.original_plan_sha256);
  if(!same(plan.owner_authorization,ownerReceipt(plan.owner_authorization.file,plan.original_plan_sha256)))throw Error('Owner authorization receipt changed');
  if(!same(plan.run_ids,original.runs.slice(1).map(r=>r.id))||!same(plan.failure,failureEvidence(original,plan.original_plan_sha256)))throw Error('Original failure evidence or remaining identities changed');
  return {plan,original};
}
function costDiagnostic(pages,cap,scope) {
  if(!Array.isArray(pages)||!pages.length||pages.some(p=>!Array.isArray(p.items)||!Array.isArray(p.gaps)||p.gaps.length||p.next_cursor))throw Error('Incomplete canonical cost diagnostic');
  const items=pages.flatMap(p=>p.items);if(items.some(i=>i.visibility!=='available'))throw Error('Unavailable cost diagnostic');
  const counter=value=>{if(typeof value!=='string'||!(/^(0|[1-9][0-9]*)$/).test(value)||!Number.isSafeInteger(Number(value)))throw Error('Invalid diagnostic counter');return Number(value);};
  const rows=kind=>items.filter(i=>i.collection===kind).map(i=>i.record),ledgers=rows('ledger'),attempts=rows('attempt'),settlements=rows('settlement');
  if(!scope?.task||ledgers.length!==1||!same(ledgers[0].scope,scope)||ledgers[0].currency!=='USD'||counter(ledgers[0].cap)!==cap)throw Error('Diagnostic ledger mismatch');
  let charged=0;const ids=new Set();
  for(const attempt of attempts){
    if(!attempt.id||ids.has(attempt.id)||!same(attempt.scope,scope))throw Error('Diagnostic attempt identity mismatch');ids.add(attempt.id);
    const amount=counter(attempt.charged);charged+=amount;
    if(!Number.isSafeInteger(charged)||amount>0&&!settlements.some(s=>s.attempt===attempt.id&&s.applied===true&&same(s.scope,scope)&&counter(s.total)===amount))throw Error('Diagnostic charge receipt missing');
  }
  const settled=counter(ledgers[0].settled);if(charged!==settled)throw Error('Diagnostic settled charge mismatch');
  return {known_settled_micros:settled,unresolved_micros:counter(ledgers[0].unresolved),active_micros:counter(ledgers[0].active),overrun:ledgers[0].overrun===true};
}
function run(file,integrity,call=invoke) {
  const {plan,original}=validate(file,integrity);
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:integrity,original_plan_sha256:plan.original_plan_sha256,owner_authorization_sha256:plan.owner_authorization.sha256,at:new Date().toISOString(),meaning:'Only untouched original rows; no replay or additional allocation.'});
  const result={schema:'p7-builtin-live-continuation-result/1',plan_sha256:integrity,original_plan_sha256:plan.original_plan_sha256,actual_cost_micros:null,known_settled_micros:0,known_settled_complete:true,original_unresolved_micros:201640,retained_failed_allocation_micros:2500000,new_unknown_liability:false,new_unknown_allocation_micros:0,quality:'pending_independent_review',stopped:false,runs:[plan.failure.retained]};
  for(const row of original.runs.slice(1)) {
    const report={case_id:row.case_id,arm:row.arm,status:'not_run',answer:null,actual_cost_micros:null};result.runs.push(report);if(result.stopped)continue;
    const base=safeChild(original.directory,row.id),profile=json(path.join(base,'profile.json'));
    // Recheck expiry and freshness immediately before each new admission.
    if(paired.profileReasons(profile).length||fs.readdirSync(path.join(base,'data')).length||fs.existsSync(path.join(base,'attempted.json'))||fs.existsSync(path.join(base,'result.json'))||hash(path.join(base,'profile.json'))!==row.profile_sha256||hash(path.join(base,'prompt.txt'))!==row.prompt_sha256||!same(inventory(path.join(base,'workspace')),row.files)||hash(original.executable)!==original.executable_sha256||hash(original.catalog)!==original.catalog_sha256){report.reason='Profile expired or frozen row inputs changed';result.stopped=true;continue;}
    const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy','plan'];if(row.arm==='skill')args.push('--skill',`vcp-builtin::${row.skill}::${row.skill}`);
    write(path.join(base,'attempted.json'),{plan_sha256:plan.original_plan_sha256,continuation_sha256:integrity,args,at:new Date().toISOString()});
    const start=Date.now();let costPages;
    try {
      const execution=call(original.executable,args,(profile.deadline_seconds+180)*1000);report.latency_ms=Date.now()-start;
      write(path.join(base,'stdout.jsonl'),execution.stdout);write(path.join(base,'stderr.txt'),execution.stderr);
      if(execution.error)throw Error('CLI interrupted; new liability unknown; continuation stopped');
      const output=frames(execution.stdout),accepted=output.find(f=>f.type==='accepted'),final=output.findLast(f=>f.type==='result');
      if(!accepted?.scope?.task||!final?.conditions)throw Error('Durable task result missing');report.scope=accepted.scope;
      const evidence={};for(const view of ['costs','routing','outputs','context']){evidence[view]=inspection(original,base,accepted.scope.task,view,call);if(view==='costs')costPages=evidence[view];write(path.join(base,view+'.json'),evidence[view]);}
      const money=prior.accounting(evidence.costs,row.cap_micros);report.actual_cost_micros=money.actual_cost_micros;result.known_settled_micros+=money.actual_cost_micros;
      report.preserved=same(inventory(path.join(base,'workspace')),Object.fromEntries(Object.entries(row.files).sort()));
      report.status=final.conditions.completed&&execution.status===0&&report.preserved?'completed':'failed';
      if(report.status==='completed'){
        report.skill_evidence=paired.skillEvidence(original,base,row,evidence.context,money.attempts,call);
        try{report.answer_source=prior.responseAnswer(original,base,evidence.outputs,money.attempts,call);report.answer=report.answer_source.answer;}catch{report.status='failed';report.reason='canonical_json_answer_unavailable';}
      }
    }catch(error){report.status='failed';report.reason=error.message;result.stopped=true;if(report.actual_cost_micros===null){result.new_unknown_liability=true;result.new_unknown_allocation_micros=row.cap_micros;try{report.cost_diagnostic=costDiagnostic(costPages,row.cap_micros,report.scope);result.known_settled_micros+=report.cost_diagnostic.known_settled_micros;}catch{report.cost_diagnostic=null;result.known_settled_complete=false;}}}
    write(path.join(base,'result.json'),report);
  }
  write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={prepare,validate,run};
if(require.main===module){try{const [command,file,digest,destination,receipt,...rest]=process.argv.slice(2);if(rest.length||!file||!digest||!['prepare','run'].includes(command)||(command==='prepare'?(!destination||!receipt):(destination||receipt)))throw Error('Usage: prepare <original-plan> <original-approved-hash> <new-directory> <owner-receipt> | run <continuation-plan> <continuation-integrity-hash>');const result=command==='prepare'?prepare(file,digest,destination,receipt):run(file,digest);console.log(JSON.stringify(result));if(command==='run'&&(result.stopped||result.runs.some(r=>r.status!=='completed')))process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
