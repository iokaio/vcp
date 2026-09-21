// SPDX-License-Identifier: Apache-2.0
'use strict';
// Only the frozen SQLite fixture, in a new process-local database. No connection
// string, database filename, extension loading, or external SQL is accepted.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const { DatabaseSync } = require('node:sqlite');
if (process.argv.length !== 3 || !['initial', 'pending'].includes(process.argv[2])) {
  throw Error('Usage: builtin-toolchain-sql.cjs initial|pending');
}
const db = new DatabaseSync(':memory:');
try {
  db.exec(fs.readFileSync('migrations/001_initial.sql', 'utf8'));
  const rows = () => db.prepare('SELECT id, name FROM customer ORDER BY id').all().map(row => ({ ...row }));
  assert.deepEqual(rows(), [{ id: 1, name: 'A' }]);
  console.log('Applied 001 to disposable memory database; existing row verified.');
  if (process.argv[2] === 'pending') {
    db.exec('BEGIN');
    try {
      db.exec(fs.readFileSync('migrations/002_email.sql', 'utf8'));
      assert.deepEqual(rows(), [{ id: 1, name: 'A' }]);
      db.exec('COMMIT');
      console.log('Pending migration accepted; existing row preserved.');
    } catch (error) {
      db.exec('ROLLBACK');
      assert.deepEqual(rows(), [{ id: 1, name: 'A' }]);
      console.error('Pending migration rejected; rollback preserved existing row.');
      throw error;
    }
  }
} finally { db.close(); }
