// SPDX-License-Identifier: Apache-2.0
'use strict';
// Zero-provider observation and retention controls on disposable data-root copies.
// Original owner workspaces are read only and inventoried before/after every run.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const repo=path.resolve(__dirname,'../..');
const prior=require(path.join(repo,'scripts/evals/p6-live-runner.cjs'));
const owner=require(path.join(repo,'scripts/evals/p805-owner-runner.cjs'));
const inventory=require(path.join(repo,'scripts/package-inventory.cjs'));
const {plain,read,write,privateDirectory,noParentInstructions,frames}=prior.boundaries;
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const hash=f=>sha(read(plain(f),1024*1024*1024));
const parse=f=>JSON.parse(read(f,256*1024*1024));
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const targets=a=>a.map(x=>x.kind+':'+x.id).sort();
const oracleFile=path.join(__dirname,'p805-owner-state-oracle.py');
function files(root){return prior.boundaries.filesUnder(root).map(relative=>{const file=path.join(root,relative);return {path:relative,bytes:fs.statSync(file).size,sha256:hash(file)};});}
function workspaceDescriptor(data){const found=files(data).filter(r=>r.path.startsWith('workspaces/')&&r.path.endsWith('/workspace.json'));if(found.length!==1)throw Error('Exactly one retained workspace descriptor required');return found[0].path;}
function clone(row,destination){
  plain(row.data);plain(row.workspace);plain(destination);if(fs.existsSync(destination))throw Error('Disposable destination must not exist');
  const before=files(row.data);fs.cpSync(row.data,destination,{recursive:true,errorOnExist:true,force:false});
  if(!same(before,files(destination)))throw Error('Copied canonical source differs');
  const relative=workspaceDescriptor(destination),file=path.join(destination,relative),descriptor=parse(file);
  if(descriptor.config.backend!==row.backend||descriptor.config.root_task!==row.task||descriptor.rebind_pending)throw Error('Owner task/backend descriptor mismatch');
  const canonical=path.join(path.dirname(file),'canonical');descriptor.config.canonical_root='\\\\?\\'+path.resolve(canonical);
  fs.writeFileSync(file,JSON.stringify(descriptor,null,2)+'\n');return {data:destination,canonical,workspace_id:descriptor.config.workspace};
}
function retentionExpected(oracle,action){return {selected:oracle.selected,dependent:oracle.dependent,protected:action==='purge'?oracle.protected_if_purge:[]};}
function assertPreview(expected,entries){
  for(const kind of ['selected','dependent','protected']){
    const actual=entries.filter(e=>e.class===kind).map(e=>e.target);
    const actualIds=kind==='protected'?[...new Set(targets(actual))]:targets(actual);
    if(!same(targets(expected[kind]),actualIds))throw Error('Independent '+kind+' ID oracle differs from preview');
  }
}
function assertRefusal(result,diagnostic){
  if(result.status!==2||!result.stderr.includes(diagnostic))throw Error('Expected retention refusal diagnostic differs');
}

