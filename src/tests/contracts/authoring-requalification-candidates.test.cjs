// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const { ownedRoot } = require('../support/experiments.cjs');
const candidates = require('../../../scripts/evals/authoring-requalification-candidates.cjs');
const repository = path.resolve(__dirname, '../../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = path.join(owner.root, 'src/evals/skills/authoring-requalification/candidates');
  fs.cpSync(path.join(repository, 'src/evals/skills/authoring-requalification/candidates'), root, { recursive: true });
  const file = path.join(repository, 'scripts/evals/authoring-requalification-candidates.cjs'), module = { exports: {} };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(createRequire(file), module, module.exports, path.join(owner.root, 'scripts/evals'));
  return { root, api: module.exports };
}
test('new authoring revisions are isolated, exact and explicitly selected', () => {
  const inventory = candidates.inspect();
  assert.equal(inventory.source_id, 'vcp-authoring-requalification-candidates');
  for (const [id, version] of Object.entries(candidates.versions)) {
    const bytes = fs.readFileSync(path.join(inventory.path, id, 'skill.json')), descriptor = JSON.parse(bytes), entry = inventory.entries.find(item => item.id === id);
    assert.equal(descriptor.version, version); assert.equal(entry.descriptor_sha256, sha(bytes)); assert.deepEqual(entry.parts, [descriptor.body, ...descriptor.resources]);
    assert.equal(candidates.configuration(id).sources[0].kind, 'user'); assert.equal(candidates.qualified(id), `vcp-authoring-requalification-candidates::.::${id}`);
  }
});
test('prior versions and changed bytes cannot enter the new source', t => {
  for (const [id, version] of [['document-authoring', '1.0.2'], ['skill-authoring', '1.0.1']]) {
    const { root, api } = fixture(t), file = path.join(root, id, 'skill.json'), descriptor = JSON.parse(fs.readFileSync(file));
    descriptor.version = version; fs.writeFileSync(file, JSON.stringify(descriptor)); assert.throws(() => api.inspect(), /Invalid requalification candidate descriptor/);
  }
  { const { root, api } = fixture(t); fs.appendFileSync(path.join(root, 'document-authoring', 'SKILL.md'), '\nchanged'); assert.throws(() => api.inspect(), /content hash differs/); }
  { const { root, api } = fixture(t); fs.writeFileSync(path.join(root, 'skill-authoring', 'extra.md'), 'undeclared'); assert.throws(() => api.inspect(), /Unexpected requalification candidate files/); }
});
