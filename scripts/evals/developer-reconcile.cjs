// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read-only reconciliation of the owner-rebooted CS-2 campaign. A halt is
// authenticated history, never permission to resume its runner or review it.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const prep = require('./developer-prepare.cjs');
const runner = require('./developer-runner.cjs');
const cs1 = require('./authoring-runner.cjs');
const { identity } = require('./authoring-prepare.cjs');
const { read, plain, safeChild, noParentInstructions, privateDirectory } = prior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const reason = 'Owner requested a reboot checkpoint on 2026-09-26. Finish retention for the already-settled current run, then refuse every further dispatch.';
const action = 'Read-only reconciliation. This campaign cannot resume or replay.';
function requireThat(value, message) { if (!value) throw Error(message); }
function json(file) { return JSON.parse(read(file)); }

// Kept separate for synthetic tests; callers cannot substitute this audit for
// inspect(), which also rederives the original preparation and identities.
function audit(plan, authorization, result, resultBytes) {
  const directory = plain(plan.directory), claim = { plan_sha256: authorization, directory };
  requireThat(plan.runs.length === 54 && new Set(plan.runs.map(row => row.id)).size === 54, 'Original 54-run cohort required');
  const rows = plan.runs.filter(row => row.block === 'llm-integration');
  requireThat(rows.length === 18 && equal(plan.runs.slice(0, 18), rows), 'Original first block required');
  requireThat(result.schema === 'cs-2-developer-block-result/1' && result.plan_sha256 === authorization && result.block === 'llm-integration' && result.stopped === true && result.final_inputs_unchanged === true && equal(result.runs.map(row => row.id), rows.map(row => row.id)), 'Stopped predecessor result coverage or identity differs');
  const claims = fs.readdirSync(plain(path.join(directory, 'claims'))).sort();
  requireThat(equal(claims, ['block-llm-integration.json', ...rows.slice(0, 8).map(row => row.id + '.json')].sort()), 'Exactly eight predecessor run claims required');
  const blockClaim = { ...claim, block: 'llm-integration' };
  requireThat(equal(json(path.join(directory, 'claims/block-llm-integration.json')), blockClaim) && equal(json(path.join(directory, 'active-block.json')), blockClaim), 'Predecessor block ownership changed');
  const retained = [];
  let cost = 0, attempts = 0;
  for (const [index, row] of plan.runs.entries()) {
    const base = safeChild(directory, row.id), reportFile = path.join(base, 'result.json');
    requireThat(sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256 && equal(json(path.join(base, 'profile.json')), row.profile), 'Predecessor prompt or profile changed');
    if (index < 8) {
      const bytes = read(reportFile), report = JSON.parse(bytes);
      requireThat(equal(json(path.join(directory, 'claims', row.id + '.json')), { ...claim, run: row.id }), 'Predecessor run claim changed');
      requireThat(equal(report, result.runs[index]) && report.id === row.id && report.case_id === row.case_id && report.arm === row.arm, 'Retained run report changed');
      requireThat(report.evidence_sha256 === runner.runEvidence(base) && report.workspace_sha256 === identity(path.join(base, 'workspace'), ['.']).content_sha256, 'Retained evidence or workspace changed');
      requireThat(report.preserved === true && report.canary_disclosed === false && report.skill_evidence?.checked_attempts > 0, 'Predecessor lacks preservation or skill evidence');
      cs1.finalWorkspace(base, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths);
      requireThat(index < 7 ? report.status === 'completed' && report.native_check?.status === 'passed' : row.id === 'LLM-boundary-partial-v3--none' && report.status === 'failed' && report.conditions?.completed === false && report.native_check?.status === 'not_run', 'Historical completion or failed eighth outcome changed');
      const money = prior.accounting(json(path.join(base, 'costs.json')), row.cap_micros);
      requireThat(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts && money.attempts.length <= row.call_ceiling, 'Predecessor canonical accounting differs');
      requireThat(equal(json(path.join(base, 'attempted.json')).plan_sha256, authorization), 'Predecessor dispatch identity changed');
      cost += money.actual_cost_micros; attempts += money.attempts.length;
      retained.push({ row, report, result_sha256: sha(bytes) });
    } else {
      requireThat(!fs.existsSync(path.join(base, 'attempted.json')) && fs.readdirSync(plain(path.join(base, 'data'))).length === 0 && cs1.preserved(base, row), 'Unclaimed predecessor run has execution evidence or changed inputs');
      const expectedFiles = ['workspace', 'data', 'prompt.txt', 'profile.json', ...(index === 8 ? ['result.json'] : [])].sort();
      requireThat(equal(fs.readdirSync(plain(base)).sort(), expectedFiles), 'Unclaimed predecessor run has unexpected retained evidence');
      if (index === 8) {
        const rejected = result.runs[index];
        requireThat(equal(json(reportFile), rejected) && rejected.status === 'failed' && rejected.actual_cost_micros === null && rejected.observed_attempts === undefined && rejected.reason === 'Campaign halted: reconciliation only; no further dispatch', 'Rejected pre-dispatch ninth run changed');
      } else {
        requireThat(!fs.existsSync(reportFile) && (index >= 18 || result.runs[index].status === 'not_run'), 'Unclaimed predecessor result differs');
      }
    }
  }
  requireThat(Number.isSafeInteger(cost) && cost === result.actual_cost_micros && attempts === result.observed_attempts, 'Predecessor block totals differ');
  for (const block of ['mcp-development', 'frontend-design']) requireThat(!fs.existsSync(path.join(directory, `result-${block}.json`)), 'Later predecessor block unexpectedly exists');
  return { plan, result, result_sha256: sha(resultBytes), retained, pending: plan.runs.slice(8), actual_cost_micros: cost, observed_attempts: attempts };
}

