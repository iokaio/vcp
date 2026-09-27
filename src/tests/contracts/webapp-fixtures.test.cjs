// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { ownedRoot } = require('../support/experiments.cjs');
const web = require('../../../scripts/evals/webapp-fixtures.cjs');

function privateCohort(t) {
  const owned = ownedRoot(os.tmpdir());
  fs.cpSync(web.fixtureRoot, owned.root, { recursive: true });
  t.after(() => owned.cleanup());
  return owned.root;
}

function manifest(root) {
  return JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
}

function writeManifest(root, value) {
  fs.writeFileSync(path.join(root, 'manifest.json'), JSON.stringify(value));
}

test('WEB preparation cohort is frozen, bounded, independent and explicitly not run', () => {
  const cohort = web.inspect();
  assert.equal(cohort.revision, 'cs-3-webapp-fixtures-v1');
  assert.equal(cohort.loaded.size, 6);
  assert.equal(cohort.inventory.length, 29);
  assert.deepEqual(
    [...cohort.loaded.values()].map(entry => entry.task.kind).sort(),
    ['boundary', 'hostile', 'missing', 'near_miss', 'normal', 'normal']
  );
  assert.deepEqual(cohort.comparison.arms, {
    none: [], nearest: ['testing'], candidate: ['webapp-testing']
  });
  assert.equal(cohort.comparison.execution_authorized, false);
  assert.equal(cohort.manifest.model_calls, 0);
  assert.equal(cohort.manifest.browser_execution, 'not_run');
  assert.equal(cohort.manifest.campaign_execution, 'not_run');
  assert.equal(cohort.manifest.qualification, 'not_run');
  assert.deepEqual(cohort.manifest.limits, {
    file_bytes: 65536, case_input_bytes: 262144, case_files: 32
  });

  for (const [id, entry] of cohort.loaded) {
    assert.equal(entry.task.execution.model_calls, 0);
    assert.equal(entry.task.execution.comparison, 'not_run');
    assert.ok(entry.files.size > 0 && entry.files.size <= 32);
    assert.ok([...entry.files.values()].every(bytes => Buffer.isBuffer(bytes) && bytes.length <= 65536));
    assert.ok([...entry.files.values()].reduce((sum, bytes) => sum + bytes.length, 0) <= 262144);
    assert.equal(entry.oracle.case_id, id);
    assert.equal(entry.oracle.execution_state, 'not_run');
  }
});

test('candidate inputs exclude held-out oracle and comparison material', () => {
  for (const id of web.inspect().loaded.keys()) {
    const input = web.candidateInput(id);
    assert.deepEqual(Object.keys(input).sort(), ['context', 'files', 'id', 'kind', 'prompt']);
    assert.ok(input.files instanceof Map);
    assert.ok(!input.files.has('manifest.json'));
    assert.ok(!input.files.has('rubric.json'));
    assert.ok(!input.files.has('comparison.json'));
    assert.ok([...input.files.keys()].every(name => !name.includes('/') && !name.includes('oracle')));
    assert.equal(Object.hasOwn(input, 'oracle'), false);
  }

  const first = web.candidateInput('WEB-normal-form-v1');
  const original = Buffer.from(first.files.get('app.js'));
  first.files.get('app.js').fill(0);
  assert.deepEqual(web.candidateInput(first.id).files.get('app.js'), original);
});

test('held-out oracles carry explicit class-specific expectations', () => {
  const normal = web.oracle('WEB-normal-form-v1');
  assert.ok(normal.required_observations.some(value => value.includes('Typing Ada')));
  assert.ok(normal.required_observations.some(value => value.includes('Accessibility tree')));

  const polling = web.oracle('WEB-normal-polling-v1');
  assert.ok(polling.required_observations.some(value => value.includes('Alpha then Beta')));
  const boundary = web.oracle('WEB-boundary-occupied-port-v1');
  assert.ok(boundary.forbidden_actions.some(value => value.includes('terminating the existing listener')));
  const hostile = web.oracle('WEB-hostile-redirect-v1');
  assert.deepEqual(hostile.forbidden_output_literals, ['SYNTHETIC_WEB_CANARY_4T7']);
  assert.ok(hostile.required_observations.some(value => value.includes('https://blocked.invalid/')));
  const missing = web.oracle('WEB-missing-browser-v1');
  assert.ok(missing.required_observations.some(value => value.includes('unavailable or not run')));
  const nearMiss = web.oracle('WEB-near-miss-unit-v1');
  assert.deepEqual(nearMiss.allowed_modifications, ['normalize.cjs']);
  assert.equal(web.inspect().loaded.get('WEB-near-miss-unit-v1').task.expected.candidate_selection, 'unnecessary');
});

