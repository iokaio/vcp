// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const { munariumClosure } = require('../../src/tests/support/dependency-closure.cjs');
try {
  if (process.argv.length !== 4) throw Error('Required: <cargo-tree-output> <new-json-record>');
  const result = munariumClosure(fs.readFileSync(process.argv[2], 'utf8'));
  fs.writeFileSync(process.argv[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'pass', packages: result.packages.length, policy: result.policy }));
} catch (error) { console.error('Dependency closure failed: ' + error.message); process.exitCode = 1; }
