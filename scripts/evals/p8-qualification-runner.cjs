// SPDX-License-Identifier: Apache-2.0
'use strict';

// This runner is deliberately a thin recorder around existing native tests.
// It never turns a missing package, host, key or fixture into a pass. A build
// wrapper may be supplied for the supported Windows toolchain; it receives one
// JSON command-file path and owns toolchain setup, while this process records
// the exact command and its output.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const {sourceIdentity: captureSourceIdentity} = require('./memory-source-identity.cjs');

const repo = path.resolve(__dirname, '../..');
const defaultManifest = path.join(__dirname, 'p8-qualification-manifest.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
const MAX_MANIFEST = 2 * 1024 * 1024;
const MAX_LOG = 256 * 1024 * 1024;

function real(file) {
  const absolute = path.resolve(file);
  for (let current = absolute; ; current = path.dirname(current)) {
    if (fs.existsSync(current) && fs.lstatSync(current).isSymbolicLink()) throw Error(`symlink or junction rejected: ${file}`);
    if (path.dirname(current) === current) break;
  }
  return absolute;
}

function read(file, limit = MAX_MANIFEST) {
  const absolute = real(file);
  const stat = fs.statSync(absolute);
  if (!stat.isFile() || stat.size > limit) throw Error(`bounded regular file required: ${file}`);
  return fs.readFileSync(absolute);
}

function writeJson(file, value) {
  fs.writeFileSync(file, json(value), {encoding: 'utf8'});
}

function validText(value, name, max = 4096) {
  if (typeof value !== 'string' || value.length === 0 || value.length > max || value.includes('\0')) throw Error(`invalid ${name}`);
}

function validateManifest(manifest) {
  if (!manifest || manifest.schema !== 'p8-qualification-manifest/1' || typeof manifest.revision !== 'string') throw Error('unsupported P8 qualification manifest');
  validText(manifest.revision, 'manifest revision', 256);
  if (!Array.isArray(manifest.cases) || manifest.cases.length === 0 || manifest.cases.length > 128) throw Error('manifest needs a bounded case list');
  const ids = new Set();
  const areas = new Set(['native_process', 'native_policy', 'packaged', 'process_recovery', 'recovery', 'cleanup', 'history', 'encryption', 'packaged_encryption']);
  for (const item of manifest.cases) {
    if (!item || typeof item !== 'object') throw Error('manifest case must be an object');
    validText(item.id, 'case id', 128);
    if (ids.has(item.id)) throw Error(`duplicate case id: ${item.id}`);
    ids.add(item.id);
    validText(item.work_item, 'work item', 32);
    if (!areas.has(item.area)) throw Error(`unknown case area: ${item.area}`);
    validText(item.title, 'case title', 512);
    if (typeof item.required !== 'boolean') throw Error(`case required flag missing: ${item.id}`);
    if (!Array.isArray(item.requires) || item.requires.some(x => typeof x !== 'string' || !x)) throw Error(`invalid requirements: ${item.id}`);
    validText(item.evidence, 'case evidence', 2048);
    if (item.command !== null && (!Array.isArray(item.command) || item.command.length < 1 || item.command.some(x => typeof x !== 'string' || !x))) throw Error(`invalid command: ${item.id}`);
    if (item.command === null && item.gap !== true) throw Error(`command-less case must be an explicit gap: ${item.id}`);
    if (item.command !== null && item.gap === true) throw Error(`executable case cannot be marked as a gap: ${item.id}`);
  }
  return manifest;
}

function loadManifest(file = defaultManifest) {
  const bytes = read(file);
  const manifest = validateManifest(JSON.parse(bytes));
  return {file: real(file), bytes, manifest, sha256: sha(bytes)};
}

function commandFor(manifest, item) {
  if (item.command === null) return null;
  const cwd = path.resolve(repo, manifest.workspace || '.');
  const relative = path.relative(repo, cwd).replaceAll(path.sep, '/');
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw Error(`command cwd escapes repository: ${item.id}`);
  return {program: item.command[0], args: item.command.slice(1), cwd, cwd_relative: relative};
}

function commandAvailable(program) {
  const result = spawnSync(program, ['--version'], {stdio: 'ignore', windowsHide: true});
  return result.error == null && result.status === 0;
}

function regexText(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function expectedTest(command) {
  const index = command.args.indexOf('--test');
  return index >= 0 && command.args[index + 2] && command.args.includes('--exact') ? command.args[index + 2] : null;
}

function observedPassingTest(command, output) {
  const expected = expectedTest(command);
  return expected === null || new RegExp(`(?:^|\\n)\\s*test\\s+[^\\r\\n]*${regexText(expected)}\\s+\\.\\.\\.\\s+ok(?:\\r?$|\\r?\\n)`, 'm').test(output);
}

function fileRequirement(value) {
  if (!value || typeof value !== 'string') return false;
  try { return fs.statSync(real(value)).isFile(); } catch (_) { return false; }
}

function packageRequirement(value) {
  if (!value || typeof value !== 'string') return false;
  try {
    const stat = fs.statSync(real(value));
    return stat.isFile() || stat.isDirectory();
  } catch (_) { return false; }
}

function requirementReasons(requirements, context = {}) {
  const reasons = [];
  for (const requirement of requirements) {
    if (requirement === 'native_windows' && (context.platform || process.platform) !== 'win32') reasons.push('native Windows host required');
    else if (requirement === 'cargo' && context.cargo_available === false) reasons.push('cargo executable unavailable');
    else if (requirement === 'cargo' && context.cargo_available === undefined && !commandAvailable('cargo')) reasons.push('cargo executable unavailable');
    else if (requirement === 'vcp_test_git' && !fileRequirement(context.vcp_test_git || process.env.VCP_TEST_GIT)) reasons.push('VCP_TEST_GIT must identify a native Git executable');
    else if (requirement === 'packaged_artifact' && !packageRequirement(context.packaged_artifact || process.env.VCP_TEST_SKILL_PACKAGE || process.env.P8_PACKAGE)) reasons.push('exact packaged artifact is unavailable');
    else if (requirement === 'age_binary' && !fileRequirement(context.age_binary || process.env.P8_AGE)) reasons.push('independent age binary is unavailable');
    else if (requirement === 'second_machine_fixture' && !fileRequirement(context.second_machine_fixture || process.env.P8_SECOND_MACHINE_FIXTURE)) reasons.push('second-machine recovery fixture is unavailable');
    else if (!['native_windows', 'cargo', 'vcp_test_git', 'packaged_artifact', 'age_binary', 'second_machine_fixture'].includes(requirement)) reasons.push(`unknown prerequisite: ${requirement}`);
  }
  return reasons;
}

function parseArgs(argv) {
  const options = {manifest: defaultManifest, cases: [], wrapperArgs: []};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = () => { if (i + 1 >= argv.length) throw Error(`missing value for ${arg}`); return argv[++i]; };
    if (arg === '--manifest') options.manifest = next();
    else if (arg === '--output') options.output = next();
    else if (arg === '--case') options.cases.push(next());
    else if (arg === '--wrapper') options.wrapper = next();
    else if (arg === '--wrapper-args-json') options.wrapperArgs = JSON.parse(next());
    else if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--list') options.list = true;
    else if (arg === '--help') options.help = true;
    else throw Error(`unknown option: ${arg}`);
  }
  if (!Array.isArray(options.wrapperArgs) || options.wrapperArgs.some(x => typeof x !== 'string')) throw Error('wrapper args must be a JSON string array');
  return options;
}

function sourceIdentity() {
  const captured = captureSourceIdentity(repo, [
    'src/tests/contracts/p8-qualification-runner.test.cjs',
    'src/third_party/upstreams.toml',
    'src/third_party/components/codex-selection.json',
    'src/third_party/components/munarium-selection.json',
    'src/third_party/components/codex-files.json',
    'src/third_party/components/munarium-files.json',
    'src/tests/fixtures',
  ]);
  // The helper already includes scripts/evals and the Codex lockfile. Do not
  // add those paths again through the explicit inputs above: duplicate file
  // records make the evidence identity ambiguous. Keep the complete selected
  // scope in the file list so its content and Git hashes describe the same
  // inputs.
  const files = [...new Map(captured.files.map(file => [file.path, file])).values()]
    .sort((a, b) => a.path.localeCompare(b.path, 'en'));
  return {...captured, scope: [...new Set(captured.scope)], files,
    content_sha256: sha(JSON.stringify(files)),
    integrity_scope: 'Captured native sources, all evaluation scripts, fixtures, P8 manifests/tests, component selections and lockfiles; content and Git hashes use this deduplicated scope'};
}

function newOutput(directory) {
  const output = real(directory);
  if (fs.existsSync(output)) throw Error(`output directory already exists: ${output}`);
  fs.mkdirSync(path.dirname(output), {recursive: true, mode: 0o700});
  fs.mkdirSync(output, {recursive: false, mode: 0o700});
  return output;
}

function safeCaseDirectory(output, id) {
  if (!/^[A-Za-z0-9._-]+$/.test(id)) throw Error(`unsafe case id: ${id}`);
  const directory = path.join(output, id);
  fs.mkdirSync(directory, {mode: 0o700});
  return directory;
}

function runCase(item, manifest, output, options, context) {
  const directory = safeCaseDirectory(output, item.id);
  const reasons = requirementReasons(item.requires, context);
  const command = commandFor(manifest, item);
  const started = new Date().toISOString();
  const record = {id: item.id, work_item: item.work_item, area: item.area, title: item.title, required: item.required, started_at: started, command: command ? [command.program, ...command.args] : null, cwd: command?.cwd_relative || null, evidence: item.evidence, status: 'not_run', reasons: []};
  if (reasons.length) { record.reasons = reasons; record.ended_at = new Date().toISOString(); return record; }
  if (command === null) { record.reasons = ['explicit qualification gap: no executable command registered']; record.ended_at = new Date().toISOString(); return record; }
  if (options.dryRun) { record.reasons = ['dry run requested; native command not launched']; record.ended_at = new Date().toISOString(); return record; }
  const commandFile = path.join(directory, 'command.json');
  const stdoutFile = path.join(directory, 'stdout.log');
  const stderrFile = path.join(directory, 'stderr.log');
  const commandSpec = {schema: 'p8-command/1', case_id: item.id, cwd: command.cwd, program: command.program, args: command.args, environment: {VCP_TEST_GIT: process.env.VCP_TEST_GIT || null, VCP_TEST_SKILL_PACKAGE: process.env.VCP_TEST_SKILL_PACKAGE || null}};
  writeJson(commandFile, commandSpec);
  const launch = options.wrapper ? {program: options.wrapper, args: [...options.wrapperArgs, commandFile], cwd: repo} : {program: command.program, args: command.args, cwd: command.cwd};
  record.launcher = [launch.program, ...launch.args];
  const stdout = fs.openSync(stdoutFile, 'w');
  const stderr = fs.openSync(stderrFile, 'w');
  const result = spawnSync(launch.program, launch.args, {cwd: launch.cwd, env: process.env, windowsHide: true, timeout: Math.max(1, item.timeout_seconds || manifest.default_timeout_seconds || 1800) * 1000, stdio: ['ignore', stdout, stderr]});
  fs.closeSync(stdout); fs.closeSync(stderr);
  record.exit_code = result.status;
  record.signal = result.signal;
  record.error = result.error ? String(result.error.message || result.error) : null;
  const expected = expectedTest(command);
  const combinedOutput = Buffer.concat([read(stdoutFile, MAX_LOG), read(stderrFile, MAX_LOG)]).toString('utf8');
  record.expected_test = expected;
  record.test_observed = observedPassingTest(command, combinedOutput);
  record.status = result.error || result.status !== 0 || !record.test_observed ? 'fail' : 'pass';
  record.reasons = result.error ? [result.error.message || String(result.error)] : result.status !== 0 ? [`native command exited with status ${result.status}`] : record.test_observed ? [] : [`targeted test did not report an exact passing case: ${expected}`];
  record.evidence_files = [commandFile, stdoutFile, stderrFile].map(file => ({path: path.relative(output, file).replaceAll(path.sep, '/'), bytes: fs.statSync(file).size, sha256: sha(read(file, MAX_LOG))}));
  record.ended_at = new Date().toISOString();
  return record;
}

function summarize(records) {
  const counts = {pass: 0, fail: 0, not_run: 0};
  const byArea = {};
  for (const record of records) { counts[record.status]++; byArea[record.area] ??= {pass: 0, fail: 0, not_run: 0}; byArea[record.area][record.status]++; }
  const required = records.filter(record => record.required);
  const overall = required.some(record => record.status === 'fail') ? 'fail' : required.some(record => record.status !== 'pass') ? 'incomplete' : 'pass';
  return {overall, counts, by_area: byArea, required_rows: required.length, required_passed: required.filter(record => record.status === 'pass').length, limitations: records.filter(record => record.status === 'not_run').map(record => ({id: record.id, reasons: record.reasons}))};
}

function runCampaign(options) {
  const loaded = loadManifest(options.manifest || defaultManifest);
  const selected = options.cases.length ? new Set(options.cases) : null;
  const unknown = selected && [...selected].filter(id => !loaded.manifest.cases.some(item => item.id === id));
  if (unknown?.length) throw Error(`unknown case selection: ${unknown.join(', ')}`);
  const output = newOutput(options.output || path.join(repo, 'artifacts', 'p8', `${Date.now()}-${crypto.randomUUID()}`));
  const source = sourceIdentity();
  const runnerBytes = read(__filename);
  const result = {schema: 'p8-qualification-result/1', run_id: path.basename(output), manifest_revision: loaded.manifest.revision, manifest_sha256: loaded.sha256, runner_sha256: sha(runnerBytes), source, status: 'running', cases: [], output};
  writeJson(path.join(output, 'manifest.json'), result);
  for (const item of loaded.manifest.cases) {
    const record = selected && !selected.has(item.id) ? {id: item.id, work_item: item.work_item, area: item.area, title: item.title, required: item.required, status: 'not_run', reasons: ['case not selected'], evidence: item.evidence} : runCase(item, loaded.manifest, output, options, {vcp_test_git: options.vcp_test_git, cargo_available: options.cargo_available, platform: options.platform});
    result.cases.push(record); result.summary = summarize(result.cases); writeJson(path.join(output, 'manifest.json'), result);
  }
  const afterManifest = sha(read(loaded.file));
  const afterRunner = sha(read(__filename));
  const afterSource = sourceIdentity();
  result.source_after = afterSource;
  const sourceChanged = afterSource.content_sha256 !== source.content_sha256;
  if (afterManifest !== loaded.sha256 || afterRunner !== result.runner_sha256 || sourceChanged) { result.status = 'fail'; result.integrity_error = 'qualification source changed during campaign'; }
  else { result.status = result.summary.overall; }
  result.ended_at = new Date().toISOString();
  writeJson(path.join(output, 'manifest.json'), result);
  return result;
}

module.exports = {validateManifest, loadManifest, commandFor, expectedTest, observedPassingTest, requirementReasons, sourceIdentity, summarize, parseArgs, runCampaign};

if (require.main === module) {
  try {
    const options = parseArgs(process.argv.slice(2));
    const loaded = loadManifest(options.manifest);
    if (options.help) { console.log('Usage: node p8-qualification-runner.cjs [--output DIR] [--case ID] [--wrapper FILE] [--wrapper-args-json JSON] [--dry-run] [--list]'); process.exit(0); }
    if (options.list) { console.log(JSON.stringify(loaded.manifest.cases.map(item => ({id: item.id, work_item: item.work_item, area: item.area, required: item.required, gap: item.gap === true})), null, 2)); process.exit(0); }
    const result = runCampaign({...options, manifest: loaded.file});
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.status === 'pass' ? 0 : 1;
  } catch (error) { console.error(error.message || String(error)); process.exitCode = 1; }
}
