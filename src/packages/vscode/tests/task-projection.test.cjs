// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { TaskProjection } = require('../dist/task_projection.js');
const scope = { workspace: 'workspace', session: 'session' };
const cut = '9007199254740993';
function task(id = 'root', state = 'running', extra = {}) {
  return { scope, task: id, root: 'root', ...(id === 'root' ? {} : { parent: 'root' }), state, effects: 'pending', reason: 'last canonical observation', revision: cut, steering_revision: '3', pending_inputs: [], ...extra };
}
function snapshot(tasks = [task()], extra = {}) {
  return { session: { scope, revision: '1', configuration_revision: '2' }, watermark: cut, sequence: cut, event_cursor: 'initial', subscription: 'subscription', tasks, complete: true, ...extra };
}
function event(sequence, taskId = 'root', extra = {}) {
  return { id: `event-${sequence}`, sequence: String(sequence), scope, task: taskId, timestamp_ms: '1', kind: 'task.updated', schema_version: '1', evidence: [], evidence_complete: true, redacted: false, ...extra };
}
function batch(events, extra = {}) {
  return { subscription: 'subscription', cursor: 'next', snapshot_sequence: events.at(-1)?.sequence ?? cut, events, at_end: true, ...extra };
}

test('snapshot pages retain exact cut and canonical root/child status with big counters', () => {
  const projection = new TaskProjection(scope);
  const waiting = task('waiting', 'waiting_for_input', { pending_inputs: [{ id: 'question', kind: 'question', revision: cut, operation_digest: 'private-digest' }] });
  projection.appendSnapshot(snapshot([waiting], { complete: false, next_cursor: 'page2' }));
  assert.equal(projection.view().phase, 'partial');
  projection.appendSnapshot(snapshot([task(), task('failed', 'failed')]));
  const view = projection.view();
  assert.equal(view.phase, 'current'); assert.equal(view.sequence, cut); assert.equal(view.needsResync, false);
  assert.deepEqual(view.rows.map(row => row.task), ['root', 'failed', 'waiting']);
  assert.equal(view.rows[2].root, 'root'); assert.equal(view.rows[2].parent, 'root');
  assert.deepEqual(view.rows[2].pendingInputs, [{ id: 'question', kind: 'question', revision: cut }]);
  assert.ok(!JSON.stringify(view).includes('private-digest'));
  view.rows[0].state = 'completed'; assert.equal(projection.view().rows[0].state, 'running');
});

test('reject changed cuts, cross-scope, duplicate task, cycles, missing ancestry without partial commit', () => {
  for (const change of [
    page => { page.watermark = '9'; },
    page => { page.session.revision = '9'; },
    page => { page.subscription = 'other'; },
    page => { page.tasks[0].scope = { ...scope, workspace: 'other' }; },
    page => { page.tasks = [task('child')]; },
    page => { page.tasks = [task(), task('cycle-a', 'running', { parent: 'cycle-b' }), task('cycle-b', 'running', { parent: 'cycle-a' })]; },
    page => { page.tasks = [task(), task('missing', 'running', { parent: 'absent' })]; },
  ]) {
    const projection = new TaskProjection(scope);
    projection.appendSnapshot(snapshot([task('child')], { complete: false, next_cursor: 'second' }));
    const page = snapshot(); change(page);
    assert.throws(() => projection.appendSnapshot(page));
    assert.equal(projection.view().phase, 'gap'); assert.deepEqual(projection.view().rows.map(row => row.task), ['child']);
  }
});

test('scoped ordered event metadata invalidates canonical status without inferring completion', () => {
  const projection = new TaskProjection(scope); projection.appendSnapshot(snapshot());
  projection.applyEvents(batch([event(BigInt(cut) + 2n, 'root', { outcome: 'succeeded', kind: 'task.completed', evidence: [{ secret: 'never-serialize' }] })]));
  let view = projection.view();
  assert.equal(view.phase, 'stale'); assert.equal(view.rows[0].state, 'running'); assert.equal(view.rows[0].dirty, true);
  assert.equal(view.rows[0].events[0].sequence, '9007199254740995');
  assert.ok(!JSON.stringify(view).includes('never-serialize'));
  projection.disconnect(); view = projection.view();
  assert.equal(view.phase, 'disconnected'); assert.equal(view.rows[0].state, 'running'); assert.equal(view.needsResync, true);
});

test('at-end empty batch remains current and a gap explicitly requires a fresh snapshot', () => {
  const projection = new TaskProjection(scope); projection.appendSnapshot(snapshot());
  projection.applyEvents(batch([]));
  projection.applyEvents(batch([])); // A repeated empty caught-up cursor is not a gap.
  assert.equal(projection.view().phase, 'current'); assert.equal(projection.view().rows[0].state, 'running');
  projection.markGap(); assert.equal(projection.view().phase, 'gap');
  assert.throws(() => projection.applyEvents(batch([], { cursor: 'later' })));
  assert.throws(() => projection.appendSnapshot(snapshot()));
});

