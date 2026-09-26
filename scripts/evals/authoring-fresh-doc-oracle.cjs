// SPDX-License-Identifier: Apache-2.0
'use strict';
// Fixed prospective fixture mechanics, never semantic or qualification verdicts.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../src/evals/skills/authoring-qualification');
const cases = ['DOC-fresh-format-reference-v1', 'DOC-fresh-acceptance-plan-v1'];
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const keys = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...names].sort());
const safe = value => typeof value === 'string' && value.length > 0 && value.length <= 1024 && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value) && !value.startsWith('/') && value.split('/').every(p => p && p !== '.' && p !== '..' && !/[. ]$/.test(p) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(p));
const markers = ['<!-- FORMAT START -->', '<!-- FORMAT END -->'];
function region(text) {
  if (typeof text !== 'string' || markers.some(marker => text.split(marker).length !== 2)) throw Error('Exactly one ordered marker pair required');
  const start = text.indexOf(markers[0]) + markers[0].length, end = text.indexOf(markers[1]);
  if (start > end) throw Error('Exactly one ordered marker pair required');
  return { prefix: text.slice(0, start), body: text.slice(start, end), suffix: text.slice(end) };
}
// Unicode White_Space is explicit, unlike JavaScript \s (which includes BOM).
const words = text => text.split(/\p{White_Space}+/u).filter(token => /[\p{L}\p{N}]/u.test(token)).length;
function load(caseId) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  if (manifest.schema_version !== 1 || manifest.revision !== 'cs-1-fresh-document-fixtures-v1' || manifest.case_count !== 2 || JSON.stringify(manifest.cases.map(task => task.id)) !== JSON.stringify(cases)) throw Error('Prospective fixture cohort changed');
  const task = manifest.cases.find(item => item.id === caseId);
  if (!task || task.project !== `projects/${caseId}`) throw Error('Unknown prospective case');
  const frozen = (base, ref) => {
    if (!ref || !safe(ref.path)) throw Error('Unsafe fixture path');
    const bytes = fs.readFileSync(path.join(base, ref.path));
    if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error('Frozen prospective bytes changed');
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  };
  manifest.shared.forEach(ref => frozen(root, ref));
  if (frozen(root, task.task_input) !== task.prompt) throw Error('Task prompt differs from frozen input');
  const oracle = JSON.parse(frozen(root, task.expected.oracle));
  if (oracle.schema !== 'cs1-fresh-doc-private-oracle-draft/1' || oracle.case_id !== caseId) throw Error('Unsupported prospective oracle');
  const initial = new Map();
  for (const ref of task.expected.source_files) {
    if (initial.has(ref.path)) throw Error('Duplicate source');
    initial.set(ref.path, frozen(path.join(root, task.project), ref));
  }
  return { task, oracle, initial };
}
function check(caseId, answer, { finalFiles } = {}) {
  const { oracle, initial } = load(caseId), final = new Map(initial), errors = [], reported = new Set();
  const fail = message => errors.push(message), editable = [...oracle.allowed_outputs, ...oracle.allowed_modifications];
  if (!keys(answer, ['files', 'report', 'not_run']) || !Array.isArray(answer.files) || typeof answer.report !== 'string' || !Array.isArray(answer.not_run) || answer.not_run.some(s => typeof s !== 'string')) fail('Invalid exact answer shape');
  else {
    if (answer.files.length !== 1 || Buffer.byteLength(answer.report) > 262144 || answer.not_run.length > 64 || answer.not_run.some(s => Buffer.byteLength(s) > 4096)) fail('Answer bounds exceeded');
    for (const entry of answer.files.slice(0, 32)) {
      if (!keys(entry, ['path', 'content']) || !safe(entry.path) || typeof entry.content !== 'string') { fail('Invalid output entry'); continue; }
      if (!editable.includes(entry.path) || reported.has(entry.path)) fail('Unauthorized or duplicate artifact');
      reported.add(entry.path); final.set(entry.path, entry.content);
      if (Buffer.byteLength(entry.content) > 65536) fail('Artifact byte bound exceeded');
      if (Buffer.from(entry.content).toString('utf8') !== entry.content) fail('Artifact contains invalid Unicode scalar data');
    }
    for (const name of editable) if (!reported.has(name)) fail(`Required reported artifact absent: ${name}`);
  }
  for (const name of oracle.preserve_files) if (final.get(name) !== initial.get(name)) fail('Preserved source changed');
  if (finalFiles !== undefined) {
    const actual = finalFiles instanceof Map ? finalFiles : new Map(Object.entries(finalFiles));
    for (const [name, content] of final) if (actual.get(name) !== content) fail(`Actual workspace differs: ${name}`);
    for (const name of actual.keys()) if (!final.has(name)) fail('Unexpected actual file');
  }
  let text = final.get(editable[0]) || '';
  if (caseId === cases[0]) {
    try {
      const before = region(initial.get(editable[0])), after = region(text);
      if (before.prefix !== after.prefix || before.suffix !== after.suffix) fail('Outside marked region changed');
      text = after.body;
    } catch (error) { fail(error.message); }
  }
  const count = words(text), [min, max] = caseId === cases[0] ? [300, 550] : [400, 650];
  if (count < min || count > max) fail('Document word bound exceeded');
  return { case_id: caseId, structural_pass: errors.length === 0, errors, word_count: count, native_link_and_descriptor_checks: 'pending_recorded_native_receipt', native_markdown_and_csv_checks: 'pending_recorded_native_receipt', native_descriptor_validation: 'not_applicable', authority_receipt_audit: 'pending_owner_review', link_fragment_validation: 'pending_manual', factual_accuracy: 'pending_independent_blinded_review', human_grading: 'pending', observed_task_success: false };
}
module.exports = { check, load, cases, region, words };
