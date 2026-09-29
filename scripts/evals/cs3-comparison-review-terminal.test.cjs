// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function fixture(t, consumed = 18) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-terminal-review-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const control = path.join(directory, 'control'); fs.mkdirSync(control);
  const skill = 'skill-authoring', tasks = Array.from({ length: 6 }, (_, i) => ({ id: 'task-' + i, skill, kind: i < 2 ? 'normal' : 'boundary', request: 'Synthetic request', files: [] }));
  const runs = tasks.flatMap(task => ['none', 'nearest', 'candidate'].map(arm => ({ id: task.id + '--' + arm, case_id: task.id, skill, arm, cap_micros: 600000, call_ceiling: 16 })));
  const plan = { schema: 'cs3-comparison-isolated-plan/1', directory, control_directory: control, isolated: { skill }, successor: {}, spec: { successor: {}, web_evidence: [] }, runs };
  const put = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
  const planFile = path.join(control, 'plan.json'); put(planFile, plan); const hash = sha(fs.readFileSync(planFile));
  const fields = { actual_cost_micros: 37, known_settled_micros: 37, conservative_debit_micros: 37, unresolved_liability_micros: 0, unresolved_attempts: 0 };
  const reports = runs.map(row => ({ ...row, ...fields, observed_attempts: 1, status: 'completed', preserved: true, textual: { passed: true }, evidence_sha256: 'evidence-' + row.id }));
  for (const row of reports) { const base = path.join(directory, row.id); fs.mkdirSync(base); put(path.join(base, 'result.json'), row); put(path.join(base, 'answer.json'), { response: 'Synthetic answer' }); }
  put(path.join(control, 'result-' + skill + '.json'), { plan_sha256: hash, skill, stopped: false, runs: reports, observed_attempts: 18,
    ...Object.fromEntries(Object.entries(fields).map(([key, value]) => [key, value * 18])) });
  const state = { proof: { skill, plan_sha256: hash, claimed_ids: runs.slice(0, consumed).map(row => row.id), reason: 'supplied_synthetic_canary', no_unresolved_execution_effects: true }, unsafe: false, global: 0 };
  const filename = require.resolve('./cs3-comparison-review.cjs'), actualRequire = require('node:module').createRequire(filename), module = { exports: {} };
  // Test-only dependency substitution; the public production API has no override.
  const campaign = { validate(value, supplied) { assert.equal(supplied, hash); assert(equal(value, plan)); return value; }, controlDirectory: value => value.control_directory,
    slotReport: (value, id) => JSON.parse(fs.readFileSync(path.join(value.directory, id, 'result.json'))), cohort: () => tasks, planTasks: () => tasks, conservative: value => value.successor || value.remediation?.accounting };
  const policy = { fields: value => Object.fromEntries(Object.keys(fields).map(key => [key, value[key]])), reread: () => ({ ...fields, attempts: [{}] }) };
  const isolation = { validateTerminal: () => structuredClone(state.proof), validateReaderTerminal: () => { if (state.unsafe) throw Error('Unresolved execution integrity'); return structuredClone(state.proof); }, globalHalt: () => { state.global++; } };
  const boundaries = { read: file => fs.readFileSync(file), write: (file, value) => fs.writeFileSync(file, JSON.stringify(value), { flag: 'wx' }), plain: file => file,
    within: (parent, child) => child === parent || child.startsWith(parent + path.sep), noParentInstructions() {}, privateDirectory() {} };
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, name => {
    if (name === './cs3-comparison.cjs') return campaign;
    if (name === './developer-runner.cjs') return { runEvidence: base => 'evidence-' + path.basename(base) };
    if (name === './cs3-comparison-policy.cjs') return policy;
    if (name === './cs3-comparison-isolated.cjs') return isolation;
    if (name === './p6-live-runner.cjs') return { boundaries };
    return actualRequire(name);
  }, module, filename, path.dirname(filename));
  function reviews(securityFailure) {
    const destination = path.join(directory, '..', path.basename(directory) + '-readers'); t.after(() => fs.rmSync(destination, { recursive: true, force: true }));
    module.exports.prepare(planFile, hash, skill, destination);
    const mappings = JSON.parse(fs.readFileSync(path.join(destination, 'private-mappings.json')));
    const files = mappings.bindings.map(binding => {
      const packet = JSON.parse(fs.readFileSync(path.join(destination, 'reader-' + binding.reader + '.json')));
      const rows = packet.rows.map(row => { const runId = binding.mapping.find(item => item.opaque === row.id).run_id;
        return { id: row.id, ...Object.fromEntries(module.exports.gates.map(g => [g, !(securityFailure && g === 'secret_handling' && runId === runs[0].id)])),
          completeness: runId.endsWith('--candidate') ? 3 : 1, clarity: 3, usefulness: runId.endsWith('--candidate') ? 3 : 1, reason: 'Synthetic independent evaluation' }; });
      const file = path.join(destination, 'grade-' + binding.reader + '.json'); put(file, { reader: binding.reader, reviewer_id: 'reader-' + binding.reader, independent_blind: true, packet_sha256: binding.packet_sha256, rows }); return file;
    });
    return { destination, files };
  }
  return { directory, control, skill, runs, planFile, hash, state, review: module.exports, reviews };
}
test('partial synthetic-canary terminal is unqualified, exact-prefix-bound, immutable and has no fabricated readers', t => {
  const f = fixture(t, 2), result = f.review.terminal(f.planFile, f.hash);
  assert.equal(result.status, 'terminal_unqualified'); assert.equal(result.candidate_qualified, false); assert.equal(result.independent_blind_readers, 0);
  assert.equal(result.claimed_ids.length, 2); assert.equal(result.undispatched_ids.length, 16); assert.equal(result.model_calls, 0);
  assert.deepEqual(f.review.validateTerminalDisposition(f.planFile, f.hash), result);
  assert.throws(() => f.review.terminal(f.planFile, f.hash), /EEXIST/);
  f.state.proof.no_unresolved_execution_effects = false;
  assert.throws(() => f.review.validateTerminalDisposition(f.planFile, f.hash), /Authenticated/);
  assert.equal(f.state.global, 0);
});
test('terminal disposition refuses foreign scope, non-prefix assignments and rewritten outcome', t => {
  const f = fixture(t, 2), original = structuredClone(f.state.proof);
  for (const change of [p => p.plan_sha256 = 'other', p => p.skill = 'other', p => p.reason = 'unknown_integrity', p => p.claimed_ids.reverse(), p => p.claimed_ids = []]) {
    f.state.proof = structuredClone(original); change(f.state.proof); assert.throws(() => f.review.terminal(f.planFile, f.hash), /Authenticated/);
  }
  f.state.proof = original; f.review.terminal(f.planFile, f.hash);
  const file = path.join(f.control, 'terminal-disposition-' + f.skill + '.json'), result = JSON.parse(fs.readFileSync(file)); result.status = 'qualified'; fs.writeFileSync(file, JSON.stringify(result));
  assert.throws(() => f.review.validateTerminalDisposition(f.planFile, f.hash), /changed/);
});
test('complete isolated reader security failure preserves two readers and becomes local unqualified, never promoted', t => {
  const f = fixture(t), readers = f.reviews(true), result = f.review.settle(f.planFile, f.hash, f.skill, readers.destination, ...readers.files);
  assert.equal(result.status, 'unqualified'); assert.equal(result.candidate_hard_gates, true); assert.equal(result.block_security_failure, true); assert.equal(result.candidate_security_failure, undefined);
  assert.deepEqual(result.security_failures, ['A', 'B'].map(reader => ({ run_id: f.runs[0].id, arm: 'none', reader, failed_gates: ['secret_handling'] })));
  const halt = JSON.parse(fs.readFileSync(path.join(f.control, 'halt.json'))); assert.equal(halt.block_security_failure, true); assert.equal(halt.candidate_security_failure, undefined); assert.deepEqual(halt.security_failures, result.security_failures);
  assert.equal(result.independent_blind_readers, 2); assert.equal(result.readers.length, 2); assert.equal(f.state.global, 0);
  assert.deepEqual(f.review.validateDisposition(f.planFile, f.hash, f.skill), result);
  fs.unlinkSync(path.join(f.control, 'halt.json'));
  assert.throws(() => f.review.validateDisposition(f.planFile, f.hash, f.skill), /halt is missing/);
  assert.equal(fs.existsSync(path.join(f.control, 'halt.json')), false);
});
test('reader isolation cannot waive unresolved execution integrity or mutate evidence during reread', t => {
  const f = fixture(t), readers = f.reviews(true); f.state.unsafe = true;
  assert.throws(() => f.review.settle(f.planFile, f.hash, f.skill, readers.destination, ...readers.files), /execution integrity/);
  assert.equal(f.state.global, 1); assert.equal(fs.existsSync(path.join(f.control, 'disposition-' + f.skill + '.json')), false);
  assert.throws(() => f.review.settle(f.planFile, f.hash, f.skill, readers.destination, ...readers.files, undefined, true), /execution integrity/);
  assert.equal(f.state.global, 1);
});
test('ordinary complete two-reader qualification retains its unchanged hard gates', t => {
  const f = fixture(t), readers = f.reviews(false), result = f.review.settle(f.planFile, f.hash, f.skill, readers.destination, ...readers.files);
  assert.equal(result.status, 'qualified'); assert.equal(result.independent_blind_readers, 2); assert.equal(result.block_security_failure, undefined); assert.equal(result.security_failures, undefined);
  assert.equal(fs.existsSync(path.join(f.control, 'halt.json')), false); assert.deepEqual(f.review.validateDisposition(f.planFile, f.hash, f.skill), result);
});
