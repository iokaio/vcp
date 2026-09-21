// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { isDeepStrictEqual } = require('node:util');
const root = path.resolve(__dirname, '../..');
const manifestPath = 'src/evals/tasks/p6-task-quality-v1.json';
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const identity = value => hash(JSON.stringify(value));
const object = x => x !== null && typeof x === 'object' && !Array.isArray(x);
function keys(x, expected) {
  return object(x) && isDeepStrictEqual(Object.keys(x).sort(), [...expected].sort());
}
function readBounded(file) {
  const limit = 1024 * 1024;
  const descriptor = fs.openSync(file, 'r');
  try {
    if (!fs.fstatSync(descriptor).isFile()) throw Error('Input must be a regular file');
    const buffer = Buffer.alloc(limit + 1);
    let used = 0;
    while (used < buffer.length) {
      const count = fs.readSync(descriptor, buffer, used, buffer.length - used, null);
      if (count === 0) break;
      used += count;
    }
    if (used > limit) throw Error('Input exceeds 1 MiB');
    return buffer.subarray(0, used);
  } finally { fs.closeSync(descriptor); }
}
function load(revision = 'p6-task-quality-v1') {
  if (!['p6-task-quality-v1', 'p6-task-quality-v2', 'p6-task-quality-v3'].includes(revision)) throw Error('Unknown task manifest revision');
  const manifestBytes = readBounded(path.join(root, revision === 'p6-task-quality-v1' ? manifestPath : `src/evals/tasks/${revision}.json`));
  const manifest = JSON.parse(manifestBytes);
  function bound(entry) {
    const bytes = readBounded(path.join(root, entry.path));
    if (hash(bytes) !== entry.sha256) throw Error('Frozen fixture identity changed: ' + entry.path);
    const value = JSON.parse(bytes);
    if (value.revision !== manifest.revision) throw Error('Fixture revision mismatch');
    return value;
  }
  const inputs = bound(manifest.inputs);
  const labels = bound(manifest.labels);
  const count = revision === 'p6-task-quality-v1' ? 6 : 18;
  if (inputs.cases.length !== count || new Set(inputs.cases.map(c => c.id)).size !== count) throw Error('Invalid case pool');
  for (const c of inputs.cases) {
    if (!Object.hasOwn(labels.labels, c.id)) throw Error('Missing label');
    if (identity({prompt:c.prompt, files:c.files}) !== manifest.start_states[c.id]) throw Error('Starting state changed');
  }
  return { manifest, cases:inputs.cases, labels:labels.labels, manifest_sha256:hash(manifestBytes) };
}
function prepare(pool = load()) {
  return {
    schema:'p6-task-quality-preparation/1', revision:pool.manifest.revision,
    manifest_sha256:pool.manifest_sha256,
    grader_sha256:hash(readBounded(__filename)),
    status:'not_run', budget_authorized:false, model_calls:0,
    reason:'Preparation performs no provider requests or canonical charge import and grants no trial authorization or model qualification.',
    grading_scope:'Small synthetic smoke corpus; no shipping quality or calibration claim',
    runs:pool.cases.flatMap(c => pool.manifest.strategies.map(s => ({
      case_id:c.id, partition:c.partition, task_class:c.class, strategy:s.id,
      start_state_sha256:pool.manifest.start_states[c.id], status:'not_run',
      actual_cost_micros:null, task_success:null, latency_ms:null
    })))
  };
}
// Closed, nonexecuting schema interpreter; model output is never code.
function validateSchema(schema) {
  if (!keys(schema, ['type','properties','required','additionalProperties']) || schema.type !== 'object' ||
      !object(schema.properties) || Object.keys(schema.properties).length !== 1 ||
      !Array.isArray(schema.required) || schema.required.length !== 1 ||
      typeof schema.required[0] !== 'string' || schema.additionalProperties !== false ||
      !Object.hasOwn(schema.properties, schema.required[0])) return false;
  return Object.values(schema.properties).every(p => keys(p, ['type','minimum','maximum']) &&
    p.type === 'integer' && Number.isSafeInteger(p.minimum) && Number.isSafeInteger(p.maximum) && p.minimum <= p.maximum);
}
function accepts(schema, value) {
  if (!object(value)) return false;
  const field = schema.required[0];
  if (!keys(value, [field])) return false;
  const rule = schema.properties[field];
  return Number.isSafeInteger(value[field]) && value[field] >= rule.minimum && value[field] <= rule.maximum;
}
function gradeAnswer(task, label, answer) {
  if (task.class === 'generation') {
    if (task.grading_kind === 'bounded_schema_v2') {
      if (!validV2(answer)) return {pass:false, reason:'invalid_schema'};
      const passed = label.probes.filter(p => acceptsV2(answer, p.input) === p.accept).length;
      return {pass:passed === label.probes.length, reason:passed === label.probes.length ? null : 'behavior_mismatch', checks:label.probes.length, passed_checks:passed};
    }
    if (!validateSchema(answer)) return {pass:false, reason:'invalid_schema', checks:0, passed_checks:0};
    const passed = label.probes.filter(p => accepts(answer, p.input) === p.accept).length;
    return {pass:passed === label.probes.length, reason:passed === label.probes.length ? null : 'behavior_mismatch', checks:label.probes.length, passed_checks:passed};
  }
  const field = task.class === 'analysis' ? 'dependencies' : 'findings';
  const expectedKeys = field === 'dependencies' ? ['path','line'] : ['path','line','kind'];
  if (!keys(answer, [field]) || !Array.isArray(answer[field]) || answer[field].length > 16 ||
      answer[field].some(v => !keys(v, expectedKeys) || typeof v.path !== 'string' || !Number.isSafeInteger(v.line) || v.line < 1 ||
        (field === 'findings' && !['boundary','coercion'].includes(v.kind)))) return {pass:false, reason:'invalid_answer'};
  const normalize = rows => rows.map(r => expectedKeys.map(k => r[k])).sort((a,b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  const pass = isDeepStrictEqual(normalize(answer[field]), normalize(label[field]));
  return {pass, reason:pass ? null : 'missing_extra_duplicate_or_wrong_evidence'};
}
function grade(submission, pool = load()) {
  if (!keys(submission, ['revision','manifest_sha256','runs']) || submission.revision !== pool.manifest.revision ||
      submission.manifest_sha256 !== pool.manifest_sha256 || !Array.isArray(submission.runs) || submission.runs.length > pool.cases.length * pool.manifest.strategies.length) throw Error('Invalid submission identity or shape');
  const planned = prepare(pool).runs;
  const supplied = new Map();
  for (const run of submission.runs) {
    if (!keys(run, ['case_id','strategy','start_state_sha256','status','answer'])) throw Error('Invalid run shape');
    const key = run.case_id + '/' + run.strategy;
    if (!planned.some(p => p.case_id === run.case_id && p.strategy === run.strategy) || supplied.has(key)) throw Error('Unknown or duplicate run');
    if (!['completed','failed','cancelled','not_run'].includes(run.status)) throw Error('Invalid run status');
    if (run.start_state_sha256 !== pool.manifest.start_states[run.case_id]) throw Error('Starting state mismatch');
    supplied.set(key, run);
  }
  const rows = planned.map(p => {
    const run = supplied.get(p.case_id + '/' + p.strategy);
    const task = pool.cases.find(c => c.id === p.case_id);
    const verdict = run?.status === 'completed' ? gradeAnswer(task, pool.labels[task.id], run.answer) : {pass:false, reason:run?.status || 'missing_run'};
    return {...p, status:run?.status || 'not_run', task_success:verdict.pass, grade:verdict};
  });
  return {
    schema:'p6-task-quality-grading/1', revision:pool.manifest.revision, manifest_sha256:pool.manifest_sha256,
    grader_sha256:hash(readBounded(__filename)),
    evidence_kind:'unverified_submitted_answers', live_qualification:'not_run', budget_authorized:false,
    limitation:'Grades supplied answers only. No provider execution, canonical cost, latency, or provenance is established.',
    summaries:pool.manifest.strategies.flatMap(s => ['tuning','held_out'].map(partition => {
      const selected = rows.filter(r => r.strategy === s.id && r.partition === partition);
      return {strategy:s.id, partition, planned:selected.length, passed:selected.filter(r => r.task_success).length,
        unsuccessful:selected.filter(r => !r.task_success).length, actual_cost_micros:null, latency_ms:null};
    })), runs:rows
  };
}
function validV2(s, depth = 0) {
  if (!object(s) || depth > 4 || !['object','array','integer','string','boolean'].includes(s.type)) return false;
  const allowed = {object:['type','properties','required','additionalProperties'], array:['type','items','minItems','maxItems'], integer:['type','minimum','maximum'], string:['type','enum','minLength','maxLength'], boolean:['type']}[s.type];
  if (Object.keys(s).some(k => !allowed.includes(k))) return false;
  if (s.type === 'object') return object(s.properties) && Object.keys(s.properties).length <= 8 && Array.isArray(s.required) && new Set(s.required).size === s.required.length && s.required.every(k => typeof k === 'string' && Object.hasOwn(s.properties,k)) && s.additionalProperties === false && Object.values(s.properties).every(p => validV2(p,depth+1));
  if (s.type === 'array' && !validV2(s.items,depth+1)) return false;
  if (s.enum !== undefined && (!Array.isArray(s.enum) || !s.enum.length || s.enum.length > 16 || s.enum.some(x => typeof x !== 'string'))) return false;
  for (const k of ['minimum','maximum','minItems','maxItems','minLength','maxLength']) if (s[k] !== undefined && (!Number.isSafeInteger(s[k]) || (k !== 'minimum' && k !== 'maximum' && s[k] < 0))) return false;
  for (const [min,max] of [['minimum','maximum'],['minItems','maxItems'],['minLength','maxLength']]) if (s[min] !== undefined && s[max] !== undefined && s[min] > s[max]) return false;
  return true;
}
function acceptsV2(s,v) {
  if (s.type === 'object') return object(v) && Object.keys(v).every(k => Object.hasOwn(s.properties,k)) && s.required.every(k => Object.hasOwn(v,k)) && Object.entries(v).every(([k,x]) => acceptsV2(s.properties[k],x));
  if (s.type === 'array') return Array.isArray(v) && v.length >= (s.minItems ?? 0) && v.length <= (s.maxItems ?? Infinity) && v.every(x => acceptsV2(s.items,x));
  if (s.type === 'integer') return Number.isSafeInteger(v) && v >= (s.minimum ?? -Infinity) && v <= (s.maximum ?? Infinity);
  if (s.type === 'boolean') return typeof v === 'boolean';
  return typeof v === 'string' && (!s.enum || s.enum.includes(v)) && Array.from(v).length >= (s.minLength ?? 0) && Array.from(v).length <= (s.maxLength ?? Infinity);
}
module.exports = {load, prepare, grade, gradeAnswer, identity, validV2, acceptsV2};
if (require.main === module) {
  try {
    const [command, input, ...extra] = process.argv.slice(2);
    if (extra.length || !['prepare','grade'].includes(command) || (command === 'prepare' && input) || (command === 'grade' && !input)) throw Error('Usage: node scripts/evals/p6-task-quality.cjs prepare | grade <answers.json>');
    const result = command === 'prepare' ? prepare() : grade(JSON.parse(readBounded(input)));
    process.stdout.write(JSON.stringify(result, null, 2) + '\n');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
