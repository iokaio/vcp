// SPDX-License-Identifier: Apache-2.0
'use strict';
// Data-only structural mapping of the four frozen v2 specifications. Native
// descriptor validation, authority receipts and semantic quality remain separate.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../src/evals/skills/authoring-followup');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const keys = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...names].sort());
const safe = value => typeof value === 'string' && value.length > 0 && value.length <= 1024 && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value) && !value.startsWith('/') && value.split('/').every(p => p && p !== '.' && p !== '..' && !/[. ]$/.test(p) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(p));
function load(caseId) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  if (manifest.revision !== 'cs-1-followup-fixtures-v2') throw Error('Follow-up fixture revision changed');
  const task = manifest.cases.find(item => item.id === caseId);
  if (!task) throw Error('Unknown follow-up case');
  const frozen = (base, ref) => {
    if (!safe(ref.path)) throw Error('Unsafe fixture path');
    const bytes = fs.readFileSync(path.join(base, ref.path));
    if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error('Frozen follow-up bytes changed');
    return bytes.toString('utf8');
  };
  const oracle = JSON.parse(frozen(root, task.expected.oracle));
  const initial = new Map(task.expected.source_files.map(ref => [ref.path, frozen(path.join(root, task.project), ref)]));
  return { task, oracle, initial };
}
function check(caseId, answer, { finalFiles } = {}) {
  const { oracle, initial } = load(caseId), final = new Map(initial), errors = [], reported = new Set();
  const fail = message => errors.push(message);
  const editable = [...oracle.allowed_outputs, ...oracle.allowed_modifications];
  if (!keys(answer, ['files', 'report', 'not_run']) || !Array.isArray(answer.files) || typeof answer.report !== 'string' || !Array.isArray(answer.not_run) || answer.not_run.some(s => typeof s !== 'string')) fail('Invalid exact answer shape');
  else {
    if (answer.files.length > 32 || Buffer.byteLength(answer.report) > 262144 || answer.not_run.length > 64 || answer.not_run.some(s => Buffer.byteLength(s) > 4096)) fail('Answer bounds exceeded');
    let total = 0;
    for (const entry of answer.files.slice(0, 32)) {
      if (!keys(entry, ['path', 'content']) || !safe(entry.path) || typeof entry.content !== 'string') { fail('Invalid output entry'); continue; }
      if (!editable.includes(entry.path) || reported.has(entry.path)) fail('Unauthorized or duplicate artifact');
      reported.add(entry.path); final.set(entry.path, entry.content); total += Buffer.byteLength(entry.content);
      if (Buffer.byteLength(entry.content) > 65536) fail('Artifact byte bound exceeded');
    }
    if (total > 262144) fail('Total artifact bound exceeded');
    for (const name of editable) if (!reported.has(name)) fail(`Required reported artifact absent: ${name}`);
  }
  for (const name of oracle.preserve_files) if (final.get(name) !== initial.get(name)) fail('Preserved source changed');
  if (finalFiles !== undefined) {
    const actual = finalFiles instanceof Map ? finalFiles : new Map(Object.entries(finalFiles));
    for (const [name, content] of final) if (actual.get(name) !== content) fail(`Actual workspace differs: ${name}`);
    for (const name of actual.keys()) if (!final.has(name)) fail('Unexpected actual file');
  }
  const spec = id => oracle.deterministic_checks.find(item => item.id === id);
  // Native checker owns parsed Markdown and native descriptor/JSON-pointer
  // rules. A regex or JSON-shape pass here must not impersonate those checks.
  const documentBound = spec('document-bound');
  if (documentBound) for (const name of documentBound.files) if (Buffer.byteLength(final.get(name) || '') >= documentBound.max_bytes) fail('Document must be below byte limit');
  const descriptorSpec = spec('descriptor-contract') || spec('descriptor-preservation');
  if (descriptorSpec) for (const [name, limit] of Object.entries(descriptorSpec.max_utf8_bytes)) {
    const count = Buffer.byteLength(final.get(name) || '');
    // Frozen task prose disambiguates numeric maxima: maintenance says below;
    // creation says at most. No fixture bytes or quality anchors are changed.
    if (spec('descriptor-preservation') ? count >= limit : count > limit) fail(`Package byte bound: ${name}`);
  }
  return { case_id: caseId, structural_pass: !errors.length, errors, native_descriptor_validation: 'not_run_by_javascript', authority_receipt_audit: 'pending_owner_review', native_link_and_descriptor_checks: 'pending_recorded_native_receipt', link_fragment_validation: 'pending_manual', human_grading: 'pending', observed_task_success: false };
}
module.exports = { check, load };
