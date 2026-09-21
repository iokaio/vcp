// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { materialize, snapshot, execute, plan } = require('../../../scripts/evals/builtin-toolchain-qualification.cjs');
const fixtures = path.resolve(__dirname, '../../evals/skills/builtin');
const manifest = JSON.parse(fs.readFileSync(path.join(fixtures, 'manifest.json'), 'utf8'));

test('native qualification executes declared fixture bytes in fresh copies and preserves originals', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    const before = snapshot(fixtures);
    const fixture = manifest.cases.find(c => c.id === 'testing-normal-v1');
    const target = path.join(temp.root, 'copy');
    materialize(fixtures, fixture, target);
    assert.equal(execute(process.execPath, ['--test', 'unit.cjs'], target).status, 'passed');
    fs.writeFileSync(path.join(target, 'amount.cjs'), 'throw Error("owned copy only");');
    assert.equal(execute(process.execPath, ['--test', 'unit.cjs'], target).status, 'failed');
    assert.throws(() => materialize(fixtures, fixture, target), /exist/i);
    assert.deepEqual(snapshot(fixtures), before);
  } finally { temp.cleanup(); }
});

test('copies exclude undeclared generated files and reject changed bytes or escaping paths', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    const source = path.join(temp.root, 'source');
    fs.mkdirSync(path.join(source, 'project'), { recursive: true });
    fs.writeFileSync(path.join(source, 'project/file'), 'original');
    fs.writeFileSync(path.join(source, 'project/generated'), 'do not copy');
    const fixture = { id: 'sample', project: 'project', expected: { preserve_files: [
      { path: 'file', bytes: 8, sha256: crypto.createHash('sha256').update('original').digest('hex') },
    ] } };
    const target = path.join(temp.root, 'copy');
    materialize(source, fixture, target);
    assert.deepEqual(fs.readdirSync(target), ['file']);
    fs.writeFileSync(path.join(source, 'project/file'), 'modified');
    assert.throws(() => materialize(source, fixture, path.join(temp.root, 'changed')), /hash mismatch/);
    for (const unsafe of ['../file', '/file', 'C:/file', 'folder\\file']) {
      const changed = structuredClone(fixture);
      changed.expected.preserve_files[0].path = unsafe;
      assert.throws(() => materialize(source, changed, path.join(temp.root, 'unsafe')), /Unsafe/);
    }
  } finally { temp.cleanup(); }
});

test('failed, missing and timed out checks retain actual receipts', () => {
  const failed = execute(process.execPath, ['-e', 'process.stderr.write("seeded failure"); process.exit(7)'], os.tmpdir());
  assert.equal(failed.exit_code, 7);
  assert.equal(failed.stderr, 'seeded failure');
  assert.equal(failed.status, 'failed');
  const absent = execute('vcp-fixture-missing-executable-986183', [], os.tmpdir());
  assert.equal(absent.error, 'ENOENT');
  assert.equal(absent.status, 'failed');
  const timeout = execute(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], os.tmpdir(), { timeout: 100 });
  assert.equal(timeout.error, 'ETIMEDOUT');
  assert.equal(timeout.status, 'failed');
});

test('synthetic negative state and unavailable pinned toolchains never authorize execution', () => {
  for (const fixture of manifest.cases.filter(c => c.kind !== 'normal')) {
    assert.equal(plan(fixture, {}).steps, undefined);
  }
  const rust = manifest.cases.find(c => c.skill === 'rust' && c.kind === 'normal');
  assert.equal(plan(rust, {}).steps, undefined);
  const selected = plan(rust, { rustup: { stdout: 'stable-x86_64-pc-windows-msvc\n1.95.0-x86_64-pc-windows-msvc\n' } });
  assert.deepEqual(selected.steps[0].args.slice(0, 4), ['run', '1.95.0-x86_64-pc-windows-msvc', 'cargo', 'test']);
  assert.ok(selected.steps[0].args.includes('--offline'));
  assert.ok(selected.steps[0].args.includes('--locked'));
  const cpp = manifest.cases.find(c => c.skill === 'cpp' && c.kind === 'normal');
  assert.equal(plan(cpp, { cmake: { status: 'passed' } }).steps, undefined);
  const native = plan(cpp, Object.fromEntries(['cmake', 'ninja', 'ctest'].map(t => [t, { status: 'passed' }])));
  assert.equal(native.steps.length, 3);
  assert.ok(native.steps[2].args.includes('--no-tests=error'));
});
