// SPDX-License-Identifier: Apache-2.0
'use strict';
// External final-file/check observations only; this never generates a candidate.
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {plain,read,safeChild,filesUnder}=require('./p6-live-runner.cjs').boundaries;
const fixture=path.resolve(__dirname,'../../src/evals/skills/builtin/debug-v1/manifest.json');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function definition(id){const bytes=read(fixture),manifest=JSON.parse(bytes),scenario=manifest.cases.find(c=>c.id===id);if(!scenario)throw Error('Unknown frozen debug scenario');return {manifest,scenario,fixture_sha256:sha(bytes)};}
function prepare(id,destination){
  const {manifest,scenario}=definition(id);if(fs.existsSync(destination))throw Error('Fresh debug workspace required');
  fs.mkdirSync(destination,{recursive:true});
  for(const [file,bytes]of Object.entries({...manifest.files,'shipping.cjs':scenario.source}))fs.writeFileSync(safeChild(destination,file),bytes,{flag:'wx'});
}
function removeTemporary(directory){
  const temporaryRoot=plain(path.resolve(os.tmpdir())),target=plain(path.resolve(directory));
  const relative=path.relative(temporaryRoot,target);
  if(!relative||relative.startsWith('..')||path.isAbsolute(relative)||path.dirname(target)!==temporaryRoot||!path.basename(target).startsWith('vcp-cr06-'))throw Error('Refusing cleanup outside the exact temporary fixture root');
  fs.rmSync(target,{recursive:true,force:true});
}
function observe(workspace){
  const help=spawnSync(process.execPath,['--help'],{env:{},encoding:'utf8',timeout:3000,maxBuffer:131072,windowsHide:true});
  if(help.error||help.status!==0||!help.stdout.includes('--allow-net'))throw Error('Qualified network-denying runtime required; candidate was not loaded');
  const adapter="for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];const encode=JSON.stringify.bind(JSON),write=process.stdout.write.bind(process.stdout);const fee=require(process.argv[1]).shippingFee;write('\\nCR06_RESULT='+encode([49,50,51].map(fee))+'\\n');";
  const result=spawnSync(process.execPath,['--permission',`--allow-fs-read=${workspace}`,'--max-old-space-size=64','-e',adapter,safeChild(workspace,'shipping.cjs')],{cwd:workspace,env:process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{},encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});
  const line=result.stdout?.trim().split(/\r?\n/).at(-1);let values=null;try{if(line?.startsWith('CR06_RESULT='))values=JSON.parse(line.slice(12));}catch{}
  return {status:!result.error&&result.status===0&&JSON.stringify(values)==='[5,0,0]'?'passed':'failed',exit_code:result.status,values,stdout:result.stdout,stderr:result.stderr,error:result.error?.code??null};
}
function preservation(workspace,manifest,scenario){
  const failures=[];
  if(JSON.stringify(filesUnder(workspace))!==JSON.stringify([...Object.keys(manifest.files),'shipping.cjs'].sort()))failures.push('workspace inventory changed');
  for(const [file,expected]of Object.entries(manifest.files))if(read(safeChild(workspace,file)).toString()!==expected)failures.push(file);
  const source=read(safeChild(workspace,'shipping.cjs'),128*1024).toString();
  for(const marker of scenario.owned_instrumentation)if(source.includes(marker))failures.push('owned instrumentation retained');
  if(scenario.retained_line&&!source.split(/\r?\n/).includes(scenario.retained_line))failures.push('concurrent human line changed');
  return failures;
}
function grade(id,directory){
  const {manifest,scenario,fixture_sha256}=definition(id),workspace=plain(path.resolve(directory));
  const inventory=()=>Object.fromEntries(filesUnder(workspace).map(file=>[file,sha(read(safeChild(workspace,file)))]));
  const current_files=inventory();
  let initial,current;
  const failures=preservation(workspace,manifest,scenario);
  if(scenario.reproduction==='unavailable')initial=current={status:'not_run',reason:'required reproduction environment and execution grant unavailable'};
  else {
    const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-cr06-original-'));
    try{const original=path.join(temporary,'workspace');prepare(id,original);initial=observe(original);}finally{removeTemporary(temporary);}
    current=observe(workspace);
  }
  failures.push(...preservation(workspace,manifest,scenario));
  if(JSON.stringify(current_files)!==JSON.stringify(inventory()))failures.push('workspace changed during observation');
  return {schema:'p7-cr06-debug-observation/1',case_id:id,fixture_sha256,current_files,initial,current,preservation:[...new Set(failures)],controls_pass:!failures.length&&(scenario.reproduction==='unavailable'||initial.status==='failed'&&current.status==='passed'),verification_complete:current.status==='passed',cleanup:failures.includes('owned instrumentation retained')?'owned_instrumentation_remaining':scenario.retained_line?'blocked_by_concurrent_human_edit':'no_owned_instrumentation_remaining',live_usefulness:'not_run'};
}
module.exports={prepare,grade,definition,removeTemporary};
if(require.main===module){try{const [command,id,directory,...rest]=process.argv.slice(2);if(rest.length||!directory||!['prepare','grade'].includes(command))throw Error('Usage: builtin-debug-oracle.cjs prepare|grade <case-id> <workspace>');if(command==='prepare')prepare(id,directory);else{const result=grade(id,directory);console.log(JSON.stringify(result));if(!result.controls_pass)process.exitCode=1;}}catch(error){console.error(error.message);process.exitCode=1;}}
