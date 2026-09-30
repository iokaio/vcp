// SPDX-License-Identifier: Apache-2.0
'use strict';
// Release receipts are reproducible local evidence, not signatures or a claim
// that a human approved a commit. The delivery workflow supplies ReviewedCommit
// only after its review/check gates, and retains that selection with its logs.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync, spawnSync } = require('node:child_process');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const sha = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const check = (condition, message) => { if (!condition) throw Error(message); };
const json = file => JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
function fileHash(file) {
  const stat = fs.lstatSync(file);
  check(stat.isFile() && !stat.isSymbolicLink(), 'Release input must be an ordinary file: ' + file);
  return hash(fs.readFileSync(file));
}
function git(root, args) {
  return execFileSync('git', ['-c', 'safe.directory=' + root.replaceAll('\\', '/'), ...args], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 128 * 1024 * 1024,
  }).toString();
}
function captureSource(root, reviewedCommit) {
  check(/^[a-f0-9]{40}$/.test(reviewedCommit || ''), 'Release requires an explicit reviewed commit');
  const commit = git(root, ['rev-parse', 'HEAD']).trim();
  check(commit === reviewedCommit, 'HEAD differs from the reviewed commit');
  check(git(root, ['ls-files', '-v', '-z']).split('\0').filter(Boolean).every(row => row.startsWith('H ')),
    'Release refuses assume-unchanged, skip-worktree or unmerged source entries');
  check(!git(root, ['status', '--porcelain=v1', '-z', '--untracked-files=all']), 'Release requires a clean checkout');
  const files = git(root, ['ls-files', '-z']).split('\0').filter(Boolean).sort().map(relative => {
    let parent = path.dirname(path.join(root, relative));
    while (parent !== root) {
      check(!fs.lstatSync(parent).isSymbolicLink(), 'Linked source directory: ' + relative);
      const next = path.dirname(parent); check(parent !== next, 'Source escapes repository'); parent = next;
    }
    const file = path.join(root, relative);
    return { path: relative, bytes: fs.statSync(file).size, sha256: fileHash(file) };
  });
  check(files.length > 0, 'Empty release source inventory');
  check(commit === git(root, ['rev-parse', 'HEAD']).trim() &&
    !git(root, ['status', '--porcelain=v1', '-z', '--untracked-files=all']), 'Source changed during inventory');
  return { schema: 'vcp-release-source/1', commit, dirty: false, files, content_sha256: hash(JSON.stringify(files)) };
}
function channel(root) {
  const filename = path.join(root, 'release/internal-beta.json'), value = json(filename);
  check(value.schema === 'vcp-beta-channel/1' && value.channel === 'internal-beta' &&
    value.target === 'x86_64-pc-windows-msvc' && value.signing?.status === 'unsigned', 'Unsupported release channel, target or signing transformation');
  check(/^\d+\.\d+\.\d+-beta\.\d+$/.test(value.native_version) &&
    /^\d+\.\d+\.\d+$/.test(value.sdk_version) && /^\d+\.\d+\.\d+$/.test(value.vsix_version), 'Invalid release versions');
  return { ...value, config_sha256: fileHash(filename) };
}
function verifyVersions(root, selected) {
  const cargo = fs.readFileSync(path.join(root, 'src/crates/vcp-cli/Cargo.toml'), 'utf8');
  check(cargo.split('[[')[0].match(/^version\s*=\s*"([^"]+)"/m)?.[1] === selected.native_version, 'Native source version mismatch');
  const lock = fs.readFileSync(path.join(root, 'src/third_party/codex/codex-rs/Cargo.lock'), 'utf8');
  check(lock.split('[[package]]').find(row => /^name = "vcp-cli"$/m.test(row))?.match(/^version = "([^"]+)"$/m)?.[1] === selected.native_version, 'Native locked version mismatch');
  for (const [directory, expected] of [['sdk-ts', selected.sdk_version], ['vscode', selected.vsix_version]]) {
    const directoryPath = path.join(root, 'src/packages', directory);
    const manifest = json(path.join(directoryPath, 'package.json')), lock = json(path.join(directoryPath, 'package-lock.json'));
    check(manifest.version === expected && lock.version === expected && lock.packages?.['']?.version === expected,
      'Package/lock version mismatch: ' + directory);
  }
}
function peArchitecture(file) {
  const bytes = fs.readFileSync(file);
  check(bytes.length >= 64 && bytes.readUInt16LE(0) === 0x5a4d, 'Executable is not a Windows PE');
  const offset = bytes.readUInt32LE(0x3c);
  check(offset <= bytes.length - 26 && bytes.readUInt32LE(offset) === 0x4550 &&
    bytes.readUInt16LE(offset + 4) === 0x8664 && bytes.readUInt16LE(offset + 24) === 0x20b,
  'Executable must be PE32+ x86_64');
  return 'x86_64-pc-windows-msvc';
}
function verifyExecutable(file, version, name = 'vcp') {
  check(['vcp', 'vcp-launch'].includes(name), 'Unknown release executable');
  peArchitecture(file);
  // The CLI prints Clap's DisplayVersion through its existing diagnostic stream.
  const run = spawnSync(file, [name === 'vcp-launch' ? '--launcher-version' : '--version'], { windowsHide: true, timeout: 30000,
    maxBuffer: 64 * 1024, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  check(!run.error && run.status === 0 && (run.stdout + run.stderr).trim() === name + ' ' + version,
    'Executable product version mismatch');
  return { sha256: fileHash(file), version, target: 'x86_64-pc-windows-msvc' };
}
function releaseIdentity(selected, source, executableHash) {
  return {
    schema: 'vcp-release-identity/1', channel: selected.channel, native_version: selected.native_version,
    sdk_version: selected.sdk_version, vsix_version: selected.vsix_version, target: selected.target,
    signing: selected.signing, reviewed_commit: source.commit, source_content_sha256: source.content_sha256,
    config_sha256: selected.config_sha256,
    candidate_id: hash(JSON.stringify([source.commit, source.content_sha256, selected.config_sha256, executableHash])),
  };
}
function validateReceipt(receipt, selected, source, executableHash) {
  check(receipt?.schema === 'vcp-local-build/1' && receipt.exit_code === 0 && receipt.cargo_exit_code === 0,
    'Successful production build receipt required');
  check(receipt.source_dirty === false && receipt.source_stable === true && receipt.toolchain_stable === true && receipt.qualification_build === false &&
    receipt.profile === 'release' && receipt.target === selected.target, 'Unstable, dirty or nonproduction build receipt');
  check(receipt.source_commit === source.commit && receipt.source_content_sha256 === source.content_sha256 &&
    JSON.stringify(receipt.inputs) === JSON.stringify(source.files), 'Build source differs from selected checkout');
  check(receipt.executable_sha256 === executableHash &&
    JSON.stringify(receipt.release) === JSON.stringify(releaseIdentity(selected, source, executableHash)), 'Release identity mismatch');
  check(receipt.executable_version === selected.native_version && receipt.executable_target === selected.target,
    'Build executable version/target mismatch');
  check(receipt.launcher_version === selected.native_version && receipt.launcher_target === selected.target &&
    sha(receipt.launcher_sha256), 'Build launcher version/target/hash mismatch');
  check(Array.isArray(receipt.vcp_features) && receipt.vcp_features.length > 0 &&
    receipt.vcp_features.every(row => Array.isArray(row.features) && !row.features.includes('qualification')),
  'Missing or qualification-enabled feature inventory');
  const artifact = receipt.compiler_artifact;
  check(artifact?.target?.name === 'vcp' && Array.isArray(artifact.features) && artifact.features.length === 0 &&
    artifact.profile?.test === false && artifact.profile.opt_level === '3' &&
    typeof artifact.package_id === 'string' &&
    (artifact.package_id.endsWith('#' + selected.native_version) || artifact.package_id.endsWith('#vcp-cli@' + selected.native_version)), 'Invalid native compiler artifact');
  const launcher = receipt.launcher_compiler_artifact;
  check(launcher?.target?.name === 'vcp-launch' && launcher.package_id === artifact.package_id &&
    Array.isArray(launcher.features) && launcher.features.length === 0 &&
    launcher.profile?.test === false && launcher.profile.opt_level === '3', 'Invalid launcher compiler artifact');
  const args = receipt.command;
  check(Array.isArray(args) && /^[1-9]\d?$/.test(args[18]) && Number(args[18]) <= 16 &&
    typeof args[16] === 'string' && /[\\/]cargo-target$/.test(args[16]) &&
    JSON.stringify(args) === JSON.stringify(['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release',
      '--no-default-features', '-p', 'vcp-cli', '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target,
      '--target-dir', args[16], '-j', args[18], '--message-format=json-render-diagnostics']), 'Unqualified build command');
  check(JSON.stringify(receipt.rustflags) === JSON.stringify(['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static']), 'Unqualified Rust flags');
  check(Array.isArray(receipt.rustc) && receipt.rustc.some(line => /^release: 1\.95\.0$/.test(line)), 'Unqualified Rust toolchain');
  for (const name of ['cl', 'link', 'lib', 'cmake', 'ninja', 'rustc', 'cargo', 'node']) {
    const matches = receipt.native_tools?.filter(tool => tool.name === name);
    check(matches?.length === 1 && sha(matches[0].sha256), 'Missing toolchain hash: ' + name);
  }
  for (const key of ['upstream_before_sha256', 'upstream_after_sha256', 'log_sha256']) check(sha(receipt[key]), 'Missing build evidence hash: ' + key);
  check(Array.isArray(receipt.cargo_configs) && receipt.cargo_configs.length === 1 && sha(receipt.cargo_configs[0].sha256), 'Missing committed Cargo configuration');
  check(receipt.cargo_configs[0].sha256 === source.files.find(row => row.path === 'src/third_party/codex/codex-rs/.cargo/config.toml')?.sha256,
    'Cargo configuration differs from reviewed source');
  check(receipt.dependency_sources_stable === true && receipt.dependency_source?.schema === 'vcp-release-dependencies/1' &&
    receipt.dependency_source.status === 'verified' && receipt.dependency_source.target === selected.target &&
    receipt.dependency_source.workspace_lock_sha256 === source.files.find(row => row.path === 'src/third_party/codex/codex-rs/Cargo.lock')?.sha256 &&
    receipt.dependency_source.components > 0 && sha(receipt.dependency_source.inventory_sha256) &&
    sha(receipt.dependencies_before_sha256) && receipt.dependencies_before_sha256 === receipt.dependencies_after_sha256,
  'Unverified or changed dependency source inventory');
  return receipt.release;
}
function verifyBuild(root, receiptFile, executable, reviewedCommit) {
  const selected = channel(root); verifyVersions(root, selected);
  const source = captureSource(root, reviewedCommit), receipt = json(receiptFile);
  const release = validateReceipt(receipt, selected, source, fileHash(executable));
  const directory = path.dirname(receiptFile);
  const launcherFile = path.join(directory, 'vcp-launch.exe');
  check(fileHash(launcherFile) === receipt.launcher_sha256, 'Launcher differs from reviewed build');
  for (const [file, key] of [['build.log', 'log_sha256'], ['upstream-verification.log', 'upstream_before_sha256'],
    ['upstream-verification-after.log', 'upstream_after_sha256']]) {
    check(fileHash(path.join(directory, file)) === receipt[key], 'Stale or missing build evidence: ' + file);
  }
  const upstream = json(path.join(root, 'src/third_party/components/codex-files.json'));
  for (const file of ['upstream-verification.log', 'upstream-verification-after.log']) {
    const evidence = json(path.join(directory, file));
    check(evidence.status === 'pass' && evidence.component === 'codex' && evidence.files === upstream.files.length &&
      evidence.files_sha256 === upstream.files_sha256, 'Upstream verification evidence mismatch');
  }
  for (const file of ['source-before.json', 'source-after.json']) {
    check(JSON.stringify(json(path.join(directory, file))) === JSON.stringify(source), 'Stale build source inventory');
  }
  for (const [file, key] of [['dependencies-before.json', 'dependencies_before_sha256'], ['dependencies-after.json', 'dependencies_after_sha256']]) {
    check(fileHash(path.join(directory, file)) === receipt[key] &&
      JSON.stringify(json(path.join(directory, file))) === JSON.stringify(receipt.dependency_source), 'Dependency verification evidence mismatch');
  }
  const artifacts = fs.readFileSync(path.join(directory, 'build.log'), 'utf8').split(/\r?\n/)
    .filter(line => line.startsWith('{')).map(line => JSON.parse(line)).filter(row => row.reason === 'compiler-artifact');
  check(artifacts.some(row => JSON.stringify(row) === JSON.stringify(receipt.compiler_artifact)), 'Compiler artifact absent from build log');
  check(artifacts.some(row => JSON.stringify(row) === JSON.stringify(receipt.launcher_compiler_artifact)), 'Launcher artifact absent from build log');
  // Check selected source, byte hashes and retained compiler evidence before
  // executing either version probe. Receipts remain local evidence, not signatures.
  verifyExecutable(executable, selected.native_version);
  verifyExecutable(launcherFile, selected.native_version, 'vcp-launch');
  return release;
}
function verifyPayloadSources(packageRoot, receipt, expectedNoticeHash) {
  const { enumerate, verifyManifest } = require('../package-inventory.cjs');
  const manifestFile = path.join(packageRoot, 'manifest.json');
  if (fs.existsSync(manifestFile)) verifyManifest(packageRoot, json(manifestFile));
  const inputs = new Map(receipt.inputs.map(row => [row.path, row.sha256]));
  if (expectedNoticeHash) require('./notices.cjs').verifyStaged(packageRoot, expectedNoticeHash,
    inputs.get('src/third_party/codex/codex-rs/Cargo.lock'), receipt.executable_sha256);
  const direct = new Map([
    ['LICENSE', 'LICENSE'], ['NOTICE', 'NOTICE'], ['THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_NOTICES.md'],
    ['tools/package-install.ps1', 'scripts/package-install.ps1'],
    ['tools/package-inventory.cjs', 'scripts/package-inventory.cjs'],
    ['tools/package-models.ps1', 'scripts/package-models.ps1'],
    ['models/minilm-assets.json', 'src/third_party/components/minilm-assets.json'],
  ]);
  for (const row of enumerate(packageRoot)) {
    if (row.path === 'manifest.json') continue; // independently verified above
    if (row.path === 'vcp.exe') { check(row.sha256 === receipt.executable_sha256, 'Staged executable mismatch'); continue; }
    if (row.path === 'vcp-launch.exe') { check(row.sha256 === receipt.launcher_sha256, 'Staged launcher mismatch'); continue; }
    if (row.path === 'build-receipt.json') continue;
    if (expectedNoticeHash && (row.path === 'component-inventory.json' || row.path === 'PREREQUISITES.md' || row.path.startsWith('licenses/'))) continue;
    const source = row.path.startsWith('skills/builtin/') ? 'src/' + row.path : direct.get(row.path);
    check(source && inputs.get(source) === row.sha256, 'Staged asset differs from build source: ' + row.path);
  }
}
// The pair binds final bytes. Repackaging, setup compilation or a future signing
// transform requires a new pair; an existing identity never silently changes.
function pairIdentity(native, vsix, setup) {
  const release = native.manifest?.release;
  check(native.schema === 'vcp-distribution-result/1' && native.status === 'release-candidate' &&
    release?.schema === 'vcp-release-identity/1' && native.manifest.source?.dirty === false &&
    native.manifest.build?.status === 'verified-release-build', 'Strict native release result required');
  check(vsix.schema === 'vcp-vsix-package/1' && JSON.stringify(vsix.release) === JSON.stringify(release) &&
    vsix.extension?.version === release.vsix_version && vsix.sdk?.version === release.sdk_version &&
    vsix.extension.source?.git_commit === release.reviewed_commit && vsix.extension.source?.dirty === false &&
    vsix.engine?.native_archive_sha256 === native.archive_sha256, 'VSIX/native release identity mismatch');
  const executable = native.manifest.files?.filter(row => row.path === 'vcp.exe');
  check(executable?.length === 1 && vsix.engine.executable_sha256 === executable[0].sha256 &&
    vsix.engine.build_receipt_sha256 === native.manifest.build.receipt_sha256 &&
    vsix.engine.source_commit === release.reviewed_commit && vsix.engine.source_dirty === false,
  'VSIX/native executable or build provenance mismatch');
  check(sha(native.archive_sha256) && sha(vsix.archive?.sha256), 'Missing final artifact hashes');
  if (setup) check(setup.schema === 'vcp-setup-result/1' && setup.native_archive_sha256 === native.archive_sha256 &&
    setup.candidate_id === release.candidate_id && sha(setup.archive?.sha256), 'Setup/native release identity mismatch');
  const artifacts = { native_zip_sha256: native.archive_sha256, vsix_sha256: vsix.archive.sha256,
    setup_sha256: setup?.archive.sha256 ?? null };
  return { schema: 'vcp-release-pair/1', status: 'qualification-required', release, artifacts,
    pair_id: hash(JSON.stringify([release.candidate_id, artifacts])), signing: { status: 'unsigned', transformations: [] } };
}
if (require.main === module) {
  try {
    const [command, ...args] = process.argv.slice(2);
    let result;
    if (command === 'source' && args.length === 2) {
      const root = path.resolve(args[0]), selected = channel(root); verifyVersions(root, selected);
      result = captureSource(root, args[1]);
    } else if (command === 'verify-build' && args.length === 4) result = verifyBuild(path.resolve(args[0]), path.resolve(args[1]), path.resolve(args[2]), args[3]);
    else throw Error('Use source <root> <reviewed-commit> or verify-build <root> <receipt> <executable> <reviewed-commit>');
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch (error) { console.error('Release provenance failed: ' + error.message); process.exitCode = 1; }
}
module.exports = { hash, fileHash, json, channel, captureSource, verifyVersions, peArchitecture, verifyExecutable,
  releaseIdentity, validateReceipt, verifyBuild, verifyPayloadSources, pairIdentity };
