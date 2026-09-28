// SPDX-License-Identifier: Apache-2.0
'use strict';
// Explicit CS-3 preparation source; never installed or discovered as a builtin skill.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { portable } = require('../skills/builtin-assets.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const repository = path.resolve(__dirname, '../..');
const root = path.join(repository, 'src/skills/candidates/webapp-testing');
const id = 'webapp-testing', version = '1.0.0', sourceId = 'vcp-webapp-candidate';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function qualified() { return `${sourceId}::.::${id}`; }

function inspect() {
  const directory = plain(root), descriptorBytes = read(path.join(directory, 'skill.json'), 1024 * 1024);
  const descriptor = JSON.parse(descriptorBytes);
  if (descriptor.schema_version !== 1 || descriptor.id !== id || descriptor.version !== version
    || descriptor.source !== 'vcp-original' || descriptor.license !== 'Apache-2.0' || descriptor.vcp_version !== 1
    || JSON.stringify(descriptor.cues) !== JSON.stringify([`explicit:${id}`])
    || JSON.stringify(descriptor.required_tools) !== JSON.stringify(['vcp_list', 'vcp_read'])
    || !Array.isArray(descriptor.environments) || descriptor.environments.length
    || !Array.isArray(descriptor.resources) || descriptor.resources.length) {
    throw Error('Invalid webapp-testing candidate descriptor');
  }
  const relative = portable(descriptor.body?.path);
  if (relative === 'skill.json' || !/^[a-f0-9]{64}$/.test(descriptor.body.sha256)) throw Error('Invalid webapp-testing content identity');
  const body = read(plain(path.join(directory, relative)), 1024 * 1024);
  if (sha(body) !== descriptor.body.sha256) throw Error('Webapp-testing candidate content hash differs');
  const actual = fs.readdirSync(directory).sort();
  if (JSON.stringify(actual) !== JSON.stringify([relative, 'skill.json'].sort())
    || actual.some(name => fs.lstatSync(plain(path.join(directory, name))).isDirectory())) {
    throw Error('Unexpected webapp-testing candidate files');
  }
  return { source_id: sourceId, path: root,
    files: [{ path: 'skill.json', bytes: descriptorBytes.length, sha256: sha(descriptorBytes) }, { path: relative, bytes: body.length, sha256: sha(body) }].sort((a, b) => a.path.localeCompare(b.path)),
    entry: { id, qualified_id: qualified(), descriptor_sha256: sha(descriptorBytes), parts: [descriptor.body] } };
}

function configuration() {
  inspect();
  return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: '0720d157-6450-438c-a7df-1d64448cb7dd', kind: 'user', enabled: true, path: root }], disabled: [] };
}

function selection(arm, task) {
  const names = task?.arm_skills?.[arm];
  if (!Array.isArray(names) || new Set(names).size !== names.length) throw Error('Invalid webapp-testing arm selection');
  if (arm === 'none') { if (names.length) throw Error('The none arm selects no skill'); return []; }
  if (arm === 'nearest') { if (JSON.stringify(names) !== JSON.stringify(['testing'])) throw Error('The nearest arm selects testing only'); return ['vcp-builtin::testing::testing']; }
  if (arm === 'candidate') { if (task.skill !== id || JSON.stringify(names) !== JSON.stringify([id])) throw Error('The candidate arm selects webapp-testing only'); return [qualified()]; }
  throw Error('Unknown webapp-testing arm');
}

module.exports = { inspect, configuration, selection, qualified, id, version, sourceId };
