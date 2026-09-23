const test = require('node:test'); // SPDX-License-Identifier: Apache-2.0
const assert = require('node:assert/strict');
const { pageItems } = require('../src/api/page.cjs');
test('empty array', () => assert.deepEqual(pageItems([]), { items: [], total: 0, offset: 0, nextOffset: null }));
test('small default page is a copy', () => { const input = [1, 2]; const result = pageItems(input); assert.deepEqual(result, { items: [1, 2], total: 2, offset: 0, nextOffset: null }); assert.notEqual(result.items, input); });
test('invalid array', () => assert.throws(() => pageItems(null), { name: 'TypeError', message: 'invalid pagination' }));
