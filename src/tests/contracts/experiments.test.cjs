// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { FakeClock, ScriptedProvider, ownedRoot, EffectLog } = require('../support/experiments.cjs');
const workloads = require('../fixtures/workloads.json');
const sourceRoot = path.resolve(__dirname, '../../..');
const { gradeAnalysis, gradeReview, gradeLabel } = require('../support/workload-graders.cjs');
test('clock gives stable ordering, cancellation, and bounded zero-delay recursion', () => {
  const clock = new FakeClock(), events = [];
  clock.schedule(10, () => events.push(['first', clock.now()]));
  clock.schedule(10, () => events.push(['second', clock.now()]));
  clock.schedule(5, () => events.push(['early', clock.now()]));
  clock.schedule(4, () => events.push(['cancelled', clock.now()]))();
  clock.advance(20);
  assert.deepEqual(events, [['early', 5], ['first', 10], ['second', 10]]);
  assert.equal(clock.now(), 20);
  assert.throws(() => clock.advance(-1));
  function loop() { clock.schedule(0, loop); }
  loop(); assert.throws(() => clock.advance(0), /Non-quiescent/);
});
test('scripted provider rejects extra/mismatched calls without disclosing request contents', () => {
  const provider = new ScriptedProvider([{ expect: { role: 'coding', prompt: 'synthetic' }, response: { text: 'read receipt.cjs' } }]);
  assert.throws(() => provider.assertConsumed());
  assert.throws(() => provider.request({ prompt: 'synthetic-secret' }), error => !error.message.includes('synthetic-secret'));
  assert.equal(provider.requests, 0);
  assert.deepEqual(provider.request({ role: 'coding', prompt: 'synthetic' }), { text: 'read receipt.cjs' });
  provider.assertConsumed();
  assert.throws(() => provider.request({}), /exhaustion/);
});
test('workspace roots are independent and refuse cleanup after marker substitution', () => {
  const first = ownedRoot(os.tmpdir()), second = ownedRoot(os.tmpdir());
  try {
    fs.writeFileSync(path.join(first.root, 'user-edit'), 'preserve');
    assert.notEqual(first.root, second.root);
    assert.equal(fs.existsSync(path.join(second.root, 'user-edit')), false);
    const marker = path.join(first.container, 'owner.json'), saved = fs.readFileSync(marker);
    fs.writeFileSync(marker, '{}');
    assert.throws(() => first.cleanup(), /marker mismatch/);
    assert.equal(fs.readFileSync(path.join(first.root, 'user-edit'), 'utf8'), 'preserve');
    fs.writeFileSync(marker, saved);
  } finally { first.cleanup(); second.cleanup(); }
});
test('supervisor observations remain ordered independently of workspace content', () => {
  const fixture = ownedRoot(os.tmpdir());
  const file = path.join(fixture.container, 'effects.jsonl'), log = new EffectLog(file);
  try {
    log.record('barrier', 'before-dispatch'); log.record('effect', 'marker-created');
    log.record('acknowledgement', 'receipt-returned'); log.close();
    const records = fs.readFileSync(file, 'utf8').trim().split('\n').map(JSON.parse);
    assert.deepEqual(records.map(r => r.sequence), [1, 2, 3]);
    assert.equal(fs.existsSync(path.join(fixture.root, 'effects.jsonl')), false);
    assert.throws(() => log.record('effect', 'after-close'));
  } finally { log.close(); fixture.cleanup(); }
});
test('analysis and review fixture retain documented structure and the intentional threshold defect', () => {
  assert.deepEqual(workloads.cases.map(c => c.acceptance), ['U01', 'U02', 'U03']);
  const { receiptTotal } = require('../fixtures/repositories/checkout/receipt.cjs');
  assert.equal(receiptTotal(49), 54);
  assert.equal(receiptTotal(51), 51);
  assert.equal(receiptTotal(50), 55); // Known defect; required corrected outcome is 50.
  assert.match(fs.readFileSync(path.join(sourceRoot, workloads.model_visible_root, 'receipt.cjs'), 'utf8'), /require\('\.\/shipping.cjs'\)/);
});
test('independent workload graders reject missing analysis, noisy review and incorrect generation', () => {
  assert.deepEqual(gradeAnalysis({ modules: ['receipt.cjs', 'shipping.cjs'], dependencies: [{ from: 'receipt.cjs', to: 'shipping.cjs' }], domainDoesIO: false }), []);
  assert.equal(gradeAnalysis({}).length, 3);
  const finding = { path: 'shipping.cjs', trigger: 50, expectedFee: 0, actualFee: 5 };
  assert.deepEqual(gradeReview([finding]), { truePositives: 1, falsePositives: 0, duplicates: 0, recall: 1, precision: 1 });
  assert.equal(gradeReview([finding, finding, { path: 'receipt.cjs' }]).precision, 1 / 3);
  assert.equal(gradeReview([]).recall, 0);
  assert.deepEqual(gradeLabel(input => {
    if (typeof input !== 'string' || !input.trim()) throw Error('Invalid label');
    return input.trim();
  }), []);
  assert.ok(gradeLabel(input => String(input).trim().toLowerCase()).length > 0);
});
