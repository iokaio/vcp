// SPDX-License-Identifier: Apache-2.0
'use strict';
// Receipts bind transformations; Windows trust verification is performed by the
// signing boundary. A JSON receipt alone is never a publisher signature.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const check = (ok, message) => { if (!ok) throw Error(message); };
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const sha = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function policy(value) {
  check(value?.status === 'unsigned' || value?.status === 'signed', 'Unsupported signing disposition');
  if (value.status === 'signed') check(value.provider === 'azure-artifact-signing' &&
    value.account === 'ioka-llc-signing' && value.profile === 'WritingForgePro' &&
    value.endpoint === 'https://wus2.codesigning.azure.net/' &&
    value.publisher === 'CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US' &&
    value.identity_eku === '1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669', 'Unapproved signing policy');
  return value;
}
function validateReceipt(receipt, selected, expected = {}) {
  policy(selected);
  check(selected.status === 'signed' && receipt?.schema === 'vcp-authenticode-transform/1' && receipt.status === 'verified' &&
    ['native', 'setup'].includes(receipt.stage) && same(receipt.policy, selected), 'Invalid signing transformation receipt');
  check(sha(receipt.candidate_id) && /^[a-f0-9]{40}$/.test(receipt.reviewed_commit || '') && sha(receipt.build_receipt_sha256), 'Missing signing provenance');
  for (const [key, field] of [['stage','stage'], ['candidateId','candidate_id'], ['reviewedCommit','reviewed_commit'], ['buildReceiptSha256','build_receipt_sha256']]) {
    if (expected[key] !== undefined) check(receipt[field] === expected[key], 'Signing provenance mismatch: ' + field);
  }
  check(sha(receipt.tools?.signtool_sha256) && sha(receipt.tools?.dlib_sha256), 'Missing signing tool identities');
  const roles = receipt.stage === 'native' ? ['engine', 'launcher'] : ['setup', 'uninstaller'];
  check(Array.isArray(receipt.files) && receipt.files.length === roles.length &&
    same(receipt.files.map(row => row.role).sort(), roles.slice().sort()), 'Incomplete or duplicate signing roles');
  for (const row of receipt.files) {
    check(sha(row.input_sha256) && sha(row.output_sha256) && row.input_sha256 !== row.output_sha256 &&
      Number.isSafeInteger(row.input_bytes) && row.input_bytes > 0 && Number.isSafeInteger(row.output_bytes) &&
      row.output_bytes > row.input_bytes && row.content_preserved === true, 'Invalid signed byte transformation');
    const sig = row.signature;
    check(sig?.status === 'Valid' && sig.subject === selected.publisher && sig.identity_eku === selected.identity_eku &&
      sha(sig.certificate_sha256) && sha(sig.timestamp_certificate_sha256) && sig.timestamp_present === true &&
      row.verification?.exit_code === 0 && sha(row.verification.log_sha256), 'Publisher, timestamp or trust verification missing');
  }
  if (expected.inputs) for (const [role, digest] of Object.entries(expected.inputs)) {
    check(receipt.files.find(row => row.role === role)?.input_sha256 === digest, 'Signing input differs from compiler: ' + role);
  }
  return receipt;
}
function validateBinding(binding, release, buildHash, stage, inputs) {
  policy(release.signing);
  if (release.signing.status === 'unsigned') {
    check(!binding || binding.status === 'unsigned', 'Unexpected signing receipt on unsigned release');
    return null;
  }
  check(binding?.status === 'signed' && binding.receipt === 'signing-receipt.json' && sha(binding.receipt_sha256), 'Bound signing receipt required');
  return validateReceipt(binding.transformation, release.signing, { stage, candidateId: release.candidate_id,
    reviewedCommit: release.reviewed_commit, buildReceiptSha256: buildHash, inputs });
}
function payloadHashes(build, receipt) {
  return receipt ? { executable_sha256: receipt.files.find(row => row.role === 'engine').output_sha256,
    launcher_sha256: receipt.files.find(row => row.role === 'launcher').output_sha256 } :
    { executable_sha256: build.executable_sha256, launcher_sha256: build.launcher_sha256 };
}
function verifyPayloadSigning(root, build, binding) {
  const buildHash = hash(fs.readFileSync(path.join(root, 'build-receipt.json')));
  const receipt = validateBinding(binding, build.release, buildHash, 'native',
    { engine: build.executable_sha256, launcher: build.launcher_sha256 });
  if (receipt) {
    const bytes = fs.readFileSync(path.join(root, binding.receipt));
    check(hash(bytes) === binding.receipt_sha256 && same(JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')), receipt), 'Signing receipt bytes mismatch');
  }
  const hashes = payloadHashes(build, receipt);
  for (const [file, key] of [['vcp.exe','executable_sha256'], ['vcp-launch.exe','launcher_sha256']]) {
    const bytes = fs.readFileSync(path.join(root, file));
    check(hash(bytes) === hashes[key], 'Signed payload mismatch: ' + file);
    if (receipt) check(bytes.length === receipt.files.find(row => row.role === (file === 'vcp.exe' ? 'engine' : 'launcher')).output_bytes,
      'Signed payload size mismatch: ' + file);
  }
  return hashes;
}
function peFields(bytes) {
  check(bytes.length >= 64 && bytes.readUInt16LE(0) === 0x5a4d, 'Signing input is not a PE image');
  const pe = bytes.readUInt32LE(0x3c), optional = pe + 24;
  check(pe >= 64 && optional + 144 <= bytes.length && bytes.readUInt32LE(pe) === 0x4550, 'Malformed PE signing header');
  const magic = bytes.readUInt16LE(optional), plus = magic === 0x20b;
  check(plus || magic === 0x10b, 'Unsupported PE optional header');
  const size = bytes.readUInt16LE(pe + 20), directories = optional + (plus ? 112 : 96);
  check(size >= (plus ? 152 : 136) && optional + size <= bytes.length && bytes.readUInt32LE(optional + (plus ? 108 : 92)) >= 5, 'Missing PE certificate directory');
  return { checksum: optional + 64, security: directories + 32 };
}
function verifyPeTransformation(unsignedFile, signedFile) {
  const input = fs.readFileSync(unsignedFile), output = fs.readFileSync(signedFile), fields = peFields(input);
  check(same(fields, peFields(output)), 'PE headers moved during signing');
  check(input.readUInt32LE(fields.security) === 0 && input.readUInt32LE(fields.security + 4) === 0, 'Signing requires an unsigned original');
  const start = output.readUInt32LE(fields.security), length = output.readUInt32LE(fields.security + 4);
  check(start === Math.ceil(input.length / 8) * 8 && length >= 8 && start + length === output.length && start % 8 === 0,
    'Invalid appended PE certificate table');
  for (let i = 0; i < input.length; i++) {
    if ((i >= fields.checksum && i < fields.checksum + 4) || (i >= fields.security && i < fields.security + 8)) continue;
    check(input[i] === output[i], 'Executable content changed during signing');
  }
  for (let i = input.length; i < start; i++) check(output[i] === 0, 'Nonzero signing alignment padding');
  let cursor = start;
  while (cursor < output.length) {
    check(cursor + 8 <= output.length, 'Truncated PE certificate record');
    const bytes = output.readUInt32LE(cursor);
    check(bytes > 8 && cursor + bytes <= output.length && output.readUInt16LE(cursor + 4) === 0x200 &&
      output.readUInt16LE(cursor + 6) === 2, 'Invalid Authenticode certificate record');
    const next = cursor + Math.ceil(bytes / 8) * 8;
    check(next <= output.length, 'Invalid certificate padding');
    for (let i = cursor + bytes; i < next; i++) check(output[i] === 0, 'Nonzero certificate padding');
    cursor = next;
  }
  return { input_sha256: hash(input), input_bytes: input.length, output_sha256: hash(output), output_bytes: output.length, content_preserved: true };
}
module.exports = { policy, validateReceipt, validateBinding, payloadHashes, verifyPayloadSigning, verifyPeTransformation };
