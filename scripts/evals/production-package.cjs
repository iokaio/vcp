// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const p = require('../release/provenance.cjs');
const inventory = require('../package-inventory.cjs');
const check = (value, message) => { if (!value) throw Error(message); };
function verify(result, manifest, build, buildHash) {
  check(result.schema === 'vcp-distribution-result/1' && result.status === 'release-candidate' &&
    /^[a-zA-Z0-9._-]+\.zip$/.test(result.package) && /^[a-f0-9]{64}$/.test(result.archive_sha256),
  'Strict versioned production distribution result required');
  check(JSON.stringify(result.manifest) === JSON.stringify(manifest), 'Archived manifest differs from distribution result');
  const release = manifest.release;
  check(manifest.schema === 'vcp-distribution-manifest/1' && manifest.source?.dirty === false &&
    release?.schema === 'vcp-release-identity/1' && release.channel === 'internal-beta' &&
    /^[a-f0-9]{40}$/.test(release.reviewed_commit) && /^[a-f0-9]{64}$/.test(release.source_content_sha256) &&
    release.signing?.status === 'unsigned' && manifest.source.git_commit === release.reviewed_commit &&
    manifest.build?.status === 'verified-release-build' && manifest.build.receipt === 'build-receipt.json' &&
    manifest.build.receipt_sha256 === buildHash, 'Strict production build and release identity required');
  check(manifest.notices?.schema === 'vcp-notice-bundle/1' &&
    manifest.notices.status === 'complete-with-recorded-provenance-limitations' &&
    /^[a-f0-9]{64}$/.test(manifest.notices.inventory_sha256), 'Bound production notices required');
  const binaries = manifest.files.filter(row => row.path === 'vcp.exe');
  const receipts = manifest.files.filter(row => row.path === 'build-receipt.json');
  check(binaries.length === 1 && receipts.length === 1 && receipts[0].sha256 === buildHash,
    'Production executable/build inventory missing or mismatched');
  check(Array.isArray(build.inputs) && build.inputs.length > 0, 'Missing recorded source inventory');
  const source = { commit: release.reviewed_commit, content_sha256: p.hash(JSON.stringify(build.inputs)), files: build.inputs };
  check(source.content_sha256 === release.source_content_sha256, 'Archived source inventory digest mismatch');
  const selected = { ...release };
  p.validateReceipt(build, selected, source, binaries[0].sha256);
  return { release, executable_sha256: binaries[0].sha256, build_receipt_sha256: buildHash };
}
function verifyDirectory(resultPath, directory) {
  const result = p.json(resultPath), manifest = p.json(path.join(directory, 'manifest.json'));
  const buildPath = path.join(directory, 'build-receipt.json');
  inventory.verifyManifest(directory, manifest);
  const bound = verify(result, manifest, p.json(buildPath), p.fileHash(buildPath));
  p.verifyPayloadSources(directory, p.json(buildPath), manifest.notices?.inventory_sha256);
  p.peArchitecture(path.join(directory, 'vcp.exe'));
  return bound;
}
if (require.main === module) {
  try { process.stdout.write(JSON.stringify(verifyDirectory(process.argv[2], process.argv[3])) + '\n'); }
  catch (error) { console.error('Production artifact rejected: ' + error.message); process.exitCode = 1; }
}
module.exports = { verify, verifyDirectory };
