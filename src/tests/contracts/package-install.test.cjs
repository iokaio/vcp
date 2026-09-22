// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const inventory = require('../../../scripts/package-inventory.cjs');

test('state compatibility is explicit and rejects changed canonical/config/index formats', () => {
  const candidate = { compatibility: { canonical: 'c1', config: 'g1', index: 'i1' } };
  assert.equal(inventory.compatibility(candidate, { compatibility: { canonical: 'c1', config: 'g1', index: 'i1' } }).status, 'compatible');
  for (const field of ['canonical', 'config', 'index']) {
    const state = { compatibility: { canonical: 'c1', config: 'g1', index: 'i1' } };
    state.compatibility[field] = 'newer';
    assert.throws(() => inventory.compatibility(candidate, state), new RegExp('Incompatible ' + field));
  }
});
