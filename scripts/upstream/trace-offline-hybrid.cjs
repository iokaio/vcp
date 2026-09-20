// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path');
const { run, validateExecution } = require('./trace-offline-vectors.cjs');
const { blocked } = require('../../src/tests/support/offline-embeddings.cjs');
const { digest, runSuite, parseSelection, writeManifest } = require('../../src/tests/support/harness.cjs');
const bytes = fs.readFileSync(path.resolve(__dirname, '../../src/tests/fixtures/local-memory/corpus.json'));
const corpus = JSON.parse(bytes);
function validate(mode, nonce, attempt, report, outcome, modelHash) {
  if (mode !== 'canonical') validateExecution(mode, attempt, outcome);
  if (mode.startsWith('control-')) {
    if (report.status !== 'pass' || report.network?.outcome !== 'connected' || report.network.nonce !== nonce)
      throw Error('Hybrid control did not connect');
    return;
  }
  if (mode !== 'canonical') {
    blocked(report.network_before, nonce);
    blocked(report.network_after, nonce);
  }
  if (mode === 'missing' || mode === 'corrupt') {
    if (report.status !== 'fail' || report.stage !== 'load_local_embedding' || report.failures?.length !== 1 || report.load_us !== undefined)
      throw Error('Hybrid fault did not reject at local model loading');
    return;
  }
  if (mode === 'inference') {
    const bundle = report.bundle;
    if (report.status !== 'pass' || report.phase !== 'hybrid-embedding-export' ||
        report.corpus_sha256 !== digest(bytes) || report.failures?.length !== 0 ||
        !Number.isSafeInteger(report.load_us) || report.load_us < 0 ||
        bundle?.schema_version !== 1 || bundle.corpus_sha256 !== digest(bytes) ||
        !/^[a-f0-9]{64}$/.test(bundle.specification) || !Array.isArray(bundle.rows) ||
        bundle.rows.length < corpus.documents.filter(d => d.current).length || bundle.rows.length > 1024 ||
        bundle.queries?.length !== corpus.queries.length)
      throw Error('Incomplete denied-network embedding export');
    const validVector = vector => Array.isArray(vector) && vector.length === 384 &&
      vector.every(Number.isFinite) && Math.abs(vector.reduce((n, x) => n + x*x, 0) - 1) <= 0.001;
    const chunks = new Set();
    for (const row of bundle.rows) {
      const document = corpus.documents.find(d => d.id === row.identity?.source && d.current);
      const identity = row.identity;
      if (!document || identity.scope?.workspace !== document.workspace || identity.scope?.task !== document.id ||
          identity.scope?.session !== 'hybrid-fixture' || identity.source_digest !== digest(Buffer.from(document.text)) ||
          identity.specification !== bundle.specification || !Number.isSafeInteger(identity.start) ||
          !Number.isSafeInteger(identity.end) || identity.start < 0 || identity.end <= identity.start ||
          identity.end > Buffer.byteLength(document.text) || identity.end - identity.start > 192 ||
          identity.content_digest !== digest(Buffer.from(document.text).subarray(identity.start, identity.end)) ||
          !/^[a-f0-9]{64}$/.test(identity.id) || chunks.has(identity.id) || !validVector(row.vector))
        throw Error('Invalid exported embedding source identity/vector');
      chunks.add(identity.id);
    }
    for (const [index, row] of bundle.queries.entries()) {
      const test = corpus.queries[index];
      if (row.id !== test.id || row.workspace !== test.workspace || row.text_sha256 !== digest(Buffer.from(test.text)) ||
          !validVector(row.vector) || !Number.isSafeInteger(row.inference_us) || row.inference_us < 0)
        throw Error('Invalid exported query embedding');
    }
    return;
  }
  if (report.status !== 'pass' || report.phase !== 'hybrid-quality' || report.corpus_sha256 !== digest(bytes) ||
      report.top_k !== 3 || report.specification?.assets !== modelHash || report.failures?.length !== 0 ||
      report.queries?.length !== 126 || report.scope_checks?.length !== 42 || report.builds?.length !== 4 || report.timings?.length !== 4)
    throw Error('Incomplete hybrid qualification');
  const seen = new Set();
  const recall = (found, expected) => expected.length ? expected.filter(id => found.includes(id)).length / expected.length : null;
  for (const row of report.queries) {
    const test = corpus.queries.find(test => test.id === row.query);
    const key = [row.backend, row.query, row.reader_repetition, row.warm_repetition].join('/');
    if (!test || !['files', 'sqlite'].includes(row.backend) || seen.has(key) ||
        ![0, 1, 2].includes(row.reader_repetition) || ![0, 1, 2].includes(row.warm_repetition) ||
        row.authorized !== true || row.ann_mode !== 'Ann' || row.ann_oracle_recall_at_3 !== 1 ||
        row.ann_chunk_ids?.length !== 3 || new Set(row.ann_chunk_ids).size !== 3 ||
        row.oracle_chunk_ids?.length !== 3 || row.oracle_chunk_ids.some(id => !row.ann_chunk_ids.includes(id)))
      throw Error('Incomplete hybrid query or exhaustive oracle observation');
    seen.add(key);
    for (const name of ['lexical', 'vector', 'fused']) {
      const metrics = row[name], found = metrics?.source_ids;
      if (!Array.isArray(found) || found.length > 3 || new Set(found).size !== found.length ||
          found.some(id => !corpus.documents.some(d => d.id === id && d.current && d.workspace === test.workspace)) ||
          metrics.lexical_label_recall_at_3 !== recall(found, test.lexical_required) ||
          metrics.semantic_label_recall_at_3 !== recall(found, test.semantic_required) ||
          metrics.union_label_recall_at_3 !== recall(found, [...new Set([...test.lexical_required, ...test.semantic_required])]))
        throw Error('Hybrid labelled metrics differ from the unchanged truth set');
    }
    for (const name of ['inference_us', 'hybrid_us', 'lexical_us', 'vector_us'])
      if (!Number.isSafeInteger(row[name]) || row[name] < 0) throw Error('Missing bounded timing observation');
  }
  const scopes = new Set();
  for (const row of report.scope_checks) {
    const key = [row.backend, row.query, row.reader_repetition].join('/');
    if (!corpus.queries.some(test => test.id === row.query) || scopes.has(key) ||
        !['files', 'sqlite'].includes(row.backend) || ![0, 1, 2].includes(row.reader_repetition) ||
        row.narrow_nonempty_exact_task !== true || row.foreign_workspace_denied !== true || row.empty_scope_no_passages !== true)
      throw Error('Incomplete current-scope qualification');
    scopes.add(key);
  }
}
async function afterNetwork({ repository, directory, binary, modelHash, record, save, signal, home }) {
  const exported = record.stages.find(stage => stage.id === 'inference')?.result?.bundle;
  if (!exported) throw Error('Denied-network embedding export missing');
  const bundlePath = path.join(directory, 'private', 'embedding-bundle.json');
  writeManifest(bundlePath, exported);
  const bundleBytes = fs.readFileSync(bundlePath);
  if (bundleBytes.length > 2 * 1024 * 1024) throw Error('Embedding handoff exceeds byte bound');
  const bundleHash = digest(bundleBytes);
  record.boundary = { inference: 'all model loading and source/query embedding in existing zero-capability AppContainer',
    canonical: 'trusted owner; no model load/inference; canonical store guards unchanged',
    handoff_sha256: bundleHash, handoff_bytes: bundleBytes.length };
  save();
  const registry = { schema_version: 1, suites: { hybrid: ['canonical'] }, cases: { canonical: {
    args: ['scripts/upstream/trace-offline-hybrid.cjs', '--canonical-worker', binary, bundlePath, bundleHash],
    task_ids: ['P5-06'], backends: ['none'], requires: [], timeout_ms: 180000, max_output_bytes: 2*1024*1024
  } } };
  const run = await runSuite({ root: repository, registry, selection: parseSelection(['--suite','hybrid'],registry),
    outputRoot: path.join(directory,'stages'), source: record.source, signal, isolation: { homeRoot: home },
    announce: () => console.log('Hybrid canonical publication/query stage: trusted owner, validated local vectors') });
  const attempt = run.manifest.attempts[0];
  const stage = { id: 'canonical', status: run.manifest.status, exit_code: run.exitCode,
    manifest: path.relative(directory,run.manifestPath).split(path.sep).join('/') };
  record.stages.push(stage); save();
  const stdout = attempt?.artifacts?.find(a => a.path.endsWith('-stdout.log'));
  if (stdout) stage.result = JSON.parse(fs.readFileSync(path.join(path.dirname(run.manifestPath),stdout.path),'utf8').trim());
  save();
  if (!attempt || attempt.exit_code !== 0 || !attempt.capture_complete || attempt.status !== 'pass' ||
      stage.result?.embedding_bundle_sha256 !== bundleHash) throw Error('Canonical hybrid phase failed or incomplete');
  validate('canonical',null,attempt,stage.result,null,modelHash);
}
if (process.argv[2] === '--canonical-worker') {
  const { spawnSync } = require('node:child_process');
  const [binary,bundle,hash] = process.argv.slice(3);
  if (!binary || !bundle || !/^[a-f0-9]{64}$/.test(hash || '')) throw Error('Invalid canonical worker arguments');
  const child = spawnSync(binary,['--hybrid',bundle,hash],{encoding:'utf8',maxBuffer:2*1024*1024,timeout:150000,windowsHide:true});
  if (child.stdout) process.stdout.write(child.stdout);
  if (child.stderr) process.stderr.write(child.stderr);
  if (child.error) process.stderr.write('Canonical hybrid worker failed: '+child.error.message+'\n');
  process.exitCode = child.status === 0 && !child.error ? 0 : 1;
} else run(process.argv.slice(2), {
  taskId: 'P5-06', validate, timeoutMs: 180000,
  afterNetwork,
  inputs: ['scripts/upstream/trace-offline-hybrid.cjs', 'src/crates/vcp-memory/examples/hybrid_quality.rs',
    'src/crates/vcp-memory/src/retrieval.rs', 'src/crates/vcp-memory/src/publication.rs',
    'src/crates/vcp-memory/src/search_record.rs', 'src/crates/vcp-memory/src/lexical.rs'],
  limitation: 'Composite boundary: real source/query embeddings under OS network denial; trusted canonical publication/hybrid retrieval consumes validated bounded vectors. No claim that the canonical process is network blocked; no production quality or resource-envelope guarantee.'
}).then(code => { process.exitCode = code; }).catch(error => { console.error(error.message); process.exitCode = 2; });
