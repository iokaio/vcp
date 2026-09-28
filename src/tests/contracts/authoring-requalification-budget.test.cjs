// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const budget = require('../../../scripts/evals/authoring-requalification-budget.cjs');
const old = () => ({ grant: budget.grant, cap_micros: 100000000, settled_micros: 644378, settled_requests: 342,
  historical_micros_not_transferred: 401397, task_ids: ['historical', 'cs2', 'doc', 'skl-first'], active_micros: 0, unresolved_micros: 0,
  current: { actual_cost_micros: 521696 }, history: { actual_cost_micros: 401397 } });
const consumed = () => [
  { id: 'old-none', task_id: 'old-task-none', status: 'completed', actual_cost_micros: 20000, observed_attempts: 10, active_micros: 0, unresolved_micros: 0 },
  { id: 'old-candidate', task_id: 'old-task-candidate', status: 'failed', actual_cost_micros: 33006, observed_attempts: 11, active_micros: 0, unresolved_micros: 0 }
];
const closed = () => budget.summarize(old(), consumed());
const row = (index, cost = 1750000) => ({ id: 'new-' + index, task_id: 'new-task-' + index, status: index % 2 ? 'failed' : 'completed',
  actual_cost_micros: cost, observed_attempts: 16, active_micros: 0, unresolved_micros: 0 });

test('all current-grant spend includes failed successor while old authorization is authenticated separately', () => {
  const value = closed();
  assert.equal(value.settled_micros, 697384); assert.equal(value.settled_requests, 363);
  assert.equal(value.historical_micros_not_transferred, 401397); assert.equal(value.task_ids.length, 6);
  assert.equal(value.reserved_micros, 94500000); assert.equal(value.reserved_requests, 864);
  assert(value.settled_micros + value.reserved_micros <= value.cap_micros);
});
test('omitted successor rows, wrong totals, unresolved debt and reused task IDs cannot close the grant', () => {
  assert.throws(() => budget.summarize(old(), consumed().slice(0, 1)), /two consumed/);
  for (const change of [r => r.actual_cost_micros--, r => r.observed_attempts--, r => r.active_micros = 1, r => r.unresolved_micros = 1,
    r => r.task_id = 'skl-first', r => r.status = 'not_run']) {
    const rows = consumed(); change(rows[1]); assert.throws(() => budget.summarize(old(), rows));
  }
  for (const change of [b => b.settled_micros++, b => b.settled_requests++, b => b.active_micros++, b => b.unresolved_micros++,
    b => b.historical_micros_not_transferred = 0, b => b.task_ids.push('cs2')]) {
    const value = old(); change(value); assert.throws(() => budget.summarize(value, consumed()));
  }
});
test('next slot admission counts both failed and completed fresh rows under the same grant', () => {
  const receipt = budget.admit(closed(), [row(0, 12), row(1, 18)]);
  assert.equal(receipt.current_authorization_settled_micros, 697414);
  assert.equal(receipt.authoring_settled_micros, 30); assert.equal(receipt.reserved_micros, 1750000);
  assert.equal(receipt.reserved_requests, 16); assert.equal(receipt.authoring_requests, 32);
});
test('54 slots and 864 requests remain the absolute campaign reservation ceiling', () => {
  const rows = Array.from({ length: 53 }, (_, index) => row(index));
  const receipt = budget.admit(closed(), rows);
  assert.equal(receipt.authoring_settled_micros + receipt.reserved_micros, 94500000);
  assert.equal(receipt.authoring_requests + receipt.reserved_requests, 864);
  assert.throws(() => budget.admit(closed(), [...rows, row(53)]), /No unused/);
});
test('new campaign task IDs cannot reuse any historical, CS2, DOC, predecessor or successor scope', () => {
  for (const task_id of closed().task_ids) assert.throws(() => budget.admit(closed(), [{ ...row(0), task_id }]), /Duplicate/);
  assert.throws(() => budget.admit(closed(), [row(0), row(0)]), /Duplicate/);
  assert.throws(() => budget.admit(closed(), [row(0), { ...row(1), task_id: row(0).task_id }]), /Duplicate/);
});
test('missing accounting, active debt, new grants and oversized rows fail closed', () => {
  for (const change of [b => b.settled_micros = null, b => b.settled_micros++, b => b.active_micros++, b => b.unresolved_micros++,
    b => b.grant = 'new-budget', b => b.task_ids.push('cs2')]) {
    const value = closed(); change(value); assert.throws(() => budget.admit(value, []));
  }
  for (const change of [r => r.actual_cost_micros = null, r => r.actual_cost_micros = -1, r => r.actual_cost_micros = 1750001,
    r => r.observed_attempts = 17, r => r.observed_attempts = 1.5, r => r.active_micros = 1, r => r.unresolved_micros = 1]) {
    const value = row(0); change(value); assert.throws(() => budget.admit(closed(), [value]));
  }
});
test('unapproved bootstrap references are rejected before any historical file access', () => {
  assert.throws(() => budget.inspect({}), /references/);
  const reference = { repository: require('node:path').resolve('synthetic'), envelope: {}, plan: {}, result: {}, halt: {} };
  assert.throws(() => budget.inspect({ cs2: {}, successor: reference }), /identity/);
});

