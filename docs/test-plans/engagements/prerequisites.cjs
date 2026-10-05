// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const baseContract = require('./base-contract.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const protectedFiles = {
  A: ['tests/health.test.ts', 'tests/regressions.test.ts'],
  B: ['src/Inventory.Web/appsettings.Development.json', 'tests/Inventory.Tests/RegressionTests.cs'],
};
function physical(file) {
  file = path.resolve(file);
  for (let next = file; ; next = path.dirname(next)) {
    assert(!fs.lstatSync(next).isSymbolicLink(), `Linked evidence path: ${next}`);
    if (path.dirname(next) === next) break;
  }
  return file;
}
function child(root, relative) {
  assert(typeof relative === 'string' && relative && !path.isAbsolute(relative));
  assert(!relative.split(/[\\/]/).some(part => !part || part === '.' || part === '..'), 'Unsafe evidence relative path');
  const result = path.resolve(root, relative);
  assert(result.startsWith(path.resolve(root) + path.sep), 'Evidence path escapes root');
  return physical(result);
}
function read(file, expected) {
  file = physical(file);
  assert(fs.statSync(file).size <= 16 * 1024 * 1024, 'Evidence JSON exceeds bound');
  const bytes = fs.readFileSync(file);
  assert.match(expected, /^[a-f0-9]{64}$/);
  assert.equal(sha(bytes), expected, `Evidence hash mismatch: ${file}`);
  return JSON.parse(bytes);
}
function baseline(kind, input) {
  const score = read(input.scorecard, input.scorecard_sha256);
  assert.equal(score.schema, 'vcp-practical-scenario/1');
  assert.equal(score.scenario, kind === 'A' ? 'a-vue-taskboard' : 'b-aspnet-inventory');
  assert.equal(score.verdict, 'pass');
  assert.equal(score.reused_project, false, 'A retained repair run is not a fresh scaffold');
  assert.equal(score.dry_run, false);
  assert.equal(score.skipped_stages, 0);
  assert(!score.fatal && !score.paid_execution_block);
  assert(Array.isArray(score.stages) && score.stages.length >= 6);
  assert(score.stages.every(stage => !stage.skipped && stage.task), 'Missing actual task-stage evidence');
  for (let n = 1; n <= 6; n++) assert(score.stages.some(stage => stage.stage.startsWith(`T${n}-`)), `Missing original T${n} stage`);
  const latest = new Map();
  for (const gate of score.gates) {
    if (gate.required) latest.set(`${gate.stage.replace(/-repair\d+$/, '')}|${gate.id}`, gate);
  }
  assert(latest.size > 0 && [...latest.values()].every(gate => gate.outcome === 'pass'), 'Required source gates did not all pass');
  for (const [stage, id] of baseContract.required(kind)) assert(latest.has(`${stage}|${id}`), `Missing original acceptance gate ${stage}/${id}`);
  for (const stage of baseContract.stages[kind]) {
    const row = score.stages.find(row => row.stage === stage);
    assert(row && row.session && Number.isInteger(row.exit_code) && Array.isArray(row.accepted_exit), `Missing executed original stage ${stage}`);
  }
  assert.equal(score.stages.find(row => row.stage === baseContract.stages[kind][4]).task, score.stages.find(row => row.stage === 'T5-resume').task, 'Original explicit resume did not retain task identity');
  assert.equal(score.required_gates, latest.size);
  assert.equal(score.required_passed, latest.size);
  assert.equal(score.required_failed, 0);
  assert([...latest.values()].some(gate => gate.stage.startsWith('FINAL') && gate.id === 'protected-files'), 'Missing final protected-file gate');
  const manifest = read(input.checkpoint, input.checkpoint_sha256);
  assert.equal(manifest.schema, 'vcp-source-checkpoint/1');
  assert.match(manifest.message, /^FINAL:/);
  assert.equal(path.resolve(manifest.workspace), path.resolve(score.workspace));
  assert(Date.parse(manifest.at) >= Date.parse(score.started) && Date.parse(manifest.at) <= Date.parse(score.finished));
  assert.equal(manifest.file_count, Object.keys(manifest.files).length);
  assert(manifest.file_count > 0);
  const files = path.join(path.dirname(physical(input.checkpoint)), 'files');
  for (const [relative, hash] of Object.entries(manifest.files)) {
    assert.match(hash, /^[a-f0-9]{64}$/);
    assert.equal(sha(fs.readFileSync(child(files, relative))), hash, `Checkpoint bytes differ: ${relative}`);
    assert.equal(sha(fs.readFileSync(child(score.workspace, relative))), hash, `Passing workspace has changed: ${relative}`);
  }
  for (const relative of protectedFiles[kind]) assert(manifest.files[relative], `Protected source missing: ${relative}`);
  return { kind, scorecard: physical(input.scorecard), scorecard_sha256: input.scorecard_sha256,
    checkpoint: physical(input.checkpoint), checkpoint_sha256: input.checkpoint_sha256,
    run_id: score.run_id, workspace: physical(score.workspace), files_root: files, files: manifest.files,
    protected: Object.fromEntries(protectedFiles[kind].map(relative => [relative, manifest.files[relative]])),
    original_candidate: { version: score.vcp_version, executable: score.vcp, model: score.model, endpoint: score.endpoint } };
}
function verify(spec) {
  assert.equal(spec.schema, 'vcp-engagement-input/1');
  assert.match(spec.source_revision, /^[a-f0-9]{40}$/);
  assert.match(spec.candidate.sha256, /^[a-f0-9]{64}$/);
  assert.equal(sha(fs.readFileSync(physical(spec.candidate.executable))), spec.candidate.sha256, 'Candidate bytes changed');
  assert.match(spec.candidate.version, /^\d+\.\d+\.\d+$/);
  const executable = physical(spec.candidate.executable);
  const receiptPath = path.join(path.dirname(executable), 'build-evidence.json');
  assert(fs.statSync(physical(receiptPath)).size <= 1024 * 1024, 'Adjacent build receipt exceeds bound');
  const receiptBytes = fs.readFileSync(receiptPath), receipt = JSON.parse(receiptBytes);
  assert.equal(receipt.schema, 'vcp-execution-diagnostic-build/1');
  assert(['built', 'pass', 'passed'].includes(receipt.status) && receipt.source_stable === true, 'Candidate build is not complete/stable');
  assert.equal(receipt.source_commit, spec.source_revision);
  assert.equal(receipt.version, spec.candidate.version);
  assert([spec.candidate.version, `vcp ${spec.candidate.version}`].includes(receipt.actual_version));
  assert.equal(receipt.sha256, spec.candidate.sha256);
  assert.equal(receipt.bytes, fs.statSync(executable).size);
  // Do not copy arbitrary receipt metadata, command environments or supplied paths.
  const candidate = { executable, sha256: receipt.sha256, bytes: receipt.bytes, version: receipt.version,
    actual_version: receipt.actual_version, source_commit: receipt.source_commit, source_stable: true, build_status: receipt.status,
    receipt: receiptPath, receipt_sha256: sha(receiptBytes), source_identity: 'declared_by_adjacent_build_receipt' };
  return { schema: 'vcp-engagement-prerequisites/1', candidate, source_revision: spec.source_revision,
    A: baseline('A', spec.A), B: baseline('B', spec.B) };
}
function copy(baseline, destination) {
  destination = path.resolve(destination);
  assert(!fs.existsSync(destination), 'Engagement workspace must be new');
  physical(path.dirname(destination));
  fs.mkdirSync(destination);
  for (const [relative, expected] of Object.entries(baseline.files)) {
    const bytes = fs.readFileSync(child(baseline.files_root, relative));
    assert.equal(sha(bytes), expected);
    const target = path.resolve(destination, relative);
    assert(target.startsWith(destination + path.sep));
    fs.mkdirSync(path.dirname(target), { recursive: true });
    physical(path.dirname(target));
    fs.writeFileSync(target, bytes, { flag: 'wx' });
  }
}
module.exports = { verify, baseline, copy, sha, physical, child, protectedFiles };
if (require.main === module) {
  try {
    const [input, output, kind, destination] = process.argv.slice(2);
    assert(fs.statSync(physical(input)).size <= 1024 * 1024, 'Input manifest exceeds bound');
    physical(path.dirname(path.resolve(output)));
    const spec = JSON.parse(fs.readFileSync(physical(input), 'utf8'));
    const proof = verify(spec);
    if (kind) { assert(['A', 'B'].includes(kind)); copy(proof[kind], destination); }
    fs.writeFileSync(output, JSON.stringify(proof, null, 2) + '\n', { flag: 'wx' });
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
