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

test('public UI validator is the qualified implementation and rejects fabricated native evidence', t => {
  const adapter = require('./webapp-execution.cjs'), ui = require('./cs3-ui-artifact.cjs');
  assert.equal(typeof adapter.validateUiArtifact, 'function');
  assert.equal(adapter.validateUiArtifact, ui.validateUiArtifact);
  assert.throws(() => campaign.describe({}, path.join(os.tmpdir(), 'unused-cs3-output')), /Unexpected campaign specification fields/);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-public-ui-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const files = { 'index.html': '<!doctype html><title>Draft</title>' }, caseId = ui.cases[0], artifactHash = sha(JSON.stringify(files));
  assert.throws(() => adapter.validateUiArtifact({ status: 'passed' }, files, caseId), /Exact observed UI browser receipt/);
  // Valid source closure cannot turn arbitrary, hash-bound JSON into an actual
  // native browser observation. This exercises the public entry point, no stub.
  const sources = ui.sourceNames.map(name => {
    const original = fs.readFileSync(path.resolve(__dirname, '../../src/tests/support/windows/webapp', name));
    const generated = name === 'UiArtifactResource.cs', bytes = generated ? Buffer.from(ui.renderResource(caseId, files, artifactHash)) : original;
    fs.writeFileSync(path.join(directory, name), bytes);
    return { path: name, sha256: sha(bytes), ...(generated ? { template_sha256: sha(original) } : {}) };
  });
  const reference = (name, content) => {
    const file = path.join(directory, name), bytes = JSON.stringify(content); fs.writeFileSync(file, bytes);
    return { path: file, sha256: sha(bytes) };
  };
  const receipt = { schema: 'cs3-ui-artifact-browser/1', case_id: caseId, artifact_sha256: artifactHash, status: 'passed',
    cleanup: 'completed', containment: 'qualified', visual_review: 'not_run', assertions: ui.assertions(caseId).map(name => ({ name, passed: true })),
    native_receipt: reference('native.json', {}), build: reference('inputs.json', { sources }) };
  assert.throws(() => adapter.validateUiArtifact(receipt, files, caseId), /Clean observed native receipt required/);
});
test('pre-block qualification window uses all expiries and requires strict complete-block margin', () => {
  const now = 1000000, required = 18 * (180 + 180) * 1000;
  const profile = { deadline_seconds: 180, provider: { valid_until: String(now + required + 1), compatibility: { valid_until: String(now + required + 1) }, price: { valid_until: String(now + required + 1) } } };
  assert.equal(campaign.qualificationWindow(profile, now).required_milliseconds, required);
  for (const owner of ['snapshot', 'compatibility', 'price']) {
    const changed = structuredClone(profile);
    (owner === 'snapshot' ? changed.provider : changed.provider[owner]).valid_until = String(now + required);
    assert.throws(() => campaign.qualificationWindow(changed, now), /qualification window/);
  }
  const invalid = structuredClone(profile); delete invalid.provider.price.valid_until;
  assert.throws(() => campaign.qualificationWindow(invalid, now), /qualification window/);
});

test('failed or mismatched qualification builds cannot satisfy executable provenance', () => {
  const executable = { path: path.resolve('artifacts/synthetic-vcp.exe'), sha256: '1'.repeat(64) }, target = path.resolve('artifacts/synthetic-target');
  const build = { schema: 'cs3-comparison-build/1', status: 'passed', exit_code: 0, source_inputs_unchanged: true, toolchain_unchanged: true,
    expected_executable_matches: true, executable_sha256: executable.sha256, expected_executable_sha256: executable.sha256, executable: executable.path,
    qualification_build: true, production_release: false, provider_calls: 0, tests_executed: 0,
    builder_sha256: sha(fs.readFileSync(path.join(__dirname, 'cs3-comparison-build.ps1'))), target_directory: target,
    compiler_artifact: { reason: 'compiler-artifact', target: { name: 'vcp' }, profile: { test: false }, features: ['qualification'], executable: path.join(target, 'debug/vcp.exe') } };
  assert.equal(campaign.buildProvenance(build, executable), build);
  for (const mutate of [b => { b.status = 'failed'; }, b => { b.exit_code = 1; }, b => { b.schema = 'other'; },
    b => { b.source_inputs_unchanged = false; }, b => { b.toolchain_unchanged = false; }, b => { b.expected_executable_matches = false; },
    b => { b.expected_executable_sha256 = '0'.repeat(64); }, b => { b.executable_sha256 = '0'.repeat(64); }, b => { b.executable = target; },
    b => { b.qualification_build = false; }, b => { b.production_release = true; }, b => { b.provider_calls = 1; }, b => { b.tests_executed = 1; },
    b => { b.builder_sha256 = '0'.repeat(64); }, b => { b.compiler_artifact.features.push('other'); }, b => { b.compiler_artifact.profile.test = true; },
    b => { b.compiler_artifact.executable = target; }, b => { b.compiler_artifact.target.name = 'other'; }]) {
    const changed = structuredClone(build); mutate(changed); assert.throws(() => campaign.buildProvenance(changed, executable), /qualification/);
  }
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
