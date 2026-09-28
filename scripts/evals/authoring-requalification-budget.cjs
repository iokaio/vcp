// SPDX-License-Identifier: Apache-2.0
'use strict';
// Authenticate closed liabilities without renewing either halted SKL envelope.
const fs = require('node:fs'), path = require('node:path');
const { isDeepStrictEqual: equal } = require('node:util');
const previousBudget = require('./authoring-qualification-budget.cjs');
const prep = require('./authoring-prepare.cjs'), prior = require('./p6-live-runner.cjs');
const { read, plain } = prior.boundaries;
const must = (ok, message) => { if (!ok) throw Error(message); };
const exact = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...keys].sort());
const grant = previousBudget.grant, document = previousBudget.document;
const limits = Object.freeze({ ...previousBudget.limits, predecessor_micros: 697384, predecessor_requests: 363 });
const PIN = Object.freeze({
  envelope: '5b105eee4da0015d4caa3bbc65531b7b611e064b9999aa87821754e36050fe3a',
  plan: 'a518acbb2f96f5b3a791764df0069a66e248beae10f62fb0afd124b35a2d5f0b',
  result: 'c7c07e40cc8f0926de9e6fc4cd32f5ca702c796754c01ec33a8f551cf621e2c8',
  halt: 'ee78758328ab4f1203c31099df2d81fb118e3a19574e9e4d69191689596e4da0',
  source: 'cdd456f5af0b1ff8bcdd505a0226f6f5ec5c047df00f7faada458661142ca82e',
  finalized_source: Object.freeze({
    'scripts/evals/authoring-skl-continuation.cjs': '3240bb996cd0a1ecd7b0b14dfc679174fd3ed38cd3dc586970d1a9260a562004',
    'scripts/evals/authoring-skl-review.cjs': '36d9760ebbfb268a7be1362d63f9cfb20dbddcfb38e1902bcc7ed8c95d4b2496'
  })
});

