// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const runner=require('../../../scripts/evals/p805-owner-runner.cjs');
const materializer=require('../../../src/evals/release/p8-owner-v3/tools/prepare.cjs');
const inventory=require('../../../scripts/package-inventory.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function temporary(t){const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-p805-runner-test-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));return root;}
function budget(){return {schema:'p7-p8-owner-campaign/1',cap_micros:100000000,settled_micros:1278161,reserved_micros:34127269,models:['qwen/qwen3.8-max-0902'],runs:[]};}

test('repeated frozen-input audit rejects changed dispatch inputs but permits completed workspace edits',t=>{
  const root=temporary(t),packageRoot=path.join(root,'package');fs.mkdirSync(packageRoot);
  const put=(name,bytes)=>{const file=path.join(root,name);fs.writeFileSync(file,bytes);return file;};
  const executable=put('package/vcp.exe','frozen executable'),payload=put('package/catalog.json','packaged catalog');
  const pkg=put('package-result.json',JSON.stringify({manifest:inventory.buildManifest(packageRoot,{})}));
  const git=put('git.exe','pinned git'),preparation=put('preparation.json',JSON.stringify({git_executable:git,git_sha256:sha('pinned git')}));
  const source=put('source-profile.json','source profile'),catalog=put('catalog.json','catalog'),runnerFile=put('runner.cjs','runner');
  const profile=put('owner-profile.json','slot profile'),prompt=put('task.txt','prompt'),workspace=put('window.cjs','initial workspace');
  const plan={preparation_file:preparation,package_file:pkg,runner_hashes:{[runnerFile]:sha('runner')},input_hashes:Object.fromEntries([executable,pkg,preparation,source,catalog].map(file=>[file,sha(fs.readFileSync(file))])),fixture_manifest_sha256:sha(fs.readFileSync(path.resolve(__dirname,'../../evals/release/p8-owner-v3/manifest.json'))),runs:[{profile,profile_sha256:sha('slot profile'),prompt,prompt_sha256:sha('prompt')}]};
  const file=put('plan.json',JSON.stringify(plan)),expected=sha(fs.readFileSync(file));
  assert.equal(runner.frozenInputs(plan,file,expected).passed,true);
  // U03 edits and accumulating canonical evidence do not invalidate a binding.
  fs.writeFileSync(workspace,'completed workspace');put('canonical-state.json','new evidence');
  assert.equal(runner.frozenInputs(plan,file,expected).passed,true);
  for(const changed of [source,catalog,profile,prompt,runnerFile,executable,payload,git,file]){
    const before=fs.readFileSync(changed);fs.appendFileSync(changed,' changed after initial validation');
    assert.throws(()=>runner.frozenInputs(plan,file,expected),/changed|differs|frozen plan/,path.basename(changed));
    fs.writeFileSync(changed,before);
    assert.equal(runner.frozenInputs(plan,file,expected).passed,true);
  }
  put('package/unexpected.dll','extra payload');
  assert.throws(()=>runner.frozenInputs(plan,file,expected),/Distribution payload differs/);
});

test('campaign reservation is durable before dispatch, refuses overlap and preserves prior uncertainty',t=>{
  const root=temporary(t),file=path.join(root,'campaign.json');fs.writeFileSync(file,JSON.stringify(budget()));
  const reserve=state=>runner.reserve(state,{directory:root},'plan',{id:'u01-sqlite'});
  runner.changeCampaign(file,reserve);
  const state=JSON.parse(fs.readFileSync(file));assert.equal(state.reserved_micros,42127269);assert.equal(state.runs[0].status,'running-held');assert.equal(state.runs[0].actual_cost_micros,null);
  assert.throws(()=>runner.changeCampaign(file,reserve),/Duplicate slot/);
  assert.deepEqual(JSON.parse(fs.readFileSync(file)),state);
  fs.writeFileSync(file+'.production-interactive.lock','held');
  assert.throws(()=>runner.changeCampaign(file,()=>{}),/EEXIST/);
});

test('campaign cap failure never writes an over-budget allocation',t=>{
  const root=temporary(t),file=path.join(root,'campaign.json'),state=budget();state.reserved_micros=99000000;state.settled_micros=0;fs.writeFileSync(file,JSON.stringify(state));
  assert.throws(()=>runner.changeCampaign(file,b=>runner.reserve(b,{directory:root},'plan',{id:'u01-sqlite'})),/Campaign ceiling/);
  assert.deepEqual(JSON.parse(fs.readFileSync(file)),state);
});

test('preservation permits only U03 declared files and detects foreign writes',t=>{
  const root=temporary(t);fs.mkdirSync(path.join(root,'src/domain'),{recursive:true});fs.writeFileSync(path.join(root,'src/domain/window.cjs'),'initial');fs.writeFileSync(path.join(root,'user-note.txt'),'keep');
  const row={case:'U03',workspace:root,workspace_files:materializer.inventory(root),git:null};
  fs.writeFileSync(path.join(root,'src/domain/window.cjs'),'edited');assert.deepEqual(runner.preservation(row,null).allowed_changes,['src/domain/window.cjs']);
  assert.throws(()=>runner.preservation(row,null,true),/Protected workspace bytes/);
  fs.writeFileSync(path.join(root,'user-note.txt'),'changed');assert.throws(()=>runner.preservation(row,null),/Protected workspace bytes/);
  fs.writeFileSync(path.join(root,'user-note.txt'),'keep');fs.writeFileSync(path.join(root,'new.txt'),'extra');assert.throws(()=>runner.preservation(row,null),/file set/);
});

test('canonical natural-language extraction preserves prose and rejects corrupt bytes',()=>{
  const event={type:'response.completed',response:{status:'completed',id:'provider-one',model:'test',output:[{type:'message',content:[{type:'output_text',text:'The boundary defect is on line 2.\nNo code changed.'}]}]}};
  const bytes=Buffer.from('data: '+JSON.stringify(event)+'\n\ndata: [DONE]\n\n');
  const pages=[{items:[{id:'artifact-one',collection:'artifact',visibility:'available',record:{spec:{channel:'response'},state:'complete',length:String(bytes.length),sha256:sha(bytes)}}],gaps:[]}];
  const call=()=>({status:0,error:null,stdout:JSON.stringify({type:'result',data:{items:[{artifact:'artifact-one',visibility:'available',range:{start:0,end:bytes.length},bytes:[...bytes]}],gaps:[],next_cursor:null}})});
  const captured=[];const answer=runner.naturalAnswer({executable:'unused'},'/unused',pages,[{provider_request:'provider-one'}],call,(name,text)=>captured.push({name,text}));
  assert.equal(answer.text,event.response.output[0].content[0].text);assert.equal(captured.length,1);
  pages[0].items[0].record.sha256='0'.repeat(64);assert.throws(()=>runner.naturalAnswer({executable:'unused'},'/unused',pages,[{provider_request:'provider-one'}],call,()=>{}),/identity mismatch/);
});

test('tool-call responses and unmatched provider identities cannot become final answers',()=>{
  const bytes=Buffer.from('data: '+JSON.stringify({type:'response.completed',response:{status:'completed',id:'another',output:[{type:'message',content:[{type:'output_text',text:'not this task'}]}]}})+'\n\n');
  const pages=[{items:[{id:'artifact',collection:'artifact',visibility:'available',record:{spec:{channel:'response'},state:'complete',length:String(bytes.length),sha256:sha(bytes)}}],gaps:[]}];
  const call=()=>({status:0,stdout:JSON.stringify({type:'result',data:{items:[{range:{start:0,end:bytes.length},bytes:[...bytes]}],gaps:[]}})});
  assert.throws(()=>runner.naturalAnswer({executable:'unused'},'/unused',pages,[{provider_request:'expected'}],call,()=>{}),/unambiguous/);
});

test('verification requires successful canonical parent check with known effects and costs',()=>{
  const record={id:'verification-one',scope:{task:'parent'},steering:'1',fingerprint:{repository:'a'.repeat(64)},outputs:['output'],unresolved_effects:[],outstanding_issues:[],cost:{certainty:'known'},checks:[{specification:'package.json#test',outcome:{status:'passed'},exit_code:0}]};
  const task={scope:record.scope,steering:record.steering,fingerprint:record.fingerprint,state:'completed',parent:null,editing:true,required_checks:['package.json#test']};
  const pages=[{gaps:[],items:[{collection:'verification',visibility:'available',record}]}];
  assert.deepEqual(runner.verificationEvidence(pages,task),['verification-one']);assert.throws(()=>runner.verificationEvidence(pages,{...task,scope:{task:'child'}}),/current-parent/);
  assert.throws(()=>runner.verificationEvidence(pages,{...task,steering:'2'}),/current-parent/);
  record.unresolved_effects.push('unknown');assert.throws(()=>runner.verificationEvidence(pages,task),/current-parent/);
});

test('current source oracle rejects edits after canonical verification and incomplete coverage',t=>{
  const root=temporary(t),files=['src/domain/window.cjs','src/api/page.cjs','package.json','test/page.test.cjs'];
  for(const file of files){fs.mkdirSync(path.dirname(path.join(root,file)),{recursive:true});fs.writeFileSync(path.join(root,file),'source');}
  const manifest={bounded_scan_complete:true,files:files.map(file=>({path:file,sha256:sha('source'),bytes:'6'}))};
  assert.equal(runner.sourceManifest(manifest,root).files,4);
  fs.writeFileSync(path.join(root,files[0]),'later edit');assert.throws(()=>runner.sourceManifest(manifest,root),/differs from current parent/);
  fs.writeFileSync(path.join(root,files[0]),'source');manifest.files.pop();assert.throws(()=>runner.sourceManifest(manifest,root),/manifest incomplete/);
});

test('bounded subprocess returns actual nonzero exits and terminates a deadline',async()=>{
  const failed=await runner.bounded(process.execPath,['-e','process.stderr.write("bounded failure");process.exit(3)'],5000,{});
  assert.equal(failed.status,3);assert.equal(failed.stderr,'bounded failure');
  const timed=await runner.bounded(process.execPath,['-e','setInterval(()=>{},1000)'],100,process.env);assert.equal(timed.error,'deadline_exceeded');
});
