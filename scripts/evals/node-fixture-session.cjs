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
// Pick the supervisor-owned identity before the runner can create any profile.
// The runner's atomic CreateAppContainerProfile must succeed for that new name;
// it never opens an existing profile. No caller-supplied cleanup target is used.
function reserveProfile() {
  for (let attempts = 0; attempts < 16; attempts++) {
    const profile = 'iokaio.vcp.memory.' + crypto.randomBytes(16).toString('hex');
    if (!fs.existsSync(path.join(process.env.LOCALAPPDATA, 'Packages', profile))) return profile;
  }
  throw Error('Owned profile identity could not be reserved');
}
// Run only after observing runner exit. Find child processes by their exact owned
// executable and hold a process handle while waiting. A recycled PID must neither
// delay reconciliation nor cause us to wait for or signal an unrelated process.
async function reconcileOwnedProfile(profile, timeoutMs = 5000) {
  if (!profilePattern.test(profile) || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30000) throw Error('Invalid profile identity');
  const folder = path.join(process.env.LOCALAPPDATA, 'Packages', profile);
  const script = `$ErrorActionPreference = 'Stop'
$profile = '${profile}'
$folder = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) ('Packages/' + $profile)
$program = [IO.Path]::GetFullPath((Join-Path $folder 'AC/node.exe'))
foreach ($process in [Diagnostics.Process]::GetProcessesByName('node')) {
  try {
    try { $null = $process.Handle; $executable = $process.MainModule.FileName }
    catch [InvalidOperationException] { continue }
    # Other users' or elevated Node processes may deny query access. They cannot
    # be the child launched with this broker's token. Never signal those processes.
    catch [ComponentModel.Win32Exception] { if ($_.Exception.NativeErrorCode -eq 5 -or $process.HasExited) { continue }; throw }
    if ($executable -ieq $program -and -not $process.WaitForExit(${timeoutMs})) { throw 'Contained child survived its owner' }
  } finally { $process.Dispose() }
}
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class FixtureProfileCleanup {
  [DllImport("userenv.dll", CharSet=CharSet.Unicode)] public static extern int DeleteAppContainerProfile(string name); }'
$result = [FixtureProfileCleanup]::DeleteAppContainerProfile($profile)
# A runner can die before profile creation, or complete cleanup before losing its
# receipt. Both missing-profile results are idempotent cleanup success.
if ($result -ne 0 -and $result -ne -2147024894 -and $result -ne -2147023728) {
  [Runtime.InteropServices.Marshal]::ThrowExceptionForHR($result)
}
if ([IO.Directory]::Exists($folder)) { throw 'Owned profile survived reconciliation' }`;
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', timeout: 30000, windowsHide: true });
  if (result.error || result.status !== 0) throw Error('Profile reconciliation failed');
  if (fs.existsSync(folder)) throw Error('Owned profile survived reconciliation');
  return { profile, reconciled: true };
}
// Public reconciliation for callers that deliberately simulate owner loss.
async function reconcileProfile(started, timeoutMs = 5000) {
  if (!started || !Number.isSafeInteger(started.pid) || started.pid <= 0 || !profilePattern.test(started.profile)) throw Error('Invalid profile identity');
  return reconcileOwnedProfile(started.profile, timeoutMs);
}
// One-shot supervision uses the same prelaunch ownership and no-receipt cleanup
// as interactive supervision. The child cannot supply the cleanup identity.
async function runSingle(configPath, { cwd = path.resolve(__dirname, '../..') } = {}) {
  const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
  if (!Number.isSafeInteger(config.timeout_ms) || config.timeout_ms < 1 || config.timeout_ms > 180000) throw Error('Runner timeout is invalid');
  const profile = reserveProfile();
  const child = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configPath, '-ProfileName', profile],
    { cwd, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  const output = [];
  let size = 0, stderr = false, failure = null;
  const stop = message => { failure ??= Error(message); child.kill(); };
  const timer = setTimeout(() => stop('Runner exceeded its close deadline'), config.timeout_ms + cleanupMargin);
  child.on('error', error => { failure ??= Error('Runner could not start', { cause: error }); });
  child.stdout.on('data', chunk => {
    size += chunk.length;
    if (size > 1048576) stop('Runner output exceeds ceiling');
    else output.push(chunk);
  });
  child.stderr.on('data', () => { stderr = true; });
  const exit = await new Promise(resolve => child.on('close', (code, signal) => resolve({ code, signal })));
  clearTimeout(timer);
  try {
    if (failure) throw failure;
    if (exit.code !== 0 || exit.signal !== null || stderr) throw Error('Runner failed before a receipt');
    let receipt;
    try { receipt = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(output))); }
    catch (error) { throw Error('Runner receipt is not JSON', { cause: error }); }
    if (receipt?.cleanup !== 'completed') throw Error('Runner cleanup receipt is invalid');
    return receipt;
  } catch (error) {
    await reconcileOwnedProfile(profile);
    throw error;
  }
}
function openInteractive(configPath, { cwd = path.resolve(__dirname, '../..'), maxFrameBytes = 65536 } = {}) {
  const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
  if (!Number.isSafeInteger(config.timeout_ms) || config.timeout_ms < 1) throw Error('Invalid interactive timeout');
  const profile = reserveProfile();
  const session = crypto.randomBytes(16).toString('hex');
  const child = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configPath, '-ProfileName', profile],
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
        if (!identity || !Number.isSafeInteger(identity.pid) || identity.pid <= 0 || identity.profile !== profile) {
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
      if (receipt === null || receipt?.cleanup !== 'completed') {
        await reconcileOwnedProfile(profile);
        const error = Error(receipt !== null ? 'Runner cleanup receipt is invalid' :
          killed ? 'Runner exceeded its close deadline' : 'Runner ended without a receipt', { cause: runnerFailure ?? candidateFailure });
        if (receipt !== null) error.receipt = receipt;
        throw error;
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
module.exports = { openInteractive, reconcileProfile, runSingle };
