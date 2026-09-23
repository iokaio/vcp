// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const integration=require('../../../scripts/evals/p805-owner-integration.cjs');
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
function temporary(t){const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-p805-integration-test-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));return root;}
function put(root,name,value){const file=path.join(root,name);fs.writeFileSync(file,JSON.stringify(value));return file;}

test('preview oracle requires exact selected/dependent IDs and compares protected IDs across multiple reasons',()=>{
  const selected={kind:'record',id:'artifact:a'},protectedTarget={kind:'event',id:'e'};
  const expected={selected:[selected],dependent:[],protected:[protectedTarget]};
  const actual=[{class:'selected',target:selected},{class:'protected',target:protectedTarget,reason:'active recovery'},{class:'protected',target:protectedTarget,reason:'capture unfinished'}];
  integration.assertPreview(expected,actual);
  assert.throws(()=>integration.assertPreview(expected,actual.slice(1)),/selected ID oracle/);
  assert.throws(()=>integration.assertPreview(expected,[...actual,{class:'dependent',target:selected}]),/dependent ID oracle/);
  assert.throws(()=>integration.assertPreview(expected,[...actual,{class:'selected',target:selected}]),/selected ID oracle/);
  assert.throws(()=>integration.assertPreview(expected,actual.filter(row=>row.class!=='protected')),/protected ID oracle/);
});

test('retention refusal requires an ordinary expected exit and the exact boundary diagnosis',()=>{
  const diagnostic='stale preview; create a new preview';
  integration.assertRefusal({status:2,stderr:'vcp: conflict: '+diagnostic},diagnostic);
  for(const result of [{status:null,stderr:diagnostic},{status:1,stderr:diagnostic},{status:2,stderr:'configuration is unavailable'},{status:0,stderr:diagnostic}])assert.throws(()=>integration.assertRefusal(result,diagnostic),/refusal diagnostic/);
});

test('purge protection does not silently change exclude or compact expected selections',()=>{
  const target={kind:'event',id:'e'},oracle={selected:[target],dependent:[],protected_if_purge:[target]};
  assert.deepEqual(integration.retentionExpected(oracle,'purge'),{selected:[target],dependent:[],protected:[target]});
  for(const action of ['exclude','compact'])assert.deepEqual(integration.retentionExpected(oracle,action),{selected:[target],dependent:[],protected:[]});
});

test('overlapping output roots are rejected before creating or inventorying original owner data',async t=>{
  const root=temporary(t),data=path.join(root,'data'),workspace=path.join(root,'workspace');fs.mkdirSync(data);fs.mkdirSync(workspace);
  const spec=put(root,'spec.json',{schema:'p805-integrated-history-spec/1',rows:[{data,workspace}]});
  const initial=fs.readdirSync(root).sort();
  for(const destination of [path.join(data,'new-evidence'),path.join(workspace,'new-evidence'),root]){
    await assert.rejects(integration.run(spec,destination,process.execPath),/disjoint from every original/);
    assert.deepEqual(fs.readdirSync(root).sort(),initial);assert.deepEqual(fs.readdirSync(data),[]);assert.deepEqual(fs.readdirSync(workspace),[]);
  }
});

test('integration preparation binds exact owner results/package and excludes slots without accepted tasks',t=>{
  const root=temporary(t),pkg=put(root,'package.json',{archive_sha256:'a'.repeat(64)});
  const plan=put(root,'plan.json',{package_sha256:'a'.repeat(64),runs:[{id:'u01-sqlite',workspace:'workspace',data:'data',backend:'sqlite',profile:'profile',profile_sha256:'b'.repeat(64)},{id:'u01-files'}]});
  const result=put(root,'owner-result.json',{schema:'p805-owner-result/1',plan_sha256:sha(fs.readFileSync(plan)),runs:[{id:'u01-sqlite',scope:{task:'accepted-task'},status:'failed'},{id:'u01-files',status:'not_run'}]});
  const output=path.join(root,'spec.json');const prepared=integration.prepare(plan,result,pkg,output),spec=JSON.parse(fs.readFileSync(output));
  assert.equal(prepared.rows,1);assert.equal(spec.rows[0].owner_status,'failed');assert.equal(spec.rows[0].task,'accepted-task');assert.equal(spec.owner_result_sha256,sha(fs.readFileSync(result)));
  assert.throws(()=>integration.prepare(plan,result,pkg,output),/EEXIST/);
  const otherPackage=put(root,'other-package.json',{archive_sha256:'c'.repeat(64)});
  assert.throws(()=>integration.prepare(plan,result,otherPackage,path.join(root,'wrong.json')),/artifact changed/);
  fs.writeFileSync(plan,'{}');assert.throws(()=>integration.prepare(plan,result,pkg,path.join(root,'changed.json')),/owner result binding/);
});

