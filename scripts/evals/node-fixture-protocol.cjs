// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
// Run only in the trusted parent. Accept one exact response, never a child verdict.
function checkResponse(receipt, requestId, expected) {
  assert.equal(receipt.schema, 1);
  assert.equal(receipt.cleanup, 'completed');
  const result = receipt.result;
  assert.equal(result.termination, 'exited');
  assert.equal(result.process.ExitCode, 0);
  assert.equal(result.process.AppContainer, true);
  assert.equal(result.process.CapabilityCount, 0);
  assert.equal(result.process.TokenSidMatchesProfile, true);
  assert.equal(result.stderr_bytes, 0);
  const bytes = Buffer.from(result.stdout_base64, 'base64');
  assert.equal(bytes.length, result.stdout_bytes);
  assert(bytes.length <= 65536);
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  assert(text.endsWith('\n') && !text.slice(0, -1).includes('\n'), 'Expected exactly one response frame');
  const response = JSON.parse(text);
  assert.deepEqual(Object.keys(response).sort(), ['id', 'result']);
  assert.equal(response.id, requestId);
  assert.deepEqual(response.result, expected);
  return { external_response_pass: true };
}
module.exports = { checkResponse };
