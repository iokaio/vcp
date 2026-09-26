// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const budget = require('../../../scripts/evals/authoring-qualification-budget.cjs');
const accounting = (cost = 500000) => ({ grant: budget.grant, cap_micros: 100000000, active_micros: 0, unresolved_micros: 0, current: { actual_cost_micros: cost, task_ids: ['cs2-task'] }, history: { task_ids: ['historical-task'], actual_cost_micros: 401397 } });
const row = (index, cost = 1750000) => ({ id: 'slot-' + index, task_id: 'fresh-' + index, status: index % 2 ? 'failed' : 'completed', actual_cost_micros: cost, observed_attempts: 16, active_micros: 0, unresolved_micros: 0 });
test('one cumulative owner grant counts settled CS2 plus successful and failed fresh rows', () => {
  const receipt = budget.admit(accounting(), [row(0, 15), row(1, 35)]);
  assert.equal(receipt.current_authorization_settled_micros, 500050);
  assert.equal(receipt.authoring_settled_micros, 50);
  assert.equal(receipt.historical_micros_not_transferred, 401397);
  assert.equal(receipt.reserved_micros, 1750000); assert.equal(receipt.reserved_requests, 16);
});
test('final slot reserve is bounded both by 94.50 envelope and remaining 100 grant', () => {
  const rows = Array.from({ length: 53 }, (_, index) => row(index));
  assert.equal(budget.admit(accounting(5500000), rows).remaining_micros, 1750000);
  assert.throws(() => budget.admit(accounting(5500001), rows), /reserve/);
  assert.throws(() => budget.admit(accounting(), [...rows, row(53)]), /No unused/);
});
test('unknown active debt, missing accounting, reused history/task IDs and oversized rows fail closed', () => {
  for (const mutate of [value => value.active_micros = 1, value => value.unresolved_micros = 1, value => value.current.actual_cost_micros = null, value => value.grant = 'bootstrap']) { const value = accounting(); mutate(value); assert.throws(() => budget.admit(value, [])); }
  for (const mutate of [value => value.task_id = 'historical-task', value => value.task_id = 'cs2-task', value => value.status = 'not_run', value => value.actual_cost_micros = null, value => value.actual_cost_micros = 1750001, value => value.observed_attempts = 17, value => value.active_micros = 1, value => value.unresolved_micros = 1]) { const value = row(0); mutate(value); assert.throws(() => budget.admit(accounting(), [value])); }
  assert.throws(() => budget.admit(accounting(), [row(0), row(0)]), /Duplicate/);
});
test('historical identities cannot be replaced with new bootstrap receipts', () => {
  assert.throws(() => budget.history([]), /three-phase/);
  assert.throws(() => budget.history(Array.from({ length: 3 }, () => ({ envelope: { sha256: '0'.repeat(64) } }))), /bootstrap/);
});

const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const { ownedRoot } = require('../support/experiments.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
function terminalFixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = owner.root, campaign = path.join(root, 'campaign'), repository = path.join(root, 'frozen');
  fs.mkdirSync(campaign); fs.mkdirSync(repository); fs.mkdirSync(path.join(campaign, 'claims'));
  const blocks = ['llm-integration', 'mcp-development', 'frontend-design'];
  const runs = Array.from({ length: 46 }, (_, index) => ({ id: 'run-' + index, block: blocks[Math.floor(index / 16)], cap_micros: 2000000 }));
  const plan = { schema: 'cs-2-developer-continuation/1', directory: campaign, source: { scope: [] }, limits: { cap_micros: 92000000 }, runs };
  const file = path.join(campaign, 'plan.json'); save(file, plan); const hash = sha(fs.readFileSync(file));
  for (const [index, row] of runs.entries()) {
    const base = path.join(campaign, row.id); fs.mkdirSync(base); save(path.join(campaign, 'claims', row.id + '.json'), {});
    save(path.join(base, 'costs.json'), [{ items: [
      { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '2000000', active: '0', unresolved: '0', settled: '1', overrun: false } },
      { collection: 'attempt', visibility: 'available', record: { id: 'attempt-' + index, phase: 'settled', role: 'main', charged: '1', request_digest: 'request-' + index, provider_request: 'provider-' + index } },
      { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt-' + index, applied: true, observation: { final_usage: true } } }
    ], gaps: [] }]);
  }
  for (const block of blocks) {
    save(path.join(campaign, 'claims', 'block-' + block + '.json'), {});
    save(path.join(campaign, 'decision-' + block + '.json'), { schema: 'cs-2-developer-decision/1', plan_sha256: hash, block, result_sha256: 'a'.repeat(64) });
    save(path.join(campaign, 'result-' + block + '.json'), { runs: runs.filter(row => row.block === block).map((row, index) => ({ id: row.id, scope: { task: 'task-' + row.id }, status: index % 2 ? 'failed' : 'completed', actual_cost_micros: 1, observed_attempts: 1 })) });
  }
  // The historical runner already owns plan/claim/evidence authentication. This
  // synthetic seam isolates terminal closure and our independent ledger sums.
  const filename = path.resolve(__dirname, '../../../scripts/evals/authoring-qualification-budget.cjs'), originalRequire = createRequire(filename), module = { exports: {} };
  const request = id => id === './authoring-prepare.cjs' ? { identity: () => plan.source } : id === path.join(repository, 'scripts/evals/developer-continuation.cjs') ? { identical: () => {}, inspectBlock: () => ({ result_sha256: 'a'.repeat(64) }) } : originalRequire(id);
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, request, module, filename, path.dirname(filename));
  return { api: module.exports, reference: { repository, plan: { file, sha256: hash } }, campaign };
}
test('terminal CS2 aggregates all 46 settled outcomes including failures and rejects active/halted closure', t => {
  const f = terminalFixture(t), receipt = f.api.terminalCs2(f.reference);
  assert.equal(receipt.rows.length, 46); assert.equal(receipt.actual_cost_micros, 46); assert.equal(receipt.observed_attempts, 46);
  assert(receipt.rows.some(row => row.status === 'failed'));
  for (const name of ['active-block.json', 'halt.json']) { const file = path.join(f.campaign, name); save(file, {}); assert.throws(() => f.api.terminalCs2(f.reference), /complete decided/); fs.unlinkSync(file); }
});
test('CS2 closure rejects extra claims, altered decisions and understated settled costs', t => {
  const f = terminalFixture(t), extra = path.join(f.campaign, 'claims', 'unknown.json'); save(extra, {});
  assert.throws(() => f.api.terminalCs2(f.reference), /omitted or extra claims/); fs.unlinkSync(extra);
  const decision = path.join(f.campaign, 'decision-llm-integration.json'), before = fs.readFileSync(decision), changed = JSON.parse(before); changed.result_sha256 = 'b'.repeat(64); save(decision, changed);
  assert.throws(() => f.api.terminalCs2(f.reference), /terminal decision/); fs.writeFileSync(decision, before);
  const result = path.join(f.campaign, 'result-llm-integration.json'), value = JSON.parse(fs.readFileSync(result)); value.runs[0].actual_cost_micros = 0; save(result, value);
  assert.throws(() => f.api.terminalCs2(f.reference), /accounting differs/);
});
