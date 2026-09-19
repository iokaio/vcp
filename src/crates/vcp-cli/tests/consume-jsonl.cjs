// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const readline = require('node:readline');
const records = [];
readline.createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', line => {
  const record = JSON.parse(line);
  assert.equal(record.schema_version, 1);
  assert.equal(typeof record.correlation, 'string');
  assert(['event', 'result'].includes(record.type));
  records.push(record);
}).on('close', () => {
  assert(records.length >= 3);
  const events = records.filter(record => record.type === 'event');
  const sequences = events.map(record => record.event.sequence);
  assert.equal(new Set(sequences).size, sequences.length);
  assert.equal(records.filter(record => record.type === 'result').length, 1);
  const result = records.at(-1);
  assert.equal(result.type, 'result');
  assert.equal(result.exit_code, 8);
  assert.equal(result.conditions.durably_paused, true);
  assert.equal(result.conditions.completed, false);
  assert.equal(typeof result.scope.task, 'string');
  assert.equal(typeof result.receipt.command, 'string');
  assert(events.some(record => record.event.event.correlation === result.receipt.command));
  process.stdout.write(JSON.stringify({ count: events.length, sequences }));
});
