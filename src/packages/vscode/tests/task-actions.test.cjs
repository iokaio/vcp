// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { TaskActions, pendingTaskCommands } = require('../dist/task_actions.js');
const scope = { workspace: 'workspace', session: 'session' };
const task = { scope, task: 'child', root: 'root', parent: 'root', turn: 'turn', revision: '9007199254740993', steering_revision: '7', state: 'waiting_for_input', reason: 'question', effects: 'pending', pending_inputs: [{ id: 'approval', kind: 'approval', revision: '4', operation_digest: 'a'.repeat(64), effect_revision: '5', policy_revision: '6' }] };
const capabilities = ['task/read', 'command/read', 'approval/respond', 'approval/source-revisions/1', 'turn/steer', 'turn/pause', 'session/resume', 'task/cancel'];
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function fixture(overrides = {}, saved = []) {
  const calls = [], saves = [], published = [];
  const receipts = new Map();
  let context;
  const call = async (method, params) => {
    calls.push({ method, params: structuredClone(params) });
    if (overrides.call) { const result = await overrides.call(method, params, receipts); if (result !== undefined) return result; }
    if (method === 'task/read') return { kind: 'task', value: structuredClone(task) };
    if (method === 'command/read') { const value = receipts.get(params.command_id); if (!value) throw Error('unavailable'); return { kind: 'acceptance', value }; }
    const value = { command_id: params.mutation.command_id, scope: params.scope, task: params.task, turn: null, revision: '8', watermark: '99', outcome: 'accepted' };
    receipts.set(value.command_id, value);
    return { kind: 'acceptance', value };
  };
  context = { generation: 1, scope, controller: true, trusted: true, capabilities, call };
  const deps = { current: () => context, save: async records => { if (overrides.save) await overrides.save(records); saves.push(structuredClone(records)); }, publish: () => published.push(context?.scope.session), promptSteering: overrides.promptSteering ?? (async () => 'private guidance'), now: overrides.now ?? (() => 10) };
  const actions = new TaskActions(deps, saved);
  return { actions, calls, saves, published, receipts, deps, context: () => context, set: update => { context = { ...context, ...update }; }, register: value => actions.register(value ?? task, { constraints: ['Keep original constraints'], acceptance: ['Keep original acceptance'] }, [{ input: task.pending_inputs[0], actionable: true, expires_at: '1000' }]), dispatch: entry => actions.dispatch({ action: 'task', id: entry.id }) };
}

test('approval uses exact source revisions, saves identity before dispatch, and coalesces duplicate clicks', async () => {
  const gate = deferred();
  const f = fixture({ save: async records => { if (records[0]?.phase === 'submitting') await gate.promise; } });
  const action = f.register().find(item => item.operation === 'allow');
  const first = f.dispatch(action), second = f.dispatch(action);
  assert.equal(first, second);
  await tick();
  assert.deepEqual(f.calls.map(item => item.method), ['task/read']);
  gate.resolve(); await first;
  const request = f.calls.find(item => item.method === 'approval/respond').params;
  assert.equal(request.mutation.expected_revision, '4');
  assert.equal(request.mutation.steering_revision, '7');
  assert.equal(request.effect_revision, '5'); assert.equal(request.policy_revision, '6');
  assert.equal(request.approval, 'approval'); assert.equal(request.decision, 'allow');
  assert.equal(f.actions.records()[0].phase, 'reconciled');
  assert.deepEqual(f.saves.map(rows => rows[0].phase), ['submitting', 'accepted', 'reconciled']);
  assert.equal(f.calls.filter(item => item.method === 'approval/respond').length, 1);
  await assert.rejects(f.dispatch(action), /expired/);
});

test('unknown, hostile, extra-field and stale generation messages never dispatch a mutation', async () => {
  const f = fixture(); const action = f.register()[0];
  for (const message of [{ action: 'task', id: 'unknown' }, { action: 'task', id: action.id, command: 'shell' }, { action: 'task', id: action.id, text: 'untrusted' }, { action: 'task', id: '../bad' }, { get action() { throw Error('getter'); }, id: action.id }]) await assert.rejects(f.actions.dispatch(message));
  f.set({ generation: 2 });
  await assert.rejects(f.dispatch(action), /authority/);
  assert.deepEqual(f.calls, []);
});

test('observer/trust gates are repeated in host handler, and missing approval profile disables response', async () => {
  for (const update of [{ controller: false }, { trusted: false }]) {
    const f = fixture(); const action = f.register()[0]; f.set(update);
    await assert.rejects(f.dispatch(action), /authority/); assert.deepEqual(f.calls, []);
  }
  const f = fixture(); f.set({ capabilities: capabilities.filter(value => value !== 'approval/source-revisions/1') });
  const action = f.register()[0]; assert.match(action.disabledReason, /source revisions/);
  await assert.rejects(f.dispatch(action), /disabled/);
});

