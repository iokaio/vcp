// SPDX-License-Identifier: Apache-2.0
'use strict';

// Pure admission checks for the single CS-1 successor campaign. The caller owns
// private-file reads, canonical ledger validation, durable claims and exclusion
// of concurrent campaigns. A summary here is never a replacement for that audit.
const crypto = require('node:crypto');
const { isDeepStrictEqual } = require('node:util');
const limits = Object.freeze({ cap_micros: 162000000, requests: 864, slot_micros: 3000000, slot_requests: 16, slots: 54, prior_micros: 123408, prior_requests: 47 });
const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const integer = value => Number.isSafeInteger(value) && value >= 0;
const name = value => typeof value === 'string' && value.length > 0;
const zero = value => value === 0 || value === '0';
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function requireThat(condition, message) { if (!condition) throw Error(message); }
function document(bytes, expected, label) {
  requireThat(Buffer.isBuffer(bytes) && digest(expected) && sha(bytes) === expected, `Prior ${label} hash differs`);
  let result;
  try { result = JSON.parse(bytes.toString('utf8')); } catch { throw Error(`Prior ${label} is not JSON`); }
  requireThat(result && typeof result === 'object' && !Array.isArray(result), `Prior ${label} must be an object`);
  return result;
}
function closed(value, label) {
  requireThat(zero(value.active_micros) && zero(value.unresolved_micros) && (!Object.hasOwn(value, 'overrun') || value.overrun === false), `${label} has unknown, active, unresolved or overrun liability`);
}
function verifyPriorCampaign(reference, bytes) {
  requireThat(reference && bytes, 'Prior campaign reference and bytes are required');
  const audit = document(bytes.accounting, reference.accounting_sha256, 'accounting');
  const result = document(bytes.result, reference.result_sha256, 'result');
  const halt = document(bytes.halt, reference.halt_sha256, 'halt');
  requireThat(digest(reference.envelope_sha256) && digest(reference.phase_sha256), 'Prior campaign identities are required');
  requireThat(reference.actual_cost_micros === limits.prior_micros && reference.observed_attempts === limits.prior_requests, 'Prior campaign totals differ from authorized history');
  requireThat(audit.schema === 'cs1-followup-owner-accounting-audit/1' && result.schema === 'cs1-followup-result/1' && halt.schema === 'cs1-followup-halt/1', 'Prior campaign schema differs');
  requireThat(audit.envelope_sha256 === reference.envelope_sha256 && result.envelope_sha256 === reference.envelope_sha256 && audit.phase_sha256 === reference.phase_sha256 && result.phase_sha256 === reference.phase_sha256 && audit.phase_result_sha256 === reference.result_sha256 && audit.halt_sha256 === reference.halt_sha256, 'Prior campaign receipt identities differ');
  requireThat(isDeepStrictEqual(audit.halt, halt) && name(halt.reason) && halt.action === 'Read-only reconciliation. This envelope cannot resume or replay.', 'Prior campaign lacks the exact retained halt');
  requireThat(audit.source_and_input_identities_unchanged === true && result.final_inputs_unchanged === true && result.stopped === false, 'Prior campaign accounting or source integrity is incomplete');
  closed(audit, 'Prior campaign');
  requireThat(Array.isArray(audit.rows) && audit.rows.length === 6 && audit.task_runs === 6 && Array.isArray(result.runs) && result.runs.length === 6, 'Prior campaign must contain its six settled task results');
  const ids = new Set(), tasks = new Set();
  let cost = 0, requests = 0, settled = 0, released = 0;
  for (let index = 0; index < audit.rows.length; index++) {
    const row = audit.rows[index], actual = result.runs[index];
    requireThat(row && actual && name(row.run_id) && name(row.task_id) && !ids.has(row.run_id) && !tasks.has(row.task_id), 'Prior campaign row identity repeats or is missing');
    ids.add(row.run_id); tasks.add(row.task_id);
    requireThat(['completed', 'failed'].includes(row.status) && row.status === actual.status && row.run_id === actual.id && row.case_id === actual.case_id && row.arm === actual.arm && row.task_id === actual.scope?.task, 'Prior audit row differs from retained result');
    requireThat(integer(row.actual_cost_micros) && row.actual_cost_micros <= limits.slot_micros && row.actual_cost_micros === actual.actual_cost_micros && integer(row.requests) && row.requests <= limits.slot_requests && row.requests === actual.observed_attempts, 'Prior row accounting differs or exceeds slot limits');
    requireThat(integer(row.settled_requests) && integer(row.released_requests) && row.requests === row.settled_requests + row.released_requests, 'Prior request liabilities are not closed');
    closed(row, 'Prior row');
    requireThat(row.preservation_verified === true && digest(row.result_sha256) && digest(row.costs_sha256) && digest(row.workspace_sha256) && row.workspace_sha256 === actual.workspace_sha256, 'Prior row audit evidence is incomplete');
    cost += row.actual_cost_micros; requests += row.requests; settled += row.settled_requests; released += row.released_requests;
  }
  requireThat(cost === reference.actual_cost_micros && cost === audit.cost_micros && cost === result.actual_cost_micros && requests === reference.observed_attempts && requests === audit.requests && requests === result.observed_attempts && settled === audit.settled_requests && released === audit.released_requests, 'Prior campaign aggregate accounting differs');
  return Object.freeze({ actual_cost_micros: cost, observed_attempts: requests, prior_task_ids: Object.freeze([...tasks]) });
}

