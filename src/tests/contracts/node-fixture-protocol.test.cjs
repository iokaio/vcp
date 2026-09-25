// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { checkResponse } = require('../../../scripts/evals/node-fixture-protocol.cjs');
const id = '0123456789abcdef0123456789abcdef';
function receipt(text = JSON.stringify({ id, result: 42 }) + '\n') {
  const bytes = Buffer.from(text);
  return { schema: 1, cleanup: 'completed', result: { termination: 'exited',
    process: { ExitCode: 0, AppContainer: true, CapabilityCount: 0, TokenSidMatchesProfile: true },
    stdout_base64: bytes.toString('base64'), stdout_bytes: bytes.length, stderr_bytes: 0 } };
}
test('parent verifies one external result', () => {
  assert.deepEqual(checkResponse(receipt(), id, 42), { external_response_pass: true });
});
test('child verdicts, extra frames, wrong IDs and wrong values cannot pass', () => {
  for (const text of [JSON.stringify({ passed: true }) + '\n',
    JSON.stringify({ id, result: 42, passed: true }) + '\n',
    JSON.stringify({ id: 'other', result: 42 }) + '\n', JSON.stringify({ id, result: 43 }) + '\n',
    JSON.stringify({ id, result: 42 }) + '\n{}\n', '{"id":', JSON.stringify({ id, result: 42 })]) {
    assert.throws(() => checkResponse(receipt(text), id, 42));
  }
});
test('failed isolation, lifecycle and capture receipts cannot pass', () => {
  for (const mutate of [r => r.cleanup = 'pending', r => r.result.termination = 'output_limit',
    r => r.result.process.ExitCode = 1, r => r.result.process.AppContainer = false,
    r => r.result.process.CapabilityCount = 1, r => r.result.process.TokenSidMatchesProfile = false,
    r => r.result.stderr_bytes = 1, r => r.result.stdout_bytes++,
    r => { r.result.stdout_base64 = '/wo='; r.result.stdout_bytes = 2; }]) {
    const value = receipt(); mutate(value); assert.throws(() => checkResponse(value, id, 42));
  }
});
