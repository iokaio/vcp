// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { digest, sourceIdentity, writeManifest, runSuite, parseSelection } = require('../../src/tests/support/harness.cjs');
const { outside } = require('../../src/tests/support/model-assets.cjs');
const { parseArgs, validateBuild, validateQuery, expectedRejection } = require('../../src/tests/support/local-memory.cjs');
// The existing harness owns the Node wrapper and its native descendant as one
// process tree, including bounded output capture and cancellation.
const native = "const r=require('node:child_process').spawnSync(process.argv[1],process.argv.slice(2),{stdio:'inherit',windowsHide:true});process.exit(r.error||r.signal||r.status===null?1:r.status)";
async function main(args) {
  const options = parseArgs(args), repository = path.resolve(__dirname, '../..');
  const binary = path.resolve(options['--binary']);
  const assets = outside(options['--assets'], [repository]);
  const output = outside(options['--output-root'], [path.join(repository, 'src'), assets]);
  const directory = path.join(output, crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, 'manifest.json');
  const record = { schema_version: 1, task_id: 'P0-02', status: 'prepared', started_at: new Date().toISOString(), stages: [],
    limitations: ['Small fixed synthetic corpus only; no production scale or OS network-denial qualification.', 'Canonical visibility is replayed through volatile Munarium governance, not a durable VCP store.'] };
  const save = () => writeManifest(file, record);
  const controller = new AbortController(), interrupt = () => controller.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  save();
  try {
    if (process.platform !== 'win32' || !fs.existsSync(binary) || !fs.statSync(binary).isFile()) { record.status = 'not_run'; record.reason = 'Built native Windows memory-spike executable required'; record.exit_code = 3; return 3; }
    record.source = await sourceIdentity(repository, controller.signal);
    record.binary_sha256 = digest(fs.readFileSync(binary));
    record.inputs = ['scripts/upstream/trace-local-memory.cjs', 'src/tests/support/local-memory.cjs', 'src/tests/support/harness.cjs',
      'src/crates/vcp-memory-spike/src/main.rs', 'src/crates/vcp-memory-spike/src/governance.rs', 'src/tests/fixtures/local-memory/corpus.json'].map(relative => ({ path: relative, sha256: digest(fs.readFileSync(path.join(repository, relative))) }));
    const corpusBytes = fs.readFileSync(path.join(repository, 'src/tests/fixtures/local-memory/corpus.json'));
    const corpus = JSON.parse(corpusBytes);
    const modelHash = digest(fs.readFileSync(path.join(repository, 'src/third_party/components/minilm-assets.json')));
    const home = path.join(directory, 'profile'); fs.mkdirSync(home);
    const index = path.join(directory, 'index');
    async function phase(id, argv, rejection) {
      const registry = { schema_version: 1, suites: { memory: [id] }, cases: { [id]: {
        args: ['-e', native, binary, ...argv], task_ids: ['P0-02'], backends: ['none'], requires: [], timeout_ms: 300000, max_output_bytes: 1024 * 1024
      } } };
      const run = await runSuite({ root: repository, registry, selection: parseSelection(['--suite', 'memory'], registry),
        outputRoot: path.join(directory, 'stages'), source: record.source, signal: controller.signal, isolation: { homeRoot: home },
        announce: () => console.log('Local-memory stage: ' + id) });
      const attempt = run.manifest.attempts[0], root = path.dirname(run.manifestPath);
      record.stages.push({ id, status: run.manifest.status, exit_code: run.exitCode, manifest: path.relative(directory, run.manifestPath).split(path.sep).join('/') }); save();
      const stdout = attempt.artifacts?.find(item => item.path.endsWith('-stdout.log'));
      const stderr = attempt.artifacts?.find(item => item.path.endsWith('-stderr.log'));
      const diagnostic = stderr ? fs.readFileSync(path.join(root, stderr.path), 'utf8') : '';
      if (rejection) {
        expectedRejection(attempt, diagnostic, rejection);
        record.stages.at(-1).expected_rejection = true; save(); return;
      }
      if (run.exitCode === 3 && attempt.reason === 'child_failed' && diagnostic.includes('missing_asset')) throw Object.assign(Error('Verified local model assets are missing'), { notRun: true });
      if (run.exitCode !== 0 || run.manifest.status !== 'pass' || !stdout) throw Error(id + ' did not pass');
      return JSON.parse(fs.readFileSync(path.join(root, stdout.path), 'utf8'));
    }
    record.status = 'running'; save();
    const built = await phase('build', ['build', assets, index]);
    const receipt = validateBuild(built, corpus, digest(corpusBytes), modelHash);
    if (digest(fs.readFileSync(path.join(index, 'receipt.json'))) !== receipt) throw Error('Build receipt bytes differ from result');
    const queried = await phase('reopen', ['query', assets, index, receipt]);
    const rows = validateQuery(queried, corpus);
    const repeated = await phase('second-reopen', ['query', assets, index, receipt]);
    if (JSON.stringify(rows) !== JSON.stringify(validateQuery(repeated, corpus))) throw Error('Fresh process results changed');
    await phase('wrong-receipt', ['query', assets, index, '0'.repeat(64)], 'receipt identity mismatch');
    const corrupt = path.join(directory, 'corrupt-index'); fs.cpSync(index, corrupt, { recursive: true, errorOnExist: true, force: false });
    const marker = path.join(corrupt, 'atlas', 'manifest.json'); const bytes = fs.readFileSync(marker); bytes[0] ^= 1; fs.writeFileSync(marker, bytes);
    await phase('corrupt-manifest', ['query', assets, corrupt, receipt], 'integrity:');
    record.build = built; record.query = queried; record.repeat_query = repeated;
    record.status = 'pass'; record.exit_code = 0; return 0;
  } catch (error) { record.status = error.notRun ? 'not_run' : 'fail'; record.reason = error.message; record.exit_code = controller.signal.aborted ? 130 : error.notRun ? 3 : 1; return record.exit_code; }
  finally { record.ended_at = new Date().toISOString(); save(); process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
    console.log(JSON.stringify({ status: record.status, manifest: file })); }
}
main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(error => { console.error(error.message); process.exitCode = 2; });
