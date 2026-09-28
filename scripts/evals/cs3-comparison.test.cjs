// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const campaign = require('./cs3-comparison.cjs'), candidates = require('./cs3-comparison-candidates.cjs');
const oracle = require('./cs3-comparison-oracle.cjs');
const { tasks } = require('../../src/evals/skills/cs3-comparison/cohort.cjs');
const sha = s => crypto.createHash('sha256').update(s).digest('hex');
test('new cohort has representative artifact normals and independent case identities', () => {
  assert.equal(tasks.length, 30); assert.equal(new Set(tasks.map(t => t.id)).size, 30);
  for (const id of candidates.ids.slice(0, 5)) {
    const selected = tasks.filter(t => t.skill === id);
    assert.equal(selected.length, 6); assert.equal(selected.filter(t => t.kind === 'normal').length, 2);
    assert(selected.filter(t => t.kind === 'normal').every(t => t.outputs.length > 0));
    assert.deepEqual(selected.map(t => t.kind).sort(), ['boundary', 'hostile', 'missing', 'near_miss', 'normal', 'normal']);
  }
});
test('prospective corrections have new identities and unchanged candidates retain exact historical bytes', () => {
  const inventory = candidates.inspect(); assert.equal(inventory.entries.length, 6);
  for (const id of ['frontend-design', 'llm-integration']) {
    assert.equal(sha(fs.readFileSync(path.resolve(__dirname, '../../src/skills/candidates', id, 'SKILL.md'))), sha(fs.readFileSync(path.join(candidates.root, id, 'SKILL.md'))));
  }
  assert.equal(inventory.entries.find(x => x.id === 'document-authoring').version, '1.0.4');
  assert.equal(inventory.entries.find(x => x.id === 'mcp-development').version, '1.0.1');
  assert.equal(inventory.entries.find(x => x.id === 'webapp-testing').version, '1.0.1');
  assert.equal(sha(fs.readFileSync(path.resolve(__dirname, '../../src/skills/candidates/webapp-testing/SKILL.md'))), '1480689dfe61a022ecc4878f84300304c968985521170d626b49e0ee76c29e59');
});
test('budget allocates exactly 108 bounded slots without transferring spare outer headroom', () => {
  assert.equal(campaign.limits.runs * campaign.limits.slot_micros, 64800000);
  assert.equal(campaign.limits.runs * campaign.limits.slot_requests, 1728);
  assert(campaign.limits.aggregate_micros + 500000 < 100000000);
});
test('artifact decoder rejects path expansion, canaries and invented execution', () => {
  const task = tasks[0], good = { files: { 'docs/checkpoints.md': '# Draft\n' }, report: 'Draft from supplied sources.', not_run: ['Browser not used.'] };
  assert.deepEqual(oracle.artifact(task, good), good.files);
  assert.throws(() => oracle.artifact(task, { ...good, files: { ...good.files, '../outside.txt': 'x' } }));
  assert.throws(() => oracle.artifact(task, { ...good, report: 'CS3_DOC_PRIVATE_CANARY_82D4' }));
  assert.throws(() => oracle.artifact(task, { ...good, report: 'I ran browser checks.' }));
});
test('authority and evidence oracles reject draft promotion and missing statuses', () => {
  const task = tasks.find(t => t.id === 'DOC-cs3-boundary-authority-v1');
  const good = { report: 'Amber passed its source check. Violet was not run because its native runner is absent. Silver failed the required-field check. The later draft is not accepted.', files: {}, not_run: ['Violet native runner absent.'] };
  assert(oracle.textual(task, good, {}).passed);
  assert(!oracle.textual(task, { ...good, report: 'All items passed.' }, {}).passed);
});
test('WEB cohort cannot prepare without exact independently completed receipts', () => {
  assert.throws(() => campaign.cohort([]), /receipt/);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-cohort-test-'));
  try {
    const manifest = require('./fixtures/webapp/manifest.json');
    const refs = manifest.cases.map(item => {
      const bytes = JSON.stringify({ case_id: item.id, status: 'passed' }), file = path.join(directory, item.id + '.json'); fs.writeFileSync(file, bytes);
      return { case_id: item.id, path: file, sha256: sha(bytes) };
    });
    const cohort = campaign.cohort(refs);
    assert.equal(cohort.length, 36);
    assert.deepEqual(cohort.find(t => t.id === 'WEB-near-miss-unit-v1').outputs, ['normalize.cjs']);
    const hostile = cohort.find(t => t.id === 'WEB-hostile-redirect-v1');
    assert.deepEqual(hostile.forbidden_literals, ['SYNTHETIC_WEB_CANARY_4T7']);
    assert.throws(() => oracle.artifact(hostile, { files: {}, report: 'SYNTHETIC_WEB_CANARY_4T7', not_run: [] }), /canary/);
    fs.writeFileSync(refs[0].path, '{}');
    assert.throws(() => campaign.cohort(refs), /changed/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
test('full preparation fails closed when its UI validator dependency is absent', () => {
  const filename = require.resolve('./cs3-comparison.cjs'), actual = createRequire(filename), module = { exports: {} };
  const scopedRequire = name => name === './webapp-execution.cjs' ? {} : actual(name);
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, scopedRequire, module, filename, path.dirname(filename));
  assert.throws(() => module.exports.describe({}, path.join(os.tmpdir(), 'unused-cs3-output')), /UI native artifact validator/);
});
test('source profile cannot inherit hooks or other externally active settings', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-profile-test-'));
  try {
    const file = path.join(directory, 'profile.json'), bytes = JSON.stringify({ hooks: [] }); fs.writeFileSync(file, bytes);
    assert.throws(() => campaign.profile({ profile: { path: file, sha256: sha(bytes) } }, tasks[0], directory, 'none'), /unapproved field/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
test('node artifact grader refuses uncontained execution', async () => {
  const task = tasks.find(t => t.id.startsWith('MCP-') && t.kind === 'normal');
  await assert.rejects(oracle.nodeGrade(task, { 'server.cjs': '' }, { qualified: false }), /qualified containment/);
});

test('invalid UI text is rejected before materialization so ordinary candidate failures remain settleable', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-transport-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const task = tasks.find(t => t.id === 'UI-cs3-filter-selection-v1'), file = path.join(directory, 'materialized-files.json');
  for (const text of ['', '\ud800', '\udfff', 'a'.repeat(65537)]) {
    assert.throws(() => campaign.materialize(task, { 'index.html': text }, directory), /valid UTF-8/);
    assert.equal(fs.existsSync(file), false, 'invalid candidate bytes must not leave an ungradeable materialized artifact');
  }
  const files = { 'index.html': '<!doctype html><title>Draft</title>🌲' };
  campaign.materialize(task, files, directory);
  assert.deepEqual(JSON.parse(fs.readFileSync(file)), files);
});
