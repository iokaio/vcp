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

test('SQLite checks exercise existing rows and retain seeded failure in an isolated memory database', () => {
  // This recipe requires Node's installed SQLite; absence is not a skipped pass.
  const temp = ownedRoot(os.tmpdir());
  try {
    const before = snapshot(fixtures);
    const fixture = manifest.cases.find(c => c.id === 'sql-normal-v1');
    const target = path.join(temp.root, 'sql');
    materialize(fixtures, fixture, target);
    assert.equal(plan(fixture, {}).steps, undefined);
    const recipe = plan(fixture, { sqlite: { status: 'passed' } });
    const receipts = recipe.steps.map(step => execute(step.command, step.args, target));
    assert.equal(receipts[0].status, 'passed', receipts[0].stderr);
    assert.equal(receipts[1].status, 'failed');
    assert.match(receipts[1].stderr, /rollback preserved existing row/);
    assert.match(receipts[1].stderr, /NOT NULL/);
    fs.writeFileSync(path.join(target, 'migrations/002_email.sql'), 'ALTER TABLE customer ADD COLUMN email TEXT;');
    const corrected = execute(recipe.steps[1].command, recipe.steps[1].args, target);
    assert.equal(corrected.status, 'passed', corrected.stderr);
    assert.deepEqual(fs.readdirSync(target).sort(), ['AGENTS.md', 'migration-policy.md', 'migrations']);
    assert.deepEqual(snapshot(fixtures), before);
  } finally { temp.cleanup(); }
});

test('data recipe requires a discovered Python 3 and does not qualify the pinned Python project', () => {
  const fixture = manifest.cases.find(c => c.id === 'data-normal-v1');
  for (const inventory of [{}, { python: { status: 'failed', stdout: 'Python 3.13.7' } },
    { python: { status: 'passed', stdout: 'Python 2.7.18' } }]) {
    assert.equal(plan(fixture, inventory).steps, undefined);
  }
  const inventory = { python: { status: 'passed', stdout: 'Python 3.13.7\n' } };
  const recipe = plan(fixture, inventory);
  assert.deepEqual(recipe.steps.map(step => step.args.at(-1)), ['valid', 'duplicates']);
  assert.ok(recipe.steps.every(step => step.args.includes('-I') && step.args.includes('-B')));
  assert.equal(plan(manifest.cases.find(c => c.id === 'python-normal-v1'), inventory).steps, undefined);
});

test('data checks retain duplicate-key failure and preserve frozen source and provenance', t => {
  const probe = execute('python', ['--version'], os.tmpdir());
  if (probe.status !== 'passed' || !/^Python 3\./.test(probe.stdout.trim())) {
    t.skip('Native data helper not-run: Python 3 is unavailable; discovery contract tested separately.');
    return;
  }
  const temp = ownedRoot(os.tmpdir());
  try {
    const before = snapshot(fixtures);
    const fixture = manifest.cases.find(c => c.id === 'data-normal-v1');
    const target = path.join(temp.root, 'data with spaces');
    materialize(fixtures, fixture, target);
    const copied = snapshot(target);
    const recipe = plan(fixture, { python: probe });
    const receipts = recipe.steps.map(step => execute(step.command, step.args, target));
    assert.equal(receipts[0].status, 'passed', receipts[0].stderr);
    assert.equal(receipts[1].status, 'failed');
    assert.match(receipts[1].stderr, /Duplicate key 1 was accepted; aggregate 20.75/);
    assert.deepEqual(snapshot(target), copied);
    fs.writeFileSync(path.join(target, 'transform.py'), [
      'from decimal import Decimal', 'def total(rows):',
      '    if len({r["id"] for r in rows}) != len(rows):',
      '        raise ValueError("duplicate key")',
      '    return sum((Decimal(r["amount"]) for r in rows if r["amount"]), Decimal(0))', '',
    ].join('\n'));
    for (const step of recipe.steps) {
      const corrected = execute(step.command, step.args, target);
      assert.equal(corrected.status, 'passed', corrected.stderr);
    }
    // Refusing every row must fail the positive check, never qualify validation.
    fs.writeFileSync(path.join(target, 'transform.py'), 'def total(rows):\n    raise ValueError("always reject")\n');
    assert.equal(execute(recipe.steps[0].command, recipe.steps[0].args, target).status, 'failed');
    assert.deepEqual(fs.readdirSync(target).sort(), ['AGENTS.md', 'input.csv', 'schema.json', 'transform.py']);
    assert.deepEqual(snapshot(fixtures), before);
  } finally { temp.cleanup(); }
});
