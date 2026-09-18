// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path');
const { memoryClosure, verifyReference } = require('../../src/tests/support/dependency-closure.cjs');
const { digest } = require('../../src/tests/support/harness.cjs');
try {
  if (process.argv.length !== 3) throw Error('Required: <native-dependency-log>');
  const repository = path.resolve(__dirname, '../..');
  const actual = memoryClosure(fs.readFileSync(process.argv[2], 'utf8'));
  const reference = JSON.parse(fs.readFileSync(path.join(repository, 'src/third_party/components/local-memory-dependencies.json')));
  const lock = digest(fs.readFileSync(path.join(repository, 'src/third_party/codex/codex-rs/Cargo.lock')));
  verifyReference(actual, reference, lock);
  console.log(JSON.stringify({ status: 'pass', packages: actual.packages.length, policy: actual.policy, lock_sha256: lock }));
} catch (error) { console.error('Local-memory dependencies: ' + error.message); process.exitCode = 1; }
