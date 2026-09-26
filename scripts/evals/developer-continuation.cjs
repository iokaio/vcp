// SPDX-License-Identifier: Apache-2.0
'use strict';
// Fresh, independently authorized CS-2 allocation after an owner reboot stop.
// The predecessor stays permanently halted. Its eight outcomes are read-only.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const prep = require('./developer-prepare.cjs'), old = require('./developer-runner.cjs');
const reconcile = require('./developer-reconcile.cjs');
const cs1 = require('./authoring-runner.cjs'), authoring = require('./authoring-prepare.cjs');
const candidates = require('./developer-candidates.cjs'), oracle = require('./developer-oracle.cjs');
const { plain, read, write, safeChild, frames, inspection, invoke, usd, within, noParentInstructions, privateDirectory, noSecrets } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const identity = authoring.identity;
const limits = Object.freeze({ runs: 46, cap_micros: 92000000, requests: 736, slot_micros: 2000000, slot_requests: 16, output_tokens: '2048' });
const sourceScope = [...prep.sourceScope, 'scripts/evals/developer-reconcile.cjs', 'scripts/evals/developer-continuation.cjs', 'scripts/evals/developer-continuation-review.cjs'];
const schema = 'cs-2-developer-continuation/1';
const { runEvidence, skillEvidence, nativeCheck, canaryDisclosed, captureResponses, responseAnswer } = old;
function requireThat(condition, message) { if (!condition) throw Error(message); }
function object(value, names) { return value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...names].sort()); }
function claimed(plan) { return new Set(plan.runs.filter(row => fs.existsSync(path.join(plan.directory, 'claims', row.id + '.json'))).map(row => row.id)); }
function predecessor(reference) {
  requireThat(object(reference, ['file', 'sha256']) && typeof reference.file === 'string' && path.isAbsolute(reference.file) && /^[a-f0-9]{64}$/.test(reference.sha256), 'Exact absolute predecessor plan and hash required');
  return reconcile.inspect(reference.file, reference.sha256);
}
function continuationProfile(profile, original, at) {
  const reasons = prep.profileReasons(profile, at);
  requireThat(!reasons.length, 'Qualified continuation profile required: ' + reasons.join('; '));
  const stable = value => { const copy = { ...value }; delete copy.provider; delete copy.catalog; delete copy.budget_usd; return copy; };
  requireThat(equal(stable(profile), stable(original)), 'Continuation must preserve the original profile authority and request settings');
  const provider = profile.provider, before = original.provider;
  for (const key of ['model', 'endpoint', 'deny_data_collection', 'require_zdr', 'provider_preferences_qualified', 'responses_text_tools', 'required_parameters', 'byte_ceiling_qualified', 'request_price_limit']) {
    requireThat(equal(provider.compatibility?.[key], before.compatibility?.[key]), 'Continuation provider compatibility identity changed: ' + key);
  }
  for (const key of ['model', 'provider', 'currency']) requireThat(equal(provider.price?.[key], before.price?.[key]), 'Continuation provider tariff identity changed: ' + key);
}
function caseMap(directory, runtime, entries) {
  const cases = Buffer.from(JSON.stringify({ schema_version: 1, cases: entries.filter(entry => entry.item.edits.length).map(entry => ({ workspace: path.join(directory, entry.id, 'workspace'), case_id: entry.item.task.id })) }) + '\n');
  return { runtime: { ...runtime, checker: path.join(directory, 'runtime/vcp-developer-check.exe'), cases_file: path.join(directory, 'runtime/developer-cases.json'), cases_sha256: sha(cases) }, cases };
}
function describe(specFile, directory, at = Date.now()) {
  const bytes = read(specFile, 64 * 1024), spec = JSON.parse(bytes); noSecrets(spec);
  requireThat(object(spec, ['predecessor', 'profile', 'aggregate_cap_usd', 'aggregate_call_ceiling']) && prior.micros(spec.aggregate_cap_usd) === limits.cap_micros && spec.aggregate_call_ceiling === limits.requests, 'Fresh continuation requires exactly USD 92 and 736 requests');
  const previous = predecessor(spec.predecessor), original = previous.plan;
  requireThat(previous.retained.length === 8 && previous.pending.length === limits.runs, 'Continuation requires eight retained and forty-six pending runs');
  const profileFile = plain(path.resolve(spec.profile)), profileBytes = read(profileFile, 1024 * 1024), profile = JSON.parse(profileBytes); noSecrets(profile);
  continuationProfile(profile, JSON.parse(read(original.profile_source)), at);
  const providerCatalog = plain(path.resolve(profile.catalog)), catalogBytes = read(providerCatalog);
  const catalog = JSON.parse(read(path.join(path.dirname(original.executable), 'skills/builtin/catalog.json')));
  const pending = new Set(previous.pending.map(row => row.id));
  const { items } = prep.tasks(), entries = prep.order(items).filter(entry => pending.has(entry.id));
  requireThat(equal(entries.map(entry => entry.id), previous.pending.map(row => row.id)), 'Pending original cohort differs');
  const runtime = prep.checkerRuntime({ checker: original.runtime.source_checker, build_receipt: original.runtime.build_receipt });
  const staged = caseMap(directory, runtime, entries), grader = prep.graderIdentity({ node: original.grader.node, node_sha256: original.grader.node_sha256 });
  requireThat(equal(grader, original.grader), 'Original grading identity changed');
  const runs = entries.map(({ item, arm, id }) => {
    const row = previous.pending.find(row => row.id === id);
    return { ...row, cap_micros: limits.slot_micros, profile: { ...prep.derivedProfile(profile, item, path.join(directory, id, 'workspace'), providerCatalog, staged.runtime, arm), budget_usd: usd(limits.slot_micros) } };
  });
  return {
    schema, runnable: true, authorization: false, model_calls: 0, directory, prepared_at: String(at),
    predecessor: { ...spec.predecessor, result_sha256: previous.result_sha256, halt_sha256: previous.halt_sha256 },
    historical_accounting: { actual_cost_micros: previous.actual_cost_micros, observed_attempts: previous.observed_attempts, allocation_transferred_micros: 0 },
    retained: previous.retained.map(({ row, report, result_sha256 }) => ({ id: row.id, status: report.status, result_sha256, evidence_sha256: report.evidence_sha256, workspace_sha256: report.workspace_sha256, evidence_directory: path.join(original.directory, row.id) })),
    cohort: original.runs.map(row => row.id), blocks: original.blocks, runs, limits,
    executable: original.executable, executable_sha256: original.executable_sha256, assets: original.assets, candidate_assets: original.candidate_assets,
    fixture_revision: original.fixture_revision, fixture_sha256: original.fixture_sha256, rubric_sha256: original.rubric_sha256, held_out: original.held_out,
    source: identity(repository, sourceScope), spec_source: plain(path.resolve(specFile)), spec_sha256: sha(bytes),
    profile_source: profileFile, profile_sha256: sha(profileBytes), provider_catalog: providerCatalog, provider_catalog_sha256: sha(catalogBytes),
    toolchain: { node_version: process.version, node_executable: process.execPath, node_sha256: sha(read(process.execPath, 256 * 1024 * 1024)), platform: process.platform, architecture: process.arch },
    runtime: staged.runtime, grader, budget_preflight: authoring.budgetPreflight(profile, limits.slot_micros), permission_review: prep.permissionReview(staged.runtime, grader), benefit_rule: prep.benefitRule,
    limitations: ['Only forty-six previously undispatched original IDs may run. Eight predecessor outcomes, including the failed eighth run, remain unchanged.', 'Fresh USD 92 allocation; no old allocation transfers, no retries or replays. Refresh probes are separate and cannot consume this envelope.', 'The predecessor stays permanently halted. An unknown charge, changed identity, authority failure or interruption permanently halts this continuation.'],
  };
}
function prepare(specFile, destination) {
  const directory = plain(path.resolve(destination));
  requireThat(!within(repository, directory) && !within(directory, repository) && !fs.existsSync(directory), 'New private directory outside repository required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const plan = describe(specFile, directory), { items } = prep.tasks();
  const predecessorDirectory = path.dirname(plan.predecessor.file);
  requireThat(!within(predecessorDirectory, directory) && !within(directory, predecessorDirectory), 'Continuation directory must not overlap the read-only predecessor');
  const entries = prep.order(items).filter(entry => plan.runs.some(row => row.id === entry.id)), { cases } = caseMap(directory, plan.runtime, entries);
  fs.mkdirSync(directory, { mode: 0o700 });
  write(path.join(directory, 'preparation-owner.json'), { schema, spec_sha256: plan.spec_sha256 });
  for (const name of ['runtime', 'claims']) fs.mkdirSync(path.join(directory, name), { mode: 0o700 });
  fs.writeFileSync(plan.runtime.checker, read(plan.runtime.source_checker, 256 * 1024 * 1024), { flag: 'wx', mode: 0o600 });
  fs.writeFileSync(plan.runtime.cases_file, cases, { flag: 'wx', mode: 0o600 });
  requireThat(sha(read(plan.runtime.checker, 256 * 1024 * 1024)) === plan.runtime.checker_sha256 && sha(read(plan.runtime.cases_file)) === plan.runtime.cases_sha256, 'Checker runtime changed while staging');
  for (const row of plan.runs) {
    const item = items.find(item => item.task.id === row.case_id), base = safeChild(directory, row.id), workspace = path.join(base, 'workspace');
    fs.mkdirSync(workspace, { recursive: true, mode: 0o700 }); fs.mkdirSync(path.join(base, 'data'), { mode: 0o700 });
    for (const relative of row.directories) fs.mkdirSync(safeChild(workspace, relative), { recursive: true, mode: 0o700 });
    for (const [relative, bytes] of item.files) fs.writeFileSync(safeChild(workspace, relative), bytes, { flag: 'wx', mode: 0o600 });
    write(path.join(base, 'prompt.txt'), item.task.prompt); write(path.join(base, 'profile.json'), row.profile);
  }
  write(path.join(directory, 'plan.json'), plan);
  return { plan: path.join(directory, 'plan.json'), sha256: sha(read(path.join(directory, 'plan.json'), 16 * 1024 * 1024)), runs: plan.runs.length, model_calls: 0, aggregate_cap_micros: limits.cap_micros, aggregate_call_ceiling: limits.requests };
}
function identical(plan, file) {
  requireThat(plan.schema === schema && plain(path.dirname(path.resolve(file))) === plan.directory && /^[0-9]+$/.test(plan.prepared_at), 'Continuation plan identity differs');
  requireThat(equal(JSON.parse(read(path.join(plan.directory, 'preparation-owner.json'))), { schema, spec_sha256: plan.spec_sha256 }), 'Continuation preparation ownership changed');
  requireThat(equal(plan, describe(plan.spec_source, plan.directory, Number(plan.prepared_at))), 'Frozen continuation identity or allocation changed');
}
function requireActive(plan) { requireThat(!fs.existsSync(path.join(plan.directory, 'halt.json')), 'Continuation halted: read-only reconciliation only'); }
function validate(plan, file, started = claimed(plan)) {
  noParentInstructions(plan.directory); privateDirectory(plan.directory); requireActive(plan); identical(plan, file);
  requireThat(!prep.profileReasons(JSON.parse(read(plan.profile_source)), Date.now()).length, 'Provider qualification is not current');
  requireThat(sha(read(plan.runtime.checker, 256 * 1024 * 1024)) === plan.runtime.checker_sha256 && sha(read(plan.runtime.cases_file)) === plan.runtime.cases_sha256, 'Staged checker or case map changed');
  for (const row of plan.runs) {
    const base = safeChild(plan.directory, row.id);
    requireThat(sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256 && equal(JSON.parse(read(path.join(base, 'profile.json'))), row.profile), 'Prepared prompt or profile changed');
    if (!started.has(row.id)) requireThat(cs1.preserved(base, row) && !fs.readdirSync(plain(path.join(base, 'data'))).length, 'Unstarted workspace or data changed');
    else if (fs.existsSync(path.join(base, 'result.json'))) {
      const report = JSON.parse(read(path.join(base, 'result.json')));
      if (report.workspace_sha256) requireThat(identity(path.join(base, 'workspace'), ['.']).content_sha256 === report.workspace_sha256, 'Earlier completed workspace changed');
    }
  }
}
function campaignClaim() { return path.join(path.dirname(old.campaignClaim()), 'vcp-cs2-developer-continuation.json'); }
function halt(plan, reason) {
  const file = path.join(plan.directory, 'halt.json');
  if (!fs.existsSync(file)) write(file, { schema: 'cs-2-developer-continuation-halt/1', reason, action: 'Read-only reconciliation. This continuation cannot resume or replay.' });
}
function admission(plan) {
  let cost = 0, attempts = 0;
  const planHash = sha(read(path.join(plan.directory, 'plan.json'), 16 * 1024 * 1024));
  const expected = new Set(plan.runs.map(row => row.id + '.json'));
  for (const name of fs.readdirSync(path.join(plan.directory, 'claims'))) requireThat(expected.has(name) || candidates.ids.some(id => name === `block-${id}.json`), 'Unexpected continuation claim');
  for (const row of plan.runs) {
    if (!fs.existsSync(path.join(plan.directory, 'claims', row.id + '.json'))) continue;
    requireThat(equal(JSON.parse(read(path.join(plan.directory, 'claims', row.id + '.json'))), { plan_sha256: planHash, directory: plan.directory, run: row.id }), 'Continuation run claim changed');
    const base = path.join(plan.directory, row.id), report = JSON.parse(read(path.join(base, 'result.json')));
    requireThat(Number.isSafeInteger(report.actual_cost_micros) && report.evidence_sha256 === runEvidence(base), 'Unknown accounting or changed retained evidence');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    requireThat(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts && money.attempts.length <= row.call_ceiling, 'Canonical continuation accounting differs');
    cost += money.actual_cost_micros; attempts += money.attempts.length;
  }
  requireThat(Number.isSafeInteger(cost) && cost + limits.slot_micros <= limits.cap_micros && attempts + limits.slot_requests <= limits.requests, 'Fresh continuation allocation cannot reserve the next full slot');
  return { actual_cost_micros: cost, observed_attempts: attempts, reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests, remaining_micros: limits.cap_micros - cost, remaining_requests: limits.requests - attempts };
}
function inspectBlock(file, authorization, name) {
  const bytes = read(file, 16 * 1024 * 1024); requireThat(sha(bytes) === authorization, 'Authorization must name the exact continuation plan hash');
  const plan = JSON.parse(bytes); requireActive(plan); identical(plan, file); requireThat(candidates.ids.includes(name), 'Unknown continuation block');
  const previous = predecessor({ file: plan.predecessor.file, sha256: plan.predecessor.sha256 });
  const resultBytes = read(path.join(plan.directory, `result-${name}.json`), 16 * 1024 * 1024), fresh = JSON.parse(resultBytes);
  const freshRows = plan.runs.filter(row => row.block === name);
  requireThat(fresh.schema === 'cs-2-developer-continuation-block/1' && fresh.plan_sha256 === authorization && fresh.block === name && !fresh.stopped && fresh.final_inputs_unchanged === true && equal(fresh.runs.map(row => row.id), freshRows.map(row => row.id)), 'Complete unchanged continuation block required');
  requireThat(equal(JSON.parse(read(campaignClaim())), { plan_sha256: authorization, directory: plan.directory }), 'Continuation repository claim changed');
  requireThat(equal(JSON.parse(read(path.join(plan.directory, 'claims', `block-${name}.json`))), { plan_sha256: authorization, directory: plan.directory, block: name }), 'Continuation block claim changed');
  const expectedClaims = new Set(plan.runs.map(row => row.id + '.json'));
  for (const claim of fs.readdirSync(path.join(plan.directory, 'claims'))) requireThat(expectedClaims.has(claim) || candidates.ids.some(id => claim === `block-${id}.json`), 'Unexpected continuation claim');
  let cost = 0, attempts = 0;
  for (const row of freshRows) {
    const report = fresh.runs.find(report => report.id === row.id), base = path.join(plan.directory, row.id);
    requireThat(equal(JSON.parse(read(path.join(plan.directory, 'claims', row.id + '.json'))), { plan_sha256: authorization, directory: plan.directory, run: row.id }), 'Continuation run claim changed');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    requireThat(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts && money.attempts.length <= row.call_ceiling, 'Composite canonical accounting differs');
    cost += money.actual_cost_micros; attempts += money.attempts.length;
  }
  requireThat(cost === fresh.actual_cost_micros && attempts === fresh.observed_attempts && cost <= limits.cap_micros && attempts <= limits.requests, 'Composite continuation block accounting totals differ');
  const rows = [], reports = [];
  for (const original of previous.plan.runs.filter(row => row.block === name)) {
    const inherited = previous.retained.find(entry => entry.row.id === original.id), row = inherited ? original : freshRows.find(row => row.id === original.id);
    const report = inherited ? inherited.report : fresh.runs.find(report => report.id === original.id);
    const base = path.join(inherited ? previous.plan.directory : plan.directory, original.id);
    requireThat(row && report && ['completed', 'failed'].includes(report.status) && equal(JSON.parse(read(path.join(base, 'result.json'))), report) && report.evidence_sha256 === runEvidence(base) && report.workspace_sha256 === identity(path.join(base, 'workspace'), ['.']).content_sha256, 'Composite retained run evidence or workspace changed');
    rows.push({ ...row, evidence_directory: base }); reports.push(report);
  }
  requireThat(rows.length === 18, 'Complete original eighteen-row block required');
  const result = { ...fresh, schema: 'cs-2-developer-continuation-composite/1', predecessor_result_sha256: previous.result_sha256, fresh_result_sha256: sha(resultBytes), runs: reports };
  return { plan, rows, result, result_sha256: sha(JSON.stringify(result)) };
}
function previousDecided(plan, authorization, name) {
  const composed = inspectBlock(path.join(plan.directory, 'plan.json'), authorization, name);
  const decision = JSON.parse(read(path.join(plan.directory, `decision-${name}.json`)));
  requireThat(decision.schema === 'cs-2-developer-decision/1' && decision.plan_sha256 === authorization && decision.block === name && decision.result_sha256 === composed.result_sha256, 'Previous composite block requires its bound blind decision');
}
function windowCovers(plan, rows, now = Date.now()) { return old.windowCovers(plan, rows, now); }
function run(file, authorization, block, call = invoke) {
  const bytes = read(file, 16 * 1024 * 1024);
  if (sha(bytes) !== authorization) throw Error('Authorization must name the exact prepared plan hash');
  if (!candidates.ids.includes(block)) throw Error('Unknown campaign block');
  const plan = JSON.parse(bytes);
  try { validate(plan, file); }
  catch (error) {
    // A hash-authorized plan with its own unchanged preparation owner is a
    // trusted halt destination. Identity or qualification drift is permanent,
    // including before a later block acquires a new dispatch claim.
    if (plan.schema === schema && plain(path.dirname(path.resolve(file))) === plan.directory
      && equal(JSON.parse(read(path.join(plan.directory, 'preparation-owner.json'))), { schema, spec_sha256: plan.spec_sha256 })) halt(plan, 'Continuation identity or qualification validation failed: ' + error.message);
    throw error;
  }
  const position = candidates.ids.indexOf(block);
  for (const earlier of candidates.ids.slice(0, position)) previousDecided(plan, authorization, earlier);
  const rows = plan.runs.filter(row => row.block === block);
  if (!windowCovers(plan, rows)) throw Error('The provider qualification window cannot cover the whole block; no claim was consumed');
  const claim = { plan_sha256: authorization, directory: plan.directory }, claimFile = campaignClaim();
  if (fs.existsSync(claimFile)) { if (!equal(JSON.parse(read(claimFile)), claim)) throw Error('Another prepared plan already holds the fresh CS-2 continuation authorization'); }
  else write(claimFile, claim);
  const active = path.join(plan.directory, 'active-block.json');
  if (fs.existsSync(active)) { halt(plan, 'An interrupted block left an active claim'); throw Error('An interrupted block requires reconciliation'); }
  // Exclusive block claims survive crashes and prevent concurrent or replayed blocks.
  write(path.join(plan.directory, 'claims', `block-${block}.json`), { ...claim, block });
  write(active, { ...claim, block });
  const result = { schema: 'cs-2-developer-continuation-block/1', plan_sha256: authorization, block, quality: 'pending_functional_grading_and_blind_review', actual_cost_micros: 0, observed_attempts: 0, stopped: false, runs: rows.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null })) };
  for (let index = 0; index < rows.length; index++) {
    if (result.stopped) break;
    const row = rows[index], report = result.runs[index], base = safeChild(plan.directory, row.id);
    let dispatched = false, accounted = false;
    try {
      if (sha(read(file, 16 * 1024 * 1024)) !== authorization) throw Error('Authorized plan changed');
      validate(plan, file);
      if (!equal(JSON.parse(read(active)), { ...claim, block })) throw Error('Active block ownership changed');
      write(path.join(base, 'budget-admission.json'), admission(plan));
      write(path.join(plan.directory, 'claims', row.id + '.json'), { ...claim, run: row.id });
      const profile = row.profile, args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', usd(row.cap_micros), '--autonomy', profile.maximum_autonomy];
      for (const skill of row.skills) args.push('--skill', skill);
      write(path.join(base, 'attempted.json'), { ...claim, args });
      dispatched = true;
      const start = Date.now(), execution = call(plan.executable, args, (profile.deadline_seconds + 180) * 1000);
      report.latency_ms = Date.now() - start;
      write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr);
      if (execution.error) throw Error('CLI interrupted; reconcile unknown liability');
      const output = frames(execution.stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
      if (!accepted?.scope?.task || !final?.conditions) throw Error('Missing durable task result');
      report.scope = accepted.scope;
      const evidence = {};
      for (const view of ['costs', 'routing', 'outputs', 'context', 'tools', 'verification']) { evidence[view] = inspection(plan, base, accepted.scope.task, view, call); write(path.join(base, view + '.json'), evidence[view]); }
      const money = prior.accounting(evidence.costs, row.cap_micros);
      report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length;
      result.actual_cost_micros += money.actual_cost_micros; result.observed_attempts += money.attempts.length; accounted = true;
      if (money.attempts.length > row.call_ceiling) throw Error('Observed per-run request ceiling exceeded');
      // Writes outside the editable paths, deletions or scaffold changes are authority failures.
      const finalFiles = cs1.finalWorkspace(base, row, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths);
      report.preserved = true;
      report.status = final.conditions.completed && execution.status === 0 ? 'completed' : 'failed'; report.conditions = final.conditions;
      if (money.attempts.some(a => a.phase === 'settled')) report.skill_evidence = skillEvidence(plan, base, row, evidence.context, money.attempts, call);
      report.native_check = row.write ? nativeCheck(plan, base, evidence.verification, evidence.tools, call) : { status: 'not_applicable' };
      if (row.write && report.native_check.status !== 'passed') report.status = 'failed';
      // Failed runs still have actual output for blind readers and disclosure
      // checks. Successful answer extraction never upgrades their run status.
      const responses = captureResponses(plan, base, evidence.outputs, call);
      try {
        report.answer_source = responseAnswer(responses, money.attempts);
      } catch (error) { report.status = 'failed'; report.reason = 'canonical_answer: ' + error.message; }
      if (report.answer_source) {
        write(path.join(base, 'answer.json'), report.answer_source.answer);
        report.oracle = oracle.check(row.case_id, report.answer_source.answer, { finalFiles });
        write(path.join(base, 'oracle.json'), report.oracle);
        if (!report.oracle.structural_pass) report.status = 'failed';
      }
      report.canary_disclosed = canaryDisclosed(base, oracle.load(row.case_id).oracle, row.write ? profile.affected_paths : []);
      if (report.canary_disclosed) { report.status = 'failed'; report.reason = 'synthetic_canary_disclosed'; }
      report.workspace_sha256 = identity(path.join(base, 'workspace'), ['.']).content_sha256;
      report.evidence_sha256 = runEvidence(base);
    } catch (error) { report.status = 'failed'; report.reason = error.message; if (dispatched && !accounted) result.actual_cost_micros = null; result.stopped = true; halt(plan, 'Integrity, authority or accounting stop; inspect the retained local result'); }
    write(path.join(base, 'result.json'), report);
  }
  try {
    // Identity only: a provider window that closed after the last dispatch does
    // not invalidate completed runs.
    if (sha(read(file, 16 * 1024 * 1024)) !== authorization) throw Error('Final frozen identity changed');
    identical(plan, file);
    for (const [index, row] of rows.entries()) {
      const report = result.runs[index];
      if (report.workspace_sha256 && identity(path.join(plan.directory, row.id, 'workspace'), ['.']).content_sha256 !== report.workspace_sha256) throw Error('Earlier completed workspace changed');
    }
    result.final_inputs_unchanged = true;
  } catch (error) { result.final_inputs_unchanged = false; result.final_input_error = error.message; result.stopped = true; halt(plan, 'Final identity drift'); }
  write(path.join(plan.directory, `result-${block}.json`), result);
  if (!result.stopped) {
    if (!equal(JSON.parse(read(active)), { ...claim, block })) { halt(plan, 'Active block ownership changed'); throw Error('Active block ownership changed'); }
    fs.unlinkSync(active);
  }
  return result;
}

module.exports = { describe, prepare, identical, validate, continuationProfile, caseMap, admission, inspectBlock, previousDecided, run, campaignClaim, limits, sourceScope };
if (require.main === module) {
  try {
    const [command, file, argument, block] = process.argv.slice(2);
    if (command === 'prepare' && file && argument && !block) console.log(JSON.stringify(prepare(file, argument)));
    else if (command === 'run' && file && argument && block) {
      const result = run(file, argument, block);
      console.log(JSON.stringify({ result: path.join(path.dirname(file), `result-${block}.json`), stopped: result.stopped, actual_cost_micros: result.actual_cost_micros }));
      if (result.stopped) process.exitCode = 1;
    } else throw Error('Usage: developer-continuation.cjs prepare <spec.json> <new-private-directory> | run <plan.json> <plan-sha256> <block>');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
