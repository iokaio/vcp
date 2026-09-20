// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const os = require('node:os'), net = require('node:net');
const { digest, sourceIdentity, writeManifest, runSuite, parseSelection } = require('../../src/tests/support/harness.cjs');
const { outside, specification, verify } = require('../../src/tests/support/model-assets.cjs');
const { parseArgs } = require('../../src/tests/support/local-memory.cjs');
const { blocked, validateTraffic } = require('../../src/tests/support/offline-embeddings.cjs');

// Independent parent checks: the child cannot self-attest its token or traffic.
function validateExecution(mode, attempt, outcome) {
  const control = mode.startsWith('control-'), fault = ['missing', 'corrupt'].includes(mode);
  const expectedExit = fault ? 1 : 0;
  if (!attempt || attempt.exit_code !== expectedExit || attempt.capture_complete !== true ||
      attempt.status !== (fault ? 'fail' : 'pass') || attempt.reason !== (fault ? 'child_failed' : null) ||
      outcome?.cleanup !== 'completed' || outcome.result?.ExitCode !== expectedExit ||
      outcome.result.AppContainer !== !control || outcome.result.CapabilityCount !== 0 ||
      outcome.result.TokenSidMatchesProfile !== !control ||
      !Number.isSafeInteger(outcome.result.PeakJobCommittedBytes) || outcome.result.PeakJobCommittedBytes <= 0 ||
      !Number.isSafeInteger(outcome.result.WallMilliseconds) || outcome.result.WallMilliseconds < 0)
    throw Error('Incomplete process, capture, resource or cleanup evidence');
}
function validate(mode, nonce, attempt, result, outcome, modelHash, stderr) {
  validateExecution(mode, attempt, outcome);
  const control = mode.startsWith('control-'), fault = ['missing', 'corrupt'].includes(mode);
  if (control) {
    if (result?.status !== 'pass' || result.phase !== 'network-control' ||
        result.observation?.outcome !== 'connected' || result.observation.nonce !== nonce)
      throw Error('Control did not connect');
  } else if (fault) {
    blocked(result?.network_before, nonce);
    if (result.status !== 'error' || result.failure?.kind !== 'local_embedding_load_failed' ||
        result.failure.stage !== 'load' || !stderr.includes('local_embedding_load_failed'))
      throw Error('Fault did not reject at actual local model loading');
  } else {
    blocked(result?.network?.before, nonce); blocked(result?.network?.after, nonce);
    const corpus = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../src/tests/fixtures/local-memory/corpus.json')));
    if (result.status !== 'pass' || result.phase !== 'vector-quality' || result.device !== 'cpu' ||
        result.dimensions !== 384 || result.vector_engine !== 'diskann' || result.top_k !== 3 ||
        result.ann_minimum_recall !== 1 || result.cases !== 7 || result.repetitions !== 3 ||
        result.asset_spec_sha256 !== modelHash || result.queries?.length !== 21 || result.builds?.length !== 2 ||
        result.scope_checks?.length !== 7 || result.corpus_sha256 !== digest(fs.readFileSync(path.resolve(__dirname, '../../src/tests/fixtures/local-memory/corpus.json'))) ||
        JSON.stringify(result.checks_passed) !== JSON.stringify(['real_cpu_embedding','embedding_cache','ann_vs_exhaustive','component_reopen','source_authorization','foreign_workspace','corrupt_component','incompatible_dimensions']))
      throw Error('Incomplete actual memory adapter qualification');
    const seen = new Set();
    for (const row of result.queries) {
      const test = corpus.queries.find(test => test.id === row.query);
      const key = row.query + '/' + row.repetition;
      if (!test || test.workspace !== row.workspace || ![0,1,2].includes(row.repetition) || seen.has(key) || row.mode !== 'ann' ||
          row.chunk_ids?.length !== 3 || new Set(row.chunk_ids).size !== 3 || row.oracle_ids?.length !== 3 ||
          row.oracle_ids.some(id => !row.chunk_ids.includes(id)) || row.ann_recall_at_3 !== 1 ||
          !Array.isArray(row.source_ids) || row.source_ids.length < 1 ||
          row.source_ids.some(id => !corpus.documents.some(doc => doc.id === id && doc.workspace === row.workspace && doc.current)) ||
          !Number.isSafeInteger(row.query_us) || row.query_us < 0) throw Error('Invalid scoped ANN/oracle observation');
      seen.add(key);
    }
    const scopes = new Set();
    for (const row of result.scope_checks) {
      if (!corpus.queries.some(test => test.id === row.query) || scopes.has(row.query) || row.source_filter !== 'pass' ||
          row.empty_scope !== 'pass' || row.foreign_workspace !== 'rejected' ||
          !['ann','exact_authorized_subset','reduced_recall'].includes(row.filtered_mode)) throw Error('Missing scope validation');
      scopes.add(row.query);
    }
    for (const row of result.builds) {
      const sources = corpus.documents.filter(doc => doc.workspace === row.workspace && doc.current).length;
      if (!sources || row.sources !== sources || row.chunks < sources || row.chunks > 1024 ||
          !/^[a-f0-9]{64}$/.test(row.component_checksum)) throw Error('Incomplete component build/reopen identity');
    }
  }
}