const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), os = require('node:os');
const { ownedRoot } = require('../support/experiments.cjs');
test('accounting documents are immutable exact hash references', t => {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const file = path.join(owner.root, 'accounting.json'), bytes = Buffer.from('{"settled":697384}'); fs.writeFileSync(file, bytes);
  const ref = { file, sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  assert.deepEqual(budget.document(ref), { settled: 697384 });
  fs.writeFileSync(file, '{"settled":0}'); assert.throws(() => budget.document(ref), /changed/);
  assert.throws(() => budget.document({ ...ref, extra: true }), /Exact/);
});
test('requalification claim uses a separate durable namespace without changing earlier claim paths', () => {
  const { createRequire } = require('node:module'), filename = path.resolve(__dirname, '../../../scripts/evals/authoring-requalification-budget.cjs');
  const realRequire = createRequire(filename), module = { exports: {} }, common = path.resolve('synthetic-git');
  const request = id => id === './authoring-qualification-budget.cjs' ? { ...realRequire(id), claimFile: () => path.join(common, 'old-claim.json') } : realRequire(id);
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, request, module, filename, path.dirname(filename));
  assert.equal(module.exports.claimFile(), path.join(common, 'vcp-' + budget.grant + '-authoring-requalification-v1.json'));
  assert.notEqual(module.exports.claimFile(), path.join(common, 'old-claim.json'));
});