function compatibleSource(repository, expected) {
  const actual = prep.identity(repository, expected.scope);
  if (equal(actual, expected)) return actual;
  must(equal(actual.directories, expected.directories) && equal(actual.scope, expected.scope) && actual.files.length === expected.files.length,
    'Frozen successor source identity inventory differs');
  const before = new Map(expected.files.map(file => [file.path, file.sha256]));
  const changed = actual.files.filter(file => before.get(file.path) !== file.sha256);
  must(changed.length === 2 && changed.every(file => PIN.finalized_source[file.path] === file.sha256) &&
    changed.every(file => before.get(file.path) === ({
      'scripts/evals/authoring-skl-continuation.cjs': 'ac8306b43c891bfa195fb74033d5303361c661495092c0572c288e982aedf489',
      'scripts/evals/authoring-skl-review.cjs': 'c6db3f6d843ce1a01aae9ab0fd54e262e21e62fe200aff834a502ed8d3535227'
    })[file.path]), 'Frozen successor source identity differs beyond the finalized merged validators');
  return actual;
}
function claimFile() {
  return path.join(path.dirname(previousBudget.claimFile()), 'vcp-' + grant + '-authoring-requalification-v1.json');
}
function summarize(budget, rows) {
  must(budget?.grant === grant && budget.cap_micros === limits.grant_micros && budget.settled_micros === 644378 && budget.settled_requests === 342 &&
    budget.active_micros === 0 && budget.unresolved_micros === 0 && budget.historical_micros_not_transferred === 401397 &&
    Array.isArray(budget.task_ids) && budget.task_ids.every(id => typeof id === 'string' && id.length) && new Set(budget.task_ids).size === budget.task_ids.length,
  'Authenticated predecessor grant closure differs');
  must(Array.isArray(rows) && rows.length === 2, 'Exactly two consumed successor rows required');
  const tasks = new Set(budget.task_ids), ids = new Set(); let cost = 0, requests = 0;
  for (const row of rows) {
    must(typeof row.id === 'string' && row.id.length && !ids.has(row.id) && typeof row.task_id === 'string' && row.task_id.length && !tasks.has(row.task_id) &&
      ['completed', 'failed'].includes(row.status) && Number.isSafeInteger(row.actual_cost_micros) && row.actual_cost_micros >= 0 && row.actual_cost_micros <= limits.slot_micros &&
      Number.isSafeInteger(row.observed_attempts) && row.observed_attempts >= 0 && row.observed_attempts <= limits.slot_requests && row.active_micros === 0 && row.unresolved_micros === 0,
    'Unknown, excessive or duplicate successor liability');
    tasks.add(row.task_id); ids.add(row.id); cost += row.actual_cost_micros; requests += row.observed_attempts;
  }
  must(cost === 53006 && requests === 21 && budget.settled_micros + cost === limits.predecessor_micros &&
    limits.predecessor_micros + limits.cap_micros <= limits.grant_micros, 'Exact settled total or full campaign reservation differs');
  return { grant, cap_micros: limits.grant_micros, settled_micros: limits.predecessor_micros, settled_requests: limits.predecessor_requests,
    historical_micros_not_transferred: budget.historical_micros_not_transferred, task_ids: [...tasks], current: budget.current, history: budget.history,
    successor_rows: rows, active_micros: 0, unresolved_micros: 0, reserved_micros: limits.cap_micros, reserved_requests: limits.requests };
}
function inspect(input) {
  must(exact(input, ['cs2', 'successor']) && exact(input.successor, ['repository', 'envelope', 'plan', 'result', 'halt']) &&
    typeof input.successor.repository === 'string' && path.isAbsolute(input.successor.repository), 'Exact frozen successor references required');
  const reference = input.successor;
  for (const name of ['envelope', 'plan', 'result', 'halt']) must(reference[name]?.sha256 === PIN[name], 'Unexpected historical identity');
  const envelope = document(reference.envelope), plan = document(reference.plan), result = document(reference.result), halt = document(reference.halt);
  const repository = plain(reference.repository);
  must(envelope.schema === 'cs1-skl-continuation-envelope/1' && envelope.source.content_sha256 === PIN.source &&
    ['authoring-skl-continuation', 'authoring-skl-reconciliation'].every(name => envelope.source.scope.includes('scripts/evals/' + name + '.cjs')) &&
    envelope.source.content_sha256 === PIN.source, 'Frozen successor source identity differs');
  const authenticatedSource = compatibleSource(repository, envelope.source);
  must(reference.envelope.file === path.join(envelope.directory, 'envelope.json') && reference.plan.file === path.join(envelope.directory, 'phases/skill-authoring--normal/plan.json') &&
    reference.result.file === path.join(plan.directory, 'result.json') && reference.halt.file === path.join(envelope.directory, 'halt.json') &&
    plan.directory === path.dirname(reference.plan.file) && plan.envelope_sha256 === PIN.envelope && plan.candidate === 'skill-authoring' && plan.phase === 'normal' && plan.runs.length === 5,
  'Historical owned phase paths or membership differ');
  // Require only authenticated historical modules; no describe/envelopeFor/run,
  // which could halt or try to validate a now-expired execution qualification.
  const runner = require(path.join(repository, 'scripts/evals/authoring-skl-continuation.cjs'));
  const reconcile = require(path.join(repository, 'scripts/evals/authoring-skl-reconciliation.cjs'));
  const previous = reconcile.inspect(envelope.predecessor), budget = runner.sourceBudget(previous);
  must(equal(envelope.budget, budget) && equal(input.cs2, budget.current.reference), 'Predecessor accounting or CS2 reference changed');
  const claim = { envelope_sha256: PIN.envelope, phase_sha256: PIN.plan };
  must(halt.schema === 'cs1-skl-continuation-halt/1' && typeof halt.reason === 'string' && halt.reason.length > 0 &&
    halt.action === 'Read-only reconciliation only. One-shot claim remains consumed.' && result.stopped === false && result.candidate_stopped === true && result.final_inputs_unchanged === true &&
    !fs.existsSync(path.join(envelope.directory, 'active-phase.json')), 'Preserved permanent review halt and released phase ownership required');
  must(envelope.common_claim === runner.claimFile(previous.modules.runner.successorClaim()) && equal(JSON.parse(read(envelope.common_claim)), {
    schema: 'cs1-skl-one-shot-claim/1', grant, predecessor_envelope_sha256: reconcile.PIN.envelope, directory: envelope.directory, envelope_sha256: PIN.envelope
  }), 'Historical durable one-shot claim differs');
  runner.resultEvidence(envelope, plan, result, PIN.plan, true);
  must(result.runs.slice(0, 2).every(row => ['completed', 'failed'].includes(row.status)) && result.runs.slice(2).every(row => row.status === 'not_run') &&
    equal(fs.readdirSync(plain(path.join(envelope.directory, 'phases'))).sort(), ['skill-authoring--normal']) &&
    equal(fs.readdirSync(plain(path.join(envelope.directory, 'claims'))).sort(), result.runs.slice(0, 2).map(row => row.id + '.json').sort()), 'Omitted phase/claim or changed consumed prefix');
  const rows = result.runs.slice(0, 2).map((report, index) => {
    const money = prior.accounting(JSON.parse(read(path.join(plan.directory, report.id, 'costs.json'))), plan.runs[index].cap_micros);
    must(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts, 'Canonical settled successor accounting differs');
    return { id: report.id, task_id: report.scope?.task, status: report.status, actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length, active_micros: 0, unresolved_micros: 0 };
  });
  must(result.actual_cost_micros === 53006 && result.observed_attempts === 21, 'Historical aggregate result differs');
  const closure = summarize(budget, rows);
  // Re-read pinned inputs after the read-only traversal; original failures remain.
  for (const name of ['envelope', 'plan', 'result', 'halt']) document(reference[name]);
  must(equal(prep.identity(repository, envelope.source.scope), authenticatedSource), 'Frozen source changed during inspection');
  return { ...closure, reference: input, grant_record: previous.envelope.budget.grant_record, qualification: 'failed_or_incomplete_preserved' };
}
function admit(budget, completedRows) {
  must(budget?.grant === grant && budget.cap_micros === limits.grant_micros && budget.settled_micros === limits.predecessor_micros &&
    budget.settled_requests === limits.predecessor_requests && budget.active_micros === 0 && budget.unresolved_micros === 0 &&
    Array.isArray(budget.task_ids) && budget.task_ids.every(id => typeof id === 'string' && id.length) && new Set(budget.task_ids).size === budget.task_ids.length,
  'Authenticated complete grant accounting required');
  must(Array.isArray(completedRows) && completedRows.length < limits.slots, 'No unused requalification slot remains');
  const tasks = new Set(budget.task_ids), ids = new Set(); let cost = 0, requests = 0;
  for (const row of completedRows) {
    must(row && typeof row.id === 'string' && row.id.length && !ids.has(row.id) && typeof row.task_id === 'string' && row.task_id.length && !tasks.has(row.task_id) &&
      ['completed', 'failed'].includes(row.status), 'Duplicate or invalid new slot/task');
    must(Number.isSafeInteger(row.actual_cost_micros) && row.actual_cost_micros >= 0 && row.actual_cost_micros <= limits.slot_micros &&
      Number.isSafeInteger(row.observed_attempts) && row.observed_attempts >= 0 && row.observed_attempts <= limits.slot_requests && row.active_micros === 0 && row.unresolved_micros === 0,
    'Unknown or excessive fresh liability');
    tasks.add(row.task_id); ids.add(row.id); cost += row.actual_cost_micros; requests += row.observed_attempts;
  }
  const aggregate = budget.settled_micros + cost;
  must(aggregate + limits.slot_micros <= limits.grant_micros && cost + limits.slot_micros <= limits.cap_micros && requests + limits.slot_requests <= limits.requests,
    'Cumulative authorization cannot reserve the next full slot');
  return { grant, historical_micros_not_transferred: budget.historical_micros_not_transferred, predecessor_settled_micros: budget.settled_micros,
    current_authorization_settled_micros: aggregate, authoring_settled_micros: cost, authoring_requests: requests,
    reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests, remaining_micros: limits.grant_micros - aggregate };
}
module.exports = { grant, limits, PIN, document, claimFile, inspect, summarize, admit };
