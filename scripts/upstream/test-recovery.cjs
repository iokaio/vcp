// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const assert = require('node:assert/strict');
async function main(argv) {
  if (argv.length !== 4 || argv[0] !== '--owner' || argv[2] !== '--output-root') throw Error('Supply --owner executable --output-root directory');
  if (process.platform !== 'win32') { console.log('not_run: native Windows required'); process.exitCode = 3; return; }
  const executable = path.resolve(argv[1]);
  const output = path.resolve(argv[3]);
  const repository = path.resolve(__dirname, '../..');
  if (!output.startsWith(path.join(repository, 'artifacts') + path.sep)) throw Error('Use an ignored artifacts output root');
  const directory = path.join(output, crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const record = { schema_version: 1, task_id: 'P0-03', status: 'running', started_at: new Date().toISOString(),
    owner_sha256: crypto.createHash('sha256').update(fs.readFileSync(executable)).digest('hex'), attempts: [],
    limitations: ['Synthetic loopback provider; private qualification CLI, not the product CLI or a public control protocol.'] };
  const save = () => fs.writeFileSync(path.join(directory, 'manifest.json'), JSON.stringify(record, null, 2) + '\n');
  function run(name, commands, expectedExit, requests) {
    const result = spawnSync(executable, [path.join(directory, 'owner-state'), String(requests)], {
      input: commands.join('\n') + '\n', encoding: 'utf8', timeout: 60000, maxBuffer: 1024 * 1024,
      windowsHide: true, env: { ...process.env, CODEX_TEST_ENVIRONMENT: 'local', RUST_MIN_STACK: '16777216' }
    });
    for (const stream of ['stdout', 'stderr']) fs.writeFileSync(path.join(directory, `${name}-${stream}.log`), result[stream] || '');
    record.attempts.push({ name, command: [executable, '<disposable-owner-state>', String(requests)], input: commands, exit_code: result.status, signal: result.signal });
    save();
    if (result.error || result.signal || result.status !== expectedExit) throw Error(`${name}: unexpected exit ${result.status}; see captured logs`);
    return result.stdout.split(/\r?\n/).filter(line => line.startsWith('VCP_FIXTURE ')).map(line => JSON.parse(line.slice(12)));
  }
  function row(rows, id) { const matches = rows.filter(row => row.command === id); assert.equal(matches.length, 1, id); return matches[0]; }
  save();
  try {
    const first = run('crash', ['t1 /turn', 'p1 /pause', 's1 /status', 'denied /turn', 'r1 /resume', 't2 /turn', 'proc /process', 'end /crash'], 77, 2);
    assert.equal(row(first, 'p1').paused, true);
    assert.equal(row(first, 's1').requests, 1);
    assert.equal(row(first, 'denied').requests, 1);
    assert.equal(row(first, 't2').requests, 2);
    assert.equal(row(first, 'proc').unresolved, 1);
    const lock = path.join(directory, 'owner-state/native-tree/locked');
    const exitObservedAt = Date.now();
    const deadline = Date.now() + 5000;
    while (true) {
      try { const handle = fs.openSync(lock, 'r+'); fs.closeSync(handle); break; }
      catch (error) { if (Date.now() >= deadline) throw error; await new Promise(resolve => setTimeout(resolve, 5)); }
    }
    record.native_owner_exit = { grandchild_lock_released: true, unresolved_intent_retained: true,
      lock_release_after_owner_exit_ms: Date.now() - exitObservedAt }; save();
    const root = row(first, 'ready').thread;
    const second = run('reopen', ['r1 /resume', 's2 /status', 'receipt /reconcile-process', 'fresh-resume /resume', 't3 /turn', 'q /quit'], 0, 1);
    assert.equal(row(second, 'ready').thread, root);
    assert.equal(row(second, 'ready').paused, true);
    assert.equal(row(second, 'ready').requests, 0);
    assert.equal(row(second, 'ready').work, 3);
    assert.equal(row(second, 'ready').unresolved, 1);
    assert.equal(row(second, 'receipt').unresolved, 0);
    assert.equal(row(second, 'r1').paused, true, 'replaying an old acknowledgement must not resume after recovery');
    assert.equal(row(second, 't3').requests, 1);
    assert.equal(row(second, 't3').work, 4);
    const third = run('child', ['r3 /resume', 'c3 /child', 'cp3 /pause child', 'rp3 /pause', 'rr3 /resume', 'cs3 /status child', 'cr3 /resume child', 'ct3 /turn child', 'q3 /quit'], 0, 1);
    assert.equal(row(third, 'cs3').paused, true);
    assert.equal(row(third, 'ct3').paused, false);
    assert.equal(row(third, 'ct3').requests, 1);
    const fourth = run('child-reopen', ['s4 /status child', 'r4 /resume', 'cs4 /status child', 'q4 /quit'], 0, 0);
    assert.equal(row(fourth, 's4').paused, true);
    assert.equal(row(fourth, 'cs4').paused, false, 'owner close must preserve local versus inherited child holds');
    assert.equal(row(fourth, 'cs4').requests, 0);
    record.status = 'pass'; record.exit_code = 0;
  } catch (error) { record.status = 'fail'; record.exit_code = 1; record.reason = error.message; }
  record.ended_at = new Date().toISOString(); save();
  console.log(JSON.stringify({ status: record.status, reason: record.reason, manifest: path.join(directory, 'manifest.json') }));
  process.exitCode = record.exit_code;
}
main(process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
