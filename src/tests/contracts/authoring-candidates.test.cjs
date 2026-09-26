// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const { ownedRoot } = require('../support/experiments.cjs');
const candidates = require('../../../scripts/evals/authoring-candidates.cjs');
const repository = path.resolve(__dirname, '../../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const expected = { 'document-authoring': '1.0.2', 'skill-authoring': '1.0.1' };

function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = path.join(owner.root, 'src/skills/candidates');
  for (const id of candidates.ids) fs.cpSync(path.join(repository, 'src/skills/candidates', id), path.join(root, id), { recursive: true });
  const file = path.join(repository, 'scripts/evals/authoring-candidates.cjs'), module = { exports: {} };
  // Resolve package files under the private fixture; imports still use the real
  // validator dependencies. No production guard or active candidate is changed.
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(createRequire(file), module, module.exports, path.join(owner.root, 'scripts/evals'));
  return { root, api: module.exports };
}

test('prospective DOC and unchanged SKL bind distinct exact versions and content hashes', () => {
  const inventory = candidates.inspect();
  assert.equal(inventory.source_id, 'vcp-authoring-candidates');
  for (const [id, version] of Object.entries(expected)) {
    const bytes = fs.readFileSync(path.join(repository, 'src/skills/candidates', id, 'skill.json')), descriptor = JSON.parse(bytes);
    const entry = inventory.entries.find(entry => entry.id === id);
    assert.equal(descriptor.version, version); assert.equal(entry.descriptor_sha256, sha(bytes));
    assert.deepEqual(entry.parts, [descriptor.body, ...descriptor.resources]);
    const configured = candidates.configuration(id);
    assert.deepEqual(configured.sources.map(source => source.kind), ['user']);
    assert.equal(configured.sources[0].path, path.join(inventory.path, id));
    assert.equal(candidates.qualified(id), `vcp-authoring-candidates::.::${id}`);
  }
});

test('candidate version contract rejects stale DOC and unsupported versions without loosening SKL', t => {
  const { root, api } = fixture(t);
  assert.equal(api.inspect().entries.length, 2);
  for (const [id, version] of [['document-authoring', '1.0.1'], ['document-authoring', '2.0.0'], ['skill-authoring', '1.0.2']]) {
    const file = path.join(root, id, 'skill.json'), bytes = fs.readFileSync(file), descriptor = JSON.parse(bytes);
    descriptor.version = version; fs.writeFileSync(file, JSON.stringify(descriptor));
    assert.throws(() => api.inspect(), /Invalid authoring candidate descriptor/);
    fs.writeFileSync(file, bytes);
  }
  assert.equal(api.inspect().entries.length, 2);
});

for (const [id, relative] of [['document-authoring', 'SKILL.md'], ['skill-authoring', 'references/package-format.md']]) {
  test(`changed ${id}/${relative} cannot reuse its descriptor digest`, t => {
    const { root, api } = fixture(t);
    fs.appendFileSync(path.join(root, id, relative), '\nchanged');
    assert.throws(() => api.inspect(), /content hash differs/);
    assert.throws(() => api.configuration(id), /content hash differs/);
  });
}

test('a prospective version does not allow undeclared package files', t => {
  const { root, api } = fixture(t);
  fs.writeFileSync(path.join(root, 'document-authoring', 'undeclared.md'), 'Not a declared resource.');
  assert.throws(() => api.inspect(), /Unexpected authoring candidate files/);
});