// Supply every canonically reconciled attempted row from ALL new phases plus
// the current partial phase. Exclude not_run rows; never exclude failed rows.
// This function cannot detect omitted rows: the caller must reconcile permanent
// claims and result evidence before supplying the list and before each dispatch.
function admitSlot(prior, completedRows) {
  requireThat(prior?.actual_cost_micros === limits.prior_micros && prior.observed_attempts === limits.prior_requests && Array.isArray(prior.prior_task_ids) && prior.prior_task_ids.length === 6 && prior.prior_task_ids.every(name) && new Set(prior.prior_task_ids).size === 6, 'Verified prior campaign accounting is required');
  requireThat(Array.isArray(completedRows) && completedRows.length < limits.slots, 'No unused fixed slot remains');
  const ids = new Set(), tasks = new Set(prior.prior_task_ids);
  let cost = prior.actual_cost_micros, requests = prior.observed_attempts;
  for (const row of completedRows) {
    requireThat(row && name(row.id) && name(row.task_id) && !ids.has(row.id) && !tasks.has(row.task_id), 'New accounting repeats a slot or canonical task');
    ids.add(row.id); tasks.add(row.task_id);
    requireThat(['completed', 'failed'].includes(row.status) && integer(row.actual_cost_micros) && row.actual_cost_micros <= limits.slot_micros && integer(row.observed_attempts) && row.observed_attempts <= limits.slot_requests, 'New row accounting is unknown or exceeds slot limits');
    closed(row, 'New row');
    cost += row.actual_cost_micros; requests += row.observed_attempts;
  }
  requireThat(Number.isSafeInteger(cost) && cost + limits.slot_micros <= limits.cap_micros, 'Cumulative USD 162 cap cannot reserve the next USD 3 slot');
  requireThat(Number.isSafeInteger(requests) && requests + limits.slot_requests <= limits.requests, 'Cumulative 864 request cap cannot reserve the next 16 requests');
  return Object.freeze({ actual_cost_micros: cost, observed_attempts: requests, remaining_micros: limits.cap_micros - cost, remaining_requests: limits.requests - requests, reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests, admitted_cost_micros: cost + limits.slot_micros, admitted_requests: requests + limits.slot_requests });
}

module.exports = { limits, verifyPriorCampaign, admitSlot };
