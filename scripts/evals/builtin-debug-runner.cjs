// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{spawnSync}=require('node:child_process');
const prior=require('./p6-live-runner.cjs'),paired=require('./builtin-live-runner.cjs'),prep=require('./builtin-debug-prepare.cjs'),generation=require('./builtin-generation-prepare.cjs');
const generationRunner=require('./builtin-generation-runner.cjs');
const {plain,read,write,safeChild,noParentInstructions,privateDirectory,usd,frames,inspection,invoke}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function validate(plan,file){
  noParentInstructions(plan.directory);privateDirectory(plan.directory);
  if(plan.schema!=='p7-cr06-debug-preparation/1'||!plan.runnable||plan.blockers.length||!plan.runtime||plain(path.dirname(path.resolve(file)))!==plan.directory)throw Error('Runnable qualified debug plan required');
  for(const [field,name]of Object.entries(prep.bindings))if(plan[field]!==sha(read(path.join(__dirname,name))))throw Error('Prepared debug source changed');
  const runtimeInput={node:plan.runtime.node,launcher:plan.runtime.launcher,build_receipt:plan.runtime.build_receipt};
  if(JSON.stringify(prep.qualifyRuntime(runtimeInput))!==JSON.stringify(plan.runtime))throw Error('Qualified debug runtime changed');
  const specBytes=read(plan.spec_source),spec=JSON.parse(specBytes);
  if(sha(specBytes)!==plan.spec_sha256||spec.propose_opaque_launcher_effects!==true||JSON.stringify(plan.permission_review)!==JSON.stringify(prep.permissionReview(plan.runtime,true)))throw Error('Exact debug permission proposal changed');
  if(plain(path.resolve(spec.executable))!==plan.executable||plain(path.resolve(spec.profile))!==plan.profile_source||prior.micros(spec.aggregate_cap_usd)!==plan.aggregate_cap_micros||JSON.stringify(prep.qualifyRuntime(spec.runtime))!==JSON.stringify(plan.runtime))throw Error('Prepared debug plan differs from exact owner spec');
  const selected=prep.pool();
  if(plan.fixture_sha256!==selected.sha256||plan.fixture_revision!==selected.manifest.revision||plan.executable_sha256!==sha(read(plan.executable,1024*1024*1024))||plan.profile_sha256!==sha(read(plan.profile_source))||plan.catalog_sha256!==sha(read(plan.catalog)))throw Error('Prepared debug fixture, executable or provider changed');
  if(JSON.stringify(generation.inventory(path.join(path.dirname(plan.executable),'skills/builtin')))!==JSON.stringify(plan.assets)||JSON.stringify(generation.inventory(path.join(repo,'src/skills/builtin')))!==JSON.stringify(plan.assets))throw Error('Packaged skills changed');
  const source=JSON.parse(read(plan.profile_source));if(prep.sourceReasons(source).length)throw Error('Source profile qualification expired or changed');
  if(plain(path.resolve(source.catalog))!==plan.catalog)throw Error('Source provider catalog changed');
  const allocation=Math.floor(plan.aggregate_cap_micros/4);if(!Number.isSafeInteger(plan.aggregate_cap_micros)||allocation<1||plan.allocated_cap_micros!==allocation*4||plan.runs.length!==4)throw Error('Four-scenario cap allocation changed');
  for(const [index,scenario]of selected.manifest.cases.entries()){
    const row=plan.runs[index],base=safeChild(plan.directory,scenario.id),workspace=path.join(base,'workspace'),files=prep.filesFor(selected.manifest,scenario);
    if(row.id!==scenario.id||row.arm!=='skill'||row.skill!=='review-debug'||row.reproduction!==scenario.reproduction||row.cap_micros!==allocation||JSON.stringify(row.files)!==JSON.stringify(files)||JSON.stringify(row.editable)!=='["shipping.cjs"]')throw Error('Frozen debug cohort changed');
    if(JSON.stringify(generation.inventory(workspace))!==JSON.stringify(files)||fs.readdirSync(path.join(base,'data')).length||row.prompt_sha256!==sha(Buffer.from(prep.promptFor(scenario)))||sha(read(path.join(base,'prompt.txt')))!==row.prompt_sha256)throw Error('Prepared debug workspace, prompt or fresh store changed');
    const profileBytes=read(path.join(base,'profile.json'));
    if(sha(profileBytes)!==row.profile_sha256||JSON.stringify(JSON.parse(profileBytes))!==JSON.stringify(prep.qualifiedProfile(source,workspace,plan.catalog,allocation,plan.runtime,true,scenario)))throw Error('Exact debug process authority changed');
  }
}
function capturedJson(plan,base,item,call){
  const descriptor=item.record,length=Number(descriptor.length);if(descriptor.state!=='complete'||!Number.isSafeInteger(length)||length<1||length>1024*1024)throw Error('Incomplete bounded debug receipt');
  const chunks=[];
  for(let offset=0;offset<length;offset+=65536){
    const pages=inspection(plan,base,item.id,'context',call,['--offset',String(offset),'--length','65536']),value=pages[0]?.items[0],end=Math.min(offset+65536,length);
    if(pages.length!==1||pages[0].items.length!==1||pages.some(p=>p.gaps.some(g=>!prior.privacyGap(g,item.id)))||value?.range?.start!==offset||value.range.end!==end||(value.artifact!==undefined&&value.artifact!==item.id)||(value.visibility!==undefined&&value.visibility!=='available')||!Array.isArray(value.bytes)||value.bytes.length!==end-offset||value.bytes.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('Debug receipt range unavailable');
    chunks.push(Buffer.from(value.bytes));
  }
  const bytes=Buffer.concat(chunks);if(sha(bytes)!==descriptor.sha256)throw Error('Debug receipt digest mismatch');return JSON.parse(bytes);
}
function canonical(value){return JSON.stringify(value,(_,item)=>item&&typeof item==='object'&&!Array.isArray(item)?Object.fromEntries(Object.keys(item).sort().map(key=>[key,item[key]])):item);}
function fileOnlyEvidence(items,effects,task,retained){
  if(items.some(i=>['effect','artifact'].includes(i.collection)&&(i.visibility!=='available'||!i.record||i.record.redaction)))throw Error('Missing-access execution evidence unavailable or redacted');
  const artifacts=items.filter(i=>i.collection==='artifact');
  if(artifacts.some(i=>/^vcp-process-/.test(i.record.spec?.schema)))throw Error('Missing-access case executed or started a native process');
  const matched=[];
  for(const effect of effects){
    if(effect.scope?.task!==task||effect.state!=='succeeded'||!effect.execution||effect.exit_code!==null)throw Error('Missing-access effect is not a settled local file operation');
    const linked=schema=>artifacts.filter(i=>i.record.spec?.schema===schema&&i.record.state==='complete'&&canonical(i.record.spec.scope)===canonical(effect.scope)&&effect.observed_changes?.includes(i.id)).map(i=>{
      const capture=retained.find(c=>c.artifact===i.id&&canonical(c.scope)===canonical(effect.scope));
      if(!capture)throw Error('Missing-access local file receipt unavailable');
      return capture;
    });
    const plans=linked('vcp-prepared-tool-v2');
    if(plans.length!==1)throw Error('Missing-access effect has no unique prepared local file operation');
    const prepared=plans[0].receipt.prepared,operation=prepared?.operation;
    if(plans[0].receipt.schema!=='vcp-prepared-tool/2'||!operation||sha(Buffer.from(canonical(operation)))!==effect.operation_digest||canonical(operation.scope)!==canonical(effect.scope)||operation.invocation?.kind!=='local'||!['vcp_read','vcp_list','vcp_search','vcp_patch'].includes(operation.tool)||!Array.isArray(operation.effects)||!operation.effects.length||operation.effects.some(e=>!['read','write'].includes(e)))throw Error('Missing-access effect is not a qualified local file operation');
    const results=linked('vcp-tool-result-v1');
    // Read/list/search completeness describes bounded coverage, not dispatch kind.
    if(results.length!==1||typeof results[0].receipt.complete!=='boolean'||operation.tool==='vcp_patch'&&results[0].receipt.complete!==true||!Array.isArray(prepared.changes))throw Error('Missing-access local file result incomplete');
    const outcomes=linked('vcp-file-outcome-v1');
    if(outcomes.length!==prepared.changes.length||outcomes.some(c=>c.receipt.effect!==effect.id||c.receipt.execution!==effect.execution||c.receipt.observation?.complete!==true||c.receipt.observation.error!==null))throw Error('Missing-access file mutation receipt incomplete');
    if(operation.effects.includes('write')&&(!prepared.changes.length||operation.tool!=='vcp_patch'))throw Error('Missing-access write lacks classified file mutation');
    matched.push({effect:effect.id,execution:effect.execution,tool:operation.tool,plan:plans[0].artifact,result:results[0].artifact,outcomes:outcomes.map(c=>c.artifact)});
  }
  return matched;
}
function processEvidence(pages,captures,row,task,currentHash,retained=[]){
  if(pages.some(p=>p.gaps.some(g=>!prior.privacyGap(g,g.artifact))))throw Error('Canonical debug tool evidence incomplete');
  const effects=pages.flatMap(p=>p.items).filter(i=>i.collection==='effect'&&i.visibility==='available').map(i=>i.record);
  if(row.reproduction==='unavailable'){
    if(captures.length)throw Error('Missing-access case executed a native process');
    const file_effects=fileOnlyEvidence(pages.flatMap(p=>p.items),effects,task,retained);
    return {status:'not_run',reason:'required reproduction environment and execution grant unavailable',verification_complete:false,file_effects};
  }
  const receipts=captures.filter(({artifact,scope,receipt:r})=>{
    const e=effects.find(e=>e.id===r.effect&&e.execution===r.execution&&e.scope?.task===task&&!e.redaction&&e.observed_changes?.includes(artifact));
    return scope?.task===task&&e&&e.exit_code===r.exit_code&&e.state===(r.exit_code===0?'succeeded':'failed')&&r.output_complete===true&&r.stop_reason===null&&r.owned_processes_remaining===0&&r.observed_workspace?.complete===true&&r.presentation?.stdout?.omitted_bytes===0&&r.presentation.stdout.replacement_characters===0;
  });
  const sourceHash=r=>r.observed_workspace.sources?.find(s=>s.path==='shipping.cjs')?.sha256;
  const original=receipts.find(({receipt:r})=>r.exit_code===1&&sourceHash(r)===row.files['shipping.cjs']&&r.presentation.stdout.tail.includes('not ok 1 - shipping fee threshold includes 50'));
  const current=receipts.find(({receipt:r})=>r.exit_code===0&&sourceHash(r)===currentHash&&r.presentation.stdout.tail.includes('ok 1 - shipping fee threshold includes 50'));
  if(!original||!current||original.artifact===current.artifact)throw Error('Missing canonical pre-fix failure and current-source passing launcher receipts');
  return {status:'passed',initial:original,current,verification_complete:true};
}
function verificationEvidence(pages,task,available){
  if(available)return {records:generationRunner.verificationEvidence(pages,task),kind:'canonical current-source executable threshold check'};
  if(pages.some(p=>p.gaps.length))throw Error('Canonical verification has gaps');
  const rows=pages.flatMap(p=>p.items).filter(i=>i.collection==='verification'&&i.visibility==='available'&&i.record.scope?.task===task&&!i.record.redaction);
  if(!rows.length)throw Error('No canonical final vcp_verify record');
  if(rows.some(i=>i.record.checks?.some(check=>check.outcome?.status==='passed')))throw Error('Missing-access verification unexpectedly claims an executable pass');
  return {records:rows.map(i=>i.id),kind:'incomplete_modified_source_verification; execution not authorized'};
}
function run(file,authorization,call=invoke){
  const bytes=read(file);if(sha(bytes)!==authorization)throw Error('Authorization must name exact prepared plan hash');const plan=JSON.parse(bytes);validate(plan,file);
  write(path.join(plan.directory,'execution-claim.json'),{plan_sha256:authorization,at:new Date().toISOString(),meaning:'One shot; inspect interrupted liability, never replay'});
  const result={schema:'p7-cr06-debug-result/1',plan_sha256:authorization,actual_cost_micros:0,stopped:false,live_usefulness:'pending_independent_review',runs:[]};
  for(const row of plan.runs){
    const report={id:row.id,status:'not_run',actual_cost_micros:null,live_usefulness:'not_run'};result.runs.push(report);if(result.stopped)continue;
    const base=safeChild(plan.directory,row.id),workspace=path.join(base,'workspace'),profile=JSON.parse(read(path.join(base,'profile.json')));
    const args=['--format','jsonl','--non-interactive','--workspace',workspace,'--data-dir',path.join(base,'data'),'--config',path.join(base,'profile.json'),'run','--file',path.join(base,'prompt.txt'),'--budget-usd',usd(row.cap_micros),'--autonomy',profile.maximum_autonomy,'--skill','vcp-builtin::review-debug::review-debug'];
    write(path.join(base,'attempted.json'),{plan_sha256:authorization,args,at:new Date().toISOString()});const start=Date.now();
    try{
      const execution=call(plan.executable,args,(profile.deadline_seconds+180)*1000);report.latency_ms=Date.now()-start;write(path.join(base,'stdout.jsonl'),execution.stdout);write(path.join(base,'stderr.txt'),execution.stderr);
      if(execution.error)throw Error('CLI interrupted; reconcile liability before another trial');
      const output=frames(execution.stdout),accepted=output.find(f=>f.type==='accepted'),final=output.findLast(f=>f.type==='result');if(!accepted?.scope?.task||!final?.conditions)throw Error('Durable task result missing');report.scope=accepted.scope;
      const evidence={};for(const view of ['costs','routing','outputs','context','verification','tools']){evidence[view]=inspection(plan,base,accepted.scope.task,view,call);write(path.join(base,view+'.json'),evidence[view]);}
      const money=prior.accounting(evidence.costs,row.cap_micros);report.actual_cost_micros=money.actual_cost_micros;result.actual_cost_micros+=money.actual_cost_micros;
      report.skill_evidence=paired.skillEvidence(plan,base,row,evidence.context,money.attempts,call);report.status='failed';
      const captures=evidence.tools.flatMap(p=>p.items).filter(i=>i.collection==='artifact'&&i.record?.spec?.schema==='vcp-process-outcome-v1').map(item=>({artifact:item.id,scope:item.record.spec.scope,receipt:capturedJson(plan,base,item,call)}));write(path.join(base,'process-receipts.json'),captures);
      const missing=row.reproduction==='unavailable';
      if(missing||execution.status===0&&final.conditions.completed===true){
        report.verification=verificationEvidence(evidence.verification,accepted.scope.task,!missing);
        const retained=missing?evidence.tools.flatMap(p=>p.items).filter(i=>i.collection==='artifact'&&['vcp-prepared-tool-v2','vcp-tool-result-v1','vcp-file-outcome-v1'].includes(i.record?.spec?.schema)).map(item=>({artifact:item.id,scope:item.record.spec.scope,receipt:capturedJson(plan,base,item,call)})):[];
        report.native_checks=processEvidence(evidence.tools,captures,row,accepted.scope.task,sha(read(path.join(workspace,'shipping.cjs'))),retained);
        const observed=spawnSync(plan.runtime.node,[path.join(__dirname,'builtin-debug-v2-oracle.cjs'),'grade',row.id,workspace],{env:process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{},encoding:'utf8',timeout:15000,maxBuffer:65536,windowsHide:true});
        if(observed.error||![0,1].includes(observed.status))throw Error('Independent debug oracle unavailable');
        report.oracle=JSON.parse(observed.stdout);report.live_usefulness='pending_independent_review';report.cli_completed=final.conditions.completed===true;
        if(observed.status===0&&report.oracle.controls_pass===true){
          if(missing){
            if(report.cli_completed||sha(read(path.join(workspace,'shipping.cjs')))===row.files['shipping.cjs'])throw Error('Missing-access trial must preserve modified-source incomplete acceptance');
            report.status='preservation_passed_verification_not_run';
          }else report.status='controls_passed';
        }
      }else report.reason='CLI task did not complete; oracle cannot override';
    }catch(error){report.status='failed';report.reason=error.message;result.actual_cost_micros=null;result.stopped=true;}
    write(path.join(base,'result.json'),report);
  }
  write(path.join(plan.directory,'result.json'),result);return result;
}
module.exports={validate,run,capturedJson,processEvidence,verificationEvidence};
if(require.main===module){try{const [command,file,authorization,...rest]=process.argv.slice(2);if(command!=='run'||!file||!authorization||rest.length)throw Error('Usage: builtin-debug-runner.cjs run <plan.json> <authorized-plan-sha256>');const result=run(file,authorization);console.log(JSON.stringify(result));if(result.stopped||result.runs.some(r=>!['controls_passed','preservation_passed_verification_not_run'].includes(r.status)))process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
