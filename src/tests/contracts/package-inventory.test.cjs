// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const inventory = require('../../../scripts/package-inventory.cjs');

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-package-'));
  fs.mkdirSync(path.join(root, 'runtime'));
  fs.writeFileSync(path.join(root, 'vcp.exe'), Buffer.from('candidate executable'));
  fs.writeFileSync(path.join(root, 'runtime', 'worker.dll'), Buffer.from('selected runtime'));
  return root;
}

test('manifest records only ordinary payload bytes and verifies exact hashes', () => {
  const root = fixture();
  const manifest = inventory.buildManifest(root, { compatibility: { cli: 'test-cli' } });
  assert.deepEqual(manifest.files.map(file => file.path), ['runtime/worker.dll', 'vcp.exe']);
  assert.equal(inventory.verifyManifest(root, manifest).status, 'passed');
  fs.appendFileSync(path.join(root, 'vcp.exe'), 'changed');
  assert.throws(() => inventory.verifyManifest(root, manifest), /differs from manifest/);
});

test('portable paths reject traversal, Windows separators and device names', () => {
  for (const value of ['../secret', 'runtime\\worker.dll', 'con.txt', 'folder/..', 'x:bad']) {
    assert.throws(() => inventory.portable(value), /Invalid portable/);
  }
});

test('bundled model records are rejected while digest-only provisioning is retained', () => {
  const root = fixture();
  const manifest = inventory.buildManifest(root, { model_provisioning: { bundled: false, records: [{ id: 'minilm', digest: 'a'.repeat(64) }] } });
  assert.equal(inventory.verifyManifest(root, manifest).status, 'passed');
  manifest.model_provisioning.bundled = true;
  assert.throws(() => inventory.verifyManifest(root, manifest), /Bundled model/);
});

test('inventory rejects the first payload beyond the installer file-count limit', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-package-count-'));
  try {
    for (let index = 0; index < 4096; index++) {
      fs.writeFileSync(path.join(root, String(index).padStart(4, '0')), '');
    }
    assert.equal(inventory.enumerate(root).length, 4096);
    fs.writeFileSync(path.join(root, '4096'), '');
    assert.throws(() => inventory.enumerate(root), /file count limit exceeded/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
