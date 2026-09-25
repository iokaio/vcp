// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const continuation = require('../../../scripts/evals/authoring-continuation.cjs');
const prep = require('../../../scripts/evals/authoring-prepare.cjs');
const budget = require('../../../scripts/evals/authoring-campaign-budget.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
// The predecessor validator is a deliberate test double. Actual canonical
// ledger/review checks are covered by authoring-followup tests and the retained
// live predecessor; these tests exercise continuation exclusion and binding.
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-continuation-unit-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const repository = path.join(root, 'repository'), directory = path.join(root, 'campaign');
  fs.mkdirSync(path.join(repository, 'scripts/evals'), { recursive: true });
  const moduleFile = path.join(repository, 'scripts/evals/authoring-followup.cjs');
  fs.writeFileSync(moduleFile, `const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
exports.sourceScope=['scripts/evals/authoring-followup.cjs'];
exports.envelopeFor=()=>{};
exports.derivePlan=(e)=>{const file=path.join(e.directory,'phases/document-authoring--normal/review-gate.json');return {prerequisites:[{file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')}]};};`);
  const phase = path.join(directory, 'phases/document-authoring--normal');
  fs.mkdirSync(phase, { recursive: true }); fs.mkdirSync(path.join(directory, 'claims'));
  const prior = { reference: { envelope_sha256: 'a'.repeat(64) }, accounting: { actual_cost_micros: 123408, observed_attempts: 47, prior_task_ids: Array.from({ length: 6 }, (_, i) => `prior-${i}`) } };
  const runs = Array.from({ length: 6 }, (_, i) => ({ id: `run-${i}`, status: i % 2 ? 'completed' : 'failed', scope: { task: `current-${i}` }, actual_cost_micros: 100, observed_attempts: 2 }));
  for (const row of runs) save(path.join(directory, 'claims', row.id + '.json'), {});
  const resultFile = path.join(phase, 'result.json'); save(resultFile, { runs, stopped: false, final_inputs_unchanged: true });
  const result_sha256 = sha(fs.readFileSync(resultFile));
  const gateFile = path.join(phase, 'review-gate.json'); save(gateFile, { result_sha256, decision: { candidate_gates_pass: false, winning_case_ids: [] } });
  const envelope = path.join(directory, 'envelope.json');
  const spec_source = path.join(root, 'spec.json'); save(spec_source, { synthetic: true });
  save(envelope, { spec_source, spec_sha256: sha(fs.readFileSync(spec_source)), directory, execution_mode: 'full_diagnostic', prior_campaign: prior, source: prep.identity(repository, ['scripts/evals/authoring-followup.cjs']) });
  const reference = { envelope, envelope_sha256: sha(fs.readFileSync(envelope)), repository, gate_sha256: sha(fs.readFileSync(gateFile)), result_sha256 };
  return { root, directory, repository, reference, prior, moduleFile, resultFile, gateFile };
}
test('seal preserves failed normal gate and accounts for all six consumed rows', t => {
  const f = fixture(t), result = continuation.seal(f.reference, f.prior);
  assert.equal(result.rows.length, 6); assert.equal(result.gate.decision.candidate_gates_pass, false);
  assert.deepEqual(continuation.verify(f.reference, f.prior), result);
  const admission = budget.admitSlot(f.prior.accounting, result.rows);
  assert.equal(admission.actual_cost_micros, 124008); assert.equal(admission.observed_attempts, 59);
  assert.throws(() => continuation.seal(f.reference, f.prior), /active or already halted/);
});
test('active predecessor cannot be sealed or authorized concurrently', t => {
  const f = fixture(t); save(path.join(f.directory, 'active-phase.json'), { phase_sha256: 'b'.repeat(64) });
  assert.throws(() => continuation.seal(f.reference, f.prior), /active or already halted/);
  assert.equal(fs.existsSync(path.join(f.directory, 'halt.json')), false);
});
test('unsealed predecessor and altered source cannot supply a continuation', t => {
  const f = fixture(t); assert.throws(() => continuation.verify(f.reference, f.prior));
  continuation.seal(f.reference, f.prior); fs.appendFileSync(f.moduleFile, '\n// drift');
  assert.throws(() => continuation.verify(f.reference, f.prior), /source identity changed/);
});
for (const field of ['resultFile', 'gateFile']) test(`changed ${field} invalidates continuation`, t => {
  const f = fixture(t); continuation.seal(f.reference, f.prior); fs.appendFileSync(f[field], ' ');
  assert.throws(() => continuation.verify(f.reference, f.prior), /hash differs/);
});
test('omitted claims and an extra phase cannot disappear from the budget chain', t => {
  const f = fixture(t); save(path.join(f.directory, 'claims/extra.json'), {});
  assert.throws(() => continuation.seal(f.reference, f.prior), /omitted execution claims/);
  fs.unlinkSync(path.join(f.directory, 'claims/extra.json'));
  fs.mkdirSync(path.join(f.directory, 'phases/skill-authoring--normal'));
  assert.throws(() => continuation.seal(f.reference, f.prior), /Only the six/);
});
test('original liability and permanent retirement cannot be replaced', t => {
  const f = fixture(t); continuation.seal(f.reference, f.prior);
  assert.throws(() => continuation.verify(f.reference, { ...f.prior, accounting: {} }), /same authenticated/);
  save(path.join(f.directory, 'halt.json'), { reason: 'different' });
  assert.throws(() => continuation.verify(f.reference, f.prior), /retirement changed/);
});
