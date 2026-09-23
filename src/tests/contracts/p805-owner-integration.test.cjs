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
