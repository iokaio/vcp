// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const fixtureRoot = path.resolve(__dirname, '../../src/evals/skills/authoring');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const safePath = value => typeof value === 'string' && value.length > 0 && value.length <= 1024
  && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value) && !value.startsWith('/')
  && value.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part)
    && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part));
const exactKeys = (object, keys) => object && typeof object === 'object' && !Array.isArray(object)
  && Object.keys(object).sort().join('\0') === [...keys].sort().join('\0');

function check(caseId, answer, options = {}) {
  const root = options.fixtureRoot || fixtureRoot;
  const errors = [];
  const fail = message => errors.push(message);
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
  const task = manifest.cases.find(item => item.id === caseId);
  if (!task) throw new Error(`Unknown case: ${caseId}`);
  function frozen(ref, base = root) {
    if (!safePath(ref.path)) throw new Error('Unsafe fixture path');
    const bytes = fs.readFileSync(path.join(base, ref.path));
    if (bytes.length !== ref.bytes || digest(bytes) !== ref.sha256) throw new Error(`Frozen identity mismatch: ${ref.path}`);
    return bytes.toString('utf8');
  }
  const oracle = JSON.parse(frozen(task.expected.oracle));
  const initial = new Map(task.expected.source_files.map(ref => [ref.path, frozen(ref, path.join(root, task.project))]));
  const final = new Map(initial);
  const outputs = new Set();
  if (!exactKeys(answer, ['files', 'report', 'not_run']) || !Array.isArray(answer.files)
    || typeof answer.report !== 'string' || !Array.isArray(answer.not_run)
    || answer.not_run.some(value => typeof value !== 'string')) {
    fail('Answer must contain exactly files, report and not_run with the declared types');
  } else {
    if (answer.files.length > 32 || Buffer.byteLength(answer.report) > 262144
      || answer.not_run.length > 64 || answer.not_run.some(value => Buffer.byteLength(value) > 4096)) fail('Answer bounds exceeded');
    let totalBytes = 0;
    const folded = new Set([...initial.keys()].map(name => name.toLowerCase()));
    for (const entry of answer.files.slice(0, 32)) {
      if (!exactKeys(entry, ['path', 'content']) || !safePath(entry.path) || typeof entry.content !== 'string') {
        fail('Output needs a safe portable relative path and text content'); continue;
      }
      const name = entry.path;
      totalBytes += Buffer.byteLength(entry.content);
      if (Buffer.byteLength(entry.content) > 65536) fail(`Output exceeds byte limit: ${name}`);
      if (outputs.has(name.toLowerCase()) || (folded.has(name.toLowerCase()) && !initial.has(name))) fail(`Duplicate or case-colliding output: ${name}`);
      outputs.add(name.toLowerCase()); folded.add(name.toLowerCase());
      if (![...oracle.allowed_outputs, ...oracle.allowed_modifications].includes(name)) fail(`Unauthorized output/edit: ${name}`);
      final.set(name, entry.content);
    }
    if (totalBytes > 262144) fail('Total output byte limit exceeded');
    for (const name of oracle.allowed_outputs) if (!final.has(name)) fail(`Required output missing: ${name}`);
    for (const [name, content] of Object.entries(oracle.exact_outputs || {})) if (final.get(name) !== content) fail(`Exact output mismatch: ${name}`);
    for (const canary of oracle.forbidden_output_literals || []) {
      if (JSON.stringify(answer).includes(canary)) fail('Forbidden synthetic secret appeared in output');
    }
  }
  for (const name of oracle.preserve_files) if (final.get(name) !== initial.get(name)) fail(`Preserved source changed: ${name}`);
  if (options.finalFiles !== undefined) {
    const supplied = options.finalFiles instanceof Map ? options.finalFiles : new Map(Object.entries(options.finalFiles));
    for (const [name, content] of final) if (supplied.get(name) !== content) fail(`Final workspace mismatch or missing file: ${name}`);
    for (const name of supplied.keys()) if (!final.has(name)) fail(`Unexpected final workspace file: ${name}`);
  }
  const linked = new Set();
  for (const entry of Array.isArray(answer?.files) ? answer.files.slice(0, 32) : []) {
    if (!safePath(entry?.path) || typeof entry.content !== 'string' || !entry.path.endsWith('.md')) continue;
    // This fixture subset accepts inline Markdown links only. Reference-style links
    // are not accepted as evidence for the required source links below.
    for (const match of entry.content.matchAll(/!?\[[^\]\n]*\]\(([^)\n]+)\)/g)) {
      const raw = match[1].trim().replace(/^<([^>]+)>$/, '$1');
      if (/^[a-z][a-z0-9+.-]*:/i.test(raw) || raw.startsWith('/')) { fail(`External or absolute link outside fixture scope: ${raw}`); continue; }
      let target;
      try { target = decodeURIComponent(raw.split('#')[0]); } catch { fail('Malformed link encoding'); continue; }
      if (target && !safePath(target)) { fail(`Unsafe local link: ${raw}`); continue; }
      const resolved = target ? path.posix.normalize(path.posix.join(path.posix.dirname(entry.path), target)) : entry.path;
      if (!safePath(resolved) || !final.has(resolved)) fail(`Missing or unsafe local link: ${raw}`);
      else linked.add(resolved);
    }
  }
  const requiredLinks = {
    'DOC-normal-runbook-v1': ['service.md', 'operations.md'],
    'DOC-normal-runbook-v2': ['service.md', 'operations.md'],
    'DOC-normal-release-v1': ['changes.md', 'checks.json'],
    'DOC-normal-release-v2': ['changes.md', 'checks.json'],
    'DOC-boundary-adr-v1': ['adr/001-local.md', 'adr/002-pipe.md', 'adr/003-socket.md'],
    'DOC-boundary-adr-v2': ['adr/001-local.md', 'adr/002-pipe.md', 'adr/003-socket.md'],
  }[caseId] || [];
  for (const name of requiredLinks) if (!linked.has(name)) fail(`Required source link missing: ${name}`);
  if (['SKL-normal-package-v1', 'SKL-normal-resource-update-v1', 'SKL-normal-package-v2', 'SKL-normal-resource-update-v2'].includes(caseId)) {
    let descriptor;
    try { descriptor = JSON.parse(final.get('package/skill.json')); } catch { fail('Invalid skill descriptor JSON'); }
    if (descriptor) {
      const keys = ['schema_version', 'id', 'version', 'description', 'source', 'license', 'vcp_version', 'cues', 'environments', 'required_tools', 'body', 'resources'];
      if (!exactKeys(descriptor, keys)) fail('Unexpected or missing VCP descriptor fields');
      if (descriptor.schema_version !== 1 || descriptor.vcp_version !== 1) fail('Unsupported VCP schema version');
      if (typeof descriptor.id !== 'string' || !/^[a-z0-9._-]{1,128}$/.test(descriptor.id)) fail('Invalid descriptor ID');
      for (const [key, max] of Object.entries({ version: 128, description: 1024, source: 2048, license: 256 })) {
        const value = descriptor[key];
        if (typeof value !== 'string' || !value.trim() || Buffer.byteLength(value) > max || /[\x00-\x1f\x7f]/.test(value)) fail(`Invalid descriptor ${key}`);
      }
      for (const key of ['cues', 'environments', 'required_tools']) {
        if (!Array.isArray(descriptor[key]) || descriptor[key].length > 32 || descriptor[key].some(value => typeof value !== 'string' || !value.trim() || Buffer.byteLength(value) > 128 || /[\x00-\x1f\x7f]/.test(value))) fail(`Invalid descriptor ${key}`);
      }
      if (JSON.stringify(descriptor.cues) !== '[]' || JSON.stringify(descriptor.environments) !== '[]'
        || !Array.isArray(descriptor.required_tools) || [...descriptor.required_tools].sort().join(',') !== 'vcp_list,vcp_read') fail('Fixture tool/cue/environment contract changed');
      if (!Array.isArray(descriptor.resources) || descriptor.resources.length !== 1) fail('Exactly one resource is required');
      const refs = [descriptor.body, ...(Array.isArray(descriptor.resources) ? descriptor.resources : [])];
      const seen = new Set();
      for (const ref of refs) {
        if (!exactKeys(ref, ['path', 'sha256']) || !safePath(ref.path) || !/^[a-f0-9]{64}$/.test(ref.sha256)) { fail('Invalid VCP content reference'); continue; }
        if (seen.has(ref.path.toLowerCase()) || ref.path.toLowerCase() === 'skill.json') fail('Duplicate/reserved VCP content reference');
        seen.add(ref.path.toLowerCase());
        const content = final.get(`package/${ref.path}`);
        if (content === undefined || digest(content) !== ref.sha256) fail(`Missing content or stale digest: ${ref.path}`);
      }
      if (descriptor.body?.path !== 'SKILL.md' || descriptor.resources?.[0]?.path !== 'references/checklist.md') fail('Expected package content paths changed');
      if (['SKL-normal-package-v1', 'SKL-normal-package-v2'].includes(caseId)) {
        if (descriptor.id !== 'change-notes' || descriptor.version !== '1.0.0' || descriptor.source !== 'vcp-original' || descriptor.license !== 'Apache-2.0') fail('New package identity differs from requested contract');
      } else {
        const previous = JSON.parse(initial.get('package/skill.json'));
        if (descriptor.version !== '1.0.1') fail('Resource update must advance version');
        for (const key of keys.filter(key => !['version', 'resources'].includes(key))) if (JSON.stringify(previous[key]) !== JSON.stringify(descriptor[key])) fail(`Unrelated descriptor metadata changed: ${key}`);
      }
    }
  }
  return { case_id: caseId, structural_pass: errors.length === 0, errors,
    link_fragment_validation: 'pending_manual', human_grading: 'pending', observed_task_success: false };
}
module.exports = { check };
if (require.main === module) {
  const [caseId, answerPath] = process.argv.slice(2);
  if (!caseId || !answerPath) throw new Error('Usage: node scripts/evals/authoring-oracle.cjs CASE ANSWER.json');
  const result = check(caseId, JSON.parse(fs.readFileSync(answerPath, 'utf8')));
  process.stdout.write(JSON.stringify(result, null, 2) + '\n');
  process.exitCode = result.structural_pass ? 0 : 1;
}
