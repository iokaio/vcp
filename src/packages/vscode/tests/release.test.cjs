// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const { releaseMode, validateNativeIdentity, checkEnvironment, compileRelease } = require('../scripts/release.cjs');
const { execFileSync } = require('node:child_process');
const { prepareOutput } = require('../scripts/output.cjs');
const provenance = require('../../../../scripts/release/provenance.cjs');
const sha = 'a'.repeat(64), commit = 'b'.repeat(40);

function fixture() {
  const selected = { channel: 'internal-beta', native_version: '0.2.0-beta.1', sdk_version: '0.2.1', vsix_version: '0.2.1',
    target: 'x86_64-pc-windows-msvc', signing: { status: 'unsigned' }, config_sha256: sha };
  const source = { commit, content_sha256: sha, files: [
    { path: 'src/third_party/codex/codex-rs/Cargo.lock', bytes: 1, sha256: sha },
    { path: 'src/third_party/codex/codex-rs/.cargo/config.toml', bytes: 1, sha256: sha }] };
  const release = provenance.releaseIdentity(selected, source, sha);
  const receipt = { schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, source_commit: commit,
    source_dirty: false, source_stable: true, toolchain_stable: true, qualification_build: false, profile: 'release', target: selected.target,
    source_content_sha256: sha, inputs: source.files, executable_sha256: sha, release,
    executable_version: selected.native_version, executable_target: selected.target,
    launcher_version: selected.native_version, launcher_target: selected.target, launcher_sha256: sha,
    launcher_compiler_artifact: { target: { name: 'vcp-launch' }, features: [], profile: { test: false, opt_level: '3' }, package_id: 'path+file:///source#vcp-cli@' + selected.native_version },
    compiler_artifact: { target: { name: 'vcp' }, features: [], profile: { test: false, opt_level: '3' }, package_id: 'path+file:///source#vcp-cli@' + selected.native_version },
    vcp_features: [{ target: 'vcp', features: [] }],
    command: ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features', '-p', 'vcp-cli', '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target, '--target-dir', '/output/cargo-target', '-j', '2', '--message-format=json-render-diagnostics'],
    rustflags: ['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static'], rustc: ['release: 1.95.0'],
    native_tools: ['cl', 'link', 'lib', 'cmake', 'ninja', 'rustc', 'cargo', 'node'].map(name => ({ name, sha256: sha })),
    upstream_before_sha256: sha, upstream_after_sha256: sha, log_sha256: sha, cargo_configs: [{ sha256: sha }],
    dependency_sources_stable: true, dependencies_before_sha256: sha, dependencies_after_sha256: sha,
    dependency_source: { schema: 'vcp-release-dependencies/1', status: 'verified', target: selected.target, components: 1, workspace_lock_sha256: sha, inventory_sha256: sha } };
  const native = { schema: 'vcp-distribution-result/1', status: 'release-candidate', manifest: {
    release, source: { git_commit: commit, dirty: false }, build: { status: 'verified-release-build', receipt: 'build-receipt.json', receipt_sha256: sha },
    files: ['vcp.exe', 'vcp-launch.exe', 'build-receipt.json'].map(path => ({ path, sha256: sha })) } };
  return { selected, source, receipt, native };
}

test('beta packaging cannot downgrade strict inputs or override the selected version', () => {
  const { native } = fixture();
  const options = { '--reviewed-commit': commit, '--build-receipt': path.resolve('build-receipt.json') };
  assert.equal(releaseMode(native, options), true);
  assert.throws(() => releaseMode(native, {}), /reviewed-commit/);
  assert.throws(() => releaseMode(native, { '--reviewed-commit': commit }), /build-receipt/);
  assert.throws(() => releaseMode(native, { ...options, '--build-receipt': 'relative.json' }), /absolute/);
  assert.throws(() => releaseMode(native, { ...options, '--version': '0.1.1' }), /version/);
  const unverified = { manifest: { build: { status: 'caller-supplied-unverified' } } };
  assert.equal(releaseMode(unverified, {}), false);
  assert.throws(() => releaseMode(unverified, { '--reviewed-commit': commit }), /verified/);
  for (const property of ['status', 'release', 'build']) {
    const partial = { manifest: {} };
    if (property === 'status') partial.status = native.status;
    else partial.manifest[property] = native.manifest[property];
    assert.throws(() => releaseMode(partial, options), /Unverified/);
  }
});

