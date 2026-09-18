// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { generateCorpus, validateMemory, validateBatches, validateTimings, validateScaleQuery, treeUsage, validateDisk } = require('../support/memory-resources.cjs');
const base = require('../fixtures/local-memory/corpus.json'), spec = require('../fixtures/local-memory/scales.json');
const corpus = generateCorpus(base, spec, 100);
function memory() { return { samples: 20, requested_interval_ms: 50, maximum_sample_gap_ms: 65,
  peak_resident_bytes: 200000000, peak_private_commit_bytes: 240000000, peak_sampled_mapped_address_bytes: 2000000,
  peak_sampled_image_address_bytes: 20000000, last: { resident_bytes: 30000000, private_commit_bytes: 20000000,
    committed_mapped_address_bytes: 1000000, committed_image_address_bytes: 20000000 } }; }
const batches = () => [16, 16, 16, 2].map(items => ({ items, inference_us: 10 }));
function query() {
  return { status: 'pass', phase: 'query', memory: memory(), governance_ms: 20, load_ms: 500, process_work_ms: 2000, warm_repetitions: 5,
    governance: { backend: 'munarium-store-mem', durable: false, workspaces: spec.workspaces.map(workspace => ({ workspace,
      recorded: 50, current_ids: corpus.documents.filter(d => d.workspace === workspace && d.current).map(d => d.id).sort(),
      historical_checks: workspace === 'atlas' ? 1 : 0, findings: 0 })) },
    workspaces: spec.workspaces.map(workspace => ({ workspace, records: 50, reopen_ms: 5, oracle_inference_ms: 100, oracle_batches: batches() })),
    queries: Array.from({ length: 6 }, (_, repetition) => corpus.queries.map(row => ({ case: row.id, workspace: row.workspace,
      lexical: row.lexical_required, semantic: row.semantic_required, ann_recall_at_3: 1, reopen_ms: 5, inference_us: 10,
      query_us: 20, oracle_compare_us: 5, repetition }))).flat() };
}
test('declared scale fixtures preserve original truth and freeze three distinct dataset identities', () => {
  const hashes = ['b7607f1198a98c8898912545aef31fe31885d8c4f938c1ced5fe4ea55667a9f0',
    '87edb8318f5ea2a68330fd8812d6554eb996726c39ce0738bcd4301fdcb187ef', '410eded2229bb3f16ee64038ff6e705abdd045524f540a0e38eaae81e1af01ac'];
  for (const [index, count] of spec.records.entries()) {
    const generated = generateCorpus(base, spec, count);
    assert.equal(generated.documents.length, count); assert.equal(generated.queries.length, 9);
    assert.deepEqual(generated.documents.slice(0, 24), base.documents); assert.deepEqual(generated.queries.slice(0, 7), base.queries);
    for (const workspace of spec.workspaces) assert.equal(generated.documents.filter(d => d.workspace === workspace).length, count / 2);
    assert.equal(new Set(generated.documents.map(d => d.id)).size, count);
    assert.equal(crypto.createHash('sha256').update(JSON.stringify(generated, null, 2) + '\n').digest('hex'), hashes[index]);
  }
  assert.throws(() => generateCorpus(base, spec, 200));
  assert.throws(() => generateCorpus(base, { ...spec, seed: 1 }, 100));
});
test('resource evidence rejects absent peaks, invalid sampling and leftover scratch files', () => {
  validateMemory(query());
  for (const mutation of [{ samples: 0 }, { requested_interval_ms: 100 }, { peak_resident_bytes: 0 },
    { peak_private_commit_bytes: 1 }, { peak_sampled_mapped_address_bytes: NaN }, { maximum_sample_gap_ms: -1 }]) {
    const result = query(); Object.assign(result.memory, mutation); assert.throws(() => validateMemory(result));
  }
  const disk = { requested_interval_ms: 100, samples: 10, maximum_sample_gap_ms: 120, peak_sampled_total_bytes: 200,
    peak_sampled_scratch_bytes: 50, retained_index_bytes: 160, final_scratch_entries: 0 };
  validateDisk(disk);
  for (const mutation of [{ final_scratch_entries: 1 }, { peak_sampled_total_bytes: 100 }, { samples: 1 }, { maximum_sample_gap_ms: NaN }])
    assert.throws(() => validateDisk({ ...disk, ...mutation }));
});
test('all first and warm query repetitions must retain independent scope, relevance and recall checks', () => {
  assert.equal(validateScaleQuery(query(), corpus).length, 9);
  for (const mutate of [r => r.queries.pop(), r => { r.queries[0].repetition = 1; },
    r => { r.queries[0].ann_recall_at_3 = 0.9; }, r => { r.queries[0].lexical = ['boreal-pause']; },
    r => { r.queries[0].oracle_compare_us = NaN; }, r => { r.warm_repetitions = 4; },
    r => { r.queries[9].semantic = ['atlas-budget']; }]) {
    const result = query(); mutate(result); assert.throws(() => validateScaleQuery(result, corpus));
  }
});
test('batch and workspace timings cannot omit model work or report oracle work as absent', () => {
  validateBatches(batches(), 50); validateTimings(query(), corpus);
  assert.throws(() => validateBatches(batches().slice(1), 50));
  assert.throws(() => validateBatches([{ items: 50, inference_us: 10 }], 50));
  for (const mutate of [r => r.workspaces.pop(), r => { r.workspaces[1].workspace = 'atlas'; },
    r => { delete r.workspaces[0].oracle_inference_ms; }, r => { r.workspaces[0].oracle_batches[0].items = 15; }]) {
    const result = query(); mutate(result); assert.throws(() => validateTimings(result, corpus));
  }
});
test('disk observation counts owned files and rejects links instead of following other roots', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const index = path.join(fixture.root, 'index'), other = path.join(fixture.root, 'other');
    fs.mkdirSync(index); fs.mkdirSync(other); fs.mkdirSync(path.join(index, 'nested'));
    fs.writeFileSync(path.join(index, 'a'), 'abc'); fs.writeFileSync(path.join(index, 'nested/b'), 'defg');
    assert.deepEqual(treeUsage(index), { bytes: 7, files: 2, entries: 3 });
    assert.deepEqual(treeUsage(path.join(index, 'missing')), { bytes: 0, files: 0, entries: 0 });
    const alias = path.join(index, 'alias'); fs.symlinkSync(other, alias, process.platform === 'win32' ? 'junction' : 'dir');
    try { assert.throws(() => treeUsage(index), /link/); assert.throws(() => treeUsage(alias), /regular directory/); }
    finally { fs.unlinkSync(alias); }
  } finally { fixture.cleanup(); }
});

test('missing resource-gate model assets are not_run before launching any native phase', t => {
  if (process.platform !== 'win32') return t.skip('Native Windows prerequisite path');
  const fixture = ownedRoot(os.tmpdir());
  try {
    const result = spawnSync(process.execPath, [path.resolve(__dirname, '../../../scripts/upstream/trace-memory-resources.cjs'),
      '--binary', process.execPath, '--assets', path.join(fixture.root, 'missing-model'), '--output-root', path.join(fixture.root, 'evidence')],
    { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 3, result.stderr);
    const summary = JSON.parse(result.stdout), record = JSON.parse(fs.readFileSync(summary.manifest));
    assert.equal(record.status, 'not_run'); assert.deepEqual(record.stages, []);
    assert.match(record.reason, /model assets are missing/);
  } finally { fixture.cleanup(); }
});
