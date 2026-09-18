// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { spawnSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { SUITES, parseArgs, outsideSource, validateResults } = require('../support/gemini-baseline.cjs');

test('Gemini command rejects incomplete, duplicate and unknown requests before allocation', () => {
  for (const args of [[], ['--source'], ['--source', '--prepare'], ['--other', 'x'],
    ['--source', 'x', '--output-root', 'y', '--prepare', '--prepare'], ['--source', 'x', '--source', 'y', '--output-root', 'z']]) assert.throws(() => parseArgs(args));
  assert.deepEqual(parseArgs(['--source', 'space path', '--output-root', 'out', '--prepare']),
    { prepare: true, '--source': 'space path', '--output-root': 'out' });
});
test('Gemini evidence cannot enter the source through direct or junction paths', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const source = path.join(fixture.root, 'source'); fs.mkdirSync(source);
    assert.throws(() => outsideSource(source, path.join(source, 'new/output')), /outside the source/);
    assert.equal(outsideSource(source, path.join(fixture.root, 'new/output')).outputRoot, path.join(fixture.root, 'new/output'));
    const alias = path.join(fixture.root, 'source-alias');
    fs.symlinkSync(source, alias, process.platform === 'win32' ? 'junction' : 'dir');
    try { assert.throws(() => outsideSource(source, path.join(alias, 'new/output')), /outside the source/); }
    finally { fs.unlinkSync(alias); }
    assert.equal(fs.existsSync(path.join(source, 'new')), false);
  } finally { fixture.cleanup(); }
});
test('a wrong checkout never reaches preparation or test execution', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const repository = path.resolve(__dirname, '../../..');
    const output = path.join(fixture.root, 'evidence');
    const result = spawnSync(process.execPath, [path.join(repository, 'scripts/upstream/test-gemini.cjs'),
      '--source', repository, '--output-root', output, '--prepare'], { encoding: 'utf8', timeout: 30000, windowsHide: true });
    assert.equal(result.error, undefined);
    assert.equal(result.status, process.platform === 'win32' ? 1 : 3, result.stdout + result.stderr);
    const record = JSON.parse(fs.readFileSync(path.join(output, fs.readdirSync(output)[0], 'manifest.json')));
    assert.equal(record.status, process.platform === 'win32' ? 'fail' : 'not_run');
    assert.deepEqual(record.stages, []);
  } finally { fixture.cleanup(); }
});
test('Gemini result validation rejects missing, duplicate, failed and filtered coverage', () => {
  const core = path.resolve('synthetic-core');
  const report = { success: true, numTotalTests: 402, numPassedTests: 402, numFailedTests: 0, numFailedTestSuites: 0,
    numPendingTests: 0, numTodoTests: 0, snapshot: { failure: false }, testResults: Object.entries(SUITES).map(([name, count]) => ({
      name: path.join(core, name), status: 'passed', assertionResults: Array.from({ length: count }, (_, i) => ({ fullName: name + i, status: 'passed', failureMessages: [] }))
    })) };
  assert.deepEqual(validateResults(report, core), { suites: 8, tests: 402, status: 'pass' });
  for (const change of [r => { r.numPassedTests = 0; }, r => { r.testResults.pop(); },
    r => { r.testResults[1] = r.testResults[0]; }, r => { r.testResults[0].assertionResults.pop(); },
    r => { r.testResults[0].assertionResults[0].status = 'failed'; }, r => { r.snapshot.failure = true; }]) {
    const invalid = structuredClone(report); change(invalid); assert.throws(() => validateResults(invalid, core), /Gemini/);
  }
});

test('missing development tools produce not_run without starting upstream stages', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const repository = path.resolve(__dirname, '../../..');
    for (const relative of ['scripts/upstream/test-gemini.cjs', 'src/tests/support/gemini-baseline.cjs', 'src/tests/support/harness.cjs']) {
      const target = path.join(fixture.root, relative);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.copyFileSync(path.join(repository, relative), target);
    }
    const output = path.join(fixture.root, 'evidence');
    const result = spawnSync(process.execPath, [path.join(fixture.root, 'scripts/upstream/test-gemini.cjs'),
      '--source', path.join(fixture.root, 'missing-source'), '--output-root', output, '--prepare'],
    { encoding: 'utf8', timeout: 30000, windowsHide: true });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 3, result.stdout + result.stderr);
    const record = JSON.parse(fs.readFileSync(path.join(output, fs.readdirSync(output)[0], 'manifest.json')));
    assert.equal(record.status, 'not_run');
    assert.deepEqual(record.stages, []);
    if (process.platform === 'win32') assert.match(record.reason, /Install VCP development tools/);
  } finally { fixture.cleanup(); }
});
