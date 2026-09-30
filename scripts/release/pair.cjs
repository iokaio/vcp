// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path');
const { json, fileHash, pairIdentity } = require('./provenance.cjs');
function recordPair(nativeFile, vsixFile, setupFile, output) {
  const native = json(nativeFile), vsix = json(vsixFile), setup = setupFile ? json(setupFile) : null;
  const pair = pairIdentity(native, vsix, setup);
  for (const [record, file, name, expected] of [
    [native, nativeFile, native.package, native.archive_sha256],
    [vsix, vsixFile, vsix.archive.file, vsix.archive.sha256],
    ...(setup ? [[setup, setupFile, setup.archive.file, setup.archive.sha256]] : []),
  ]) {
    if (typeof name !== 'string' || !name || path.basename(name) !== name || /[\\/:]/.test(name) ||
        fileHash(path.join(path.dirname(path.resolve(file)), name)) !== expected) throw Error('Final artifact hash mismatch');
  }
  if (vsix.engine.native_manifest_sha256 !== fileHash(nativeFile)) throw Error('VSIX does not bind exact native result');
  pair.receipts = { native_sha256: fileHash(nativeFile), vsix_sha256: fileHash(vsixFile),
    setup_sha256: setupFile ? fileHash(setupFile) : null };
  fs.writeFileSync(output, JSON.stringify(pair, null, 2) + '\n', { flag: 'wx' });
  return pair;
}
if (require.main === module) {
  try {
    const [native, vsix, output, setup, ...extra] = process.argv.slice(2);
    if (!native || !vsix || !output || extra.length) throw Error('Use pair.cjs <native-result.json> <vsix-manifest.json> <new-pair.json> [setup-result.json]');
    console.log(JSON.stringify(recordPair(native, vsix, setup, output)));
  } catch (error) { console.error('Release pairing failed: ' + error.message); process.exitCode = 1; }
}
module.exports = { recordPair };
