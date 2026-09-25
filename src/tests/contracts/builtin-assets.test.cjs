// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { ownedRoot } = require('../support/experiments.cjs');
const { inspectAssets, stageAssets, portable } = require('../../../scripts/skills/builtin-assets.cjs');
const assets = path.resolve(__dirname, '../../skills/builtin');
test('builtin staging copies exactly the selected hashed inventory and preserves an existing destination', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    const target = path.join(temp.root, 'assets');
    const inventory = stageAssets(assets, target);
    assert.equal(inventory.skills, 21);
    assert.equal(inventory.files.length, 44);
    assert.deepEqual(inspectAssets(target).inventory, inventory);
    assert.equal(fs.existsSync(path.join(target, 'document-authoring')), false);
    assert.equal(inventory.files.some(file => JSON.stringify(file).includes('document-authoring')), false);
    fs.writeFileSync(path.join(target, 'user-sentinel'), 'preserve');
    assert.throws(() => stageAssets(assets, target), /exist/i);
    assert.equal(fs.readFileSync(path.join(target, 'user-sentinel'), 'utf8'), 'preserve');
  } finally { temp.cleanup(); }
});
test('packaging rejects missing, changed and additional content instead of creating a broad archive', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    for (const mode of ['missing', 'changed', 'extra', 'catalog']) {
      const target = path.join(temp.root, mode);
      stageAssets(assets, target);
      if (mode === 'missing') fs.unlinkSync(path.join(target, 'rust/SKILL.md'));
      if (mode === 'changed') fs.appendFileSync(path.join(target, 'rust/SKILL.md'), 'tamper');
      if (mode === 'extra') fs.writeFileSync(path.join(target, 'unlisted-secret'), 'synthetic-not-for-package');
      if (mode === 'catalog') fs.appendFileSync(path.join(target, 'catalog.json'), '\n');
      assert.throws(() => inspectAssets(target), /Missing|unexpected|hash mismatch|Catalog does not match/);
    }
  } finally { temp.cleanup(); }
});
test('linked asset directories are rejected without traversing or deleting their target', () => {
  const temp = ownedRoot(os.tmpdir()), outside = ownedRoot(os.tmpdir());
  try {
    const target = path.join(temp.root, 'assets');
    stageAssets(assets, target);
    fs.writeFileSync(path.join(outside.root, 'sentinel'), 'preserve outside');
    fs.symlinkSync(outside.root, path.join(target, 'alias'), process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => inspectAssets(target), /Linked asset/);
    assert.equal(fs.readFileSync(path.join(outside.root, 'sentinel'), 'utf8'), 'preserve outside');
  } finally { temp.cleanup(); outside.cleanup(); }
});
test('archive asset paths reject traversal, aliases, rooted paths and Windows device names', () => {
  for (const relative of ['../escape', '/root', 'C:/root', 'a\\b', 'a//b', 'a/./b', 'NUL.txt', 'a/COM1', 'a/trailing.', 'a/trailing ']) {
    assert.throws(() => portable(relative), /portable asset path/);
  }
  assert.equal(portable('rust/SKILL.md'), 'rust/SKILL.md');
});
