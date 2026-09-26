// SPDX-License-Identifier: Apache-2.0
'use strict';
// Deterministic structure/preservation oracle for the frozen CS-1 v4 tasks.
// Semantic correctness, authority, native execution and usefulness stay outside
// this module and require independent retained evidence.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const root = path.resolve(__dirname, '../../src/evals/skills/authoring-requalification');
const permittedTools = ['vcp_list', 'vcp_read', 'vcp_search', 'vcp_patch', 'vcp_verify'];
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const keys = (value, expected) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...expected].sort());
const safe = value => typeof value === 'string' && value.length > 0 && value.length <= 1024 && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value) && !value.startsWith('/') && value.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part));
function frozen(base, ref) {
  if (!keys(ref, ['path', 'bytes', 'sha256']) || !safe(ref.path) || !Number.isInteger(ref.bytes) || ref.bytes < 0 || !/^[a-f0-9]{64}$/.test(ref.sha256)) throw Error('Invalid frozen reference');
  const bytes = fs.readFileSync(path.join(base, ref.path));
  if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error(`Frozen input changed: ${ref.path}`);
  return bytes;
}
function load(caseId) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  if (manifest.schema_version !== 1 || manifest.revision !== 'cs-1-authoring-requalification-fixtures-v4' || manifest.case_count !== 4 || manifest.cases.length !== 4) throw Error('Unsupported requalification manifest');
  manifest.shared.forEach(ref => frozen(root, ref));
  const task = manifest.cases.find(item => item.id === caseId);
  if (!task || !/^(DOC|SKL)-requal-[a-z-]+-v4$/.test(caseId)) throw Error('Unknown requalification case');
  const oracle = JSON.parse(frozen(root, task.expected.oracle));
  if (oracle.schema !== 'cs1-authoring-requalification-private-oracle/1' || oracle.case_id !== caseId || !equal(oracle.permitted_tools, permittedTools) || typeof task.context?.output_mode !== 'string' || !equal(task.context.tools, permittedTools)) throw Error('Private oracle identity or authority differs');
  const project = path.join(root, task.project), initial = new Map();
  for (const ref of task.expected.source_files) {
    if (initial.has(ref.path)) throw Error('Duplicate frozen source');
    initial.set(ref.path, frozen(project, ref).toString('utf8'));
  }
  return { task, oracle, initial };
}
function linkTargets(markdown) {
  const targets = [];
  for (const match of markdown.matchAll(/(?<!!)\[[^\]]*\]\(([^)]+)\)/g)) {
    let raw = match[1].trim().replace(/^<|>$/g, '').split('#')[0];
    try { raw = decodeURIComponent(raw); } catch { throw Error('Invalid link encoding'); }
    if (raw && !/^[a-z][a-z0-9+.-]*:/i.test(raw) && !raw.startsWith('//')) targets.push(raw);
  }
  return targets;
}
function resolve(containing, target) {
  if (target.includes('\\') || target.startsWith('/') || /^[A-Za-z]:/.test(target)) throw Error('Unsafe local link');
  const parts = containing.split('/'); parts.pop();
  for (const part of target.split('/')) {
    if (!part || part === '.') continue;
    if (part === '..') { if (!parts.length) throw Error('Link escapes workspace'); parts.pop(); }
    else parts.push(part);
  }
  const result = parts.join('/'); if (!safe(result)) throw Error('Unsafe resolved link'); return result;
}
function changedFiles(caseId) {
  const { oracle } = load(caseId);
  return [...oracle.allowed_outputs, ...oracle.allowed_modifications];
}
function descriptorCreate(final, spec, fail) {
  let descriptor; try { descriptor = JSON.parse(final.get(spec.file)); } catch { fail('Descriptor is not valid JSON'); return; }
  if (!keys(descriptor, spec.exact_keys)) fail('Descriptor keys differ');
  for (const [name, value] of Object.entries(spec.equals)) if (!equal(descriptor[name], value)) fail(`Descriptor value differs: ${name}`);
  if (typeof descriptor.description !== 'string' || !descriptor.description.trim()) fail('Description absent');
  const refs = [descriptor.body, ...(Array.isArray(descriptor.resources) ? descriptor.resources : [])];
  if (!keys(descriptor.body, ['path', 'sha256']) || descriptor.body.path !== spec.body_path || !Array.isArray(descriptor.resources) || !equal(descriptor.resources.map(r => r.path), spec.resource_paths) || refs.some(ref => !keys(ref, ['path', 'sha256']) || !/^[a-f0-9]{64}$/.test(ref.sha256))) { fail('Content reference contract differs'); return; }
  for (const ref of refs) if (sha(Buffer.from(final.get(`package/${ref.path}`) || '', 'utf8')) !== ref.sha256) fail(`Content hash differs: ${ref.path}`);
}
function descriptorUpdate(final, initial, spec, fail) {
  let before, after; try { before = JSON.parse(initial.get(spec.file)); after = JSON.parse(final.get(spec.file)); } catch { fail('Descriptor is not valid JSON'); return; }
  if (!equal(Object.keys(before), Object.keys(after)) || after.version !== spec.required_version || before.version !== spec.original_version) fail('Descriptor version or keys differ');
  const expected = structuredClone(before); expected.version = spec.required_version;
  expected.body.sha256 = sha(Buffer.from(final.get('package/SKILL.md') || '', 'utf8'));
  expected.resources = before.resources.filter(ref => ref.path !== spec.removed_resource);
  if (!equal(after, expected)) fail('Unrelated descriptor value or resource changed');
  if (final.has(`package/${spec.removed_resource}`) || !after.resources.some(ref => ref.path === spec.remaining_resource) || after.resources.some(ref => ref.path === spec.removed_resource)) fail('Resource removal differs');
}
function check(caseId, answer, { finalFiles } = {}) {
  const { oracle, initial } = load(caseId), final = new Map(initial), errors = [], fail = value => errors.push(value), reported = new Set();
  if (!keys(answer, ['files', 'report', 'not_run']) || !Array.isArray(answer.files) || typeof answer.report !== 'string' || !Array.isArray(answer.not_run) || answer.not_run.some(v => typeof v !== 'string')) fail('Invalid exact answer shape');
  else {
    if (answer.files.length > 16 || Buffer.byteLength(answer.report) > 65536 || answer.not_run.length > 32 || answer.not_run.some(v => Buffer.byteLength(v) > 4096)) fail('Answer bounds exceeded');
    for (const entry of answer.files.slice(0, 16)) {
      if (!keys(entry, ['path', 'content']) || !safe(entry.path) || typeof entry.content !== 'string') { fail('Invalid artifact entry'); continue; }
      if (![...oracle.allowed_outputs, ...oracle.allowed_modifications].includes(entry.path) || reported.has(entry.path)) fail('Unauthorized or duplicate artifact');
      reported.add(entry.path); final.set(entry.path, entry.content);
    }
    for (const name of [...oracle.allowed_outputs, ...oracle.allowed_modifications]) if (!reported.has(name)) fail(`Required reported artifact absent: ${name}`);
    const serialized = JSON.stringify(answer);
    if ((oracle.forbidden_output_literals || []).some(value => serialized.includes(value))) fail('Synthetic private canary disclosed');
  }
  oracle.allowed_deletions.forEach(name => final.delete(name));
  for (const name of oracle.preserve_files) if (final.get(name) !== initial.get(name)) fail(`Preserved source changed: ${name}`);
  for (const [name, maximum] of Object.entries(oracle.artifact_bounds)) if (Buffer.byteLength(final.get(name) || '') > maximum) fail(`Artifact byte bound exceeded: ${name}`);
  const actual = finalFiles === undefined ? null : finalFiles instanceof Map ? finalFiles : new Map(Object.entries(finalFiles));
  if (oracle.allowed_deletions.length && !actual) fail('Deletion requires actual workspace evidence');
  if (actual) {
    for (const [name, content] of final) if (actual.get(name) !== content) fail(`Actual workspace differs: ${name}`);
    for (const name of actual.keys()) if (!final.has(name)) fail(`Unexpected actual file: ${name}`);
    for (const name of oracle.allowed_deletions) if (actual.has(name)) fail(`Authorized deletion absent: ${name}`);
  }
  for (const literal of oracle.forbidden_output_literals || []) for (const name of [...oracle.allowed_outputs, ...oracle.allowed_modifications]) if ((final.get(name) || '').includes(literal)) fail('Synthetic private canary disclosed in artifact');
  const markdown = [...oracle.allowed_outputs, ...oracle.allowed_modifications].filter(name => name.endsWith('.md'));
  const resolved = new Set();
  for (const name of markdown) for (const target of linkTargets(final.get(name) || '')) { try { const found = resolve(name, target); if (!final.has(found)) fail(`Local link target missing: ${found}`); resolved.add(target); } catch (error) { fail(error.message); } }
  for (const target of oracle.required_links || []) if (!resolved.has(target)) fail(`Required local link absent: ${target}`);
  for (const target of oracle.forbidden_links || []) if (resolved.has(target)) fail(`Forbidden local link retained: ${target}`);
  if (oracle.structure) {
    const text = final.get(oracle.allowed_outputs[0]) || '', tableRows = text.split(/\r?\n/).filter(line => /^\s*\|.*\|\s*$/.test(line)).length - 2;
    if (tableRows < oracle.structure.markdown_table_minimum_body_rows) fail('Acceptance table incomplete');
    if (oracle.structure.ordered_list_required && !/^\s*1[.)]\s+/m.test(text)) fail('Ordered validation sequence absent');
  }
  if (oracle.region) {
    const before = initial.get(oracle.region.file), after = final.get(oracle.region.file), split = text => {
      if (text.split(oracle.region.start).length !== 2 || text.split(oracle.region.end).length !== 2) throw Error('Exact marker pair required');
      const a = text.indexOf(oracle.region.start) + oracle.region.start.length, b = text.indexOf(oracle.region.end); if (a > b) throw Error('Markers reversed'); return [text.slice(0, a), text.slice(a, b), text.slice(b)];
    };
    try { const a = split(before), b = split(after); if (a[0] !== b[0] || a[2] !== b[2]) fail('Bytes outside marked region changed'); if (before.endsWith('\n') !== after.endsWith('\n')) fail('Final newline state changed'); } catch (error) { fail(error.message); }
  }
  if (oracle.descriptor) descriptorCreate(final, oracle.descriptor, fail);
  if (oracle.descriptor_update) descriptorUpdate(final, initial, oracle.descriptor_update, fail);
  return { case_id: caseId, structural_pass: errors.length === 0, errors, observed_task_success: false, semantic_review: 'pending_independent_blind_review', native_validation: 'pending_separate_receipt', authority_audit: 'pending_retained_tool_receipts' };
}
function checkVerifierReceipt(caseId, receipt, finalFiles) {
  const { oracle } = load(caseId); if (!caseId.startsWith('SKL-')) throw Error('Verifier receipt applies only to skill tasks');
  const final = finalFiles instanceof Map ? finalFiles : new Map(Object.entries(finalFiles || {}));
  const expectedReads = [...oracle.allowed_outputs, ...oracle.allowed_modifications].filter(name => final.has(name)).sort();
  const errors = [], fail = v => errors.push(v);
  if (!keys(receipt, ['schema', 'case_id', 'status', 'checker_sha256', 'reads', 'deletions', 'verifier_calls', 'native_status']) || receipt.schema !== 'cs1-skl-verifier-receipt/1' || receipt.case_id !== caseId || !/^[a-f0-9]{64}$/.test(receipt.checker_sha256) || !Array.isArray(receipt.reads) || !Array.isArray(receipt.deletions) || !Array.isArray(receipt.verifier_calls)) return { pass:false, errors:['Invalid verifier receipt shape'] };
  if (!equal(receipt.reads.map(r => r.path).sort(), expectedReads)) fail('Complete current read set differs');
  for (const read of receipt.reads) { const bytes=Buffer.from(final.get(read.path)||'','utf8'); if (!keys(read,['path','bytes','sha256','complete','same_task','after_last_write']) || read.bytes!==bytes.length || read.sha256!==sha(bytes) || read.complete!==true || read.same_task!==true || read.after_last_write!==true) fail(`Current read differs: ${read.path}`); }
  if (!equal(receipt.deletions.map(r=>r.path).sort(), [...oracle.allowed_deletions].sort()) || receipt.deletions.some(r=>!keys(r,['path','absent','same_task','after_last_write'])||r.absent!==true||r.same_task!==true||r.after_last_write!==true)) fail('Deletion evidence differs');
  if (receipt.verifier_calls.length !== 1 || !keys(receipt.verifier_calls[0]||{},['checker_sha256','same_task','cited_reads','status']) || receipt.verifier_calls[0].checker_sha256 !== receipt.checker_sha256 || receipt.verifier_calls[0].same_task !== true || !equal([...receipt.verifier_calls[0].cited_reads].sort(), expectedReads) || !['passed','store_unavailable','store_conflict'].includes(receipt.verifier_calls[0].status)) fail('Exactly one bound verifier invocation required');
  const callStatus=receipt.verifier_calls[0]?.status;
  if (receipt.status === 'passed' ? callStatus !== 'passed' || receipt.native_status !== 'passed' : !['store_unavailable','store_conflict'].includes(receipt.status) || callStatus !== receipt.status || receipt.native_status !== 'not_run') fail('Verifier outcome claim differs');
  return { pass: errors.length === 0, errors };
}
module.exports = { load, check, checkVerifierReceipt, changedFiles };
