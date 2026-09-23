// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

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
    for (const root of [output, path.join(output, 'node_modules/@vcp/sdk'), path.join(output, 'node_modules/@vcp/protocol')]) assert.ok(fs.existsSync(path.join(root, 'NOTICE')));
    const probe = path.join(output, 'package-probe.cjs');
    fs.writeFileSync(probe, "import('@vcp/sdk').then(sdk=>{if(typeof sdk.launchLocal!=='function'||typeof sdk.newCommandId!=='function')process.exit(2)}).catch(()=>process.exit(1));\n");
    const imported = spawnSync(process.execPath, [probe], { cwd: output, encoding: 'utf8', timeout: 15000, windowsHide: true });
    assert.equal(imported.status, 0, imported.stderr);
  } finally {
    // Exact test-owned path created above; never delete a computed parent.
    if (fs.existsSync(path.join(output, '.vcp-stage.json'))) fs.rmSync(output, { recursive: true });
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
