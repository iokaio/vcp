// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), os = require('node:os');
const { digest, sourceIdentity, writeManifest, runSuite, parseSelection } = require('../../src/tests/support/harness.cjs');
const { outside, specification, verify } = require('../../src/tests/support/model-assets.cjs');
const { parseArgs, validateBuild } = require('../../src/tests/support/local-memory.cjs');
const { generateCorpus, validateTimings, validateScaleQuery, treeUsage, validateDisk } = require('../../src/tests/support/memory-resources.cjs');
// The harness supplies the allowlisted environment and owns this wrapper and
// native child as one bounded process tree. Native temp indexes have a dedicated
// scratch root so disk observation includes both retained and temporary files.
const native = "const r=require('node:child_process').spawnSync(process.argv[1],process.argv.slice(3),{env:{...process.env,TEMP:process.argv[2],TMP:process.argv[2]},stdio:'inherit',windowsHide:true});process.exit(r.error||r.signal||r.status===null?1:r.status)";
async function main(args) {
  const options = parseArgs(args), repository = path.resolve(__dirname, '../..');
  const binary = path.resolve(options['--binary']), assets = outside(options['--assets'], [repository]);
  const directory = path.join(outside(options['--output-root'], [path.join(repository, 'src'), assets]), crypto.randomUUID());
  fs.mkdirSync(directory, { recursive: true });
  const manifest = path.join(directory, 'manifest.json');
  const record = { schema_version: 1, task_id: 'P0-02', status: 'prepared', started_at: new Date().toISOString(), stages: [],
    limitations: ['Synthetic scaling and fresh processes; OS file caches are not purged.',
      'Mapped-address and disk peaks are sampled lower bounds; native OS resident/private peaks are read through the final sample.',
      'Volatile governance replay and whole-corpus candidate filtering are prototype costs, not durable product recovery.',
      'CPU inference network denial is qualified separately; no product hardware or installation support claim.'] };
  const save = () => writeManifest(manifest, record), controller = new AbortController(), interrupt = () => controller.abort();
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt); save();
  try {
    if (process.platform !== 'win32' || !fs.existsSync(binary) || !fs.statSync(binary).isFile()) throw Object.assign(Error('Built native Windows corpus executable required'), { notRun: true });
    const model = specification(repository);
    try { record.assets = await verify(assets, model.spec); }
    catch (error) {
      if (error.code === 'ENOENT') throw Object.assign(Error('Verified local model assets are missing'), { notRun: true });
      throw error;
    }
    record.asset_spec_sha256 = model.sha256; record.binary_sha256 = digest(fs.readFileSync(binary)); record.binary_bytes = fs.statSync(binary).size;
    record.source = await sourceIdentity(repository, controller.signal);
    record.host = { platform: process.platform, release: os.release(), architecture: process.arch, node: process.version,
      cpu_model: os.cpus()[0]?.model, logical_cpus: os.cpus().length, total_memory_bytes: os.totalmem() };
    record.inputs = ['scripts/upstream/trace-memory-resources.cjs', 'src/tests/support/memory-resources.cjs',
      'src/tests/support/harness.cjs', 'src/tests/support/local-memory.cjs', 'src/tests/support/model-assets.cjs',
      'src/crates/vcp-memory-spike/src/main.rs', 'src/crates/vcp-memory-spike/src/governance.rs', 'src/crates/vcp-memory-spike/src/resources.rs',
      'src/crates/vcp-embedding/src/lib.rs', 'src/tests/fixtures/local-memory/corpus.json', 'src/tests/fixtures/local-memory/scales.json']
      .map(relative => ({ path: relative, sha256: digest(fs.readFileSync(path.join(repository, relative))) }));
    const base = JSON.parse(fs.readFileSync(path.join(repository, 'src/tests/fixtures/local-memory/corpus.json')));
    const spec = JSON.parse(fs.readFileSync(path.join(repository, 'src/tests/fixtures/local-memory/scales.json')));
    const home = path.join(directory, 'profile'); fs.mkdirSync(home);
    record.status = 'running'; save();
    for (const count of [100, 1000, 10000]) {
      const corpus = generateCorpus(base, spec, count), corpusFile = path.join(directory, 'corpus-' + count + '.json');
      const corpusBytes = JSON.stringify(corpus, null, 2) + '\n'; fs.writeFileSync(corpusFile, corpusBytes, { flag: 'wx' });
      const index = path.join(directory, 'index-' + count); let receipt, previousRows;
      for (const phase of ['build', 'reopen', 'second-reopen']) {
        const id = phase + '-' + count, scratch = path.join(directory, 'scratch-' + id); fs.mkdirSync(scratch);
        const registry = { schema_version: 1, suites: { resources: [id] }, cases: { [id]: {
          args: ['-e', native, binary, scratch, phase === 'build' ? 'build' : 'query', assets, index,
            ...(phase === 'build' ? [] : [receipt]), '--corpus', corpusFile],
          task_ids: ['P0-02'], backends: ['none'], requires: [], timeout_ms: spec.phase_timeout_ms, max_output_bytes: 2 * 1024 * 1024
        } } };
        const disk = { samples: 0, requested_interval_ms: spec.disk_sample_interval_ms,
          peak_sampled_total_bytes: 0, peak_sampled_scratch_bytes: 0, maximum_sample_gap_ms: 0 };
        let previous = performance.now(), samplingError;
        const sample = () => {
          try {
            const current = performance.now(); disk.maximum_sample_gap_ms = Math.max(disk.maximum_sample_gap_ms, current - previous); previous = current;
            const retained = treeUsage(index), temporary = treeUsage(scratch); disk.samples++;
            disk.peak_sampled_total_bytes = Math.max(disk.peak_sampled_total_bytes, retained.bytes + temporary.bytes);
            disk.peak_sampled_scratch_bytes = Math.max(disk.peak_sampled_scratch_bytes, temporary.bytes);
            disk.retained_index_bytes = retained.bytes; disk.final_scratch_entries = temporary.entries;
          } catch (error) { samplingError = error; controller.abort(); }
        };
        sample(); const timer = setInterval(sample, spec.disk_sample_interval_ms); let run;
        try {
          run = await runSuite({ root: repository, registry, selection: parseSelection(['--suite', 'resources'], registry),
            outputRoot: path.join(directory, 'stages'), source: record.source, signal: controller.signal,
            isolation: { homeRoot: home }, sensitiveValues: [assets], announce: () => console.log('Local resource stage: ' + id) });
        } finally { clearInterval(timer); sample(); }
        const stage = { id, records: count, status: run.manifest.status, exit_code: run.exitCode,
          manifest: path.relative(directory, run.manifestPath).split(path.sep).join('/'), corpus_sha256: digest(corpusBytes), disk };
        record.stages.push(stage); save();
        if (samplingError) throw samplingError;
        const attempt = run.manifest.attempts[0], stdout = attempt?.artifacts?.find(item => item.path.endsWith('-stdout.log'));
        if (run.exitCode !== 0 || run.manifest.status !== 'pass' || attempt.capture_complete !== true || !stdout) throw Error(id + ' did not pass');
        const result = JSON.parse(fs.readFileSync(path.join(path.dirname(run.manifestPath), stdout.path), 'utf8'));
        stage.result = result; save();
        if (!fs.existsSync(scratch)) throw Error('Dedicated scratch root disappeared');
        validateDisk(disk); validateTimings(result, corpus);
        if (phase === 'build') {
          receipt = validateBuild(result, corpus, digest(corpusBytes), model.sha256);
          if (digest(fs.readFileSync(path.join(index, 'receipt.json'))) !== receipt) throw Error('Build receipt differs from reported bytes');
        } else {
          const rows = validateScaleQuery(result, corpus);
          if (previousRows && JSON.stringify(rows) !== JSON.stringify(previousRows)) throw Error('Fresh process query results changed');
          previousRows = rows;
        }
        stage.validated = true; save();
      }
    }
    record.status = 'pass'; record.exit_code = 0; return 0;
  } catch (error) {
    record.status = error.notRun ? 'not_run' : 'fail'; record.reason = error.message;
    record.exit_code = controller.signal.aborted ? 130 : error.notRun ? 3 : 1; return record.exit_code;
  } finally {
    record.ended_at = new Date().toISOString(); save();
    process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
    console.log(JSON.stringify({ status: record.status, manifest }));
  }
}
main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(error => { console.error(error.message); process.exitCode = 2; });
