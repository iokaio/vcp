// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { checkResponse, decodeFrame, checkInteractiveReceipt } = require('../../../scripts/evals/node-fixture-protocol.cjs');
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

test('single-shot supervisor rejects runner faults and reconciles its prelaunch identity', async () => {
  const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
  const { EventEmitter } = require('node:events');
  const { PassThrough } = require('node:stream');
  const filename = path.resolve(__dirname, '../../../scripts/evals/node-fixture-session.cjs');
  for (const [output, code, stderr, expected] of [
    ['{"cleanup":"completed","valid":true}', 0, '', null],
    ['{"cleanup":"completed","valid":true}', 0, 'runner diagnostic', /Runner failed/],
    ['{"cleanup":"pending"}', 0, '', /Runner cleanup receipt is invalid/],
    ['', 1, '', /Runner failed/],
    ['{"partial":', 0, '', /Runner receipt is not JSON/],
    [Buffer.from([0xff]), 0, '', /Runner receipt is not JSON/],
    ['x'.repeat(1048577), 0, '', /Runner output exceeds ceiling/],
  ]) {
    const child = new EventEmitter();
    child.stdout = new PassThrough(); child.stderr = new PassThrough();
    child.kill = () => child.emit('close', null, 'SIGTERM');
    const cleanups = [], launches = [], module = { exports: {} };
    vm.runInNewContext(fs.readFileSync(filename, 'utf8'), {
      module, __dirname: path.dirname(filename), process: { env: { LOCALAPPDATA: path.resolve('fixture-local-app-data') } },
      Buffer, TextDecoder, setTimeout, clearTimeout,
      require(name) {
        if (name === 'node:child_process') return {
          spawn: (...args) => { launches.push(args); return child; },
          spawnSync: (...args) => { cleanups.push(args); return { status: 0 }; },
        };
        if (name === 'node:crypto') return { randomBytes: () => Buffer.alloc(16, 0xbb) };
        if (name === 'node:fs') return { readFileSync: () => '{"timeout_ms":1000}', existsSync: () => false };
        if (name === './node-fixture-protocol.cjs') return { decodeFrame };
        return require(name);
      },
    }, { filename });
    const result = module.exports.runSingle('fixture.json');
    assert.equal(launches[0][1].slice(-2).join(','), '-ProfileName,iokaio.vcp.memory.' + 'b'.repeat(32));
    child.stdout.write(output); child.stderr.write(stderr); child.emit('close', code, null);
    if (expected) {
      await assert.rejects(result, expected);
      assert.equal(cleanups.length, 1, 'Failure must reconcile even before a startup envelope');
      assert.match(cleanups[0][1].at(-1), /iokaio\.vcp\.memory\.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/);
    } else {
      assert.equal((await result).valid, true);
      assert.equal(cleanups.length, 0);
    }
  }
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
const session = 'fedcba9876543210fedcba9876543210';
const frame = (value, text = JSON.stringify(value)) => Buffer.from(text).toString('base64');
test('parent decodes relayed frames only for its session and next sequence', () => {
  assert.deepEqual(decodeFrame(frame({ session, seq: 1, body: { ok: true } }), session, 1), { ok: true });
  for (const [encoded, seq] of [
    [frame({ session: '0'.repeat(32), seq: 1, body: 1 }), 1], [frame({ session, seq: 1, body: 1 }), 2],
    [frame({ session, seq: 2, body: 1 }), 1], [frame({ session, seq: 1, body: 1, passed: true }), 1],
    [frame({ session, seq: 1 }), 1], [frame(null, 'not json'), 1], [frame(null, '[1]'), 1],
    [Buffer.from([0xff, 0x0a]).toString('base64'), 1], [frame({ session, seq: 1, body: 1 }) + '=', 1],
  ]) assert.throws(() => decodeFrame(encoded, session, seq));
  assert.throws(() => decodeFrame(frame({ session, seq: 1, body: 'x'.repeat(64) }), session, 1, 32));
});
function interactiveReceipt() {
  return { schema: 1, mode: 'interactive', cleanup: 'completed', result: { termination: 'exited',
    process: { ExitCode: 0, AppContainer: true, CapabilityCount: 0, TokenSidMatchesProfile: true },
    stderr_bytes: 0, trailing_bytes: 0, frames_to_child: 3, frames_from_child: 2 } };
}
test('parent accepts an interactive receipt only when its own frame counts match', () => {
  assert.deepEqual(checkInteractiveReceipt(interactiveReceipt(), { sent: 2, consumed: 2, unread: 0 }), { external_interaction_pass: true });
  for (const counts of [{ sent: 2, consumed: 2, unread: 1 }, { sent: 1, consumed: 2, unread: 0 }, { sent: 2, consumed: 1, unread: 0 }]) {
    assert.throws(() => checkInteractiveReceipt(interactiveReceipt(), counts));
  }
  for (const mutate of [r => delete r.mode, r => r.cleanup = 'pending', r => r.result.termination = 'idle_timeout',
    r => r.result.termination = 'frame_limit', r => r.result.process.ExitCode = 1, r => r.result.process.AppContainer = false,
    r => r.result.process.CapabilityCount = 1, r => r.result.process.TokenSidMatchesProfile = false,
    r => r.result.stderr_bytes = 1, r => r.result.trailing_bytes = 1]) {
    const value = interactiveReceipt(); mutate(value);
    assert.throws(() => checkInteractiveReceipt(value, { sent: 2, consumed: 2, unread: 0 }));
  }
});

// Exercise supervisor protocol failures without launching a Windows process.
// The receipt alone cannot certify that its enclosing runner completed cleanly.
test('interactive supervisor rejects truncated envelopes and failed runner completion', async () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const vm = require('node:vm');
  const { EventEmitter } = require('node:events');
  const { PassThrough } = require('node:stream');
  const filename = path.resolve(__dirname, '../../../scripts/evals/node-fixture-session.cjs');
  function runner() {
    const child = new EventEmitter();
    child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
    child.kill = () => { child.emit('close', null, 'SIGTERM'); };
    const module = { exports: {} }, cleanups = [];
    vm.runInNewContext(fs.readFileSync(filename, 'utf8'), {
      module, __dirname: path.dirname(filename), process: {
        env: { LOCALAPPDATA: path.resolve('fixture-local-app-data') },
        kill() { throw Object.assign(Error('exited'), { code: 'ESRCH' }); },
      }, Buffer, setTimeout, clearTimeout,
      require(name) {
        if (name === 'node:crypto') return { randomBytes: () => Buffer.alloc(16, 0xaa) };
        if (name === 'node:child_process') return { spawn: () => child, spawnSync: (...args) => { cleanups.push(args); return { status: 0 }; } };
        if (name === 'node:fs') return { readFileSync: () => '{"timeout_ms":1000}', existsSync: () => false };
        if (name === './node-fixture-protocol.cjs') return { decodeFrame };
        return require(name);
      },
    }, { filename });
    return { child, session: module.exports.openInteractive('fixture.json'), cleanups };
  }
  const started = JSON.stringify({ started: { pid: 123, profile: 'iokaio.vcp.memory.' + 'a'.repeat(32) } }) + '\n';
  const receiptLine = JSON.stringify({ receipt: interactiveReceipt() }) + '\n';
  for (const cleanup of [undefined, 'pending', 'failed']) {
    const invalid = interactiveReceipt(); invalid.cleanup = cleanup;
    const { child, session, cleanups } = runner();
    child.stdout.write(started + JSON.stringify({ receipt: invalid }) + '\n');
    child.emit('close', 0, null);
    await assert.rejects(session.close(), error => {
      assert.match(error.message, /Runner cleanup receipt is invalid/);
      assert.equal(error.receipt.cleanup, cleanup);
      return true;
    });
    assert.equal(cleanups.length, 1, 'Invalid cleanup receipt must reconcile the reserved profile');
    assert.match(cleanups[0][1].at(-1), /iokaio\.vcp\.memory\.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/);
  }
  for (const [suffix, code, diagnostic, expected] of [
    ['', 0, '', null], ['{"frame":', 0, '', /Partial runner envelope/],
    ['', 1, '', /Runner failed/], ['', 0, 'runner error', /Runner failed/],
  ]) {
    const { child, session } = runner();
    child.stdout.write(started + receiptLine + suffix);
    child.stderr.write(diagnostic);
    child.emit('close', code, null);
    if (expected) await assert.rejects(session.close(), expected);
    else assert.equal((await session.close()).exit.code, 0);
  }
  for (const [output, expected] of [
    [receiptLine, /Runner ended without a receipt/],
    ['{"started":{"pid":-1,"profile":"bad"}}\n', /Runner ended without a receipt/],
    [started + receiptLine + 'x'.repeat(196609), /Runner envelope exceeds ceiling/],
  ]) {
    const { child, session } = runner();
    child.stdout.write(output);
    await assert.rejects(session.close(), expected);
  }
  // Candidate protocol failure cannot hide subsequent loss of the trusted runner.
  for (const [receipt, exitCode, suffix = ''] of [[null, 0], [interactiveReceipt(), 1], [interactiveReceipt(), 0], [interactiveReceipt(), 0, '{"frame":']]) {
    const { child, session } = runner();
    child.stdout.write(started + JSON.stringify({ frame: frame(null, 'not json') }) + '\n');
    await assert.rejects(session.receive());
    if (receipt) child.stdout.write(JSON.stringify({ receipt }) + '\n');
    if (suffix) child.stdout.write(suffix);
    child.emit('close', exitCode, null);
    await assert.rejects(session.close(), error => {
      if (!receipt) assert.match(error.message, /Runner ended without a receipt/);
      else if (exitCode) assert.match(error.message, /Runner failed after receipt/);
      else if (suffix) assert.match(error.message, /Partial runner envelope/);
      else assert.deepEqual(JSON.parse(JSON.stringify(error.receipt)), receipt);
      return true;
    });
  }
});
