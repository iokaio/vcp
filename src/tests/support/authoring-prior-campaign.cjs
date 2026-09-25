// SPDX-License-Identifier: Apache-2.0
'use strict';
const crypto = require('node:crypto');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const encode = value => Buffer.from(JSON.stringify(value));

// Synthetic accounting only. Never invokes a provider or writes a receipt.
function priorCampaignFixture(mutate = () => {}) {
  const envelope_sha256 = crypto.randomBytes(32).toString('hex'), phase_sha256 = crypto.randomBytes(32).toString('hex');
  const halt = { schema: 'cs1-followup-halt/1', reason: 'Synthetic owner halt', action: 'Read-only reconciliation. This envelope cannot resume or replay.' };
  const rows = Array.from({ length: 6 }, (_, index) => ({ run_id: 'old-slot-' + index, case_id: 'case-' + Math.floor(index / 3), arm: ['none', 'nearest', 'candidate'][index % 3], task_id: 'old-task-' + index, status: index === 2 ? 'completed' : 'failed', actual_cost_micros: index === 5 ? 23408 : 20000, requests: index === 5 ? 7 : 8, settled_requests: index === 5 ? 7 : 8, released_requests: 0, active_micros: '0', unresolved_micros: '0', preservation_verified: true, result_sha256: 'c'.repeat(64), costs_sha256: 'd'.repeat(64), workspace_sha256: 'e'.repeat(64) }));
  const result = { schema: 'cs1-followup-result/1', envelope_sha256, phase_sha256, actual_cost_micros: 123408, observed_attempts: 47, stopped: false, final_inputs_unchanged: true, runs: rows.map(row => ({ id: row.run_id, case_id: row.case_id, arm: row.arm, status: row.status, actual_cost_micros: row.actual_cost_micros, observed_attempts: row.requests, scope: { task: row.task_id }, workspace_sha256: row.workspace_sha256 })) };
  const audit = { schema: 'cs1-followup-owner-accounting-audit/1', envelope_sha256, phase_sha256, source_and_input_identities_unchanged: true, task_runs: 6, requests: 47, settled_requests: 47, released_requests: 0, cost_micros: 123408, cost_usd: '0.123408', active_micros: 0, unresolved_micros: 0, halt, rows };
  mutate({ audit, result, halt });
  const buffers = { halt: encode(halt), result: encode(result) };
  audit.halt_sha256 = sha(buffers.halt); audit.phase_result_sha256 = sha(buffers.result);
  buffers.accounting = encode(audit);
  const reference = { accounting_sha256: sha(buffers.accounting), result_sha256: sha(buffers.result), halt_sha256: sha(buffers.halt), envelope_sha256, phase_sha256, actual_cost_micros: 123408, observed_attempts: 47 };
  return { reference, buffers };
}
module.exports = { priorCampaignFixture };
