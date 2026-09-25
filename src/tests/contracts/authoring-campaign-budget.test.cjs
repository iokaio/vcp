// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { limits, verifyPriorCampaign, admitSlot } = require('../../../scripts/evals/authoring-campaign-budget.cjs');
const { priorCampaignFixture } = require('../support/authoring-prior-campaign.cjs');
const verify = fixture => verifyPriorCampaign(fixture.reference, fixture.buffers);
const prior = () => verify(priorCampaignFixture());
const row = (index, cost = 1, requests = 1) => ({ id: 'new-slot-' + index, task_id: 'new-task-' + index, status: 'completed', actual_cost_micros: cost, observed_attempts: requests, active_micros: 0, unresolved_micros: 0 });
function rehashAudit(fixture, mutate) {
  const audit = JSON.parse(fixture.buffers.accounting); mutate(audit);
  fixture.buffers.accounting = Buffer.from(JSON.stringify(audit));
  fixture.reference.accounting_sha256 = crypto.createHash('sha256').update(fixture.buffers.accounting).digest('hex');
  return fixture;
}

test('prior audit binds exact result and halt bytes without confusing native failure with unknown cost', () => {
  const verified = prior();
  assert.equal(verified.actual_cost_micros, 123408); assert.equal(verified.observed_attempts, 47);
  assert.equal(verified.prior_task_ids.length, 6); assert.ok(Object.isFrozen(verified));
  assert.deepEqual(admitSlot(verified, []), { actual_cost_micros: 123408, observed_attempts: 47, remaining_micros: 161876592, remaining_requests: 817, reserved_micros: 3000000, reserved_requests: 16, admitted_cost_micros: 3123408, admitted_requests: 63 });
});
test('each immutable byte input must match its pinned hash', () => {
  for (const key of ['accounting', 'result', 'halt']) {
    const fixture = priorCampaignFixture(); fixture.buffers[key] = Buffer.concat([fixture.buffers[key], Buffer.from(' ')]);
    assert.throws(() => verify(fixture), /hash differs/);
  }
  const fixture = priorCampaignFixture(); fixture.reference.halt_sha256 = null;
  assert.throws(() => verify(fixture), /hash differs/);
});
test('rehashing cannot hide mismatched prior campaign, result, halt or row identities', () => {
  for (const mutate of [a => { a.phase_result_sha256 = 'f'.repeat(64); }, a => { a.halt_sha256 = 'f'.repeat(64); }, a => { a.envelope_sha256 = 'f'.repeat(64); }, a => { a.phase_sha256 = 'f'.repeat(64); }, a => { a.halt.reason = 'Other halt'; }, a => { a.rows[0].task_id = 'other-task'; }, a => { a.rows[0].workspace_sha256 = 'f'.repeat(64); }]) {
    assert.throws(() => verify(rehashAudit(priorCampaignFixture(), mutate)), /differ|halt|evidence/);
  }
});
test('unknown liabilities, overruns and incomplete preservation are rejected even in hash-bound receipts', () => {
  for (const mutate of [({ audit }) => { audit.active_micros = 1; }, ({ audit }) => { audit.unresolved_micros = null; }, ({ audit }) => { delete audit.active_micros; }, ({ audit }) => { audit.overrun = true; }, ({ audit }) => { audit.rows[0].unresolved_micros = '1'; }, ({ audit }) => { audit.rows[0].active_micros = '00'; }, ({ audit }) => { audit.rows[0].overrun = true; }, ({ audit }) => { audit.rows[0].preservation_verified = false; }, ({ result }) => { result.final_inputs_unchanged = false; }, ({ result }) => { result.stopped = true; }]) {
    assert.throws(() => verify(priorCampaignFixture(mutate)), /liability|incomplete/);
  }
});
test('prior costs and request closures agree at row and aggregate levels', () => {
  for (const mutate of [({ audit }) => { audit.rows[0].actual_cost_micros = null; }, ({ result }) => { result.runs[0].actual_cost_micros++; }, ({ audit }) => { audit.cost_micros++; }, ({ audit }) => { audit.requests--; }, ({ audit }) => { audit.rows[0].released_requests++; }, ({ audit }) => { audit.settled_requests--; }, ({ result }) => { result.observed_attempts--; }, ({ audit }) => { audit.rows[0].requests = 17; }, ({ audit }) => { audit.rows[0].actual_cost_micros = 3000001; }]) {
    assert.throws(() => verify(priorCampaignFixture(mutate)), /accounting|liabilities/);
  }
  const fixture = priorCampaignFixture(); fixture.reference.actual_cost_micros = 0;
  assert.throws(() => verify(fixture), /authorized history/);
});
test('prior duplicate or omitted rows and invalid halt are rejected', () => {
  for (const mutate of [({ audit }) => { audit.rows.pop(); }, ({ result }) => { result.runs.pop(); }, ({ audit }) => { audit.rows[1] = { ...audit.rows[0] }; }, ({ halt }) => { halt.action = 'May resume'; }]) assert.throws(() => verify(priorCampaignFixture(mutate)));
});
test('cumulative dollars reserve a full slot: exact equality admitted, one micro more denied', () => {
  const rows = Array.from({ length: 52 }, (_, i) => row(i, 3000000, 0)); rows.push(row(52, 2876592, 0));
  assert.equal(admitSlot(prior(), rows).admitted_cost_micros, 162000000);
  rows[52].actual_cost_micros++;
  assert.throws(() => admitSlot(prior(), rows), /Cumulative USD/);
});
test('cumulative requests include prior 47: exact equality admitted, one request more denied', () => {
  const rows = Array.from({ length: 50 }, (_, i) => row(i, 0, 16)); rows.push(row(50, 0, 1));
  assert.equal(admitSlot(prior(), rows).admitted_requests, 864);
  rows[50].observed_attempts++;
  assert.throws(() => admitSlot(prior(), rows), /Cumulative 864/);
});
test('all settled new rows count, including failures and rows from current partial phase', () => {
  const rows = [row(0, 30, 2), { ...row(1, 70, 4), status: 'failed' }];
  const result = admitSlot(prior(), rows);
  assert.equal(result.actual_cost_micros, 123508); assert.equal(result.observed_attempts, 53);
});
test('admission rejects unknown, unresolved, active, overrun, oversized and unexecuted new rows', () => {
  for (const mutation of [{ actual_cost_micros: null }, { actual_cost_micros: -1 }, { actual_cost_micros: 3000001 }, { actual_cost_micros: Number.MAX_SAFE_INTEGER + 1 }, { actual_cost_micros: '1' }, { observed_attempts: null }, { observed_attempts: 17 }, { observed_attempts: 0.5 }, { active_micros: 1 }, { unresolved_micros: 1 }, { active_micros: undefined }, { overrun: true }, { overrun: null }, { overrun: 'false' }, { status: 'not_run' }]) assert.throws(() => admitSlot(prior(), [{ ...row(0), ...mutation }]));
});
test('fresh campaign slot names may repeat old names; canonical tasks may not be reused', () => {
  assert.doesNotThrow(() => admitSlot(prior(), [{ ...row(0), id: 'old-slot-0' }]));
  assert.throws(() => admitSlot(prior(), [{ ...row(0), task_id: 'old-task-0' }]), /repeats/);
  assert.throws(() => admitSlot(prior(), [row(0), row(0)]), /repeats/);
  assert.throws(() => admitSlot(prior(), [row(0), { ...row(1), task_id: 'new-task-0' }]), /repeats/);
  assert.throws(() => admitSlot(prior(), Array.from({ length: 54 }, (_, i) => row(i, 0, 0))), /No unused/);
});
test('admission cannot silently reset prior cost or request history', () => {
  for (const mutation of [{ actual_cost_micros: 0 }, { observed_attempts: 0 }, { prior_task_ids: [] }]) assert.throws(() => admitSlot({ ...prior(), ...mutation }, []), /Verified prior/);
  assert.ok(Object.isFrozen(limits));
});
