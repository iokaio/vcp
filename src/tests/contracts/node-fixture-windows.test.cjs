// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { checkResponse } = require('../../../scripts/evals/node-fixture-protocol.cjs');
const root = path.resolve(__dirname, '../../..');
const runner = path.join(root, 'scripts/evals/node-fixture-runner.ps1');
const bootstrap = path.join(root, 'scripts/evals/node-fixture-bootstrap.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
test('actual Windows pinned Node runner and hostile protocol outputs', { skip: process.platform !== 'win32', timeout: 120000 }, t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-node-protocol-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const nodeHash = hash(process.execPath);
  const candidate = path.join(directory, 'candidate.cjs');
  const requestId = crypto.randomBytes(16).toString('hex');
  function run(code, modify = () => {}) {
    fs.writeFileSync(candidate, code);
    const config = { schema: 1, node: process.execPath, node_sha256: nodeHash,
      bootstrap_sha256: hash(bootstrap), memory_bytes: 268435456, output_limit: 65536, timeout_ms: 3000,
      input_base64: Buffer.from(JSON.stringify({ id: requestId, input: { value: 41 } })).toString('base64'),
      files: [{ name: 'candidate.cjs', path: candidate, sha256: hash(candidate) }] };
    modify(config);
    const configPath = path.join(directory, 'config.json'); fs.writeFileSync(configPath, JSON.stringify(config));
    const child = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Config', configPath], {
      cwd: root, encoding: 'utf8', timeout: 15000, maxBuffer: 1048576, windowsHide: true });
    assert.ifError(child.error);
    return child;
  }
  const good = run('exports.compute = input => input.value + 1;');
  assert.equal(good.status, 0, good.stderr);
  assert.deepEqual(checkResponse(JSON.parse(good.stdout), requestId, 42), { external_response_pass: true });
  const readonly = run(`exports.compute = () => {
    const fs = require('node:fs'); let changed = 0;
    for (const file of ['candidate.cjs', 'bootstrap.cjs', 'package.json', 'new-file', 'temp/new-file']) {
      try { fs.writeFileSync(file, 'bad'); changed++; } catch {}
    }
    return changed;
  };`);
  assert.equal(readonly.status, 0, readonly.stderr);
  checkResponse(JSON.parse(readonly.stdout), requestId, 0);
  for (const code of [
    'exports.compute = () => 43;',
    'process.stdout.write(JSON.stringify({passed:true}) + "\\n"); exports.compute = () => 42;',
    'exports.compute = () => { process.stdout.write("{}\\n"); return 42; };',
    'exports.compute = () => { process.stderr.write("diagnostic"); return 42; };',
    'exports.compute = () => { process.exit(0); };',
  ]) {
    const result = run(code); assert.equal(result.status, 0, result.stderr);
    assert.throws(() => checkResponse(JSON.parse(result.stdout), requestId, 42));
  }
  for (const modify of [c => c.node_sha256 = '0'.repeat(64), c => c.files[0].sha256 = '0'.repeat(64),
    c => c.files[0].name = '../candidate.cjs', c => c.files.push({ ...c.files[0] }),
    c => c.input_base64 = Buffer.alloc(65537).toString('base64'), c => c.memory_bytes = 1]) {
    const result = run('exports.compute = () => 42;', modify);
    assert.notEqual(result.status, 0, 'Invalid authority or identity unexpectedly launched');
    assert.equal(result.stdout, '');
  }
  assert.equal(hash(process.execPath), nodeHash);
});
