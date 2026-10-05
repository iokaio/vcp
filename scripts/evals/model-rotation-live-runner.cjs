// SPDX-License-Identifier: Apache-2.0
'use strict';
// One-shot, synthetic source edits only. Preparation never calls a provider.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {boundaries:b}=require('./p6-live-runner.cjs');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const boundarySource=path.join(__dirname,'p6-live-runner.cjs');
const json=value=>JSON.stringify(value,null,2)+'\n';
const files={
  'value.cjs':"module.exports = 41;\n",
  'package.json':'{"private":true,"scripts":{"test":"node --test acceptance.cjs"}}\n',
  'acceptance.cjs':"const test=require('node:test'),assert=require('node:assert/strict');test('changed_value',()=>assert.equal(require('./value.cjs'),42));\n",
};
const prompt='Change only value.cjs so its CommonJS export is the number 42. Preserve package.json and acceptance.cjs exactly. Use the configured native verification check via vcp_verify before completion. Report actual checks and any unrun checks truthfully. Do not install dependencies or add files.\n';
function bounds(plan) {
  if(plan.schema!=='model-rotation-live-plan/1'||plan.aggregate_cap_micros!==25000000||plan.runs.length!==4||plan.runs.some((r,i)=>r.id!==['baseline-A','baseline-B','rotation-A','rotation-B'][i]||r.cap_micros!==3000000)||plan.runs.reduce((n,r)=>n+r.cap_micros,0)!==12000000)throw Error('Four permanent $3 allocations within the $25 aggregate are required');
}
function gate(p,strategy,now=Date.now()) {
  b.noSecrets(p);
  if(p.version!==1||p.trust_workspace!==true||p.max_requests!==12||p.deadline_seconds!==180||p.output_tokens!=='1024'||p.max_transport_retries>2||p.provider_timeout_seconds>45||p.qualification_endpoint||p.skills||p.decisions||p.mcp?.length||p.mcp_http?.length||p.hooks?.length||p.observers)throw Error('Bounded production profile required');
  if(p.provider?.price?.currency!=='USD'||p.provider?.valid_until<=now||p.provider?.compatibility?.valid_until<=now)throw Error('Current USD metadata required');
  if(p.provider?.compatibility?.model!=='qwen/qwen3-coder'||p.provider?.compatibility?.endpoint!=='google-vertex/us-south1')throw Error('Exact baseline endpoint required');
  if(strategy==='baseline'&&p.routing)throw Error('Baseline must be fixed');
  if(strategy==='rotation'&&!p.routing?.rotation)throw Error('Rotation requires captured owner choice sets');
  if(p.maximum_autonomy!=='autonomous'||JSON.stringify([...p.automatic_effects].sort())!==JSON.stringify(['execute','opaque','read','write'])||JSON.stringify([...p.canonical_tools].sort())!==JSON.stringify(['vcp_patch','vcp_read','vcp_verify']))throw Error('Fixed native verifier grants and model tool ceiling required');
}
function prepare(specFile,directory) {
  const spec=JSON.parse(b.read(specFile));b.noSecrets(spec);
  directory=b.plain(path.resolve(directory));b.privateDirectory(directory);b.noParentInstructions(directory);
  if(fs.existsSync(directory))throw Error('New private destination required');
  const executable=b.plain(path.resolve(spec.executable)),node=b.plain(path.resolve(spec.node));
  const templates={};
  for(const strategy of ['baseline','rotation']) {const file=b.plain(path.resolve(spec[strategy+'_profile']));const bytes=b.read(file),profile=JSON.parse(bytes);gate(profile,strategy);templates[strategy]={file,hash:sha(bytes),profile,catalog:b.plain(path.resolve(profile.catalog)),catalog_sha256:sha(b.read(profile.catalog))};}
  const plan={schema:'model-rotation-live-plan/1',directory,executable,executable_sha256:sha(b.read(executable,1024*1024*1024)),node,node_sha256:sha(b.read(node,1024*1024*1024)),runner_sha256:sha(b.read(__filename)),boundary_sha256:sha(b.read(boundarySource)),aggregate_cap_micros:25000000,allocated_cap_micros:12000000,unused_cap_micros:13000000,account_root:path.join(process.env.LOCALAPPDATA||'', 'VCP','account'),templates,runs:['baseline-A','baseline-B','rotation-A','rotation-B'].map(id=>({id,cap_micros:3000000})),limitations:'Four small synthetic tasks; no guarantee an upstream pool will produce 429s; comparative quality and high-token throughput are unqualified.'};
  bounds(plan);fs.mkdirSync(directory,{mode:0o700});
  for(const row of plan.runs) {
    const base=b.safeChild(directory,row.id);fs.mkdirSync(base);fs.mkdirSync(path.join(base,'workspace'));fs.mkdirSync(path.join(base,'data'));
    row.files={};for(const [name,text] of Object.entries(files)){b.write(path.join(base,'workspace',name),text);row.files[name]=sha(Buffer.from(text));}
    const p={...templates[row.id.split('-')[0]].profile,workspace:path.join(base,'workspace'),catalog:templates[row.id.split('-')[0]].catalog,budget_usd:'3.000000',affected_paths:['value.cjs'],processes:[{name:'node',executable:node,environment:process.env.SystemRoot?{SystemRoot:process.env.SystemRoot}:{},required_isolation:[],reduced_isolation:true,inputs:[],max_timeout_ms:10000}],checks:[{manifest:'package.json',runner:'node',profile:'node',timeout_ms:10000,expected_tests:['changed_value'],rationale:'Frozen synthetic source acceptance'}]};
    b.write(path.join(base,'profile.json'),p);b.write(path.join(base,'prompt.txt'),prompt);row.profile_sha256=sha(b.read(path.join(base,'profile.json')));row.prompt_sha256=sha(Buffer.from(prompt));
  }
  b.write(path.join(directory,'plan.json'),plan);return {plan:path.join(directory,'plan.json'),plan_sha256:sha(b.read(path.join(directory,'plan.json'))),allocated_cap_micros:12000000,model_calls:0};
}
function redact(text) {
  for(const [key,value] of Object.entries(process.env))if(/key|token|secret|password|credential/i.test(key)&&value.length>=8)text=text.split(value).join('[REDACTED]');
  return text.replace(/Bearer\s+[^\s"']+/gi,'Bearer [REDACTED]').replace(/sk-or-v1-[a-zA-Z0-9_-]+/g,'[REDACTED]');
}
function invoke(executable,args,timeout,cwd) {
  return new Promise(resolve=>{
    const child=spawn(executable,args,{cwd,shell:false,windowsHide:true,stdio:['ignore','pipe','pipe']});let stdout='',stderr='',error=null,size=0;
    const kill=()=>{if(process.platform==='win32'&&child.pid)spawn(path.join(process.env.SystemRoot,'System32','taskkill.exe'),['/PID',String(child.pid),'/T','/F'],{windowsHide:true,stdio:'ignore'});else child.kill();};
    const timer=setTimeout(()=>{error='deadline';kill();},timeout);
    const collect=stream=>bytes=>{size+=bytes.length;if(size>16*1024*1024){error='output_bound';kill();return;}if(stream==='stdout')stdout+=bytes.toString('utf8');else stderr+=bytes.toString('utf8');};
    child.stdout.on('data',collect('stdout'));child.stderr.on('data',collect('stderr'));child.on('error',()=>{error='spawn_failed';});child.on('close',status=>{clearTimeout(timer);resolve({status,error,stdout:redact(stdout),stderr:redact(stderr)});});
  });
}
async function inspect(plan,base,task,view,call) {
  const pages=[];let cursor=null;
  do {const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'inspect',task,'--view',view,'--limit','128'];if(cursor)args.push('--cursor',JSON.stringify(cursor));const response=await call(plan.executable,args,30000);if(response.error||response.status!==0)throw Error('Canonical inspection unavailable');const data=b.frames(response.stdout).find(f=>f.type==='result')?.data;if(!Array.isArray(data?.items)||!Array.isArray(data?.gaps)||pages.length>=64)throw Error('Inspection bounds exceeded');pages.push(data);cursor=data.next_cursor;}while(cursor);return pages;
}
function accounting(pages,cap) {
  const items=pages.flatMap(p=>p.items);if(pages.some(p=>p.gaps.length)||items.some(i=>i.visibility!=='available'))throw Error('Incomplete accounting');
  const ledgers=items.filter(i=>i.collection==='ledger').map(i=>i.record);
  if(ledgers.length!==1||ledgers[0].currency!=='USD'||ledgers[0].cap!==String(cap)||ledgers[0].overrun)throw Error('Invalid canonical cap');
  const money={};for(const field of ['settled','active','unresolved']) {const value=ledgers[0][field];if(!/^(0|[1-9][0-9]*)$/.test(value)||!Number.isSafeInteger(Number(value)))throw Error('Invalid money');money[field]=Number(value);}
  if(money.settled+money.active+money.unresolved>cap)throw Error('Canonical costs exceed allocation');return money;
}
async function run(planFile,authorization,call=invoke) {
  const bytes=b.read(planFile);if(sha(bytes)!==authorization)throw Error('Exact prepared plan hash required');const plan=JSON.parse(bytes);bounds(plan);b.privateDirectory(plan.directory);b.noParentInstructions(plan.directory);
  if(path.resolve(path.dirname(planFile))!==plan.directory||plan.runner_sha256!==sha(b.read(__filename))||plan.boundary_sha256!==sha(b.read(boundarySource))||plan.executable_sha256!==sha(b.read(plan.executable,1024*1024*1024))||plan.node_sha256!==sha(b.read(plan.node,1024*1024*1024))||plan.account_root!==path.join(process.env.LOCALAPPDATA||'','VCP','account'))throw Error('Prepared execution/account identity changed');
  for(const t of Object.values(plan.templates))if(t.hash!==sha(b.read(t.file))||t.catalog_sha256!==sha(b.read(t.catalog)))throw Error('Source metadata/profile changed');
  for(const row of plan.runs){const base=b.safeChild(plan.directory,row.id),workspace=path.join(base,'workspace');if(fs.readdirSync(path.join(base,'data')).length||JSON.stringify(b.filesUnder(workspace))!==JSON.stringify(Object.keys(row.files).sort())||Object.entries(row.files).some(([name,hash])=>hash!==sha(b.read(path.join(workspace,name))))||row.profile_sha256!==sha(b.read(path.join(base,'profile.json')))||row.prompt_sha256!==sha(b.read(path.join(base,'prompt.txt'))))throw Error('Frozen task changed');gate(JSON.parse(b.read(path.join(base,'profile.json'))),row.id.split('-')[0]);}
  // Permanent before the first dispatch. A crash never frees or recycles money.
  b.write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,allocated_cap_micros:12000000,meaning:'One shot; no task replay or reuse of unused/failed allocations'});
  const result={schema:'model-rotation-live-result/1',plan_sha256:authorization,reserved_cap_micros:12000000,runs:[],stopped:false};
  async function task(row){const base=b.safeChild(plan.directory,row.id),report={id:row.id,cap_micros:row.cap_micros,status:'unknown',money:null,oracle:'not_run'};const start=Date.now();const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd','3.000000','--autonomy',JSON.parse(b.read(path.join(base,'profile.json'))).maximum_autonomy];b.write(path.join(base,'attempted.json'),{plan_sha256:authorization,started_at:new Date().toISOString(),args});const out=await call(plan.executable,args,240000);report.elapsed_ms=Date.now()-start;b.write(path.join(base,'stdout.jsonl'),redact(out.stdout));b.write(path.join(base,'stderr.txt'),redact(out.stderr));try {if(out.error)throw Error('Process interrupted');const frames=b.frames(out.stdout),accepted=frames.find(f=>f.type==='accepted'),final=frames.findLast(f=>f.type==='result');if(!accepted?.scope?.task)throw Error('Missing canonical task identity');report.scope=accepted.scope;for(const view of ['costs','routing','outputs']){const pages=await inspect(plan,base,accepted.scope.task,view,call);b.write(path.join(base,view+'.json'),pages);if(view==='costs')report.money=accounting(pages,row.cap_micros);}report.status=final?.conditions?.completed?'completed':'failed';if(report.money.active||report.money.unresolved)report.status='unknown';const workspace=path.join(base,'workspace');const preserved=['package.json','acceptance.cjs'].every(name=>sha(b.read(path.join(workspace,name)))===row.files[name])&&JSON.stringify(b.filesUnder(workspace))===JSON.stringify(Object.keys(row.files).sort());if(preserved&&b.read(path.join(workspace,'value.cjs')).toString('utf8')==='module.exports = 42;\n'){const check=await call(plan.node,['--test','acceptance.cjs'],10000,workspace);b.write(path.join(base,'oracle.json'),check);report.oracle=check.status===0&&!check.error?'passed':'failed';}else report.oracle='failed';}catch{report.status='unknown';report.reason='Interrupted or incomplete canonical evidence; retain full allocation';}b.write(path.join(base,'result.json'),report);return report;}
  for(const pair of [plan.runs.slice(0,2),plan.runs.slice(2,4)]){if(result.stopped)break;result.runs.push(...await Promise.all(pair.map(task)));if(result.runs.some(r=>r.status==='unknown'))result.stopped=true;}
  b.write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={prepare,run,bounds,gate,accounting,redact,files,invoke};
if(require.main===module)(async()=>{try{const [command,input,extra,...rest]=process.argv.slice(2);if(rest.length||!input||!extra||!['prepare','run'].includes(command))throw Error('Usage: model-rotation-live-runner.cjs prepare SPEC NEW_PRIVATE_DIRECTORY | run PLAN EXACT_PLAN_SHA256');console.log(json(command==='prepare'?prepare(input,extra):await run(input,extra)));}catch(error){console.error(error.message);process.exitCode=1;}})();
