// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), zlib = require('node:zlib');
const { hash } = require('../../../scripts/release/provenance.cjs');
const { crateFiles } = require('../../../scripts/release/crates.cjs');
const notices = require('../../../scripts/release/notices.cjs');
const inventory = require('../../../scripts/package-inventory.cjs');
function temporary(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-notices-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true })); return root;
}
function archive(files) {
  const records = [];
  for (const [name, text, type = '0'] of files) {
    const bytes = Buffer.from(text), header = Buffer.alloc(512);
    header.write(name); header.write('0000644\0', 100); header.write('0000000\0', 108); header.write('0000000\0', 116);
    header.write(bytes.length.toString(8).padStart(11, '0') + '\0', 124); header.write('00000000000\0', 136);
    header.fill(32, 148, 156); header.write(type, 156); header.write('ustar\0', 257);
    header.write(header.reduce((a, b) => a + b, 0).toString(8).padStart(6, '0') + '\0 ', 148);
    records.push(header, bytes, Buffer.alloc((512 - bytes.length % 512) % 512));
  }
  records.push(Buffer.alloc(1024)); return zlib.gzipSync(Buffer.concat(records));
}
test('crate reader binds original archives and rejects tampering, traversal and links', () => {
  const bytes = archive([['tiny-1.0.0/LICENSE', 'original license text']]), found = [];
  crateFiles(bytes, hash(bytes), (name, body) => found.push([name, body.toString()]));
  assert.deepEqual(found, [['tiny-1.0.0/LICENSE', 'original license text']]);
  assert.throws(() => crateFiles(bytes, 'a'.repeat(64), () => {}), /checksum/);
  for (const [name, type] of [['../LICENSE', '0'], ['tiny-1.0.0/LICENSE', '2']]) {
    const bad = archive([[name, 'bad', type]]); assert.throws(() => crateFiles(bad, hash(bad), () => {}), /archive (path|entry)/);
  }
});
function fixture(t, license = 'MIT', included = true) {
  const root = temporary(t), cargoHome = path.join(root, 'cache');
  for (const relative of ['release', 'src/third_party/codex/codex-rs', 'cache/registry/cache/pinned']) fs.mkdirSync(path.join(root, relative), { recursive: true });
  fs.writeFileSync(path.join(root, 'release/license-overrides.json'), JSON.stringify({ schema: 'vcp-release-license-overrides/1', records: [] }));
  fs.writeFileSync(path.join(root, 'LICENSE'), 'Apache standard fixture terms');
  const files = [['tiny-1.0.0/Cargo.toml', '[package]\nname="tiny"\nversion="1.0.0"\nlicense="' + license + '"\nauthors=["Fixture Author"]\n']];
  if (included) files.push(['tiny-1.0.0/LICENSE', 'Original ' + license + ' fixture grant with copyright attribution']);
  const bytes = archive(files);
  for (const [name, contents] of files) {
    const file = path.join(cargoHome, 'registry/src/pinned', name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, contents);
  }
  fs.writeFileSync(path.join(cargoHome, 'registry/cache/pinned/tiny-1.0.0.crate'), bytes);
  fs.writeFileSync(path.join(root, 'src/third_party/codex/codex-rs/Cargo.lock'), 'version=4\n[[package]]\nname="tiny"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + hash(bytes) + '"\n');
  return { root, options: { tree: 'tiny v1.0.0\n', runtimeTree: 'tiny v1.0.0\n', workspace: { packages: [] }, cargoHome } };
}
test('notice inventory uses verified archive bytes and keeps missing license evidence incomplete', t => {
  const { root, options } = fixture(t);
  const result = notices.collect(root, options);
  assert.equal(result.inventory.status, 'complete'); assert.equal(result.texts.size, 1);
  assert.equal(result.inventory.components[0].licenses[0].origin.kind, 'cargo-archive');
  assert.deepEqual(result.inventory.components[0].authors, ['Fixture Author']);
  const missing = fixture(t, 'MIT', false), incomplete = notices.collect(missing.root, missing.options);
  assert.equal(incomplete.inventory.status, 'incomplete'); assert.match(incomplete.inventory.missing[0], /license text unavailable/);
  const archivePath = path.join(options.cargoHome, 'registry/cache/pinned/tiny-1.0.0.crate');
  const sourceFile = path.join(options.cargoHome, 'registry/src/pinned/tiny-1.0.0/LICENSE');
  const original = fs.readFileSync(sourceFile); fs.appendFileSync(sourceFile, 'cache mutation');
  assert.throws(() => notices.collect(root, options), /differs from locked archive/); fs.writeFileSync(sourceFile, original);
  fs.appendFileSync(archivePath, 'tamper'); assert.throws(() => notices.collect(root, options), /checksum/);
});
test('MPL components retain unchanged source; simple dual grants select Apache without dropping compound conditions', t => {
  const { root, options } = fixture(t, 'MPL-2.0'); const result = notices.collect(root, options);
  assert.equal(result.sources.size, 1);
  assert.equal(result.inventory.components[0].source_archive, 'licenses/sources/tiny-1.0.0.crate');
  assert.equal(notices.selectedLicense('Apache-2.0 OR GPL-2.0-only'), 'Apache-2.0');
  assert.equal(notices.selectedLicense('(MIT OR Apache-2.0) AND Unicode-3.0'), '(MIT OR Apache-2.0) AND Unicode-3.0');
  assert.throws(() => notices.selectedLicense('GPL-2.0-only'), /restrictive/);
  assert.throws(() => notices.selectedLicense(null), /permission/);
});
test('compiled inventory rejects package injection and identifies observed normal/build dependencies', () => {
  const inv = { components: [{ name: 'tiny', version: '1.0.0' }] };
  const artifact = { reason: 'compiler-artifact', package_id: 'registry+https://example.invalid#tiny@1.0.0' };
  notices.verifyCompiledGraph(JSON.stringify(artifact), inv, { packages: [] });
  assert.equal(inv.components[0].compiler_observed, true);
  artifact.package_id = 'registry+https://example.invalid#development-fixture@1.0.0';
  assert.throws(() => notices.verifyCompiledGraph(JSON.stringify(artifact), inv, { packages: [] }), /absent/);
});
test('strict release metadata requires bound notice evidence', () => {
  const metadata = { release: { schema: 'vcp-release-identity/1', reviewed_commit: 'a'.repeat(40) },
    source: { dirty: false, git_commit: 'a'.repeat(40) }, build: { status: 'verified-release-build' } };
  assert.throws(() => inventory.validateMetadata(metadata), /notice inventory/);
  metadata.notices = { schema: 'vcp-notice-bundle/1', inventory: 'component-inventory.json', inventory_sha256: 'b'.repeat(64), status: 'complete-with-recorded-provenance-limitations' };
  assert.equal(inventory.validateMetadata(metadata).notices.inventory, 'component-inventory.json');
});
test('staged license inventory rejects changed texts, unexpected files and mismatched native locks', t => {
  const root = temporary(t); fs.mkdirSync(path.join(root, 'licenses'));
  fs.writeFileSync(path.join(root, 'licenses/notice.txt'), 'attribution');
  const inv = { schema: 'vcp-native-components/1', status: 'complete', missing: [], workspace_lock_sha256: 'a'.repeat(64),
    native_executable_sha256: 'b'.repeat(64), files: inventory.enumerate(root) };
  const bytes = JSON.stringify(inv); fs.writeFileSync(path.join(root, 'component-inventory.json'), bytes);
  notices.verifyStaged(root, hash(bytes), inv.workspace_lock_sha256, inv.native_executable_sha256);
  assert.throws(() => notices.verifyStaged(root, hash(bytes), 'c'.repeat(64), inv.native_executable_sha256), /Unbound/);
  fs.writeFileSync(path.join(root, 'licenses/notice.txt'), 'changed');
  assert.throws(() => notices.verifyStaged(root, hash(bytes), inv.workspace_lock_sha256, inv.native_executable_sha256), /differs/);
});