test('changed owner evidence is rejected before any qualification directory is created',async t=>{
  const root=temporary(t),ownerPlan=put(root,'owner-plan.json',{}),ownerResult=put(root,'owner-result.json',{});
  const spec=put(root,'spec.json',{schema:'p805-integrated-history-spec/1',owner_plan:ownerPlan,owner_plan_sha256:sha(fs.readFileSync(ownerPlan)),owner_result:ownerResult,owner_result_sha256:sha(fs.readFileSync(ownerResult)),rows:[{}]});
  fs.writeFileSync(ownerResult,'{"changed":true}');const output=path.join(root,'new-evidence');
  await assert.rejects(integration.run(spec,output,process.execPath),/Owner plan\/result evidence changed/);assert.equal(fs.existsSync(output),false);
});

test('retention-only reuse binds unchanged canonical cut, original inventories and complete raw bytes',t=>{
  const root=temporary(t),row={id:'u01-sqlite',task:'task',backend:'sqlite'},base=path.join(root,row.id);fs.mkdirSync(base);
  const bytes=Buffer.from('complete output beyond a summary'),digest=sha(bytes),data=[{path:'canonical',sha256:'a'}],workspace=[{path:'source',sha256:'b'}];
  const current={records:{'artifact:output':{collection:'artifact',value:{spec:{id:'output',scope:{task:'task'},channel:'response'},state:'complete',sha256:digest,length:String(bytes.length)}}},events:[]};
  for(const name of ['original-data-before.json','original-data-after.json'])put(base,name,data);
  for(const name of ['original-workspace-before.json','original-workspace-after.json'])put(base,name,workspace);
  for(const name of ['independent-before.json','independent-after-read.json'])put(base,name,{state:current});
  const raw=path.join(base,'raw-output.bin');fs.writeFileSync(raw,bytes);
  const receipt=value=>({status:0,error:null,process_reaped:true,stdout:JSON.stringify({type:'result',exit_code:0,data:value})+'\n'});
  put(base,'0001-retention-default.json',receipt({automatic:null}));put(base,'0002-full-history-0.json',receipt({rows:[],gaps:[],next_cursor:null}));
  const report={rows:[{...row,status:'read-only-observations-passed-retention-not-run',originals_preserved:true,checks:['history-pagination-independent-event-ids','default-no-delete-and-full-output-digests'].map(name=>({name,status:'passed'})),full_outputs:[{id:'output',sha256:digest,bytes:bytes.length}]}]};
  const previous=put(root,'result.json',report),reused=integration.reuseObservations(previous,row,current,data,workspace);
  assert.equal(reused.bindings.length,9);assert.ok(reused.bindings.some(r=>r.file===raw&&r.sha256===digest));
  assert.throws(()=>integration.reuseObservations(previous,row,current,[{changed:true}],workspace),/inventory no longer matches/);
  assert.throws(()=>integration.reuseObservations(previous,row,{...current,events:[{changed:true}]},data,workspace),/canonical observation cut/);
  fs.writeFileSync(raw,Buffer.alloc(bytes.length));assert.throws(()=>integration.reuseObservations(previous,row,current,data,workspace),/raw output bytes changed/);fs.writeFileSync(raw,bytes);
  const history=path.join(base,'0002-full-history-0.json');fs.writeFileSync(history,JSON.stringify(receipt({rows:[],gaps:[{missing:true}],next_cursor:null})));assert.throws(()=>integration.reuseObservations(previous,row,current,data,workspace),/history receipt event IDs differ/);fs.writeFileSync(history,JSON.stringify(receipt({rows:[],gaps:[],next_cursor:null})));
  report.rows[0].checks[0].status='failed';fs.writeFileSync(previous,JSON.stringify(report));assert.throws(()=>integration.reuseObservations(previous,row,current,data,workspace),/only completed successful observations/);
  report.rows[0].checks[0].status='passed';report.rows[0].checks.push({name:'other-gate',status:'failed'});fs.writeFileSync(previous,JSON.stringify(report));assert.throws(()=>integration.reuseObservations(previous,row,current,data,workspace),/only completed successful observations/);
  report.rows[0].checks.pop();report.rows[0].status='failed';fs.writeFileSync(previous,JSON.stringify(report));assert.throws(()=>integration.reuseObservations(previous,row,current,data,workspace),/only completed successful observations/);
});

test('retention-only row selection rejects undeclared, duplicate or unbound rows before creating output',async t=>{
  const root=temporary(t),spec=put(root,'spec.json',{schema:'p805-integrated-history-spec/1',rows:[{id:'u01-sqlite'}]});
  for(const [previous,subset] of [[undefined,['u01-sqlite']],['prior',['unknown']],['prior',['u01-sqlite','u01-sqlite']],['prior',[]]]){
    const destination=path.join(root,'new-evidence');await assert.rejects(integration.run(spec,destination,process.execPath,previous,subset),/Explicit unique existing row subset/);assert.equal(fs.existsSync(destination),false);
  }
});
