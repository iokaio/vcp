// SPDX-License-Identifier: Apache-2.0
'use strict';
// One fresh authoring envelope under the owner's current USD 100 resumption
// authorization. Historical accounting is evidence, never a renewed allowance.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const { execFileSync } = require('node:child_process');
const prior = require('./p6-live-runner.cjs');
const { identity } = require('./authoring-prepare.cjs');
const { runEvidence } = require('./developer-runner.cjs');
const { read, plain } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const grant = 'owner-2026-09-26-openrouter-resumption-usd100';
const limits = Object.freeze({ grant_micros: 100000000, cap_micros: 94500000, requests: 864, slot_micros: 1750000, slot_requests: 16, slots: 54, output_tokens: '2048' });
function requireThat(condition, message) { if (!condition) throw Error(message); }
function exact(value, keys) { return value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...keys].sort()); }
function document(ref) {
  requireThat(exact(ref, ['file', 'sha256']) && typeof ref.file === 'string' && path.isAbsolute(ref.file) && /^[a-f0-9]{64}$/.test(ref.sha256), 'Exact absolute accounting reference required');
  const bytes = read(plain(ref.file), 16 * 1024 * 1024);
  requireThat(sha(bytes) === ref.sha256, 'Accounting reference bytes changed');
  return JSON.parse(bytes);
}
function claimFile() {
  const common = execFileSync('git', ['-c', `safe.directory=${repository.replaceAll('\\', '/')}`, '-C', repository, 'rev-parse', '--path-format=absolute', '--git-common-dir'], { encoding: 'utf8', timeout: 30000 }).trim();
  requireThat(path.isAbsolute(common), 'Absolute shared Git administration directory required');
  return path.join(plain(common), 'vcp-' + grant + '-authoring.json');
}
function phaseHistory(ref, expected) {
  requireThat(exact(ref, ['envelope', 'plan', 'result', 'halt']), 'Historical phase references differ');
  const envelope = document(ref.envelope), plan = document(ref.plan), result = document(ref.result), halt = document(ref.halt);
  requireThat(envelope.schema === 'cs-1-followup-envelope/1' && plan.schema === 'cs1-followup-phase/1' && result.schema === 'cs1-followup-result/1', 'Historical schema differs');
  requireThat(ref.envelope.file === path.join(envelope.directory, 'envelope.json') && ref.plan.file === path.join(plan.directory, 'plan.json') && ref.result.file === path.join(plan.directory, 'result.json') && ref.halt.file === path.join(envelope.directory, 'halt.json'), 'Historical owned paths differ');
  requireThat(plan.envelope_sha256 === ref.envelope.sha256 && result.envelope_sha256 === ref.envelope.sha256 && result.phase_sha256 === ref.plan.sha256 && result.final_inputs_unchanged === true && equal(result.runs.map(row => row.id), plan.runs.map(row => row.id)), 'Historical result binding differs');
  requireThat(halt.schema === 'cs1-followup-halt/1' && typeof halt.reason === 'string' && halt.reason.length > 0 && halt.action === 'Read-only reconciliation. This envelope cannot resume or replay.', 'Historical permanent halt required');
  requireThat(plan.candidate === 'document-authoring' && plan.phase === expected.phase && plan.runs.length === expected.rows, 'Historical fixed phase differs');
  let cost = 0, requests = 0;
  const rows = [], claims = [];
  for (const [index, row] of plan.runs.entries()) {
    const report = result.runs[index], base = path.join(plan.directory, row.id), file = path.join(envelope.directory, 'claims', row.id + '.json');
    if (!fs.existsSync(file)) {
      requireThat(index >= expected.executed && !fs.existsSync(path.join(base, 'attempted.json')) && !report.scope?.task && report.actual_cost_micros === null, 'Unclaimed historical execution or liability');
      continue;
    }
    requireThat(index < expected.executed && equal(JSON.parse(read(file)), { envelope_sha256: ref.envelope.sha256, phase_sha256: ref.plan.sha256, slot: row.id }), 'Historical claim identity differs');
    requireThat(['completed', 'failed'].includes(report.status) && equal(JSON.parse(read(path.join(base, 'result.json'))), report) && report.evidence_sha256 === runEvidence(base) && report.workspace_sha256 === identity(path.join(base, 'workspace'), ['.']).content_sha256, 'Historical result or evidence changed');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    requireThat(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts && money.attempts.length <= row.call_ceiling, 'Historical canonical accounting differs');
    cost += money.actual_cost_micros; requests += money.attempts.length;
    claims.push(row.id + '.json'); rows.push({ id: row.id, task_id: report.scope?.task, status: report.status, actual_cost_micros: report.actual_cost_micros, observed_attempts: report.observed_attempts });
  }
  requireThat(rows.length === expected.executed && cost === expected.cost && requests === expected.requests && result.actual_cost_micros === cost && result.observed_attempts === requests, 'Historical immutable totals differ');
  requireThat(equal(fs.readdirSync(plain(path.join(envelope.directory, 'claims'))).sort(), claims.sort()), 'Omitted historical execution claim');
  return { reference: ref, actual_cost_micros: cost, observed_attempts: requests, rows, stopped: result.stopped, halt };
}
function history(references) {
  requireThat(Array.isArray(references) && references.length === 3, 'Complete three-phase historical CS-1 chain required');
  const expected = [{ phase: 'normal', rows: 6, executed: 6, cost: 123408, requests: 47 }, { phase: 'normal', rows: 6, executed: 6, cost: 114964, requests: 45 }, { phase: 'inherited', rows: 18, executed: 13, cost: 163025, requests: 89 }];
  const identities = ['6fe72a3ce023918043dfdb2525c177ad842e7c21c9663ee3037db0f967005ad6', '1436fac6f1fd32309db1af9937c5ce89c2f5917e4609a5ae5595fc2fa615e3c2', 'fa0e625f9b36fa6a39a712052540d08f435c689eb9eded79bfc8e25f8c35279d'];
  requireThat(references.every((ref, index) => ref.envelope?.sha256 === identities[index]), 'Historical campaign identities cannot be replaced by a bootstrap');
  const phases = references.map((ref, index) => phaseHistory(ref, expected[index]));
  const taskIds = phases.flatMap(phase => phase.rows.map(row => row.task_id));
  requireThat(taskIds.every(id => typeof id === 'string' && id.length) && new Set(taskIds).size === taskIds.length && phases[2].stopped === true, 'Historical task identities or authority stop differ');
  return { phases, actual_cost_micros: phases.reduce((n, phase) => n + phase.actual_cost_micros, 0), observed_attempts: phases.reduce((n, phase) => n + phase.observed_attempts, 0), task_ids: taskIds, qualification: 'failed_or_incomplete_preserved' };
}
function terminalCs2(reference) {
  requireThat(exact(reference, ['repository', 'plan']) && typeof reference.repository === 'string' && path.isAbsolute(reference.repository), 'Frozen CS-2 repository and plan reference required');
  const plan = document(reference.plan), frozenRepository = plain(reference.repository);
  requireThat(plan.schema === 'cs-2-developer-continuation/1' && refPath(reference.plan.file) === path.join(plan.directory, 'plan.json') && equal(identity(frozenRepository, plan.source.scope), plan.source), 'Frozen CS-2 source or plan identity changed');
  // Only authenticate and inspect completed evidence with its pinned source.
  // Never call a historical runner, bootstrap, renewal or campaign preparer.
  const runner = require(path.join(frozenRepository, 'scripts/evals/developer-continuation.cjs'));
  runner.identical(plan, reference.plan.file);
  requireThat(!fs.existsSync(path.join(plan.directory, 'active-block.json')) && !fs.existsSync(path.join(plan.directory, 'halt.json')), 'Only the complete decided CS-2 continuation is supported; halted/partial closure needs separate reconciliation');
  requireThat(plan.runs.length === 46 && plan.limits.cap_micros === 92000000, 'Current authorization CS-2 membership differs');
  const rows = [], blockRefs = [];
  for (const block of ['llm-integration', 'mcp-development', 'frontend-design']) {
    const inspected = runner.inspectBlock(reference.plan.file, reference.plan.sha256, block);
    const decisionFile = path.join(plan.directory, `decision-${block}.json`), decisionBytes = read(decisionFile), decision = JSON.parse(decisionBytes);
    requireThat(decision.schema === 'cs-2-developer-decision/1' && decision.plan_sha256 === reference.plan.sha256 && decision.block === block && decision.result_sha256 === inspected.result_sha256, 'CS-2 terminal decision does not bind composite result');
    const fresh = JSON.parse(read(path.join(plan.directory, `result-${block}.json`)));
    for (const report of fresh.runs) {
      const row = plan.runs.find(row => row.id === report.id), base = path.join(plan.directory, report.id);
      requireThat(row && ['completed', 'failed'].includes(report.status), 'Unexecuted fresh CS-2 slot prevents closure');
      const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
      requireThat(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts, 'CS-2 accounting differs');
      rows.push({ id: report.id, task_id: report.scope?.task, status: report.status, actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length });
    }
    blockRefs.push({ block, result_sha256: inspected.result_sha256, decision_sha256: sha(decisionBytes) });
  }
  requireThat(rows.length === 46 && new Set(rows.map(row => row.id)).size === 46 && rows.every(row => typeof row.task_id === 'string') && new Set(rows.map(row => row.task_id)).size === 46, 'CS-2 closure coverage differs');
  const expectedClaims = [...plan.runs.map(row => row.id + '.json'), ...['llm-integration', 'mcp-development', 'frontend-design'].map(block => `block-${block}.json`)].sort();
  requireThat(equal(fs.readdirSync(path.join(plan.directory, 'claims')).sort(), expectedClaims), 'CS-2 closure has omitted or extra claims');
  return { reference, block_refs: blockRefs, rows, actual_cost_micros: rows.reduce((n, row) => n + row.actual_cost_micros, 0), observed_attempts: rows.reduce((n, row) => n + row.observed_attempts, 0), task_ids: rows.map(row => row.task_id) };
}
function refPath(file) { return plain(path.resolve(file)); }
function inspect(input) {
  requireThat(exact(input, ['grant', 'grant_record', 'cs2', 'historical_cs1', 'additional_paid_calls']) && input.grant === grant, 'Exact existing USD 100 authorization identity required');
  const owner = document(input.grant_record);
  requireThat(owner.schema === 'cs2-continuation-authorization/1' && owner.user_authorization === 'You are authorized to spend up to $100 via openrouter calls.' && owner.new_total_cap_micros === limits.grant_micros
    && owner.endpoint === 'https://openrouter.ai/api/v1/responses' && owner.model === 'openai/gpt-5.6-luna' && owner.provider_endpoint === 'amazon-bedrock/us-east-1'
    && owner.plan_sha256 === '2744de3f29883d01d2e1c661d1b2d823bc85b805a9cc98c3dc2092a22800c716' && owner.original_plan_sha256 === '99ff620b9a3597fe6fe817c0b9bb1e60897e751acc3547561574da08c40dfc8a'
    && owner.plan === input.cs2.plan.file && owner.plan_sha256 === input.cs2.plan.sha256 && owner.campaign_cap_micros === 92000000 && owner.refresh_new_requests === 0, 'Owner grant or current campaign membership differs');
  // This bounded increment starts after the current complete CS-2 envelope.
  // No paid refresh has been proposed or admitted here. Refuse such history
  // rather than silently treating a future probe as free or inventing its schema.
  requireThat(Array.isArray(input.additional_paid_calls) && input.additional_paid_calls.length === 0, 'Additional paid calls require their concrete authenticated accounting path before preparation');
  const old = history(input.historical_cs1), current = terminalCs2(input.cs2);
  requireThat(current.actual_cost_micros + limits.cap_micros <= limits.grant_micros, 'Remaining owner authorization cannot reserve the full fresh envelope');
  return { grant, grant_record: input.grant_record, cap_micros: limits.grant_micros, history: old, current, active_micros: 0, unresolved_micros: 0 };
}
function admit(priorBudget, completedRows) {
  requireThat(priorBudget?.grant === grant && priorBudget.cap_micros === limits.grant_micros && priorBudget.active_micros === 0 && priorBudget.unresolved_micros === 0 && Number.isSafeInteger(priorBudget.current?.actual_cost_micros) && priorBudget.current.actual_cost_micros >= 0 && Array.isArray(priorBudget.current.task_ids) && Array.isArray(priorBudget.history?.task_ids), 'Authenticated current-budget accounting required');
  requireThat(Array.isArray(completedRows) && completedRows.length < limits.slots, 'No unused authoring slot remains');
  const ids = new Set(), tasks = new Set([...priorBudget.current.task_ids, ...priorBudget.history.task_ids]);
  let cost = 0, requests = 0;
  for (const row of completedRows) {
    requireThat(row && typeof row.id === 'string' && typeof row.task_id === 'string' && row.task_id.length && !ids.has(row.id) && !tasks.has(row.task_id) && ['completed', 'failed'].includes(row.status), 'Duplicate or invalid fresh slot/task');
    requireThat(Number.isSafeInteger(row.actual_cost_micros) && row.actual_cost_micros >= 0 && row.actual_cost_micros <= limits.slot_micros && Number.isSafeInteger(row.observed_attempts) && row.observed_attempts >= 0 && row.observed_attempts <= limits.slot_requests && row.active_micros === 0 && row.unresolved_micros === 0, 'Unknown or excessive fresh accounting');
    ids.add(row.id); tasks.add(row.task_id); cost += row.actual_cost_micros; requests += row.observed_attempts;
  }
  const aggregate = priorBudget.current.actual_cost_micros + cost;
  requireThat(Number.isSafeInteger(aggregate) && aggregate + limits.slot_micros <= limits.grant_micros && cost + limits.slot_micros <= limits.cap_micros && requests + limits.slot_requests <= limits.requests, 'Cumulative authorization cannot reserve the next full authoring slot');
  return { grant, historical_micros_not_transferred: priorBudget.history.actual_cost_micros, current_authorization_settled_micros: aggregate, authoring_settled_micros: cost, authoring_requests: requests, reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests, remaining_micros: limits.grant_micros - aggregate };
}
module.exports = { grant, limits, document, claimFile, phaseHistory, history, terminalCs2, inspect, admit };