async function main(args, qualification = {}) {
  const validateResult = qualification.validate || validate;
  const taskId = qualification.taskId || 'P5-04';
  const options = parseArgs(args), repository = path.resolve(__dirname, '../..');
  const binary = path.resolve(options['--binary']), assets = outside(options['--assets'], [repository]);
  const directory = path.join(outside(options['--output-root'], [path.join(repository, 'src'), assets]), crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, 'manifest.json');
  const record = { schema_version: 1, task_id: taskId, status: 'prepared', started_at: new Date().toISOString(), stages: [],
    limitations: [qualification.limitation || 'Actual CPU embedding and DiskANN fixture under OS denial; no P5-05 activation or production resource envelope.',
      'Forced termination of the broker can leave its journaled disposable profile; pending cleanup fails qualification.'] };
  const save = () => writeManifest(file, record), controller = new AbortController(), interrupt = () => controller.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  const sockets = new Set(), received = []; let server, trafficError;
  save();
  try {
    const address = Object.values(os.networkInterfaces()).flat().find(row => row && !row.internal && row.family === 'IPv4' &&
      /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(row.address))?.address;
    if (process.platform !== 'win32' || !address || !process.env.USERPROFILE || !process.env.LOCALAPPDATA || !fs.existsSync(binary) || !fs.statSync(binary).isFile()) {
      throw Object.assign(Error('Native Windows executable, OS profile paths and same-host private IPv4 required'), { notRun: true });
    }
    const model = specification(repository); await verify(assets, model.spec);
    record.source = await sourceIdentity(repository, controller.signal);
    record.binary_sha256 = digest(fs.readFileSync(binary)); record.asset_spec_sha256 = model.sha256;
    record.node = process.version; record.windows = os.release();
    record.inputs = ['scripts/upstream/trace-offline-vectors.cjs', 'src/tests/support/offline-embeddings.cjs',
      'src/tests/support/windows/offline-embedding.ps1', 'src/tests/support/windows/AppContainerFixture.cs',
      'src/tests/support/harness.cjs', 'src/tests/support/model-assets.cjs', 'src/tests/support/local-memory.cjs',
      'src/crates/vcp-memory/examples/vector_quality.rs', 'src/crates/vcp-memory/src/embedding.rs', 'src/crates/vcp-memory/src/vector.rs', 'src/crates/vcp-embedding/src/bin/qualify/network.rs',
      'src/crates/vcp-embedding/src/lib.rs', 'src/tests/fixtures/local-memory/corpus.json', ...(qualification.inputs || [])]
      .map(relative => ({ path: relative, sha256: digest(fs.readFileSync(path.join(repository, relative))) }));
    const privateRoot = path.join(directory, 'private'); fs.mkdirSync(privateRoot);
    const home = path.join(privateRoot, 'home'); fs.mkdirSync(home);
    server = net.createServer(socket => {
      if (received.length + sockets.size >= 4) { trafficError = 'Too many canary connections'; socket.destroy(); return; }
      sockets.add(socket); let data = Buffer.alloc(0);
      socket.setTimeout(3000, () => { trafficError = 'Canary socket timed out'; socket.destroy(); });
      socket.on('data', bytes => {
        if (data.length + bytes.length > 128) { trafficError = 'Canary output exceeded bound'; socket.destroy(); }
        else data = Buffer.concat([data, bytes]);
      });
      socket.on('end', () => { received.push(data.toString('utf8')); socket.end(); });
      socket.on('error', () => { trafficError = 'Canary socket failed'; });
      socket.on('close', () => sockets.delete(socket));
    });
    server.maxConnections = 4;
    server.on('error', () => { trafficError = 'Canary listener failed'; controller.abort(); });
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, address, resolve); });
    const port = server.address().port, controls = [];
    record.status = 'running'; save();
    for (const mode of ['control-before', 'inference', 'missing', 'corrupt', 'control-after']) {
      if (controller.signal.aborted || trafficError) throw Error(trafficError || 'Qualification cancelled');
      const nonce = crypto.randomBytes(16).toString('hex');
      if (mode.startsWith('control-')) controls.push(nonce);
      const config = path.join(privateRoot, mode + '.json'), outcomeFile = path.join(privateRoot, mode + '-outcome.json');
      writeManifest(config, { mode, nonce, address, port, binary, assets, files: model.spec.files, outcome: outcomeFile,
        worker: path.join(repository, 'src/tests/support/windows/offline-embedding.ps1'),
        userProfile: process.env.USERPROFILE, localAppData: process.env.LOCALAPPDATA });
      const registry = { schema_version: 1, suites: { offline: [mode] }, cases: { [mode]: {
        args: ['src/tests/support/offline-embeddings.cjs', config], task_ids: [taskId], backends: ['none'], requires: [],
        timeout_ms: qualification.timeoutMs || 90000, max_output_bytes: 1024 * 1024
      } } };
      const run = await runSuite({ root: repository, registry, selection: parseSelection(['--suite', 'offline'], registry),
        outputRoot: path.join(directory, 'stages'), source: record.source, signal: controller.signal, isolation: { homeRoot: home },
        sensitiveValues: [address, assets, process.env.USERPROFILE], announce: () => console.log('Offline memory vector stage: ' + mode) });
      const attempt = run.manifest.attempts[0], root = path.dirname(run.manifestPath);
      const stage = { id: mode, status: run.manifest.status, exit_code: run.exitCode,
        manifest: path.relative(directory, run.manifestPath).split(path.sep).join('/') };
      record.stages.push(stage); save();
      const readLog = kind => { const artifact = attempt?.artifacts?.find(item => item.path.endsWith('-' + kind + '.log'));
        return artifact ? fs.readFileSync(path.join(root, artifact.path), 'utf8').trim() : ''; };
      const outcome = fs.existsSync(outcomeFile) ? JSON.parse(fs.readFileSync(outcomeFile, 'utf8').replace(/^\uFEFF/, '')) : null;
      stage.cleanup = outcome?.cleanup || 'unconfirmed'; save();
      const result = JSON.parse(readLog('stdout'));
      // Failed runs retain their actual observations before acceptance checks.
      stage.process = outcome?.result || null; stage.result = result; save();
      validateResult(mode, nonce, attempt, result, outcome, model.sha256, readLog('stderr'));
      if (['missing', 'corrupt'].includes(mode)) stage.expected_rejection = true;
      save();
    }
    // Stop accepting and drain existing sockets under their deadlines before
    // deciding whether the independent observer saw both controls.
    await new Promise(resolve => server.close(resolve));
    if (trafficError || sockets.size) throw Error(trafficError || 'Unclosed canary sockets');
    validateTraffic(received, controls);
    record.traffic = { expected_controls: 2, observed_connections: received.length, restricted_connections: 0 };
    if (qualification.afterNetwork) {
      await qualification.afterNetwork({ repository, directory, binary, modelHash: model.sha256,
        record, save, signal: controller.signal, home });
    }
    record.status = 'pass'; record.exit_code = 0; return 0;
  } catch (error) {
    record.status = error.notRun ? 'not_run' : 'fail'; record.reason = error.message;
    record.exit_code = controller.signal.aborted ? 130 : error.notRun ? 3 : 1; return record.exit_code;
  } finally {
    for (const socket of sockets) socket.destroy();
    if (server?.listening) await new Promise(resolve => server.close(resolve));
    record.ended_at = new Date().toISOString(); save();
    process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
    console.log(JSON.stringify({ status: record.status, manifest: file }));
  }
}
module.exports = { run: main, validateExecution };
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(error => { console.error(error.message); process.exitCode = 2; });