test('fresh task/input revision mismatch fails before saving or mutation', async () => {
  for (const change of [{ revision: '9007199254740994' }, { steering_revision: '8' }, { turn: 'other-turn' }, { scope: { ...scope, session: 'other' } }, { pending_inputs: [{ ...task.pending_inputs[0], effect_revision: '9' }] }]) {
    const f = fixture({ call: async method => method === 'task/read' ? { kind: 'task', value: { ...task, ...change } } : undefined });
    await assert.rejects(f.dispatch(f.register()[0]), /changed/);
    assert.equal(f.saves.length, 0); assert.equal(f.calls.length, 1);
  }
});

test('save failure and scope change during save never send the prepared command', async () => {
  const failure = fixture({ save: async () => { throw Error('disk'); } });
  await assert.rejects(failure.dispatch(failure.register()[0]), /nothing was submitted/);
  assert.equal(failure.calls.length, 1);
  const gate = deferred(); const f = fixture({ save: async records => { if (records[0].phase === 'submitting') await gate.promise; } });
  const pending = f.dispatch(f.register()[0]); await tick(); f.set({ generation: 2, scope: { ...scope, session: 'new' } }); gate.resolve();
  await assert.rejects(pending, /authority/); assert.equal(f.calls.length, 1); assert.deepEqual(f.published, []);
});

test('lost reply persists one identity; reload observes its receipt without replaying', async () => {
  const f = fixture({ call: async (method, params, receipts) => {
    if (method === 'approval/respond') {
      receipts.set(params.mutation.command_id, { command_id: params.mutation.command_id, scope, task: task.task, revision: '8', watermark: '99', outcome: 'accepted', turn: null });
      throw Error('transport ended after commit');
    }
  } });
  await assert.rejects(f.dispatch(f.register()[0]), /Outcome not yet known/);
  const saved = f.saves.at(-1); assert.equal(saved[0].phase, 'unknown');
  const restored = new TaskActions(f.deps, saved); const before = f.calls.length;
  f.set({ controller: false }); await restored.reconcile();
  assert.deepEqual(f.calls.slice(before).map(item => item.method), ['command/read']);
  assert.equal(restored.records()[0].phase, 'reconciled');
  assert.equal(f.calls.filter(item => item.method === 'approval/respond').length, 1);
});

test('missing command after reload remains unknown and blocks another answer but leaves pause/cancel available', async () => {
  const saved = [{ version: 1, commandId: 'pending-command', scope, task: task.task, target: 'approval', operation: 'allow', phase: 'submitting' }];
  const f = fixture({}, saved); await f.actions.reconcile();
  assert.equal(f.actions.records()[0].phase, 'unknown');
  const actions = f.register(); assert.match(actions.find(row => row.operation === 'deny').disabledReason, /Reconcile/);
  assert.equal(actions.find(row => row.operation === 'pause').disabledReason, undefined);
  assert.equal(actions.find(row => row.operation === 'cancel').disabledReason, undefined);
  assert.deepEqual(f.calls.map(item => item.method), ['command/read']);
});

test('steering prompt stays host-side and private guidance never enters persisted metadata', async () => {
  const f = fixture(); const steer = f.register().find(row => row.operation === 'steer'); await f.dispatch(steer);
  const request = f.calls.find(item => item.method === 'turn/steer').params;
  assert.equal(request.objective, 'private guidance'); assert.equal(request.mutation.expected_revision, task.revision);
  assert.deepEqual(request.constraints, ['Keep original constraints']); assert.deepEqual(request.acceptance, ['Keep original acceptance']);
  assert.equal(JSON.stringify(f.saves).includes('private guidance'), false);
  assert.equal(JSON.stringify(f.saves).includes('operation_digest'), false);
  assert.equal(JSON.stringify(f.saves).includes('Keep original'), false);
  const big = fixture({ promptSteering: async () => '🙂'.repeat(20000) });
  await assert.rejects(big.dispatch(big.register().find(row => row.operation === 'steer')), /UTF-8/);
  assert.deepEqual(big.calls, []);
});

test('late old-scope receipt is retained without publishing into the newly selected scope', async () => {
  const gate = deferred(); const f = fixture({ call: async method => { if (method === 'turn/pause') await gate.promise; } });
  const pending = f.dispatch(f.register().find(row => row.operation === 'pause')); await tick();
  const count = f.published.length; f.set({ generation: 2, scope: { ...scope, session: 'new' } }); gate.resolve(); await pending;
  assert.equal(f.published.length, count); assert.equal(f.actions.records()[0].phase, 'accepted');
  assert.equal(f.calls.some(row => row.method === 'command/read'), false);
});

