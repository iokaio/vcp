// SPDX-License-Identifier: Apache-2.0
// Synthetic 130-version history on explicitly verified production bytes.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { createRequire } from 'node:module';
import { launchLocal } from '../../src/packages/sdk-ts/dist/index.js';
const require = createRequire(import.meta.url);
const { verifyDirectory } = require('./production-package.cjs');
const input = JSON.parse(fs.readFileSync(process.argv[2], 'utf8').replace(/^\uFEFF/, ''));
assert.equal(input.versions.length, 130);
assert(['Files', 'Sqlite'].includes(input.backend));
const initialize = { protocol_version: '1.0', client: { name: 'production-startup130', version: '1' }, capabilities: ['task/read'], required_capabilities: ['task/read'] };
const bootstrap = { schema: 'vcp-local-bootstrap/1', workspace: input.workspace, data: input.data, role: 'observer', transport: 'stdio', observer_reconnect: true };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function bounded(promise, ms, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(label)), ms); })]); }
  finally { clearTimeout(timer); }
}
const normalize = value => path.resolve(value).replace(/^\\\\\?\\/, '').toLowerCase();
function verify() {
  const identity = verifyDirectory(input.package_result, path.dirname(input.executable));
  assert.equal(identity.executable_sha256, input.executable_sha256);
}
const event = (pid, role) => fs.appendFileSync(input.process_events, JSON.stringify({ pid, role, at: new Date().toISOString() }) + '\n');
const alive = pid => { try { process.kill(pid, 0); return true; } catch (error) { if (error.code === 'ESRCH') return false; throw error; } };
const ownerPID = () => { try { return JSON.parse(fs.readFileSync(path.join(input.canonical_root, 'owner.lock'), 'utf8')).pid; } catch { return undefined; } };
function ownedServer(parent) {
  assert(Number.isInteger(parent) && parent > 0);
  const script = '$rows=@(Get-CimInstance Win32_Process -Filter ("ParentProcessId = " + $env:VCP_STARTUP_PARENT) | Select-Object ProcessId,ExecutablePath); ConvertTo-Json -InputObject $rows -Compress';
  const rows = JSON.parse(execFileSync(path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'), ['-NoProfile', '-Command', script], {
    windowsHide: true, timeout: 5000, maxBuffer: 64 * 1024, encoding: 'utf8', env: { ...process.env, VCP_STARTUP_PARENT: String(parent) },
  }));
  return rows.find(row => row.ExecutablePath && normalize(row.ExecutablePath) === normalize(input.executable))?.ProcessId;
}
async function probe(cancel) {
  verify();
  const priorOwner = ownerPID(), began = performance.now(), result = { command: [input.executable, 'local-bridge'], deadline_ms: 75000 };
  const child = spawn(input.executable, ['local-bridge'], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const closed = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal })); });
  event(child.pid, cancel || 'native');
  let buffered = '', outputBytes = 0, ready = false, initStart, parseError, server;
  child.stderr.on('data', bytes => { outputBytes += bytes.length; if (outputBytes > 2 * 1024 * 1024) child.kill(); });
  child.stdin.on('error', () => {});
  child.stdout.on('data', bytes => {
    try {
      outputBytes += bytes.length; assert(outputBytes <= 2 * 1024 * 1024);
      buffered += bytes.toString();
      let newline;
      while ((newline = buffered.indexOf('\n')) >= 0) {
        const row = JSON.parse(buffered.slice(0, newline)); buffered = buffered.slice(newline + 1);
        if (!ready) {
          assert.equal(cancel, undefined, 'cancellation must occur during actual store replay');
          assert.equal(row.schema, 'vcp-local-ready/1');
          ready = true; result.ready_ms = Math.round(performance.now() - began); initStart = performance.now();
          child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: initialize }) + '\n');
        } else { result.initialize_ms = Math.round(performance.now() - initStart); assert(row.result); child.stdin.end(); }
      }
    } catch (error) { parseError = error; child.stdin.end(); }
  });
  child.stdin.write(JSON.stringify(bootstrap) + '\n');
  try {
    await delay(250); server = ownedServer(child.pid);
    if (server) event(server, `${cancel || 'native'}-server`);
    if (cancel) {
      assert(Number.isInteger(server) && server > 0 && server !== priorOwner && server !== process.pid, 'new bridge owns this native server');
      assert(alive(server)); assert.equal(ready, false);
      const cancelled = performance.now();
      if (cancel === 'eof') child.stdin.end(); else child.stdin.write('{}\n');
      const exit = await bounded(closed, 10000, 'cancellation process did not close');
      result.cancel_ms = Math.round(performance.now() - cancelled);
      assert(result.cancel_ms < 5000, 'native cancellation precedes SDK forced bridge termination');
      assert.equal(exit.signal, null); assert.notEqual(exit.code, 0); assert.equal(ready, false);
      assert.equal(alive(server), false, 'owned native server must stop'); result.owned_server_stopped = true;
    } else {
      const exit = await bounded(closed, 75000, 'native readiness/close deadline');
      assert.equal(exit.code, 0); assert.equal(exit.signal, null); assert(ready);
      assert(result.ready_ms < 60000); assert(result.initialize_ms < 10000);
    }
    if (parseError) throw parseError;
    result.wall_ms = Math.round(performance.now() - began);
    return result;
  } finally {
    if (child.exitCode === null && child.signalCode === null) {
      child.stdin.end();
      try { await bounded(closed, 5000, 'graceful cleanup deadline'); }
      catch { child.kill(); await bounded(closed, 10000, 'forced cleanup deadline'); }
    }
  }
}
const result = { ok: false, backend: input.backend, versions: input.versions.length };
try {
  result.eof = await probe('eof'); result.early_frame = await probe('early-frame'); result.native = await probe();
  verify();
  const began = performance.now(); let client;
  try {
    client = await bounded(launchLocal({ executable: input.executable, workspace: input.workspace, data: input.data, role: 'observer', transport: 'stdio', initialize }), 75000, 'SDK startup deadline');
    result.sdk_ready_initialize_ms = Math.round(performance.now() - began);
    assert(result.sdk_ready_initialize_ms < 75000);
    const task = await bounded(client.call('task/read', { scope: input.scope, task: input.task }), 10000, 'SDK task read deadline');
    assert.equal(task.value.task, input.task); result.sdk_ok = true;
  } finally { if (client) await bounded(client.dispose(), 10000, 'SDK disposal deadline'); }
  verify(); result.ok = true;
} catch (error) { result.error = error.message; process.exitCode = 1; }
process.stdout.write(JSON.stringify(result));
