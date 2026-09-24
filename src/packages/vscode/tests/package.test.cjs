// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { inventory, sha256 } = require('../scripts/inventory.cjs');

test('staged extension contains a standalone SDK and schema without runtime file links', () => {
  const packageRoot = path.resolve(__dirname, '..');
  const repository = path.resolve(packageRoot, '../../..');
  const output = path.join(repository, 'artifacts', `p4-package-test-${process.pid}`);
  try {
    const staged = spawnSync(process.execPath, [path.join(packageRoot, 'scripts/stage.cjs'), output], { encoding: 'utf8', timeout: 30000, windowsHide: true });
    assert.equal(staged.status, 0, staged.stderr);
    assert.equal(fs.lstatSync(path.join(output, 'node_modules/@vcp/sdk')).isSymbolicLink(), false);
    const manifest = JSON.parse(fs.readFileSync(path.join(output, 'package.json')));
    assert.equal(manifest.main, './dist/extension.js');
    assert.equal(manifest.capabilities.untrustedWorkspaces.supported, 'limited');
    assert.equal(manifest.dependencies['@vcp/sdk'], '0.1.0');
    assert.equal(manifest.devDependencies, undefined);
    assert.ok(!fs.existsSync(path.join(output, 'src')));
    assert.ok(fs.existsSync(path.join(output, 'node_modules/@vcp/protocol/schema.json')));
    for (const root of [output, path.join(output, 'node_modules/@vcp/sdk'), path.join(output, 'node_modules/@vcp/protocol')]) {
      for (const notice of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) assert.ok(fs.existsSync(path.join(root, notice)));
    }
    const probe = path.join(output, 'package-probe.cjs');
    fs.writeFileSync(probe, "import('@vcp/sdk').then(sdk=>{if(typeof sdk.launchLocal!=='function'||typeof sdk.newCommandId!=='function')process.exit(2)}).catch(()=>process.exit(1));\n");
    const imported = spawnSync(process.execPath, [probe], { cwd: output, encoding: 'utf8', timeout: 15000, windowsHide: true });
    assert.equal(imported.status, 0, imported.stderr);
  } finally {
    // Exact test-owned path created above; never delete a computed parent.
    if (fs.existsSync(path.join(output, '.vcp-stage.json'))) fs.rmSync(output, { recursive: true });
  }
});

