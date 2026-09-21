// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { quoteCart } = require('../src/cart.cjs');
test('existing subtotal behavior', () => {
  assert.deepEqual(quoteCart([{ unitCents: 250, quantity: 2 }]), {
    subtotalCents: 500, discountCents: 0, totalCents: 500,
  });
});
test('basis point discount rounds the discount half up', () => {
  assert.deepEqual(quoteCart([{ unitCents: 5, quantity: 1 }], { discountBps: 1000 }), {
    subtotalCents: 5, discountCents: 1, totalCents: 4,
  });
});
