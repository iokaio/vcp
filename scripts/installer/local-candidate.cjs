// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const p = require('../release/provenance.cjs');
const inventory = require('../package-inventory.cjs');
const check = (ok, message) => { if (!ok) throw Error(message); };
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const extras = [
  'scripts/build-production.ps1', 'scripts/build-local-setup.ps1', 'scripts/build-local-deploy.ps1', 'scripts/build-setup.ps1',
  'scripts/release', 'scripts/installer', 'scripts/upstream', 'scripts/package.ps1', 'scripts/package-install.ps1',
  'scripts/package-inventory.cjs', 'scripts/package-models.ps1', 'scripts/skills',
  'src/third_party/upstreams.toml', 'src/third_party/components', 'src/skills/builtin',
  'release/internal-beta.json', 'release/installer-notices', 'docs/usage',
  'src/packages/sdk-ts/package.json', 'src/packages/sdk-ts/package-lock.json',
  'src/packages/vscode/package.json', 'src/packages/vscode/package-lock.json',
  'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md',
];
function captureSource(root) {
  return require('../evals/memory-source-identity.cjs').sourceIdentity(root, extras);
}
function validateReceipt(receipt, selected, source, executableHash, launcherHash) {
  check(receipt.schema === 'vcp-local-build/1' && receipt.exit_code === 0 && receipt.cargo_exit_code === 0,
    'Successful local production build required');
  check(!receipt.release && receipt.source_stable === true && receipt.toolchain_stable === true &&
    receipt.qualification_build === false && receipt.profile === 'release' && receipt.target === selected.target,
    'Stable, nonqualification local production build required');
  check(receipt.source_commit === source.commit && receipt.source_dirty === source.dirty &&
    receipt.source_content_sha256 === source.content_sha256 && equal(receipt.inputs, source.files),
    'Local build inputs differ from the current checkout');
  check(receipt.executable_sha256 === executableHash && receipt.launcher_sha256 === launcherHash,
    'Local executable or launcher hash differs');
  check(Array.isArray(receipt.vcp_features) && receipt.vcp_features.length > 0 &&
    receipt.vcp_features.every(row => Array.isArray(row.features) && !row.features.includes('qualification')),
    'Local VCP feature inventory missing or qualification enabled');
  for (const [artifact, name] of [[receipt.compiler_artifact, 'vcp'], [receipt.launcher_compiler_artifact, 'vcp-launch']]) {
    check(artifact?.target?.name === name && artifact.profile?.test === false && artifact.profile.opt_level === '3' &&
      Array.isArray(artifact.features) && artifact.features.length === 0 && typeof artifact.package_id === 'string' &&
      (artifact.package_id.endsWith('#' + selected.native_version) || artifact.package_id.endsWith('#vcp-cli@' + selected.native_version)),
      'Local compiler artifact/version rejected: ' + name);
  }
  check(receipt.compiler_artifact.package_id === receipt.launcher_compiler_artifact.package_id, 'Launcher package differs');
  const args = receipt.command;
  check(Array.isArray(args) && typeof args[18] === 'string' && /^[1-9]\d{0,2}$/.test(args[18]) &&
    String(Number(args[18])) === args[18] && Number(args[18]) <= 256 &&
    typeof args[16] === 'string' && /[\\/]codex-target$/.test(args[16]) &&
    equal(args, ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features',
      '-p', 'vcp-cli', '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target,
      '--target-dir', args[16], '-j', args[18], '--message-format=json-render-diagnostics']), 'Unexpected local production build command');
  check(equal(receipt.rustflags, ['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static']), 'Unexpected local Rust flags');
  check(Array.isArray(receipt.rustc) && receipt.rustc.some(line => /^release: 1\.95\.0$/.test(line)), 'Unexpected local Rust toolchain');
}
function verifyBuild(root, receiptFile, launcherFile) {
  const selected = p.channel(root), receipt = p.json(receiptFile), source = captureSource(root);
  p.verifyVersions(root, selected);
  // The bounded source identity delegates retained upstream bytes to its pinned inventory.
  // Recheck those bytes now; successful historical logs alone do not prove the current tree.
  execFileSync(process.execPath, [path.join(root, 'scripts/upstream/reconstruct.cjs'), 'verify', '--component', 'codex'],
    { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  validateReceipt(receipt, selected, source, p.fileHash(receipt.executable), p.fileHash(launcherFile));
  const directory = path.dirname(receiptFile);
  for (const [name, key] of [['build.log', 'log_sha256'], ['upstream-verification.log', 'upstream_before_sha256'],
    ['upstream-verification-after.log', 'upstream_after_sha256']]) {
    check(p.fileHash(path.join(directory, name)) === receipt[key], 'Changed local build evidence: ' + name);
  }
  const upstream = p.json(path.join(root, 'src/third_party/components/codex-files.json'));
  for (const name of ['upstream-verification.log', 'upstream-verification-after.log']) {
    const evidence = p.json(path.join(directory, name));
    check(evidence.status === 'pass' && evidence.component === 'codex' && evidence.files === upstream.files.length &&
      evidence.files_sha256 === upstream.files_sha256, 'Local upstream evidence mismatch');
  }
  for (const name of ['source-before.json', 'source-after.json']) {
    const observed = p.json(path.join(directory, name));
    check(observed.commit === source.commit && observed.content_sha256 === source.content_sha256 && equal(observed.files, source.files),
      'Local source changed during or after compilation');
  }
  const rows = fs.readFileSync(path.join(directory, 'build.log'), 'utf8').split(/\r?\n/)
    .filter(row => row.startsWith('{')).map(row => JSON.parse(row));
  for (const artifact of [receipt.compiler_artifact, receipt.launcher_compiler_artifact]) {
    check(rows.some(row => equal(row, artifact)), 'Local compiler artifact absent from build log');
  }
  for (const name of ['cl', 'link', 'lib', 'cmake', 'ninja', 'rustc', 'cargo', 'node', 'powershell']) {
    const tools = receipt.native_tools?.filter(row => row.name === name);
    check(tools?.length === 1 && p.fileHash(tools[0].path) === tools[0].sha256, 'Local build tool changed: ' + name);
  }
  check(receipt.cargo_configs?.length === 1 && receipt.cargo_configs[0].sha256 ===
    source.files.find(row => row.path === 'src/third_party/codex/codex-rs/.cargo/config.toml')?.sha256,
    'Local Cargo configuration mismatch');
  p.verifyExecutable(receipt.executable, selected.native_version);
  p.verifyExecutable(launcherFile, selected.native_version, 'vcp-launch');
  return { schema: 'vcp-local-candidate/1', status: 'unsigned-local-candidate',
    native_version: selected.native_version, sdk_version: selected.sdk_version, vsix_version: selected.vsix_version,
    target: selected.target, source_commit: source.commit, source_dirty: source.dirty,
    source_content_sha256: source.content_sha256, build_receipt_sha256: p.fileHash(receiptFile),
    executable_sha256: receipt.executable_sha256, launcher_sha256: receipt.launcher_sha256,
    qualification: 'not-release-qualified', signing: 'unsigned' };
}
function stage(root, packageRoot, resultFile, receiptFile, launcherFile) {
  const result = p.json(resultFile), receipt = p.json(receiptFile), manifest = p.json(path.join(packageRoot, 'manifest.json'));
  check(result.schema === 'vcp-distribution-result/1' && result.status === 'candidate' &&
    manifest.artifact === 'unsigned-local-candidate' && manifest.signing?.status === 'unsigned' &&
    !manifest.release && !manifest.local_candidate && manifest.build?.status === 'recorded-local-build' &&
    equal(manifest, result.manifest), 'Unmodified local distribution result required');
  check(manifest.build.receipt_sha256 === p.fileHash(receiptFile) &&
    p.fileHash(path.join(packageRoot, 'build-receipt.json')) === p.fileHash(receiptFile), 'Local package receipt mismatch');
  inventory.verifyManifest(packageRoot, manifest);
  p.verifyPayloadSources(packageRoot, receipt);
  const identity = verifyBuild(root, receiptFile, launcherFile);
  check(manifest.source.git_commit === identity.source_commit && manifest.source.dirty === identity.source_dirty,
    'Local package source differs');
  identity.input_archive_sha256 = result.archive_sha256;
  identity.candidate_id = p.hash(JSON.stringify(identity));
  manifest.local_candidate = identity;
  manifest.compatibility.cli = 'vcp-cli/' + identity.native_version;
  manifest.limitations.push('Explicit local installer from unreviewed checkout; no CI, signing, release qualification or publication evidence.');
  fs.writeFileSync(path.join(packageRoot, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  inventory.verifyManifest(packageRoot, manifest);
  return identity;
}
if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'source') process.stdout.write(JSON.stringify(captureSource(...args)) + '\n');
  else if (command === 'verify') process.stdout.write(JSON.stringify(verifyBuild(...args)) + '\n');
  else if (command === 'stage') process.stdout.write(JSON.stringify(stage(...args)) + '\n');
  else throw Error('Expected source, verify, or stage');
}
module.exports = { captureSource, validateReceipt, verifyBuild, stage };
