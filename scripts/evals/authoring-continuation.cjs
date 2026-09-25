// SPDX-License-Identifier: Apache-2.0
'use strict';
// One prospective authority correction after the six fresh DOC comparisons.
// The original implementation validates its own evidence; no historical replay.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const reason = 'Prospective inherited tool-authority amendment; completed fresh phase retained without replay.';
function requireThat(condition, message) { if (!condition) throw Error(message); }
function document(file, hash) {
  const bytes = read(plain(path.resolve(file)), 8 * 1024 * 1024);
  requireThat(/^[a-f0-9]{64}$/.test(hash) && sha(bytes) === hash, 'Continuation evidence hash differs');
  return JSON.parse(bytes);
}
function inspect(reference, priorCampaign, sealed) {
  requireThat(reference && equal(Object.keys(reference).sort(), ['envelope', 'envelope_sha256', 'repository', 'gate_sha256', 'result_sha256'].sort()), 'Exact continuation references required');
  requireThat([reference.envelope, reference.repository].every(p => typeof p === 'string' && path.isAbsolute(p)), 'Absolute continuation paths required');
  const envelope = document(reference.envelope, reference.envelope_sha256);
  requireThat(reference.envelope === path.join(envelope.directory, 'envelope.json') && envelope.execution_mode === 'full_diagnostic' && !envelope.continuation, 'Only the original full diagnostic envelope may continue');
  requireThat(equal(envelope.prior_campaign, priorCampaign), 'Continuation must retain the same authenticated original liability');
  requireThat(equal(prep.identity(reference.repository, envelope.source.scope), envelope.source), 'Predecessor frozen source identity changed');
  document(envelope.spec_source, envelope.spec_sha256);
  const old = require(path.join(reference.repository, 'scripts/evals/authoring-followup.cjs'));
  requireThat(equal(old.sourceScope, envelope.source.scope), 'Predecessor validation scope changed');
  const active = path.join(envelope.directory, 'active-phase.json');
  const halt = path.join(envelope.directory, 'halt.json');
  if (sealed) {
    requireThat(equal(JSON.parse(read(active)), { schema: 'cs1-continuation-seal/1', envelope_sha256: reference.envelope_sha256 }), 'Predecessor must remain exclusively sealed');
    requireThat(equal(JSON.parse(read(halt)), { schema: 'cs1-followup-halt/1', reason, action: 'Read-only reconciliation. This envelope cannot resume or replay.' }), 'Predecessor retirement changed');
  } else {
    requireThat(!fs.existsSync(active) && !fs.existsSync(halt), 'Predecessor is active or already halted');
    old.envelopeFor(reference.envelope, reference.envelope_sha256);
  }
  const phase = path.join(envelope.directory, 'phases', 'document-authoring--normal');
  requireThat(equal(fs.readdirSync(path.join(envelope.directory, 'phases')), ['document-authoring--normal']), 'Only the six fresh DOC runs may precede this amendment');
  const result = document(path.join(phase, 'result.json'), reference.result_sha256);
  const gateDocument = document(path.join(phase, 'review-gate.json'), reference.gate_sha256);
  requireThat(result.runs.length === 6 && result.runs.every(row => ['completed', 'failed'].includes(row.status)) && !result.stopped && result.final_inputs_unchanged === true, 'Six settled predecessor runs required');
  requireThat(equal(fs.readdirSync(path.join(envelope.directory, 'claims')).sort(), result.runs.map(row => row.id + '.json').sort()), 'Predecessor has omitted execution claims');
  // This revalidates original canonical accounting, every result hash, final
  // workspace, two independent reviews, native receipts and original derivation.
  const inherited = old.derivePlan(envelope, reference.envelope_sha256, 'document-authoring', 'inherited');
  requireThat(inherited.prerequisites.length === 1 && inherited.prerequisites[0].sha256 === reference.gate_sha256 && gateDocument.result_sha256 === reference.result_sha256, 'Predecessor review gate differs');
  const rows = result.runs.map(row => ({ id: row.id, task_id: row.scope.task, status: row.status, actual_cost_micros: row.actual_cost_micros, observed_attempts: row.observed_attempts, active_micros: 0, unresolved_micros: 0 }));
  requireThat(envelope.candidate_assets && Array.isArray(envelope.candidate_assets.entries), 'Predecessor lacks explicit candidate source identity');
  return { reference, execution: { executable_sha256: envelope.executable_sha256, profile_sha256: envelope.profile_sha256, provider_catalog_sha256: envelope.provider_catalog_sha256, assets: envelope.assets, candidate_assets: envelope.candidate_assets }, gate: { ...inherited.prerequisites[0], decision: gateDocument.decision }, rows };
}
function verify(reference, priorCampaign) { return inspect(reference, priorCampaign, true); }
function seal(reference, priorCampaign) {
  inspect(reference, priorCampaign, false);
  const envelope = document(reference.envelope, reference.envelope_sha256);
  // wx takes the same exclusion slot used by the frozen runner. Failure never
  // releases another process's claim; a crash requires read-only reconciliation.
  fs.writeFileSync(path.join(envelope.directory, 'active-phase.json'), JSON.stringify({ schema: 'cs1-continuation-seal/1', envelope_sha256: reference.envelope_sha256 }) + '\n', { flag: 'wx', mode: 0o600 });
  fs.writeFileSync(path.join(envelope.directory, 'halt.json'), JSON.stringify({ schema: 'cs1-followup-halt/1', reason, action: 'Read-only reconciliation. This envelope cannot resume or replay.' }) + '\n', { flag: 'wx', mode: 0o600 });
  return verify(reference, priorCampaign);
}
module.exports = { verify, seal };
