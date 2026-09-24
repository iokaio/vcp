// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import schema from '@vcp/protocol/schema.json' with { type: 'json' };
import { RESULT_KINDS, REQUIRED_PROFILES } from '../dist/method-results.js';

test('every canonical method has an audited result mapping', () => {
  const methods = schema.definitions.Call.oneOf.map(branch => branch.properties.method.enum[0]);
  assert.equal(methods.length, 53);
  assert.deepEqual(Object.keys(RESULT_KINDS).sort(), methods.sort());
  const kinds = new Set(schema.definitions.ResultValue.oneOf.map(branch => branch.properties.kind.enum[0]));
  for (const [method, replies] of Object.entries(RESULT_KINDS)) {
    assert.ok(Object.isFrozen(replies));
    assert.ok(replies.every(kind => kinds.has(kind)));
    assert.ok(replies.length > 0);
    if (method.startsWith('editor/')) assert.deepEqual(REQUIRED_PROFILES[method], ['editor/prepared-edits/1']);
  }
});

test('controller receipts and stream unions match dispatcher envelopes', () => {
  assert.deepEqual(RESULT_KINDS['controller/read'], ['controller']);
  assert.deepEqual(RESULT_KINDS['task/presentation'], ['presentation']);
  for (const method of ['controller/acquire', 'controller/release', 'controller/recover', 'command/read']) assert.deepEqual(RESULT_KINDS[method], ['acceptance']);
  assert.deepEqual(RESULT_KINDS['session/snapshot'], ['snapshot', 'gap']);
  for (const method of ['events/subscribe', 'events/next']) assert.deepEqual(RESULT_KINDS[method], ['events', 'gap']);
  assert.deepEqual(RESULT_KINDS['memory/forget'], ['forgotten']);
  assert.deepEqual(RESULT_KINDS['session/export'], ['export']);
  assert.deepEqual(REQUIRED_PROFILES['memory/query'], ['memory/query-sources/1']);
  assert.deepEqual(REQUIRED_PROFILES['memory/forget'], ['memory/retention/1']);
  assert.deepEqual(REQUIRED_PROFILES['memory/inspect'], ['memory/inspection-state/1']);
  assert.deepEqual(RESULT_KINDS['history/query'], ['history']);
  assert.deepEqual(REQUIRED_PROFILES['history/query'], ['history/query/1']);
  assert.deepEqual(RESULT_KINDS['memory/history'], ['memory_history']);
  assert.deepEqual(REQUIRED_PROFILES['memory/history'], ['memory/history/1']);
  assert.deepEqual(RESULT_KINDS['policy/read'], ['policy']);
  assert.deepEqual(REQUIRED_PROFILES['policy/read'], ['policy/inspection/1']);
  assert.deepEqual(RESULT_KINDS['routing/status'], ['routing_status']);
  assert.deepEqual(REQUIRED_PROFILES['routing/status'], ['routing/status/1']);
  for (const method of ['routing/reportCapture', 'routing/apply', 'routing/rollback']) {
    assert.deepEqual(RESULT_KINDS[method], ['acceptance']);
    assert.deepEqual(REQUIRED_PROFILES[method], ['routing/optimizer/1']);
  }
  assert.deepEqual(RESULT_KINDS['routing/reportRead'], ['routing_report']);
  assert.deepEqual(RESULT_KINDS['routing/preview'], ['routing_preview']);
});
