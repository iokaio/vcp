// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test');const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');const crypto=require('node:crypto');
const {checkedWorkload,arm,report}=require('../../../scripts/evals/p6-decision-comparison.cjs');
const spec=checkedWorkload();
const workload_sha256=crypto.createHash('sha256').update(fs.readFileSync(path.join(__dirname,'../../evals/markov/decision-workload-v1.json'))).digest('hex');
test('frozen remote state omits all labels and human semantic case identifiers',()=>{
  assert.equal(Object.keys(spec.state).length,12);
  for(const [id,state] of Object.entries(spec.state)){
    assert.match(id,/^c\d{2}$/);assert.deepEqual(Object.keys(state).sort(),['after','before','failed_verifications','training','window']);
    assert.equal(JSON.stringify(state).includes('stall_label'),false);
  }
});
test('native Brier and serious missed abstention use original held-out denominators',()=>{
  const answers=Object.fromEntries(Object.keys(spec.questions).map(k=>[k,0.5]));
  const report={status:'observed',workload_sha256,probe_answers:answers,actual_cost_micros:'5',elapsed_ms:10};
  const result=arm(report,true,spec);assert.equal(result.held_out.cases,4);assert.equal(result.held_out.native_brier,0.25);assert.equal(result.qualification.enabled,false);
  delete answers.c00;
  const invalid=arm(report,true,spec);assert.equal(invalid.valid_batch,false);assert.equal(invalid.all_splits.covered,0);assert.ok(invalid.held_out.serious_misses_including_abstention>0);
});
test('conventional booleans cannot acquire native probability calibration',()=>{
  const answers=Object.fromEntries(Object.keys(spec.questions).map(k=>[k,true]));
  const result=arm({status:'observed',workload_sha256,probe_answers:answers,actual_cost_micros:'5'},false,spec);
  assert.equal(result.valid_batch,true);assert.equal(result.held_out.native_brier,null);assert.equal(result.held_out.native_probability_samples,0);
  answers.c00=0.8;assert.equal(arm({status:'observed',workload_sha256,probe_answers:answers},false,spec).valid_batch,false);
});
test('wrong source binding and unknown costs never graduate a purpose',()=>{
  const result=arm({status:'observed',workload_sha256:'0'.repeat(64),probe_answers:{}},true,spec);
  assert.equal(result.valid_batch,false);assert.ok(result.qualification.reasons.includes('unknown_liability'));assert.equal(result.qualification.enabled,false);
});
test('retained local run is bound through its exact historical Windows line endings',()=>{
  const result=report({status:'failed'},{status:'failed'});
  assert.equal(result.local_source_binding.recorded_sha256,'51bedc59eae56aba4340f730c7164eedc7e3981b4c914783e1c4b2414d520dd1');
  assert.equal(result.local_source_binding.current_sha256,spec.source_sha256);
  assert.equal(result.arms.length,4);assert.equal(result.shipping_evaluator_enabled,false);
});
