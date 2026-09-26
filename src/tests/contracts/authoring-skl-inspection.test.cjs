// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), path = require('node:path');
const { createInspection, limits } = require('../../../scripts/evals/authoring-skl-inspection.cjs');
const executable = path.resolve('vcp.exe'), base = path.resolve('capture'), task = 'task-id';
const page = (view = 'outputs', items = [], gaps = [], next_cursor = null) => ({ schema_version: 1, scope: { workspace: 'w', session: 's', task }, view, items, gaps, next_cursor });
const result = value => ({ status: 0, error: null, stdout: JSON.stringify({ type: 'result', data: value }), stderr: '' });
const args = (id = task, view = 'outputs', extra = []) => ['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'),'inspect',id,'--view',view,'--limit','128',...extra];
function fixture(output = value => result(value), clock = { value: 0 }) {
  const invoked = [], saved = [];
  const adapter = createInspection({ executable, base, task, now: () => clock.value, invoke(file, argv, timeout) { invoked.push({ file, argv: [...argv], timeout }); return output(page(argv[10])); }, save(name, value) { saved.push({ name, value }); } });
  return { ...adapter, invoked, saved, clock };
}
test('upgrades only the exact frozen inspect call and retains raw result', () => {
  const f = fixture(); f.call(executable, args(), 30000);
  assert.equal(f.invoked[0].timeout, limits.per_call_ms); assert.equal(f.saved[0].name, 'inspection-001.json');
  for (const bad of [[executable,args().map((v,i)=>i===7?'run':v),30000],[executable,args(),120000],['other.exe',args(),30000]]) assert.throws(()=>f.call(...bad));
});
test('pagination is exact, same-view and bounded', () => {
  let first = true; const f = fixture(() => result(page('outputs', [], [], first ? (first=false,{after:1}) : null)));
  f.call(executable,args(),30000); f.call(executable,args(task,'outputs',['--cursor','{"after":1}']),30000);
  assert.throws(()=>f.call(executable,args(task,'tools',['--cursor','{"after":1}']),30000));
});
test('registered descriptor authorizes only exact ranges and validates returned identity', () => {
  const descriptor={spec:{id:'artifact',scope:{workspace:'w',session:'s',task},channel:'response'},state:'complete',length:'3',sha256:'a'.repeat(64)};
  const item={id:'artifact',collection:'artifact',visibility:'available',record:descriptor}; let ranged=false;
  const f=fixture(value=>{ if(!ranged){ranged=true;return result(page('outputs',[item]));} return result(page('outputs',[{artifact:'artifact',descriptor,visibility:'available',range:{start:0,end:3},bytes:[1,2,3]}])); });
  f.call(executable,args(),30000); f.call(executable,args('artifact','outputs',['--offset','0','--length','65536']),30000);
  assert.throws(()=>f.call(executable,args('artifact','tools',['--offset','0','--length','65536']),30000));
  assert.throws(()=>f.call(executable,args('artifact','outputs',['--offset','1','--length','65536']),30000));
});
test('failed inspection is retained once and cannot be retried', () => {
  const f=fixture(()=>({status:null,error:'ETIMEDOUT',stdout:'',stderr:''})); const first=f.call(executable,args(),30000);
  assert.equal(first.error,'ETIMEDOUT'); assert.equal(f.saved.length,1); assert.throws(()=>f.call(executable,args(),30000),/cannot be retried/);
});
test('deadline, call and output bounds apply before another invocation', () => {
  const clock={value:0}, f=fixture(undefined,clock); clock.value=limits.aggregate_ms-1000; f.call(executable,args(),30000); assert.equal(f.invoked[0].timeout,1000);
  clock.value=limits.aggregate_ms+1; assert.throws(()=>f.call(executable,args(),30000),/deadline/);
});
