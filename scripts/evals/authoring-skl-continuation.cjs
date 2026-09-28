// SPDX-License-Identifier: Apache-2.0
'use strict';
// One shot: only the 26 identities not dispatched by the pinned halted envelope.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), prep = require('./authoring-prepare.cjs');
const reconcile = require('./authoring-skl-reconciliation.cjs');
const { read, plain, safeChild, within, privateDirectory, noParentInstructions, noSecrets, invoke, frames, inspection, usd } = prior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const write = (file, value) => fs.writeFileSync(file, Buffer.isBuffer(value) || typeof value === 'string' ? value : JSON.stringify(value, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
const object = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...keys].sort());
const must = (ok, message) => { if (!ok) throw Error(message); };
const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const repository = path.resolve(__dirname, '../..'), phases = ['normal', 'inherited', 'confirmation'];
const files = ['authoring-skl-continuation', 'authoring-skl-reconciliation', 'authoring-skl-inspection', 'authoring-skl-review'].map(name => 'scripts/evals/' + name + '.cjs');
const sourceScope = [...new Set([...require('./authoring-qualification.cjs').sourceScope, ...files])];
const limits = Object.freeze({ slots: 26, cap_micros: 45500000, requests: 416, slot_micros: 1750000, slot_requests: 16, output_tokens: '2048', grant_micros: 100000000 });
const phasePath = (envelope, phase) => path.join(envelope.directory, 'phases', 'skill-authoring--' + phase);
function retained(file, hash) { const bytes = read(file, 16 * 1024 * 1024); must(digest(hash) && sha(bytes) === hash, 'Exact retained hash differs'); return JSON.parse(bytes); }
function allocation(slots) {
  const expected = require('./authoring-qualification.cjs').slots().filter(s => s.candidate === 'skill-authoring');
  must(equal(slots.filter(s => s.candidate === 'skill-authoring'), expected) && expected.length === 27 && expected[0].id === reconcile.consumed, 'Original fixed SKL allocation differs');
  return expected.slice(1);
}
function claimFile(oldClaim) { return path.join(path.dirname(oldClaim), 'vcp-cs1-skl-successor-' + reconcile.PIN.envelope + '.json'); }
function claimEnvelope(file, envelope, hash) {
  write(file, { schema: 'cs1-skl-one-shot-claim/1', grant: envelope.budget.grant, predecessor_envelope_sha256: reconcile.PIN.envelope, directory: envelope.directory, envelope_sha256: hash });
}
function sourceBudget(previous) {
  const old = previous.envelope.budget, doc = previous.manifest.doc.rows;
  const consumed = { id: reconcile.consumed, task_id: previous.manifest.task_id, status: 'failed', actual_cost_micros: 30538, observed_attempts: 12 };
  const taskIds = [...old.history.task_ids, ...old.current.task_ids, ...doc.map(r => r.task_id), consumed.task_id];
  must(new Set(taskIds).size === taskIds.length && taskIds.every(id => typeof id === 'string' && id.length), 'Historical/current task identity duplicated');
  const settled = old.current.actual_cost_micros + doc.reduce((n, r) => n + r.actual_cost_micros, 0) + consumed.actual_cost_micros;
  must(settled === 644378 && old.current.actual_cost_micros === 521696 && old.current.observed_attempts === 289 && settled + limits.cap_micros <= limits.grant_micros && old.active_micros === 0 && old.unresolved_micros === 0, 'Authenticated grant closure differs');
  return { grant: old.grant, cap_micros: limits.grant_micros, settled_micros: settled, settled_requests: 342, historical_micros_not_transferred: old.history.actual_cost_micros, task_ids: taskIds, consumed_ids: doc.map(r => r.id).concat(consumed.id), current: old.current, history: old.history, active_micros: 0, unresolved_micros: 0 };
}
function admit(budget, eligible, completed, next) {
  must(eligible.length === 26 && new Set(eligible.map(r => r.id)).size === 26 && eligible.every(r => r.candidate === 'skill-authoring' && r.id !== reconcile.consumed && r.cap_micros === limits.slot_micros && r.call_ceiling === 16 && r.output_tokens === '2048'), 'Only exact undispatched SKL slots admitted');
  must(equal(eligible.find(r => r.id === next.id), next) && completed.length < 26 && budget.cap_micros === limits.grant_micros && budget.active_micros === 0 && budget.unresolved_micros === 0, 'No eligible next slot or complete grant accounting');
  const ids = new Set(budget.consumed_ids), tasks = new Set(budget.task_ids); let cost = 0, requests = 0;
  for (const row of completed) {
    must(eligible.some(s => s.id === row.id) && !ids.has(row.id) && typeof row.task_id === 'string' && row.task_id.length && !tasks.has(row.task_id) && ['completed', 'failed'].includes(row.status), 'Duplicate, foreign or replayed slot/task accounting');
    must(Number.isSafeInteger(row.actual_cost_micros) && row.actual_cost_micros >= 0 && row.actual_cost_micros <= limits.slot_micros && Number.isInteger(row.observed_attempts) && row.observed_attempts >= 0 && row.observed_attempts <= 16 && row.active_micros === 0 && row.unresolved_micros === 0, 'Unknown or excessive successor liability');
    ids.add(row.id); tasks.add(row.task_id); cost += row.actual_cost_micros; requests += row.observed_attempts;
  }
  must(!ids.has(next.id) && cost + limits.slot_micros <= limits.cap_micros && requests + 16 <= limits.requests && budget.settled_micros + cost + limits.slot_micros <= limits.grant_micros, 'Successor or shared grant cannot reserve next full slot');
  return { grant: budget.grant, predecessor_settled_micros: budget.settled_micros, successor_settled_micros: cost, successor_requests: requests, reserved_micros: limits.slot_micros, reserved_requests: 16, current_authorization_settled_micros: budget.settled_micros + cost };
}
function describe(specFile, directory, preparedAt = Date.now()) {
  const specBytes = read(specFile, 262144), spec = JSON.parse(specBytes); noSecrets(spec);
  must(object(spec, ['predecessor', 'aggregate_cap_usd', 'aggregate_call_ceiling']) && prior.micros(spec.aggregate_cap_usd) === limits.cap_micros && spec.aggregate_call_ceiling === limits.requests, 'Exact USD45.50/416 successor specification required');
  const previous = reconcile.inspect(spec.predecessor), old = previous.envelope;
  return { schema: 'cs1-skl-continuation-envelope/1', authorization: false, model_calls: 0, directory, prepared_at: preparedAt, spec_source: plain(path.resolve(specFile)), spec_sha256: sha(specBytes), predecessor: spec.predecessor, reconciliation: previous.manifest, source: prep.identity(repository, sourceScope), toolchain: { node_version: process.version, node_executable: process.execPath, node_sha256: sha(read(process.execPath, 128 * 1024 * 1024)), platform: process.platform, architecture: process.arch }, executable: old.executable, executable_sha256: old.executable_sha256, runtime: old.runtime, profile_source: old.profile_source, profile_sha256: old.profile_sha256, provider_catalog: old.provider_catalog, provider_catalog_sha256: old.provider_catalog_sha256, candidate_assets: old.candidate_assets, budget: sourceBudget(previous), slots: allocation(old.slots), cohort: old.slots.filter(r => r.candidate === 'skill-authoring'), consumed: reconcile.consumed, aggregate_cap_micros: limits.cap_micros, aggregate_call_ceiling: limits.requests, output_bounds: old.output_bounds, inspection_limits: require('./authoring-skl-inspection.cjs').limits, common_claim: claimFile(previous.modules.runner.successorClaim()), benefit_rule: old.benefit_rule, confirmation_selection: old.confirmation_selection, permission_review: old.permission_review, limitations: ['Original halted envelope and failed nearest baseline remain unchanged.', 'New read-only capture supplements integrity/accounting evidence; it does not upgrade original execution.', 'No resume, replay, DOC allocation transfer, refresh probe or additional grant.', 'Original frozen checker build provenance is retained; successor sources are separately bound.'] };
}
function prepare(specFile, destination) {
  const directory = plain(path.resolve(destination));
  must(!fs.existsSync(directory) && !within(repository, directory) && !within(directory, repository), 'New external private successor directory required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const envelope = describe(specFile, directory);
  const forbidden = protectedRoots(envelope);
  must(!forbidden.some(root => within(root, directory) || within(directory, root)), 'Successor overlaps frozen evidence/source');
  const bytes = Buffer.from(JSON.stringify(envelope, null, 2) + '\n'), hash = sha(bytes);
  claimEnvelope(envelope.common_claim, envelope, hash); // permanent even if staging crashes
  fs.mkdirSync(directory, { mode: 0o700 }); write(path.join(directory, 'envelope.json'), bytes);
  write(path.join(directory, 'preparation-owner.json'), { schema: envelope.schema, envelope_sha256: hash });
  for (const name of ['phases', 'claims']) fs.mkdirSync(path.join(directory, name), { mode: 0o700 });
  return { envelope: path.join(directory, 'envelope.json'), sha256: hash, slots: 26, model_calls: 0, runnable: false };
}
function protectedRoots(envelope) { return [repository, envelope.predecessor.repository, envelope.predecessor.cs2_repository, path.dirname(envelope.predecessor.envelope), path.dirname(path.dirname(envelope.predecessor.capture.file)), envelope.budget.current.reference.repository, path.dirname(envelope.budget.current.reference.plan.file), ...envelope.budget.history.phases.map(p => path.dirname(p.reference.envelope.file))]; }
function halt(envelope, reason) { const file = path.join(envelope.directory, 'halt.json'); if (!fs.existsSync(file)) write(file, { schema: 'cs1-skl-continuation-halt/1', reason, action: 'Read-only reconciliation only. One-shot claim remains consumed.' }); }
function envelopeFor(file, expected) {
  const envelope = retained(file, expected);
  must(envelope.schema === 'cs1-skl-continuation-envelope/1' && path.resolve(file) === path.join(envelope.directory, 'envelope.json'), 'Owned successor path required');
  privateDirectory(envelope.directory); noParentInstructions(envelope.directory);
  must(equal(JSON.parse(read(path.join(envelope.directory, 'preparation-owner.json'))), { schema: envelope.schema, envelope_sha256: expected }), 'Successor preparation ownership differs');
  must(equal(JSON.parse(read(envelope.common_claim)), { schema: 'cs1-skl-one-shot-claim/1', grant: envelope.budget.grant, predecessor_envelope_sha256: reconcile.PIN.envelope, directory: envelope.directory, envelope_sha256: expected }), 'One-shot common claim differs');
  must(!fs.existsSync(path.join(envelope.directory, 'halt.json')), 'Successor permanently halted');
  try { must(equal(envelope, describe(envelope.spec_source, envelope.directory, envelope.prepared_at)), 'Frozen successor or predecessor identity changed'); } catch (error) { halt(envelope, 'Frozen evidence/input validation failed'); throw error; }
  return envelope;
}
function prerequisites(envelope, envelopeHash, phase) {
  must(phases.includes(phase), 'Unknown fixed SKL phase'); const refs = [envelope.reconciliation.doc.gate]; let selected = null;
  if (phase !== 'normal') {
    const normal = gateFor(envelope, envelopeHash, 'normal'); refs.push(normal);
    must(normal.decision.candidate_gates_pass && normal.decision.winning_case_ids.length > 0, 'Normal gates/benefit did not qualify inherited dispatch');
    if (phase === 'confirmation') { const inherited = gateFor(envelope, envelopeHash, 'inherited'); refs.push(inherited); must(inherited.decision.candidate_gates_pass, 'Inherited gates did not qualify confirmation'); selected = [...normal.decision.winning_case_ids].sort()[0]; }
  }
  return { refs: refs.map(({ file, sha256 }) => ({ file, sha256 })), selected };
}
function phaseRuntime(envelope, directory, rows, all) {
  const cases = rows.filter(row => { const item = all.find(t => t.task.id === row.case_id); return prep.affectedPaths(item.task, item.root).length; }).map(row => ({ workspace: path.join(directory, row.id, 'workspace'), case_id: row.case_id }));
  const bytes = Buffer.from(JSON.stringify({ schema_version: 1, cases }) + '\n');
  return { runtime: { ...envelope.runtime, checker: path.join(directory, 'runtime/vcp-authoring-check.exe'), cases_file: path.join(directory, 'runtime/authoring-cases.json'), cases_sha256: sha(bytes) }, bytes };
}
function derivePlan(envelope, envelopeHash, phase) {
  const gate = prerequisites(envelope, envelopeHash, phase), directory = phasePath(envelope, phase);
  const frozen = reconcile.inspect(envelope.predecessor), all = frozen.modules.runner.tasks();
  const slots = envelope.slots.filter(s => s.phase === phase).map(s => ({ ...s, case_id: s.case_id || gate.selected }));
  const { runtime } = phaseRuntime(envelope, directory, slots, all), sourceProfile = JSON.parse(read(envelope.profile_source));
  const candidate = require(path.join(envelope.predecessor.repository, 'scripts/evals/authoring-candidates.cjs'));
  const runs = slots.map(slot => {
    const { task, root } = all.find(i => i.task.id === slot.case_id);
    must(Array.isArray(task.context.tools) && task.context.tools.every(tool => ['vcp_read', 'vcp_list', 'vcp_search', 'vcp_patch', 'vcp_verify'].includes(tool)) && (!sourceProfile.canonical_tools || task.context.tools.every(t => sourceProfile.canonical_tools.includes(t))), 'Frozen tool ceiling differs');
    const profile = frozen.modules.prep.derivedProfile(sourceProfile, task, path.join(directory, slot.id, 'workspace'), envelope.provider_catalog, slot.cap_micros, slot.call_ceiling, runtime, root, slot.arm === 'candidate'); profile.canonical_tools = [...task.context.tools];
    return { ...slot, skill: candidate.selection(slot.arm, task), prompt_sha256: sha(Buffer.from(frozen.modules.prep.promptFor(task, root))), profile, files: frozen.modules.prep.preparedFiles(task, root), directories: frozen.modules.prep.preparedDirectories(task, root), scaffold_paths: [...frozen.modules.prep.scaffold(task, root).keys()], oracle: task.expected.oracle, status: 'not_run' };
  });
  return { schema: 'cs1-skl-continuation-phase/1', authorization: false, model_calls: 0, envelope_sha256: envelopeHash, qualification_prerequisites_pass: true, candidate: 'skill-authoring', phase, directory, executable: envelope.executable, selected_confirmation_case: gate.selected, prerequisites: gate.refs, runtime, permission_review: frozen.modules.prep.permissionReview(runtime), aggregate_cap_micros: slots.length * limits.slot_micros, aggregate_call_ceiling: slots.length * 16, runs };
}
function preparePhase(envelopeFile, envelopeHash, phase) {
  const envelope = envelopeFor(envelopeFile, envelopeHash);
  must(!fs.existsSync(path.join(envelope.directory, 'active-phase.json')), 'Active/crashed successor cannot prepare another phase');
  const plan = derivePlan(envelope, envelopeHash, phase), previous = reconcile.inspect(envelope.predecessor), all = previous.modules.runner.tasks();
  must(!fs.existsSync(plan.directory), 'Phase preparation is one shot'); fs.mkdirSync(plan.directory, { mode: 0o700 });
  write(path.join(plan.directory, 'preparation-owner.json'), { envelope_sha256: envelopeHash, phase }); fs.mkdirSync(path.join(plan.directory, 'runtime'), { mode: 0o700 });
  write(plan.runtime.checker, read(envelope.runtime.source_checker, 256 * 1024 * 1024)); write(plan.runtime.cases_file, phaseRuntime(envelope, plan.directory, plan.runs, all).bytes);
  for (const row of plan.runs) {
    const base = safeChild(plan.directory, row.id), workspace = path.join(base, 'workspace'), item = all.find(i => i.task.id === row.case_id);
    fs.mkdirSync(workspace, { recursive: true, mode: 0o700 }); fs.mkdirSync(path.join(base, 'data'), { mode: 0o700 });
    for (const relative of row.directories) fs.mkdirSync(safeChild(workspace, relative), { recursive: true, mode: 0o700 });
    for (const [relative, bytes] of item.files) write(safeChild(workspace, relative), bytes);
    write(path.join(base, 'prompt.txt'), previous.modules.prep.promptFor(item.task, item.root)); write(path.join(base, 'profile.json'), row.profile);
  }
  const file = path.join(plan.directory, 'plan.json'); write(file, plan); const hash = sha(read(file)); validatePhase(envelopeFile, envelopeHash, file, hash);
  return { plan: file, sha256: hash, slots: plan.runs.length, model_calls: 0, authorized: false };
}
function validatePhase(envelopeFile, envelopeHash, file, phaseHash, started = 0) {
  const envelope = envelopeFor(envelopeFile, envelopeHash), plan = retained(file, phaseHash), previous = reconcile.inspect(envelope.predecessor);
  must(path.resolve(file) === path.join(phasePath(envelope, plan.phase), 'plan.json') && equal(plan, derivePlan(envelope, envelopeHash, plan.phase)), 'Exact successor phase derivation differs');
  must(equal(JSON.parse(read(path.join(plan.directory, 'preparation-owner.json'))), { envelope_sha256: envelopeHash, phase: plan.phase }) && sha(read(plan.runtime.checker, 256 * 1024 * 1024)) === plan.runtime.checker_sha256 && sha(read(plan.runtime.cases_file)) === plan.runtime.cases_sha256, 'Successor phase ownership/checker/map differs');
  for (const [index, row] of plan.runs.entries()) {
    const base = safeChild(plan.directory, row.id);
    must(sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256 && equal(JSON.parse(read(path.join(base, 'profile.json'))), row.profile), 'Prompt/profile changed');
    if (index >= started) must(previous.modules.original.preserved(base, row) && fs.readdirSync(plain(path.join(base, 'data'))).length === 0, 'Undispatched workspace/data changed');
    else { previous.modules.original.finalWorkspace(base, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths); const f = path.join(base, 'result.json'); if (fs.existsSync(f)) { const r = JSON.parse(read(f)); must(r.workspace_sha256 === prep.identity(path.join(base, 'workspace'), ['.']).content_sha256, 'Completed workspace changed'); } else must(previous.modules.original.preserved(base, row) && fs.readdirSync(plain(path.join(base, 'data'))).length === 0, 'Unrun workspace/data changed'); }
  }
  return { envelope, plan, previous };
}
function runEvidence(base) {
  const names = fs.readdirSync(plain(base)).filter(n => !['workspace', 'data', 'result.json'].includes(n)).sort(); let total = 0;
  must(names.length <= 256, 'Successor evidence file bound exceeded');
  return sha(JSON.stringify(names.map(name => { const bytes = read(safeChild(base, name), 16 * 1024 * 1024); total += bytes.length; must(total <= 536870912, 'Successor evidence byte bound exceeded'); return { path: name, sha256: sha(bytes) }; })));
}
function resultEvidence(envelope, plan, result, phaseHash, activeAllowed = false) {
  const original = require(path.join(envelope.predecessor.repository, 'scripts/evals/authoring-runner.cjs')), claim = { envelope_sha256: plan.envelope_sha256, phase_sha256: phaseHash };
  must(equal(JSON.parse(read(path.join(plan.directory, 'execution-claim.json'))), claim) && result.schema === 'cs1-skl-continuation-result/1' && result.envelope_sha256 === plan.envelope_sha256 && result.phase_sha256 === phaseHash, 'Successor result/execution claim binding differs');
  const active = path.join(envelope.directory, 'active-phase.json'); must(activeAllowed || !fs.existsSync(active), 'Active/interrupted successor cannot be reviewed');
  must(Array.isArray(result.runs) && result.runs.length === plan.runs.length, 'Result coverage differs'); let unexecuted = false;
  for (const [index, row] of plan.runs.entries()) {
    const r = result.runs[index], base = path.join(plan.directory, row.id), slotClaim = path.join(envelope.directory, 'claims', row.id + '.json');
    must(r.id === row.id && r.case_id === row.case_id && r.arm === row.arm && ['not_run', 'completed', 'failed'].includes(r.status), 'Result fixed row differs');
    if (r.status === 'not_run') { unexecuted = true; must(!fs.existsSync(slotClaim) && !fs.existsSync(path.join(base, 'attempted.json')) && !fs.existsSync(path.join(base, 'result.json')) && original.preserved(base, row) && fs.readdirSync(plain(path.join(base, 'data'))).length === 0, 'Unrun slot has changed or attempted evidence'); continue; }
    must(!unexecuted && equal(JSON.parse(read(slotClaim)), { ...claim, slot: row.id }) && equal(JSON.parse(read(path.join(base, 'result.json'))), r) && r.evidence_sha256 === runEvidence(base) && r.workspace_sha256 === prep.identity(path.join(base, 'workspace'), ['.']).content_sha256, 'Executed slot claim/evidence changed');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    must(money.actual_cost_micros === r.actual_cost_micros && money.attempts.length === r.observed_attempts && r.observed_attempts <= row.call_ceiling, 'Canonical successor costs differ'); original.finalWorkspace(base, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths);
  }
}
function cumulativeAdmission(envelope, plan, current, phaseHash, nextIndex) {
  const completed = [];
  for (const name of fs.readdirSync(plain(path.join(envelope.directory, 'phases')))) {
    must(phases.some(p => name === 'skill-authoring--' + p), 'Unexpected successor phase');
    const directory = path.join(envelope.directory, 'phases', name), claimFile = path.join(directory, 'execution-claim.json');
    if (!fs.existsSync(claimFile)) continue;
    const claim = JSON.parse(read(claimFile)), other = directory === plan.directory ? plan : retained(path.join(directory, 'plan.json'), claim.phase_sha256);
    const result = directory === plan.directory ? current : JSON.parse(read(path.join(directory, 'result.json')));
    if (directory !== plan.directory) must(!result.stopped && result.final_inputs_unchanged === true, 'Prior phase is interrupted');
    resultEvidence(envelope, other, result, claim.phase_sha256, true);
    for (const r of result.runs.filter(r => r.status !== 'not_run')) completed.push({ id: r.id, task_id: r.scope?.task, status: r.status, actual_cost_micros: r.actual_cost_micros, observed_attempts: r.observed_attempts, active_micros: 0, unresolved_micros: 0 });
  }
  must(equal(fs.readdirSync(path.join(envelope.directory, 'claims')).sort(), completed.map(r => r.id + '.json').sort()), 'Cumulative claim coverage differs');
  return admit(envelope.budget, envelope.slots, completed, envelope.slots.find(s => s.id === plan.runs[nextIndex].id));
}
function composeProjection(envelope, plan, result, phaseHash, resultHash, previous) {
  must(result.schema === 'cs1-skl-continuation-result/1' && !result.stopped && result.final_inputs_unchanged === true, 'Only complete successor evidence can be projected');
  const old = plan.phase === 'normal';
  const rows = old ? [previous.row, ...plan.runs] : plan.runs;
  const retained = { ...previous.report, ...previous.manifest.supplemental, status: 'failed', original_report_sha256: reconcile.PIN.result, original_evidence_sha256: reconcile.PIN.evidence };
  const reports = old ? [retained, ...result.runs] : result.runs;
  const origins = reports.map(r => r.id === reconcile.consumed ? { id: r.id, kind: 'failed_predecessor', phase_sha256: reconcile.PIN.phase, result_sha256: reconcile.PIN.result, original_status: 'failed', evidence_directory: previous.capture_directory, workspace_directory: path.join(previous.original_base, 'workspace'), reconciliation_sha256: sha(Buffer.from(JSON.stringify(previous.manifest))) } : { id: r.id, kind: 'successor', phase_sha256: phaseHash, result_sha256: resultHash, evidence_directory: path.join(plan.directory, r.id), workspace_directory: path.join(plan.directory, r.id, 'workspace') });
  return { schema: 'cs1-skl-mixed-source-projection/1', envelope_sha256: plan.envelope_sha256, phase_sha256: phaseHash, successor_result_sha256: resultHash, phase: plan.phase, stopped: false, final_inputs_unchanged: true, actual_cost_micros: reports.filter(r => r.status !== 'not_run').reduce((n, r) => n + r.actual_cost_micros, 0), observed_attempts: reports.filter(r => r.status !== 'not_run').reduce((n, r) => n + r.observed_attempts, 0), origins, runs: reports, planned_rows: rows };
}
function inspectPhase(envelopeFile, envelopeHash, phase) {
  const envelope = envelopeFor(envelopeFile, envelopeHash), file = path.join(phasePath(envelope, phase), 'plan.json'), planBytes = read(file), phaseHash = sha(planBytes), plan = JSON.parse(planBytes);
  const checked = validatePhase(envelopeFile, envelopeHash, file, phaseHash, plan.runs.length), bytes = read(path.join(plan.directory, 'result.json')), result = JSON.parse(bytes);
  resultEvidence(envelope, plan, result, phaseHash);
  const projection = composeProjection(envelope, plan, result, phaseHash, sha(bytes), checked.previous), projectionBytes = Buffer.from(JSON.stringify(projection, null, 2) + '\n');
  const projectionFile = path.join(plan.directory, 'projection.json'); if (fs.existsSync(projectionFile)) must(read(projectionFile).equals(projectionBytes), 'Mixed-source projection changed');
  return { envelope, plan: { ...plan, runs: projection.planned_rows }, dispatchPlan: plan, result: projection, phaseHash, resultHash: sha(projectionBytes), projectionBytes, previous: checked.previous, runner: checked.previous.modules.runner };
}
function gateFor(envelope, envelopeHash, phase) { return require('./authoring-skl-review.cjs').gateFor(envelope, envelopeHash, phase); }
function run(envelopeFile, envelopeHash, file, phaseHash, call = invoke) {
  let envelope, plan, previous;
  try { ({ envelope, plan, previous } = validatePhase(envelopeFile, envelopeHash, file, phaseHash)); }
  catch (error) {
    const bytes = read(envelopeFile), trusted = JSON.parse(bytes);
    if (sha(bytes) === envelopeHash && trusted.schema === 'cs1-skl-continuation-envelope/1' && path.resolve(envelopeFile) === path.join(trusted.directory, 'envelope.json')) halt(trusted, 'Exact authorized phase validation failed');
    throw error;
  }
  const { original, capture, runner: frozenRunner } = previous.modules;
  const sourceProfile = JSON.parse(read(envelope.profile_source));
  if (require('./developer-prepare.cjs').qualificationEnds(sourceProfile) <= Date.now() + plan.runs.length * (sourceProfile.deadline_seconds + 180) * 1000) throw Error('Qualification window does not cover the whole bounded phase');
  const claim = { envelope_sha256: envelopeHash, phase_sha256: phaseHash };
  // Exclusive global claim survives crashes and prevents concurrent/replayed phases.
  const active = path.join(envelope.directory, 'active-phase.json');
  write(active, claim);
  try { write(path.join(plan.directory, 'execution-claim.json'), claim); }
  catch (error) { halt(envelope, 'Phase execution claim already exists'); throw error; }
  const result = { schema: 'cs1-skl-continuation-result/1', ...claim, quality: 'pending_independent_owner_review', actual_cost_micros: 0, observed_attempts: 0, stopped: false, candidate_stopped: false, runs: plan.runs.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null })) };
  for (let index = 0; index < plan.runs.length; index++) {
    if (result.stopped || result.candidate_stopped) break;
    const row = plan.runs[index], report = result.runs[index], base = safeChild(plan.directory, row.id);
    let dispatched = false, accounted = false;
    try {
      validatePhase(envelopeFile, envelopeHash, file, phaseHash, index);
      if (require('./developer-prepare.cjs').profileReasons(JSON.parse(read(envelope.profile_source))).length) throw Error('Provider qualification expired before dispatch');
      if (!equal(JSON.parse(read(active)), claim)) throw Error('Active envelope ownership changed');
      write(path.join(base, 'budget-admission.json'), cumulativeAdmission(envelope, plan, result, phaseHash, index));
      write(path.join(envelope.directory, 'claims', row.id + '.json'), { ...claim, slot: row.id });
      const profile = row.profile, args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', usd(row.cap_micros), '--autonomy', profile.maximum_autonomy];
      if (row.skill) args.push('--skill', row.skill);
      write(path.join(base, 'attempted.json'), { ...claim, args });
      dispatched = true;
      const start = Date.now(), execution = call(plan.executable, args, (profile.deadline_seconds + 180) * 1000);
      report.latency_ms = Date.now() - start;
      write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr);
      if (execution.error) throw Error('CLI interrupted; reconcile unknown liability');
      const output = frames(execution.stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
      if (!accepted?.scope?.task || !final?.conditions) throw Error('Missing durable task result');
      report.scope = accepted.scope;
      const reader = require('./authoring-skl-inspection.cjs').createInspection({ executable: plan.executable, base, task: accepted.scope.task, invoke: call, save: (name, value) => write(path.join(base, name), value) });
      const evidence = {};
      for (const view of ['costs', 'routing', 'outputs', 'context', 'tools', 'verification']) { evidence[view] = inspection(plan, base, accepted.scope.task, view, reader.call); write(path.join(base, view + '.json'), evidence[view]); }
      const responses = capture.captureResponses(plan, base, evidence.outputs, reader.call);
      const money = prior.accounting(evidence.costs, row.cap_micros);
      reconcile.canonicalViews({ routing: evidence.routing, outputs: evidence.outputs, context: evidence.context, tools: evidence.tools, verification: evidence.verification }, evidence.costs, row.profile, accepted.scope);
      reconcile.canonicalResponseLinks(responses, money.attempts);
      report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length;
      result.actual_cost_micros += money.actual_cost_micros; result.observed_attempts += money.attempts.length; accounted = true;
      if (money.attempts.length > row.call_ceiling || result.actual_cost_micros > plan.aggregate_cap_micros || result.observed_attempts > plan.aggregate_call_ceiling) throw Error('Observed fixed budget exceeded');
      const finalFiles = original.finalWorkspace(base, row, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths); report.preserved = true;
      report.status = final.conditions.completed && execution.status === 0 ? 'completed' : 'failed'; report.conditions = final.conditions;
      // Inspect settled request contexts even when the native task fails.
      if (money.attempts.some(a => a.phase === 'settled')) report.skill_evidence = original.skillEvidence(plan, base, row, evidence.context, money.attempts, reader.call);
      if (evidence.tools.some(page => page.gaps.some(gap => !prior.privacyGap(gap, gap.artifact)))) throw Error('Canonical tool evidence incomplete');
      report.tool_audit = frozenRunner.auditTools(responses, profile.canonical_tools);
      report.native_check = frozenRunner.nativeCheck(plan, base, evidence.verification, evidence.tools, reader.call);
      if (row.scaffold_paths.length && report.native_check.status !== 'passed') report.status = 'failed';
      // Failed and incomplete provider streams are retained before interpretation.
      let answer;
      try { answer = capture.responseAnswer(responses, money.attempts); }
      catch (error) { report.status = 'failed'; report.answer_error = error.message; }
      const item = frozenRunner.tasks().find(item => item.task.id === row.case_id);
      if (answer) {
        report.answer_source = answer; write(path.join(base, 'answer.json'), answer.answer);
        const oracle = require(path.join(envelope.predecessor.repository, 'scripts/evals', row.case_id.endsWith('-v3') ? 'authoring-followup-oracle.cjs' : 'authoring-oracle.cjs'));
        report.oracle = oracle.check(row.case_id, answer.answer, { finalFiles, fixtureRoot: item.root }); write(path.join(base, 'oracle.json'), report.oracle);
        if (!report.oracle.structural_pass) report.status = 'failed';
      }
      const definition = JSON.parse(read(safeChild(item.root, item.task.expected.oracle.path)));
      must(sha(read(safeChild(item.root, item.task.expected.oracle.path))) === item.task.expected.oracle.sha256, 'Frozen oracle changed');
      report.canary_disclosed = frozenRunner.canaryDisclosed(base, definition, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths, finalFiles);
      if (report.canary_disclosed) throw Error('Retained response or output disclosed a forbidden canary');
      report.workspace_sha256 = prep.identity(path.join(base, 'workspace'), ['.']).content_sha256;
      report.evidence_sha256 = runEvidence(base);
    } catch (error) {
      report.status = 'failed'; report.reason = error.message; if (dispatched && !accounted) result.actual_cost_micros = null; result.stopped = true;
      try { report.workspace_sha256 = prep.identity(path.join(base, 'workspace'), ['.']).content_sha256; report.evidence_sha256 = runEvidence(base); } catch (retentionError) { report.retention_error = retentionError.message; }
      halt(envelope, 'Integrity/accounting stop; inspect retained local result');
    }
    write(path.join(base, 'result.json'), report);
    // Complete the current triplet before a deterministic candidate failure ends the phase.
    if (row.position === 2 && result.runs.filter(r => r.case_id === row.case_id).some(r => r.arm === 'candidate' && r.status !== 'completed')) result.candidate_stopped = true;
  }
  try {
    // A halt intentionally prevents future validation; still check exact identity here.
    if (!equal(envelope, describe(envelope.spec_source, envelope.directory, envelope.prepared_at)) || sha(read(file)) !== phaseHash) throw Error('Final frozen identity changed');
    for (const [index, row] of plan.runs.entries()) {
      const base = path.join(plan.directory, row.id), report = result.runs[index];
      if (report.workspace_sha256 && prep.identity(path.join(base, 'workspace'), ['.']).content_sha256 !== report.workspace_sha256) throw Error('Earlier completed workspace changed');
    }
    result.final_inputs_unchanged = true;
  } catch (error) { result.final_inputs_unchanged = false; result.final_input_error = error.message; result.stopped = true; halt(envelope, 'Final identity drift'); }
  write(path.join(plan.directory, 'result.json'), result);
  if (!result.stopped) { if (!equal(JSON.parse(read(active)), claim)) { halt(envelope, 'Active ownership changed'); throw Error('Active ownership changed'); } fs.unlinkSync(active); }
  return result;
}
module.exports = { run, prepare, preparePhase, describe, envelopeFor, derivePlan, validatePhase, resultEvidence, inspectPhase, composeProjection, runEvidence, sourceBudget, admit, allocation, limits, claimFile, claimEnvelope, sourceScope, protectedRoots, halt, retained, write, sha, phasePath, cumulativeAdmission };

if (require.main === module) {
  try {
    const [command, ...args] = process.argv.slice(2); let result;
    if (command === 'prepare' && args.length === 2) result = prepare(...args);
    else if (command === 'phase' && args.length === 3) result = preparePhase(...args);
    else if (command === 'run' && args.length === 4) result = run(...args);
    else throw Error('Usage: authoring-skl-continuation.cjs prepare <spec> <new-private-directory> | phase <envelope> <hash> <normal|inherited|confirmation> | run <envelope> <hash> <phase-plan> <phase-hash>');
    console.log(JSON.stringify(command === 'run' ? { stopped: result.stopped, candidate_stopped: result.candidate_stopped, actual_cost_micros: result.actual_cost_micros, observed_attempts: result.observed_attempts } : result));
    if (result.stopped || result.runs?.some(row => row.status !== 'completed')) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
