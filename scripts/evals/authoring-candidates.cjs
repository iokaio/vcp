// SPDX-License-Identifier: Apache-2.0
'use strict';
// Explicit evaluation sources; never installed or discovered as builtin skills.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { portable } = require('../skills/builtin-assets.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const repository = path.resolve(__dirname, '../..');
const root = path.join(repository, 'src/skills/candidates');
const ids = ['document-authoring', 'skill-authoring'];
// Prospective package identities only. Historical runs retain their original
// frozen validator and assets; a new version never inherits qualification.
const versions = Object.freeze({ 'document-authoring': '1.0.2', 'skill-authoring': '1.0.1' });
const sourceId = 'vcp-authoring-candidates';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function qualified(id) {
  if (!ids.includes(id)) throw Error('Unknown authoring candidate');
  return `${sourceId}::.::${id}`;
}
function inspect() {
  const files = [], entries = [];
  for (const id of ids) {
    const directory = plain(path.join(root, id)), bytes = read(path.join(directory, 'skill.json'), 1024 * 1024);
    const descriptor = JSON.parse(bytes);
    if (descriptor.schema_version !== 1 || descriptor.id !== id || descriptor.version !== versions[id] || !Array.isArray(descriptor.resources) || descriptor.resources.length > 32) throw Error('Invalid authoring candidate descriptor');
    const parts = [descriptor.body, ...descriptor.resources];
    const wanted = new Set(['skill.json']);
    files.push({ path: `${id}/skill.json`, bytes: bytes.length, sha256: sha(bytes) });
    for (const part of parts) {
      const relative = portable(part.path);
      if (wanted.has(relative) || !/^[a-f0-9]{64}$/.test(part.sha256)) throw Error('Invalid candidate content identity');
      wanted.add(relative);
      const content = read(plain(path.join(directory, relative)), 1024 * 1024);
      if (sha(content) !== part.sha256) throw Error('Authoring candidate content hash differs');
      files.push({ path: `${id}/${relative}`, bytes: content.length, sha256: sha(content) });
    }
    const actual = [];
    let nodes = 0;
    function visit(at, depth = 0) {
      if (depth > 8 || ++nodes > 128) throw Error('Candidate inventory bound exceeded');
      for (const name of fs.readdirSync(plain(path.join(directory, at))).sort()) {
        const relative = at ? `${at}/${name}` : name;
        if (++nodes > 128) throw Error('Candidate inventory bound exceeded');
        portable(relative);
        const file = plain(path.join(directory, relative));
        if (fs.lstatSync(file).isDirectory()) visit(relative, depth + 1);
        else actual.push(relative);
      }
    }
    visit('');
    if (JSON.stringify(actual.sort()) !== JSON.stringify([...wanted].sort())) throw Error('Unexpected authoring candidate files');
    entries.push({ id, qualified_id: qualified(id), descriptor_sha256: sha(bytes), parts });
  }
  files.sort((a, b) => a.path.localeCompare(b.path));
  return { source_id: sourceId, path: root, files, entries };
}
function configuration(id) {
  qualified(id);
  inspect();
  return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: '924e3aba-9b72-45b7-9125-17a0fa7617f1', kind: 'user', enabled: true, path: path.join(root, id) }], disabled: [] };
}
function selection(arm, task) {
  return arm === 'none' ? null : arm === 'candidate' ? qualified(task.skill) : `vcp-builtin::${task.nearest_skill}::${task.nearest_skill}`;
}
module.exports = { inspect, configuration, selection, qualified, ids };
