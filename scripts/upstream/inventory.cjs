// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { inventory } = require('../../src/tests/support/upstream-inventory.cjs');
function main(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!['--source', '--commit', '--output'].includes(argv[i]) || Object.hasOwn(options, argv[i]) || !argv[i + 1]) throw Error('Invalid inventory arguments');
    options[argv[i]] = argv[i + 1];
  }
  if (Object.keys(options).length !== 3) throw Error('Required: --source <checkout> --commit <full SHA> --output <new JSON file>');
  const result = inventory(path.resolve(options['--source']), options['--commit']);
  fs.writeFileSync(path.resolve(options['--output']), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ commit: result.commit, tree: result.tree, files: result.files.length, files_sha256: result.files_sha256 }));
}
try { main(process.argv.slice(2)); }
catch (error) { console.error('Inventory failed: ' + error.message); process.exitCode = 1; }
