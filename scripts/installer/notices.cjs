// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { enumerate } = require('../package-inventory.cjs');
const { fileHash, json } = require('../release/provenance.cjs');
function verify(root, channel, compiler) {
  const inventory = json(path.join(root, 'inventory.json'));
  if (inventory.schema !== 'vcp-setup-notices/1' || inventory.version !== channel.installer.version ||
      inventory.source_commit !== channel.installer.source_commit || !Array.isArray(inventory.files)) {
    throw Error('Setup runtime notice inventory differs from pinned compiler');
  }
  const actual = enumerate(root).filter(row => row.path !== 'inventory.json');
  const expected = inventory.files.map(({ path, bytes, sha256 }) => ({ path, bytes, sha256 }));
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw Error('Setup notice/source files changed');
  if (compiler && fileHash(path.join(compiler, 'license.txt')) !== fileHash(path.join(root, 'Inno-Setup.txt'))) {
    throw Error('Pinned compiler license differs from retained runtime notice');
  }
  return { schema: inventory.schema, inventory_sha256: fileHash(path.join(root, 'inventory.json')),
    source_commit: inventory.source_commit, files: actual };
}
if (require.main === module) {
  console.log(JSON.stringify(verify(process.argv[2], json(process.argv[3]), process.argv[4])));
}
module.exports = { verify };
