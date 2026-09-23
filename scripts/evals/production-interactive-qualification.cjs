// SPDX-License-Identifier: Apache-2.0
'use strict';
// One-shot exact-artifact terminal observation. Prepare makes no model calls.
// Run reserves the entire $16 in the existing campaign before launching the PTY.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawn, spawnSync} = require('node:child_process');
const readline = require('node:readline');
const repo = path.resolve(__dirname, '../..');
const prior = require(path.join(repo, 'scripts/evals/p6-live-runner.cjs'));
const builtin = require(path.join(repo, 'scripts/evals/builtin-live-runner.cjs'));
const terminal = require(path.join(repo, 'scripts/evals/delegation-terminal-runner.cjs'));
const inventory = require(path.join(repo, 'scripts/package-inventory.cjs'));
const {plain, read, privateDirectory, noParentInstructions, noSecrets} = prior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const hash = file => sha(read(file, 1024 * 1024 * 1024));
const json = value => JSON.stringify(value, null, 2) + '\n';
// The PTY driver's stdin is JSONL: each physical line must be one complete
// control. Pretty JSON is only for evidence files, never this transport.
const controlFrame = value => JSON.stringify(value) + '\n';
function controlWriter(stream,onFailure,onControl=()=>{}) {
  let failed=false,terminating=false;
  const fail=error=>{if(!failed){failed=true;onFailure(error);}};
  // A writable check cannot prevent the reader from closing before write's
  // asynchronous completion. Handle both callback and stream errors once.
  stream.on('error',fail);
  return value=>{
    if(failed)return false;
    if(stream.destroyed||!stream.writable){fail(Error('PTY control channel unavailable'));return false;}
    if(value.action==='terminate'&&terminating)return true;
    if(value.action==='terminate')terminating=true;
    try{onControl(value);stream.write(controlFrame(value),error=>{if(error)fail(error);});return true;}
    catch(error){fail(error);return false;}
  };
}
const put = (file, value) => fs.writeFileSync(file, typeof value === 'string' ? value : json(value), {flag:'wx', mode:0o600});
const CAP = 16000000;
const PAUSED = 'Pause requested; inspect retained effects before resuming.';
const RESUMED = 'Resumed after revalidation.';
const DRAINED = 'Parent turn interrupted; inspect current state before explicit /resume.';
const MAX_MS = 90000;

function credentialSurface(secret, surface, value, report) {
  const raw=String(value||'');
  const leaked=raw.includes(secret);
  report.credential_surface_checks ||= {};
  report.credential_surface_checks[surface] = report.credential_surface_checks[surface] === 'detected' || leaked ? 'detected' : 'not_detected';
  if(leaked){const failure='Provider credential appeared in '+surface+' before redaction';if(!report.failures.includes(failure))report.failures.push(failure);}
  return raw.split(secret).join('[REDACTED_PROVIDER_CREDENTIAL]');
}

