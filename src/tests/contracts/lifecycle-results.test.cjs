// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { cases, validateResults, testBinary } = require('../support/lifecycle-results.cjs');
const fixture = group => cases[group].map(name => `test suite::turn_input_submission::${name} ... ok`).join('\n') +
  '\n\ntest result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 1582 filtered out; finished in 0.12s\n';
test('lifecycle evidence requires every declared native success', () => {
  for (const group of Object.keys(cases)) assert.equal(validateResults(fixture(group), group), 5);
});
test('empty, skipped, duplicate and missing native cases cannot pass', () => {
  const valid = fixture('continuation');
  for (const text of ['', valid.replace(' ... ok', ' ... ignored'), valid.replace(/^.*\n/, ''),
    valid.replace(cases.continuation[0], cases.continuation[1]), valid.replace('5 passed', '0 passed'),
    valid.replace('0 ignored', '1 ignored'), valid + valid]) {
    assert.throws(() => validateResults(text, 'continuation'));
  }
});
test('only the successful core integration artifact is executable', () => {
  const artifact = { reason: 'compiler-artifact', package_id: 'path+file:///repo/core#codex-core@0.0.0',
    target: { name: 'all', kind: ['test'] }, profile: { test: true }, executable: 'C:/owned/all.exe' };
  const finished = { reason: 'build-finished', success: true };
  const log = rows => rows.map(row => JSON.stringify(row)).join('\n');
  assert.equal(testBinary(log([artifact, finished])), artifact.executable);
  for (const rows of [[artifact], [artifact, artifact, finished], [{ ...artifact, package_id: 'foreign@0' }, finished],
    [{ ...artifact, profile: { test: false } }, finished], [artifact, { ...finished, success: false }]]) {
    assert.throws(() => testBinary(log(rows)));
  }
});
