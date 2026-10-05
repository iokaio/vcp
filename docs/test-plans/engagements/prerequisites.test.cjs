// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {verify,copy,sha,protectedFiles}=require('./prerequisites.cjs'),{inject}=require('./fault.cjs');
const baseContract=require('./base-contract.cjs');
function evidence() {
  const root=fs.mkdtempSync(path.resolve(__dirname,'../../../artifacts/ee07-prerequisite-fixture-'));
  const write=(name,data)=>{const file=path.join(root,name);fs.mkdirSync(path.dirname(file),{recursive:true});const bytes=Buffer.from(typeof data==='string'?data:JSON.stringify(data));fs.writeFileSync(file,bytes);return {file,hash:sha(bytes)};};
  const executable=write('candidate/vcp.exe','synthetic non-executable identity fixture');
  const spec={schema:'vcp-engagement-input/1',source_revision:'a'.repeat(40),candidate:{executable:executable.file,sha256:executable.hash,version:'0.0.1'}};
  write('candidate/build-evidence.json',{schema:'vcp-execution-diagnostic-build/1',version:'0.0.1',actual_version:'vcp 0.0.1',source_commit:spec.source_revision,source_stable:true,status:'passed',sha256:executable.hash,bytes:fs.statSync(executable.file).size,command:'MUST NOT COPY',secret:'MUST NOT COPY'});
  for(const kind of ['A','B']) {
    const files={}; for(const name of [...protectedFiles[kind],'README.md']) {const row=write(`${kind}/workspace/${name}`,'retained original '+name);write(`${kind}/checkpoint/files/${name}`,'retained original '+name);files[name]=row.hash;}
    const workspace=path.join(root,kind,'workspace');
    const gates=baseContract.required(kind).map(([stage,id])=>({stage,id,required:true,outcome:'pass'}));
    const score=write(`${kind}/scorecard.json`,{schema:'vcp-practical-scenario/1',scenario:kind==='A'?'a-vue-taskboard':'b-aspnet-inventory',verdict:'pass',reused_project:false,dry_run:false,skipped_stages:0,stages:baseContract.stages[kind].map((stage,n)=>({stage,task:`task-${n===5?4:n}`,session:'session',exit_code:n===4?8:0,accepted_exit:n===4?[8]:[0]})),gates,required_gates:gates.length,required_passed:gates.length,required_failed:0,workspace,started:'2026-01-01T00:00:00Z',finished:'2026-01-01T02:00:00Z'});
    const checkpoint=write(`${kind}/checkpoint/manifest.json`,{schema:'vcp-source-checkpoint/1',message:'FINAL: verified state',workspace,at:'2026-01-01T01:00:00Z',files,file_count:Object.keys(files).length});
    spec[kind]={scorecard:score.file,scorecard_sha256:score.hash,checkpoint:checkpoint.file,checkpoint_sha256:checkpoint.hash};
  }
  return {root,spec,write};
}
test('strict passing fresh A+B and actual candidate bytes produce a safe copy and whitelisted identity',()=>{
  const {root,spec}=evidence();const proof=verify(spec);
  assert(!JSON.stringify(proof).includes('MUST NOT COPY'));
  const destination=path.join(root,'copy');copy(proof.A,destination);
  assert.equal(sha(fs.readFileSync(path.join(destination,'tests/health.test.ts'))),proof.A.protected['tests/health.test.ts']);
  assert.throws(()=>copy(proof.A,destination),/must be new/);
});
test('missing, failed, skipped or reused base refuses qualification even if verdict claims pass',()=>{
  for(const patch of [{reused_project:true},{dry_run:true},{skipped_stages:1},{required_failed:1},{stages:[]},{fatal:'failed'}]) {
    const {spec}=evidence();const file=spec.B.scorecard;const value={...JSON.parse(fs.readFileSync(file)),...patch};fs.writeFileSync(file,JSON.stringify(value));spec.B.scorecard_sha256=sha(fs.readFileSync(file));assert.throws(()=>verify(spec));
  }
});
test('source/protected/checkpoint hash drift and mismatched build receipt fail closed',()=>{
  const a=evidence();fs.appendFileSync(path.join(a.root,'A/workspace/tests/health.test.ts'),'changed');assert.throws(()=>verify(a.spec),/Passing workspace has changed/);
  const b=evidence();fs.appendFileSync(path.join(b.root,'B/checkpoint/files/README.md'),'changed');assert.throws(()=>verify(b.spec),/Checkpoint bytes differ/);
  const c=evidence();const receipt=path.join(c.root,'candidate/build-evidence.json');const value=JSON.parse(fs.readFileSync(receipt));value.status='building';fs.writeFileSync(receipt,JSON.stringify(value));assert.throws(()=>verify(c.spec),/not complete/);
  value.status='built';fs.writeFileSync(receipt,JSON.stringify(value));assert.equal(verify(c.spec).candidate.build_status,'built');
});
test('claimed pass with missing original gate or missing native execution evidence is rejected',()=>{
  for(const missing of ['migration-script','api.movements','jsonl','explicit-pause']) {
    const {spec}=evidence(),file=spec.B.scorecard,value=JSON.parse(fs.readFileSync(file));
    value.gates=value.gates.filter(g=>g.id!==missing);value.required_gates=value.required_passed=value.gates.length;
    fs.writeFileSync(file,JSON.stringify(value));spec.B.scorecard_sha256=sha(fs.readFileSync(file));assert.throws(()=>verify(spec),/Missing original acceptance gate/);
  }
});
test('declared fault is one bounded source splice, retains original bytes, rejects protected or escaping targets',()=>{
  const {root,write}=evidence();const workspace=path.join(root,'fault-workspace');const before='if (document.schemaVersion !== 1) return invalid();';write('fault-workspace/server/import.ts',before);
  const plan={schema:'vcp-engagement-declared-fault/1',check:'unsupported-import-version',path:'server/import.ts',before,after:'if (false) return invalid();'};
  const declaration=write('declaration.json',plan).file;
  const receipt=inject('A',workspace,declaration,path.join(root,'fault-evidence'));
  assert.equal(receipt.before_sha256,sha(before));assert.equal(fs.readFileSync(path.join(root,'fault-evidence/original-source'),'utf8'),before);
  plan.path='../server/import.ts';fs.writeFileSync(declaration,JSON.stringify(plan));assert.throws(()=>inject('A',workspace,declaration,path.join(root,'unsafe')));
});
