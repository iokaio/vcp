// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { EditorJournal, editorCommandRecords } = require('../dist/editor_journal.js');
const scope = { workspace: 'workspace', session: 'session' };

test('persisted editor identity is required before dispatch; save failure cannot authorize sending', async () => {
  const journal = new EditorJournal(async () => { throw Error('storage unavailable'); });
  let sent = false;
  await assert.rejects(async () => { await journal.begin(scope, 'task', 'dispatch', { change: 'change', file: 0 }); sent = true; });
  assert.equal(sent, false);
});

test('reload preserves original uncertain identity and absent receipts never trigger mutation', async () => {
  let saved;
  const journal = new EditorJournal(async records => { saved = structuredClone(records); });
  const id = await journal.begin(scope, 'task', 'dispatch', { change: 'change', file: 0 });
  const reloaded = new EditorJournal(async records => { saved = structuredClone(records); }, saved);
  assert.equal(reloaded.records()[0].phase, 'unknown');
  const calls = [];
  await reloaded.reconcile(scope, async (method, params) => { calls.push([method, params.command_id]); throw Error('pruned'); });
  assert.deepEqual(calls, [['command/read', id]]);
  assert.equal(reloaded.records()[0].phase, 'unknown');
  await reloaded.reconcile(scope, async () => ({ kind: 'acceptance', value: { command_id: id, scope, task: 'task', outcome: 'accepted' } }));
  assert.equal(reloaded.records()[0].phase, 'accepted');
  assert.equal(reloaded.records()[0].id, id);
});

test('late settlement shares one serialized writer and preserves newer command identities', async () => {
  let saved;
  const journal = new EditorJournal(async records => { await new Promise(resolve => setImmediate(resolve)); saved = structuredClone(records); });
  const first = await journal.begin(scope, 'task', 'prepare');
  const second = journal.begin(scope, 'task', 'dispatch', { change: 'change', file: 0 });
  const late = journal.settle(first, 'accepted', 'change');
  const secondId = await second; await late;
  assert.deepEqual(saved.map(record => record.id), [first, secondId]);
  assert.equal(saved[0].change, 'change');
});

test('journal rejects content-bearing, malformed or duplicate records without invoking accessors', () => {
  const record = { version: 1, id: 'command', scope, task: 'task', kind: 'start', phase: 'unknown' };
  for (const records of [[{ ...record, objective: 'private text' }], [{ ...record, credential: 'private' }], [record, record], [{ ...record, scope: { get workspace() { throw Error('must not read'); }, session: 'session' } }]]) {
    assert.deepEqual(editorCommandRecords(records), []);
  }
});

test('accepted dispatch intent remains protected from eviction until its outcome is resolved', async () => {
  const records = Array.from({ length: 128 }, (_, index) => ({ version: 1, id: `command-${index}`, scope, task: 'task', kind: 'dispatch', change: 'change', file: 0, phase: 'accepted' }));
  const journal = new EditorJournal(async () => {}, records);
  await assert.rejects(journal.begin(scope, 'task', 'prepare'), /full/);
  assert.deepEqual(journal.records(), records);
});