test('reject hostile event order/scope/subscription/counter and preserve previous rows atomically', () => {
  for (const bad of [
    batch([event(BigInt(cut) + 1n), event(BigInt(cut) + 1n)]),
    batch([event(BigInt(cut) + 1n, 'root', { scope: { ...scope, session: 'other' } })]),
    batch([event(BigInt(cut) + 1n)], { subscription: 'other' }),
    batch([event(BigInt(cut) + 1n)], { cursor: 'initial' }),
    batch([event(BigInt(cut) + 1n)], { snapshot_sequence: cut }),
    batch([event(BigInt(cut) + 1n, 'root', { sequence: '18446744073709551616' })]),
    batch([event(BigInt(cut) + 1n), event(BigInt(cut) + 2n, 'root', { id: `event-${BigInt(cut) + 1n}` })]),
    batch(Array.from({ length: 129 }, (_, index) => event(BigInt(cut) + BigInt(index + 1)))),
  ]) {
    const projection = new TaskProjection(scope); projection.appendSnapshot(snapshot());
    const before = projection.view().rows;
    assert.throws(() => projection.applyEvents(bad));
    assert.equal(projection.view().phase, 'gap'); assert.deepEqual(projection.view().rows, before);
  }
});

test('noisy child cannot evict sibling waiting/failed status or fill their event summaries', () => {
  const projection = new TaskProjection(scope);
  projection.appendSnapshot(snapshot([task(), task('noisy'), task('waiting', 'waiting_for_input'), task('failed', 'failed')]));
  for (let index = 1n; index <= 50n; index++) projection.applyEvents(batch([event(BigInt(cut) + index, 'noisy')], { cursor: `cursor${index}` }));
  const view = projection.view();
  assert.equal(view.total, 4); assert.equal(view.stateCounts.failed, 1); assert.equal(view.stateCounts.waiting_for_input, 1);
  assert.equal(view.rows.find(row => row.task === 'noisy').events.length, 8);
  for (const name of ['failed', 'waiting']) { const row = view.rows.find(row => row.task === name); assert.equal(row.dirty, false); assert.equal(row.events.length, 0); }
  assert.equal(projection.view(0, 2).hasMore, true); assert.equal(projection.view(2, 2).hasMore, false);
  assert.throws(() => projection.view(-1)); assert.throws(() => projection.view(0, 101));
});

test('paginated editor projection preserves the canonical semantic task trace', () => {
  const trace = [task(), task('worker', 'paused'), task('reviewer', 'failed'), task('approval', 'waiting_for_input', { pending_inputs: [{ id: 'reconcile', kind: 'reconciliation', revision: '5' }] })];
  const editor = new TaskProjection(scope);
  editor.appendSnapshot(snapshot(trace.slice(2), { complete: false, next_cursor: 'second' }));
  editor.appendSnapshot(snapshot(trace.slice(0, 2)));
  const semantics = row => ({ task: row.task, root: row.root, state: row.state, effects: row.effects, revision: row.revision });
  const expected = trace.map(semantics).sort((a, b) => a.task < b.task ? -1 : 1);
  assert.deepEqual(editor.view().rows.map(semantics).sort((a, b) => a.task < b.task ? -1 : 1), expected);
});

test('oversized pages, invalid counters and repeated pagination are bounded failures', () => {
  for (const page of [snapshot(Array.from({ length: 129 }, (_, index) => task(`child${index}`))), snapshot([task('root', 'running', { revision: '01' })])]) {
    const projection = new TaskProjection(scope); assert.throws(() => projection.appendSnapshot(page)); assert.equal(projection.view().total, 0);
  }
  const projection = new TaskProjection(scope);
  projection.appendSnapshot(snapshot([], { complete: false, next_cursor: 'same' }));
  assert.throws(() => projection.appendSnapshot(snapshot([], { complete: false, next_cursor: 'same' })));
  const emptyPages = new TaskProjection(scope);
  for (let page = 0; page < 256; page++) emptyPages.appendSnapshot(snapshot([], { complete: false, next_cursor: `page${page}` }));
  assert.throws(() => emptyPages.appendSnapshot(snapshot([], { complete: false, next_cursor: 'unbounded' })));
});

test('unknown-task event requires resnapshot without inventing a new child', () => {
  const projection = new TaskProjection(scope); projection.appendSnapshot(snapshot());
  projection.applyEvents(batch([event(BigInt(cut) + 1n, 'new-child')]));
  assert.equal(projection.view().phase, 'stale'); assert.equal(projection.view().total, 1);
  assert.equal(projection.view().rows[0].state, 'running');
});
