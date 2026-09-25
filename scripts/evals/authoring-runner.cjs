// SPDX-License-Identifier: Apache-2.0
'use strict';
// One-shot CS-1 comparison. The harness never executes or applies final JSON;
// task artifact edits occur through the scoped native vcp_patch tool.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const prior = require('./p6-live-runner.cjs');
const prep = require('./authoring-prepare.cjs');
const oracle = require('./authoring-oracle.cjs');
const { profileReasons } = prep;
const { requireEmbeddedCatalog } = require('./builtin-generation-prepare.cjs');
const { inspectAssets } = require('../skills/builtin-assets.cjs');
const { plain, read, write, safeChild, noParentInstructions, privateDirectory, noSecrets, usd, frames, inspection, invoke } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const fixture = path.join(repository, 'src/evals/skills/authoring/manifest.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function inventory(directory) {
  return prep.identity(directory, ['.']).files.map(f => ({ ...f, path: f.path.slice(2) })).sort((a, b) => a.path.localeCompare(b.path, 'en'));
}
function preserved(base, row) {
  requireDirectories(path.join(base, 'workspace'), row);
  return equal(inventory(path.join(base, 'workspace')), [...row.files].sort((a, b) => a.path.localeCompare(b.path, 'en')));
}
function requireDirectories(root, row) {
  const actual = prep.identity(root, ['.']).directories.filter(name => name !== '.').map(name => name.slice(2)).sort();
  if (!equal(actual, row.directories)) throw Error('Workspace directory scaffold changed');
}
function finalWorkspace(base, row, allowed) {
  const root = path.join(base, 'workspace'), actual = inventory(root);
  requireDirectories(root, row);
  const originals = new Map(row.files.map(file => [file.path, file]));
  let changedBytes = 0;
  for (const file of actual) {
    const initial = originals.get(file.path);
    if ((!initial || !equal(file, initial)) && (!allowed.includes(file.path) || file.bytes > prep.outputBounds.file_bytes || (changedBytes += file.bytes) > prep.outputBounds.total_bytes)) throw Error('Workspace changed outside bounded authorized artifacts');
  }
  if (row.files.some(file => !actual.some(found => found.path === file.path))) throw Error('Workspace source file deleted');
  for (const relative of row.scaffold_paths || []) {
    if (!equal(actual.find(file => file.path === relative), originals.get(relative))) throw Error('Verification scaffold changed');
  }
  return new Map(actual.filter(file => !(row.scaffold_paths || []).includes(file.path)).map(file => [file.path, read(safeChild(root, file.path), 1024 * 1024).toString('utf8')]));
}
function validate(plan, file, startedRows = 0) {
  noParentInstructions(plan.directory); privateDirectory(plan.directory);
  if (plan.schema !== 'cs-1-authoring-preparation/3' || plan.runnable !== true || plan.authorization !== false || plan.model_calls !== 0 || plain(path.dirname(path.resolve(file))) !== plan.directory || !equal(plan.output_bounds, prep.outputBounds)) throw Error('Prepared plan contract differs');
  if (!equal(plan.source, prep.identity(repository, prep.sourceScope))) throw Error('Prepared source identity changed');
  const executableBytes = read(plan.executable, 1024 * 1024 * 1024), assetsRoot = path.join(path.dirname(plan.executable), 'skills/builtin');
  if (sha(executableBytes) !== plan.executable_sha256 || !equal(inspectAssets(assetsRoot).inventory, plan.assets)) throw Error('Prepared executable or assets changed');
  requireEmbeddedCatalog(executableBytes, read(path.join(assetsRoot, 'catalog.json')));
  const specBytes = read(plan.spec_source, 64 * 1024), spec = JSON.parse(specBytes);
  const profileBytes = read(plan.profile_source, 1024 * 1024), profile = JSON.parse(profileBytes);
  noSecrets(spec); noSecrets(profile);
  if (sha(specBytes) !== plan.spec_sha256 || sha(profileBytes) !== plan.profile_sha256 || plain(path.resolve(spec.executable)) !== plan.executable || plain(path.resolve(spec.profile)) !== plan.profile_source || plain(path.resolve(profile.catalog)) !== plan.provider_catalog || sha(read(plan.provider_catalog)) !== plan.provider_catalog_sha256 || profileReasons(profile).length) throw Error('Provider qualification or spec changed');
  const toolchain = { node_version: process.version, node_executable: process.execPath, node_sha256: sha(read(process.execPath, 128 * 1024 * 1024)), platform: process.platform, architecture: process.arch, candidate_processes: [plan.runtime?.checker] };
  if (!equal(plan.toolchain, toolchain)) throw Error('Prepared toolchain changed');
  const manifestBytes = read(fixture), manifest = JSON.parse(manifestBytes);
  if (sha(manifestBytes) !== plan.fixture_sha256 || manifest.revision !== plan.fixture_revision || !equal(plan.held_out, manifest.shared) || plan.runs.length !== 36 || manifest.cases.length !== 12) throw Error('Frozen cohort changed');
  if (spec.propose_opaque_checker_effects !== true || Object.keys(spec).sort().join(',') !== 'aggregate_call_ceiling,aggregate_cap_usd,executable,profile,propose_opaque_checker_effects,runtime' || !equal(plan.runtime, prep.checkerRuntime(spec.runtime, plan.directory, manifest))) throw Error('Checker runtime or process proposal changed');
  if (!equal(plan.permission_review, prep.permissionReview(plan.runtime))) throw Error('Checker permission proposal changed');
  if (sha(read(plan.runtime.checker, 256 * 1024 * 1024)) !== plan.runtime.checker_sha256 || sha(read(plan.runtime.cases_file)) !== plan.runtime.cases_sha256 || !read(plan.runtime.cases_file).equals(prep.checkerCasesBytes(plan.directory, manifest))) throw Error('Staged checker or case mapping changed');
  const cap = prior.micros(spec.aggregate_cap_usd), allocation = Math.floor(cap / 36);
  const calls = Math.min(profile.max_requests, Math.floor(spec.aggregate_call_ceiling / 36));
  if (!Number.isSafeInteger(spec.aggregate_call_ceiling) || spec.aggregate_call_ceiling < 36 || spec.aggregate_call_ceiling > 576 || allocation < 1 || plan.aggregate_cap_micros !== cap || plan.allocated_cap_micros !== allocation * 36 || plan.aggregate_call_ceiling !== spec.aggregate_call_ceiling || plan.allocated_call_ceiling !== calls * 36) throw Error('Frozen allocation changed');
  if (!equal(plan.budget_preflight, prep.budgetPreflight(profile, allocation))) throw Error('First-request budget preflight changed');
  const owner = JSON.parse(read(path.join(plan.directory, 'preparation-owner.json')));
  if (owner.schema !== plan.schema || owner.spec_sha256 !== plan.spec_sha256) throw Error('Preparation ownership changed');
  let index = 0;
  for (const task of manifest.cases) for (const arm of ['none', 'nearest', 'candidate']) {
    const row = plan.runs[index++], selected = arm === 'none' ? null : arm === 'nearest' ? task.nearest_skill : task.skill;
    const id = task.id + '--' + arm, base = safeChild(plan.directory, id);
    if (!equal(row.directories, prep.preparedDirectories(task))) throw Error('Frozen directory scaffold changed');
    requireDirectories(path.join(base, 'workspace'), row);
    if (row.id !== id || row.case_id !== task.id || row.arm !== arm || row.skill !== (selected ? `vcp-builtin::${selected}::${selected}` : null) || row.cap_micros !== allocation || row.call_ceiling !== calls || row.status !== 'not_run' || !equal(row.files, prep.preparedFiles(task)) || !equal(row.scaffold_paths, [...prep.scaffold(task).keys()]) || !equal(row.oracle, task.expected.oracle) || row.prompt_sha256 !== sha(Buffer.from(prep.promptFor(task)))) throw Error('Frozen arm or allocation changed');
    if (index > startedRows && (!preserved(base, row) || fs.readdirSync(plain(path.join(base, 'data'))).length) || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256 || sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256) throw Error('Prepared inputs changed or data store not fresh');
    const derived = prep.derivedProfile(profile, task, path.join(base, 'workspace'), plan.provider_catalog, allocation, calls, plan.runtime);
    if (!equal(JSON.parse(read(path.join(base, 'profile.json'))), derived)) throw Error('Derived profile differs');
  }
}
function capturedContext(plan, base, item, call) {
  const descriptor = item.record, length = Number(descriptor.length);
  if (descriptor.state !== 'complete' || !Number.isSafeInteger(length) || length < 1 || length > 1024 * 1024) throw Error('Incomplete context capture');
  const chunks = [];
  for (let offset = 0; offset < length; offset += 65536) {
    const pages = inspection(plan, base, item.id, 'context', call, ['--offset', String(offset), '--length', '65536']);
    const row = pages[0]?.items[0], end = Math.min(offset + 65536, length);
    if (pages.length !== 1 || pages[0].items.length !== 1 || pages.some(p => p.gaps.some(g => !prior.privacyGap(g, item.id))) || row?.range?.start !== offset || row.range.end !== end || row.artifact !== undefined && row.artifact !== item.id || row.visibility !== undefined && row.visibility !== 'available' || !Array.isArray(row.bytes) || row.bytes.length !== end - offset || row.bytes.some(b => !Number.isInteger(b) || b < 0 || b > 255)) throw Error('Context range unavailable');
    chunks.push(Buffer.from(row.bytes));
  }
  const bytes = Buffer.concat(chunks);
  if (sha(bytes) !== descriptor.sha256) throw Error('Context digest mismatch');
  return JSON.parse(bytes);
}
function skillEvidence(plan, base, row, pages, attempts, call) {
  if (pages.some(p => p.gaps.some(g => !prior.privacyGap(g, g.artifact)))) throw Error('Context evidence incomplete');
  const captures = pages.flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.schema === 'context-manifest/1');
  const manifests = captures.map(item => ({ artifact: item.id, manifest: capturedContext(plan, base, item, call) }));
  const catalog = JSON.parse(read(path.join(repository, 'src/skills/builtin/catalog.json')));
  const entry = row.skill ? catalog.skills.find(s => row.skill === `vcp-builtin::${s.id}::${s.id}`) : null;
  if (row.skill && !entry) throw Error('Unknown selected builtin');
  const expected = entry ? [entry.body, ...(entry.resources || [])].map((part, index) => ({ id: 'skill-' + sha(Buffer.from(row.skill)) + '-' + index, hash: part.sha256 })) : [];
  const dispatched = attempts.filter(a => a.phase === 'settled');
  if (!dispatched.length) throw Error('No settled model attempt');
  for (const attempt of dispatched) {
    const matched = manifests.filter(m => m.manifest.request_sha256 === attempt.request_digest);
    if (!matched.length) throw Error('No canonical context for dispatched request');
    for (const { manifest } of matched) {
      if (!Array.isArray(manifest.included)) throw Error('Invalid context manifest');
      const active = manifest.included.filter(p => p.kind === 'skill');
      if (active.length !== expected.length || expected.some(part => active.filter(p => p.id === part.id && p.source_hash === part.hash && p.trust === 'active_skill').length !== 1)) throw Error('Dispatched skill body/resource context differs');
    }
  }
  return { qualified_id: row.skill, parts: expected.length, checked_attempts: dispatched.length, manifests: manifests.map(m => m.artifact) };
}
function run(file, authorization, call = invoke) {
  const bytes = read(file);
  if (sha(bytes) !== authorization) throw Error('Authorization must name exact prepared plan hash');
  const plan = JSON.parse(bytes); validate(plan, file);
  write(path.join(plan.directory, 'execution-claim.json'), { plan_sha256: authorization, at: new Date().toISOString(), meaning: 'One shot. Interruption or unknown liability requires reconciliation; never replay.' });
  const result = { schema: 'cs-1-authoring-result/3', plan_sha256: authorization, quality: 'pending_independent_review', actual_cost_micros: 0, observed_attempts: 0, stopped: false, runs: plan.runs.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null })) };
  for (let index = 0; index < plan.runs.length; index++) {
    if (result.stopped) break;
    const row = plan.runs[index], report = result.runs[index], base = safeChild(plan.directory, row.id);
    const started = Date.now();
    let dispatched = false, accounted = false;
    try {
      if (sha(read(file)) !== authorization) throw Error('Authorized plan changed');
      validate(plan, file, index);
      const profile = JSON.parse(read(path.join(base, 'profile.json')));
      const args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', usd(row.cap_micros), '--autonomy', profile.maximum_autonomy];
      if (row.skill) args.push('--skill', row.skill);
      write(path.join(base, 'attempted.json'), { plan_sha256: authorization, args, at: new Date().toISOString() });
      dispatched = true;
      const execution = call(plan.executable, args, (profile.deadline_seconds + 180) * 1000);
      report.latency_ms = Date.now() - started;
      write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr);
      if (execution.error) throw Error('CLI interrupted; reconcile unknown liability');
      const output = frames(execution.stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
      if (!accepted?.scope?.task || !final?.conditions) throw Error('Missing durable task result');
      report.scope = accepted.scope;
      const evidence = {};
      for (const view of ['costs', 'routing', 'outputs', 'context']) { evidence[view] = inspection(plan, base, accepted.scope.task, view, call); write(path.join(base, view + '.json'), evidence[view]); }
      const money = prior.accounting(evidence.costs, row.cap_micros);
      report.actual_cost_micros = money.actual_cost_micros; result.actual_cost_micros += money.actual_cost_micros; result.observed_attempts += money.attempts.length;
      accounted = true;
      if (money.attempts.length > row.call_ceiling) throw Error('Observed call ceiling exceeded');
      const finalFiles = finalWorkspace(base, row, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths);
      report.preserved = true;
      report.status = final.conditions.completed && execution.status === 0 ? 'completed' : 'failed';
      if (report.status === 'failed') {
        report.conditions = final.conditions;
        report.reason = final.conditions.budget_exhausted ? 'native_budget_exhausted' : 'native_task_incomplete';
      }
      if (report.status === 'completed') {
        report.skill_evidence = skillEvidence(plan, base, row, evidence.context, money.attempts, call);
        try {
          report.answer_source = prior.responseAnswer(plan, base, evidence.outputs, money.attempts, call);
          write(path.join(base, 'answer.json'), report.answer_source.answer);
          report.oracle = oracle.check(row.case_id, report.answer_source.answer, { finalFiles });
          write(path.join(base, 'oracle.json'), report.oracle);
          if (!report.oracle.structural_pass) report.status = 'failed';
        } catch (error) { report.status = 'failed'; report.reason = 'canonical_answer_or_oracle: ' + error.message; }
      }
    } catch (error) { report.status = 'failed'; report.reason = error.message; if (dispatched && !accounted) result.actual_cost_micros = null; result.stopped = true; }
    write(path.join(base, 'result.json'), report);
  }
  try { validate(plan, file, plan.runs.length); result.final_inputs_unchanged = sha(read(file)) === authorization; }
  catch (error) { result.final_inputs_unchanged = false; result.final_input_error = error.message; }
  if (!result.final_inputs_unchanged) result.stopped = true;
  write(path.join(plan.directory, 'result.json'), result);
  return result;
}
module.exports = { validate, run, skillEvidence, inventory, preserved, finalWorkspace };
if (require.main === module) {
  try {
    const [command, file, authorization, ...extra] = process.argv.slice(2);
    if (command !== 'run' || !file || !authorization || extra.length) throw Error('Usage: authoring-runner.cjs run <plan.json> <authorized-plan-sha256>');
    const result = run(file, authorization);
    console.log(JSON.stringify({ result: path.join(path.dirname(file), 'result.json'), stopped: result.stopped, actual_cost_micros: result.actual_cost_micros }));
    if (result.stopped || result.runs.some(row => row.status !== 'completed')) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
