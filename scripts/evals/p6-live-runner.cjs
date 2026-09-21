// SPDX-License-Identifier: Apache-2.0
'use strict';
// Explicitly authorized, one-shot CLI smoke trials. Never invent qualification.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawnSync} = require('node:child_process');
const quality = require('./p6-task-quality.cjs');
const repo = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
function plain(file) {
  file = path.resolve(file);
  for (let p = file; ; p = path.dirname(p)) {
    if (fs.existsSync(p) && fs.lstatSync(p).isSymbolicLink()) throw Error('Symlink or junction rejected');
    if (path.dirname(p) === p) break;
  }
  return file;
}
function read(file, limit = 16 * 1024 * 1024) {
  plain(file);
  const stat = fs.statSync(file);
  if (!stat.isFile() || stat.size > limit) throw Error('Bounded regular file required');
  return fs.readFileSync(file);
}
function write(file, value) { fs.writeFileSync(file, typeof value === 'string' ? value : json(value), {flag:'wx', mode:0o600}); }
function within(root, target) { const r = path.relative(root, target); return r === '' || (!r.startsWith('..') && !path.isAbsolute(r)); }
function safeChild(root, relative) {
  if (typeof relative !== 'string' || !relative || relative.includes('\\') || relative.includes(':')) throw Error('Invalid relative path');
  const result = plain(path.resolve(root, relative));
  if (!within(root, result) || result === root) throw Error('Path escapes trial root');
  return result;
}
function filesUnder(root) {
  const result=[];
  function visit(directory) { for(const entry of fs.readdirSync(directory,{withFileTypes:true})) {
    const file=plain(path.join(directory,entry.name));
    if(entry.isDirectory()) visit(file); else if(entry.isFile()) result.push(path.relative(root,file).split(path.sep).join('/')); else throw Error('Only ordinary project files are allowed');
  } }
  visit(root); return result.sort();
}
function noParentInstructions(directory) {
  for(let p=directory;;p=path.dirname(p)) { if(fs.existsSync(path.join(p,'AGENTS.md'))) throw Error('Trial needs an isolated ancestor chain without AGENTS.md'); if(path.dirname(p)===p) break; }
}
function privateDirectory(directory) {
  for(let p=directory;;p=path.dirname(p)) { if(fs.existsSync(path.join(p,'.git'))) throw Error('Private trial directory must be outside all repositories'); if(path.dirname(p)===p) break; }
  for(const name of ['OneDrive','OneDriveConsumer','OneDriveCommercial']) {
    const value=process.env[name]; if(value && fs.existsSync(value) && within(fs.realpathSync(value),directory)) throw Error('Private trial directory must be outside known sync roots');
  }
  if(process.platform==='win32' && !/^[a-z]:[\\/]/i.test(directory)) throw Error('Private trial requires a local drive');
}
function micros(text) {
  if (typeof text !== 'string' || !/^(0|[1-9][0-9]*)\.[0-9]{1,6}$/.test(text)) throw Error('Use exact positive USD decimal with at most six fractional digits');
  const [whole, fraction] = text.split('.');
  const result = BigInt(whole) * 1000000n + BigInt(fraction.padEnd(6, '0'));
  if (result <= 0n || result > BigInt(Number.MAX_SAFE_INTEGER)) throw Error('USD cap out of bounds');
  return Number(result);
}
const usd = n => `${Math.floor(n / 1000000)}.${String(n % 1000000).padStart(6,'0')}`;
function counter(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value)>BigInt(Number.MAX_SAFE_INTEGER)) throw Error('Expected a bounded canonical decimal counter');
  return Number(value);
}
function noSecrets(value) {
  if (!value || typeof value !== 'object') return;
  for (const [key, child] of Object.entries(value)) {
    if (/api[_-]?key|secret|password|authorization|credential/i.test(key) || /^(token|access_token|refresh_token)$/i.test(key)) throw Error('Embedded credentials rejected; use the existing OPENROUTER_API_KEY environment boundary');
    noSecrets(child);
  }
}
function profileReasons(p, strategy, now = Date.now()) {
  const reasons = [];
  if (p.version !== 1 || p.trust_workspace !== true) reasons.push('explicit trusted profile required');
  if (!Number.isSafeInteger(p.max_requests) || p.max_requests < 1 || p.max_requests > 8 || !Number.isSafeInteger(p.deadline_seconds) || p.deadline_seconds < 1 || p.deadline_seconds > 600) reasons.push('trial needs 1..8 requests and 1..600 second deadline');
  if (p.processes?.length || p.checks?.length || p.mcp?.length || p.mcp_http?.length || p.skills || p.decisions || p.qualification_endpoint) reasons.push('external tools, skills, evaluators, executable checks and endpoint overrides are outside this smoke trial');
  const snapshot = p.provider;
  if (!snapshot || snapshot.valid_until <= now || snapshot.compatibility?.valid_until <= now || snapshot.price?.currency !== 'USD' || snapshot.compatibility?.responses_text_tools !== true || snapshot.compatibility?.provider_preferences_qualified !== true) reasons.push('current qualified USD provider snapshot required');
  const routing = p.routing;
  const output = Math.min(counter(p.output_tokens ?? '4096'), counter(snapshot?.max_output ?? '0'), counter(routing?.policy?.output_tokens ?? '4096'));
  if (!(output > 0 && output <= 512)) reasons.push('host output limit must enforce the frozen 512-token bound');
  if (routing) {
    const e = routing.escalation;
    if (!e || e.max_transport_retries !== 0 || e.max_quality_switches !== 0 || e.max_decompositions !== 0 || e.max_total_attempts > p.max_requests || e.deadline <= now) reasons.push('routing must explicitly disable retries, switches and decomposition');
    const pin = routing.policy?.pin;
    if (strategy.startsWith('fixed_') && (!pin || pin.fallback_candidates?.length !== 0)) reasons.push('fixed strategy requires an exact pin without fallback');
    if (strategy === 'routed' && pin) reasons.push('routed strategy cannot use a fixed pin');
    const candidates = routing.catalog?.entries || [];
    if (!candidates.some(c => c.availability === 'supported' && c.compatibility?.some(e => e.kind === 'live' && e.state === 'supported' && e.valid_until > now) && c.memberships?.some(m => m.roles?.some(r => r.kind === 'live' && r.role === 'main' && r.valid_until > now)))) reasons.push('no current live-qualified routing memberships; bootstrap fixed-provider evidence first');
  } else {
    if (strategy === 'routed') reasons.push('routed strategy requires a qualified routing catalog');
    // Older CLI profiles cannot turn off their two transport retries.
    if (p.max_transport_retries !== 0 && p.max_requests !== 1) reasons.push('fixed-provider host must explicitly disable transport retries');
  }
  return reasons;
}
function prepare(specFile, destination) {
  const pool = quality.load();
  const specBytes = read(specFile, 1024 * 1024);
  const spec = JSON.parse(specBytes); noSecrets(spec);
  if (Object.keys(spec).sort().join() !== 'aggregate_cap_usd,executable,strategies' || !spec.strategies || typeof spec.strategies !== 'object') throw Error('Spec needs executable, aggregate_cap_usd, strategies');
  const ids = pool.manifest.strategies.map(s => s.id);
  if (Object.keys(spec.strategies).some(id => !ids.includes(id))) throw Error('Unknown strategy');
  const executable = plain(path.resolve(spec.executable));
  const executableHash = sha(read(executable, 1024 * 1024 * 1024));
  destination = plain(path.resolve(destination));
  if (within(repo, destination) || within(destination, repo)) throw Error('Private trial directory must be outside the repository');
  if (fs.existsSync(destination)) throw Error('Trial destination must be new; never reuse a store or workspace');
  noParentInstructions(path.dirname(destination));
  privateDirectory(destination);
  const aggregate = micros(spec.aggregate_cap_usd), allocation = Math.floor(aggregate / 18);
  if (allocation === 0) throw Error('Cap must fund a positive allocation for every planned run');
  const profiles = {};
  for (const id of ids) {
    if (!spec.strategies[id]) { profiles[id] = {reasons:['no externally supplied strategy profile']}; continue; }
    const file = plain(path.resolve(spec.strategies[id]));
    const bytes = read(file); const p = JSON.parse(bytes); noSecrets(p);
    const catalog = plain(path.resolve(p.catalog)); const catalogBytes = read(catalog);
    profiles[id] = {profile:p, source:file, source_sha256:sha(bytes), catalog, catalog_sha256:sha(catalogBytes), reasons:profileReasons(p,id)};
  }
  const fixed = ids.filter(id => id.startsWith('fixed_')).map(id => profiles[id].profile?.routing?.policy?.pin?.candidate ?? profiles[id].profile?.provider?.compatibility);
  if (fixed.every(Boolean) && fixed[0].model === fixed[1].model && fixed[0].endpoint === fixed[1].endpoint) throw Error('Fixed economical and stronger arms must name different exact providers/models');
  fs.mkdirSync(destination, {recursive:false, mode:0o700});
  const plan = {schema:'p6-live-plan/1', manifest_sha256:pool.manifest_sha256, grader_sha256:sha(read(path.join(__dirname,'p6-task-quality.cjs'))), runner_sha256:sha(read(__filename)), spec_sha256:sha(specBytes), directory:destination, executable, executable_sha256:executableHash, aggregate_cap_micros:aggregate, allocated_cap_micros:allocation*18, authorization:false, limitations:'Synthetic smoke only; no shipping defaults, statistical qualification, or evaluator claims.', strategies:{}, runs:[]};
  for (const id of ids) {
    const p = profiles[id];
    plan.strategies[id] = {reasons:p.reasons, profile_source:p.source ?? null, profile_sha256:p.source_sha256 ?? null, catalog:p.catalog ?? null, catalog_sha256:p.catalog_sha256 ?? null, provider:p.profile?.provider?.compatibility ?? null, policy_id:p.profile?.routing?.policy?.id ?? null};
  }
  for (const task of pool.cases) for (const strategy of ids) {
    const id = task.id + '--' + strategy, base = safeChild(destination,id);
    fs.mkdirSync(base, {mode:0o700}); fs.mkdirSync(path.join(base,'workspace')); fs.mkdirSync(path.join(base,'data'));
    const files = {};
    for (const [name, content] of Object.entries(task.files)) {
      const file = safeChild(path.join(base,'workspace'),name); fs.mkdirSync(path.dirname(file),{recursive:true}); write(file,content); files[name]=sha(Buffer.from(content));
    }
    write(path.join(base,'prompt.txt'),task.prompt);
    let profileHash = null;
    if (profiles[strategy].profile) {
      const derived = {...profiles[strategy].profile, workspace:path.join(base,'workspace'), catalog:profiles[strategy].catalog, budget_usd:usd(allocation), affected_paths:Object.keys(task.files)};
      write(path.join(base,'profile.json'),derived); profileHash=sha(read(path.join(base,'profile.json')));
    }
    plan.runs.push({id, case_id:task.id, strategy, start_state_sha256:pool.manifest.start_states[task.id], files, prompt_sha256:sha(Buffer.from(task.prompt)), profile_sha256:profileHash, cap_micros:allocation, status:profiles[strategy].reasons.length ? 'not_run' : 'ready', reasons:profiles[strategy].reasons});
  }
  write(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'), plan_sha256:sha(read(path.join(destination,'plan.json'))), eligible_runs:plan.runs.filter(r=>r.status==='ready').length, allocated_cap_micros:plan.allocated_cap_micros, authorization:false};
}
function invoke(executable,args,timeout) {
  const result = spawnSync(executable,args,{shell:false,windowsHide:true,encoding:'utf8',timeout,maxBuffer:16*1024*1024,stdio:['ignore','pipe','pipe']});
  return {status:result.status, stdout:result.stdout || '', stderr:result.stderr || '', error:result.error?.code ?? null};
}
function frames(text) { return text.trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line)); }
function inspection(plan,base,id,view,call,extra=[]) {
  const pages=[]; let cursor=null;
  do {
    const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'inspect',id,'--view',view,'--limit','128',...extra];
    if(cursor) args.push('--cursor',JSON.stringify(cursor));
    const result=call(plan.executable,args,30000);
    if(result.status!==0 || result.error) throw Error('Canonical inspection unavailable');
    const data=frames(result.stdout).find(f=>f.type==='result')?.data;
    if(!data || !Array.isArray(data.items) || !Array.isArray(data.gaps) || pages.length>=64) throw Error('Invalid or unbounded inspection');
    pages.push(data); cursor=data.next_cursor;
  } while(cursor);
  return pages;
}
function accounting(pages,cap) {
  const items=pages.flatMap(p=>p.items);
  if(pages.some(p=>p.gaps.length) || items.some(i=>i.visibility!=='available')) throw Error('Canonical cost evidence incomplete');
  const rows=type=>items.filter(i=>i.collection===type).map(i=>i.record);
  const ledgers=rows('ledger'), attempts=rows('attempt'), settlements=rows('settlement');
  if(ledgers.length!==1 || ledgers[0].currency!=='USD' || counter(ledgers[0].cap)!==cap || ledgers[0].overrun || counter(ledgers[0].active)!==0 || counter(ledgers[0].unresolved)!==0) throw Error('Canonical ledger has unknown liability, overrun, or a different cap');
  if(attempts.some(a=>!['settled','released'].includes(a.phase) || a.uncertain || a.previous || a.role!=='main' || (a.phase==='settled' && !settlements.some(s=>s.attempt===a.id && s.applied && s.observation?.final_usage)))) throw Error('Unsettled, retried, helper or unsupported attempt accounting');
  const total=attempts.reduce((sum,a)=>sum+counter(a.charged),0);
  if(!Number.isSafeInteger(total) || total!==counter(ledgers[0].settled) || total>cap) throw Error('Cost totals do not reconcile');
  return {actual_cost_micros:total,attempts};
}
function responseAnswer(plan,base,pages,attempts,call) {
  const answers=[];
  for(const item of pages.flatMap(p=>p.items).filter(i=>i.collection==='artifact' && i.record?.spec?.channel==='response')) {
    const descriptor=item.record;
    const length=counter(descriptor.length);
    if(descriptor.state!=='complete' || length>1024*1024) throw Error('Response capture is incomplete or too large');
    const chunks=[];
    for(let offset=0;offset<length;offset+=65536) {
      const ranged=inspection(plan,base,item.id,'outputs',call,['--offset',String(offset),'--length','65536']);
      const row=ranged[0].items[0];
      if(ranged.some(p=>p.gaps.length) || row?.range?.start!==offset || !Array.isArray(row.bytes)) throw Error('Response range unavailable');
      chunks.push(Buffer.from(row.bytes));
    }
    const bytes=Buffer.concat(chunks);
    if(bytes.length!==length || sha(bytes)!==descriptor.sha256) throw Error('Response identity mismatch');
    write(path.join(base,`response-${sha(Buffer.from(item.id))}.sse`),bytes.toString('utf8'));
    for(const block of bytes.toString('utf8').split(/\r?\n\r?\n/)) {
      const data=block.split(/\r?\n/).filter(l=>l.startsWith('data:')).map(l=>l.slice(5).trimStart()).join('\n');
      if(!data || data==='[DONE]') continue;
      const event=JSON.parse(data);
      if(event.type!=='response.completed' || event.response?.status!=='completed') continue;
      const response=event.response;
      if(!attempts.some(a=>a.provider_request===response.id) || response.output?.some(o=>o.type==='function_call')) continue;
      const texts=(response.output||[]).flatMap(o=>(o.content||[]).filter(c=>c.type==='output_text').map(c=>c.text));
      if(texts.length) answers.push({answer:JSON.parse(texts.join('')),artifact:item.id,provider_request:response.id,served_model:response.model??null});
    }
  }
  if(answers.length!==1) throw Error('Expected exactly one unambiguous canonical JSON final answer');
  return answers[0];
}
function run(planFile,authorization,call=invoke) {
  const bytes=read(planFile); if(sha(bytes)!==authorization) throw Error('Explicit authorization must match the exact prepared plan hash');
  const plan=JSON.parse(bytes), pool=quality.load();
  noParentInstructions(plan.directory);
  privateDirectory(plan.directory);
  if(plan.schema!=='p6-live-plan/1' || plain(path.dirname(path.resolve(planFile)))!==plan.directory || plan.manifest_sha256!==pool.manifest_sha256 || plan.runner_sha256!==sha(read(__filename)) || plan.grader_sha256!==sha(read(path.join(__dirname,'p6-task-quality.cjs'))) || plan.executable_sha256!==sha(read(plan.executable,1024*1024*1024))) throw Error('Prepared execution identity changed');
  if(plan.runs.length!==18 || plan.runs.reduce((sum,r)=>sum+r.cap_micros,0)!==plan.allocated_cap_micros || plan.allocated_cap_micros>plan.aggregate_cap_micros) throw Error('Prepared cap allocation changed');
  // Validate every arm before creating the permanent execution claim.
  for(const strategy of Object.values(plan.strategies)) if(strategy.profile_source) {
    if(sha(read(strategy.profile_source))!==strategy.profile_sha256 || sha(read(strategy.catalog))!==strategy.catalog_sha256) throw Error('External profile or catalog changed; prepare a new trial');
  }
  for(const row of plan.runs) {
    const base=safeChild(plan.directory,row.id);
    if(JSON.stringify(filesUnder(path.join(base,'workspace')))!==JSON.stringify(Object.keys(row.files).sort())) throw Error('Frozen project file set changed');
    if(sha(read(path.join(base,'prompt.txt')))!==row.prompt_sha256 || Object.entries(row.files).some(([name,digest])=>sha(read(safeChild(path.join(base,'workspace'),name)))!==digest)) throw Error('Frozen prompt or project file changed');
    if(fs.readdirSync(path.join(base,'data')).length) throw Error('Trial data directory is not fresh');
    if(row.profile_sha256 && sha(read(path.join(base,'profile.json')))!==row.profile_sha256) throw Error('Derived profile changed');
    if(row.status==='ready' && profileReasons(JSON.parse(read(path.join(base,'profile.json'))),row.strategy).length) throw Error('Profile qualification or deadline expired; prepare again');
  }
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,claimed_at:new Date().toISOString(),meaning:'One shot; crash or interruption requires inspection, never replay this plan.'});
  const result={schema:'p6-live-result/1',plan_sha256:authorization,manifest_sha256:pool.manifest_sha256,authorization:true,shipping_qualification:'insufficient_smoke_corpus',runs:[],actual_cost_micros:0,stopped:false};
  for(const row of plan.runs) {
    const report={case_id:row.case_id,strategy:row.strategy,start_state_sha256:row.start_state_sha256,status:'not_run',answer:null,actual_cost_micros:null,latency_ms:null,reasons:row.reasons};
    result.runs.push(report);
    if(row.status!=='ready' || result.stopped) continue;
    const base=safeChild(plan.directory,row.id), profile=JSON.parse(read(path.join(base,'profile.json')));
    const args=['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy','plan'];
    write(path.join(base,'attempted.json'),{plan_sha256:authorization,args,started_at:new Date().toISOString()});
    const start=Date.now(); const execution=call(plan.executable,args,(profile.deadline_seconds+180)*1000); report.latency_ms=Date.now()-start;
    write(path.join(base,'stdout.jsonl'),execution.stdout); write(path.join(base,'stderr.txt'),execution.stderr);
    try {
      if(execution.error) throw Error('CLI interrupted or output bound exceeded; liability requires inspection');
      const output=frames(execution.stdout), accepted=output.find(f=>f.type==='accepted'), final=output.findLast(f=>f.type==='result');
      if(!accepted?.scope?.task || !final?.conditions) throw Error('Missing durable accepted/result identity');
      report.scope=accepted.scope; report.status=final.conditions.completed?'completed':final.conditions.cancelled?'cancelled':'failed';
      const evidence={};
      for(const view of ['costs','routing','outputs']) { evidence[view]=inspection(plan,base,accepted.scope.task,view,call); write(path.join(base,view+'.json'),evidence[view]); }
      const money=accounting(evidence.costs,row.cap_micros); report.actual_cost_micros=money.actual_cost_micros;
      result.actual_cost_micros+=money.actual_cost_micros;
      if(report.status==='completed') {
        try { const answer=responseAnswer(plan,base,evidence.outputs,money.attempts,call); report.answer=answer.answer; report.answer_source=answer; }
        catch { report.status='failed'; report.reasons=['canonical final answer missing, ambiguous, or invalid JSON']; }
      }
    } catch(error) { report.status='failed'; report.reasons=[error.message]; result.actual_cost_micros=null; result.stopped=true; }
    write(path.join(base,'result.json'),report);
  }
  const submission={revision:pool.manifest.revision,manifest_sha256:pool.manifest_sha256,runs:result.runs.map(({case_id,strategy,start_state_sha256,status,answer})=>({case_id,strategy,start_state_sha256,status,answer}))};
  write(path.join(plan.directory,'answers.json'),submission);
  // The answer grader intentionally retains its narrower, unverified-answer label.
  write(path.join(plan.directory,'answer-grades.json'),quality.grade(submission,pool));
  if(result.runs.some(r=>r.status==='not_run')) result.actual_cost_micros=null;
  write(path.join(plan.directory,'result.json'),result); return result;
}
module.exports={prepare,run,profileReasons,accounting,micros};
if(require.main===module) {
  try {
    const [command,input,extra,...rest]=process.argv.slice(2);
    if(rest.length || !input || !extra || !['prepare','run'].includes(command)) throw Error('Usage: node scripts/evals/p6-live-runner.cjs prepare <spec.json> <new-private-directory> | run <plan.json> <authorized-plan-sha256>');
    const result=command==='prepare'?prepare(input,extra):run(input,extra);
    process.stdout.write(json(command==='prepare'?result:{result:path.join(path.dirname(input),'result.json'),stopped:result.stopped,actual_cost_micros:result.actual_cost_micros}));
  } catch(error) { console.error(error.message); process.exitCode=1; }
}
