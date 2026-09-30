// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { authoring, tree } = require('../../../scripts/release/helper-node.cjs');
const repo = path.resolve(__dirname, '../../..');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-helper-contract-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const validator = path.join(root, 'skill-authoring/scripts/validate.cjs');
  fs.mkdirSync(path.dirname(validator), { recursive: true });
  fs.copyFileSync(path.join(repo, 'src/skills/builtin/skill-authoring/scripts/validate.cjs'), validator);
  const rows = [];
  for (const id of ['clean', 'warn']) {
    const directory = path.join(root, id); fs.mkdirSync(directory);
    const body = id === 'warn' ? 'See [missing](missing.md).\n' : 'Synthetic fixture.\n';
    fs.writeFileSync(path.join(directory, 'SKILL.md'), body);
    const descriptor = { schema_version: 1, id, version: '1.0.0', description: 'Use when testing a synthetic fixture.',
      source: 'VCP qualification regression', license: 'Apache-2.0', vcp_version: 1, cues: [], environments: [], required_tools: [],
      body: { path: 'SKILL.md', sha256: hash(body) }, resources: [] };
    const bytes = JSON.stringify(descriptor); fs.writeFileSync(path.join(directory, 'skill.json'), bytes);
    rows.push({ id, descriptor: `${id}/skill.json`, descriptor_sha256: hash(bytes) });
  }
  const catalog = { schema_version: 1, skills: rows };
  const save = () => fs.writeFileSync(path.join(root, 'catalog.json'), JSON.stringify(catalog)); save();
  return { root, catalog, save };
}
test('helper qualification validates every catalog entry and retains actual authoring warnings', t => {
  const f = fixture(t), report = authoring(f.root);
  assert.equal(report.status, 'pass'); assert.equal(report.skills.length, 2); assert.equal(report.warnings, 1);
  assert.equal(report.skills[0].status, 'pass'); assert.equal(report.skills[1].status, 'pass-with-warnings');
  assert.deepEqual(report.skills[1].warnings, ['Link in SKILL.md points at an undeclared file: missing.md']);
  fs.writeFileSync(path.join(f.root, 'clean/SKILL.md'), 'changed source bytes');
  assert.throws(() => authoring(f.root), /Content hash mismatch/);
});
test('helper catalog selection rejects duplicate IDs, traversal, hash mismatch and missing entries', t => {
  const cases = [
    f => f.catalog.skills.push(f.catalog.skills[0]),
    f => { f.catalog.skills[0].id = '../clean'; },
    f => { f.catalog.skills[0].descriptor = 'warn/skill.json'; },
    f => { f.catalog.skills[0].descriptor_sha256 = '0'.repeat(64); },
    f => { f.catalog.skills = []; },
  ];
  for (const change of cases) { const f = fixture(t); change(f); f.save(); assert.throws(() => authoring(f.root)); }
});
test('dependency inventory binds content and refuses redirected directories', t => {
  const f = fixture(t), selected = path.join(f.root, 'dependencies'); fs.mkdirSync(selected);
  const file = path.join(selected, 'dependency.dat'); fs.writeFileSync(file, 'before');
  const before = tree(selected, path.join(f.root, 'before.json'));
  fs.writeFileSync(file, 'after');
  const after = tree(selected, path.join(f.root, 'after.json'));
  assert.notEqual(before.sha256, after.sha256); assert.equal(before.files, 1);
  const redirected = path.join(f.root, 'redirected');
  fs.symlinkSync(selected, redirected, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => tree(redirected, path.join(f.root, 'redirect.json')), /Redirected/);
  assert.equal(fs.readFileSync(file, 'utf8'), 'after');
});
test('helper runner refuses output in a checkout before it starts any candidate program', { skip: process.platform !== 'win32' }, t => {
  const f = fixture(t), checkout = path.join(f.root, 'checkout'); fs.mkdirSync(checkout); fs.mkdirSync(path.join(checkout, '.git'));
  const output = path.join(checkout, 'must-not-create');
  const result = spawnSync('pwsh', ['-NoProfile', '-File', path.join(repo, 'scripts/release/helper-qualification.ps1'),
    '-NativeResult', 'unavailable', '-InstalledEngine', 'unavailable', '-Python', 'unavailable', '-Node', 'unavailable',
    '-BrowserProject', 'unavailable', '-OutputRoot', output, '-QualificationExecutable', 'unavailable'],
  { encoding: 'utf8', windowsHide: true, timeout: 10000 });
  assert.ifError(result.error); assert.notEqual(result.status, 0);
  assert.match(result.stderr, /outside repository trees/); assert.equal(fs.existsSync(output), false);
});
