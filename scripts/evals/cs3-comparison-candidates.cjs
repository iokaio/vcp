// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const { portable } = require('../skills/builtin-assets.cjs');
const root = path.resolve(__dirname, '../../src/evals/skills/cs3-comparison/candidates');
const ids = ['document-authoring', 'skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
const versions = ['1.0.4', '1.0.2', '1.0.0', '1.0.1', '1.0.0', '1.0.1'];
const sourceId = 'vcp-cs3-comparison-candidates';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const qualified = id => { if (!ids.includes(id)) throw Error('Unknown CS-3 candidate'); return `${sourceId}::.::${id}`; };
function inspect() {
  const files = [], entries = [];
  for (const [index, id] of ids.entries()) {
    const directory = plain(path.join(root, id)), descriptorBytes = read(path.join(directory, 'skill.json')), descriptor = JSON.parse(descriptorBytes);
    if (descriptor.schema_version !== 1 || descriptor.id !== id || descriptor.version !== versions[index] || !Array.isArray(descriptor.resources)) throw Error('Invalid prospective candidate');
    const parts = [descriptor.body, ...descriptor.resources], expected = ['skill.json'];
    files.push({ path: `${id}/skill.json`, sha256: sha(descriptorBytes), bytes: descriptorBytes.length });
    for (const part of parts) {
      portable(part.path);
      if (expected.includes(part.path)) throw Error('Duplicate candidate part');
      expected.push(part.path);
      const bytes = read(plain(path.join(directory, part.path)), 1024 * 1024);
      if (sha(bytes) !== part.sha256) throw Error('Candidate part changed');
      files.push({ path: `${id}/${part.path}`, sha256: sha(bytes), bytes: bytes.length });
    }
    const actual = [];
    function visit(at = '', depth = 0) {
      if (depth > 8 || actual.length > 128) throw Error('Candidate inventory bound exceeded');
      for (const name of fs.readdirSync(plain(path.join(directory, at))).sort()) {
        const relative = at ? `${at}/${name}` : name, file = plain(path.join(directory, relative));
        if (fs.lstatSync(file).isDirectory()) visit(relative, depth + 1); else actual.push(relative);
      }
    }
    visit();
    if (JSON.stringify(actual.sort()) !== JSON.stringify(expected.sort())) throw Error('Unexpected candidate files');
    entries.push({ id, version: descriptor.version, qualified_id: qualified(id), descriptor_sha256: sha(descriptorBytes), parts });
  }
  return { source_id: sourceId, path: root, files, entries };
}
function configuration(id) {
  qualified(id); inspect();
  return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: '5c02c0c4-380c-4d85-a7fc-35fcfd2295ef', kind: 'user', enabled: true, path: path.join(root, id) }], disabled: [] };
}
module.exports = { root, ids, versions, qualified, configuration, inspect };
