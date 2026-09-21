// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const {load, prepare, grade, gradeAnswer} = require('../../../scripts/evals/p6-task-quality.cjs');
const pool = load();
// Independently authored accepted responses; these do not come from production routing.
const good = {
  'tuning-analysis':{dependencies:[{path:'./view.mjs',line:3},{path:'./sum.mjs',line:1}]},
  'heldout-analysis':{dependencies:[{path:'./load.mjs',line:2}]},
  'tuning-review':{findings:[{path:'lookup.mjs',line:3,kind:'boundary'}]},
  'heldout-review':{findings:[{path:'permitted.mjs',line:2,kind:'coercion'}]},
  'tuning-generation':{type:'object',properties:{count:{type:'integer',minimum:0,maximum:3}},required:['count'],additionalProperties:false},
  'heldout-generation':{type:'object',properties:{offset:{type:'integer',minimum:-2,maximum:2}},required:['offset'],additionalProperties:false}
};
const verdict = (id, answer) => gradeAnswer(pool.cases.find(c => c.id === id), pool.labels[id], answer);
const submission = () => ({revision:pool.manifest.revision,manifest_sha256:pool.manifest_sha256,runs:prepare(pool).runs.map(r => ({case_id:r.case_id,strategy:r.strategy,start_state_sha256:r.start_state_sha256,status:'completed',answer:structuredClone(good[r.case_id])}))});
test('frozen pool has each task class per partition and matched states across arms', () => {
  for (const partition of ['tuning','held_out']) assert.deepEqual(pool.cases.filter(c => c.partition === partition).map(c => c.class).sort(), ['analysis','generation','review']);
  const planned = prepare(pool);
  assert.equal(planned.runs.length,18);
  assert.equal(planned.budget_authorized,false);
  assert.equal(planned.model_calls,0);
  for (const c of pool.cases) {
    const rows = planned.runs.filter(r => r.case_id === c.id);
    assert.equal(new Set(rows.map(r => r.start_state_sha256)).size,1);
    assert.ok(rows.every(r => r.status === 'not_run' && r.actual_cost_micros === null && r.task_success === null));
  }
  assert.equal(JSON.stringify(planned).includes('probes'),false);
});
test('independent accepted responses pass all six behavioral/evidence rubrics', () => {
  for (const [id, answer] of Object.entries(good)) assert.equal(verdict(id,answer).pass,true,id);
  const report = grade(submission(),pool);
  assert.ok(report.summaries.every(s => s.passed === 3 && s.unsuccessful === 0));
  assert.equal(report.live_qualification,'not_run');
  assert.ok(report.runs.every(r => r.actual_cost_micros === null && r.latency_ms === null));
});
test('analysis rejects omitted, transitive, duplicate and wrong-line evidence', () => {
  for (const dependencies of [[],[{path:'./number.mjs',line:1}], [...good['heldout-analysis'].dependencies,...good['heldout-analysis'].dependencies],[{path:'./load.mjs',line:1}]]) {
    assert.equal(verdict('heldout-analysis',{dependencies}).pass,false);
  }
});
test('review rejects serious misses, false positives and malformed evidence', () => {
  for (const answer of [{findings:[]},{findings:[...good['heldout-review'].findings,{path:'permitted.mjs',line:1,kind:'boundary'}]},{findings:[{path:'permitted.mjs',line:2,kind:'boundary'}]},{findings:[{path:'permitted.mjs',line:2,kind:'coercion',confidence:1}]},null]) {
    assert.equal(verdict('heldout-review',answer).pass,false);
  }
});
test('generation catches strict/inclusive boundary, type and extra-property regressions', () => {
  for (const [field,value] of [['minimum',-1],['minimum',-3],['maximum',1],['maximum',3],['type','number']]) {
    const answer = structuredClone(good['heldout-generation']);
    answer.properties.offset[field] = value;
    assert.equal(verdict('heldout-generation',answer).pass,false,field + '=' + value);
  }
  for (const change of [s => {s.additionalProperties=true;},s => {s.required=[];},s => {s.properties.offset.minimum=null;},s => {s.$ref='https://invalid.example/schema';},s => {s.properties.offset.maximum=Infinity;}]) {
    const answer = structuredClone(good['heldout-generation']); change(answer);
    assert.equal(verdict('heldout-generation',answer).pass,false);
  }
  assert.equal(verdict('heldout-generation','process.exit(0)').pass,false);
});
test('missing, failed and cancelled attempts stay in denominators despite supplied correct answers', () => {
  const data = submission();
  data.runs[0].status='failed'; data.runs[1].status='cancelled'; data.runs.splice(2,1);
  const report = grade(data,pool);
  assert.equal(report.runs.length,18);
  assert.equal(report.runs.filter(r => r.task_success).length,15);
  assert.equal(report.summaries.reduce((sum,s) => sum+s.planned,0),18);
  assert.equal(report.summaries.reduce((sum,s) => sum+s.unsuccessful,0),3);
  assert.ok(report.summaries.every(s => s.actual_cost_micros === null));
});
test('identity drift, unknown/duplicate arms and unverified cost claims are rejected', () => {
  for (const change of [s => {s.manifest_sha256='0'.repeat(64);},s => {s.runs[0].start_state_sha256='0'.repeat(64);},s => {s.runs[0].strategy='substitute';},s => {s.runs[0].case_id='invented';},s => {s.runs[1]=s.runs[0];},s => {s.runs[0].cost_micros=0;},s => {s.runs[0].status='success';}]) {
    const data=submission(); change(data); assert.throws(() => grade(data,pool));
  }
});
test('prototype keys cannot inject verdicts or broaden generated schema behavior', () => {
  const answer = JSON.parse('{"type":"object","properties":{"__proto__":{"type":"integer","minimum":-2,"maximum":2}},"required":["__proto__"],"additionalProperties":false}');
  assert.equal(verdict('heldout-generation',answer).pass,false);
  const data = submission();
  data.runs[0].answer = JSON.parse('{"dependencies":[],"__proto__":{"pass":true}}');
  assert.equal(grade(data,pool).runs[0].task_success,false);
  assert.equal({}.pass,undefined);
});
test('CLI emits only an unqualified report and rejects oversized input before parsing', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(),'vcp-p6-grader-'));
  const input = path.join(directory,'answers.json');
  const runner = path.resolve(__dirname,'../../../scripts/evals/p6-task-quality.cjs');
  try {
    fs.writeFileSync(input,JSON.stringify({revision:pool.manifest.revision,manifest_sha256:pool.manifest_sha256,runs:[]}));
    const result = spawnSync(process.execPath,[runner,'grade',input],{encoding:'utf8',timeout:10000,maxBuffer:128*1024});
    assert.equal(result.status,0,result.stderr);
    assert.equal(JSON.parse(result.stdout).live_qualification,'not_run');
    assert.deepEqual(fs.readdirSync(directory),['answers.json']);
    fs.writeFileSync(input,Buffer.alloc(1024*1024+1,32));
    const oversized = spawnSync(process.execPath,[runner,'grade',input],{encoding:'utf8',timeout:10000,maxBuffer:128*1024});
    assert.equal(oversized.status,1);
    assert.match(oversized.stderr,/exceeds 1 MiB/);
    assert.equal(oversized.stdout,'');
  } finally { fs.unlinkSync(input); fs.rmdirSync(directory); }
});
