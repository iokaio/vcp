// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const readline = require('node:readline');
const seen = new Set();
let final;
readline.createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', line => {
  assert.equal(final, undefined, 'no records after final result');
  const record = JSON.parse(line);
  assert.equal(record.schema_version, 1);
  assert.equal(typeof record.correlation, 'string');
  assert(['accepted', 'event', 'required_input', 'cursor_gap', 'result'].includes(record.type));
  if (record.type === 'event') {
    const key = `${record.event.event.session}:${record.event.sequence}`;
    assert(!seen.has(key), 'no duplicate durable events'); seen.add(key);
  }
  if (record.type === 'result') final = record;
}).on('close', () => {
  assert(final, 'exactly one final result');
  assert.equal(final.exit_code, Number(process.argv[2]));
  if (final.conditions && final.scope) assert.equal(typeof final.receipt.command, 'string');
});
