// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { munariumClosure, verifyReference } = require('../../src/tests/support/dependency-closure.cjs');
try {
  if (![4, 5].includes(process.argv.length)) throw Error('Required: <cargo-tree-output> <new-json-record> [selected-reference]');
  const result = munariumClosure(fs.readFileSync(process.argv[2], 'utf8'));
  if (process.argv[4]) {
    const lock = fs.readFileSync(path.resolve(__dirname, '../../src/third_party/codex/codex-rs/Cargo.lock'));
    verifyReference(result, JSON.parse(fs.readFileSync(process.argv[4], 'utf8')), createHash('sha256').update(lock).digest('hex'));
  }
  fs.writeFileSync(process.argv[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'pass', packages: result.packages.length, policy: result.policy }));
} catch (error) { console.error('Dependency closure failed: ' + error.message); process.exitCode = 1; }