async function run(specFile,destination,python){
  const spec=parse(specFile);destination=plain(path.resolve(destination));python=plain(path.resolve(python));
  if(spec.schema!=='p805-integrated-history-spec/1'||!Array.isArray(spec.rows)||!spec.rows.length||spec.rows.length>6)throw Error('Bounded declared owner rows required');
  if(spec.owner_plan&&(hash(spec.owner_plan)!==spec.owner_plan_sha256||hash(spec.owner_result)!==spec.owner_result_sha256))throw Error('Owner plan/result evidence changed');
  for(const row of spec.rows)for(const original of [row.data,row.workspace]){
    const source=plain(path.resolve(original));
    if(prior.boundaries.within(source,destination)||prior.boundaries.within(destination,source))throw Error('Evidence destination must be disjoint from every original data/workspace root');
  }
  privateDirectory(destination);noParentInstructions(path.dirname(destination));if(fs.existsSync(destination))throw Error('New private evidence root required');
  const pkg=parse(spec.package_result),packageRoot=path.join(path.dirname(spec.package_result),'package'),executable=path.join(packageRoot,'vcp.exe');
  inventory.verifyManifest(packageRoot,pkg.manifest);
  const exeRow=pkg.manifest.files.filter(r=>r.path==='vcp.exe');
  if(exeRow.length!==1||hash(executable)!==exeRow[0].sha256||hash(path.join(path.dirname(spec.package_result),pkg.package))!==pkg.archive_sha256)throw Error('Exact production package required');
  const build=parse(path.join(packageRoot,'build-receipt.json'));if(build.profile!=='release'||build.qualification_build!==false||build.executable_sha256!==exeRow[0].sha256)throw Error('Production artifact required');
  if(new Set(spec.rows.map(r=>r.id)).size!==spec.rows.length||spec.rows.some(r=>!/^[-a-z0-9]+$/.test(r.id)))throw Error('Unique simple row IDs required');
  fs.mkdirSync(destination,{mode:0o700});
  const report={schema:'p805-integrated-history-result/1',status:'running',purpose:spec.purpose,model_calls:0,package_sha256:pkg.archive_sha256,executable_sha256:exeRow[0].sha256,spec_sha256:hash(specFile),runner_sha256:hash(__filename),oracle_sha256:hash(oracleFile),python_sha256:hash(python),rows:[],limitations:['Current-host disposable copies of actual owner roots; no machine handoff or clean-OS claim.','Whole-task retention oracle supports only fresh single-root history without prior redactions or complex memory/advisory lineage.','No 30-day boundary, cleanup kill, physical exhaustion, retained-cloud-copy deletion or active provider/delegated pause qualification.','Optimizer/skill inspection is not live routing, optimization policy apply/rollback, skill activation or MCP invocation.','No human quality judgment or final acceptance is recorded.']};
  const secret=process.env.OPENROUTER_API_KEY;const env={...process.env};delete env.OPENROUTER_API_KEY;
  let commandIndex=0;
  function capture(file,value){const text=typeof value==='string'?value:JSON.stringify(value,null,2)+'\n';if(secret&&text.includes(secret))throw Error('Credential occurrence detected in observation; raw payload not written');write(file,text);}
  const save=()=>fs.writeFileSync(path.join(destination,'result.json'),JSON.stringify(report,null,2)+'\n');save();
  async function invoke(base,label,program,args,expectedExit=0){
    const began=Date.now(),result=await owner.bounded(program,args,120000,env),name=String(++commandIndex).padStart(4,'0')+'-'+label;
    capture(path.join(base,name+'.json'),{program,arguments:args,elapsed_ms:Date.now()-began,...result});
    if(result.error||!result.process_reaped){const failure=Error('Bounded process/pipe supervision failed: '+label);failure.supervision=true;throw failure;}
    if(expectedExit===0&&result.status!==0)throw Error('Command failed: '+label);
    if(expectedExit==='nonzero'&&(!Number.isInteger(result.status)||result.status<=0))throw Error('Expected normal nonzero command refusal: '+label);
    return result;
  }
  async function cli(base,row,copy,label,args,expected=0){
    if(!['history','inspect','prune','retention','memory','optimize','skills'].includes(args[0]))throw Error('Command is outside zero-provider allowlist');
    const configuration=args[0]==='skills'&&row.profile?['--config',row.profile]:[];
    if(configuration.length&&hash(row.profile)!==row.profile_sha256)throw Error('Owner skill-inspection profile changed');
    const result=await invoke(base,label,executable,['--format','jsonl','--non-interactive','--workspace',row.workspace,'--data-dir',copy.data,...configuration,...args],expected);
    if(expected==='nonzero')return result;
    const output=frames(result.stdout).filter(r=>r.type==='result');if(output.length!==1||output[0].exit_code!==0||!output[0].data)throw Error('Single successful structured result required: '+label);return output[0].data;
  }
  async function state(base,row,copy,label){const output=path.join(base,label+'.json');await invoke(base,label,python,[oracleFile,copy.canonical,row.backend,row.task,output]);return parse(output);}
  async function history(base,row,copy,label,extra=[]){
    const pages=[];let cursor=null;const seen=new Set();
    do{if(pages.length>=128)throw Error('History pagination exceeds declared bound');const args=['history','list','--task',row.task,'--limit','32',...extra];if(cursor)args.push('--cursor',JSON.stringify(cursor));const page=await cli(base,row,copy,label+'-'+pages.length,args);if(!Array.isArray(page.rows)||!Array.isArray(page.gaps)||page.workspace!==copy.workspace_id)throw Error('History scope/shape mismatch');pages.push(page);cursor=page.next_cursor;if(cursor){const key=JSON.stringify(cursor);if(seen.has(key))throw Error('Repeated history cursor');seen.add(key);}}while(cursor);return pages;
  }
  async function preview(base,row,copy,action){
    const first=await cli(base,row,copy,'preview-'+action,['history','prune','--task',row.task,'--preview','--action',action]);
    if(!/^[a-f0-9]{64}$/.test(first.id))throw Error('Saved preview identity missing');
    const entries=[];let offset=0;
    do{const page=await cli(base,row,copy,'preview-page-'+action+'-'+offset,['prune','show',first.id,'--offset',String(offset),'--limit','128']);if(page.preview!==first.id||page.offset!==offset||!Array.isArray(page.entries)||entries.length>8192*3)throw Error('Preview page mismatch');entries.push(...page.entries);offset=page.next_offset;if(offset!==null&&offset!==undefined&&offset!==entries.length)throw Error('Preview offset mismatch');}while(offset!==null&&offset!==undefined);
    return {first,entries};
  }
  async function fullOutput(base,row,copy,artifact){
    const length=Number(artifact.length);if(artifact.state!=='complete'||!Number.isSafeInteger(length)||length>8*1024*1024)throw Error('Output is outside complete bounded capture contract');
    const chunks=[];
    for(let offset=0;offset<length;offset+=65536){const page=await cli(base,row,copy,'raw-'+artifact.spec.id+'-'+offset,['inspect',artifact.spec.id,'--view','outputs','--offset',String(offset),'--length','65536']);const item=page.items?.[0],end=Math.min(length,offset+65536);if(page.items?.length!==1||page.next_cursor||page.gaps?.some(g=>!prior.privacyGap(g,artifact.spec.id))||Number(item?.range?.start)!==offset||Number(item?.range?.end)!==end||item.bytes?.length!==end-offset||item.bytes.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('Full raw artifact has gaps');chunks.push(Buffer.from(item.bytes));}
    const bytes=Buffer.concat(chunks);if(bytes.length!==length||sha(bytes)!==artifact.sha256)throw Error('Independent artifact digest mismatch');
    if(secret&&bytes.includes(Buffer.from(secret)))throw Error('Credential occurrence in raw artifact');
    const file=path.join(base,'raw-'+artifact.spec.id+'.bin');fs.writeFileSync(file,bytes,{flag:'wx',mode:0o600});return {id:artifact.spec.id,bytes:length,sha256:sha(bytes),channel:artifact.spec.channel,larger_than_history_summary:length>8192};
  }
  for(const row of spec.rows){
    const base=path.join(destination,row.id);fs.mkdirSync(base);const result={id:row.id,task:row.task,backend:row.backend,status:'running',checks:[],limitations:[]};report.rows.push(result);
    let originalData,originalWorkspace;
    try{
      originalData=files(row.data);originalWorkspace=files(row.workspace);capture(path.join(base,'original-data-before.json'),originalData);capture(path.join(base,'original-workspace-before.json'),originalWorkspace);
      const seed=clone(row,path.join(base,'read-data'));
      const before=await state(base,row,seed,'independent-before');capture(path.join(base,'predeclared-oracle.json'),before.retention_oracle);
      const expectedIds=before.state.events.filter(e=>e.event.task===row.task).map(e=>e.event.id).sort();
      if(!expectedIds.length)throw Error('No actual owner events');
      const policy=await cli(base,row,seed,'retention-default',['retention','show']);if(policy.automatic!==null)throw Error('Fresh owner retention is not notification-only');
      const pages=await history(base,row,seed,'full-history',['--expand-compacted']);
      const actualIds=pages.flatMap(p=>p.rows.map(r=>r.event.event.id)).sort();if(pages.some(p=>p.gaps.length)||!same(actualIds,expectedIds))throw Error('History event ID oracle mismatch or gaps');
      result.checks.push({name:'history-pagination-independent-event-ids',status:'passed',events:expectedIds.length,pages:pages.length});
      const artifacts=Object.values(before.state.records).filter(r=>r.collection==='artifact'&&r.value.spec.scope.task===row.task&&['response','stdout','stderr'].includes(r.value.spec.channel)).map(r=>r.value);
      if(artifacts.length>256||artifacts.reduce((sum,a)=>sum+Number(a.length),0)>32*1024*1024)throw Error('Raw outputs exceed declared aggregate bound');
      result.full_outputs=[];
      for(const artifact of artifacts){if(artifact.state==='complete')result.full_outputs.push(await fullOutput(base,row,seed,artifact));else result.limitations.push('Incomplete capture retained as '+artifact.state+': '+artifact.spec.id);}
      const after=await state(base,row,seed,'independent-after-read');
      if(!same(before.state,after.state))throw Error('Read-only history/default retention changed canonical state');
      result.checks.push({name:'default-no-delete-and-full-output-digests',status:'passed',complete_outputs:result.full_outputs.length});
      // Inspection stays separate from retention copies and never changes paid fixture profiles.
      result.optimizer=[];
      for(const operation of ['status','observations','transitions','cycles','forecasts']){
        try{const data=await cli(base,row,seed,'optimize-'+operation,['optimize',operation]);result.optimizer.push({operation,status:'observed',data_file:commandIndex,local_workflow:data.local_workflow??null});}catch(error){if(error.supervision)throw error;result.optimizer.push({operation,status:'failed',reason:error.message});}
      }
      try{const data=await cli(base,row,seed,'skills-list',['skills','list']);result.skills={status:'observed',configured:data.configured??null,meaning:'Read-only configured descriptor/setup observation; no activation or invocation'};}catch(error){if(error.supervision)throw error;result.skills={status:'failed',reason:error.message};}
      const oracle=before.retention_oracle;
      if(!oracle.supported){result.retention={status:'not_run',reasons:oracle.not_run_reasons};result.status='read-only-observations-passed-retention-not-run';continue;}
      result.retention={status:'running',cases:[]};
      for(const action of ['exclude','compact','purge']){
        const copy=clone(row,path.join(base,action+'-data')),expected=retentionExpected(oracle,action);
        capture(path.join(base,action+'-expected.json'),expected);
        const planned=await preview(base,row,copy,action);assertPreview(expected,planned.entries);
        const item={action,preview:planned.first.id,selected:expected.selected.length,dependent:0,protected:expected.protected.length,status:'running'};result.retention.cases.push(item);
        if(expected.protected.length){
          const refusal=await cli(base,row,copy,'protected-purge-refusal',['prune','apply',planned.first.id],'nonzero');
          assertRefusal(refusal,'reconcile protected dependency closure before purge');
          const unchanged=await history(base,row,copy,'protected-retained',['--expand-compacted']);
          if(!expectedIds.every(id=>unchanged.some(p=>p.rows.some(r=>r.event.event.id===id))))throw Error('Protected purge lost original event');
          item.status='protected-refusal-passed';continue;
        }
        const receipt=await cli(base,row,copy,'apply-'+action,['prune','apply',planned.first.id]);
        if(receipt.preview?.id!==planned.first.id||receipt.preview.action!==action)throw Error('Applied receipt differs from exact preview');
        const post=await history(base,row,copy,'after-'+action,action==='compact'?[]:['--expand-compacted']);
        const retained=post.flatMap(p=>p.rows).filter(r=>expectedIds.includes(r.event.event.id));
        if(action==='purge'){
          if(retained.length!==0||!post.some(p=>p.gaps.length))throw Error('Purged history not represented by explicit gaps');
          const complete=artifacts.find(a=>a.state==='complete'&&Number(a.length)>0);
          if(complete){const unavailable=await cli(base,row,copy,'purged-output-gap',['inspect',complete.spec.id,'--view','outputs','--offset','0','--length','64']);if(!unavailable.gaps?.length||unavailable.items?.some(i=>i.bytes?.length))throw Error('Purged artifact bytes remain inspectable');}
          item.status='logical-purge-and-explicit-gap-passed';item.physical_cleanup='not-run';item.external_copies=receipt.backup_copies;
        }else{
          if(retained.length!==expectedIds.length||retained.some(r=>action==='exclude'?!r.recall_excluded:!r.compacted))throw Error('Expected history retention flags absent');
          if(action==='compact'){const expanded=await history(base,row,copy,'expanded-compact',['--expand-compacted']);if(!expectedIds.every(id=>expanded.some(p=>p.rows.some(r=>r.event.event.id===id))))throw Error('Compacted raw history no longer expands');}
          const complete=artifacts.find(a=>a.state==='complete');if(complete){const rawBase=path.join(base,action+'-raw');fs.mkdirSync(rawBase);await fullOutput(rawBase,row,copy,complete);}
          item.status='exact-preview-apply-and-retained-raw-passed';
        }
      }
      const staleCopy=clone(row,path.join(base,'stale-data'));
      const stale=await preview(base,row,staleCopy,'exclude');assertPreview(retentionExpected(oracle,'exclude'),stale.entries);
      await cli(base,row,staleCopy,'stale-mutate-policy',['retention','set','--notification-only']);
      const refused=await cli(base,row,staleCopy,'stale-apply-refusal',['prune','apply',stale.first.id],'nonzero');
      assertRefusal(refused,'stale preview; create a new preview');
      const staleState=await state(base,row,staleCopy,'independent-stale-refusal');
      if(Object.values(staleState.state.records).some(r=>r.value.document_type==='vcp_retention_decision_v1'||r.collection==='tombstone'))throw Error('Stale preview created deletion/recall decisions');
      if(!expectedIds.every(id=>staleState.state.events.some(e=>e.event.id===id&&!e.redaction)))throw Error('Stale refusal lost original event');
      result.retention.cases.push({action:'stale-preview',status:'refused-with-no-deletion-passed',exit_code:refused.status});
      result.retention.status='passed';result.status='bounded-integrated-controls-passed';
    }catch(error){result.status='failed';result.reason=String(error.message);if(error.supervision)report.stopped=true;}
    finally{
      try{const dataAfter=files(row.data),workspaceAfter=files(row.workspace);capture(path.join(base,'original-data-after.json'),dataAfter);capture(path.join(base,'original-workspace-after.json'),workspaceAfter);result.originals_preserved=same(originalData,dataAfter)&&same(originalWorkspace,workspaceAfter);if(!result.originals_preserved)result.status='failed';}catch(error){result.originals_preserved=false;result.preservation_error=String(error.message);result.status='failed';}
      save();
    }
    if(report.stopped)break;
  }
  if(hash(executable)!==report.executable_sha256||hash(__filename)!==report.runner_sha256||hash(oracleFile)!==report.oracle_sha256)throw Error('Artifact or harness changed during observations');
  report.status=report.rows.some(r=>r.status==='failed')?'failed':report.rows.some(r=>r.retention?.status==='not_run'||r.optimizer?.some(o=>o.status==='failed')||r.skills?.status==='failed')?'partial':'passed';save();return {directory:destination,status:report.status,rows:report.rows.length,model_calls:0};
}

function prepare(ownerPlanFile,ownerResultFile,packageResult,output){
  const plan=parse(ownerPlanFile),result=parse(ownerResultFile);
  if(result.plan_sha256!==hash(ownerPlanFile)||result.schema!=='p805-owner-result/1')throw Error('Exact owner result binding required');
  const pkg=parse(packageResult);if(pkg.archive_sha256!==plan.package_sha256)throw Error('Post-owner artifact changed');
  const rows=result.runs.filter(r=>r.scope?.task&&r.status!=='running').map(r=>{const source=plan.runs.find(s=>s.id===r.id);if(!source)throw Error('Unknown owner slot');return {id:r.id,task:r.scope.task,workspace:source.workspace,data:source.data,backend:source.backend,profile:source.profile,profile_sha256:source.profile_sha256,owner_status:r.status};});
  if(!rows.length)throw Error('No accepted owner roots to inspect');
  write(output,{schema:'p805-integrated-history-spec/1',purpose:'Post-owner exact-production observations; no inference or human approval',package_result:path.resolve(packageResult),owner_plan:path.resolve(ownerPlanFile),owner_plan_sha256:hash(ownerPlanFile),owner_result:path.resolve(ownerResultFile),owner_result_sha256:hash(ownerResultFile),rows});return {spec:output,sha256:hash(output),rows:rows.length};
}
module.exports={run,prepare,assertPreview,assertRefusal,retentionExpected};
if(require.main===module){(async()=>{const [command,...args]=process.argv.slice(2);if(command==='prepare'&&args.length===4)console.log(JSON.stringify(prepare(...args)));else if(command==='run'&&args.length===3){const result=await run(...args);console.log(JSON.stringify(result));if(result.status==='failed')process.exitCode=1;}else throw Error('Usage: p805-owner-integration.cjs prepare <owner-plan> <owner-result> <package-result> <new-spec> | run <spec> <new-private-directory> <python-executable>');})().catch(error=>{console.error(error.message);process.exitCode=1;});}
