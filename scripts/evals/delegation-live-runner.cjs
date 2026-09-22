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
const delegationFixture=path.join(repo,'src/evals/delegation/generation-v1'),delegationProject=path.join(delegationFixture,'project');
const adapterEnvironmentPin={RUST_MIN_STACK:'16777216'};
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
const reviewPrompt='Review current shipping.cjs, tax.cjs and receipt.cjs against contract.md and the corresponding base/ files. Report supported correctness defects and distinguish pre-existing defects from defects introduced by the diff. Check the contract and relevant surrounding source. Never infer causality from a changed line alone. Do not modify files or run processes. Return only one JSON object with findings array. Each finding has kind (defect or suggestion), path, line, trigger, consequence, evidence (array of source references), uncertainty (nonempty string), introduced_by_change (introduced, pre_existing, or unknown), reproduction (arguments array, expected, actual). Use concrete reproducible boundary values. Keep reproduction arguments, expected and actual as the actual JSON values: numeric results must be JSON numbers, not explanatory strings. Put explanations in trigger or consequence. The final response must begin with { and end with }; do not use Markdown code fences or surrounding prose. Invoke canonical vcp_verify with observed read evidence as needed; do not claim executable checks ran.\n';
const humanNote='Human edit after child snapshot: preserve this exact note during integration.\n';
function put(dir,files){for(const [name,bytes]of Object.entries(files)){const target=safeChild(dir,name);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});}}
function modelFiles(dir){return filesUnder(dir).filter(p=>p!=='.git'&&!p.startsWith('.git/'));}
function inventory(dir){return Object.fromEntries(modelFiles(dir).map(p=>[p,sha(read(safeChild(dir,p)))]));}
function gitEnvironment(extra={}){
 const allowedExtra=['GIT_AUTHOR_DATE','GIT_COMMITTER_DATE'];if(Object.keys(extra).some(key=>!allowedExtra.includes(key)))throw Error('Unsupported synthetic Git environment');
 const env={};for(const key of ['SystemRoot','WINDIR','PATH','TEMP','TMP'])if(process.env[key]!==undefined)env[key]=process.env[key];
 Object.assign(env,{GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:process.platform==='win32'?'NUL':'/dev/null',GIT_OPTIONAL_LOCKS:'0'},extra);
 return env;
}
function gitRun(cwd,git,args,extra={}){
 const executable=plain(git),result=spawnSync(executable,args,{cwd,env:gitEnvironment(extra),encoding:'utf8',timeout:5000,windowsHide:true});if(result.error||result.status!==0)throw Error(`Synthetic Git command failed: ${executable} ${args.join(' ')}: ${result.stderr||result.error?.message||result.status}`);return result;
}
function adapterEnvironment(){const env={};for(const key of ['SystemRoot','WINDIR','PATH','TEMP','TMP','OPENROUTER_API_KEY'])if(process.env[key]!==undefined)env[key]=process.env[key];return {...env,...adapterEnvironmentPin};}
function invokeAdapter(executable,args,timeout){const result=spawnSync(executable,args,{shell:false,windowsHide:true,encoding:'utf8',timeout,maxBuffer:16*1024*1024,stdio:['ignore','pipe','pipe'],env:adapterEnvironment()});return {status:result.status,signal:result.signal??null,stdout:result.stdout||'',stderr:result.stderr||'',error:result.error?.code??null};}
function stderrSummary(value){return String(value||'').replace(/sk-or-v1-[A-Za-z0-9_-]+/gi,'[redacted-provider-key]').replace(/(authorization\s*[:=]\s*(?:bearer\s+)?)[^\s]+/gi,'$1[redacted]').replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g,'').trim().slice(-2048);}
function workspaceGitState(root,expected=null,{git:gitExecutable,editable=[]}={}){
 if(!gitExecutable)throw Error('Pinned Git executable required');
 const git=path.join(root,'.git'),headFile=path.join(git,'HEAD'),indexFile=path.join(git,'index');if(!fs.statSync(git).isDirectory())throw Error('Prepared delegation workspace needs a native Git worktree');
 const headRef=read(headFile,4096).toString('utf8').trim();if(headRef!=='ref: refs/heads/main')throw Error('Prepared delegation HEAD changed');const head=read(path.join(git,'refs','heads','main'),4096).toString('utf8').trim(),indexBefore=read(indexFile,128*1024*1024);
 const status=gitRun(root,gitExecutable,['status','--porcelain=v1','--untracked-files=all']),text=status.stdout.trimEnd(),rows=(text?text.split(/\r?\n/):[]).filter(Boolean).sort(),fixed=[' M unstaged-note.txt','?? notes.txt','M  staged-note.txt'],allowed=new Set(editable.map(file=>` M ${file}`));
 if(rows.some(row=>!fixed.includes(row)&&!allowed.has(row))||fixed.some(row=>!rows.includes(row)))throw Error('Prepared delegation dirty worktree changed');
 const index=read(indexFile,128*1024*1024);if(!index.equals(indexBefore))throw Error('Synthetic Git status changed the prepared index');
 const configBytes=read(path.join(git,'config'),128*1024),config=configBytes.toString('utf8');if(!config.includes('hooksPath = .git/hooks'))throw Error('Prepared delegation Git hooks must remain inert and local');
 const state={head_ref:headRef,head,index_bytes:index.length,index_sha256:sha(index),config_sha256:sha(configBytes),status_sha256:sha(Buffer.from(rows.join('\n')+'\n')),worktree:Object.fromEntries(['notes.txt','staged-note.txt','unstaged-note.txt'].map(file=>[file,sha(read(path.join(root,file)))]))};
 if(expected){const identity={head_ref:state.head_ref,head:state.head,index_bytes:state.index_bytes,index_sha256:state.index_sha256,config_sha256:state.config_sha256,worktree:state.worktree},frozen={head_ref:expected.head_ref,head:expected.head,index_bytes:expected.index_bytes,index_sha256:expected.index_sha256,config_sha256:expected.config_sha256,worktree:expected.worktree};if(!isDeepStrictEqual(identity,frozen)||(!editable.length&&state.status_sha256!==expected.status_sha256))throw Error('Prepared delegation Git identity changed');}
 return state;
}
function prepareWorkspaceGit(root,files,git){
 const notes=files['notes.txt'];if(!files['staged-note.txt']||!files['unstaged-note.txt']||!notes)throw Error('Delegation fixture dirty inputs missing');
 fs.rmSync(path.join(root,'notes.txt'));fs.writeFileSync(path.join(root,'staged-note.txt'),'Committed staged-note baseline.\n');fs.writeFileSync(path.join(root,'unstaged-note.txt'),'Committed unstaged-note baseline.\n');
 gitRun(root,git,['init','--initial-branch=main']);gitRun(root,git,['config','user.name','VCP-Delegation-Fixture']);gitRun(root,git,['config','user.email','vcp-delegation-fixture@example.invalid']);gitRun(root,git,['config','core.hooksPath','.git/hooks']);gitRun(root,git,['add',...Object.keys(files).filter(file=>file!=='notes.txt')]);gitRun(root,git,['commit','-m','fixture: establish clean delegation base'],{GIT_AUTHOR_DATE:'2000-01-01T00:00:00Z',GIT_COMMITTER_DATE:'2000-01-01T00:00:00Z'});
 fs.writeFileSync(path.join(root,'staged-note.txt'),'Staged edit after the synthetic base.\n');fs.writeFileSync(path.join(root,'unstaged-note.txt'),'Working-tree edit after the synthetic base.\n');fs.writeFileSync(path.join(root,'notes.txt'),notes);gitRun(root,git,['add','staged-note.txt']);return workspaceGitState(root,null,{git});
}
function fixtureFiles(root,manifestFile){const manifest=JSON.parse(read(manifestFile)),files={};if(manifest.revision!=='p7-05-delegation-generation-v1'||JSON.stringify(manifest.editable)!=='["src/cart.cjs","src/cents.cjs"]'||!manifest.files.some(file=>file.path==='src/cart.cjs')||!manifest.files.some(file=>file.path==='src/cents.cjs'))throw Error('Frozen delegation fixture manifest changed');for(const file of manifest.files){const bytes=read(safeChild(root,file.path));if(bytes.length!==file.bytes||sha(bytes)!==file.sha256)throw Error('Frozen delegation fixture changed: '+file.path);files[file.path]=bytes;}return {manifest,files};}
function exactCounter(v){if(typeof v!=='string'||!/^\d+$/.test(v)||!Number.isSafeInteger(Number(v)))throw Error('Invalid canonical counter');return Number(v);}
function reproductionValue(value){
 if(typeof value==='number')return value;
 // Historical prompts did not specify numeric scalar types for expected/actual.
 // Recognize only an explicit integer annotation, never infer a value from prose.
 if(typeof value!=='string'||value.length>2048)return null;
 const match=/^(0|[1-9][0-9]*)[ \t]+[—–-][ \t]+(\S[^\r\n]*)(?![\s\S])/.exec(value);
 if(!match)return null;
 const number=Number(match[1]);return Number.isSafeInteger(number)?number:null;
}
function demonstrated(reproduction,defect,allowAnnotations=false){
 if(!reproduction||!Array.isArray(reproduction.arguments)||!reproduction.arguments.every(n=>Number.isSafeInteger(n)&&n>=0))return false;
 const args=reproduction.arguments;let expected,actual;
 if(defect.path==='shipping.cjs'&&args.length===1){expected=args[0]>=100?0:5;actual=args[0]>100?0:5;}
 else if(defect.path==='tax.cjs'&&args.length===2&&args[1]<=100){expected=Number((BigInt(args[0])*BigInt(args[1])+50n)/100n);actual=Math.floor(args[0]*args[1]/100);}
 else return false;
 const scalar=value=>allowAnnotations?reproductionValue(value):value;
 return expected!==actual&&scalar(reproduction.expected)===expected&&scalar(reproduction.actual)===actual;
}
// Resolve citations only from retained canonical artifacts. Model-authored maps
// and path-looking substrings are not authority for an opaque evidence ID.
function reviewEvidence(state,transcript,evidence,files,taskId){
 const rows=Object.values(state.records),one=(collection,id)=>{const matches=rows.filter(row=>row.collection===collection&&row.id===id);return matches.length===1?matches[0].value:null;};
 const task=one('task',taskId),scope=task?.scope;
 if(!scope||scope.task!==taskId)throw Error('Canonical review task missing');
 const retained=(item,channel,schema)=>{
  const descriptor=one('artifact',item?.artifact),bytes=typeof item?.text==='string'?Buffer.from(item.text):null;
  return descriptor&&bytes&&bytes.length<=1024*1024&&descriptor.spec?.id===item.artifact&&descriptor.state==='complete'&&descriptor.spec.channel===channel&&descriptor.spec.schema===schema&&isDeepStrictEqual(descriptor.spec.scope,scope)&&descriptor.length===String(bytes.length)&&descriptor.sha256===sha(bytes)&&item.sha256===descriptor.sha256&&isDeepStrictEqual(descriptor.retained,[{start:'0',end:String(bytes.length)}]);
 };
 if(!retained(transcript,'child_transcript','retained-full-output/1'))throw Error('Canonical review transcript identity mismatch');
 const workspace=one('workspace',scope.workspace),graph=one('projection',task.root),child=graph?.document_type==='vcp_task_graph_v1'?graph.children?.[taskId]:null;
 const root=task.parent?child?.isolated_root:scope.workspace,binding=task.parent?child?.binding:workspace?.binding?.revision;
 if(!root||typeof binding!=='string'||(task.parent&&child?.parent!==task.parent))throw Error('Canonical review source binding missing');
 const references=new Map(),duplicates=new Set();
 for(const item of evidence??[]){
  if(references.has(item?.artifact)||duplicates.has(item?.artifact)){references.delete(item.artifact);duplicates.add(item.artifact);continue;}
  if(!retained(item,'evidence','vcp-tool-result-v1'))continue;
  let value;try{value=JSON.parse(item.text);}catch{continue;}
  const version=value.version,source=version&&sources[version.path];
  if(typeof source!=='string'||version.root!==root||version.binding!==binding||version.sha256!==files[version.path]||version.bytes!==String(Buffer.byteLength(source))||sha(Buffer.from(source))!==version.sha256||typeof value.text!=='string')continue;
  const range=value.returned_range;
  // This fixed review fixture has one relevant line per source. Partial reads
  // must prove that line was actually returned, not merely hash the whole file.
  if(range){if(range.start_line!==1||range.end_line!==1||value.text!==source)continue;}
  else if(value.complete!==true||value.text!==source)continue;
  references.set(item.artifact,{path:version.path,start:1,end:1});
 }
 return {references,files};
}
function gradeReview(answer,expected=rubric,context=null,options={}){
 if(!answer||!Array.isArray(answer.findings)||answer.findings.length>64)throw Error('Bounded JSON review answer required');
 const valid=f=>f&&['defect','suggestion'].includes(f.kind)&&typeof f.path==='string'&&Number.isInteger(f.line)&&['trigger','consequence','uncertainty'].every(k=>typeof f[k]==='string'&&f[k].trim().length>0&&f[k].length<=2048)&&Array.isArray(f.evidence)&&f.evidence.length>0&&f.evidence.every(x=>typeof x==='string'&&x.length<=1024);
 const defects=answer.findings.filter(f=>f?.kind==='defect');
 const reference=e=>{
  if(context?.references.has(e))return context.references.get(e);
  if(Object.hasOwn(sources,e)&&(!context||Object.hasOwn(context.files,e)))return {path:e,start:1,end:1};
  const match=/^([A-Za-z0-9_./-]+):([1-9][0-9]*)(?:-([1-9][0-9]*))?(?=$|\s|[,;])/.exec(e);
  if(!match||!Object.hasOwn(sources,match[1])||(context&&!Object.hasOwn(context.files,match[1])))return null;
  const start=Number(match[2]),end=Number(match[3]??match[2]);
  return start===1&&end===1?{path:match[1],start,end}:null;
 };
 // Annotation compatibility is opt-in for offline regrading of historical prompts.
 // New live prompts explicitly require typed values and retain strict comparison.
 const detected=(f,d)=>valid(f)&&f.path===d.path&&f.line===d.line&&demonstrated(f.reproduction,d,options.historical_annotations===true);
 const matching=(f,d)=>detected(f,d)&&f.introduced_by_change===d.introduced_by_change&&f.evidence.some(e=>reference(e)?.path==='base/'+d.path);
 const hits=expected.defects.filter(d=>defects.some(f=>matching(f,d))).length;
 const noticed=expected.defects.filter(d=>defects.some(f=>detected(f,d))).length;
 const falsePositives=defects.filter(f=>!expected.defects.some(d=>detected(f,d))).length;
 const unqualified=defects.filter(f=>expected.defects.some(d=>detected(f,d))&&!expected.defects.some(d=>matching(f,d))).length;
 const duplicates=expected.defects.reduce((count,d)=>count+Math.max(0,defects.filter(f=>detected(f,d)).length-1),0);
 return {pass:answer.findings.every(valid)&&hits===expected.minimum_seeded&&falsePositives<=expected.maximum_false_positives&&unqualified===0&&duplicates===0,seeded_found:hits,seeded_detected:noticed,seeded_total:expected.minimum_seeded,false_positives:falsePositives,unqualified_findings:unqualified,duplicate_findings:duplicates,manual_evidence_usefulness_review_required:true};
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
 if(typeof spec.stage_cap_usd!=='string'||typeof spec.overall_cap_usd!=='string'||!Number.isSafeInteger(spec.prior_exposure_micros)||spec.prior_exposure_micros<0)throw Error('Explicit positive stage/overall ceiling and prior exposure are required');
 const cap=prior.micros(spec.stage_cap_usd),overall=prior.micros(spec.overall_cap_usd);
 if(overall>100000000||BigInt(spec.prior_exposure_micros)+BigInt(cap)>BigInt(overall))throw Error('Stage allocation exceeds remaining explicit campaign ceiling');
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
 const delegation=spec.stage==='generation'?fixtureFiles(delegationProject,path.join(delegationFixture,'manifest.json')):null;
 const files=spec.stage==='review'?sources:delegation.files;
 const prompt=spec.stage==='review'?reviewPrompt:delegation.manifest.prompt;
 const arms=spec.stage==='review'?['baseline','review_child']:['generation_child'];
 const allocation=Math.floor(cap/arms.length);if(allocation<1||arms.some(arm=>arm!=='baseline'&&Math.floor(allocation*3/4)<1))throw Error('Positive per-arm allocations required');
 fs.mkdirSync(destination,{mode:0o700});
 write(path.join(destination,'private-rubric.json'),rubric);
 const dependencies=spec.stage==='generation'?['scripts/evals/p6-live-runner.cjs','scripts/evals/builtin-live-runner.cjs','scripts/evals/builtin-generation-prepare.cjs','scripts/evals/delegation-generation-oracle.cjs','src/evals/delegation/generation-v1/manifest.json']:['scripts/evals/p6-live-runner.cjs','scripts/evals/builtin-live-runner.cjs','scripts/evals/builtin-generation-prepare.cjs','scripts/evals/builtin-generation-oracle.cjs','src/evals/skills/builtin/generation-v1/manifest.json'];
 const plan={schema:'p7-delegation-live-plan/1',stage:spec.stage,directory:destination,adapter,adapter_sha256:sha(adapterBytes),adapter_source_sha256:sha(read(path.join(repo,'src/crates/vcp-cli/examples/delegation-live-adapter.rs'))),runner_sha256:sha(read(__filename)),dependencies:Object.fromEntries(dependencies.map(f=>[f,sha(read(path.join(repo,f)))])),adapter_environment:{...adapterEnvironmentPin},generation_fixture:spec.stage==='generation'?inventory(delegationProject):null,generation_manifest:spec.stage==='generation'?path.join(delegationFixture,'manifest.json'):null,assets,profile_source:plain(spec.profile),profile_sha256:sha(profileBytes),catalog,catalog_sha256:sha(read(catalog)),git,git_sha256:sha(read(git,128*1024*1024)),runtime,stage_cap_micros:cap,prior_exposure_micros:spec.prior_exposure_micros,overall_cap_micros:overall,permission_review:spec.stage==='generation'?generation.permissionReview(runtime,true):{automatic_effects:[],mode:'plan'},arms:[]};
 for(const arm of arms){
   const base=path.join(destination,arm),workspace=path.join(base,'workspace');fs.mkdirSync(workspace,{recursive:true});fs.mkdirSync(path.join(base,'children'));put(workspace,files);const armGit=spec.stage==='generation'?prepareWorkspaceGit(workspace,files,git):null;
   const derived=spec.stage==='generation'?{...generation.qualifiedProfile(profile,workspace,catalog,allocation,runtime,true),affected_paths:['src/cart.cjs','src/cents.cjs']}:{...profile,workspace,catalog,budget_usd:usd(allocation),maximum_autonomy:'plan',automatic_effects:[],affected_paths:Object.keys(files),checks:[],processes:[]};
  write(path.join(base,'profile.json'),derived);write(path.join(base,'prompt.txt'),prompt);
  const delegated=arm!=='baseline';
  if(delegated)write(path.join(base,'delegation.json'),{version:1,git,disposable_parent:path.join(base,'children'),objective:prompt,acceptance:['Meet the frozen independent acceptance rubric; report check limitations'],mode:spec.stage==='review'?'read_only':'isolated_write',write_paths:spec.stage==='review'?[]:['src/cart.cjs','src/cents.cjs'],read_paths:[''],untracked_inputs:Object.keys(files),allocation_usd:usd(Math.floor(allocation*3/4)),seconds:derived.deadline_seconds,required_checks:[],role:spec.stage==='review'?'review':'generation'});
  const driver={profile:path.join(base,'profile.json'),workspace,directory:base,prompt:path.join(base,'prompt.txt'),delegation:delegated?path.join(base,'delegation.json'):null,git,generation:spec.stage==='generation',human_note:spec.stage==='generation'?humanNote:null};
  write(path.join(base,'adapter-spec.json'),driver);
   plan.arms.push({name:arm,cap_micros:allocation,files:inventory(workspace),git:armGit,frozen:Object.fromEntries(['profile.json','prompt.txt','adapter-spec.json',...(delegated?['delegation.json']:[])].map(f=>[f,sha(read(path.join(base,f)))]))});
 }
 plan.rubric_sha256=sha(read(path.join(destination,'private-rubric.json')));write(path.join(destination,'plan.json'),plan);
 return {plan:path.join(destination,'plan.json'),sha256:sha(read(path.join(destination,'plan.json'))),stage_cap_micros:cap,model_calls:0,qualification:'adapter build and offline controls required before execution; not terminal U06'};
}
function validate(plan,file){
 if(plan.schema!=='p7-delegation-live-plan/1'||!['review','generation'].includes(plan.stage)||plain(path.dirname(file))!==plan.directory||plan.runner_sha256!==sha(read(__filename))||plan.adapter_sha256!==sha(read(plan.adapter,1024*1024*1024))||plan.adapter_source_sha256!==sha(read(path.join(repo,'src/crates/vcp-cli/examples/delegation-live-adapter.rs')))||plan.profile_sha256!==sha(read(plan.profile_source))||plan.catalog_sha256!==sha(read(plan.catalog))||plan.git_sha256!==sha(read(plan.git,128*1024*1024))||!isDeepStrictEqual(plan.adapter_environment,adapterEnvironmentPin))throw Error('Frozen stage identity changed');
 noParentInstructions(plan.directory);privateDirectory(plan.directory);
 for(const [name,digest]of Object.entries(plan.dependencies))if(sha(read(safeChild(repo,name)))!==digest)throw Error('Qualification dependency changed');
 if(!Number.isSafeInteger(plan.prior_exposure_micros)||plan.prior_exposure_micros<0||!Number.isSafeInteger(plan.stage_cap_micros)||plan.stage_cap_micros<=0||!Number.isSafeInteger(plan.overall_cap_micros)||plan.overall_cap_micros<=0||plan.overall_cap_micros>100000000||plan.prior_exposure_micros+plan.stage_cap_micros>plan.overall_cap_micros||!Array.isArray(plan.arms)||!plan.arms.length||plan.arms.some(a=>!Number.isSafeInteger(a.cap_micros)||a.cap_micros<=0)||plan.arms.reduce((s,a)=>s+a.cap_micros,0)>plan.stage_cap_micros)throw Error('Stage exposure exceeds approved campaign ceiling');
 const expectedFixture=plan.stage==='generation'?inventory(delegationProject):null;if(!isDeepStrictEqual(plan.generation_fixture,expectedFixture))throw Error('Independent generation fixture changed');
 if(plan.stage==='generation'&&plan.generation_manifest!==path.join(delegationFixture,'manifest.json'))throw Error('Independent generation manifest changed');
 if(skills.fixedProfileReasons(JSON.parse(read(plan.profile_source))).length)throw Error('Exact provider qualification expired or changed');
 if(JSON.stringify(generation.inventory(path.join(path.dirname(plan.adapter),'skills/builtin')))!==JSON.stringify(plan.assets)||sha(read(path.join(plan.directory,'private-rubric.json')))!==plan.rubric_sha256)throw Error('Assets or private rubric changed');
 if(plan.runtime){const input={node:plan.runtime.node,launcher:plan.runtime.launcher,...(plan.runtime.build_receipt?{build_receipt:plan.runtime.build_receipt}:{})};if(JSON.stringify(generation.qualifyRuntime(input))!==JSON.stringify(plan.runtime))throw Error('Pinned runtime changed');}
  for(const arm of plan.arms){const base=safeChild(plan.directory,arm.name);if(JSON.stringify(inventory(path.join(base,'workspace')))!==JSON.stringify(arm.files))throw Error('Prepared fixture changed');if(plan.stage==='generation')workspaceGitState(path.join(base,'workspace'),arm.git,{git:plan.git});for(const [f,digest]of Object.entries(arm.frozen))if(sha(read(safeChild(base,f)))!==digest)throw Error('Prepared arm changed');}
}
function generationGrade(plan,base,arm,adapter,state){
 const workspace=path.join(base,'workspace'),expected={...arm.files,'notes.txt':sha(Buffer.from(humanNote))},actual=inventory(workspace);workspaceGitState(workspace,{...arm.git,worktree:{...arm.git.worktree,'notes.txt':sha(Buffer.from(humanNote))}},{git:plan.git,editable:['src/cart.cjs','src/cents.cjs']});
 for(const name of new Set([...Object.keys(expected),...Object.keys(actual)]))if(!['src/cart.cjs','src/cents.cjs'].includes(name)&&actual[name]!==expected[name])throw Error('Parent preservation failed: '+name);
 const verification=adapter.result.verification,root=adapter.result.scope.task,task=Object.values(state.records).find(r=>r.collection==='task'&&r.id===root)?.value;
 if(!verification||verification.scope.task!==root||JSON.stringify(verification.fingerprint)!==JSON.stringify(task?.fingerprint)||verification.cost?.certainty!=='known'||verification.unresolved_effects?.length||verification.outstanding_issues?.length||!verification.checks?.length||verification.checks.some(c=>c.outcome?.status!=='passed'||c.exit_code!==0))throw Error('Current integrated parent verification did not pass');
 // The independent integer oracle runs the exact integrated source in a frozen
 // mirror, while preservation of the actual concurrent human note is checked
 // above. Parent verification itself ran against the real current workspace.
 const mirror=path.join(base,'oracle-workspace');fs.mkdirSync(mirror);
 const fixture=delegationProject;put(mirror,Object.fromEntries(modelFiles(fixture).map(f=>[f,read(safeChild(fixture,f))])));
 for(const file of ['src/cart.cjs','src/cents.cjs'])fs.writeFileSync(path.join(mirror,file),read(path.join(workspace,file)));
 const result=spawnSync(plan.runtime.node,[path.join(__dirname,'delegation-generation-oracle.cjs'),mirror],{env:{SystemRoot:process.env.SystemRoot},encoding:'utf8',timeout:10000,maxBuffer:65536,windowsHide:true});
 if(result.error||![0,1].includes(result.status))throw Error('Independent generation oracle unavailable');
 return {...JSON.parse(result.stdout),actual_parent_source_sha256:actual['src/cart.cjs'],actual_parent_sources_sha256:{'src/cart.cjs':actual['src/cart.cjs'],'src/cents.cjs':actual['src/cents.cjs']},human_note_preserved:true,canonical_parent_verification:verification.id};
}
function run(file,authorization){
 if(sha(read(file))!==authorization)throw Error('Exact authorized stage hash required');const plan=JSON.parse(read(file));validate(plan,file);
 write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,at:new Date().toISOString(),meaning:'Entire stage allocation held until reconciliation; never replay or recycle'});
 const report={schema:'p7-delegation-result/1',stage:plan.stage,plan_sha256:authorization,actual_cost_micros:0,stopped:false,arms:[],terminal_u06:false};
 for(const arm of plan.arms){if(report.stopped)break;const base=path.join(plan.directory,arm.name),record={arm:arm.name,status:'failed',actual_cost_micros:null};report.arms.push(record);
  try{
   const spec=path.join(base,'adapter-spec.json'),start=Date.now(),result=invokeAdapter(plan.adapter,[spec,sha(read(spec))],(JSON.parse(read(path.join(base,'profile.json'))).deadline_seconds+240)*1000);
   record.latency_ms=Date.now()-start;write(path.join(base,'adapter-stdout.jsonl'),result.stdout);write(path.join(base,'adapter-stderr.txt'),result.stderr);
   record.adapter_exit_code=result.status;record.adapter_signal=result.signal;if(result.error||result.status!==0){record.stderr_summary=stderrSummary(result.stderr);throw Error(`Adapter interrupted${result.error?` (${result.error})`:result.signal?` (${result.signal})`:` (exit ${result.status})`}; canonical accounting is unknown and must be reconciled before any next arm${record.stderr_summary?`: ${record.stderr_summary}`:''}`);}
   const adapter=JSON.parse(read(path.join(base,'adapter-result.json'))),state=JSON.parse(read(path.join(base,'canonical-state.json'))),config=JSON.parse(read(path.join(base,'canonical-config.json')));
   record.accounting=accounting(state,arm.cap_micros,config.root_task,adapter.result?.child);record.actual_cost_micros=record.accounting.actual_cost_micros;
   if(!Number.isSafeInteger(report.actual_cost_micros)||report.actual_cost_micros+record.actual_cost_micros>plan.overall_cap_micros)throw Error('Observed model work exceeds frozen campaign ceiling');
   report.actual_cost_micros+=record.actual_cost_micros;
   if(adapter.status!=='observed'||adapter.owner_close_error)throw Error(adapter.reason||'Owner did not close cleanly');
   if(arm.name!=='baseline'&&!record.accounting.child_attempts)throw Error('No actual retained provider child was observed');
   if(plan.stage==='review'){
    if(JSON.stringify(inventory(path.join(base,'workspace')))!==JSON.stringify(arm.files))throw Error('Read-only review changed parent source or index');
    if(adapter.result.child){const child=path.join(base,'children',adapter.result.child),observed=inventory(child);delete observed['.vcp-child-owner'];if(!isDeepStrictEqual(observed,arm.files))throw Error('Read-only review changed isolated source or index');}
    const answers=adapter.result.transcripts.flatMap(transcript=>{try{return [{answer:JSON.parse(transcript.text),transcript}];}catch{return [];}}).filter(x=>Array.isArray(x.answer.findings));
    record.quality=answers.length===1?gradeReview(answers[0].answer,JSON.parse(read(path.join(plan.directory,'private-rubric.json'))),reviewEvidence(state,answers[0].transcript,adapter.result.evidence,arm.files,adapter.result.child??config.root_task)):{pass:false,reason:'Expected one unambiguous retained JSON review answer'};
   }else record.quality=generationGrade(plan,base,arm,adapter,state);
   record.status=record.quality.pass?'passed':'failed';
  }catch(error){record.reason=error.message;report.stopped=true;if(record.actual_cost_micros===null)report.actual_cost_micros=null;}
  write(path.join(base,'result.json'),record);
 }
 write(path.join(plan.directory,'result.json'),report);return report;
}
module.exports={prepare,validate,run,gradeReview,reviewEvidence,accounting,sources,rubric,workspaceGitState,prepareWorkspaceGit,gitEnvironment,gitRun,adapterEnvironment,invokeAdapter,stderrSummary,humanNote};
if(require.main===module){try{const [command,file,extra,...rest]=process.argv.slice(2);if(rest.length||!file||!extra||!['prepare','run'].includes(command))throw Error('Usage: delegation-live-runner.cjs prepare <spec> <new-private-dir> | run <plan> <authorized-sha256>');const output=command==='prepare'?prepare(file,extra):run(file,extra);console.log(JSON.stringify(output));if(command==='run'&&(output.stopped||output.arms.some(arm=>arm.status!=='passed')))process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
