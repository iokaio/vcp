// SPDX-License-Identifier: Apache-2.0
'use strict';
// Trusted parent side of the interactive Windows Node fixture runner. Probes send
// frames and judge the decoded replies; the contained child never supplies a verdict.
const { spawn } = require('node:child_process');
const crypto = require('node:crypto');
const path = require('node:path');
const { decodeFrame } = require('./node-fixture-protocol.cjs');
const runner = path.join(__dirname, 'node-fixture-runner.ps1');
const lineLimit = 196608, streamLimit = 2097152;
function openInteractive(configPath, { cwd = path.resolve(__dirname, '../..'), maxFrameBytes = 65536 } = {}) {
  const session = crypto.randomBytes(16).toString('hex');
  const child = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configPath],
    { cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  const frames = [];
  let pending = '', received = 0, stderr = '', receipt = null, started = null, failure = null, wake = null, sent = 0, consumed = 0;
  const exited = new Promise(resolve => child.on('close', (code, signal) => { notify(); resolve({ code, signal }); }));
  function notify() { if (wake) { const resume = wake; wake = null; resume(); } }
  function fail(error) { failure ??= error; notify(); }
  child.on('error', fail);
  child.stdin.on('error', () => {});
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', text => { if (stderr.length < 65536) stderr += text; });
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', text => {
    received += text.length;
    if (received > streamLimit) { fail(Error('Runner output exceeds ceiling')); child.kill(); return; }
    pending += text;
    for (let index; (index = pending.indexOf('\n')) >= 0; pending = pending.slice(index + 1)) {
      const line = pending.slice(0, index);
      if (line.length > lineLimit) { fail(Error('Runner envelope exceeds ceiling')); child.kill(); return; }
      let envelope;
      try { envelope = JSON.parse(line); } catch { fail(Error('Malformed runner envelope')); child.kill(); return; }
      const keys = envelope && typeof envelope === 'object' ? Object.keys(envelope) : [];
      if (keys.length !== 1) { fail(Error('Unexpected runner envelope')); child.kill(); return; }
      if (keys[0] === 'started' && started === null && frames.length === 0) started = envelope.started;
      else if (keys[0] === 'frame' && started !== null && receipt === null) frames.push(envelope.frame);
      else if (keys[0] === 'receipt' && receipt === null) receipt = envelope.receipt;
      else { fail(Error('Unexpected runner envelope order')); child.kill(); return; }
    }
    notify();
  });
  child.stdin.write(JSON.stringify({ session }) + '\n');
  return {
    session,
    pid: child.pid,
    get started() { return started; },
    send(body) { sent++; child.stdin.write(JSON.stringify({ seq: sent, body }) + '\n'); },
    async receive(timeoutMs = 5000) {
      const deadline = Date.now() + timeoutMs;
      while (frames.length === 0) {
        if (failure) throw failure;
        if (receipt !== null || child.exitCode !== null) throw Error('Child ended before replying');
        const remaining = deadline - Date.now();
        if (remaining <= 0) throw Error('Timed out waiting for child frame');
        await new Promise(resolve => { wake = resolve; setTimeout(resolve, remaining).unref(); });
      }
      consumed++;
      return decodeFrame(frames.shift(), session, consumed, maxFrameBytes);
    },
    async waitStarted(timeoutMs = 15000) {
      const deadline = Date.now() + timeoutMs;
      while (started === null) {
        if (failure) throw failure;
        if (child.exitCode !== null) throw Error('Runner ended before start');
        const remaining = deadline - Date.now();
        if (remaining <= 0) throw Error('Timed out waiting for runner start');
        await new Promise(resolve => { wake = resolve; setTimeout(resolve, remaining).unref(); });
      }
      return started;
    },
    async close(timeoutMs = 20000) {
      child.stdin.end();
      const timer = setTimeout(() => child.kill(), timeoutMs);
      const exit = await exited;
      clearTimeout(timer);
      if (failure) throw failure;
      return { exit, receipt, sent, consumed, unread: frames.length, stderr };
    },
    kill() { child.kill(); return exited; },
  };
}
module.exports = { openInteractive };
