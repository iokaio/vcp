// SPDX-License-Identifier: Apache-2.0
'use strict';
// One exact P8 profile renewal probe; this never qualifies or publishes a profile.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {changeCampaign,bounded}=require('./p805-owner-runner.cjs');
const {plain,read,write,privateDirectory}=require('./p6-live-runner.cjs').boundaries;
const CAP=12000000,MODEL='qwen/qwen3.8-max-0902';
const BINARY='a969e4597a8a88e0091a7a35c84b9a79d5d868fd2c804a3566597bb9555ba670';
const repo=path.resolve(__dirname,'../..');
const hash=file=>crypto.createHash('sha256').update(read(file,256*1024*1024)).digest('hex');
const parse=file=>{const bytes=read(file);try{return JSON.parse(bytes);}catch{throw Error('Invalid bounded JSON input');}};
const safeError=error=>{const message=String(error.message),key=process.env.OPENROUTER_API_KEY;return key?message.split(key).join('[redacted]'):message;};
const same=require('node:util').isDeepStrictEqual;
function runnerFiles(){
  // Bind all loaded repository code, including dependencies loaded by reused guards.
  return Object.keys(require.cache).filter(f=>f.startsWith(repo+path.sep)&&/\.(cjs|js)$/.test(f)&&!f.includes(path.sep+'node_modules'+path.sep)).sort();
}
function freshness(spec,now=Date.now()){
  const start=Number(spec.observed_at),end=Number(spec.valid_until);
  if(![spec.observed_at,spec.valid_until].every(v=>typeof v==='string'&&/^[0-9]+$/.test(v))||![start,end].every(Number.isSafeInteger)||start>now||end<=now+300000||end-start>86400000||end<=start)throw Error('Fresh truthful catalog window required');
}
function inputs(plan,file,expected){
  if(hash(file)!==expected)throw Error('Proposal changed');
  for(const item of [plan.executable,plan.spec,plan.catalog])if(hash(item.path)!==item.sha256)throw Error('Pinned input changed');
  for(const name of runnerFiles())if(plan.runner_hashes?.[name]!==hash(name))throw Error('Coordinator dependency changed');
}
function validate(file,expected,now=Date.now()){
  if(!/^[a-f0-9]{64}$/.test(expected||'')||hash(file)!==expected)throw Error('Exact proposal hash required');
  const plan=parse(file),root=plain(path.dirname(path.resolve(file)));
  privateDirectory(root);
  if(plan.schema!=='p8-conformance-proposal/2'||plan.spend_authorized!==false||plan.cap_micros!==CAP||plan.max_requests!==2||plan.retries!==0)throw Error('Frozen renewal contract required');
  if(path.resolve(plan.spec.path)!==path.join(root,'spec.json')||path.resolve(plan.catalog.path)!==path.join(root,'endpoints.json')||path.resolve(plan.output)!==path.join(root,'probe'))throw Error('Exact private proposal paths required');
  if(path.resolve(plan.campaign)!==path.join(repo,'artifacts/p7-p8-owner-campaign.json')||plan.executable.sha256!==BINARY)throw Error('Exact campaign and qualified native executable required');
  if(hash(plan.campaign)!==plan.campaign_sha256)throw Error('Prepared campaign snapshot changed');
  inputs(plan,file,expected);
  const spec=parse(plan.spec.path);
  if(Object.keys(spec).sort().join(',')!=='cap_usd,catalog,catalog_sha256,endpoint,max_output_tokens,model,observed_at,request_price_limit,valid_until'||spec.model!==MODEL||spec.endpoint!=='alibaba'||spec.max_output_tokens!==512||spec.cap_usd!=='12.000000'||spec.request_price_limit!=='0.001'||spec.catalog!==plan.catalog.path||spec.catalog_sha256!==plan.catalog.sha256)throw Error('Exact probe spec required');
  freshness(spec,now);
  const catalog=parse(plan.catalog.path),endpoints=catalog?.data?.endpoints;
  const candidates=Array.isArray(endpoints)?endpoints.filter(e=>e.tag==='alibaba'):[];
  if(candidates.length!==1||candidates[0].model_id!==MODEL||!['tools','tool_choice','max_tokens'].every(p=>candidates[0].supported_parameters?.includes(p)))throw Error('Exact capable endpoint required');
  if(fs.existsSync(plan.output)||fs.existsSync(path.join(root,'renewal-claim.json')))throw Error('One-shot proposal already claimed or output exists');
  const budget=parse(plan.campaign);
  reserve(structuredClone(budget),plan,expected);
  if(budget.settled_micros+budget.reserved_micros+CAP>100000000)throw Error('Insufficient campaign headroom');
  return {plan,root};
}
function reserve(budget,plan,expected){
  if(budget.schema!=='p7-p8-owner-campaign/1'||budget.cap_micros!==100000000||![budget.settled_micros,budget.reserved_micros].every(n=>Number.isSafeInteger(n)&&n>=0)||budget.settled_micros+budget.reserved_micros+CAP>budget.cap_micros)throw Error('Campaign bound exceeded');
  if(!budget.models?.includes(MODEL)||!Array.isArray(budget.runs)||budget.runs.some(r=>r.sha256===expected||['prepared','running','running-held'].includes(r.status)))throw Error('Duplicate probe or outstanding launch');
  budget.reserved_micros+=CAP;
  budget.runs.push({model:MODEL,stage:'p8-profile-renewal',sha256:expected,plan:plan.spec.path,cap_micros:CAP,status:'running-held',actual_cost_micros:null});
}
function counter(v){if(typeof v!=='string'||!/^(0|[1-9][0-9]*)$/.test(v)||!Number.isSafeInteger(Number(v)))throw Error('Canonical decimal required');return Number(v);}
function accounting(report,records,claim,plan){
  if(claim.binary_sha256!==plan.executable.sha256||claim.spec_sha256!==plan.spec.sha256||!same(claim.spec,parse(plan.spec.path)))throw Error('Probe claim mismatch');
  const ledger=report.ledger,scope=report.scope;
  if(report.schema!=='p6-provider-conformance/1'||!['observed','failed'].includes(report.status)||!scope?.task||!scope.workspace||!scope.session||!same(ledger?.scope,scope)||ledger.cap!==String(CAP)||ledger.currency!=='USD'||ledger.active!=='0'||ledger.unresolved!=='0'||ledger.overrun!==false)throw Error('Unknown or invalid canonical ledger');
  const cost=counter(report.actual_cost_micros);
  if(cost>CAP||counter(ledger.settled)!==cost||!Array.isArray(records)||records.length>256)throw Error('Invalid settled charge');
  const ledgers=records.filter(r=>r.collection==='ledger');
  if(ledgers.length!==1||ledgers[0].id!==scope.task||ledgers[0].workspace!==scope.workspace||!same(ledgers[0].value,ledger))throw Error('Canonical ledger witness mismatch');
  const reservations=records.filter(r=>r.collection==='reservation'),attempts=records.filter(r=>r.collection==='attempt');
  if(attempts.length>2||reservations.length!==attempts.length||new Set(attempts.map(r=>r.id)).size!==attempts.length||new Set(reservations.map(r=>r.value.attempt)).size!==attempts.length)throw Error('Probe attempt bound or identity mismatch');
  let sum=0;
  for(const r of reservations){
    const v=r.value;
    if(v.phase!=='settled'||v.liability!=='0'||v.root!==scope.task||r.workspace!==scope.workspace||!same(v.scope,scope)||!attempts.some(a=>a.id===v.attempt&&a.workspace===scope.workspace))throw Error('Unsettled or foreign reservation');
    sum+=counter(v.charged);
    const attempt=attempts.find(a=>a.id===v.attempt).value;
    if(!attempt||attempt.id!==v.attempt||attempt.reservation!==r.id||attempt.root!==scope.task||!same(attempt.scope,scope)||attempt.phase!=='settled'||attempt.charged!==v.charged||attempt.role!=='main'||v.role!=='main')throw Error('Canonical attempt witness mismatch');
  }
  if(sum!==cost)throw Error('Independent reservation charge sum mismatch');
  if(report.status==='observed'&&(attempts.length!==2||report.responses?.length!==2||report.responses_text_tools!==true))throw Error('Observed pair incomplete');
  return cost;
}
async function run(file,expected,controls={}){
  // Test-only dependency injection is never accepted from proposal data or CLI.
  const check=controls.validate||validate,audit=controls.inputs||inputs,change=controls.changeCampaign||changeCampaign,execute=controls.execute||bounded;
  if((controls.platform||process.platform)!=='win32')throw Error('Native Windows renewal required');
  const {plan,root}=check(file,expected);
  write(path.join(root,'renewal-claim.json'),{proposal_sha256:expected,at:new Date().toISOString(),meaning:'One shot, including prelaunch failures; not profile qualification'});
  const result={schema:'p8-profile-renewal-result/1',proposal_sha256:expected,status:'failed',reserved:false,actual_cost_micros:null,failures:[]};
  let accounted=false;
  try{
    if(!process.env.OPENROUTER_API_KEY?.trim())throw Error('Existing process credential required');
    audit(plan,file,expected);
    change(plan.campaign,b=>{if(hash(plan.campaign)!==plan.campaign_sha256)throw Error('Prepared campaign snapshot changed');reserve(b,plan,expected);});result.reserved=true;
    freshness(parse(plan.spec.path));
    audit(plan,file,expected);
    const execution=await execute(plan.executable.path,[plan.spec.path,plan.output,plan.spec.sha256],300000,{SystemRoot:process.env.SystemRoot,TEMP:process.env.TEMP,TMP:process.env.TMP,OPENROUTER_API_KEY:process.env.OPENROUTER_API_KEY});
    // Never persist arbitrary process output, which may include credential-bearing errors.
    const key=process.env.OPENROUTER_API_KEY;
    result.sensitive_output_detected=[execution.stdout,execution.stderr].some(text=>typeof text==='string'&&text.includes(key));
    const outputMeta=text=>{const redacted=(text||'').split(key).join('[redacted]');return {redacted_bytes:Buffer.byteLength(redacted),redacted_sha256:crypto.createHash('sha256').update(redacted).digest('hex')};};
    write(path.join(root,'execution.json'),{status:execution.status,error:execution.error?'bounded_execution_error':null,process_reaped:execution.process_reaped,sensitive_output_detected:result.sensitive_output_detected,stdout:outputMeta(execution.stdout),stderr:outputMeta(execution.stderr)});
    result.process_reaped=execution.process_reaped;
    if(!execution.process_reaped||execution.error)throw Error('Probe did not terminate cleanly; retain full cap');
    audit(plan,file,expected);
    const report=parse(path.join(plan.output,'result.json'));
    result.actual_cost_micros=accounting(report,parse(path.join(plan.output,'canonical-records.json')),parse(path.join(plan.output,'claim.json')),plan);
    accounted=true;
    result.status=execution.status===0&&report.status==='observed'&&!result.sensitive_output_detected?'observed-awaiting-qualification':'failed-reconciled';
    if(result.sensitive_output_detected)result.failures.push('Sensitive process output detected');
  }catch(error){result.failures.push(safeError(error));}
  if(result.reserved){
    try{change(plan.campaign,b=>{
      const row=b.runs.find(r=>r.sha256===expected);
      if(!row||row.status!=='running-held'||row.cap_micros!==CAP||b.reserved_micros<CAP)throw Error('Reservation ownership changed');
      if(accounted){b.reserved_micros-=CAP;b.settled_micros+=result.actual_cost_micros;row.actual_cost_micros=result.actual_cost_micros;row.status=result.status;}
      else{row.status=result.process_reaped===true?'failed-unknown':'running-held';row.unresolved_upper_bound_micros=CAP;}
    });}catch(error){result.failures.push('Reconciliation failed: '+safeError(error));result.status='failed';}
  }
  write(path.join(root,'renewal-result.json'),result);
  return result;
}
module.exports={validate,inputs,runnerFiles,reserve,accounting,freshness,run};
if(require.main===module){
  const [mode,file,expected,...extra]=process.argv.slice(2);
  Promise.resolve().then(()=>{if(extra.length||!['validate','run'].includes(mode))throw Error('Usage: p8-profile-renewal.cjs validate|run proposal.json exact-sha256');return mode==='validate'?(validate(file,expected),{status:'valid-not-run',model_calls:0}):run(file,expected);}).then(result=>{process.stdout.write(JSON.stringify(result)+'\n');if(mode==='run'&&result.status!=='observed-awaiting-qualification')process.exitCode=1;}).catch(error=>{process.stderr.write(safeError(error)+'\n');process.exitCode=1;});
}
