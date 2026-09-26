// SPDX-License-Identifier: Apache-2.0
'use strict';
// Explicit CS-2 evaluation sources; never installed or discovered as builtin skills.
// Adapted from authoring-candidates.cjs, which stays byte-identical as CS-1 identity.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { portable } = require('../skills/builtin-assets.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const repository = path.resolve(__dirname, '../..');
const root = path.join(repository, 'src/skills/candidates');
// Campaign order: most deterministic evidence first, browser-dependent evidence last.
const ids = ['llm-integration', 'mcp-development', 'frontend-design'];
const version = '1.0.0';
const sourceId = 'vcp-developer-candidates';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function qualified(id) {
  if (!ids.includes(id)) throw Error('Unknown developer candidate');
  return `${sourceId}::.::${id}`;
}
function inspect() {
  const files = [], entries = [];
  for (const id of ids) {
    const directory = plain(path.join(root, id)), bytes = read(path.join(directory, 'skill.json'), 1024 * 1024);
    const descriptor = JSON.parse(bytes);
    // Project-local references only: activation would load every declared resource.
    if (descriptor.schema_version !== 1 || descriptor.id !== id || descriptor.version !== version || !Array.isArray(descriptor.resources) || descriptor.resources.length !== 0 || JSON.stringify(descriptor.cues) !== JSON.stringify([`explicit:${id}`])) throw Error('Invalid developer candidate descriptor');
    const relative = portable(descriptor.body?.path);
    if (relative === 'skill.json' || !/^[a-f0-9]{64}$/.test(descriptor.body.sha256)) throw Error('Invalid candidate content identity');
    const content = read(plain(path.join(directory, relative)), 1024 * 1024);
    if (sha(content) !== descriptor.body.sha256) throw Error('Developer candidate content hash differs');
    files.push({ path: `${id}/skill.json`, bytes: bytes.length, sha256: sha(bytes) }, { path: `${id}/${relative}`, bytes: content.length, sha256: sha(content) });
    const actual = fs.readdirSync(directory).sort();
    if (JSON.stringify(actual) !== JSON.stringify([relative, 'skill.json'].sort()) || actual.some(name => fs.lstatSync(plain(path.join(directory, name))).isDirectory())) throw Error('Unexpected developer candidate files');
    entries.push({ id, qualified_id: qualified(id), descriptor_sha256: sha(bytes), parts: [descriptor.body] });
  }
  files.sort((a, b) => a.path.localeCompare(b.path));
  return { source_id: sourceId, path: root, files, entries };
}
function configuration(id) {
  qualified(id);
  inspect();
  return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: '5c0d7f53-9a4e-4f0b-b8d2-3e61c2a9f7d4', kind: 'user', enabled: true, path: path.join(root, id) }], disabled: [] };
}
// Qualified selections for one arm, in activation order. The nearest arm may
// select several builtin skills; each becomes one repeated --skill argument.
function selection(arm, task) {
  const names = task.arm_skills?.[arm];
  if (!Array.isArray(names) || new Set(names).size !== names.length) throw Error('Invalid arm skill selection');
  if (arm === 'none') { if (names.length) throw Error('The none arm selects no skill'); return []; }
  if (arm === 'candidate') { if (names.length !== 1 || names[0] !== task.skill) throw Error('The candidate arm selects exactly its own candidate'); return [qualified(task.skill)]; }
  if (arm !== 'nearest' || !names.length || names.some(name => ids.includes(name) || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(name))) throw Error('The nearest arm selects builtin skills only');
  return names.map(name => `vcp-builtin::${name}::${name}`);
}
module.exports = { inspect, configuration, selection, qualified, ids, version, sourceId };
