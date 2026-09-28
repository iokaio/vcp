// SPDX-License-Identifier: Apache-2.0
'use strict';
// Prospective package identities for a new campaign; never replace historical candidates.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { portable } = require('../skills/builtin-assets.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const repository = path.resolve(__dirname, '../..');
const root = path.join(repository, 'src/evals/skills/authoring-requalification/candidates');
const ids = ['document-authoring', 'skill-authoring'];
const versions = Object.freeze({ 'document-authoring': '1.0.3', 'skill-authoring': '1.0.2' });
const sourceId = 'vcp-authoring-requalification-candidates';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function qualified(id) {
  if (!ids.includes(id)) throw Error('Unknown requalification candidate');
  return `${sourceId}::.::${id}`;
}
function inspect() {
  const files = [], entries = [];
  for (const id of ids) {
    const directory = plain(path.join(root, id)), bytes = read(path.join(directory, 'skill.json'), 1024 * 1024), descriptor = JSON.parse(bytes);
    if (descriptor.schema_version !== 1 || descriptor.id !== id || descriptor.version !== versions[id] || !Array.isArray(descriptor.resources) || descriptor.resources.length > 32) throw Error('Invalid requalification candidate descriptor');
    const parts = [descriptor.body, ...descriptor.resources], wanted = new Set(['skill.json']);
    files.push({ path: `${id}/skill.json`, bytes: bytes.length, sha256: sha(bytes) });
    for (const part of parts) {
      const relative = portable(part.path);
      if (wanted.has(relative) || !/^[a-f0-9]{64}$/.test(part.sha256)) throw Error('Invalid requalification content identity');
      wanted.add(relative);
      const content = read(plain(path.join(directory, relative)), 1024 * 1024);
      if (sha(content) !== part.sha256) throw Error('Requalification candidate content hash differs');
      files.push({ path: `${id}/${relative}`, bytes: content.length, sha256: sha(content) });
    }
    const actual = [];
    function visit(at, depth = 0) {
      if (depth > 8 || actual.length > 128) throw Error('Requalification inventory bound exceeded');
      for (const name of fs.readdirSync(plain(path.join(directory, at))).sort()) {
        const relative = at ? `${at}/${name}` : name, file = plain(path.join(directory, relative));
        portable(relative);
        if (fs.lstatSync(file).isDirectory()) visit(relative, depth + 1); else actual.push(relative);
      }
    }
    visit('');
    if (JSON.stringify(actual.sort()) !== JSON.stringify([...wanted].sort())) throw Error('Unexpected requalification candidate files');
    entries.push({ id, qualified_id: qualified(id), descriptor_sha256: sha(bytes), parts });
  }
  files.sort((a, b) => a.path.localeCompare(b.path));
  return { source_id: sourceId, path: root, files, entries };
}
function configuration(id) {
  qualified(id); inspect();
  return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: 'aa3c15aa-f4bd-41ac-8ff6-e39d9517af86', kind: 'user', enabled: true, path: path.join(root, id) }], disabled: [] };
}
function selection(arm, task) { return arm === 'none' ? null : arm === 'candidate' ? qualified(task.skill) : `vcp-builtin::${task.nearest_skill}::${task.nearest_skill}`; }
module.exports = { inspect, configuration, selection, qualified, ids, versions };
