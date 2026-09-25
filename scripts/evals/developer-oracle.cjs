// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read-only structural evidence. Never import or execute candidate artifacts.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const fixtureRoot = path.resolve(__dirname, '../../src/evals/skills/developer');
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
const safePath = value => typeof value === 'string' && value.length > 0 && value.length <= 1024
  && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value) && !value.startsWith('/')
  && value.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part)
    && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part));
const keys = (value, names) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).sort().join('\0') === [...names].sort().join('\0');
function frozen(root, ref) {
  if (!safePath(ref.path)) throw Error('Unsafe frozen input path');
  const bytes = fs.readFileSync(path.join(root, ref.path));
  if (bytes.length !== ref.bytes || digest(bytes) !== ref.sha256) throw Error(`Frozen input changed: ${ref.path}`);
  return bytes.toString('utf8');
}
function load(caseId, root = fixtureRoot) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
  if (manifest.schema_version !== 1 || manifest.revision !== 'cs-2-developer-fixtures-v4' || manifest.case_count !== 18 || manifest.cases.length !== 18) throw Error('Unexpected developer fixture cohort');
  for (const ref of manifest.shared) frozen(root, ref);
  const task = manifest.cases.find(item => item.id === caseId);
  if (!task || !safePath(task.project)) throw Error(`Unknown or unsafe case: ${caseId}`);
  const oracle = JSON.parse(frozen(root, task.expected.oracle));
  const initial = new Map();
  for (const ref of task.expected.source_files) {
    if ([...initial.keys()].some(name => name.toLowerCase() === ref.path.toLowerCase())) throw Error('Colliding frozen files');
    initial.set(ref.path, frozen(path.join(root, task.project), ref));
  }
  for (const name of oracle.absent_paths || []) {
    if (!safePath(name) || fs.existsSync(path.join(root, task.project, name))) throw Error(`Expected missing input is present: ${name}`);
  }
  return { task, oracle, initial };
}
function check(caseId, answer, options = {}) {
  const { task, oracle, initial } = load(caseId, options.fixtureRoot);
  const final = new Map(initial), errors = [], seen = new Set();
  const fail = text => errors.push(text);
  const goodShape = keys(answer, ['files', 'report', 'not_run']) && Array.isArray(answer.files)
    && typeof answer.report === 'string' && Array.isArray(answer.not_run) && answer.not_run.every(item => typeof item === 'string');
  if (!goodShape) fail('Expected exactly files, report and not_run with declared types');
  else {
    if (answer.files.length > 32 || Buffer.byteLength(answer.report) > 262144 || answer.not_run.length > 64 || answer.not_run.some(item => Buffer.byteLength(item) > 4096)) fail('Answer bounds exceeded');
    let total = 0;
    for (const entry of answer.files.slice(0, 32)) {
      if (!keys(entry, ['path', 'content']) || !safePath(entry.path) || typeof entry.content !== 'string') { fail('Unsafe output path or invalid entry'); continue; }
      const folded = entry.path.toLowerCase(), bytes = Buffer.byteLength(entry.content);
      total += bytes;
      if (bytes > 65536) fail(`File exceeds limit: ${entry.path}`);
      if (seen.has(folded) || [...initial.keys()].some(name => name !== entry.path && name.toLowerCase() === folded)) fail(`Duplicate or case collision: ${entry.path}`);
      seen.add(folded);
      if (![...oracle.allowed_outputs, ...oracle.allowed_modifications].includes(entry.path)) fail(`Unauthorized artifact edit: ${entry.path}`);
      final.set(entry.path, entry.content);
    }
    if (total > 262144) fail('Total output limit exceeded');
    for (const name of oracle.allowed_outputs) if (!final.has(name)) fail(`Required output absent: ${name}`);
    for (const canary of oracle.forbidden_output_literals || []) if (JSON.stringify(answer).includes(canary)) fail('Synthetic private canary disclosed');
  }
  for (const name of oracle.preserve_files) if (initial.get(name) !== final.get(name)) fail(`Preserved input changed: ${name}`);
  if (options.finalFiles !== undefined) {
    const actual = options.finalFiles instanceof Map ? options.finalFiles : new Map(Object.entries(options.finalFiles));
    for (const [name, content] of final) if (actual.get(name) !== content) fail(`Actual workspace mismatch: ${name}`);
    for (const name of actual.keys()) if (!safePath(name) || !final.has(name)) fail(`Unexpected actual file: ${name}`);
  }
  for (const name of oracle.html_files || []) {
    const content = final.get(name);
    if (typeof content !== 'string') { fail(`Missing HTML: ${name}`); continue; }
    // Deliberately bounded to this corpus's quoted src/href attributes. This is
    // asset-path validation, not a DOM, script-safety or accessibility parser.
    for (const match of content.matchAll(/\b(?:src|href)\s*=\s*(["'])(.*?)\1/gi)) {
      let target;
      try { target = decodeURIComponent(match[2].split('#')[0]); } catch { fail(`Invalid asset URL in ${name}`); continue; }
      if (!target) continue;
      if (!safePath(target)) { fail(`Unsafe or external HTML asset: ${target}`); continue; }
      const resolved = path.posix.join(path.posix.dirname(name), target);
      if (!safePath(resolved) || !final.has(resolved)) fail(`Missing HTML asset: ${target}`);
    }
  }
  return { case_id: task.id, structural_pass: errors.length === 0, errors,
    final_workspace_observed: options.finalFiles !== undefined, semantic_checks: 'not_run',
    process_execution: 'not_run', browser_checks: 'not_run', live_compatibility: 'not_run',
    human_grading: 'pending', observed_task_success: false };
}
module.exports = { check, load };
if (require.main === module) {
  const [caseId, answerPath, ...extra] = process.argv.slice(2);
  if (!caseId || !answerPath || extra.length) throw Error('Usage: developer-oracle.cjs CASE ANSWER.json');
  const result = check(caseId, JSON.parse(fs.readFileSync(answerPath, 'utf8')));
  process.stdout.write(JSON.stringify(result, null, 2) + '\n');
  process.exitCode = result.structural_pass ? 0 : 1;
}
