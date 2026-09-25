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
// Decode one relayed child frame. The session and sequence are checked here so a
// forged, duplicated or reordered frame fails before its body reaches a probe.
function decodeFrame(base64, session, seq, maxBytes = 65536) {
  assert.equal(typeof base64, 'string');
  const bytes = Buffer.from(base64, 'base64');
  assert.equal(bytes.toString('base64'), base64, 'Frame encoding is not canonical');
  assert(bytes.length <= maxBytes, 'Frame exceeds byte ceiling');
  const frame = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  assert(frame && typeof frame === 'object' && !Array.isArray(frame), 'Frame is not an object');
  assert.deepEqual(Object.keys(frame).sort(), ['body', 'seq', 'session']);
  assert.equal(frame.session, session);
  assert.equal(frame.seq, seq);
  return frame.body;
}
// Accept an interactive receipt only when every frame was exchanged as the parent
// observed it and the contained process exited cleanly without side channels.
function checkInteractiveReceipt(receipt, { sent, consumed, unread }) {
  assert.equal(receipt.schema, 1);
  assert.equal(receipt.mode, 'interactive');
  assert.equal(receipt.cleanup, 'completed');
  const result = receipt.result;
  assert.equal(result.termination, 'exited');
  assert.equal(result.process.ExitCode, 0);
  assert.equal(result.process.AppContainer, true);
  assert.equal(result.process.CapabilityCount, 0);
  assert.equal(result.process.TokenSidMatchesProfile, true);
  assert.equal(result.stderr_bytes, 0);
  assert.equal(result.trailing_bytes, 0);
  assert.equal(unread, 0, 'Child sent unrequested frames');
  assert.equal(result.frames_to_child, sent + 1, 'Parent frame count differs');
  assert.equal(result.frames_from_child, consumed, 'Child frame count differs');
  return { external_interaction_pass: true };
}
module.exports = { checkResponse, decodeFrame, checkInteractiveReceipt };
