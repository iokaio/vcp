// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { runSuite, parseSelection, sourceIdentity, digest, writeManifest } = require('../../src/tests/support/harness.cjs');
const { outside } = require('../../src/tests/support/model-assets.cjs');
const { validateResults, testBinary } = require('../../src/tests/support/lifecycle-results.cjs');
async function main(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!['--cargo-log', '--output-root'].includes(argv[i]) || !argv[i + 1] || Object.hasOwn(options, argv[i])) throw Error('Invalid lifecycle arguments');
    options[argv[i]] = argv[i + 1];
  }
  if (Object.keys(options).length !== 2) throw Error('Supply --cargo-log and --output-root');
  if (process.platform !== 'win32') { console.log(JSON.stringify({ status: 'not_run', reason: 'Native Windows is required' })); process.exitCode = 3; return; }
  const root = path.resolve(__dirname, '../..');
  const binary = testBinary(fs.readFileSync(options['--cargo-log'], 'utf8'));
  const directory = path.join(outside(options['--output-root'], [path.join(root, 'src')]), crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const home = path.join(directory, 'profile'); fs.mkdirSync(home);
  const manifest = { schema_version: 1, task_id: 'P0-03', status: 'running', started_at: new Date().toISOString(),
    binary_sha256: digest(fs.readFileSync(binary)), cargo_log_sha256: digest(fs.readFileSync(options['--cargo-log'])),
    limitations: ['Admission only; no active cancellation, process stop, durable checkpoint, owner loss or CLI pause.'] };
  const manifestPath = path.join(directory, 'manifest.json');
  writeManifest(manifestPath, manifest);
  const controller = new AbortController();
  process.once('SIGINT', () => controller.abort()); process.once('SIGTERM', () => controller.abort());
  try {
    const native = "const r=require('node:child_process').spawnSync(process.argv[1],process.argv.slice(2),{env:{...process.env,CODEX_TEST_ENVIRONMENT:'local',RUST_MIN_STACK:'8388608'},stdio:'inherit',windowsHide:true});process.exit(r.error||r.signal||r.status===null?1:r.status)";
    const registry = { schema_version: 1, suites: { lifecycle: ['drain', 'continuation'] }, cases: {} };
    for (const [group, filter] of [['drain', 'host_drain_'], ['continuation', 'continuation_seal_']]) {
      registry.cases[group] = { args: ['-e', native, binary, 'suite::turn_input_submission::' + filter, '--nocapture', '--test-threads=1'],
        task_ids: ['P0-03'], backends: ['none'], requires: [], timeout_ms: 180000, max_output_bytes: 2 * 1024 * 1024 };
    }
    const source = await sourceIdentity(root, controller.signal); manifest.source = source;
    const result = await runSuite({ root, registry, selection: parseSelection(['--suite', 'lifecycle'], registry),
      outputRoot: path.join(directory, 'attempts'), source, signal: controller.signal, isolation: { homeRoot: home }, announce: () => {} });
    manifest.result = result.manifestPath;
    if (result.exitCode !== 0 || result.manifest.status !== 'pass' || result.manifest.attempts.length !== 2) throw Error('Native lifecycle suite failed');
    manifest.tests_passed = 0;
    for (const attempt of result.manifest.attempts) {
      if (attempt.exit_code !== 0 || !attempt.capture_complete) throw Error('Incomplete native lifecycle capture');
      const output = attempt.artifacts.find(a => a.path.endsWith('-stdout.log'));
      const bytes = fs.readFileSync(path.join(path.dirname(result.manifestPath), output.path));
      if (digest(bytes) !== output.sha256) throw Error('Lifecycle output changed after capture');
      manifest.tests_passed += validateResults(bytes.toString('utf8'), attempt.case_id);
    }
    manifest.status = 'pass'; manifest.exit_code = 0;
  } catch (error) { manifest.status = 'fail'; manifest.exit_code = 1; manifest.reason = error.message; }
  manifest.ended_at = new Date().toISOString(); writeManifest(manifestPath, manifest);
  console.log(JSON.stringify({ status: manifest.status, tests_passed: manifest.tests_passed, reason: manifest.reason, manifest: manifestPath }));
  process.exitCode = manifest.exit_code;
}
main(process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