test('official VSIX preserves exact standalone inventory and requires matching engine provenance', async () => {
  const packageRoot = path.resolve(__dirname, '..');
  const artifacts = path.resolve(packageRoot, '../../../artifacts');
  const fixture = fs.mkdtempSync(path.join(artifacts, 'p4-vsix-test-'));
  const output = path.join(fixture, 'candidate');
  const engine = path.join(fixture, 'vcp.exe');
  const native = path.join(fixture, 'native.json');
  const stale = path.join(packageRoot, 'dist', 'stale-package-fixture.js');
  const engineBytes = Buffer.from('test provenance input, not a native engine');
  fs.writeFileSync(engine, engineBytes);
  fs.writeFileSync(stale, 'SECRET_FIXTURE_MUST_NOT_SHIP');
  const metadata = { schema: 'vcp-distribution-result/1', archive_sha256: 'b'.repeat(64), manifest: { source: { git_commit: 'a'.repeat(40), dirty: true }, build: { status: 'caller-supplied-unverified', executable_sha256: sha256(engineBytes) }, files: [{ path: 'vcp.exe', sha256: sha256(engineBytes) }] } };
  fs.writeFileSync(native, JSON.stringify(metadata));
  const run = () => spawnSync(process.execPath, [path.join(packageRoot, 'scripts/package.cjs'), '--engine', engine, '--engine-manifest', native, '--output', output], { encoding: 'utf8', timeout: 60000, windowsHide: true });
  try {
    const packaged = run();
    assert.equal(packaged.status, 0, packaged.stderr + packaged.stdout);
    const manifest = JSON.parse(fs.readFileSync(path.join(output, 'manifest.json')));
    const archive = path.join(output, manifest.archive.file);
    assert.equal(manifest.engine.executable_sha256, sha256(engineBytes));
    assert.equal(manifest.engine.build_status, 'caller-supplied-unverified');
    assert.equal(manifest.archive.sha256, sha256(fs.readFileSync(archive)));
    assert.deepEqual(manifest.files, await inventory(archive));
    const names = manifest.files.map(row => row.path);
    assert.ok(names.includes('extension/dist/extension.js'));
    assert.ok(names.includes('extension/node_modules/@vcp/sdk/dist/index.js'));
    assert.ok(names.includes('extension/node_modules/@vcp/protocol/schema.json'));
    assert.ok(names.includes('extension/COMPATIBILITY.md'));
    for (const prefix of ['extension', 'extension/node_modules/@vcp/sdk', 'extension/node_modules/@vcp/protocol']) {
      for (const notice of ['NOTICE', 'THIRD_PARTY_NOTICES.md']) assert.ok(names.includes(`${prefix}/${notice}`));
      assert.ok(names.includes(`${prefix}/${prefix === 'extension' ? 'LICENSE.txt' : 'LICENSE'}`));
    }
    assert.ok(!names.some(name => /stale-package|\.map$|\.d\.ts$|\.exe$|\/tests\/|\/src\/|\.vcp-stage|vsce|yauzl/.test(name)));
    const original = fs.readFileSync(archive);
    metadata.manifest.build.executable_sha256 = '0'.repeat(64);
    fs.writeFileSync(native, JSON.stringify(metadata));
    assert.notEqual(run().status, 0);
    assert.deepEqual(fs.readFileSync(archive), original, 'invalid provenance must not replace candidate');
    const receipt = Buffer.from(JSON.stringify({ schema: 'vcp-local-build/1', exit_code: 0, executable_sha256: sha256(engineBytes) }));
    fs.mkdirSync(path.join(fixture, 'package'));
    fs.writeFileSync(path.join(fixture, 'package/build-receipt.json'), receipt);
    metadata.manifest.build = { status: 'recorded-local-build', receipt: 'build-receipt.json', receipt_sha256: sha256(receipt) };
    metadata.manifest.files.push({ path: 'build-receipt.json', sha256: sha256(receipt) });
    fs.writeFileSync(native, JSON.stringify(metadata));
    assert.equal(run().status, 0, 'recorded native build has a distinct valid metadata shape');
    assert.equal(JSON.parse(fs.readFileSync(path.join(output, 'manifest.json'))).engine.build_receipt_sha256, sha256(receipt));
  } finally {
    fs.unlinkSync(stale);
    fs.rmSync(fixture, { recursive: true });
  }
});

test('stager preserves unrecognized directories and rejects redirected ancestors', () => {
  const packageRoot = path.resolve(__dirname, '..');
  const artifacts = path.resolve(packageRoot, '../../../artifacts');
  const comparable = value => process.platform === 'win32' ? path.normalize(value).toLowerCase() : path.normalize(value);
  assert.equal(comparable(fs.realpathSync(path.dirname(artifacts))), comparable(path.dirname(artifacts)));
  if (!fs.existsSync(artifacts)) fs.mkdirSync(artifacts);
  assert.equal(comparable(fs.realpathSync(artifacts)), comparable(artifacts));
  const fixture = fs.mkdtempSync(path.join(artifacts, 'p4-stage-guard-'));
  const target = path.join(fixture, 'valuable');
  const link = path.join(fixture, 'redirect');
  fs.mkdirSync(target);
  fs.writeFileSync(path.join(target, 'keep.txt'), 'preserve');
  try {
    const run = output => spawnSync(process.execPath, [path.join(packageRoot, 'scripts/stage.cjs'), output], { encoding: 'utf8', timeout: 10000, windowsHide: true });
    assert.notEqual(run(target).status, 0);
    assert.equal(fs.readFileSync(path.join(target, 'keep.txt'), 'utf8'), 'preserve');
    fs.writeFileSync(path.join(target, '.vcp-stage.json'), JSON.stringify({ format: 'vcp-extension-stage/1' }));
    fs.symlinkSync(target, link, process.platform === 'win32' ? 'junction' : 'dir');
    assert.notEqual(run(link).status, 0);
    assert.notEqual(run(path.join(link, 'nested')).status, 0);
    assert.equal(fs.readFileSync(path.join(target, 'keep.txt'), 'utf8'), 'preserve');
  } finally {
    if (fs.existsSync(link)) fs.unlinkSync(link);
    // The exact freshly-created test directory is local and no redirect remains.
    fs.rmSync(fixture, { recursive: true });
  }
});
