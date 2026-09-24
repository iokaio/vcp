// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { TaskSession } = require('../dist/task_session.js');
const scope = { workspace: 'workspace', session: 'session' };
const reference = { artifact: 'artifact', offset: '7', length: '100000', sha256: 'a'.repeat(64) };
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; let reject; const promise = new Promise((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; }
function task(id = 'root') { return { scope, task: id, root: 'root', ...(id === 'root' ? {} : { parent: 'root' }), state: 'running', effects: 'pending', reason: 'engine state', revision: '1', steering_revision: '0', pending_inputs: [] }; }
function detail(id = 'root', text = 'objective', extra = {}) {
  return { task: task(id), watermark: '1', objective: { text, truncated: false }, model: { id: null, group: null, source: 'unavailable' }, role: null, model_policy: null, commentary: 'unavailable', questions: [], rows: [{ kind: 'evidence', id: 'evidence', task: id, schema: 'public', evidence: [reference] }], next_cursor: 'older', complete: false, ...extra };
}
function harness(overrides = {}) {
  const states = []; const calls = []; let snapshots = 0;
  const actions = { invalidate() {}, register() { return [{ id: 'b'.repeat(36), label: 'Pause' }]; }, records() { return []; }, async reconcile() {}, async dispatch() {} };
  const client = { scope, initialized: { methods: ['task/presentation', 'events/next', 'controller/read', 'usage/read'] },
    async call(method, params) {
      calls.push({ method, params });
      if (overrides[method]) return overrides[method](params, calls);
      if (method === 'session/snapshot') return { kind: 'snapshot', value: { session: { scope, revision: '1', configuration_revision: '0' }, subscription: `subscription-${++snapshots}`, sequence: '1', watermark: '1', event_cursor: 'cursor', complete: true, tasks: [task(), task('child')] } };
      if (method === 'controller/read') return { kind: 'controller', value: { ownership: 'another_connection' } };
      if (method === 'task/presentation') return { kind: 'presentation', value: detail(params.task) };
      if (method === 'usage/read') return { kind: 'usage', value: { scope, root: 'root', task: params.task, currency: 'usd', settled_micros: '3', reserved_micros: '4', unresolved_micros: '5', cap_micros: '100', overrun: false } };
      if (method === 'events/unsubscribe') return { kind: 'unsubscribed', value: { subscription: params.subscription } };
      if (method === 'events/next') return { kind: 'events', value: { subscription: params.subscription, snapshot_sequence: '1', cursor: `${params.cursor}-next`, events: [], at_end: true } };
      if (method === 'artifact/read') return { kind: 'artifact', value: { artifact: params.artifact, offset: params.offset, sha256: reference.sha256, encoding: 'base64', content: Buffer.from('evidence text').toString('base64'), total_bytes: '100007', complete: false } };
      throw new Error(`unexpected ${method}`);
    }, async dispose() {},
  };
  const session = new TaskSession({ actions, publish(state) { states.push(state); } });
  const connect = async (generation = 1) => { session.connection({ phase: 'connected', generation }, client); await session.refresh(); };
  return { session, client, calls, states, connect, state: () => states.at(-1) };
}

test('live gap publishes resync and obtains a fresh canonical snapshot without manufacturing completion', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const h = harness({ 'events/next': params => ({ kind: 'gap', value: { subscription: params.subscription, snapshot_sequence: '1', reason: 'sequence_unavailable', resubscribe_required: true } }) });
  t.after(() => h.session.dispose()); await h.connect();
  t.mock.timers.tick(500); await tick(); await tick();
  assert.ok(h.states.some(state => state.phase === 'gap'));
  assert.equal(h.calls.filter(call => call.method === 'session/snapshot').length, 2);
  assert.equal(h.state().phase, 'current'); assert.equal(h.state().rows[0].state, 'running');
  assert.ok(h.calls.some(call => call.method === 'events/unsubscribe' && call.params.subscription === 'subscription-1'));
});

test('event failure clears details/actions but retains last canonical running state as disconnected', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const h = harness({ 'events/next': () => { throw new Error('provider secret must not escape'); } });
  t.after(() => h.session.dispose()); await h.connect(); t.mock.timers.tick(500); await tick();
  assert.equal(h.state().phase, 'disconnected'); assert.equal(h.state().rows[0].state, 'running');
  assert.equal(h.state().detail, undefined); assert.deepEqual(h.state().actions, []);
  assert.ok(!JSON.stringify(h.state()).includes('provider secret'));
});