test('beta VSIX binds the native release, reviewed source, production receipt and both executable hashes', () => {
  const { native, selected, source, receipt } = fixture();
  assert.deepEqual(validateNativeIdentity(native, selected, source, sha, receipt), native.manifest.release);
  for (const change of [
    n => { n.status = 'candidate'; }, n => { n.manifest.source.dirty = true; },
    n => { n.manifest.source.git_commit = 'c'.repeat(40); }, n => { n.manifest.release.vsix_version = '0.2.3'; },
    n => { n.manifest.build.status = 'recorded-local-build'; }, n => { n.manifest.build.receipt = '../receipt.json'; },
    n => { n.manifest.files[0].sha256 = 'c'.repeat(64); }, n => { n.manifest.files[1].sha256 = 'c'.repeat(64); },
    n => { n.manifest.files[2].sha256 = 'c'.repeat(64); }, n => { n.manifest.files.push(n.manifest.files[0]); },
  ]) {
    const changed = structuredClone(native); change(changed);
    assert.throws(() => validateNativeIdentity(changed, selected, source, sha, receipt));
  }
  for (const change of [r => { r.qualification_build = true; }, r => { r.compiler_artifact.features.push('qualification'); }, r => { r.inputs[0].sha256 = 'c'.repeat(64); }]) {
    const changed = structuredClone(receipt); change(changed);
    assert.throws(() => validateNativeIdentity(native, selected, source, sha, changed));
  }
});

test('strict output never replaces a prior recognized candidate', t => {
  const artifacts = path.resolve(__dirname, '../../../../artifacts');
  fs.mkdirSync(artifacts, { recursive: true });
  const directory = fs.mkdtempSync(path.join(artifacts, 'beta-vsix-output-'));
  t.after(() => fs.rmSync(directory, { recursive: true }));
  const output = path.join(directory, 'candidate');
  prepareOutput(output, 'vcp-beta-vsix-output/1', true);
  fs.writeFileSync(path.join(output, 'evidence'), 'retain');
  assert.throws(() => prepareOutput(output, 'vcp-beta-vsix-output/1', true), /new directory/);
  assert.throws(() => prepareOutput(output, 'vcp-vsix-output/1'), /unrecognized/);
  assert.equal(fs.readFileSync(path.join(output, 'evidence'), 'utf8'), 'retain');
});

test('strict builds reject Node preloads and module-path overrides', () => {
  checkEnvironment({});
  for (const name of ['NODE_OPTIONS', 'NODE_PATH', 'node_options', 'Node_Path']) assert.throws(() => checkEnvironment({ [name]: 'unreviewed' }), /refuses/);
});