test('persistence validation rejects credentials, payloads, duplicate identities and noncanonical revisions', () => {
  const row = { version: 1, commandId: 'command', scope, task: task.task, operation: 'resume', phase: 'submitting' };
  assert.equal(pendingTaskCommands([row])[0].phase, 'unknown');
  for (const changed of [{ ...row, credentials: 'secret' }, { ...row, objective: 'private' }, { ...row, revision: '01' }, { ...row, watermark: '18446744073709551616' }]) assert.deepEqual(pendingTaskCommands([changed]), []);
  assert.deepEqual(pendingTaskCommands([row, row]), []);
});

test('pause and explicit resume use task revision without inventing automatic continuation', async () => {
  const f = fixture(); await f.dispatch(f.register().find(row => row.operation === 'pause'));
  const pause = f.calls.find(row => row.method === 'turn/pause').params;
  assert.equal(pause.turn, task.turn); assert.equal(pause.mutation.expected_revision, task.revision);
  assert.equal(f.calls.some(row => row.method === 'session/resume'), false);
  const paused = { ...task, state: 'paused' };
  const r = fixture({ call: async method => method === 'task/read' ? { kind: 'task', value: paused } : undefined });
  await r.dispatch(r.register(paused).find(row => row.operation === 'resume'));
  const resume = r.calls.find(row => row.method === 'session/resume').params;
  assert.equal(resume.task, task.task); assert.equal(resume.mutation.expected_revision, task.revision);
});

test('stale engine errors invalidate action; reconciliation gaps preserve a known acceptance', async () => {
  const f = fixture({ call: async method => { if (method === 'approval/respond') throw { code: 'rpc', classification: { applicationCode: 'APPROVAL_STALE', retry: 'after_revalidation' } }; } });
  const action = f.register()[0]; await assert.rejects(f.dispatch(action), /rejected/);
  assert.equal(f.actions.records()[0].phase, 'rejected'); await assert.rejects(f.dispatch(action), /expired/);
  const r = fixture({ call: async method => { if (method === 'command/read') throw Error('retained evidence unavailable'); } });
  await r.dispatch(r.register()[0]); assert.equal(r.actions.records()[0].phase, 'accepted');
});

test('reply correlation cannot redirect a persisted command to another task', async () => {
  const f = fixture({ call: async (method, params) => method === 'approval/respond' ? { kind: 'acceptance', value: { command_id: params.mutation.command_id, scope, task: 'other-child', turn: null, revision: '8', watermark: '99', outcome: 'accepted' } } : undefined });
  await assert.rejects(f.dispatch(f.register()[0]), /Outcome not yet known/);
  assert.equal(f.actions.records()[0].phase, 'unknown');
});

test('steering cannot silently discard unavailable or oversized objective criteria', async () => {
  const f = fixture();
  for (const objective of [undefined, { constraints: ['x'.repeat(4097)], acceptance: [] }]) {
    const action = f.actions.register(task, objective).find(row => row.operation === 'steer');
    assert.match(action.disabledReason, /criteria/);
    await assert.rejects(f.dispatch(action), /disabled/);
  }
  assert.deepEqual(f.calls, []);
});

test('approval requires current question validity and expiry is rechecked after asynchronous reads', async () => {
  const missing = fixture(); const unavailable = missing.actions.register(task)[0];
  assert.match(unavailable.disabledReason, /validity/); await assert.rejects(missing.dispatch(unavailable), /disabled/);
  const f = fixture();
  const stale = f.actions.register(task, undefined, [{ input: task.pending_inputs[0], actionable: false, expires_at: '1000' }])[0];
  assert.match(stale.disabledReason, /validity/); await assert.rejects(f.dispatch(stale), /disabled/);
  const gate = deferred(); let now = 10;
  const expiry = fixture({ now: () => now, call: async method => { if (method === 'task/read') await gate.promise; } });
  const pending = expiry.dispatch(expiry.register()[0]); await tick(); now = 1000; gate.resolve();
  await assert.rejects(pending, /expired/); assert.equal(expiry.calls.length, 1); assert.equal(expiry.saves.length, 0);
});

test('unchanged selected-task refresh does not starve a steering prompt beside noisy child traffic', async () => {
  const gate = deferred(); const f = fixture({ promptSteering: async () => { await gate.promise; return 'guidance'; } });
  const pending = f.dispatch(f.register().find(row => row.operation === 'steer'));
  for (let i = 0; i < 10; i++) f.register();
  gate.resolve(); await pending;
  assert.equal(f.calls.filter(row => row.method === 'turn/steer').length, 1);
});

test('pending answer does not impose a global lock on an independent explicit pause', async () => {
  const gate = deferred(); const f = fixture({ call: async method => { if (method === 'approval/respond') await gate.promise; } });
  const actions = f.register(); const answer = f.dispatch(actions.find(row => row.operation === 'allow')); await tick();
  await f.dispatch(actions.find(row => row.operation === 'pause'));
  assert.equal(f.calls.filter(row => row.method === 'turn/pause').length, 1);
  gate.resolve(); await answer;
});
