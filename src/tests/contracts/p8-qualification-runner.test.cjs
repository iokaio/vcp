// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const runner = require('../../../scripts/evals/p8-qualification-runner.cjs');

const manifestFile = path.resolve(__dirname, '../../../scripts/evals/p8-qualification-manifest.json');

test('P8 manifest maps bounded rows to real commands and explicit gaps', () => {
  const loaded = runner.loadManifest(manifestFile);
  assert.equal(loaded.manifest.schema, 'p8-qualification-manifest/1');
  assert.ok(loaded.manifest.cases.length >= 15);
  assert.equal(new Set(loaded.manifest.cases.map(row => row.id)).size, loaded.manifest.cases.length);
  const executable = loaded.manifest.cases.filter(row => row.command !== null);
  assert.ok(executable.every(row => row.command[0] === 'cargo'));
  const packaged = loaded.manifest.cases.find(row => row.id === 'p8-01-packaged-cli-assets');
  assert.deepEqual(packaged.command.slice(0, 10), ['cargo', 'test', '--locked', '--offline', '-p', 'vcp-cli', '--features', 'qualification', '--test', 'executable']);
  assert.ok(packaged.command.includes('executable_packaged_skills_are_relocatable_lazy_and_integrity_checked'));
  assert.ok(packaged.requires.includes('packaged_artifact'));
  const gaps = loaded.manifest.cases.filter(row => row.gap === true);
  assert.ok(gaps.some(row => row.id === 'p8-03-packaged-encrypted-recovery'));
  assert.equal(gaps.length, 1);
  assert.ok(gaps.every(row => row.required === true && row.command === null));
});

test('P8 command construction binds every executable row to the repository workspace', () => {
  const loaded = runner.loadManifest(manifestFile);
  for (const row of loaded.manifest.cases.filter(row => row.command !== null)) {
    const command = runner.commandFor(loaded.manifest, row);
    assert.equal(command.program, 'cargo');
    assert.match(command.cwd_relative, /^src\/third_party\/codex\/codex-rs$/);
    assert.ok(command.args.includes('--locked'));
    assert.ok(command.args.includes('--offline'));
    assert.equal(command.args.at(-1), '--exact');
  }
});

test('a zero-test Cargo exit cannot become a passing qualification row', () => {
  const loaded = runner.loadManifest(manifestFile);
  const row = loaded.manifest.cases.find(row => row.id === 'p8-03-review-findings');
  const command = runner.commandFor(loaded.manifest, row);
  assert.equal(runner.expectedTest(command), 'structured_roundtrip_keeps_scope_uncertainty_and_revision');
  assert.equal(runner.observedPassingTest(command, 'test result: ok. 0 passed; 0 failed;'), false);
  assert.equal(runner.observedPassingTest(command, 'test review_findings::structured_roundtrip_keeps_scope_uncertainty_and_revision ... ok\n'), true);
});

test('missing package prerequisites remain not-run reasons', () => {
  assert.deepEqual(
    runner.requirementReasons(['packaged_artifact'], {
      platform: 'win32',
      cargo_available: true,
      vcp_test_git: process.execPath
    }),
    ['exact packaged artifact is unavailable']
  );
});

test('packaged skill prerequisite follows the exact extracted package environment', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-p8-package-'));
  const packageDirectory = path.join(root, 'extracted-package');
  fs.mkdirSync(packageDirectory);
  assert.deepEqual(runner.requirementReasons(['packaged_artifact'], {
    platform: 'win32',
    cargo_available: true,
    packaged_artifact: packageDirectory
  }), []);
  fs.rmSync(root, {recursive: true, force: true});
});

test('campaign source identity includes content and untracked source hashes', () => {
  const identity = runner.sourceIdentity();
  assert.match(identity.schema, /bounded-source-identity/);
  assert.equal(identity.content_sha256.length, 64);
  assert.equal(identity.untracked_sha256.length, 64);
  assert.ok(identity.files.some(file => file.path === 'scripts/evals/p8-qualification-manifest.json'));
  assert.ok(identity.files.some(file => file.path === 'src/tests/contracts/p8-qualification-runner.test.cjs'));
  assert.ok(identity.files.some(file => file.path === 'src/third_party/codex/codex-rs/Cargo.lock'));
  assert.equal(new Set(identity.files.map(file => file.path)).size, identity.files.length);
  assert.equal(new Set(identity.scope).size, identity.scope.length);
  assert.match(identity.integrity_scope, /deduplicated scope/);
});

test('native wrapper is self-contained and pinned to the repository toolchain', () => {
  const wrapper = fs.readFileSync(path.resolve(__dirname, '../../../scripts/evals/p8-native-command.ps1'), 'utf8');
  assert.match(wrapper, /p8-command\/1/);
  assert.match(wrapper, /RustToolchain = '1\.98\.0'/);
  assert.match(wrapper, /Launch-VsDevShell\.ps1/);
  assert.match(wrapper, /run \$RustToolchain cargo/);
  assert.match(wrapper, /VCP_TEST_NODE/);
  assert.match(wrapper, /VCP_TEST_CARGO/);
  assert.match(wrapper, /VCP_TEST_SKILL_PACKAGE/);
});

test('summary never reports a partial or missing required campaign as pass', () => {
  const rows = [
    {id: 'pass', area: 'history', required: true, status: 'pass'},
    {id: 'gap', area: 'packaged', required: true, status: 'not_run', reasons: ['missing package']},
    {id: 'optional', area: 'native_process', required: false, status: 'not_run', reasons: ['not selected']}
  ];
  const summary = runner.summarize(rows);
  assert.equal(summary.overall, 'incomplete');
  assert.deepEqual(summary.counts, {pass: 1, fail: 0, not_run: 2});
  assert.equal(summary.required_rows, 2);
  assert.equal(summary.required_passed, 1);
  assert.equal(summary.limitations.length, 2);
});

test('dry-run creates a reviewable result without invoking Cargo', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-p8-runner-'));
  const output = path.join(root, 'run');
  const result = runner.runCampaign({manifest: manifestFile, output, cases: ['p8-03-review-findings'], dryRun: true, wrapperArgs: [], cargo_available: true});
  assert.equal(result.status, 'incomplete');
  const selected = result.cases.find(row => row.id === 'p8-03-review-findings');
  assert.equal(selected.status, 'not_run');
  assert.match(selected.reasons[0], /dry run/);
  assert.ok(result.cases.every(row => Array.isArray(row.reasons) || row.status === 'pass'));
  assert.ok(fs.existsSync(path.join(output, 'manifest.json')));
  fs.rmSync(root, {recursive: true, force: true});
});
