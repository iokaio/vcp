// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync, spawn } = require('node:child_process');
const { validateResults } = require('../../src/tests/support/lifecycle-results.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
async function main(argv) {
  if (argv.length !== 4 || argv[0] !== '--cargo-log' || argv[2] !== '--output-root') throw Error('Supply --cargo-log and --output-root');
  if (process.platform !== 'win32') { console.log('not_run: native Windows required'); process.exitCode = 3; return; }
  const messages = fs.readFileSync(argv[1], 'utf8').replace(/^\uFEFF/, '').split(/\r?\n/).filter(Boolean).map(JSON.parse);
  if (!messages.some(message => message.reason === 'build-finished' && message.success)) throw Error('Cargo build did not finish successfully');
  function artifact(name, kind, test) {
    const matches = messages.filter(message => message.reason === 'compiler-artifact' && message.target?.name === name &&
      message.target.kind?.includes(kind) && message.profile?.test === test && message.executable && /(?:#vcp-lifecycle@|\/vcp-lifecycle#)\d/.test(message.package_id));
    if (matches.length !== 1) throw Error('Expected one compiled artifact: ' + name);
    return matches[0].executable;
  }
  const programs = { host: artifact('controller', 'test', true), journal: artifact('vcp_lifecycle', 'lib', true),
    owner: artifact('lifecycle-owner', 'example', false), fixture: artifact('vcp-process-fixture', 'bin', false) };
  const directory = path.resolve(argv[3]); fs.mkdirSync(directory, { recursive: true });
  const repository = path.resolve(__dirname, '../..');
  const fixturePaths = ['scripts/upstream/qualify-recovery.cjs', 'scripts/upstream/test-recovery.cjs',
    'scripts/upstream/test-execution-boundary.cjs', 'src/tests/support/lifecycle-results.cjs',
    'src/tests/support/windows/execution-boundary.ps1', 'src/tests/support/windows/AppContainerFixture.cs'];
  const manifest = path.join(directory, 'manifest.json');
  if (fs.existsSync(manifest)) throw Error('Recovery evidence already exists');
  const record = { schema_version: 1, task_ids: ['P0-03', 'P0-05'], status: 'running', started_at: new Date().toISOString(),
    cargo_log_sha256: hash(argv[1]), binaries: Object.fromEntries(Object.entries(programs).map(([name, file]) => [name, hash(file)])),
    fixture_sources: fixturePaths.map(file => ({ path: file, sha256: hash(path.join(repository, file)) })), stages: [] };
  const save = () => fs.writeFileSync(manifest, JSON.stringify(record, null, 2) + '\n'); save();
  try {
    for (const group of ['journal', 'host']) {
      const args = ['--nocapture', '--test-threads=1'];
      const result = spawnSync(programs[group], args, { encoding: 'utf8', timeout: 180000, maxBuffer: 2 * 1024 * 1024,
        windowsHide: true, env: { ...process.env, RUST_MIN_STACK: '16777216', CODEX_TEST_ENVIRONMENT: 'local' } });
      fs.writeFileSync(path.join(directory, group + '-stdout.log'), result.stdout || '');
      fs.writeFileSync(path.join(directory, group + '-stderr.log'), result.stderr || '');
      if (result.error || result.signal || result.status !== 0) throw Error(group + ' native tests failed');
      if (/\bpanicked at\b/.test(result.stdout + result.stderr)) throw Error('Background Rust panic in ' + group);
      if (group === 'host') validateResults(result.stdout + '\n' + result.stderr, 'host');
      else {
        const expected = ['exclusive_writer_and_workspace_identity', 'every_torn_tail_recovers_only_complete_frames', 'complete_corruption_is_not_silently_discarded'];
        for (const name of expected) if (!result.stdout.includes(`test journal::tests::${name} ... ok`)) throw Error('Missing journal result: ' + name);
        if (!result.stdout.includes('test result: ok. 3 passed; 0 failed; 0 ignored;')) throw Error('Unexpected journal summary');
      }
      const stage = { group, command: [programs[group], ...args], status: 'pass', stdout_sha256: hash(path.join(directory, group + '-stdout.log')) };
      if (group === 'host') {
        const measurements = [...result.stderr.matchAll(/^VCP_PROCESS_STOP (.+)$/gm)];
        if (measurements.length !== 1) throw Error('Missing native stop observation');
        const stop = JSON.parse(measurements[0][1]);
        if (stop.members_before !== 2 || stop.members_after !== 0 || !Number.isSafeInteger(stop.job_empty_ms) || stop.job_empty_ms < 0 ||
            !Number.isSafeInteger(stop.exclusive_lock_released_ms) || stop.exclusive_lock_released_ms < stop.job_empty_ms) throw Error('Invalid native stop measurement');
        stage.native_stop = stop;
      }
      record.stages.push(stage); save();
    }
    for (const [group, script, flag, program] of [
      ['owner', 'test-recovery.cjs', '--owner', programs.owner],
      ['execution', 'test-execution-boundary.cjs', '--fixture', programs.fixture]
    ]) {
      const args = [path.join(__dirname, script), flag, program, '--output-root', path.join(directory, group)];
      const result = await new Promise((resolve, reject) => {
        const child = spawn(process.execPath, args, { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
        let stdout = '', stderr = '';
        child.stdout.on('data', bytes => { stdout += bytes; }); child.stderr.on('data', bytes => { stderr += bytes; });
        child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal, stdout, stderr }));
      });
      fs.writeFileSync(path.join(directory, group + '-stdout.log'), result.stdout);
      fs.writeFileSync(path.join(directory, group + '-stderr.log'), result.stderr);
      if (result.code !== 0 || result.signal) throw Error(group + ' qualification failed; see captured logs');
      const summary = JSON.parse(result.stdout.trim());
      if (summary.status !== 'pass' || !fs.existsSync(summary.manifest)) throw Error('Missing qualification manifest');
      record.stages.push({ group, command: [process.execPath, ...args], status: 'pass', manifest: summary.manifest, manifest_sha256: hash(summary.manifest) }); save();
    }
    for (const source of record.fixture_sources) if (hash(path.join(repository, source.path)) !== source.sha256) throw Error('Qualification source changed during execution');
    record.status = 'pass'; record.exit_code = 0;
  } catch (error) { record.status = 'fail'; record.exit_code = 1; record.reason = error.message; }
  record.ended_at = new Date().toISOString(); save();
  console.log(JSON.stringify({ status: record.status, reason: record.reason, manifest })); process.exitCode = record.exit_code;
}
main(process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
