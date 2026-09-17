// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { anchors, dependencies, graphErrors } = require('./repository.cjs');
test('heading anchors ignore fenced examples and handle repeated Unicode titles', () => {
  assert.deepEqual([...anchors('# Résumé — One\n## Résumé — One\n```text\n# Hidden\n```\n')], ['résumé--one', 'résumé--one-1']);
});
test('dependency notation expands ranges and shared phases without accepting typos', () => {
  assert.deepEqual(dependencies('P0-02…05, P0-08/09'), ['P0-02', 'P0-03', 'P0-04', 'P0-05', 'P0-08', 'P0-09']);
  assert.throws(() => dependencies('P0-05…02'));
  assert.throws(() => dependencies('P0-01/missing'));
});
test('graph validation detects cycles and unresolved dependencies', () => {
  assert.deepEqual(graphErrors(new Map([['a', ['b']], ['b', []]])), []);
  assert.match(graphErrors(new Map([['a', ['b']], ['b', ['a']]])).join(), /cycle/);
  assert.match(graphErrors(new Map([['a', ['missing']]])).join(), /Unknown/);
});
