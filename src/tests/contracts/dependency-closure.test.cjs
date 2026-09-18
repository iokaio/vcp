// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { munariumClosure, verifyReference } = require('../support/dependency-closure.cjs');
const fixture = ['munarium-core v1.2.1 (/synthetic/source/core)', 'munarium-store-mem v1.2.1', 'munarium-datastore v1.2.1',
  'tantivy v0.22.1', 'diskann v0.56.0', 'diskann-vector v0.56.0', 'diskann-vector v0.56.0 (*)', 'async-trait v0.1.92 (proc-macro)'].join('\n');
test('records selected native libraries without machine paths or duplicate tree rows', () => {
  const result = munariumClosure(fixture);
  assert.equal(result.packages.length, 7);
  assert.equal(JSON.stringify(result).includes('/synthetic'), false);
  assert.deepEqual(result.packages.find(item => item.name === 'diskann'), { name: 'diskann', version: '0.56.0' });
  assert.deepEqual(munariumClosure(fixture.replaceAll('\n', '\r\n')), result);
});
test('rejects server/provider/database dependencies and disabled search engines', () => {
  for (const name of ['sqlx', 'sqlx-core', 'axum', 'tonic', 'reqwest', 'hyper-util', 'munarium-providers', 'munarium-store-pg', 'munarium-retrieval']) {
    assert.throws(() => munariumClosure(fixture + '\n' + name + ' v1.0.0'), /Forbidden dependency/);
  }
  assert.throws(() => munariumClosure(fixture.replace('diskann v0.56.0', '')), /Missing required dependency: diskann/);
  assert.throws(() => munariumClosure(fixture + '\nunknown-format'), /Unrecognized dependency record/);
  assert.throws(() => munariumClosure(''), /Missing required/);
});
test('selected closure rejects version, membership and lockfile drift', () => {
  const actual = munariumClosure(fixture), lock = 'a'.repeat(64);
  const reference = { ...actual, target: 'x86_64-pc-windows-msvc',
    features: ['munarium-datastore/vector-diskann'], workspace_lock_sha256: lock };
  assert.doesNotThrow(() => verifyReference(actual, reference, lock));
  assert.throws(() => verifyReference(actual, reference, 'b'.repeat(64)), /identity or lockfile drift/);
  assert.throws(() => verifyReference(munariumClosure(fixture.replace('tantivy v0.22.1', 'tantivy v0.23.0')), reference, lock), /graph drift/);
  assert.throws(() => verifyReference(munariumClosure(fixture + '\nextra v1.0.0'), reference, lock), /graph drift/);
  assert.throws(() => verifyReference(actual, { ...reference, packages: reference.packages.slice(1) }, lock), /graph drift/);
});