function inspect(file, authorization) {
  file = plain(path.resolve(file));
  const bytes = read(file), plan = JSON.parse(bytes);
  requireThat(/^[a-f0-9]{64}$/.test(authorization) && sha(bytes) === authorization && file === path.join(plan.directory, 'plan.json'), 'Exact predecessor plan identity required');
  noParentInstructions(plan.directory); privateDirectory(plan.directory);
  const haltBytes = read(path.join(plan.directory, 'halt.json')), halt = JSON.parse(haltBytes);
  requireThat(equal(halt, { schema: 'cs-2-developer-halt/1', reason, action, requested_at_utc: '2026-09-26T05:08:48.9446126Z', plan_sha256: authorization }), 'Only the unchanged owner reboot halt may precede continuation');
  requireThat(equal(json(runner.campaignClaim()), { plan_sha256: authorization, directory: plan.directory }), 'Original repository campaign claim changed');
  runner.identical(plan, file);
  const staged = prep.staged(plan.directory, plan.runtime, prep.order(prep.tasks().items));
  requireThat(sha(read(plan.runtime.checker, 256 * 1024 * 1024)) === plan.runtime.checker_sha256 && read(plan.runtime.cases_file).equals(staged.cases), 'Predecessor staged checker changed');
  const resultBytes = read(path.join(plan.directory, 'result-llm-integration.json'));
  const reconciled = audit(plan, authorization, JSON.parse(resultBytes), resultBytes);
  // Re-read immutable anchors after the audit, before any successor can use it.
  requireThat(read(file).equals(bytes) && read(path.join(plan.directory, 'halt.json')).equals(haltBytes) && read(path.join(plan.directory, 'result-llm-integration.json')).equals(resultBytes), 'Predecessor changed during reconciliation');
  return { ...reconciled, halt_sha256: sha(haltBytes) };
}
module.exports = { inspect, audit };
if (require.main === module) {
  try {
    const [file, hash, ...extra] = process.argv.slice(2);
    if (!file || !hash || extra.length) throw Error('Usage: developer-reconcile.cjs <original-plan.json> <original-plan-sha256>');
    const result = inspect(file, hash);
    console.log(JSON.stringify({ schema: 'cs2-resumption-reconciliation/1', plan_sha256: hash, result_sha256: result.result_sha256, halt_sha256: result.halt_sha256, retained: result.retained.map(({ report }) => ({ id: report.id, status: report.status })), pending_runs: result.pending.length, actual_cost_micros: result.actual_cost_micros, observed_attempts: result.observed_attempts, active_liability_micros: 0, unresolved_liability_micros: 0 }));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