test('strict Windows compilation installs dev tools under production NODE_ENV and never uses a poisoned ancestor compiler', { skip: process.platform !== 'win32' }, t => {
  const repository = path.resolve(__dirname, '../../../..');
  fs.mkdirSync(path.join(repository, 'artifacts'), { recursive: true });
  const root = fs.mkdtempSync(path.join(repository, 'artifacts/beta-vsix-build-'));
  t.after(() => fs.rmSync(root, { recursive: true }));
  const copy = relative => {
    fs.mkdirSync(path.dirname(path.join(root, relative)), { recursive: true });
    fs.copyFileSync(path.join(repository, relative), path.join(root, relative));
  };
  for (const relative of ['release/internal-beta.json', 'src/crates/vcp-cli/Cargo.toml', 'src/third_party/codex/codex-rs/Cargo.lock', 'src/packages/protocol-ts/package.json']) copy(relative);
  fs.writeFileSync(path.join(root, '.gitignore'), 'node_modules/\ndist/\nartifacts/\n');
  const ancestor = path.join(root, 'node_modules/typescript');
  fs.mkdirSync(path.join(ancestor, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(ancestor, 'package.json'), JSON.stringify({ name: 'typescript', version: '5.9.3' }));
  fs.writeFileSync(path.join(ancestor, 'bin/tsc'), 'throw Error("UNVERIFIED_ANCESTOR_COMPILER_MUST_NOT_RUN")');
  for (const name of ['sdk-ts', 'vscode']) {
    const directory = 'src/packages/' + name;
    for (const file of ['package.json', 'package-lock.json']) copy(directory + '/' + file);
    const absolute = path.join(root, directory);
    fs.mkdirSync(path.join(absolute, 'src'));
    fs.writeFileSync(path.join(absolute, 'src/index.ts'), 'export const identity: string = "reviewed fixture";\n');
    fs.writeFileSync(path.join(absolute, 'tsconfig.json'), JSON.stringify({ compilerOptions: { target: 'ES2022', module: 'NodeNext', rootDir: 'src', outDir: 'dist', skipLibCheck: true }, include: ['src/**/*.ts'] }));
    fs.mkdirSync(path.join(absolute, 'dist'));
    fs.writeFileSync(path.join(absolute, 'dist/index.js'), 'STALE_RUNTIME_MUST_NOT_SHIP');
    fs.mkdirSync(path.join(absolute, 'node_modules/typescript/bin'), { recursive: true });
    fs.writeFileSync(path.join(absolute, 'node_modules/typescript/bin/tsc'), 'throw Error("UNVERIFIED_COMPILER_MUST_NOT_RUN")');
  }
  const git = args => execFileSync('git', ['-c', 'safe.directory=' + root.replaceAll('\\', '/'), ...args], { cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] }).toString().trim();
  git(['init']); git(['add', '.']); git(['-c', 'user.name=Release Test', '-c', 'user.email=release@example.invalid', 'commit', '-m', 'fixture']);
  const reviewed = git(['rev-parse', 'HEAD']), output = path.join(root, 'artifacts'); fs.mkdirSync(output);
  const priorNodeEnv = process.env.NODE_ENV;
  let record;
  try {
    process.env.NODE_ENV = 'production';
    record = compileRelease(root, reviewed, output);
  } finally {
    if (priorNodeEnv === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = priorNodeEnv;
  }
  assert.match(record.node_sha256, /^[a-f0-9]{64}$/);
  assert.match(record.npm_tree_sha256, /^[a-f0-9]{64}$/);
  assert.equal(record.packages.length, 2);
  for (const build of record.packages) {
    assert.match(build.compiler_tree_sha256, /^[a-f0-9]{64}$/);
    assert.ok(fs.existsSync(path.join(root, 'src/packages', build.package, 'node_modules/typescript/lib/_tsc.js')));
    assert.match(fs.readFileSync(path.join(root, 'src/packages', build.package, 'dist/index.js'), 'utf8'), /reviewed fixture/);
    for (const log of [build.install_log, build.compile_log]) assert.equal(provenance.fileHash(path.join(output, log.file)), log.sha256);
  }
  assert.equal(provenance.captureSource(root, reviewed).dirty, false);
  const dist = path.join(root, 'src/packages/sdk-ts/dist'), unrelated = path.join(output, 'unrelated');
  fs.mkdirSync(unrelated); fs.writeFileSync(path.join(unrelated, 'index.js'), 'preserve unrelated output');
  assert.ok(path.resolve(dist).startsWith(root + path.sep));
  fs.rmSync(dist, { recursive: true });
  fs.symlinkSync(unrelated, dist, 'junction');
  try {
    assert.throws(() => compileRelease(root, reviewed, output), /Redirected build directory/);
    assert.equal(fs.readFileSync(path.join(unrelated, 'index.js'), 'utf8'), 'preserve unrelated output');
  } finally { fs.unlinkSync(dist); }
  fs.mkdirSync(dist);
  fs.linkSync(path.join(unrelated, 'index.js'), path.join(dist, 'index.js'));
  assert.throws(() => compileRelease(root, reviewed, output), /multiply linked/);
  assert.equal(fs.readFileSync(path.join(unrelated, 'index.js'), 'utf8'), 'preserve unrelated output');
  fs.unlinkSync(path.join(dist, 'index.js'));
  fs.appendFileSync(path.join(root, 'src/packages/sdk-ts/src/index.ts'), '// changed after review\n');
  assert.throws(() => compileRelease(root, reviewed, output), /clean checkout/);
});
