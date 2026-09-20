// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const { sourceIdentity } = require('./memory-source-identity.cjs');
const identity = sourceIdentity(path.resolve(__dirname, '../..'), [
  'src/evals/skills', 'src/skills/builtin', 'scripts/skills', 'scripts/package-skills.ps1',
  'src/tests/contracts/builtin-assets.test.cjs', 'src/tests/platform/builtin-archive.ps1',
]);
identity.schema = 'p7-builtin-skills-source-identity/1';
process.stdout.write(JSON.stringify(identity) + '\n');
