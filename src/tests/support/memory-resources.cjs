// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { validateQuery } = require('./local-memory.cjs');
const subjects = ['amber minerals', 'ceramic pigments', 'orbital spectra', 'woven fibers', 'copper crystals', 'limestone strata', 'glass prisms', 'acoustic resonators'];
const settings = ['a dry chamber', 'a shaded enclosure', 'a rotating platform', 'a calibrated frame', 'a sealed cabinet', 'a shallow basin', 'a cooled tray', 'a vertical stand'];
function generateCorpus(base, spec, count) {
  if (spec.schema_version !== 1 || spec.generator !== 'vcp-scale-v1' || spec.seed !== 17092026 ||
      JSON.stringify(spec.records) !== '[100,1000,10000]' || JSON.stringify(spec.workspaces) !== '["atlas","boreal"]' ||
      spec.warm_repetitions !== 5 || spec.minimum_recall_at_3 !== 1 || spec.phase_timeout_ms !== 900000 ||
      spec.disk_sample_interval_ms !== 100 || spec.memory_sample_interval_ms !== 50 || !spec.records.includes(count) ||
      base.version !== 2 || base.documents.length !== 24 || base.queries.length !== 7) throw Error('Unrecognized scale fixture specification');
  const corpus = structuredClone(base);
  for (let i = base.documents.length; i < count; i++) {
    const workspace = i % 2 ? 'boreal' : 'atlas', ordinal = Math.floor((i - base.documents.length) / 2);
    const bytes = crypto.createHash('sha256').update(spec.generator + ':' + spec.seed + ':' + ordinal).digest();
    const marker = 'calibrationz' + String(ordinal).padStart(5, '0');
    const id = workspace + '-scale-' + String(ordinal).padStart(5, '0');
    corpus.documents.push({ id, workspace, current: true,
      text: `The sample_measurement function for ${marker} describes ${subjects[bytes[0] % subjects.length]} in ${settings[bytes[1] % settings.length]}. Synthetic specimen ${ordinal} has a ${workspace === 'atlas' ? 'matte' : 'glossy'} surface and ${10 + bytes[2]} bands at interval ${1 + bytes[3]}.` });
  }
  for (const workspace of spec.workspaces) corpus.queries.push({ id: workspace + '-scale-marker', workspace,
    text: 'calibrationz00000', lexical_required: [workspace + '-scale-00000'], semantic_required: [] });
  return corpus;
}
function nonnegative(value) { return Number.isSafeInteger(value) && value >= 0; }
function validateMemory(result) {
  const memory = result.memory;
  if (!memory || memory.requested_interval_ms !== 50 || !nonnegative(memory.samples) || memory.samples < 2 ||
      !nonnegative(memory.maximum_sample_gap_ms) || !nonnegative(result.process_work_ms) ||
      !nonnegative(result.governance_ms) || !nonnegative(result.load_ms)) throw Error('Missing native resource measurements');
  for (const [peak, current] of [['peak_resident_bytes', 'resident_bytes'], ['peak_private_commit_bytes', 'private_commit_bytes'],
    ['peak_sampled_mapped_address_bytes', 'committed_mapped_address_bytes'], ['peak_sampled_image_address_bytes', 'committed_image_address_bytes']]) {
    if (!nonnegative(memory[peak]) || !nonnegative(memory.last?.[current]) || memory[peak] < memory.last[current]) throw Error('Invalid memory peak or snapshot');
  }
  if (memory.peak_resident_bytes === 0 || memory.peak_private_commit_bytes === 0) throw Error('Absent process memory evidence');
}
function validateBatches(batches, count) {
  if (!Array.isArray(batches) || batches.length !== Math.ceil(count / 16) ||
      batches.some((row, i) => row.items !== Math.min(16, count - i * 16) || !nonnegative(row.inference_us))) throw Error('Incomplete measured embedding batches');
}
function validateTimings(result, corpus) {
  validateMemory(result);
  const rows = result.phase === 'build' ? result.timings : result.workspaces;
  if (!Array.isArray(rows) || rows.length !== 2 || new Set(rows.map(row => row.workspace)).size !== 2) throw Error('Missing workspace timings');
  for (const row of rows) {
    if (!['atlas', 'boreal'].includes(row.workspace)) throw Error('Unexpected measured workspace');
    const count = corpus.documents.filter(d => d.workspace === row.workspace).length;
    if (result.phase === 'build') {
      if (row.documents !== count || !nonnegative(row.inference_ms) || !nonnegative(row.index_ms)) throw Error('Incomplete build timings');
      validateBatches(row.batches, count);
    } else {
      if (row.records !== count || !nonnegative(row.reopen_ms) || !nonnegative(row.oracle_inference_ms)) throw Error('Incomplete reopen/oracle timings');
      validateBatches(row.oracle_batches, count);
    }
  }
}
function validateScaleQuery(result, corpus) {
  validateTimings(result, corpus);
  if (result.warm_repetitions !== 5 || !Array.isArray(result.queries) || result.queries.length !== corpus.queries.length * 6 ||
      result.queries.some(row => !Number.isInteger(row.repetition) || row.repetition < 0 || row.repetition > 5 || !nonnegative(row.oracle_compare_us))) throw Error('Incomplete warm query evidence');
  let reference;
  for (let repetition = 0; repetition <= 5; repetition++) {
    const rows = validateQuery({ ...result, queries: result.queries.filter(row => row.repetition === repetition) }, corpus);
    if (reference && JSON.stringify(rows) !== JSON.stringify(reference)) throw Error('Warm query results changed');
    reference ||= rows;
  }
  return reference;
}
// Only call on newly allocated index/scratch roots owned by this experiment.
// Files can disappear between enumeration and stat as native temp indexes close.
function treeUsage(root) {
  let bytes = 0, files = 0, entries = 0;
  try {
    const stat = fs.lstatSync(root);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw Error('Owned measurement root is not a regular directory');
    for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
      entries++;
      if (entry.isSymbolicLink()) throw Error('Unexpected link in owned measurement root');
      const file = path.join(root, entry.name);
      if (entry.isDirectory()) { const child = treeUsage(file); bytes += child.bytes; files += child.files; entries += child.entries; }
      else {
        try { const item = fs.lstatSync(file); if (!item.isFile() || item.isSymbolicLink()) throw Error('Unexpected measured file type'); bytes += item.size; files++; }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
      }
    }
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  return { bytes, files, entries };
}
function validateDisk(disk) {
  if (disk.requested_interval_ms !== 100 || !nonnegative(disk.samples) || disk.samples < 2 ||
      !Number.isFinite(disk.maximum_sample_gap_ms) || disk.maximum_sample_gap_ms < 0 ||
      !nonnegative(disk.peak_sampled_total_bytes) || !nonnegative(disk.peak_sampled_scratch_bytes) ||
      !nonnegative(disk.retained_index_bytes) || disk.retained_index_bytes === 0 ||
      disk.peak_sampled_total_bytes < disk.retained_index_bytes || disk.peak_sampled_total_bytes < disk.peak_sampled_scratch_bytes ||
      disk.final_scratch_entries !== 0) throw Error('Incomplete disk or scratch cleanup evidence');
}
module.exports = { generateCorpus, validateMemory, validateBatches, validateTimings, validateScaleQuery, treeUsage, validateDisk };
