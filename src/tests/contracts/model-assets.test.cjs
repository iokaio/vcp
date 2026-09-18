// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { specification, outside, verify, acquire } = require('../support/model-assets.cjs');
const repository = path.resolve(__dirname, '../../..');

test('model acquisition rejects source containment, aliases, existing roots and invalid CLI modes', async () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const source = path.join(fixture.root, 'source'); fs.mkdirSync(source);
    const alias = path.join(fixture.root, 'alias'); fs.symlinkSync(source, alias, process.platform === 'win32' ? 'junction' : 'dir');
    try { assert.throws(() => outside(path.join(alias, 'model'), [source]), /outside/); }
    finally { fs.unlinkSync(alias); }
    assert.throws(() => outside(path.join(source, 'new/model'), [source]), /outside/);
    await assert.rejects(acquire(source, {}), /new destination/);
    assert.deepEqual(fs.readdirSync(source), []);
    const result = spawnSync(process.execPath, [path.join(repository, 'scripts/upstream/model-assets.cjs'), 'unknown', '--root', path.join(fixture.root, 'never-created')], { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 2);
    assert.equal(fs.existsSync(path.join(fixture.root, 'never-created')), false);
    assert.equal(specification(repository).spec.files.length, 10);
  } finally { fixture.cleanup(); }
});

test('model acquisition publishes verified bytes and quarantines truncated or corrupt downloads', async () => {
  const fixture = ownedRoot(os.tmpdir()), original = global.fetch;
  const good = Buffer.from('synthetic model bytes');
  const spec = { repository: 'https://example.invalid/model', revision: 'a'.repeat(40), files: [{ path: 'nested/weights', bytes: good.length, sha256: crypto.createHash('sha256').update(good).digest('hex') }] };
  try {
    for (const [name, bytes, passes] of [['good', good, true], ['short', good.subarray(0, 3), false], ['wrong', Buffer.alloc(good.length), false]]) {
      global.fetch = async (url, options) => { assert.equal(url, spec.repository + '/resolve/' + spec.revision + '/nested/weights'); assert.equal(Object.hasOwn(options.headers, 'Authorization'), false); return new Response(bytes); };
      const root = path.join(fixture.root, name);
      if (passes) {
        assert.deepEqual(await acquire(root, spec), { files: 1, bytes: good.length });
        assert.deepEqual(fs.readFileSync(path.join(root, 'nested/weights')), good);
        assert.equal(JSON.parse(fs.readFileSync(path.join(root, '.vcp-acquisition.json'))).status, 'verified');
      } else {
        await assert.rejects(acquire(root, spec), /digest mismatch/);
        assert.equal(fs.existsSync(path.join(root, 'nested/weights')), false);
        assert.equal(JSON.parse(fs.readFileSync(path.join(root, '.vcp-acquisition.json'))).status, 'failed');
      }
    }
  } finally { global.fetch = original; fixture.cleanup(); }
});

test('verification detects changed local bytes and missing assets remain not_run', async () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const file = { path: 'asset', bytes: 3, sha256: crypto.createHash('sha256').update('abc').digest('hex') };
    fs.writeFileSync(path.join(fixture.root, 'asset'), 'abd');
    await assert.rejects(verify(fixture.root, { files: [file] }), /digest mismatch/);
    fs.writeFileSync(path.join(fixture.root, 'asset'), 'too long');
    await assert.rejects(verify(fixture.root, { files: [file] }), /size mismatch/);
    const result = spawnSync(process.execPath, [path.join(repository, 'scripts/upstream/model-assets.cjs'), 'verify', '--root', path.join(fixture.root, 'absent')], { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 3);
    assert.equal(JSON.parse(result.stderr).status, 'not_run');
  } finally { fixture.cleanup(); }
});

test('native wrapper rejects evidence inside model assets before creating output', t => {
  if (process.platform !== 'win32') return t.skip('Requires the native Windows wrapper');
  const fixture = ownedRoot(os.tmpdir());
  try {
    const output = path.join(fixture.root, 'must-not-exist');
    const result = spawnSync('pwsh', ['-NoProfile', '-File', path.join(repository, 'scripts/test-embeddings.ps1'),
      '-AssetsRoot', fixture.root, '-OutputRoot', output], { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 2, result.stdout + result.stderr);
    assert.equal(fs.existsSync(output), false);
  } finally { fixture.cleanup(); }
});
