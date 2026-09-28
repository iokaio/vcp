// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const { ownedRoot } = require('../support/experiments.cjs');
const candidate = require('../../../scripts/evals/webapp-candidate.cjs');
const repository = path.resolve(__dirname, '../../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const packageRoot = path.join(owner.root, 'src/skills/candidates/webapp-testing');
  fs.cpSync(path.join(repository, 'src/skills/candidates/webapp-testing'), packageRoot, { recursive: true });
  const file = path.join(repository, 'scripts/evals/webapp-candidate.cjs'), module = { exports: {} };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(createRequire(file), module, module.exports, path.join(owner.root, 'scripts/evals'));
  return { packageRoot, api: module.exports };
}

test('webapp-testing is an exact resource-free explicit candidate outside the builtin catalog', () => {
  const inventory = candidate.inspect(), descriptorBytes = fs.readFileSync(path.join(inventory.path, 'skill.json'));
  const descriptor = JSON.parse(descriptorBytes), catalog = JSON.parse(fs.readFileSync(path.join(repository, 'src/skills/builtin/catalog.json')));
  assert.equal(inventory.source_id, 'vcp-webapp-candidate');
  assert.equal(inventory.entry.descriptor_sha256, sha(descriptorBytes));
  assert.deepEqual(inventory.entry.parts, [descriptor.body]);
  assert.deepEqual(descriptor.cues, ['explicit:webapp-testing']);
  assert.deepEqual(descriptor.resources, []);
  assert.equal(catalog.skills.some(skill => skill.id === 'webapp-testing'), false);
  const configured = candidate.configuration();
  assert.deepEqual(configured.sources.map(source => source.kind), ['user']);
  assert.equal(configured.sources[0].path, inventory.path);
  assert.equal(candidate.qualified(), 'vcp-webapp-candidate::.::webapp-testing');
});

test('webapp-testing comparison selection is explicit and keeps testing as its sole nearest baseline', () => {
  const task = { skill: 'webapp-testing', arm_skills: { none: [], nearest: ['testing'], candidate: ['webapp-testing'] } };
  assert.deepEqual(candidate.selection('none', task), []);
  assert.deepEqual(candidate.selection('nearest', task), ['vcp-builtin::testing::testing']);
  assert.deepEqual(candidate.selection('candidate', task), ['vcp-webapp-candidate::.::webapp-testing']);
  assert.throws(() => candidate.selection('none', { ...task, arm_skills: { ...task.arm_skills, none: ['testing'] } }), /selects no skill/);
  assert.throws(() => candidate.selection('nearest', { ...task, arm_skills: { ...task.arm_skills, nearest: ['javascript-typescript'] } }), /testing only/);
  assert.throws(() => candidate.selection('candidate', { ...task, arm_skills: { ...task.arm_skills, candidate: ['testing', 'webapp-testing'] } }), /webapp-testing only/);
});

test('changed candidate bytes, descriptor shape and undeclared files fail closed', t => {
  const { packageRoot, api } = fixture(t), body = path.join(packageRoot, 'SKILL.md'), descriptorFile = path.join(packageRoot, 'skill.json');
  fs.appendFileSync(body, '\nchanged');
  assert.throws(() => api.inspect(), /content hash differs/);
  fs.copyFileSync(path.join(repository, 'src/skills/candidates/webapp-testing/SKILL.md'), body);
  const original = fs.readFileSync(descriptorFile), descriptor = JSON.parse(original);
  descriptor.resources = [{ path: 'extra.md', sha256: 'a'.repeat(64) }];
  fs.writeFileSync(descriptorFile, JSON.stringify(descriptor));
  assert.throws(() => api.inspect(), /Invalid webapp-testing candidate descriptor/);
  fs.writeFileSync(descriptorFile, original);
  fs.writeFileSync(path.join(packageRoot, 'undeclared.md'), 'not declared');
  assert.throws(() => api.inspect(), /Unexpected webapp-testing candidate files/);
});

test('candidate body states the unavailable executable boundary without claiming browser evidence', () => {
  const body = fs.readFileSync(path.join(candidate.inspect().path, 'SKILL.md'), 'utf8');
  assert.match(body, /No qualified VCP browser\/server adapter exists/);
  assert.match(body, /unavailable or\s+not run/);
  assert.match(body, /does not\s+authorize dependency installation, browser provisioning/);
});
