// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { SUITES, parseArgs, outsideSource, verifySource, validateResults } = require('../../src/tests/support/gemini-baseline.cjs');
const { digest, sourceIdentity, writeManifest, runSuite, parseSelection } = require('../../src/tests/support/harness.cjs');

async function main(argv, profile = { task_id: 'P0-07', suites: SUITES }) {
  const options = parseArgs(argv);
  const { source, outputRoot } = outsideSource(options['--source'], options['--output-root']);
  const repository = path.resolve(__dirname, '../..');
  const directory = path.join(outputRoot, crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, 'manifest.json');
  const record = { schema_version: 1, task_id: profile.task_id, candidate: 'gemini-cli', status: 'prepared',
    candidate_commit: null, candidate_tree: null, started_at: new Date().toISOString(),
    node: process.version, platform: process.platform, os_release: os.release(), stages: [],
    limitations: ['Pinned upstream unit tests with mocked SDK/transports; no VCP port, live-provider or OS-enforcement qualification.'] };
  const controller = new AbortController();
  const interrupt = () => controller.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  const save = () => writeManifest(file, record);
  function notRun(reason) { record.status = 'not_run'; record.reason = reason; record.exit_code = 3; }
  save();
  try {
    if (process.platform !== 'win32' || Number(process.versions.node.split('.')[0]) < 24) { notRun('Native Windows and Node 24 or later are required'); return 3; }
    if (!fs.existsSync(path.join(repository, 'src/tests/node_modules/@iarna/toml/package.json'))) { notRun('Install VCP development tools with npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund'); return 3; }
    const TOML = require('../../src/tests/node_modules/@iarna/toml');
    const expected = TOML.parse(fs.readFileSync(path.join(repository, 'src/third_party/upstreams.toml'), 'utf8')).upstream.find(p => p.id === 'gemini-cli');
    if (!expected || expected.origin !== 'https://github.com/google-gemini/gemini-cli') throw Error('Missing Gemini provenance');
    record.candidate_commit = expected.commit; record.candidate_tree = expected.tree;
    if (!fs.existsSync(source)) { notRun('Acquire the pinned Gemini checkout first'); return 3; }
    const gitVersion = spawnSync('git', ['--version'], { encoding: 'utf8', windowsHide: true, timeout: 10000 });
    if (gitVersion.error?.code === 'ENOENT') { notRun('Git is required'); return 3; }
    if (gitVersion.error || gitVersion.status !== 0) throw Error('Cannot execute Git prerequisite');
    record.source = verifySource(source, expected);
    record.vcp_source = await sourceIdentity(repository, controller.signal);
    record.runner_sha256 = digest(fs.readFileSync(__filename));
    record.harness_sha256 = digest(fs.readFileSync(path.join(repository, 'src/tests/support/harness.cjs')));
    record.policy_sha256 = digest(fs.readFileSync(path.join(repository, 'src/tests/support/gemini-baseline.cjs')));
    record.suites = profile.suites;
    if (profile.fixture) {
      const fixture = fs.readFileSync(profile.fixture);
      if (JSON.parse(fixture).revision !== expected.commit) throw Error('Fixture revision differs from the candidate pin');
      record.fixture_sha256 = digest(fixture);
      record.profile_runner_sha256 = digest(fs.readFileSync(profile.runner));
    }
    record.package_lock_sha256 = digest(fs.readFileSync(path.join(source, 'package-lock.json')));
    const core = path.join(source, 'packages/core');
    const required = ['node_modules/vitest/vitest.mjs', 'node_modules/typescript/bin/tsc'];
    if (required.some(p => !fs.existsSync(path.join(source, p)))) { notRun('Run npm ci --ignore-scripts --no-audit --no-fund in the pinned checkout'); return 3; }
    record.vitest = JSON.parse(fs.readFileSync(path.join(source, 'node_modules/vitest/package.json'))).version;
    record.typescript = JSON.parse(fs.readFileSync(path.join(source, 'node_modules/typescript/package.json'))).version;
    const lock = JSON.parse(fs.readFileSync(path.join(source, 'package-lock.json')));
    if (record.vitest !== lock.packages['node_modules/vitest'].version || record.typescript !== lock.packages['node_modules/typescript'].version) throw Error('Installed compiler/test runner differs from the lockfile');
    const homeRoot = path.join(directory, 'synthetic-home'); fs.mkdirSync(homeRoot);
    const isolation = { homeRoot, gitCommit: expected.commit };
    const reportFile = path.join(directory, 'test-results.json');
    const phases = options.prepare ? [
      { id: 'metadata', root: source, args: ['scripts/generate-git-commit-info.js'] },
      { id: 'typescript', root: core, args: ['../../node_modules/typescript/bin/tsc', '--build', '--force'] }
    ] : [];
    if (!options.prepare && ['src/generated/git-commit.ts', 'dist/index.js'].some(p => !fs.existsSync(path.join(core, p)))) {
      notRun('Run this explicit qualification command with --prepare to generate metadata and compile core'); return 3;
    }
    phases.push({ id: 'tests', root: core, args: ['../../node_modules/vitest/vitest.mjs', 'run', ...Object.keys(profile.suites),
      '--coverage.enabled=false', '--maxWorkers=2', '--reporter=default', '--reporter=json', '--outputFile.json=' + reportFile] });
    record.status = 'running'; save();
    for (const phase of phases) {
      const registry = { schema_version: 1, suites: { qualification: [phase.id] }, cases: { [phase.id]: {
        args: phase.args, task_ids: [profile.task_id], backends: ['none'], requires: [], timeout_ms: 600000, max_output_bytes: 16 * 1024 * 1024
      } } };
      const result = await runSuite({ root: phase.root, registry, selection: parseSelection(['--suite', 'qualification'], registry),
        outputRoot: path.join(directory, 'stages'), source: record.source, signal: controller.signal, isolation,
        announce: () => console.log('Gemini qualification stage: ' + phase.id) });
      record.stages.push({ id: phase.id, status: result.manifest.status, exit_code: result.exitCode, manifest: path.relative(directory, result.manifestPath).split(path.sep).join('/') });
      save();
      if (result.exitCode !== 0) { record.status = result.manifest.status; record.exit_code = result.exitCode; return result.exitCode; }
    }
    record.result = validateResults(JSON.parse(fs.readFileSync(reportFile)), core, profile.suites);
    record.result_sha256 = digest(fs.readFileSync(reportFile));
    verifySource(source, expected);
    record.status = 'pass'; record.exit_code = 0;
    return 0;
  } catch (error) {
    record.status = 'fail'; record.reason = error.message; record.exit_code = controller.signal.aborted ? 130 : 1;
    return record.exit_code;
  } finally {
    record.ended_at = new Date().toISOString(); save();
    process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
    console.log(JSON.stringify({ status: record.status, tests: record.result?.tests, manifest: file }));
  }
}
module.exports = { main };
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(error => {
  console.error('Gemini qualification failed: ' + error.message); process.exitCode = 2;
});
