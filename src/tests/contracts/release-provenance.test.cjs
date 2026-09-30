// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');
const p = require('../../../scripts/release/provenance.cjs');
const { recordPair } = require('../../../scripts/release/pair.cjs');
const digest = value => p.hash(value);
const hash = digest('fixture'), commit = 'a'.repeat(40);
function temporary(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-release-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true })); return root;
}
function fixture() {
  const selected = { channel: 'internal-beta', native_version: '0.2.0-beta.1', sdk_version: '0.2.1', vsix_version: '0.2.1',
    target: 'x86_64-pc-windows-msvc', signing: { status: 'unsigned' }, config_sha256: hash };
  const source = { commit, content_sha256: hash, files: [{ path: 'src/third_party/codex/codex-rs/Cargo.lock', bytes: 7, sha256: hash },
    { path: 'src/third_party/codex/codex-rs/.cargo/config.toml', bytes: 7, sha256: hash }] };
  const release = p.releaseIdentity(selected, source, hash);
  const receipt = { schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, source_commit: commit,
    source_dirty: false, source_stable: true, toolchain_stable: true, qualification_build: false, profile: 'release', target: selected.target,
    source_content_sha256: hash, inputs: source.files, executable_sha256: hash, release,
    executable_version: selected.native_version, executable_target: selected.target,
    launcher_version: selected.native_version, launcher_target: selected.target, launcher_sha256: hash,
    launcher_compiler_artifact: { target: { name: 'vcp-launch' }, features: [],
      profile: { test: false, opt_level: '3' }, package_id: 'path+file:///source#vcp-cli@' + selected.native_version },
    vcp_features: [{ target: 'vcp', features: [] }], compiler_artifact: { target: { name: 'vcp' }, features: [],
      profile: { test: false, opt_level: '3' }, package_id: 'path+file:///source#vcp-cli@' + selected.native_version },
    command: ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features', '-p', 'vcp-cli',
      '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target, '--target-dir', '/output/cargo-target', '-j', '2', '--message-format=json-render-diagnostics'],
    rustflags: ['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static'], rustc: ['rustc 1.95.0', 'release: 1.95.0'],
    native_tools: ['cl', 'link', 'lib', 'cmake', 'ninja', 'rustc', 'cargo', 'node'].map(name => ({ name, sha256: hash })),
    upstream_before_sha256: hash, upstream_after_sha256: hash, log_sha256: hash, cargo_configs: [{ sha256: hash }],
    dependency_sources_stable: true, dependencies_before_sha256: hash, dependencies_after_sha256: hash,
    dependency_source: { schema: 'vcp-release-dependencies/1', status: 'verified', target: selected.target,
      components: 1, workspace_lock_sha256: hash, inventory_sha256: hash } };
  return { selected, source, receipt, release };
}
test('strict receipt refuses dirty, stale, unverified, wrong-version and qualification builds', () => {
  const { selected, source, receipt } = fixture();
  assert.deepEqual(p.validateReceipt(receipt, selected, source, hash), receipt.release);
  const mutations = [
    r => { r.exit_code = 1; }, r => { delete r.cargo_exit_code; }, r => { r.source_dirty = true; },
    r => { r.source_stable = false; }, r => { r.toolchain_stable = false; }, r => { r.qualification_build = true; }, r => { r.profile = 'debug'; },
    r => { r.target = 'aarch64-pc-windows-msvc'; }, r => { r.source_commit = 'b'.repeat(40); },
    r => { r.source_content_sha256 = 'b'.repeat(64); }, r => { r.inputs[0].sha256 = 'b'.repeat(64); },
    r => { delete r.release; }, r => { r.release.config_sha256 = 'b'.repeat(64); },
    r => { r.executable_version = '0.1.0'; }, r => { r.executable_sha256 = 'b'.repeat(64); },
    r => { delete r.launcher_sha256; }, r => { r.launcher_version = '0.1.0'; },
    r => { r.launcher_compiler_artifact.features.push('qualification'); },
    r => { r.launcher_compiler_artifact.profile.opt_level = '0'; },
    r => { r.vcp_features[0].features.push('qualification'); }, r => { r.compiler_artifact.features.push('qualification'); },
    r => { r.compiler_artifact.profile.opt_level = '0'; }, r => { r.command.push('--features', 'qualification'); },
    r => { r.command.push('--config', 'unreviewed.toml'); }, r => { r.cargo_configs[0].sha256 = 'b'.repeat(64); },
    r => { r.rustflags.push('-C', 'opt-level=0'); }, r => { r.rustc = ['release: 1.94.0']; },
    r => { r.native_tools.pop(); }, r => { delete r.log_sha256; }, r => { r.cargo_configs = []; },
    r => { r.dependency_sources_stable = false; }, r => { r.dependency_source.workspace_lock_sha256 = 'b'.repeat(64); },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(receipt); mutate(changed);
    assert.throws(() => p.validateReceipt(changed, selected, source, hash));
  }
});
test('release source binds every tracked byte and rejects unselected or dirty source', t => {
  const root = temporary(t), git = args => execFileSync('git', ['-c', 'safe.directory=' + root.replaceAll('\\', '/'), ...args],
    { cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] }).toString().trim();
  git(['init']); fs.writeFileSync(path.join(root, 'input'), 'first'); git(['add', 'input']);
  git(['-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', 'commit', '-m', 'fixture']);
  const selected = git(['rev-parse', 'HEAD']), source = p.captureSource(root, selected);
  assert.equal(source.files[0].sha256, digest('first')); assert.equal(source.dirty, false);
  assert.throws(() => p.captureSource(root, undefined), /reviewed commit/);
  assert.throws(() => p.captureSource(root, commit), /differs/);
  fs.writeFileSync(path.join(root, 'input'), 'second'); assert.throws(() => p.captureSource(root, selected), /clean/);
  git(['update-index', '--assume-unchanged', 'input']);
  assert.throws(() => p.captureSource(root, selected), /assume-unchanged/);
  git(['update-index', '--no-assume-unchanged', 'input']);
  fs.writeFileSync(path.join(root, 'input'), 'first'); fs.writeFileSync(path.join(root, 'untracked'), 'extra');
  assert.throws(() => p.captureSource(root, selected), /clean/);
});
test('native target inspection rejects non-PE, x86 and ARM64 bytes', t => {
  const root = temporary(t), file = path.join(root, 'vcp.exe'), pe = Buffer.alloc(128);
  pe.writeUInt16LE(0x5a4d); pe.writeUInt32LE(64, 0x3c); pe.writeUInt32LE(0x4550, 64);
  pe.writeUInt16LE(0x8664, 68); pe.writeUInt16LE(0x20b, 88); fs.writeFileSync(file, pe);
  assert.equal(p.peArchitecture(file), 'x86_64-pc-windows-msvc');
  for (const machine of [0x14c, 0xaa64]) { pe.writeUInt16LE(machine, 68); fs.writeFileSync(file, pe); assert.throws(() => p.peArchitecture(file), /x86_64/); }
  fs.writeFileSync(file, 'not executable'); assert.throws(() => p.peArchitecture(file), /PE/);
});
test('staged helpers and assets must match the exact build source bytes', t => {
  const root = temporary(t), file = path.join(root, 'NOTICE'); fs.writeFileSync(file, 'notice');
  const receipt = { inputs: [{ path: 'NOTICE', sha256: digest('notice') }] };
  p.verifyPayloadSources(root, receipt);
  const manifest = require('../../../scripts/package-inventory.cjs').buildManifest(root, {});
  fs.writeFileSync(path.join(root, 'manifest.json'), JSON.stringify(manifest));
  p.verifyPayloadSources(root, receipt);
  fs.writeFileSync(path.join(root, 'manifest.json'), JSON.stringify({ ...manifest, files: [] }));
  assert.throws(() => p.verifyPayloadSources(root, receipt), /differs/);
  fs.unlinkSync(path.join(root, 'manifest.json'));
  fs.writeFileSync(file, 'stale notice'); assert.throws(() => p.verifyPayloadSources(root, receipt), /differs/);
  fs.writeFileSync(file, 'notice'); fs.writeFileSync(path.join(root, 'secret.env'), 'fixture');
  assert.throws(() => p.verifyPayloadSources(root, receipt), /differs/);
});
test('packaged user guides bind reviewed source and reject extra documents', t => {
  const root = temporary(t), guides = path.join(root, 'docs/usage');
  fs.mkdirSync(guides, { recursive: true });
  const receipt = { inputs: [] };
  for (const name of ['beta-installation.md', 'beta-onboarding.md', 'beta-recovery.md', 'beta-known-issues.md']) {
    fs.writeFileSync(path.join(guides, name), name);
    receipt.inputs.push({ path: 'docs/usage/' + name, sha256: digest(name) });
  }
  p.verifyPayloadSources(root, receipt);
  fs.writeFileSync(path.join(guides, 'beta-onboarding.md'), 'stale onboarding');
  assert.throws(() => p.verifyPayloadSources(root, receipt), /differs/);
  fs.writeFileSync(path.join(guides, 'beta-onboarding.md'), 'beta-onboarding.md');
  fs.writeFileSync(path.join(guides, 'private-profile.md'), 'private fixture');
  assert.throws(() => p.verifyPayloadSources(root, receipt), /differs/);
});
function pairFixture() {
  const { release } = fixture();
  const native = { schema: 'vcp-distribution-result/1', status: 'release-candidate', archive_sha256: hash, package: 'native.zip',
    manifest: { release, source: { dirty: false }, build: { status: 'verified-release-build', receipt_sha256: hash },
      files: [{ path: 'vcp.exe', sha256: hash }] } };
  const vsix = { schema: 'vcp-vsix-package/1', release, archive: { file: 'editor.vsix', sha256: hash },
    extension: { version: '0.2.1', source: { git_commit: commit, dirty: false } }, sdk: { version: '0.2.1' },
    engine: { native_archive_sha256: hash, executable_sha256: hash, build_receipt_sha256: hash, source_commit: commit, source_dirty: false } };
  return { native, vsix };
}
test('exact pair identity rejects mismatched source, versions, build and transformed bytes', () => {
  const { native, vsix } = pairFixture();
  const pair = p.pairIdentity(native, vsix); assert.equal(pair.status, 'qualification-required');
  for (const mutate of [v => { v.release.candidate_id = 'b'.repeat(64); }, v => { v.extension.source.dirty = true; },
    v => { v.extension.version = '0.1.0'; }, v => { v.engine.native_archive_sha256 = 'b'.repeat(64); },
    v => { v.engine.executable_sha256 = 'b'.repeat(64); }]) {
    const changed = structuredClone(vsix); mutate(changed); assert.throws(() => p.pairIdentity(native, changed));
  }
  assert.throws(() => p.pairIdentity({ ...native, status: 'candidate' }, vsix));
  const changed = structuredClone(vsix); changed.archive.sha256 = 'b'.repeat(64);
  assert.notEqual(p.pairIdentity(native, changed).pair_id, pair.pair_id);
});
test('pair recording independently checks final bytes and never overwrites a receipt', t => {
  const root = temporary(t), { native, vsix } = pairFixture();
  const nativeFile = path.join(root, 'result.json'), vsixFile = path.join(root, 'manifest.json'), output = path.join(root, 'pair.json');
  fs.writeFileSync(path.join(root, 'native.zip'), 'fixture'); fs.writeFileSync(path.join(root, 'editor.vsix'), 'fixture');
  fs.writeFileSync(nativeFile, JSON.stringify(native)); vsix.engine.native_manifest_sha256 = p.fileHash(nativeFile);
  fs.writeFileSync(vsixFile, JSON.stringify(vsix)); assert.equal(recordPair(nativeFile, vsixFile, null, output).schema, 'vcp-release-pair/1');
  assert.throws(() => recordPair(nativeFile, vsixFile, null, output), /EEXIST/);
  fs.writeFileSync(path.join(root, 'editor.vsix'), 'changed');
  assert.throws(() => recordPair(nativeFile, vsixFile, null, path.join(root, 'next.json')), /hash mismatch/);
});