function prepare(packageResult, destination, sourceProfile) {
  if (process.platform !== 'win32') throw Error('Native Windows required');
  if (typeof sourceProfile !== 'string' || !sourceProfile) throw Error('Explicit private source profile required');
  destination = plain(path.resolve(destination));
  privateDirectory(destination); noParentInstructions(path.dirname(destination));
  if (fs.existsSync(destination)) throw Error('New private directory required');
  const receiptFile = plain(path.resolve(packageResult));
  const receipt = JSON.parse(read(receiptFile));
  const packageRoot = path.dirname(receiptFile);
  const payloadRoot = plain(path.join(packageRoot, 'package'));
  inventory.verifyManifest(payloadRoot, receipt.manifest);
  if (read(path.join(payloadRoot,'manifest.json')).toString('utf8') !== json(receipt.manifest)) throw Error('Extracted manifest differs from the frozen package receipt');
  const archive = plain(path.join(packageRoot, receipt.package));
  const executable = plain(path.join(packageRoot, 'package/vcp.exe'));
  const binaryRow = receipt.manifest.files.filter(row => row.path === 'vcp.exe');
  if (binaryRow.length !== 1 || hash(archive) !== receipt.archive_sha256 || hash(executable) !== binaryRow[0].sha256) throw Error('Exact archive/extracted executable required');
  const buildFile=plain(path.join(packageRoot,'package/build-receipt.json'));
  const build=JSON.parse(read(buildFile));
  if(receipt.manifest.build?.status!=='recorded-local-build'||hash(buildFile)!==receipt.manifest.build.receipt_sha256||build.executable_sha256!==binaryRow[0].sha256||build.profile!=='release'||build.qualification_build!==false||build.source_stable!==true||build.exit_code!==0)throw Error('Frozen production release build receipt required');
  // Never enable a qualification endpoint to make production CLI accept a mock.
  const original = JSON.parse(read(plain(path.resolve(sourceProfile)))); noSecrets(original);
  const reasons = builtin.fixedProfileReasons(original, Date.now() + MAX_MS);
  if (reasons.length) throw Error(reasons.join('; '));
  if (original.provider.compatibility.model !== 'qwen/qwen3.8-max-0902') throw Error('Frozen Qwen model identity required');
  const driver = plain(path.join(repo, 'artifacts/p7-owner-native-package-v2/delegation-pty-driver.exe'));
  const exporter = plain(path.join(repo, 'artifacts/p7-owner-native-package-v2/delegation-state-export.exe'));
  const inputHashes = Object.fromEntries([receiptFile, archive, executable, buildFile, driver, exporter, path.resolve(sourceProfile), plain(original.catalog), __filename,
    path.join(repo,'scripts/evals/p6-live-runner.cjs'), path.join(repo,'scripts/evals/builtin-live-runner.cjs'), path.join(repo,'scripts/evals/delegation-terminal-runner.cjs'),
    path.join(repo,'scripts/package-inventory.cjs'), path.join(payloadRoot,'manifest.json'),
    ...receipt.manifest.files.map(row=>path.join(payloadRoot,row.path))].map(file => [file, hash(file)]));
  fs.mkdirSync(destination, {mode:0o700});
  const workspace = path.join(destination, 'workspace'); fs.mkdirSync(workspace);
  const data = path.join(destination, 'data'); fs.mkdirSync(data);
  put(path.join(workspace,'README.synthetic.txt'), 'Synthetic production terminal probe. Read-only analysis; no external tools or delegation.\n');
  put(path.join(workspace,'observations.txt'), 'The synthetic service validates inputs before constructing a report. Explain this dependency and cite this file. Preserve this exact text.\n');
  const profile = {...original, workspace, maximum_autonomy:'plan', automatic_effects:[], affected_paths:['README.synthetic.txt','observations.txt'], budget_usd:'16.000000', max_requests:2, deadline_seconds:90, provider_timeout_seconds:60};
  const profileFile = path.join(destination,'profile.json'); put(profileFile,profile);
  put(path.join(destination,'task.txt'),'Read observations.txt and explain its dependency ordering with one cited observation. This is a bounded read-only terminal lifecycle probe. Do not change files, run processes, delegate, or use external services. Keep the final answer brief.\n');
  const args = ['--workspace',workspace,'--data-dir',data,'--config',profileFile,'run','--file',path.join(destination,'task.txt'),'--budget-usd','16.000000','--autonomy','plan'];
  put(path.join(destination,'driver-spec.json'),{executable,workspace,arguments:args});
  const generated = Object.fromEntries(['profile.json','task.txt','driver-spec.json','workspace/README.synthetic.txt','workspace/observations.txt'].map(file => [file,hash(path.join(destination,file))]));
  const plan = {schema:'p8-production-interactive-plan/1',directory:destination,executable,driver,exporter,workspace,data,profile:profileFile,
    campaign:path.join(repo,'artifacts/p7-p8-owner-campaign.json'),cap_micros:CAP,max_ms:MAX_MS,model:original.provider.compatibility.model,
    package_sha256:receipt.archive_sha256,package_root:payloadRoot,package_manifest:receipt.manifest,executable_sha256:binaryRow[0].sha256,input_hashes:inputHashes,generated,
    allocation_basis:{max_requests:2,per_request_reservation_micros:6492808,explanation:'The qualified profile lacks a byte ceiling and reserves full 983616-token input capacity for input/cache-read/cache-write plus output and request. One $16 probe permits initial and resumed reservations while the paused request may retain unknown liability. Output allowance and provider qualification remain unchanged; no retries or repeat trials.'},
    criteria:{same_process:true,same_task:true,pause_ack_ms:5000,resume_ack_ms:5000,exit_ms:5000,paused_observation_ms:1000,forbidden_workspace_changes:0},
    limitations:['Single synthetic lifecycle observation, not P8-05 task quality or statistical latency acceptance.','90-second task and 60-second provider limits intentionally bound cancellation/resume observation; they are not task-completion quality thresholds.','Canonical submitted attempts are dispatch evidence; no gateway/network observer proves delivery or counts wire requests.','Exact current credential is checked before redaction on captured terminal, driver stderr and command/preflight output only; this is not the full sensitive-surface matrix.','No machine handoff, full-volume fault, or delegated-child coverage.']};
  put(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'),sha256:hash(path.join(destination,'plan.json')),model_calls:0,reservation_made:false};
}

function validate(file, expected) {
  if (!/^[a-f0-9]{64}$/.test(expected) || hash(file) !== expected) throw Error('Exact frozen plan hash required');
  const plan = JSON.parse(read(file));
  if (plan.schema !== 'p8-production-interactive-plan/1' || plan.cap_micros !== CAP || plan.max_ms !== MAX_MS) throw Error('Frozen bounds changed');
  privateDirectory(plan.directory); noParentInstructions(path.dirname(plan.directory));
  inventory.verifyManifest(plan.package_root,plan.package_manifest);
  for (const [name,digest] of Object.entries(plan.input_hashes)) if (hash(plain(name)) !== digest) throw Error('Frozen external input changed');
  for (const [name,digest] of Object.entries(plan.generated)) if (hash(plain(path.join(plan.directory,name))) !== digest) throw Error('Frozen generated input changed');
  const reasons = builtin.profileReasons(JSON.parse(read(plan.profile)),Date.now()+MAX_MS);
  if (reasons.length) throw Error(reasons.join('; '));
  return plan;
}

function changeCampaign(file, change) {
  const lock=file+'.production-interactive.lock'; const fd=fs.openSync(lock,'wx');
  try {
    const budget=JSON.parse(read(file));
    if (budget.schema !== 'p7-p8-owner-campaign/1' || budget.cap_micros !== 100000000 || ![budget.settled_micros,budget.reserved_micros].every(Number.isSafeInteger)) throw Error('Existing campaign authority/amounts changed');
    change(budget);
    if (budget.settled_micros+budget.reserved_micros>budget.cap_micros) throw Error('Campaign ceiling exceeded');
    const temp=file+'.'+crypto.randomUUID()+'.tmp'; put(temp,budget); fs.renameSync(temp,file);
  } finally { fs.closeSync(fd); fs.unlinkSync(lock); }
}

async function run(file, expected) {
  const plan=validate(plain(path.resolve(file)),expected);
  if (process.platform!=='win32' || !process.env.OPENROUTER_API_KEY) throw Error('Native Windows and existing credential environment required');
  const marker=path.join(plan.directory,'run-once.json');
  put(marker,{plan_sha256:expected,started_at:new Date().toISOString(),cap_micros:CAP});
  const started=Date.now();
  const result={schema:'p8-production-interactive-result/1',plan_sha256:expected,status:'failed',cap_micros:CAP,reservation_made:false,held_upper_bound_micros:0,actual_cost_micros:0,cost_status:'not_started',failures:[],events:[],commands:[],observations:[],limitations:plan.limitations};
  const secret=process.env.OPENROUTER_API_KEY;
  const redact=text=>String(text).split(secret).join('[REDACTED_PROVIDER_CREDENTIAL]');
  const checkSurface=(surface,value)=>credentialSurface(secret,surface,value,result);
  const save=()=>fs.writeFileSync(path.join(plan.directory,'result.json'),redact(json(result)),{mode:0o600});
  const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  let child=null, closed=false, text='', driverStderr='', structuredExit=null, forced=false, poll=0, task=null, controls=null, controlFailure=null;
  const record=event=>result.events.push({at_ms:Date.now()-started,...event});
  const send=value=>{if(!controls?.(value))throw Error('PTY control channel unavailable');};
  const wait=async(predicate,ms,label)=>{const deadline=Math.min(started+MAX_MS,Date.now()+ms);while(!predicate()){if(controlFailure)throw Error(controlFailure);if(closed)throw Error('Terminal exited before '+label);if(Date.now()>=deadline)throw Error(label+' deadline exceeded');await delay(25);}};
  function invoke(name,program,args,timeout=5000,env=process.env) {
    const began=Date.now(); const output=spawnSync(program,args,{windowsHide:true,shell:false,encoding:'utf8',timeout,maxBuffer:4*1024*1024,env,stdio:['ignore','pipe','pipe']});
    const log={name,program,arguments:args,exit_code:output.status,error:output.error?.code||null,elapsed_ms:Date.now()-began,stdout:checkSurface('command '+name+' stdout',output.stdout),stderr:checkSurface('command '+name+' stderr',output.stderr)};
    result.commands.push(log);save();
    if(output.status!==0||output.error)throw Error(name+' failed; retained bounded command receipt');
    return log.stdout;
  }
  function cli(name,args) {
    const output=invoke(name,plan.executable,['--format','jsonl','--non-interactive','--workspace',plan.workspace,'--data-dir',plan.data,...args]);
    const frame=output.trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line)).findLast(row=>row.type==='result');
    if(!frame||frame.exit_code!==0)throw Error(name+' lacks successful result');
    return frame.data;
  }
  function inspectCosts(name) {
    const pages=[];let cursor=null;
    do {const args=['inspect',task,'--view','costs','--limit','128'];if(cursor)args.push('--cursor',JSON.stringify(cursor));const page=cli(name+'-'+pages.length,args);if(!Array.isArray(page.items)||!Array.isArray(page.gaps)||page.gaps.length||pages.length>=4)throw Error('Cost evidence incomplete or unbounded');pages.push(page);cursor=page.next_cursor;}while(cursor);
    return pages;
  }
  const attempts=pages=>pages.flatMap(page=>page.items).filter(row=>row.collection==='attempt'&&row.visibility==='available').map(row=>row.record);
  function cliPid(name) {
    const script="ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Process -Filter \"Name = 'vcp.exe'\" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($env:VCP_PTY_MATCH) } | Select-Object ProcessId,ParentProcessId)";
    const rows=JSON.parse(invoke(name,'pwsh',['-NoProfile','-Command',script],5000,{...process.env,VCP_PTY_MATCH:plan.profile}));
    if(!Array.isArray(rows)||rows.length!==1||!Number.isInteger(rows[0].ProcessId))throw Error('Exact terminal process identity unavailable');
    return rows[0].ProcessId;
  }
  try {
    // Credential-free startup preflight makes no paid call and consumes no reservation.
    const env={...process.env};delete env.OPENROUTER_API_KEY;
    const pre=spawnSync(plan.executable,['--format','jsonl','--non-interactive','--workspace',plan.workspace,'--data-dir',plan.data,'--config',plan.profile,'run','Offline startup validation','--budget-usd','16.000000','--autonomy','plan'],{windowsHide:true,encoding:'utf8',timeout:10000,maxBuffer:1024*1024,env});
    result.preflight={exit_code:pre.status,stdout:checkSurface('preflight stdout',pre.stdout),stderr:checkSurface('preflight stderr',pre.stderr)};save();
    if(pre.status!==2||pre.error||!String(pre.stderr).includes('OPENROUTER_API_KEY is required')||terminal.discoverWorkspace(plan.data))throw Error('Offline credential-free startup preflight rejected');
    if(result.failures.length)throw Error('Preflight sensitive-output check failed');
    // Atomic existing-campaign reservation is required immediately before the only paid launch.
    changeCampaign(plan.campaign,budget=>{
      if (!budget.models.includes(plan.model) || budget.runs.some(row=>row.sha256===expected || ['prepared','running','running-held'].includes(row.status))) throw Error('Duplicate trial or another outstanding launch reservation');
      budget.reserved_micros+=CAP;
      budget.runs.push({model:plan.model,stage:'p8-production-interactive',cap_micros:CAP,plan:path.resolve(file),sha256:expected,status:'running-held',actual_cost_micros:null});
    });
    result.reservation_made=true;result.held_upper_bound_micros=CAP;result.actual_cost_micros=null;result.cost_status='unknown';save();
    child=spawn(plan.driver,[path.join(plan.directory,'driver-spec.json')],{windowsHide:true,shell:false,stdio:['pipe','pipe','pipe'],env:{...process.env,RUST_MIN_STACK:'16777216'}});
    controls=controlWriter(child.stdin,error=>{controlFailure='PTY control write failed: '+String(error.code||error.message);result.failures.push(controlFailure);},value=>record({control:value}));
    result.driver_pid=child.pid;
    let capturedBytes=0;
    const lines=readline.createInterface({input:child.stdout});
    lines.on('line',line=>{
      capturedBytes+=Buffer.byteLength(line);if(capturedBytes>4*1024*1024){result.failures.push('Output ceiling exceeded');controls({action:'terminate'});return;}
      try {const event=JSON.parse(line);if(event.type==='output'){if(typeof event.text!=='string')throw Error('text required');text+=event.text;checkSurface('terminal',text);record({output_bytes:Buffer.byteLength(event.text),output_sha256:sha(event.text)});}else if(event.type==='exit'){structuredExit=event.code;record({event});}else if(event.type==='started'){record({event});}else throw Error('unknown event');}catch{result.failures.push('Invalid PTY event');}
    });
    child.stderr.on('data',bytes=>{capturedBytes+=bytes.length;driverStderr+=bytes.toString('utf8');checkSurface('driver stderr',driverStderr);if(capturedBytes>4*1024*1024)controls({action:'terminate'});});
    child.on('error',error=>{result.failures.push(error.message);closed=true;});
    child.on('close',()=>{closed=true;});
    await wait(()=>text.includes('/pause /resume'),30000,'interactive startup');
    const discovered=terminal.discoverWorkspace(plan.data);if(!discovered)throw Error('Canonical task descriptor unavailable');task=discovered.root_task_id;result.task=task;result.descriptor=discovered.path;
    const firstPid=cliPid('initial-process');result.cli_pid=firstPid;
    let submitted=null;
    while(Date.now()-started<45000){const pages=inspectCosts('dispatch-'+poll++);submitted=attempts(pages).find(row=>row.phase==='submitted');if(submitted){result.dispatch_observation={kind:'canonical_submitted_attempt',attempt:submitted.id,phase:submitted.phase};break;}await delay(100);}
    if(!submitted)throw Error('No submitted model attempt observed before pause; dispatch unavailable');
    const pauseAt=Date.now(),pauseOffset=text.length;send({action:'write',text:'/pause\r'});
    await wait(()=>text.slice(pauseOffset).includes(PAUSED),5000,'pause acknowledgement');result.pause_ack_ms=Date.now()-pauseAt;
    const paused=cli('paused-status',['tasks','status',task]);
    if(paused.records?.[0]?.state!=='paused')throw Error('Canonical task is not paused');
    result.observations.push({kind:'paused-live-owner',data:paused});
    send({action:'write',text:'/status\r/cost\r'});
    const before=attempts(inspectCosts('paused-costs-before'));
    await delay(1000);
    const after=attempts(inspectCosts('paused-costs-after'));
    if(JSON.stringify(before.map(row=>row.id).sort())!==JSON.stringify(after.map(row=>row.id).sort()))throw Error('New canonical attempt appeared while paused');
    result.paused_attempt_ids=after.map(row=>row.id).sort();
    result.no_new_dispatch_claim='No new canonical attempt IDs during 1000ms paused observation; no network oracle.';
    await wait(()=>text.slice(pauseOffset).includes(DRAINED),15000,'parent drain');
    const resumeAt=Date.now(),resumeOffset=text.length;send({action:'write',text:'/resume\r'});
    await wait(()=>text.slice(resumeOffset).includes(RESUMED),5000,'resume acknowledgement');result.resume_ack_ms=Date.now()-resumeAt;
    if(cliPid('resumed-process')!==firstPid||terminal.discoverWorkspace(plan.data)?.root_task_id!==task)throw Error('Resume changed task or process');
    const resumed=cli('resumed-status',['tasks','status',task]);result.observations.push({kind:'resumed-live-owner',data:resumed});
    if(resumed.records?.[0]?.state!=='running')throw Error('Resumed task is not running');
    const exitAt=Date.now(),exitOffset=text.length;send({action:'write',text:'/exit\r'});
    const exitDeadline=Date.now()+5000;while(!closed&&Date.now()<exitDeadline)await delay(25);
    result.exit_ms=Date.now()-exitAt;
    if(!closed)throw Error('Terminal exit deadline exceeded');
    const terminalFrame=terminal.finalTerminalResult(text,exitOffset);
    const terminalError=terminal.terminalResultReason(terminalFrame,structuredExit,task);
    if(terminalError)throw Error(terminalError);
    result.terminal_result=terminalFrame;
    result.structured_exit=structuredExit;
  } catch(error) {result.failures.push(redact(error.message));}
  finally {
    if(child&&!closed){forced=true;try{send({action:'terminate'});}catch{}const end=Date.now()+3000;while(!closed&&Date.now()<end)await delay(25);if(!closed)child.kill();}
    result.forced_termination=forced;
    if(child&&!closed){const end=Date.now()+3000;while(!closed&&Date.now()<end)await delay(25);}
    if(!closed&&child)result.failures.push('PTY owner termination unconfirmed; no recovery export attempted');
    if(closed&&task){
      try{
        const reopened=cli('production-reopen-status',['tasks','status',task]);
        if(reopened.records?.[0]?.state!=='paused'){result.failures.push('Production reopen did not observe durably paused task');throw Error('Production recovery status rejected');}
        result.observations.push({kind:'production-fresh-reopen',data:reopened});
        const snapshot=path.join(plan.directory,'recovery-state.json');invoke('canonical-recovery-export',plan.exporter,[result.descriptor,snapshot],15000);
        result.snapshot_sha256=hash(snapshot);
        const state=JSON.parse(read(snapshot,16*1024*1024));
        const rows=Object.values(state.records);
        const root=rows.find(row=>row.collection==='task'&&row.id===task)?.value;
        if(root?.state!=='paused'){result.failures.push('Recovered root task is not durably paused');throw Error('Snapshot recovery status rejected');}
        const pages=[{gaps:[],items:rows.map(row=>({collection:row.collection,record:row.value,visibility:'available'}))}];
        const accounting=prior.accounting(pages,CAP);result.accounting=accounting;result.actual_cost_micros=accounting.actual_cost_micros;result.cost_status='canonical';
      }catch(error){result.accounting_limitation=redact(error.message);}
    }
    for(const [name,digest]of Object.entries(plan.generated)){try{if(hash(path.join(plan.directory,name))!==digest)result.failures.push('Frozen input changed: '+name);}catch{result.failures.push('Frozen input missing: '+name);}}
    result.final_input_hashes={};
    for(const [name,digest]of Object.entries(plan.input_hashes)){
      try{const actual=hash(plain(name));result.final_input_hashes[name]={sha256:actual,unchanged:actual===digest};if(actual!==digest)result.failures.push('Frozen external input changed: '+name);}
      catch{result.final_input_hashes[name]={unchanged:false};result.failures.push('Frozen external input missing or unreadable: '+name);}
    }
    try{if(hash(file)!==expected)result.failures.push('Frozen plan changed during execution');}catch{result.failures.push('Frozen plan missing after execution');}
    try{inventory.verifyManifest(plan.package_root,plan.package_manifest);}catch{result.failures.push('Distribution inventory changed during execution');}
    if(fs.readdirSync(plan.workspace).sort().join(',')!=='README.synthetic.txt,observations.txt')result.failures.push('Unexpected workspace files');
    put(path.join(plan.directory,'terminal.redacted.txt'),checkSurface('terminal',text));
    put(path.join(plan.directory,'driver-stderr.redacted.txt'),checkSurface('driver stderr',driverStderr));
    result.transcript_sha256=hash(path.join(plan.directory,'terminal.redacted.txt'));
    result.driver_stderr_sha256=hash(path.join(plan.directory,'driver-stderr.redacted.txt'));
    result.status=result.failures.length?'failed':'observed';result.elapsed_ms=Date.now()-started;save();
    if(result.reservation_made)changeCampaign(plan.campaign,budget=>{
      const rows=budget.runs.filter(row=>row.sha256===expected);if(rows.length!==1||rows[0].status!=='running-held')throw Error('Campaign reservation changed');
      const row=rows[0];row.status=result.status==='observed'?'observed-held':'failed-held';row.result=path.join(plan.directory,'result.json');row.actual_cost_micros=result.actual_cost_micros;
      if(result.status==='observed'&&result.cost_status==='canonical'){budget.reserved_micros-=CAP;budget.settled_micros+=result.actual_cost_micros;row.status='passed';result.held_upper_bound_micros=result.actual_cost_micros;}
    });save();
  }
  return {result:path.join(plan.directory,'result.json'),status:result.status,cost_status:result.cost_status,held_upper_bound_micros:result.held_upper_bound_micros};
}

module.exports={prepare,validate,run,credentialSurface,controlFrame,controlWriter};
if(require.main===module){(async()=>{const[command,file,extra,sourceProfile,...rest]=process.argv.slice(2);if(rest.length||!file||!extra||!['prepare','run'].includes(command)||(command==='prepare'&&!sourceProfile)||(command==='run'&&sourceProfile))throw Error('Usage: production-interactive-qualification.cjs prepare <package-result.json> <new-private-dir> <private-source-profile.json> | run <plan.json> <exact-plan-sha256>');const result=command==='prepare'?prepare(file,extra,sourceProfile):await run(file,extra);console.log(json(result));if(command==='run'&&result.status!=='observed')process.exitCode=1;})().catch(error=>{console.error(error.message);process.exitCode=1;});}
