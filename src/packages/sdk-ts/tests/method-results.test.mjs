// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import schema from '@vcp/protocol/schema.json' with { type: 'json' };
import { RESULT_KINDS, REQUIRED_PROFILES } from '../dist/method-results.js';

test('every canonical method has an audited result mapping; editor success remains unavailable', () => {
  const methods = schema.definitions.Call.oneOf.map(branch => branch.properties.method.enum[0]);
  assert.equal(methods.length, 39);
  assert.deepEqual(Object.keys(RESULT_KINDS).sort(), methods.sort());
  const kinds = new Set(schema.definitions.ResultValue.oneOf.map(branch => branch.properties.kind.enum[0]));
  for (const [method, replies] of Object.entries(RESULT_KINDS)) {
    assert.ok(Object.isFrozen(replies));
    assert.ok(replies.every(kind => kinds.has(kind)));
    assert.equal(replies.length === 0, method.startsWith('editor/'));
  }
});

test('controller receipts and stream unions match dispatcher envelopes', () => {
  assert.deepEqual(RESULT_KINDS['controller/read'], ['controller']);
  for (const method of ['controller/acquire', 'controller/release', 'controller/recover', 'command/read']) assert.deepEqual(RESULT_KINDS[method], ['acceptance']);
  assert.deepEqual(RESULT_KINDS['session/snapshot'], ['snapshot', 'gap']);
  for (const method of ['events/subscribe', 'events/next']) assert.deepEqual(RESULT_KINDS[method], ['events', 'gap']);
  assert.deepEqual(RESULT_KINDS['memory/forget'], ['forgotten']);
  assert.deepEqual(RESULT_KINDS['session/export'], ['export']);
  assert.deepEqual(REQUIRED_PROFILES['memory/query'], ['memory/query-sources/1']);
  assert.deepEqual(REQUIRED_PROFILES['memory/forget'], ['memory/retention/1']);
  assert.deepEqual(REQUIRED_PROFILES['memory/inspect'], ['memory/inspection-state/1']);
});
