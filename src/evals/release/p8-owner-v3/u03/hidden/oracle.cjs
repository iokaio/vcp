'use strict'; // SPDX-License-Identifier: Apache-2.0
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
function evaluate(project) {
  const results = [];
  const check = (id, body) => { try { body(); results.push({ id, pass: true }); } catch (error) { results.push({ id, pass: false, error: error.message }); } };
  let windowFor, pageItems;
  try { ({ windowFor } = require(path.join(project, 'src/domain/window.cjs'))); ({ pageItems } = require(path.join(project, 'src/api/page.cjs'))); }
  catch (error) { return { pass: false, checks: [{ id: 'load-source', pass: false, error: error.message }], passed: 0, total: 1 }; }
  const windows = [
    ['empty', [0], { start: 0, end: 0, nextOffset: null }],
    ['default-bounded', [30], { start: 0, end: 25, nextOffset: 25 }],
    ['first', [9, 0, 4], { start: 0, end: 4, nextOffset: 4 }],
    ['middle', [9, 4, 4], { start: 4, end: 8, nextOffset: 8 }],
    ['last', [9, 8, 4], { start: 8, end: 9, nextOffset: null }],
    ['at-end', [9, 9, 4], { start: 9, end: 9, nextOffset: null }],
    ['maximum-limit', [102, 0, 100], { start: 0, end: 100, nextOffset: 100 }],
    ['safe-integer-end', [9007199254740991, 9007199254740990, 100], { start: 9007199254740990, end: 9007199254740991, nextOffset: null }],
    ['safe-integer-middle', [9007199254740991, 9007199254740890, 100], { start: 9007199254740890, end: 9007199254740990, nextOffset: 9007199254740990 }]
  ];
  for (const [id, args, expected] of windows) check('window-' + id, () => assert.deepEqual(windowFor(...args), expected));
  const invalid = [
    ['negative-total', [-1]], ['fraction-total', [1.5]], ['unsafe-total', [9007199254740992]], ['null-total', [null]], ['string-total', ['3']],
    ['negative-offset', [3, -1]], ['past-end', [3, 4]], ['fraction-offset', [3, 0.5]], ['null-offset', [3, null]], ['string-offset', [3, '1']],
    ['zero-limit', [3, 0, 0]], ['over-limit', [3, 0, 101]], ['fraction-limit', [3, 0, 1.5]], ['null-limit', [3, 0, null]], ['string-limit', [3, 0, '2']]
  ];
  for (const [id, args] of invalid) check('invalid-' + id, () => assert.throws(() => windowFor(...args), { name: 'TypeError', message: 'invalid pagination' }));
  check('api-middle', () => assert.deepEqual(pageItems(['a', 'b', 'c', 'd'], { offset: 1, limit: 2 }), { items: ['b', 'c'], total: 4, offset: 1, nextOffset: 3 }));
  check('api-last', () => assert.deepEqual(pageItems([1, 2, 3], { offset: 2, limit: 2 }), { items: [3], total: 3, offset: 2, nextOffset: null }));
  check('api-empty', () => assert.deepEqual(pageItems([]), { items: [], total: 0, offset: 0, nextOffset: null }));
  check('api-default-bound', () => { const items = Array.from({ length: 30 }, (_, i) => i); const result = pageItems(items); assert.equal(result.items.length, 25); assert.equal(result.nextOffset, 25); });
  for (const [id, options] of [['null', null], ['array', []], ['string', 'x'], ['number', 1]]) check('api-options-' + id, () => assert.throws(() => pageItems([1], options), { name: 'TypeError', message: 'invalid pagination' }));
  check('api-null-offset', () => assert.throws(() => pageItems([1], { offset: null }), { name: 'TypeError', message: 'invalid pagination' }));
  check('api-null-limit', () => assert.throws(() => pageItems([1], { limit: null }), { name: 'TypeError', message: 'invalid pagination' }));
  check('api-non-array', () => assert.throws(() => pageItems({ length: 3 }), { name: 'TypeError', message: 'invalid pagination' }));
  check('api-preserves-input-and-shallow-copy', () => { const item = Object.freeze({ id: 7 }); const items = Object.freeze([item, 2, 3]); const options = Object.freeze({ offset: 0, limit: 1, extra: 'ignored' }); const result = pageItems(items, options); assert.deepEqual(result, { items: [item], total: 3, offset: 0, nextOffset: 1 }); assert.notEqual(result.items, items); assert.equal(result.items[0], item); assert.deepEqual(items, [item, 2, 3]); assert.deepEqual(options, { offset: 0, limit: 1, extra: 'ignored' }); });
  check('api-domain-dependency', () => assert.match(fs.readFileSync(path.join(project, 'src/api/page.cjs'), 'utf8'), /require\(['"]\.\.\/domain\/window\.cjs['"]\)/));
  return { pass: results.every(r => r.pass), passed: results.filter(r => r.pass).length, total: results.length, checks: results };
}
module.exports = { evaluate };
if (require.main === module) { const result = evaluate(path.resolve(process.argv[2])); console.log(JSON.stringify(result, null, 2)); if (!result.pass) process.exitCode = 1; }