// Authenticate real synthetic ledger files while isolating the already-tested
// historical validators and pinned production documents behind a require seam.
function inspectionFixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const repository = path.join(owner.root, 'frozen'), directory = path.join(owner.root, 'campaign');
  const phase = path.join(directory, 'phases/skill-authoring--normal'), claims = path.join(directory, 'claims');
  fs.mkdirSync(repository); fs.mkdirSync(phase, { recursive: true }); fs.mkdirSync(claims);
  const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
  const previous = old(); previous.current.reference = { repository: 'historical-cs2', plan: { file: 'historical-plan', sha256: 'a'.repeat(64) } };
  const source = { content_sha256: budget.PIN.source, scope: ['scripts/evals/authoring-skl-continuation.cjs', 'scripts/evals/authoring-skl-reconciliation.cjs'] };
  const commonClaim = path.join(owner.root, 'old-exclusive.json'), oldEnvelope = 'b'.repeat(64);
  const envelope = { schema: 'cs1-skl-continuation-envelope/1', source, directory, budget: previous, common_claim: commonClaim, predecessor: {} };
  const reports = consumed().map(row => ({ ...row, scope: { task: row.task_id } }));
  for (let i = 2; i < 5; i++) reports.push({ id: 'pending-' + i, status: 'not_run' });
  const plan = { directory: phase, envelope_sha256: budget.PIN.envelope, candidate: 'skill-authoring', phase: 'normal', runs: reports.map(row => ({ id: row.id, cap_micros: 1750000 })) };
  const result = { runs: reports, actual_cost_micros: 53006, observed_attempts: 21, stopped: false, candidate_stopped: true, final_inputs_unchanged: true };
  const halt = { schema: 'cs1-skl-continuation-halt/1', reason: 'retained failure', action: 'Read-only reconciliation only. One-shot claim remains consumed.' };
  const claim = { envelope_sha256: budget.PIN.envelope, phase_sha256: budget.PIN.plan };
  save(commonClaim, { schema: 'cs1-skl-one-shot-claim/1', grant: budget.grant, predecessor_envelope_sha256: oldEnvelope, directory, envelope_sha256: budget.PIN.envelope });
  for (const row of reports.slice(0, 2)) {
    save(path.join(claims, row.id + '.json'), claim); const base = path.join(phase, row.id); fs.mkdirSync(base);
    save(path.join(base, 'costs.json'), [{ items: [
      { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '1750000', active: '0', unresolved: '0', settled: String(row.actual_cost_micros), overrun: false } },
      ...Array.from({ length: row.observed_attempts }, (_, index) => [
        { collection: 'attempt', visibility: 'available', record: { id: 'attempt-' + index, phase: 'settled', role: 'main', charged: String(index ? 0 : row.actual_cost_micros), provider_request: 'request-' + index } },
        { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt-' + index, applied: true, observation: { final_usage: true } } }
      ]).flat()
    ], gaps: [] }]);
  }
  const documents = { envelope, plan, result, halt }, successor = { repository };
  for (const name of Object.keys(documents)) successor[name] = { file: path.join(name === 'plan' || name === 'result' ? phase : directory, name + '.json'), sha256: budget.PIN[name] };
  const { createRequire } = require('node:module'), filename = path.resolve(__dirname, '../../../scripts/evals/authoring-requalification-budget.cjs'), real = createRequire(filename), module = { exports: {} };
  let validated = 0, sourceChanged = false;
  const request = id => {
    if (id === './authoring-qualification-budget.cjs') return { ...real(id), document: ref => documents[Object.keys(successor).find(name => successor[name] === ref)] };
    if (id === './authoring-prepare.cjs') return { identity: () => sourceChanged ? {} : source };
    if (id === path.join(repository, 'scripts/evals/authoring-skl-continuation.cjs')) return { sourceBudget: () => previous, claimFile: () => commonClaim, resultEvidence: () => { validated++; } };
    if (id === path.join(repository, 'scripts/evals/authoring-skl-reconciliation.cjs')) return { PIN: { envelope: oldEnvelope }, inspect: () => ({ modules: { runner: { successorClaim: () => commonClaim } }, envelope: { budget: { grant_record: {} } } }) };
    return real(id);
  };
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, request, module, filename, path.dirname(filename));
  return { api: module.exports, input: { cs2: previous.current.reference, successor }, directory, phase, claims, result, reports, save, validated: () => validated, drift: () => { sourceChanged = true; } };
}
test('read-only closure recomputes actual canonical ledgers, retains halted state and authenticates predecessor', t => {
  const f = inspectionFixture(t), result = f.api.inspect(f.input);
  assert.equal(result.settled_micros, 697384); assert.equal(result.qualification, 'failed_or_incomplete_preserved');
  assert.equal(f.validated(), 1); assert(!fs.existsSync(path.join(f.directory, 'active-phase.json')));
  assert.equal(fs.readdirSync(f.claims).length, 2);
});
test('inspection rejects extra claims, missing halt state, source drift and ledger liabilities', t => {
  const f = inspectionFixture(t), extra = path.join(f.claims, 'unexpected.json'); f.save(extra, {});
  assert.throws(() => f.api.inspect(f.input), /Omitted/); fs.unlinkSync(extra);
  f.save(path.join(f.directory, 'active-phase.json'), {}); assert.throws(() => f.api.inspect(f.input), /halt/);
  fs.unlinkSync(path.join(f.directory, 'active-phase.json'));
  const file = path.join(f.phase, f.reports[0].id, 'costs.json'), pages = JSON.parse(fs.readFileSync(file)); pages[0].items[0].record.unresolved = '1'; f.save(file, pages);
  assert.throws(() => f.api.inspect(f.input), /liability/);
  f.drift(); assert.throws(() => f.api.inspect(f.input), /source identity/);
});
