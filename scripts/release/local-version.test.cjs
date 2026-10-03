// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { prepare, jsonDocument } = require('./local-version.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const patchPath = 'src/third_party/patches/codex/local-candidate-product-version.patch';
function fixture(t, product = '0.2.6', overrides = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-local-version-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (relative, bytes) => { fs.mkdirSync(path.dirname(path.join(root, relative)), { recursive: true }); fs.writeFileSync(path.join(root, relative), bytes); };
  const json = (relative, value) => write(relative, JSON.stringify(value, null, 2) + '\n');
  const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
  const channelVersion = overrides.channel || product;
  json('release/internal-beta.json', { schema: 'vcp-beta-channel/1', channel: 'internal-beta', native_version: channelVersion, sdk_version: channelVersion, vsix_version: channelVersion, installer: { version: '6.7.3' } });
  write('src/crates/vcp-cli/Cargo.toml', '# Preserve this comment\n[package]\nname = "vcp-cli"\nversion = "' + product + '"\n[dependencies]\nunrelated = "0.2.6"\n');
  let originalLock = '# Lockfile\n[[package]]\nname = "unrelated"\nversion = "0.2.6"\n\n[[package]]\nname = "vcp-cli"\nversion = "' + product + '"\ndependencies = [\n "unrelated",\n]\n';
  if (overrides.crlf) originalLock = originalLock.replaceAll('\n', '\r\n');
  write('src/third_party/codex/codex-rs/Cargo.lock', originalLock);
  for (const directory of ['sdk-ts', 'vscode']) {
    const name = directory === 'sdk-ts' ? '@vcp/sdk' : 'vcp', v = overrides[directory] || product;
    json('src/packages/' + directory + '/package.json', { name, version: v, dependencies: { pinned: '0.2.6' } });
    const packages = { '': { name, version: v, dependencies: { pinned: '0.2.6' } }, 'node_modules/pinned': { version: '0.2.6', integrity: 'retain-me' } };
    if (directory === 'vscode') packages['../sdk-ts'] = { name: '@vcp/sdk', version: overrides.bundled || product, dependencies: { pinned: '0.2.6' } };
    json('src/packages/' + directory + '/package-lock.json', { name, version: overrides.lock || v, lockfileVersion: 3, packages });
  }
  const selection = { schema_version: 1, component: 'codex', commit: 'a'.repeat(40), destination: 'src/third_party/codex', result_inventory: 'src/third_party/components/codex-files.json', patches: [] };
  const files = [{ path: 'codex-rs/Cargo.lock', original: { mode: '100644', bytes: Buffer.byteLength(originalLock), sha256: hash(originalLock) }, result: { mode: '100644', bytes: Buffer.byteLength(originalLock), sha256: hash(originalLock) }, transformation: 'patch-series' }];
  json('src/third_party/components/codex-selection.json', selection);
  json('src/third_party/components/codex-files.json', { schema_version: 1, component: 'codex', commit: selection.commit, selection_sha256: hash(JSON.stringify(selection)), files_sha256: hash(JSON.stringify(files)), files, removed: [] });
  write('src/third_party/patches/codex/README.md', '# Upstream modifications\nExisting description.\n');
  fs.mkdirSync(path.join(root, path.dirname(patchPath)), { recursive: true });
  return { root, write, json, read, originalLock, output: path.join(root, 'artifacts/local-setup') };
}
function snapshot(root) {
  const result = {};
  function walk(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(filename);
      else result[path.relative(root, filename)] = fs.readFileSync(filename).toString('base64');
    }
  }
  walk(root); return result;
}
function assertSynchronized(f, expected) {
  const channel = JSON.parse(f.read('release/internal-beta.json'));
  assert.deepEqual([channel.native_version, channel.sdk_version, channel.vsix_version], [expected, expected, expected]);
  assert.match(f.read('src/crates/vcp-cli/Cargo.toml'), new RegExp('version = "' + expected.replaceAll('.', '\\.') + '"'));
  for (const directory of ['sdk-ts', 'vscode']) {
    assert.equal(JSON.parse(f.read('src/packages/' + directory + '/package.json')).version, expected);
    const lock = JSON.parse(f.read('src/packages/' + directory + '/package-lock.json'));
    assert.equal(lock.version, expected); assert.equal(lock.packages[''].version, expected);
    if (directory === 'vscode') assert.equal(lock.packages['../sdk-ts'].version, expected);
    assert.equal(lock.packages['node_modules/pinned'].version, '0.2.6');
    assert.equal(lock.packages['node_modules/pinned'].integrity, 'retain-me');
    assert.equal(lock.packages[''].dependencies.pinned, '0.2.6');
  }
  assert.match(f.read('src/crates/vcp-cli/Cargo.toml'), /unrelated = "0\.2\.6"/);
  assert.match(f.read('src/third_party/codex/codex-rs/Cargo.lock'), /name = "unrelated"\nversion = "0\.2\.6"/);
}
test('used version advances, synchronizes exact product fields and preserves output bytes', t => {
  const f = fixture(t); f.write('artifacts/local-setup/0.2.6/old.exe', 'retain-existing-bytes');
  const result = prepare(f.root, f.output);
  assert.equal(result.version, '0.2.7'); assertSynchronized(f, '0.2.7');
  assert.equal(f.read('artifacts/local-setup/0.2.6/old.exe'), 'retain-existing-bytes');
  assert.equal(result.versionRoot, path.join(f.output, '0.2.7'));
  assert.ok(fs.statSync(result.canonicalReservation).isDirectory());
  assert.match(f.read('src/third_party/patches/codex/README.md'), /local-candidate-product-version\.patch/);
});
test('repeated preparation increments once each and reuses a single reconstructible final patch', t => {
  const f = fixture(t); f.write('artifacts/local-setup/0.2.6/receipt', 'original');
  assert.equal(prepare(f.root, f.output).version, '0.2.7');
  assert.equal(prepare(f.root, f.output).version, '0.2.8'); assertSynchronized(f, '0.2.8');
  const selection = JSON.parse(f.read('src/third_party/components/codex-selection.json'));
  assert.equal(selection.patches.length, 1); assert.equal(selection.patches[0].sha256, hash(f.read(patchPath)));
  const record = JSON.parse(f.read('src/third_party/components/codex-files.json'));
  assert.equal(record.selection_sha256, hash(JSON.stringify(selection)));
  assert.equal(record.files_sha256, hash(JSON.stringify(record.files)));
  assert.equal(record.files[0].result.sha256, hash(f.read('src/third_party/codex/codex-rs/Cargo.lock')));
  assert.equal(record.files[0].original.sha256, hash(f.originalLock));
  const scratch = path.join(f.root, 'reconstruct'); fs.mkdirSync(path.join(scratch, 'codex-rs'), { recursive: true });
  fs.writeFileSync(path.join(scratch, 'codex-rs/Cargo.lock'), f.originalLock);
  execFileSync('git', ['-c', 'core.autocrlf=false', 'apply', path.join(f.root, patchPath)], { cwd: scratch, windowsHide: true });
  assert.equal(fs.readFileSync(path.join(scratch, 'codex-rs/Cargo.lock'), 'utf8'), f.read('src/third_party/codex/codex-rs/Cargo.lock'));
  assert.equal((f.read('src/third_party/patches/codex/README.md').match(/Local setup preparation/g) || []).length, 1);
});
test('an unused manually advanced source version is preserved', t => {
  const f = fixture(t, '0.2.9'); f.write('artifacts/local-setup/0.2.8/result', 'old');
  const before = snapshot(f.root), result = prepare(f.root, f.output);
  assert.equal(result.version, '0.2.9'); assert.deepEqual(result.changed, []); assert.deepEqual(snapshot(f.root), before);
});
test('manual source drift is synchronized upward including extension bundled SDK metadata', t => {
  const f = fixture(t, '0.2.6', { channel: '0.2.9', lock: '0.2.7', bundled: '0.2.8' });
  assert.equal(prepare(f.root, f.output).version, '0.2.9'); assertSynchronized(f, '0.2.9');
});
test('recorded signed candidates and local VSIX candidates establish a numeric floor', t => {
  const f = fixture(t); fs.mkdirSync(path.join(f.root, 'artifacts/local-vsix/0.2.10'), { recursive: true });
  f.json('artifacts/local-candidate/signed-new/packet/pair.json', { release: { native_version: '0.2.12', sdk_version: '0.2.12', vsix_version: '0.2.12' } });
  fs.mkdirSync(path.join(f.output, '0.2.900-beta.1'), { recursive: true });
  assert.equal(prepare(f.root, f.output).version, '0.2.13');
});
test('custom output roots reserve canonical versions so switching roots cannot reuse bytes', t => {
  const f = fixture(t), first = path.join(f.root, 'custom-one'), second = path.join(f.root, 'custom-two');
  const a = prepare(f.root, first); assert.equal(a.version, '0.2.6');
  assert.ok(fs.statSync(a.canonicalReservation).isDirectory()); assert.ok(fs.statSync(a.versionRoot).isDirectory());
  f.write('custom-one/0.2.6/installer.exe', 'candidate');
  const b = prepare(f.root, second); assert.equal(b.version, '0.2.7');
  assert.equal(f.read('custom-one/0.2.6/installer.exe'), 'candidate');
});
test('Windows output path case aliases reserve the same physical candidate only once', { skip: process.platform !== 'win32' }, t => {
  const f = fixture(t); f.write('artifacts/local-setup/0.2.6/old', 'old');
  const output = f.output.toUpperCase(), result = prepare(f.root, output);
  assert.equal(result.version, '0.2.7'); assert.ok(fs.statSync(result.versionRoot).isDirectory());
  assert.ok(fs.statSync(result.canonicalReservation).isDirectory());
  assert.equal(prepare(f.root, output).version, '0.2.8');
});
test('reservation collision rolls back source edits and preserves the competing output', t => {
  const f = fixture(t), output = path.join(f.root, 'custom-output');
  f.write('artifacts/local-setup/0.2.6/old', 'old'); fs.mkdirSync(output);
  const before = snapshot(f.root), mkdir = fs.mkdirSync, competing = path.join(output, '0.2.7');
  fs.mkdirSync = (directory, options) => {
    if (directory === competing) {
      fs.writeFileSync(competing, 'another builder owns this');
      const error = Error('Competing reservation'); error.code = 'EEXIST'; throw error;
    }
    return mkdir(directory, options);
  };
  try { assert.throws(() => prepare(f.root, output), /Competing reservation/); }
  finally { fs.mkdirSync = mkdir; }
  assert.equal(f.read('custom-output/0.2.7'), 'another builder owns this');
  const after = snapshot(f.root); delete after[path.join('custom-output', '0.2.7')];
  assert.deepEqual(after, before); assert.equal(fs.existsSync(path.join(f.output, '0.2.7')), false);
});
test('Windows CRLF lockfiles retain exact bytes and reconstruct through the generated patch', t => {
  const f = fixture(t, '0.2.6', { crlf: true }); f.write('artifacts/local-setup/0.2.6/old', 'old');
  prepare(f.root, f.output);
  const newLock = f.read('src/third_party/codex/codex-rs/Cargo.lock');
  assert.equal(newLock, f.originalLock.replace('name = "vcp-cli"\r\nversion = "0.2.6"', 'name = "vcp-cli"\r\nversion = "0.2.7"'));
  const scratch = path.join(f.root, 'reconstruct'); fs.mkdirSync(path.join(scratch, 'codex-rs'), { recursive: true });
  fs.writeFileSync(path.join(scratch, 'codex-rs/Cargo.lock'), f.originalLock);
  execFileSync('git', ['-c', 'core.autocrlf=false', 'apply', path.join(f.root, patchPath)], { cwd: scratch, windowsHide: true });
  assert.equal(fs.readFileSync(path.join(scratch, 'codex-rs/Cargo.lock'), 'utf8'), newLock);
});
for (const [description, corrupt] of [
  ['malformed JSON', f => f.write('src/packages/vscode/package-lock.json', '{')],
  ['missing synchronized lock field', f => { const p = JSON.parse(f.read('src/packages/sdk-ts/package-lock.json')); delete p.packages[''].version; f.json('src/packages/sdk-ts/package-lock.json', p); }],
  ['nonnumeric product version', f => { const p = JSON.parse(f.read('release/internal-beta.json')); p.native_version = '0.2.7-beta.1'; f.json('release/internal-beta.json', p); }],
  ['unexpected native lock bytes', f => f.write('src/third_party/codex/codex-rs/Cargo.lock', f.originalLock + '# Unrecorded change\n')],
  ['unregistered automatic patch', f => f.write(patchPath, 'user work')],
  ['provenance identity mismatch', f => { const p = JSON.parse(f.read('src/third_party/components/codex-files.json')); p.selection_sha256 = '0'.repeat(64); f.json('src/third_party/components/codex-files.json', p); }],
]) test(description + ' fails before any source mutation or candidate reservation', t => {
  const f = fixture(t); f.write('artifacts/local-setup/0.2.6/old', 'old'); corrupt(f);
  const before = snapshot(f.root); assert.throws(() => prepare(f.root, f.output)); assert.deepEqual(snapshot(f.root), before);
  assert.equal(fs.existsSync(path.join(f.output, '0.2.7')), false);
});
test('JSON surgical replacement retains BOM, CRLF, identical unrelated versions and escapes', () => {
  const before = '\uFEFF{\r\n  "version": "0.2.6",\r\n  "a": {"version": "0.2.6", "escaped": "\\\\\\\""}\r\n}\r\n';
  assert.equal(jsonDocument(before).replace([[['version'], '0.2.7']]), before.replace('"version": "0.2.6"', '"version": "0.2.7"'));
  assert.throws(() => jsonDocument('{"version":"0.2.6","version":"0.2.7"}'), /Duplicate JSON key/);
});
