// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const oracle=require('../../../scripts/evals/builtin-debug-oracle.cjs');
function workspace(t,id){const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-cr06-'));t.after(()=>oracle.removeTemporary(root));const directory=path.join(root,'workspace');oracle.prepare(id,directory);return directory;}
test('temporary cleanup refuses the parent and unrelated directories',()=>{
  assert.throws(()=>oracle.removeTemporary(os.tmpdir()),/Refusing cleanup/);
  assert.throws(()=>oracle.removeTemporary(__dirname),/Refusing cleanup/);
});
test('missing reproduction remains not-run while external preservation checks still apply',t=>{
  const directory=workspace(t,'missing-reproduction-access');
  const observed=oracle.grade('missing-reproduction-access',directory);
  assert.equal(observed.controls_pass,true);assert.equal(observed.verification_complete,false);assert.equal(observed.current.status,'not_run');assert.equal(observed.initial.status,'not_run');
  fs.writeFileSync(path.join(directory,'notes.txt'),'lost human work');
  assert.equal(oracle.grade('missing-reproduction-access',directory).controls_pass,false);
});
test('external debug checks retain failures and reject leftover instrumentation or overwritten human edits',t=>{
  const help=require('node:child_process').spawnSync(process.execPath,['--help'],{env:{},encoding:'utf8'});
  if(!help.stdout.includes('--allow-net'))return t.skip('Requires qualified network-denying Node runtime');
  for(const id of ['seeded-failure','interrupted-instrumentation','concurrent-human-edit']){
    const directory=workspace(t,id),source=path.join(directory,'shipping.cjs');
    assert.equal(oracle.grade(id,directory).controls_pass,false);
    let fixed=fs.readFileSync(source,'utf8').replace('subtotal > 50','subtotal >= 50');
    fs.writeFileSync(source,fixed);
    if(id==='interrupted-instrumentation'){
      const retained=oracle.grade(id,directory);assert.equal(retained.current.status,'passed');assert.equal(retained.controls_pass,false);
      fixed=fixed.replace("console.warn('CR06_OWNED_TRACE', subtotal); ",'');fs.writeFileSync(source,fixed);
    }
    const observed=oracle.grade(id,directory);assert.equal(observed.controls_pass,true,JSON.stringify(observed));assert.equal(observed.initial.status,'failed');assert.equal(observed.current.status,'passed');assert.equal(observed.live_usefulness,'not_run');
    if(id==='concurrent-human-edit'){
      assert.equal(observed.cleanup,'blocked_by_concurrent_human_edit');
      fs.writeFileSync(source,fixed.replace(oracle.definition(id).scenario.retained_line,''));
      assert.equal(oracle.grade(id,directory).controls_pass,false);
    }
  }
});
