// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'), assert=require('node:assert/strict');
const reporter=require('../../../scripts/evals/p6-profile-qualification.cjs');
const quality=require('../../../scripts/evals/p6-task-quality.cjs');
function failedReceipts(pool) {
  return {schema:'p6-live-result/1',manifest_sha256:pool.manifest_sha256,runs:quality.prepare(pool).runs.map(r=>({case_id:r.case_id,strategy:r.strategy,start_state_sha256:r.start_state_sha256,status:'failed',answer:null,actual_cost_micros:7,latency_ms:20}))};
}
test('one-sided exact success bound matches independently known all-success and zero-success cases',()=>{
  assert.equal(reporter.lowerBound(0,30),0);
  assert.ok(Math.abs(reporter.lowerBound(30,30)-Math.pow(0.05,1/30))<1e-12);
  assert.ok(reporter.lowerBound(29,30)<reporter.lowerBound(30,30));
  for(const [passed,planned] of [[31,30],[-1,30],[1,0],[1,10001],[1.5,30]]) assert.throws(()=>reporter.lowerBound(passed,planned));
});
test('nearest-rank latency retains small sample limitations and does not mutate measurements',()=>{
  const samples=[40,10,30,20];
  assert.equal(reporter.percentile(samples,0.5),20);
  assert.equal(reporter.percentile(samples,0.95),40);
  assert.equal(reporter.percentile([],0.95),null);
  assert.deepEqual(samples,[40,10,30,20]);
});
test('failed, missing, and unknown-cost runs remain in frozen denominators and cannot qualify defaults',()=>{
  const pool=quality.load('p6-task-quality-v3'),result=failedReceipts(pool);
  result.runs[0].status='not_run';result.runs[0].actual_cost_micros=null;result.runs[0].latency_ms=null;
  const report=reporter.summarize(result,pool);
  assert.equal(report.decision,'reject_automatic_default_activation');
  assert.deepEqual(report.enabled_defaults,[]);
  const first=report.summaries.find(r=>r.strategy===result.runs[0].strategy && r.partition==='tuning');
  assert.equal(first.planned,9);assert.equal(first.passed,0);assert.equal(first.unsuccessful,9);
  assert.equal(first.actual_cost_micros,null);assert.equal(first.latency_observations,8);
  assert.ok(first.rejection_reasons.includes('incomplete attributable cost'));
  assert.ok(report.summaries.every(r=>r.activation_eligible===false && r.classes.length===3));
  result.manifest_sha256='0'.repeat(64);
  assert.throws(()=>reporter.summarize(result,pool),/identity/);
});