test('old connection snapshot cannot populate a reconnected root', async t => {
  const old = deferred(); let count = 0;
  const h = harness({ 'session/snapshot': () => ++count === 1 ? old.promise : { kind: 'snapshot', value: { session: { scope, revision: '1', configuration_revision: '0' }, subscription: 'new-subscription', sequence: '1', watermark: '1', event_cursor: 'cursor', complete: true, tasks: [task()] } } });
  t.after(() => h.session.dispose());
  const first = h.connect(1); await tick(); await h.connect(2);
  const before = h.state();
  old.resolve({ kind: 'snapshot', value: { session: { scope, revision: '1', configuration_revision: '0' }, subscription: 'old-subscription', sequence: '1', watermark: '1', event_cursor: 'cursor', complete: true, tasks: [task('obsolete')] } });
  await first; assert.equal(h.state(), before);
  assert.ok(h.calls.some(call => call.method === 'events/unsubscribe' && call.params.subscription === 'old-subscription'));
});

test('out-of-order task selection cannot overwrite the newly selected task', async t => {
  const child = deferred(); const h = harness({ 'task/presentation': params => params.task === 'child' ? child.promise : { kind: 'presentation', value: detail() } });
  t.after(() => h.session.dispose()); await h.connect();
  const ids = Object.fromEntries(h.state().rows.map(row => [row.task, row.actionId]));
  const first = h.session.dispatch({ action: 'select', id: ids.child }); await tick();
  await h.session.dispatch({ action: 'ready' });
  await h.session.dispatch({ action: 'select', id: h.state().rows.find(row => row.task === 'root').actionId });
  child.resolve({ kind: 'presentation', value: detail('child', 'obsolete child') }); await first;
  assert.equal(h.state().detail.task.task, 'root'); assert.equal(h.state().detail.objective.text, 'objective');
});

test('late history for the same task cannot overwrite a newer refresh or restore its cursor', async t => {
  const old = deferred(); let fresh = false;
  const h = harness({ 'task/presentation': params => params.cursor ? old.promise : { kind: 'presentation', value: detail('root', fresh ? 'new snapshot' : 'initial', { rows: [], next_cursor: fresh ? null : 'older', complete: fresh }) } });
  t.after(() => h.session.dispose()); await h.connect();
  const pending = h.session.dispatch({ action: 'history', id: h.state().historyId }); await tick();
  fresh = true; await h.session.refresh();
  old.resolve({ kind: 'presentation', value: detail('root', 'obsolete page', { next_cursor: 'obsolete-cursor' }) }); await pending;
  assert.equal(h.state().detail.objective.text, 'new snapshot'); assert.equal(h.state().historyId, undefined);
});

test('evidence actions accept registered opaque IDs, cap reads, and reject stale handles', async t => {
  const h = harness(); t.after(() => h.session.dispose()); await h.connect();
  const evidenceId = h.state().evidence[0].id;
  await h.session.dispatch({ action: 'evidence', id: 'artifact' });
  assert.equal(h.calls.filter(call => call.method === 'artifact/read').length, 0);
  await h.session.dispatch({ action: 'evidence', id: evidenceId });
  const request = h.calls.find(call => call.method === 'artifact/read');
  assert.deepEqual(request.params, { scope, task: 'root', artifact: 'artifact', offset: '7', length: 65536 });
  assert.match(h.state().artifact, /^Artifact artifact, bytes 7–20\. UTF-8 preview\.\nevidence text$/);
  await h.session.refresh(); await h.session.dispatch({ action: 'evidence', id: evidenceId });
  assert.equal(h.calls.filter(call => call.method === 'artifact/read').length, 1);
});

test('artifact offset mismatch never opens unrelated evidence content', async t => {
  const h = harness({ 'artifact/read': () => ({ kind: 'artifact', value: { artifact: 'artifact', offset: '999', sha256: reference.sha256, encoding: 'utf8', content: 'wrong range', total_bytes: '100007', complete: false } }) });
  t.after(() => h.session.dispose()); await h.connect();
  await h.session.dispatch({ action: 'evidence', id: h.state().evidence[0].id });
  assert.equal(h.state().artifact, undefined); assert.equal(h.state().historyId, undefined);
});