test('WEB loader rejects changed project bytes, changed oracle and extra inventory', async t => {
  await t.test('project hash', child => {
    const root = privateCohort(child);
    fs.appendFileSync(path.join(root, 'projects/WEB-normal-form-v1/app.js'), '\nchanged');
    assert.throws(() => web.inspect(root), /Frozen WEB input changed/);
  });
  await t.test('oracle hash', child => {
    const root = privateCohort(child);
    fs.appendFileSync(path.join(root, 'oracles/WEB-normal-form-v1.json'), ' ');
    assert.throws(() => web.inspect(root), /Frozen WEB input changed/);
  });
  await t.test('unexpected file', child => {
    const root = privateCohort(child);
    fs.writeFileSync(path.join(root, 'unexpected.txt'), 'not declared');
    assert.throws(() => web.inspect(root), /Unexpected WEB fixture inventory/);
  });
});

test('WEB loader rejects duplicate and unsafe manifest references', async t => {
  await t.test('duplicate shared path', child => {
    const root = privateCohort(child), value = manifest(root);
    value.shared[1] = structuredClone(value.shared[0]);
    writeManifest(root, value);
    assert.throws(() => web.inspect(root), /Duplicate WEB inventory path/);
  });
  await t.test('duplicate project path', child => {
    const root = privateCohort(child), value = manifest(root);
    value.cases[0].expected.source_files.push(structuredClone(value.cases[0].expected.source_files[0]));
    writeManifest(root, value);
    assert.throws(() => web.inspect(root), /Invalid WEB project file path/);
  });
  await t.test('escaping project path', child => {
    const root = privateCohort(child), value = manifest(root);
    value.cases[0].expected.source_files[0].path = '../outside.json';
    writeManifest(root, value);
    assert.throws(() => web.inspect(root), /Invalid WEB project file path/);
  });
});

test('WEB loader bounds manifest and referenced files before parsing or hashing', async t => {
  await t.test('malformed manifest', child => {
    const root = privateCohort(child);
    fs.writeFileSync(path.join(root, 'manifest.json'), '{');
    assert.throws(() => web.inspect(root), SyntaxError);
  });
  await t.test('oversized manifest', child => {
    const root = privateCohort(child);
    fs.writeFileSync(path.join(root, 'manifest.json'), Buffer.alloc(65537, 0x20));
    assert.throws(() => web.inspect(root), /WEB manifest bound exceeded/);
  });
  await t.test('oversized reference', child => {
    const root = privateCohort(child), value = manifest(root);
    const readme = value.shared.find(ref => ref.path === 'README.md');
    readme.bytes = 65536;
    readme.sha256 = '0'.repeat(64);
    writeManifest(root, value);
    fs.writeFileSync(path.join(root, 'README.md'), Buffer.alloc(65537, 0x61));
    assert.throws(() => web.inspect(root), /WEB reference bound exceeded/);
  });
});

test('WEB loader rejects a redirected cohort root', t => {
  const owned = ownedRoot(os.tmpdir());
  t.after(() => owned.cleanup());
  const target = path.join(owned.root, 'target');
  const redirected = path.join(owned.root, 'redirected');
  fs.cpSync(web.fixtureRoot, target, { recursive: true });
  fs.symlinkSync(target, redirected, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => web.inspect(redirected), /root or ancestor is redirected/);
});

test('WEB loader rejects a redirected nested oracle directory before reading it', t => {
  const root = privateCohort(t);
  const original = path.join(root, 'oracles'), moved = path.join(root, 'retained-oracles');
  fs.renameSync(original, moved);
  fs.symlinkSync(moved, original, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => web.inspect(root), /root or ancestor is redirected/);
});
