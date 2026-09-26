// SPDX-License-Identifier: Apache-2.0
'use strict';
// Trusted parent side of the interactive Windows Node fixture runner. Probes send
// frames and judge the decoded replies; the contained child never supplies a verdict.
const { spawn, spawnSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { decodeFrame } = require('./node-fixture-protocol.cjs');
const runner = path.join(__dirname, 'node-fixture-runner.ps1');
const lineLimit = 196608, streamLimit = 2097152, cleanupMargin = 30000;
const profilePattern = /^iokaio\.vcp\.memory\.[a-f0-9]{32}$/;
function alive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) { return error.code !== 'ESRCH'; }
}
// Supervisor reconciliation after a runner ended without its own cleanup receipt.
// Only the exact profile recorded in the runner's `started` envelope is deleted.
async function reconcileProfile(started, timeoutMs = 5000) {
  if (!started || !Number.isSafeInteger(started.pid) || !profilePattern.test(started.profile)) throw Error('Invalid profile identity');
  const deadline = Date.now() + timeoutMs;
  while (alive(started.pid)) {
    if (Date.now() >= deadline) throw Error('Contained child survived its owner');
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  const folder = path.join(process.env.LOCALAPPDATA, 'Packages', started.profile);
  const script = "Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class FixtureProfileCleanup { " +
    "[DllImport(\"userenv.dll\", CharSet=CharSet.Unicode)] public static extern int DeleteAppContainerProfile(string name); }'; " +
    `[Runtime.InteropServices.Marshal]::ThrowExceptionForHR([FixtureProfileCleanup]::DeleteAppContainerProfile('${started.profile}'))`;
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', timeout: 30000, windowsHide: true });
  if (result.error || result.status !== 0) throw Error('Profile reconciliation failed');
  if (fs.existsSync(folder)) throw Error('Owned profile survived reconciliation');
  return { profile: started.profile, reconciled: true };
}
function openInteractive(configPath, { cwd = path.resolve(__dirname, '../..'), maxFrameBytes = 65536 } = {}) {
  const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
  if (!Number.isSafeInteger(config.timeout_ms) || config.timeout_ms < 1) throw Error('Invalid interactive timeout');
  const session = crypto.randomBytes(16).toString('hex');
  const child = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configPath],
    { cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  const frames = [], waiters = [];
  let pending = '', received = 0, stderr = '', receipt = null, started = null, runnerFailure = null, candidateFailure = null, closed = false, sent = 0, consumed = 0;
  const exited = new Promise(resolve => child.on('close', (code, signal) => {
    closed = true;
    if (pending !== '') fail(Error('Partial runner envelope'));
    notify(); resolve({ code, signal });
  }));
  function notify() { for (const resume of waiters.splice(0)) resume(); }
  function fail(error) { runnerFailure ??= error; notify(); }
  function abort(message) { fail(Error(message)); child.kill(); }
  async function wait(deadline) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) return false;
    let timer;
    await new Promise(resolve => { waiters.push(resolve); timer = setTimeout(resolve, remaining); });
    clearTimeout(timer);
    return true;
  }
  child.on('error', fail);
  child.stdin.on('error', () => {});
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', text => { if (stderr.length < 65536) stderr += text; });
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', text => {
    received += text.length;
    if (received > streamLimit) return abort('Runner output exceeds ceiling');
    pending += text;
    for (let index; (index = pending.indexOf('\n')) >= 0; pending = pending.slice(index + 1)) {
      const line = pending.slice(0, index);
      if (line.length > lineLimit) return abort('Runner envelope exceeds ceiling');
      let envelope;
      try { envelope = JSON.parse(line); } catch { return abort('Malformed runner envelope'); }
      const keys = envelope && typeof envelope === 'object' ? Object.keys(envelope) : [];
      if (keys.length !== 1) return abort('Unexpected runner envelope');
      if (keys[0] === 'started' && started === null && receipt === null) {
        const identity = envelope.started;
        if (!identity || !Number.isSafeInteger(identity.pid) || identity.pid <= 0 || !profilePattern.test(identity.profile)) {
          return abort('Invalid runner start identity');
        }
        started = identity;
      }
      else if (keys[0] === 'frame' && started !== null && receipt === null) frames.push(envelope.frame);
      else if (keys[0] === 'receipt' && started !== null && receipt === null) receipt = envelope.receipt;
      else return abort('Unexpected runner envelope order');
    }
    if (pending.length > lineLimit) return abort('Runner envelope exceeds ceiling');
    notify();
  });
  child.stdin.write(JSON.stringify({ session }) + '\n');
  return {
    session,
    get started() { return started; },
    send(body) { sent++; child.stdin.write(JSON.stringify({ seq: sent, body }) + '\n'); },
    async receive(timeoutMs = 5000) {
      const deadline = Date.now() + timeoutMs;
      while (frames.length === 0) {
        if (runnerFailure || candidateFailure) throw runnerFailure ?? candidateFailure;
        if (receipt !== null || closed) throw Error('Child ended before replying');
        if (!await wait(deadline)) throw Error('Timed out waiting for child frame');
      }
      if (runnerFailure || candidateFailure) throw runnerFailure ?? candidateFailure;
      consumed++;
      try { return decodeFrame(frames.shift(), session, consumed, maxFrameBytes); }
      catch (error) { candidateFailure ??= error; notify(); throw error; } // A rejected frame poisons the session.
    },
    async waitStarted(timeoutMs = 15000) {
      const deadline = Date.now() + timeoutMs;
      while (started === null) {
        if (runnerFailure || candidateFailure) throw runnerFailure ?? candidateFailure;
        if (closed) throw Error('Runner ended before start');
        if (!await wait(deadline)) throw Error('Timed out waiting for runner start');
      }
      return started;
    },
    // Close input, then wait for the runner's own deadline plus its cleanup. A run
    // without a receipt is reconciled by profile identity and never passes.
    async close(timeoutMs = config.timeout_ms + cleanupMargin) {
      child.stdin.end();
      let killed = false;
      const timer = setTimeout(() => { killed = true; child.kill(); }, timeoutMs);
      const exit = await exited;
      clearTimeout(timer);
      if (receipt === null) {
        if (started !== null) await reconcileProfile(started);
        throw Error(killed ? 'Runner exceeded its close deadline' : 'Runner ended without a receipt', { cause: runnerFailure ?? candidateFailure });
      }
      if (exit.code !== 0 || exit.signal !== null || stderr !== '') runnerFailure ??= Error('Runner failed after receipt');
      // Runner protocol faults take precedence even if a bad candidate frame
      // poisoned the session earlier. Preserve the first runner diagnostic.
      const failure = runnerFailure ?? candidateFailure;
      if (failure) { failure.receipt = receipt; throw failure; }
      return { exit, receipt, sent, consumed, unread: frames.length, stderr };
    },
    // Simulates abrupt owner loss; the caller reconciles the recorded profile.
    kill() { child.kill(); return exited; },
  };
}
module.exports = { openInteractive, reconcileProfile };