test('presentation failure clears stale history and opaque evidence before another click', async t => {
  const h = harness({ 'task/presentation': params => { if (params.cursor) throw new Error('stale cursor'); return { kind: 'presentation', value: detail() }; } });
  t.after(() => h.session.dispose()); await h.connect();
  const stale = h.state().historyId;
  await h.session.dispatch({ action: 'history', id: stale });
  assert.equal(h.state().historyId, undefined); assert.equal(h.state().detail, undefined); assert.deepEqual(h.state().evidence, []);
  const reads = h.calls.filter(call => call.method === 'task/presentation').length;
  await h.session.dispatch({ action: 'history', id: stale });
  assert.equal(h.calls.filter(call => call.method === 'task/presentation').length, reads);
});

test('late rejected history cannot clear a newer successful snapshot', async t => {
  const old = deferred(); let fresh = false;
  const h = harness({ 'task/presentation': params => params.cursor ? old.promise : { kind: 'presentation', value: detail('root', fresh ? 'new snapshot' : 'initial') } });
  t.after(() => h.session.dispose()); await h.connect();
  const pending = h.session.dispatch({ action: 'history', id: h.state().historyId }); await tick();
  fresh = true; await h.session.refresh();
  old.reject(new Error('obsolete request failed')); await pending;
  assert.equal(h.state().detail?.objective.text, 'new snapshot');
});

test('actual base64 range contract yields bounded opaque continuation with exact byte offsets', async t => {
  const h = harness({ 'artifact/read': params => {
    const count = Math.min(params.length, 49152, 100007 - Number(params.offset));
    return { kind: 'artifact', value: { artifact: params.artifact, offset: params.offset, sha256: reference.sha256, encoding: 'base64', content: Buffer.alloc(count, 'x').toString('base64'), total_bytes: '100007', complete: Number(params.offset) + count === 100007 } };
  } });
  t.after(() => h.session.dispose()); await h.connect();
  const original = h.state().evidence[0].id;
  await h.session.dispatch({ action: 'evidence', id: original });
  assert.match(h.state().artifact, /bytes 7–49159\./);
  let next = h.state().evidence[0].id; assert.notEqual(next, original);
  await h.session.dispatch({ action: 'evidence', id: original });
  assert.equal(h.calls.filter(call => call.method === 'artifact/read').length, 1);
  await h.session.dispatch({ action: 'evidence', id: next });
  assert.match(h.state().artifact, /bytes 49159–98311\./);
  const last = h.state().evidence[0].id; assert.notEqual(last, next);
  await h.session.dispatch({ action: 'evidence', id: last });
  assert.match(h.state().artifact, /bytes 98311–100007\./);
  assert.deepEqual(h.calls.filter(call => call.method === 'artifact/read').map(call => ({ offset: call.params.offset, length: call.params.length })), [
    { offset: '7', length: 65536 }, { offset: '49159', length: 50848 }, { offset: '98311', length: 1696 },
  ]);
  assert.equal(h.state().rows[0].state, 'running'); // artifact completion is not task completion.
});

test('invalid base64 or mismatched retained hash cannot become an artifact preview', async t => {
  for (const changed of [{ content: '%%%invalid' }, { content: 'Zg' }, { content: 'Zh==' }, { sha256: 'b'.repeat(64) }]) {
    const h = harness({ 'artifact/read': params => ({ kind: 'artifact', value: { artifact: params.artifact, offset: params.offset, sha256: reference.sha256, encoding: 'base64', content: 'Zg==', total_bytes: '100007', complete: false, ...changed } }) });
    try {
      await h.connect(); await h.session.dispatch({ action: 'evidence', id: h.state().evidence[0].id });
      assert.equal(h.state().artifact, undefined); assert.deepEqual(h.state().evidence, []);
    } finally { h.session.dispose(); }
  }
});

test('truncated commentary preserves its single retained evidence reference for authorized reads', async t => {
  const h = harness({ 'task/presentation': params => ({ kind: 'presentation', value: detail(params.task, 'objective', { commentary: 'observed', rows: [{ kind: 'commentary', id: 'commentary', task: params.task, turn: null, text: { text: 'bounded transcript', truncated: true }, evidence: reference }] }) }) });
  t.after(() => h.session.dispose()); await h.connect();
  assert.equal(h.state().detail?.commentary, 'observed'); assert.equal(h.state().evidence.length, 1);
  await h.session.dispatch({ action: 'evidence', id: h.state().evidence[0].id });
  assert.match(h.state().artifact, /evidence text$/);
});
