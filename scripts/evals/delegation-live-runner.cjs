// SPDX-License-Identifier: Apache-2.0
'use strict';
// One-shot U02/U03 qualification through the retained CLI child adapter. Never
// replays an interrupted stage or calls a provider during preparation/grading.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {isDeepStrictEqual}=require('node:util');
const prior=require('./p6-live-runner.cjs'),skills=require('./builtin-live-runner.cjs'),generation=require('./builtin-generation-prepare.cjs');
const {plain,read,write,within,safeChild,filesUnder,noParentInstructions,privateDirectory,noSecrets,usd}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const sources={
 'contract.md':'Shipping fee is 5 for subtotals below 100, otherwise 0. Tax is nonnegative integer cents, rounded to nearest cent with exact halves up. Receipt accepts nonnegative safe integers. Review current files against base/; base/ is reference only.\n',
 'shipping.cjs':'exports.fee = subtotal => subtotal > 100 ? 0 : 5;\n',
 'base/shipping.cjs':'exports.fee = subtotal => subtotal >= 100 ? 0 : 5;\n',
 'tax.cjs':'exports.tax = (cents, rate) => Math.floor(cents * rate / 100);\n',
 'base/tax.cjs':'exports.tax = (cents, rate) => Math.floor(cents * rate / 100);\n',
 'receipt.cjs':'exports.receipt = cents => `${cents}`;\n',
 'base/receipt.cjs':'exports.receipt = cents => String(cents);\n',
};
const rubric={schema:'p7-review-rubric/1',minimum_seeded:2,maximum_false_positives:0,defects:[
 {path:'shipping.cjs',line:1,reproduction:{arguments:[100],expected:0,actual:5},introduced_by_change:'introduced'},
 {path:'tax.cjs',line:1,reproduction:{arguments:[5,10],expected:1,actual:0},introduced_by_change:'pre_existing'},
]};
const reviewPrompt='Review current shipping.cjs, tax.cjs and receipt.cjs against contract.md and the corresponding base/ files. Report supported correctness defects and distinguish pre-existing defects from defects introduced by the diff. Check the contract and relevant surrounding source. Never infer causality from a changed line alone. Do not modify files or run processes. Return only one JSON object with findings array. Each finding has kind (defect or suggestion), path, line, trigger, consequence, evidence (array of source references), uncertainty (nonempty string), introduced_by_change (introduced, pre_existing, or unknown), reproduction (arguments array, expected, actual). Use concrete reproducible boundary values. Invoke canonical vcp_verify with observed read evidence as needed; do not claim executable checks ran.\n';
const humanNote='Human edit after child snapshot: preserve this exact note during integration.\n';
function inventory(dir){return Object.fromEntries(filesUnder(dir).map(p=>[p,sha(read(safeChild(dir,p)))]));}
function put(dir,files){for(const [name,bytes]of Object.entries(files)){const target=safeChild(dir,name);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});}}
function exactCounter(v){if(typeof v!=='string'||!/^\d+$/.test(v)||!Number.isSafeInteger(Number(v)))throw Error('Invalid canonical counter');return Number(v);}
function demonstrated(reproduction,defect){
 if(!reproduction||!Array.isArray(reproduction.arguments)||!reproduction.arguments.every(n=>Number.isSafeInteger(n)&&n>=0))return false;
 const args=reproduction.arguments;let expected,actual;
 if(defect.path==='shipping.cjs'&&args.length===1){expected=args[0]>=100?0:5;actual=args[0]>100?0:5;}
 else if(defect.path==='tax.cjs'&&args.length===2&&args[1]<=100){expected=Number((BigInt(args[0])*BigInt(args[1])+50n)/100n);actual=Math.floor(args[0]*args[1]/100);}
 else return false;
 return expected!==actual&&reproduction.expected===expected&&reproduction.actual===actual;
}
function gradeReview(answer,expected=rubric){
 if(!answer||!Array.isArray(answer.findings)||answer.findings.length>64)throw Error('Bounded JSON review answer required');
 const valid=f=>f&&['defect','suggestion'].includes(f.kind)&&typeof f.path==='string'&&Number.isInteger(f.line)&&['trigger','consequence','uncertainty'].every(k=>typeof f[k]==='string'&&f[k].trim().length>0&&f[k].length<=2048)&&Array.isArray(f.evidence)&&f.evidence.length>0&&f.evidence.every(x=>typeof x==='string'&&x.length<=1024);
 const defects=answer.findings.filter(f=>f.kind==='defect');
 const matching=(f,d)=>valid(f)&&f.path===d.path&&f.line===d.line&&demonstrated(f.reproduction,d)&&f.introduced_by_change===d.introduced_by_change&&f.evidence.some(e=>e.includes('base/'+d.path));
 const hits=expected.defects.filter(d=>defects.some(f=>matching(f,d))).length;
 const falsePositives=defects.filter(f=>!expected.defects.some(d=>matching(f,d))).length;
 return {pass:answer.findings.every(valid)&&hits===expected.minimum_seeded&&falsePositives<=expected.maximum_false_positives,seeded_found:hits,seeded_total:expected.minimum_seeded,false_positives:falsePositives,manual_evidence_usefulness_review_required:true};
}
function accounting(state,cap,root,child){
 const rows=Object.values(state.records),of=c=>rows.filter(r=>r.collection===c).map(r=>r.value),ledgers=of('ledger'),attempts=of('attempt'),settlements=of('settlement');
 if(ledgers.length!==1||ledgers[0].currency!=='USD'||exactCounter(ledgers[0].cap)!==cap||ledgers[0].overrun||exactCounter(ledgers[0].active)!==0||exactCounter(ledgers[0].unresolved)!==0)throw Error('Unsettled root liability or cap mismatch; stop and reconcile');
 if(!attempts.length||attempts.some(a=>!['settled','released'].includes(a.phase)||a.uncertain||a.previous||![root,child].includes(a.scope.task)||!['main','child'].includes(a.role)||(a.phase==='settled'&&!settlements.some(s=>s.attempt===a.id&&s.applied&&s.observation?.final_usage))))throw Error('Incomplete/unsupported root or child accounting; no next arm');
 const total=attempts.reduce((n,a)=>n+exactCounter(a.charged),0);
 if(!Number.isSafeInteger(total)||total!==exactCounter(ledgers[0].settled)||total>cap)throw Error('Root total does not reconcile all model work');
 return {actual_cost_micros:total,attempts:attempts.length,child_attempts:attempts.filter(a=>a.role==='child').length,input_units:attempts.map(a=>a.usage??null)};
}
function prepare(specFile,destination){
 const bytes=read(specFile),spec=JSON.parse(bytes);noSecrets(spec);
 if(!['review','generation'].includes(spec.stage))throw Error('Stage must be review or generation');
 const cap=prior.micros(spec.stage_cap_usd),overall=prior.micros(spec.overall_cap_usd);
 if(overall!==10000000||!Number.isSafeInteger(spec.prior_exposure_micros)||spec.prior_exposure_micros<0||spec.prior_exposure_micros+cap>overall)throw Error('Stage allocation exceeds remaining explicit $10 ceiling');
 const profileBytes=read(spec.profile),profile=JSON.parse(profileBytes);noSecrets(profile);
 const reasons=skills.fixedProfileReasons(profile);if(reasons.length)throw Error(reasons.join('; '));
 if(profile.maximum_autonomy!=='workspace'||JSON.stringify([...profile.automatic_effects].sort())!==JSON.stringify(['read','write']))throw Error('Source profile must have only workspace read/write authority');
 const adapter=plain(spec.adapter),adapterBytes=read(adapter,1024*1024*1024),assets=generation.inventory(path.join(path.dirname(adapter),'skills/builtin'));
 if(JSON.stringify(assets)!==JSON.stringify(generation.inventory(path.join(repo,'src/skills/builtin'))))throw Error('Adapter needs exact packaged skill sidecars');
 generation.requireEmbeddedCatalog(adapterBytes,read(path.join(path.dirname(adapter),'skills/builtin/catalog.json')));
 const runtime=spec.stage==='generation'?generation.qualifyRuntime(spec.runtime):null;
 if(spec.stage==='generation'&&(!runtime||spec.propose_opaque_launcher_effects!==true))throw Error('Generation needs explicitly authorized pinned parent verification launcher');
 destination=plain(destination);if(fs.existsSync(destination)||within(repo,destination)||within(destination,repo))throw Error('New private directory outside repository required');
 privateDirectory(destination);noParentInstructions(path.dirname(destination));
 const catalog=plain(profile.catalog),git=plain(spec.git);read(git,128*1024*1024);
 const files=spec.stage==='review'?sources:Object.fromEntries(JSON.parse(read(path.join(repo,'src/evals/skills/builtin/generation-v1/manifest.json'))).files.map(f=>[f.path,read(path.join(repo,'src/evals/skills/builtin/generation-v1/project',f.path))]));
 const prompt=spec.stage==='review'?reviewPrompt:JSON.parse(read(path.join(repo,'src/evals/skills/builtin/generation-v1/manifest.json'))).prompt;
 const arms=spec.stage==='review'?['baseline','review_child']:['generation_child'];
 fs.mkdirSync(destination,{mode:0o700});
 write(path.join(destination,'private-rubric.json'),rubric);
 const dependencies=['scripts/evals/p6-live-runner.cjs','scripts/evals/builtin-live-runner.cjs','scripts/evals/builtin-generation-prepare.cjs','scripts/evals/builtin-generation-oracle.cjs','src/evals/skills/builtin/generation-v1/manifest.json'];
 const plan={schema:'p7-delegation-live-plan/1',stage:spec.stage,directory:destination,adapter,adapter_sha256:sha(adapterBytes),adapter_source_sha256:sha(read(path.join(repo,'src/crates/vcp-cli/examples/delegation-live-adapter.rs'))),runner_sha256:sha(read(__filename)),dependencies:Object.fromEntries(dependencies.map(f=>[f,sha(read(path.join(repo,f)))])),generation_fixture:inventory(path.join(repo,'src/evals/skills/builtin/generation-v1/project')),assets,profile_source:plain(spec.profile),profile_sha256:sha(profileBytes),catalog,catalog_sha256:sha(read(catalog)),git,git_sha256:sha(read(git,128*1024*1024)),runtime,stage_cap_micros:cap,prior_exposure_micros:spec.prior_exposure_micros,overall_cap_micros:overall,permission_review:spec.stage==='generation'?generation.permissionReview(runtime,true):{automatic_effects:[],mode:'plan'},arms:[]};
 for(const arm of arms){
  const base=path.join(destination,arm),workspace=path.join(base,'workspace');fs.mkdirSync(workspace,{recursive:true});fs.mkdirSync(path.join(base,'children'));put(workspace,files);
  const allocation=Math.floor(cap/arms.length),derived=spec.stage==='generation'?generation.qualifiedProfile(profile,workspace,catalog,allocation,runtime,true):{...profile,workspace,catalog,budget_usd:usd(allocation),maximum_autonomy:'plan',automatic_effects:[],affected_paths:Object.keys(files),checks:[],processes:[]};
  write(path.join(base,'profile.json'),derived);write(path.join(base,'prompt.txt'),prompt);
  const delegated=arm!=='baseline';
  if(delegated)write(path.join(base,'delegation.json'),{version:1,git,disposable_parent:path.join(base,'children'),objective:prompt,acceptance:['Meet the frozen independent acceptance rubric; report check limitations'],mode:spec.stage==='review'?'read_only':'isolated_write',write_paths:spec.stage==='review'?[]:['src/cart.cjs'],read_paths:[''],untracked_inputs:Object.keys(files),allocation_usd:usd(Math.floor(allocation*3/4)),seconds:derived.deadline_seconds,required_checks:[],role:spec.stage==='review'?'review':'generation'});
  const driver={profile:path.join(base,'profile.json'),workspace,directory:base,prompt:path.join(base,'prompt.txt'),delegation:delegated?path.join(base,'delegation.json'):null,git,generation:spec.stage==='generation',human_note:spec.stage==='generation'?humanNote:null};
  write(path.join(base,'adapter-spec.json'),driver);
  plan.arms.push({name:arm,cap_micros:allocation,files:inventory(workspace),frozen:Object.fromEntries(['profile.json','prompt.txt','adapter-spec.json',...(delegated?['delegation.json']:[])].map(f=>[f,sha(read(path.join(base,f)))]))});
 }
 plan.rubric_sha256=sha(read(path.join(destination,'private-rubric.json')));write(path.join(destination,'plan.json'),plan);
 return {plan:path.join(destination,'plan.json'),sha256:sha(read(path.join(destination,'plan.json'))),stage_cap_micros:cap,model_calls:0,qualification:'adapter build and offline controls required before execution; not terminal U06'};
}
function validate(plan,file){
 if(plan.schema!=='p7-delegation-live-plan/1'||plain(path.dirname(file))!==plan.directory||plan.runner_sha256!==sha(read(__filename))||plan.adapter_sha256!==sha(read(plan.adapter,1024*1024*1024))||plan.adapter_source_sha256!==sha(read(path.join(repo,'src/crates/vcp-cli/examples/delegation-live-adapter.rs')))||plan.profile_sha256!==sha(read(plan.profile_source))||plan.catalog_sha256!==sha(read(plan.catalog))||plan.git_sha256!==sha(read(plan.git,128*1024*1024)))throw Error('Frozen stage identity changed');
 noParentInstructions(plan.directory);privateDirectory(plan.directory);
 for(const [name,digest]of Object.entries(plan.dependencies))if(sha(read(safeChild(repo,name)))!==digest)throw Error('Qualification dependency changed');
 if(!isDeepStrictEqual(plan.generation_fixture,inventory(path.join(repo,'src/evals/skills/builtin/generation-v1/project'))))throw Error('Independent generation fixture changed');
 if(plan.prior_exposure_micros+plan.stage_cap_micros>plan.overall_cap_micros||plan.overall_cap_micros!==10000000||plan.arms.reduce((s,a)=>s+a.cap_micros,0)>plan.stage_cap_micros)throw Error('Stage exposure exceeds approved ceiling');
 if(skills.fixedProfileReasons(JSON.parse(read(plan.profile_source))).length)throw Error('Exact provider qualification expired or changed');
 if(JSON.stringify(generation.inventory(path.join(path.dirname(plan.adapter),'skills/builtin')))!==JSON.stringify(plan.assets)||sha(read(path.join(plan.directory,'private-rubric.json')))!==plan.rubric_sha256)throw Error('Assets or private rubric changed');
 if(plan.runtime){const input={node:plan.runtime.node,launcher:plan.runtime.launcher,...(plan.runtime.build_receipt?{build_receipt:plan.runtime.build_receipt}:{})};if(JSON.stringify(generation.qualifyRuntime(input))!==JSON.stringify(plan.runtime))throw Error('Pinned runtime changed');}
 for(const arm of plan.arms){const base=safeChild(plan.directory,arm.name);if(JSON.stringify(inventory(path.join(base,'workspace')))!==JSON.stringify(arm.files))throw Error('Prepared fixture changed');for(const [f,digest]of Object.entries(arm.frozen))if(sha(read(safeChild(base,f)))!==digest)throw Error('Prepared arm changed');}
}
function generationGrade(plan,base,arm,adapter,state){
 const workspace=path.join(base,'workspace'),expected={...arm.files,'notes.txt':sha(Buffer.from(humanNote))},actual=inventory(workspace);
 for(const name of new Set([...Object.keys(expected),...Object.keys(actual)]))if(name!=='src/cart.cjs'&&actual[name]!==expected[name])throw Error('Parent preservation failed: '+name);
 const verification=adapter.result.verification,root=adapter.result.scope.task,task=Object.values(state.records).find(r=>r.collection==='task'&&r.id===root)?.value;
 if(!verification||verification.scope.task!==root||JSON.stringify(verification.fingerprint)!==JSON.stringify(task?.fingerprint)||verification.cost?.certainty!=='known'||verification.unresolved_effects?.length||verification.outstanding_issues?.length||!verification.checks?.length||verification.checks.some(c=>c.outcome?.status!=='passed'||c.exit_code!==0))throw Error('Current integrated parent verification did not pass');
 // The independent integer oracle runs the exact integrated source in a frozen
 // mirror, while preservation of the actual concurrent human note is checked
 // above. Parent verification itself ran against the real current workspace.
 const mirror=path.join(base,'oracle-workspace');fs.mkdirSync(mirror);
 const fixture=path.join(repo,'src/evals/skills/builtin/generation-v1/project');put(mirror,Object.fromEntries(filesUnder(fixture).map(f=>[f,read(safeChild(fixture,f))])));
 fs.writeFileSync(path.join(mirror,'src/cart.cjs'),read(path.join(workspace,'src/cart.cjs')));
 const result=spawnSync(plan.runtime.node,[path.join(__dirname,'builtin-generation-oracle.cjs'),mirror],{env:{SystemRoot:process.env.SystemRoot},encoding:'utf8',timeout:10000,maxBuffer:65536,windowsHide:true});
 if(result.error||![0,1].includes(result.status))throw Error('Independent generation oracle unavailable');
 return {...JSON.parse(result.stdout),actual_parent_source_sha256:actual['src/cart.cjs'],human_note_preserved:true,canonical_parent_verification:verification.id};
}
function run(file,authorization){
 if(sha(read(file))!==authorization)throw Error('Exact authorized stage hash required');const plan=JSON.parse(read(file));validate(plan,file);
 write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,at:new Date().toISOString(),meaning:'Entire stage allocation held until reconciliation; never replay or recycle'});
 const report={schema:'p7-delegation-result/1',stage:plan.stage,plan_sha256:authorization,actual_cost_micros:0,stopped:false,arms:[],terminal_u06:false};
 for(const arm of plan.arms){if(report.stopped)break;const base=path.join(plan.directory,arm.name),record={arm:arm.name,status:'failed',actual_cost_micros:null};report.arms.push(record);
  try{
   const spec=path.join(base,'adapter-spec.json'),start=Date.now(),result=prior.boundaries.invoke(plan.adapter,[spec,sha(read(spec))],(JSON.parse(read(path.join(base,'profile.json'))).deadline_seconds+240)*1000);
   record.latency_ms=Date.now()-start;write(path.join(base,'adapter-stdout.jsonl'),result.stdout);write(path.join(base,'adapter-stderr.txt'),result.stderr);
   if(result.error)throw Error('Adapter interrupted; canonical liability must be reconciled before any next arm');
   const adapter=JSON.parse(read(path.join(base,'adapter-result.json'))),state=JSON.parse(read(path.join(base,'canonical-state.json'))),config=JSON.parse(read(path.join(base,'canonical-config.json')));
   record.accounting=accounting(state,arm.cap_micros,config.root_task,adapter.result?.child);record.actual_cost_micros=record.accounting.actual_cost_micros;report.actual_cost_micros+=record.actual_cost_micros;
   if(adapter.status!=='observed'||adapter.owner_close_error)throw Error(adapter.reason||'Owner did not close cleanly');
   if(arm.name!=='baseline'&&!record.accounting.child_attempts)throw Error('No actual retained provider child was observed');
   if(plan.stage==='review'){
    if(JSON.stringify(inventory(path.join(base,'workspace')))!==JSON.stringify(arm.files))throw Error('Read-only review changed parent source or index');
    if(adapter.result.child){const child=path.join(base,'children',adapter.result.child),observed=inventory(child);delete observed['.vcp-child-owner'];if(!isDeepStrictEqual(observed,arm.files))throw Error('Read-only review changed isolated source or index');}
    const answers=adapter.result.transcripts.flatMap(t=>{try{return [JSON.parse(t.text)];}catch{return [];}}).filter(x=>Array.isArray(x.findings));
    record.quality=answers.length===1?gradeReview(answers[0],JSON.parse(read(path.join(plan.directory,'private-rubric.json')))):{pass:false,reason:'Expected one unambiguous retained JSON review answer'};
   }else record.quality=generationGrade(plan,base,arm,adapter,state);
   record.status=record.quality.pass?'passed':'failed';
  }catch(error){record.reason=error.message;report.stopped=true;if(record.actual_cost_micros===null)report.actual_cost_micros=null;}
  write(path.join(base,'result.json'),record);
 }
 write(path.join(plan.directory,'result.json'),report);return report;
}
module.exports={prepare,validate,run,gradeReview,accounting,sources,rubric};
if(require.main===module){try{const [command,file,extra,...rest]=process.argv.slice(2);if(rest.length||!file||!extra||!['prepare','run'].includes(command))throw Error('Usage: delegation-live-runner.cjs prepare <spec> <new-private-dir> | run <plan> <authorized-sha256>');console.log(JSON.stringify(command==='prepare'?prepare(file,extra):run(file,extra)));}catch(error){console.error(error.message);process.exitCode=1;}}
