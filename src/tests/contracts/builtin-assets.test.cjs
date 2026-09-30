// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { inspectAssets, stageAssets, rehashAssets, portable } = require('../../../scripts/skills/builtin-assets.cjs');
const assets = path.resolve(__dirname, '../../skills/builtin');
const baselineIds = ['architecture', 'review-debug', 'testing', 'git-workflow', 'javascript-typescript',
  'python', 'rust', 'dotnet-powershell', 'jvm', 'go', 'cpp', 'ruby', 'php', 'swift', 'dart', 'shell',
  'sql', 'data', 'infrastructure', 'project-optimize', 'memory-hygiene'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
test('builtin staging copies exactly the selected hashed inventory and preserves an existing destination', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    const target = path.join(temp.root, 'assets');
    const catalogBytes = fs.readFileSync(path.join(assets, 'catalog.json')), catalog = JSON.parse(catalogBytes);
    const ids = catalog.skills.map(skill => skill.id);
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids.filter(id => baselineIds.includes(id)).sort(), [...baselineIds].sort());
    const expected = [{ path: 'catalog.json', sha256: sha(catalogBytes) }, catalog.coverage];
    for (const skill of catalog.skills) {
      const descriptorBytes = fs.readFileSync(path.join(assets, skill.descriptor)), descriptor = JSON.parse(descriptorBytes);
      assert.equal(sha(descriptorBytes), skill.descriptor_sha256);
      assert.equal(descriptor.id, skill.id); assert.equal(descriptor.version, skill.version);
      assert.deepEqual(descriptor.body, skill.body); assert.deepEqual(descriptor.resources, skill.resources);
      expected.push({ path: skill.descriptor, sha256: skill.descriptor_sha256 },
        ...[descriptor.body, ...descriptor.resources].map(item => ({ path: `${skill.id}/${item.path}`, sha256: item.sha256 })));
    }
    assert.equal(new Set(expected.map(item => item.path)).size, expected.length);
    const inventory = stageAssets(assets, target);
    assert.equal(inventory.skills, catalog.skills.length);
    assert.equal(inventory.files.length, expected.length);
    assert.deepEqual(inventory.files.map(item => item.path), expected.map(item => item.path).sort());
    for (const item of inventory.files) {
      const selected = expected.find(row => row.path === item.path), bytes = fs.readFileSync(path.join(assets, item.path));
      assert.equal(item.sha256, selected.sha256); assert.equal(sha(bytes), selected.sha256);
      assert.equal(item.bytes, bytes.length); assert.deepEqual(fs.readFileSync(path.join(target, item.path)), bytes);
    }
    assert.deepEqual(inspectAssets(target).inventory, inventory);
    for (const id of ['document-authoring', 'skill-authoring']) {
      assert.equal(fs.existsSync(path.join(target, id)), ids.includes(id));
      assert.equal(inventory.files.some(file => file.path.startsWith(id + '/')), ids.includes(id));
    }
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
test('resource roles must match between catalog and descriptor, and bodies stay context', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    for (const mode of ['resource', 'body', 'unknown']) {
      const target = path.join(temp.root, mode);
      stageAssets(assets, target);
      const catalog = JSON.parse(fs.readFileSync(path.join(target, 'catalog.json')));
      const entry = catalog.skills.find(skill => skill.resources.length);
      if (mode === 'resource') entry.resources[0].use = entry.resources[0].use === 'file' ? 'context' : 'file';
      if (mode === 'body') entry.body.use = 'file';
      if (mode === 'unknown') entry.resources[0].use = 'execute';
      const bytes = Buffer.from(JSON.stringify(catalog));
      fs.writeFileSync(path.join(target, 'catalog.json'), bytes);
      assert.throws(() => inspectAssets(target, bytes), /Descriptor differs|body must be context|Invalid builtin resource role/);
    }
  } finally { temp.cleanup(); }
});
test('rehash is idempotent and refreshes only edited content, descriptor and catalog digests', () => {
  const temp = ownedRoot(os.tmpdir());
  try {
    const target = path.join(temp.root, 'assets');
    stageAssets(assets, target);
    assert.deepEqual(rehashAssets(target).changed, []);
    fs.appendFileSync(path.join(target, 'rust/SKILL.md'), 'Edited guidance.\n');
    assert.throws(() => inspectAssets(target, fs.readFileSync(path.join(target, 'catalog.json'))), /hash mismatch/);
    const result = rehashAssets(target);
    assert.deepEqual(result.changed.sort(), ['catalog.json', 'rust/skill.json']);
    const descriptor = JSON.parse(fs.readFileSync(path.join(target, 'rust/skill.json')));
    assert.equal(descriptor.body.sha256, sha(fs.readFileSync(path.join(target, 'rust/SKILL.md'))));
    assert.equal(result.inventory.skills, JSON.parse(fs.readFileSync(path.join(assets, 'catalog.json'))).skills.length);
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
