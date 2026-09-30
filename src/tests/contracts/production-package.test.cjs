// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const p = require('../../../scripts/release/provenance.cjs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs'), os = require('node:os');
const { verify } = require('../../../scripts/evals/production-package.cjs');
const digest = p.hash('fixture');
function fixture() {
  const selected = { channel: 'internal-beta', native_version: '0.2.0-beta.1', sdk_version: '0.2.1', vsix_version: '0.2.1',
    target: 'x86_64-pc-windows-msvc', signing: { status: 'unsigned' }, config_sha256: digest };
  const files = ['src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/.cargo/config.toml']
    .map(path => ({ path, bytes: 7, sha256: digest }));
  const source = { commit: 'a'.repeat(40), files, content_sha256: p.hash(JSON.stringify(files)) };
  const release = p.releaseIdentity(selected, source, digest);
  const artifact = name => ({ target: { name }, features: [], profile: { test: false, opt_level: '3' },
    package_id: 'path+file:///source#vcp-cli@' + selected.native_version });
  const build = { schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, source_commit: source.commit,
    source_dirty: false, source_stable: true, toolchain_stable: true, qualification_build: false, profile: 'release', target: selected.target,
    source_content_sha256: source.content_sha256, inputs: files, executable_sha256: digest, release,
    executable_version: selected.native_version, executable_target: selected.target, compiler_artifact: artifact('vcp'),
    launcher_version: selected.native_version, launcher_target: selected.target, launcher_sha256: digest,
    launcher_compiler_artifact: artifact('vcp-launch'), vcp_features: [{ target: 'vcp', features: [] }],
    command: ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features', '-p', 'vcp-cli',
      '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target, '--target-dir', '/output/cargo-target', '-j', '2', '--message-format=json-render-diagnostics'],
    rustflags: ['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static'], rustc: ['rustc 1.95.0', 'release: 1.95.0'],
    native_tools: ['cl', 'link', 'lib', 'cmake', 'ninja', 'rustc', 'cargo', 'node'].map(name => ({ name, sha256: digest })),
    upstream_before_sha256: digest, upstream_after_sha256: digest, log_sha256: digest, cargo_configs: [{ sha256: digest }],
    dependency_sources_stable: true, dependencies_before_sha256: digest, dependencies_after_sha256: digest,
    dependency_source: { schema: 'vcp-release-dependencies/1', status: 'verified', target: selected.target,
      components: 1, workspace_lock_sha256: digest, inventory_sha256: digest } };
  const manifest = { schema: 'vcp-distribution-manifest/1', release, source: { dirty: false, git_commit: source.commit },
    build: { status: 'verified-release-build', receipt: 'build-receipt.json', receipt_sha256: digest },
    notices: { schema: 'vcp-notice-bundle/1', status: 'complete-with-recorded-provenance-limitations', inventory_sha256: digest },
    files: [{ path: 'vcp.exe', bytes: 7, sha256: digest }, { path: 'build-receipt.json', bytes: 7, sha256: digest }] };
  return { result: { schema: 'vcp-distribution-result/1', status: 'release-candidate', package: 'vcp-0.2.0-beta.1-windows-x64-unsigned.zip', archive_sha256: digest, manifest }, manifest, build };
}
test('qualification accepts versioned strict artifacts and binds archived source/build identity', () => {
  const { result, manifest, build } = fixture();
  assert.equal(verify(result, manifest, build, digest).release.candidate_id, manifest.release.candidate_id);
  assert.throws(() => verify(result, manifest, build, 'b'.repeat(64)), /identity/);
  const mismatched = structuredClone(manifest); mismatched.source.git_commit = 'b'.repeat(40);
  assert.throws(() => verify(result, mismatched, build, digest), /Archived manifest/);
});
test('qualification rejects historical loose, debug, changed-source and qualification receipts', () => {
  for (const mutate of [
    f => { f.result.status = 'candidate'; }, f => { f.result.package = '../escape.zip'; },
    f => { f.manifest.build.status = 'recorded-local-build'; }, f => { f.manifest.source.dirty = true; },
    f => { delete f.manifest.notices; }, f => { f.build.qualification_build = true; },
    f => { f.build.compiler_artifact.features = ['qualification']; }, f => { f.build.profile = 'debug'; },
    f => { f.build.inputs[0].sha256 = 'b'.repeat(64); }, f => { f.build.release.candidate_id = 'b'.repeat(64); },
  ]) {
    const f = fixture(); mutate(f); assert.throws(() => verify(f.result, f.manifest, f.build, digest));
  }
});
test('Windows ZIP boundary rejects traversal, collisions and changed bytes before extraction', { skip: process.platform !== 'win32' }, () => {
  const output = execFileSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/evals/production-package.test.ps1')],
    { encoding: 'utf8', windowsHide: true, timeout: 30000, stdio: ['ignore', 'pipe', 'pipe'] });
  const evidence = JSON.parse(output);
  assert.equal(evidence.status, 'pass'); assert.equal(evidence.cases.length, 5);
});

test('production startup retains a failed receipt and never dispatches a loose package', { skip: process.platform !== 'win32' }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-startup130-rejection-'));
  const receipt = path.join(root, 'native.json'), fixture = path.join(root, 'fixture.json');
  fs.writeFileSync(receipt, JSON.stringify({ schema: 'vcp-distribution-result/1', status: 'candidate' }));
  fs.writeFileSync(fixture, JSON.stringify({ backend: 'Files', versions: Array(130).fill('synthetic') }));
  const output = path.join(root, 'evidence');
  assert.throws(() => execFileSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/evals/production-startup130.ps1'),
    '-PackageResult', receipt, '-InstalledExecutable', path.join(root, 'never-executed', 'vcp.exe'),
    '-FixtureInput', fixture, '-QualificationExecutable', process.execPath, '-NodeExecutable', process.execPath, '-OutputRoot', output],
  { encoding: 'utf8', windowsHide: true, timeout: 30000, stdio: ['ignore', 'pipe', 'pipe'] }));
  const report = JSON.parse(fs.readFileSync(path.join(output, 'result.json'), 'utf8').replace(/^\uFEFF/, ''));
  assert.equal(report.status, 'failed'); assert.match(report.error, /Strict production/);
  assert.equal(report.process_cleanup_complete, true); assert.deepEqual(report.resources, []);
  assert.equal(fs.existsSync(path.join(output, 'process-events.jsonl')), false);
  assert.equal(fs.existsSync(path.join(output, 'stdout.json')), false);
});
