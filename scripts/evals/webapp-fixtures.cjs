// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read-only CS-3 WEB fixture identity. Project code is data and is never loaded.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const fixtureRoot = path.resolve(__dirname, 'fixtures/webapp');
const revision = 'cs-3-webapp-fixtures-v1';
const manifestSha256 = 'dbe34a441187381f0e5d3f587ea72b0ac15e096ca9d90f99a065ef8e51d84ee8';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const safe = value => typeof value === 'string' && value.length > 0 && value.length <= 512
  && !value.startsWith('/') && !/[\\:*?"<>|\x00-\x1f\x7f]/.test(value)
  && value.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part)
    && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part));
function ordinaryTreeRoot(root) {
  const resolved = path.resolve(root), real = path.resolve(fs.realpathSync.native(resolved));
  const same = process.platform === 'win32'
    ? resolved.toLowerCase() === real.toLowerCase()
    : resolved === real;
  if (!same) throw Error('WEB fixture root or ancestor is redirected');
  const stat = fs.lstatSync(resolved);
  if (!stat.isDirectory() || stat.isSymbolicLink()) throw Error('Ordinary WEB fixture root required');
  return resolved;
}
function boundedFile(file, maximum, label) {
  // Reject redirected ancestors before opening, including nested oracle and
  // project directories. These are trusted static fixture reads, not atomic
  // admission of a hostile concurrently mutated filesystem.
  ordinaryTreeRoot(path.dirname(file));
  const before = fs.lstatSync(file);
  if (!before.isFile() || before.isSymbolicLink()) throw Error(`${label} is not an ordinary file`);
  if (before.size < 1 || before.size > maximum) throw Error(`${label} bound exceeded`);
  const fd = fs.openSync(file, 'r');
  try {
    const opened = fs.fstatSync(fd);
    if (!opened.isFile() || opened.size !== before.size || opened.dev !== before.dev || opened.ino !== before.ino) throw Error(`${label} identity changed`);
    const bytes = Buffer.allocUnsafe(opened.size);
    let offset = 0;
    while (offset < bytes.length) {
      const count = fs.readSync(fd, bytes, offset, bytes.length - offset, offset);
      if (count === 0) throw Error(`${label} was truncated`);
      offset += count;
    }
    const after = fs.fstatSync(fd);
    if (after.size !== opened.size || after.dev !== opened.dev || after.ino !== opened.ino) throw Error(`${label} identity changed`);
    return bytes;
  } finally { fs.closeSync(fd); }
}
function frozen(root, ref) {
  if (!ref || !safe(ref.path) || !Number.isSafeInteger(ref.bytes) || ref.bytes < 1 || ref.bytes > 65536 || !/^[a-f0-9]{64}$/.test(ref.sha256)) throw Error('Invalid frozen WEB reference');
  const file = path.join(root, ...ref.path.split('/'));
  const bytes = boundedFile(file, 65536, 'WEB reference');
  if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error(`Frozen WEB input changed: ${ref.path}`);
  return bytes;
}
function include(expected, relative) {
  if (expected.has(relative)) throw Error(`Duplicate WEB inventory path: ${relative}`);
  expected.add(relative);
}
function walk(root) {
  const files = []; let nodes = 0;
  function visit(relative, depth) {
    if (depth > 6 || ++nodes > 96) throw Error('WEB fixture inventory bound exceeded');
    const directory = relative ? path.join(root, ...relative.split('/')) : root, stat = fs.lstatSync(directory);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw Error('WEB fixture directory is redirected');
    for (const name of fs.readdirSync(directory).sort()) {
      if (++nodes > 96) throw Error('WEB fixture inventory bound exceeded');
      const child = relative ? `${relative}/${name}` : name;
      if (!safe(child)) throw Error('Unsafe WEB inventory path');
      const childStat = fs.lstatSync(path.join(root, ...child.split('/')));
      if (childStat.isSymbolicLink()) throw Error('Redirected WEB fixture entry');
      if (childStat.isDirectory()) visit(child, depth + 1);
      else if (childStat.isFile()) files.push(child);
      else throw Error('Non-file WEB fixture entry');
    }
  }
  visit('', 0); return files.sort();
}
function inspect(at = fixtureRoot) {
  const root = ordinaryTreeRoot(at);
  const manifestBytes = boundedFile(path.join(root, 'manifest.json'), 65536, 'WEB manifest');
  const manifest = JSON.parse(manifestBytes);
  if (manifest.schema_version !== 1 || manifest.revision !== revision || manifest.declared_before_execution !== true
    || manifest.source !== 'vcp-original' || manifest.license !== 'Apache-2.0' || manifest.case_count !== 6
    || manifest.planned_task_runs !== 18 || manifest.model_calls !== 0 || manifest.browser_execution !== 'not_run'
    || manifest.campaign_execution !== 'not_run' || manifest.qualification !== 'not_run'
    || JSON.stringify(manifest.limits) !== JSON.stringify({ file_bytes: 65536, case_input_bytes: 262144, case_files: 32 })
    || !Array.isArray(manifest.shared) || manifest.shared.length !== 3 || !Array.isArray(manifest.cases) || manifest.cases.length !== 6) throw Error('Unexpected WEB fixture cohort');
  const expected = new Set(['manifest.json']);
  for (const ref of manifest.shared) { include(expected, ref?.path); frozen(root, ref); }
  const comparison = JSON.parse(frozen(root, manifest.shared.find(ref => ref.path === 'comparison.json')));
  const rubric = JSON.parse(frozen(root, manifest.shared.find(ref => ref.path === 'rubric.json')));
  if (comparison.revision !== 'cs-3-webapp-comparison-v1' || comparison.planned_task_runs !== 18 || comparison.execution_authorized !== false || comparison.model_calls !== 0
    || JSON.stringify(comparison.arms) !== JSON.stringify({ none: [], nearest: ['testing'], candidate: ['webapp-testing'] })
    || rubric.version !== 'cs-3-webapp-rubric-v1' || rubric.execution !== 'not_run' || rubric.visual_review !== 'not_run') throw Error('WEB comparison or rubric differs');
  const ids = new Set(), kinds = new Map(), loaded = new Map();
  for (const task of manifest.cases) {
    if (!task || !/^WEB-[a-z0-9-]+-v1$/.test(task.id) || ids.has(task.id) || task.skill !== 'webapp-testing'
      || !['normal', 'boundary', 'hostile', 'missing', 'near_miss'].includes(task.kind) || task.project !== `projects/${task.id}`
      || typeof task.prompt !== 'string' || !task.prompt || Buffer.byteLength(task.prompt) > 8192
      || JSON.stringify(task.comparison_arms) !== JSON.stringify(['none', 'nearest', 'candidate'])
      || JSON.stringify(task.arm_skills) !== JSON.stringify(comparison.arms)
      || task.expected?.automatic_activation !== false || !['appropriate', 'unnecessary'].includes(task.expected?.candidate_selection)
      || !Array.isArray(task.expected?.source_files) || !task.expected.source_files.length || task.expected.source_files.length > manifest.limits.case_files
      || task.execution?.model_calls !== 0 || task.execution?.comparison !== 'not_run') throw Error('Invalid WEB fixture case');
    ids.add(task.id); kinds.set(task.kind, (kinds.get(task.kind) || 0) + 1);
    if (task.kind === 'near_miss' ? task.expected.candidate_selection !== 'unnecessary' : task.expected.candidate_selection !== 'appropriate') throw Error('WEB candidate selection expectation differs');
    const files = new Map(); let total = 0;
    for (const ref of task.expected.source_files) {
      if (!safe(ref.path) || ref.path.includes('/') || files.has(ref.path)) throw Error('Invalid WEB project file path');
      const full = { ...ref, path: `${task.project}/${ref.path}` }, bytes = frozen(root, full);
      total += bytes.length; if (total > manifest.limits.case_input_bytes) throw Error('WEB case input bound exceeded');
      files.set(ref.path, bytes);
      include(expected, full.path);
    }
    const oracleRef = task.expected.oracle;
    if (oracleRef?.path !== `oracles/${task.id}.json` || files.has(path.basename(oracleRef.path))) throw Error('WEB oracle is not independently held out');
    include(expected, oracleRef.path);
    const oracle = JSON.parse(frozen(root, oracleRef));
    if (oracle.schema_version !== 1 || oracle.case_id !== task.id || oracle.rubric_version !== rubric.version
      || !Array.isArray(oracle.allowed_modifications) || !Array.isArray(oracle.required_observations) || !oracle.required_observations.length
      || !Array.isArray(oracle.preservation) || !oracle.preservation.length || !Array.isArray(oracle.forbidden_actions)
      || oracle.execution_state !== 'not_run' || oracle.allowed_modifications.some(name => !safe(name) || !files.has(name))) throw Error('Invalid held-out WEB oracle');
    const expectedMods = task.kind === 'near_miss' ? ['normalize.cjs'] : [];
    if (JSON.stringify(oracle.allowed_modifications) !== JSON.stringify(expectedMods)) throw Error('WEB modification scope differs');
    loaded.set(task.id, { task, files, oracle });
  }
  if (kinds.get('normal') !== 2 || kinds.get('boundary') !== 1 || kinds.get('hostile') !== 1 || kinds.get('missing') !== 1 || kinds.get('near_miss') !== 1) throw Error('WEB fixture class coverage differs');
  const actual = walk(root);
  if (JSON.stringify(actual) !== JSON.stringify([...expected].sort())) throw Error('Unexpected WEB fixture inventory');
  // Validate shape and bounded references first for precise diagnostics, but
  // never expose a cohort, candidate input or oracle with an unpinned manifest.
  // This binds prompts/context and the subordinate hashes to this exact revision.
  if (sha(manifestBytes) !== manifestSha256) throw Error('Frozen WEB manifest changed');
  return { root, revision, manifest, manifest_sha256: sha(manifestBytes), comparison, rubric, loaded, inventory: actual };
}
function candidateInput(caseId, at = fixtureRoot) {
  const entry = inspect(at).loaded.get(caseId);
  if (!entry) throw Error('Unknown WEB fixture case');
  return { id: entry.task.id, kind: entry.task.kind, prompt: entry.task.prompt, context: structuredClone(entry.task.context), files: new Map([...entry.files].map(([name, bytes]) => [name, Buffer.from(bytes)])) };
}
function oracle(caseId, at = fixtureRoot) {
  const entry = inspect(at).loaded.get(caseId);
  if (!entry) throw Error('Unknown WEB fixture case');
  return structuredClone(entry.oracle);
}
module.exports = { inspect, candidateInput, oracle, revision, manifestSha256, fixtureRoot, safe };
