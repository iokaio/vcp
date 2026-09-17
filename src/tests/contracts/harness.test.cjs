// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { runSuite, parseSelection, redactor, environment, hashCommand } = require('../support/harness.cjs');
function registry(code, overrides = {}) {
  return { schema_version: 1, suites: { fast: ['sample'] }, cases: { sample: {
    args: ['-e', code], task_ids: ['P0-01'], backends: ['none'], requires: [],
    timeout_ms: 5000, max_output_bytes: 65536, ...overrides
  } } };
}
function temporary(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-harness-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}
function run(t, config, extra = {}) {
  return runSuite({ root: temporary(t), outputRoot: temporary(t), registry: config,
    selection: extra.selection || parseSelection([], config), source: { fixture: 'synthetic' }, ...extra });
}
test('source hashing streams beyond the old 64 MiB buffer and rejects partial failure', async () => {
  const hash = require('node:crypto').createHash('sha256');
  const chunk = Buffer.alloc(1024 * 1024, 120);
  for (let i = 0; i < 65; i++) hash.update(chunk);
  const actual = await hashCommand(process.execPath, ['-e', 'const fs=require("node:fs");const b=Buffer.alloc(1024*1024,120);for(let i=0;i<65;i++)fs.writeSync(1,b);']);
  assert.equal(actual, hash.digest('hex'));
  await assert.rejects(hashCommand(process.execPath, ['-e', 'console.log("partial");process.exit(7)']), /failed: 7/);
});
test('rejects invalid requests before allocating evidence', async t => {
  const config = registry('process.exit(0)');
  for (const args of [['--suite', 'absent'], ['--case', 'absent'], ['--backend', 'both'],
    ['--backend', 'typo'], ['--suite'], ['--typo', 'fast'], ['--suite', 'fast', '--suite', 'fast']]) {
    assert.throws(() => parseSelection(args, config));
  }
  const outputRoot = path.join(temporary(t), 'must-not-exist');
  await assert.rejects(runSuite({ registry: config, selection: { suite: 'fast', ids: ['sample'], backends: ['none', 'none'] }, outputRoot }));
  assert.equal(fs.existsSync(outputRoot), false);
});
test('records failure, preserves child exit code and distinct retries', async t => {
  const config = registry('console.log("synthetic failure"); process.exit(7)');
  const outputRoot = temporary(t);
  const first = await run(t, config, { outputRoot });
  const second = await run(t, config, { outputRoot });
  assert.equal(first.exitCode, 7);
  assert.equal(first.manifest.status, 'fail');
  assert.notEqual(first.manifest.run_id, second.manifest.run_id);
  assert.deepEqual(JSON.parse(fs.readFileSync(first.manifestPath)), first.manifest);
  const attempt = first.manifest.attempts[0];
  const log = fs.readFileSync(path.join(path.dirname(first.manifestPath), attempt.artifacts[0].path), 'utf8');
  assert.match(log, /synthetic failure/);
});
test('missing prerequisite remains not_run and does not invoke child', async t => {
  const result = await run(t, registry('process.exit(9)', { requires: [{ kind: 'file', value: 'missing' }] }));
  assert.equal(result.exitCode, 3);
  assert.equal(result.manifest.status, 'not_run');
  assert.equal(result.manifest.attempts[0].exit_code, null);
});
test('both backend attempts are required for success', async t => {
  const config = registry('process.exit(process.argv[1] === "files" ? 8 : 0)', {
    args: ['-e', 'process.exit(process.argv[1] === "files" ? 8 : 0)', '{backend}'], backends: ['sqlite', 'files']
  });
  const result = await run(t, config, { selection: parseSelection(['--backend', 'both'], config) });
  assert.deepEqual(result.manifest.attempts.map(a => [a.backend, a.status]), [['sqlite', 'pass'], ['files', 'fail']]);
  assert.equal(result.exitCode, 8);
});
test('timeout and output overflow fail with bounded logs', async t => {
  const timeout = await run(t, registry('setInterval(() => {}, 100)', { timeout_ms: 150 }));
  assert.equal(timeout.manifest.attempts[0].reason, 'timeout');
  assert.notEqual(timeout.exitCode, 0);
  const overflow = await run(t, registry('process.stdout.write("x".repeat(200000))', { max_output_bytes: 128 }));
  assert.equal(overflow.manifest.attempts[0].reason, 'output_limit');
  assert.equal(overflow.manifest.attempts[0].capture_complete, false);
  assert.ok(overflow.manifest.attempts[0].artifacts.every(a => a.bytes <= 128));
});
test('cancellation fails an active attempt and prevents subsequent dispatch', async t => {
  const config = registry('setInterval(() => {}, 100)');
  config.cases.later = { ...config.cases.sample, args: ['-e', 'process.exit(0)'] };
  config.suites.fast.push('later');
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 200);
  t.after(() => clearTimeout(timer));
  const result = await run(t, config, { signal: controller.signal });
  assert.equal(result.exitCode, 130);
  assert.deepEqual(result.manifest.attempts.map(a => a.status), ['fail', 'not_run']);
});
test('recorder failure prevents a success result', async t => {
  const file = path.join(temporary(t), 'file');
  fs.writeFileSync(file, 'occupied');
  await assert.rejects(run(t, registry('process.exit(0)'), { outputRoot: file }));
});
test('failure to finalize evidence after dispatch cannot return success', async t => {
  const outputRoot = temporary(t);
  await assert.rejects(run(t, registry('process.exit(0)'), { outputRoot,
    announce() {
      const manifest = path.join(outputRoot, fs.readdirSync(outputRoot)[0], 'manifest.json');
      fs.unlinkSync(manifest);
      fs.mkdirSync(manifest);
    }
  }));
});
test('redacts secrets across every chunk boundary and preserves UTF-8', () => {
  const input = Buffer.from('before synthetic-secret 🧪 after');
  for (let split = 0; split <= input.length; split++) {
    let output = '';
    const sink = redactor(s => { output += s; }, ['synthetic-secret']);
    sink.push(input.subarray(0, split)); sink.push(input.subarray(split)); sink.end();
    assert.equal(output, 'before [REDACTED] 🧪 after');
  }
});
test('ordinary checks cannot inherit provider credentials or Node startup injection', () => {
  const prior = process.env.NODE_OPTIONS;
  process.env.NODE_OPTIONS = '--inspect';
  try { assert.equal(environment().NODE_OPTIONS, undefined); }
  finally { if (prior === undefined) delete process.env.NODE_OPTIONS; else process.env.NODE_OPTIONS = prior; }
  assert.ok(Object.keys(environment()).every(key => !/TOKEN|KEY|SECRET/i.test(key)));
});
test('PowerShell wrapper selects one Node executable when PATH has two installations', t => {
  const root = temporary(t);
  const directories = ['first', 'second'].map(name => path.join(root, name));
  for (const directory of directories) {
    fs.mkdirSync(directory);
    const executable = path.join(directory, process.platform === 'win32' ? 'node.exe' : 'node');
    try { fs.linkSync(process.execPath, executable); }
    catch (error) { if (error.code !== 'EXDEV') throw error; fs.copyFileSync(process.execPath, executable); }
  }
  const env = environment();
  const key = Object.keys(env).find(k => k.toUpperCase() === 'PATH') || 'PATH';
  env[key] = [...directories, env[key]].join(path.delimiter);
  const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/test.ps1'), '-Suite', 'absent'],
    { cwd: root, env, encoding: 'utf8', timeout: 20000, windowsHide: true });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 2, result.stderr);
  assert.match(result.stderr, /Unknown suite: absent/);
  assert.equal(fs.existsSync(path.join(root, 'artifacts')), false);
});
