// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { audit } = require('../../../scripts/evals/developer-reconcile.cjs');
const { runEvidence } = require('../../../scripts/evals/developer-runner.cjs');
const { identity } = require('../../../scripts/evals/authoring-prepare.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const write = (file, value) => fs.writeFileSync(file, JSON.stringify(value));

function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const directory = owner.root, authorization = 'a'.repeat(64), claim = { plan_sha256: authorization, directory };
  fs.mkdirSync(path.join(directory, 'claims'));
  const blockClaim = { ...claim, block: 'llm-integration' };
  write(path.join(directory, 'claims/block-llm-integration.json'), blockClaim);
  write(path.join(directory, 'active-block.json'), blockClaim);
  const runs = Array.from({ length: 54 }, (_, index) => {
    const id = index === 7 ? 'LLM-boundary-partial-v3--none' : `case-${index}--none`;
    const row = { id, case_id: id.split('--')[0], arm: 'none', block: ['llm-integration', 'mcp-development', 'frontend-design'][Math.floor(index / 18)], prompt_sha256: sha('fixture'), profile: { maximum_autonomy: 'plan', affected_paths: [] }, files: [], directories: [], cap_micros: 3000000, call_ceiling: 16 };
    const base = path.join(directory, id);
    fs.mkdirSync(base); fs.mkdirSync(path.join(base, 'workspace')); fs.mkdirSync(path.join(base, 'data'));
    fs.writeFileSync(path.join(base, 'prompt.txt'), 'fixture'); write(path.join(base, 'profile.json'), row.profile);
    return row;
  });
  const reports = runs.slice(0, 18).map((row, index) => {
    const base = path.join(directory, row.id), report = { id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null };
    if (index < 8) {
      write(path.join(directory, 'claims', row.id + '.json'), { ...claim, run: row.id });
      write(path.join(base, 'attempted.json'), { ...claim, args: [] });
      write(path.join(base, 'costs.json'), [{ gaps: [], items: [
        { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '100', overrun: false } },
        { collection: 'attempt', visibility: 'available', record: { id: row.id, phase: 'settled', role: 'main', charged: '100' } },
        { collection: 'settlement', visibility: 'available', record: { attempt: row.id, applied: true, observation: { final_usage: {} } } },
      ] }]);
      Object.assign(report, { status: index === 7 ? 'failed' : 'completed', actual_cost_micros: 100, observed_attempts: 1, preserved: true, canary_disclosed: false, skill_evidence: { checked_attempts: 1 }, conditions: { completed: index !== 7 }, native_check: { status: index === 7 ? 'not_run' : 'passed' }, evidence_sha256: runEvidence(base), workspace_sha256: identity(path.join(base, 'workspace'), ['.']).content_sha256 });
    } else if (index === 8) Object.assign(report, { status: 'failed', reason: 'Campaign halted: reconciliation only; no further dispatch' });
    if (index <= 8) write(path.join(base, 'result.json'), report);
    return report;
  });
  const plan = { directory, runs }, result = { schema: 'cs-2-developer-block-result/1', plan_sha256: authorization, block: 'llm-integration', stopped: true, final_inputs_unchanged: true, actual_cost_micros: 800, observed_attempts: 8, runs: reports };
  const check = () => audit(plan, authorization, result, Buffer.from(JSON.stringify(result)));
  return { plan, result, check, base: index => path.join(directory, runs[index].id), claim };
}

test('reconciliation preserves failed eighth and leaves rejected ninth among 46 unrun slots without writes', t => {
  const f = fixture(t), before = identity(f.plan.directory, ['.']);
  const result = f.check();
  assert.equal(result.retained.length, 8); assert.equal(result.pending.length, 46);
  assert.equal(result.retained[7].report.status, 'failed');
  assert.equal(result.pending[0].id, f.result.runs[8].id);
  assert.equal(result.actual_cost_micros, 800); assert.equal(result.observed_attempts, 8);
  assert.deepEqual(identity(f.plan.directory, ['.']), before);
});

test('reconciliation refuses omitted or extra execution claims', t => {
  const f = fixture(t);
  write(path.join(f.plan.directory, 'claims', f.plan.runs[8].id + '.json'), { ...f.claim, run: f.plan.runs[8].id });
  assert.throws(f.check, /Exactly eight/);
});

test('reconciliation refuses changed retained workspace and canonical unknown liability', t => {
  const f = fixture(t), file = path.join(f.base(0), 'costs.json'), costs = JSON.parse(fs.readFileSync(file));
  costs[0].items[0].record.unresolved = '1'; write(file, costs);
  f.result.runs[0].evidence_sha256 = runEvidence(f.base(0)); write(path.join(f.base(0), 'result.json'), f.result.runs[0]);
  assert.throws(f.check, /unknown liability/);
  costs[0].items[0].record.unresolved = '0'; write(file, costs);
  f.result.runs[0].evidence_sha256 = runEvidence(f.base(0)); write(path.join(f.base(0), 'result.json'), f.result.runs[0]);
  fs.writeFileSync(path.join(f.base(0), 'workspace', 'extra'), 'changed');
  assert.throws(f.check, /workspace changed/);
});

test('reconciliation refuses laundering the failed eighth into success', t => {
  const f = fixture(t); f.result.runs[7].status = 'completed';
  write(path.join(f.base(7), 'result.json'), f.result.runs[7]);
  assert.throws(f.check, /failed eighth outcome/);
});

test('reconciliation refuses hidden execution evidence for an unclaimed slot', t => {
  const f = fixture(t); write(path.join(f.base(8), 'attempted.json'), f.claim);
  assert.throws(f.check, /Unclaimed predecessor run/);
});

test('reconciliation rejects orphan accounting files even without a dispatch marker', t => {
  const f = fixture(t); write(path.join(f.base(20), 'costs.json'), []);
  assert.throws(f.check, /unexpected retained evidence/);
});
