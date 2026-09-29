// SPDX-License-Identifier: Apache-2.0
'use strict';
// Successor evaluation accounting only. Never writes or settles a native ledger.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), capture = require('./developer-runner.cjs');
const { read, write, inspection, frames } = prior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function bound(reference) {
  if (!reference || !path.isAbsolute(reference.path || '') || !/^[a-f0-9]{64}$/.test(reference.sha256 || '')) throw Error('Exact successor evidence reference required');
  const bytes = read(reference.path, 16 * 1024 * 1024);
  if (sha(bytes) !== reference.sha256) throw Error('Successor evidence changed');
  return JSON.parse(bytes);
}
function allocations(decision) {
  const expected = { prior_conservative_micros: 113737, outer_cap_micros: 100000000, campaign_cap_micros: 64800000, campaign_requests: 1728,
    qualification_cap_micros: 250000, qualification_requests: 2, runtime_preflight_cap_micros: 600000, runtime_preflight_requests: 16,
    runtime_preflight_replacement_cap_micros: 600000, runtime_preflight_replacement_requests: 16, refresh_cap_micros: 250000, refresh_requests: 2 };
  if (Object.entries(expected).some(([k, v]) => decision[k] !== v)) throw Error('Successor conservative allocation differs');
  // Both native preflights retain their full individual ceilings, even when actual billing is unknown.
  const fixed = decision.prior_conservative_micros + decision.qualification_cap_micros + decision.runtime_preflight_cap_micros
    + decision.runtime_preflight_replacement_cap_micros + decision.refresh_cap_micros;
  if (fixed + decision.campaign_cap_micros > decision.outer_cap_micros) throw Error('Outer authorization cannot cover successor allocations');
  return fixed;
}
function priorRuntime(spec, decision) {
  const reference = spec.successor.prior_runtime_preflight;
  if (!reference || reference.sha256 !== decision.prior_runtime_preflight_sha256
    || decision.prior_runtime_preflight_sha256 !== '6225eee921f374b021ea69d830ee82858ec94dcac4c29c896301ea36611a83ca'
    || !path.isAbsolute(reference.source_archive || '')) throw Error('Exact failed prior native preflight required');
  const evidence = require('./cs3-read-preflight.cjs').validatePriorRuntime(reference, spec);
  if (evidence?.status !== 'conservative_failed_preflight_preserved' || evidence.conservative_debit_micros !== 600000 || evidence.actual_cost_micros !== null) throw Error('Prior native preflight must preserve unresolved billing and full conservative debit');
}
function validateSpec(spec) {
  const successor = spec.successor;
  if (!successor || !equal(Object.keys(successor).sort(), ['decision', 'predecessor', 'prior_runtime_preflight', 'qualification', 'runtime_preflight'])) throw Error('Exact successor approval and prerequisites required');
  const decision = bound(successor.decision), tracked = require('../../src/evals/skills/cs3-comparison/continuation-decision.json');
  if (!equal(decision, tracked) || decision.schema !== 'cs3-comparison-continuation-decision/1' || decision.authority !== 'owner_explicit_conservative_liability_continuation'
    || decision.model !== 'deepseek/deepseek-v3.2' || decision.endpoint !== 'deepinfra/fp4' || decision.unknown_slot_debit_micros !== 600000
    || decision.unknown_actual_cost !== 'null_not_settled' || decision.unknown_quality !== 'failed_never_replayed' || decision.historical_campaign !== 'terminal_halt_preserved') throw Error('Source-pinned successor decision differs');
  const fixed = allocations(decision);
  const predecessor = bound(successor.predecessor), directory = path.dirname(successor.predecessor.path);
  if (successor.predecessor.sha256 !== decision.predecessor_plan_sha256 || predecessor.schema !== 'cs3-comparison-plan/1' || predecessor.directory !== directory) throw Error('Exact terminal predecessor plan required');
  const prep = require('./authoring-prepare.cjs');
  if (!path.isAbsolute(successor.predecessor.source_archive || '') || !equal(prep.identity(successor.predecessor.source_archive, predecessor.source.scope), predecessor.source)) throw Error('Exact archived predecessor execution source required');
  if (!equal(bound(successor.predecessor.claim), { directory, plan_sha256: successor.predecessor.sha256 })) throw Error('Original predecessor claim differs');
  const audit = bound(successor.predecessor.audit);
  if (successor.predecessor.audit.sha256 !== decision.predecessor_audit_sha256 || audit.schema !== 'cs3-halted-campaign-audit/1' || audit.plan_sha256 !== successor.predecessor.sha256) throw Error('Original committed predecessor audit required');
  const halt = JSON.parse(read(path.join(directory, 'halt.json')));
  if (halt.plan_sha256 !== successor.predecessor.sha256 || sha(read(path.join(directory, 'halt.json'))) !== audit.halt_sha256
    || sha(read(path.join(directory, 'result-document-authoring.json'))) !== audit.result_sha256) throw Error('Original predecessor terminal halt/result required');
  const claimed = fs.readdirSync(path.join(directory, 'claims'));
  const expectedClaims = ['block-document-authoring.json', ...predecessor.runs.slice(0, 7).map(row => row.id + '.json')];
  if (!equal([...claimed].sort(), expectedClaims.sort()) || !equal(audit.rows.map(r => r.id), predecessor.runs.slice(0, 7).map(r => r.id))) throw Error('Exact original first seven claims required');
  for (const name of ['active-block.json', 'claims/block-document-authoring.json']) if (!equal(JSON.parse(read(path.join(directory, name))), { plan_sha256: successor.predecessor.sha256, skill: 'document-authoring' })) throw Error('Original predecessor block ownership differs');
  let carry = 197; // Exact separately retained original qualification debit.
  const inventory = ['halt.json', 'active-block.json', 'result-document-authoring.json', ...claimed.map(name => 'claims/' + name)];
  if (claimed.filter(name => name !== 'block-document-authoring.json').length !== 7) throw Error('Exact seven consumed predecessor slots required');
  for (const row of predecessor.runs) {
    const base = path.join(directory, row.id), workspace = prep.identity(path.join(base, 'workspace'), ['.']).files.map(f => ({ ...f, path: f.path.slice(2) })).sort((a, b) => a.path.localeCompare(b.path));
    if (!equal(workspace, [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Predecessor workspace preservation differs');
    if (sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256) throw Error('Predecessor prompt/profile differs');
    if (!claimed.includes(row.id + '.json') && (fs.readdirSync(path.join(base, 'data')).length || !equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']))) throw Error('Undispatched predecessor slot already has evidence or canonical data');
  }
  for (const row of predecessor.runs.filter(r => claimed.includes(r.id + '.json'))) {
    const base = path.join(directory, row.id), money = accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    if (!equal(JSON.parse(read(path.join(directory, 'claims', row.id + '.json'))), { plan_sha256: successor.predecessor.sha256, id: row.id })) throw Error('Original predecessor slot claim differs');
    const committed = audit.rows.find(r => r.id === row.id);
    if (!committed || committed.result_sha256 !== sha(read(path.join(base, 'result.json'))) || committed.costs_sha256 !== sha(read(path.join(base, 'costs.json'))) || committed.evidence_sha256 !== capture.runEvidence(base)) throw Error('Committed predecessor slot evidence changed');
    const report = JSON.parse(read(path.join(base, 'result.json')));
    if (report.id !== row.id || report.case_id !== row.case_id || report.arm !== row.arm || report.actual_cost_micros !== money.actual_cost_micros || money.attempts.some(a => !equal(a.scope, report.scope))
      || report.evidence_sha256 && report.evidence_sha256 !== capture.runEvidence(base)) throw Error('Predecessor report/accounting evidence differs');
    for (const name of fs.readdirSync(base).filter(name => !['data', 'workspace'].includes(name))) inventory.push(row.id + '/' + name);
    carry += money.known_settled_micros + money.unresolved_liability_micros;
  }
  if (carry !== decision.prior_conservative_micros) throw Error('Predecessor conservative carry differs');
  priorRuntime(spec, decision);
  const helper = require('./cs3-read-preflight.cjs');
  if (helper.validateQualification(successor.qualification, spec)?.status !== 'passed' || helper.validate(successor.runtime_preflight, spec)?.status !== 'passed') throw Error('Successor native/provider prerequisite validation failed');
  const predecessor_evidence = inventory.sort().map(relative => ({ path: relative, sha256: sha(read(path.join(directory, relative))) }));
  return { decision_id: decision.decision_id, fixed_conservative_micros: fixed, outer_cap_micros: decision.outer_cap_micros, predecessor_plan_sha256: successor.predecessor.sha256, predecessor_evidence };
}
function count(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || !Number.isSafeInteger(Number(value))) throw Error('Invalid canonical accounting counter');
  return Number(value);
}
function accounting(pages, cap) {
  if (!Array.isArray(pages) || !pages.length || pages.some(p => !Array.isArray(p.items) || p.gaps.length)) throw Error('Canonical cost evidence incomplete');
  const items = pages.flatMap(p => p.items);
  if (items.some(i => i.visibility !== 'available') || new Set(items.map(i => i.collection + ':' + i.id)).size !== items.length) throw Error('Unavailable or duplicate canonical accounting');
  const rows = kind => items.filter(i => i.collection === kind).map(i => i.record);
  const ledgers = rows('ledger'), attempts = rows('attempt'), reservations = rows('reservation'), settlements = rows('settlement');
  if (ledgers.length !== 1 || attempts.length > 16 || reservations.length !== attempts.length
    || new Set(attempts.map(a => a.id)).size !== attempts.length || new Set(reservations.map(r => r.id)).size !== reservations.length) throw Error('Exact bounded attempt/reservation inventory required');
  const ledger = ledgers[0], scope = ledger.scope;
  if (!scope?.task || ledger.currency !== 'USD' || count(ledger.cap) !== cap || ledger.overrun !== false || count(ledger.active) !== 0 || count(ledger.protected) !== 0 || !equal(ledger.allocations, {})) throw Error('Active, overrun or incompatible native ledger');
  let charged = 0, unresolved = 0, pending = 0;
  for (const attempt of attempts) {
    if (!equal(attempt.scope, scope) || attempt.root !== scope.task || attempt.role !== 'main' || attempt.previous !== null || !['settled', 'released', 'reconciliation_pending'].includes(attempt.phase)) throw Error('Unsupported attempt scope, retry, role or phase');
    const selected = reservations.filter(r => r.id === attempt.reservation && r.attempt === attempt.id);
    if (selected.length !== 1) throw Error('Exact reservation association required');
    const reservation = selected[0];
    if (!equal(reservation.scope, scope) || reservation.root !== scope.task || reservation.role !== 'main' || reservation.phase !== attempt.phase
      || reservation.charged !== attempt.charged || !equal(reservation.amount, attempt.quote?.amount) || reservation.amount.currency !== 'USD'
      || count(reservation.protected_draw) !== 0 || count(reservation.protected_returned) !== 0) throw Error('Reservation identity differs from attempt');
    const cost = count(attempt.charged), liability = count(reservation.liability), reserved = count(reservation.amount.micros);
    if (reserved > cap || cost > cap || liability > reserved) throw Error('Attempt exceeds bounded slot');
    const applied = settlements.filter(s => s.attempt === attempt.id && s.applied === true);
    if (applied.some(s => !equal(s.scope, scope) || !equal(s.observation?.scope, scope) || s.observation.attempt !== attempt.id || s.observation.amount?.currency !== 'USD')) throw Error('Settlement scope differs');
    if (attempt.phase === 'settled') {
      if (attempt.uncertain !== null || liability !== 0 || !applied.some(s => s.observation.final_usage === true && count(s.total) === cost && count(s.observation.amount.micros) === cost && s.observation.provider_request === attempt.provider_request)) throw Error('Settled attempt lacks exact final usage');
    } else if (attempt.phase === 'released') {
      if (cost || liability || attempt.uncertain !== null || attempt.send_intent !== null) throw Error('Released attempt has dispatched or charged liability');
    } else {
      if (typeof attempt.uncertain !== 'string' || !attempt.uncertain || !attempt.send_intent || !attempt.request_digest || liability <= 0 || cost + liability > cap) throw Error('Pending attempt lacks bounded canonical liability');
      pending++; unresolved += liability;
    }
    charged += cost;
  }
  if (settlements.some(s => !attempts.some(a => a.id === s.attempt)) || !Number.isSafeInteger(charged + unresolved) || charged !== count(ledger.settled) || unresolved !== count(ledger.unresolved) || charged + unresolved > cap) throw Error('Canonical liability totals do not reconcile');
  return { actual_cost_micros: pending ? null : charged, known_settled_micros: charged, conservative_debit_micros: pending ? cap : charged,
    unresolved_liability_micros: unresolved, unresolved_attempts: pending, attempts, ledger_scope: scope };
}
function pendingSafety(execution, output, evidence, profile, money) {
  const final = output.findLast(x => x.type === 'result'), accepted = output.find(x => x.type === 'accepted');
  if (!accepted?.scope?.task || !equal(money.ledger_scope, accepted.scope) || !equal(final?.scope, accepted.scope) || money.attempts.some(a => !equal(a.scope, accepted.scope) || a.root !== accepted.scope.task)) throw Error('Canonical accounting/final scope differs from executed task');
  if (!money.attempts.length) {
    // worker::admit_inner captures context-manifest and request-body artifacts
    // before reserve_captured. Prepared configuration/context is not dispatch.
    const data = output.map(x => x.event?.event?.data).filter(Boolean);
    if (data.some(d => d.attempt || d.reservation || d.settlement || (d.facts || []).some(f => ['attempt', 'reservation', 'settlement', 'effect'].includes(f.collection)))) throw Error('Zero-dispatch ledger contradicts canonical activity');
    const preparatory = ['canonical-skill-discovery/1', 'canonical-skill-discovery-context/1', 'canonical-active-skill-body/1', 'canonical-active-skill-resource/1', 'canonical-skill-activation/1',
      'verification-source/1', 'verification-baseline/1', 'verification-configuration/1', 'openrouter-endpoints/1', 'openrouter-provider-configuration/1', 'canonical-tool-ceiling/1',
      'canonical-coding-capabilities/1', 'canonical-coding-configuration/1', 'canonical-coding-content/1', 'context-manifest/1', 'canonical-context-handoff/1'];
    const preparedArtifact = descriptor => {
      const spec = descriptor?.spec;
      if (!equal(spec?.scope, accepted.scope) || spec.source !== 'retained-codex'
        || !(spec.channel === 'evidence' && preparatory.includes(spec.schema) || spec.channel === 'request_body' && ['responses-request/1', 'decisions-request/1'].includes(spec.schema))) throw Error('Zero-dispatch ledger contradicts response, context or effect evidence');
    };
    for (const fact of data.flatMap(d => d.facts || []).filter(f => f.collection === 'artifact')) preparedArtifact(fact.value);
    for (const view of ['tools', 'outputs', 'context']) {
      const pages = evidence[view];
      if (!Array.isArray(pages) || !pages.length || pages.some(p => !Array.isArray(p.items) || !Array.isArray(p.gaps) || p.gaps.length)) throw Error('Zero-dispatch evidence incomplete');
      for (const item of pages.flatMap(p => p.items)) {
        if (item.collection !== 'artifact' || item.visibility !== 'available') throw Error('Zero-dispatch ledger contradicts response, context or effect evidence');
        preparedArtifact(item.record);
      }
    }
    if (final.conditions?.completed !== false) throw Error('Zero-dispatch observation cannot qualify completed output');
  }
  if (!money.unresolved_attempts) {
    if (execution.error || final?.conditions?.unresolved_effect !== false) throw Error('Settled billing does not waive unresolved execution effects');
    return;
  }
  const expected = { unresolved_effect: true, cancelled: false, budget_exhausted: false, required_input: false, incomplete: false, invalid_configuration: false, internal_failure: false, durably_paused: true, completed: false };
  if (execution.error || execution.status !== 7 || final?.exit_code !== 7 || !equal(final.conditions, expected) || !equal(final.scope, accepted?.scope)) throw Error('Unknown billing lacks a terminated provider-only pause');
  const facts = output.flatMap(x => x.event?.event?.data?.facts || []);
  for (const kind of ['task', 'turn']) {
    const last = facts.filter(f => f.collection === kind).at(-1)?.value;
    if (!last || last.state !== 'paused' || last.reason !== 'provider outcome requires accounting reconciliation' || !equal(last.scope, accepted.scope)) throw Error('Provider-only pause lacks canonical task/turn evidence');
  }
  if (profile.maximum_autonomy !== 'plan' || !equal(profile.automatic_effects, []) || ['processes', 'checks', 'mcp', 'mcp_http'].some(k => profile[k] !== undefined && !equal(profile[k], []))
    || !Array.isArray(profile.canonical_tools) || profile.canonical_tools.some(t => !['vcp_list', 'vcp_read', 'vcp_search', 'vcp_verify'].includes(t))) throw Error('Pending provider outcome outside read-only profile');
  if (evidence.tools.some(p => p.gaps.length)) throw Error('Pending tool evidence incomplete');
  for (const item of evidence.tools.flatMap(p => p.items).filter(i => i.collection !== 'artifact')) {
    const effect = item.record;
    if (item.collection !== 'effect' || item.visibility !== 'available' || effect.state !== 'succeeded' || !equal(effect.scope, accepted.scope)
      || effect.exit_code !== null || effect.reason !== 'broker observed bounded file results; no automatic replay') throw Error('Pending provider outcome includes an ambiguous or unresolved effect');
  }
}
function captureResponses(plan, base, pages, attempts, call) {
  if (pages.some(p => p.gaps.some(g => !prior.privacyGap(g, g.artifact)))) throw Error('Response inventory incomplete');
  const items = pages.flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.channel === 'response');
  if (attempts.filter(a => a.send_intent || a.phase === 'settled').some(a => items.filter(i => i.record.spec.source === 'retained-codex-attempt:' + a.id).length !== 1)) throw Error('Dispatched provider attempt lacks one retained response');
  return items.map(item => {
    const descriptor = item.record;
    const attempt = attempts.find(a => descriptor.spec.source === 'retained-codex-attempt:' + a.id);
    if (!attempt || !attempt.send_intent || !equal(descriptor.spec.scope, attempt.scope)
      || items.filter(i => i.record.spec.source === descriptor.spec.source).length !== 1) throw Error('Response source/scope differs from dispatched attempt');
    if (descriptor.state === 'complete') {
      const bytes = capture.retained(plan, base, item, 'outputs', call);
      write(path.join(base, `response-${sha(item.id)}.sse`), bytes.toString('utf8')); return { item, bytes };
    }
    const length = count(descriptor.length);
    if (descriptor.state !== 'aborted' || descriptor.spec.schema !== 'responses-sse-observed-through-terminal/1' || !attempt || attempt.phase !== 'reconciliation_pending'
      || !equal(descriptor.spec.scope, attempt.scope) || !equal([...descriptor.spec.omissions].sort(), ['authentication_headers', 'explicit_abort', 'recovery_material'])
      || length > 1024 * 1024 || !equal(descriptor.retained, length ? [{ start: '0', end: String(length) }] : [])) throw Error('Aborted response lacks exact bounded provider-attempt provenance');
    const chunks = [];
    for (let offset = 0; offset < length; offset += 65536) {
      const ranged = inspection(plan, base, item.id, 'outputs', call, ['--offset', String(offset), '--length', '65536']);
      const row = ranged[0]?.items[0], end = Math.min(offset + 65536, length);
      const gapAllowed = gap => prior.privacyGap(gap, item.id) || gap.artifact === item.id && gap.visibility === 'omitted' && gap.capture_state === 'aborted'
        && gap.reason === 'only retained observed bytes are available; not reconstructed' && gap.range === undefined && equal([...(gap.omissions || [])].sort(), ['authentication_headers', 'explicit_abort', 'recovery_material']);
      if (ranged.length !== 1 || ranged[0].items.length !== 1 || ranged.some(p => p.gaps.some(g => !gapAllowed(g))) || row?.artifact !== item.id || row.visibility !== 'available'
        || !equal(row.descriptor, descriptor) || row.range?.start !== offset || row.range.end !== end || !Array.isArray(row.bytes) || row.bytes.length !== end - offset
        || row.bytes.some(b => !Number.isInteger(b) || b < 0 || b > 255)) throw Error('Aborted retained byte range unavailable');
      chunks.push(Buffer.from(row.bytes));
    }
    const bytes = Buffer.concat(chunks);
    if (sha(bytes) !== descriptor.sha256 || !bytes.equals(Buffer.from(bytes.toString('utf8')))) throw Error('Aborted response digest or UTF-8 differs');
    write(path.join(base, `response-${sha(item.id)}.sse`), bytes.toString('utf8')); return { item, bytes };
  });
}
function fields(money) { return Object.fromEntries(['actual_cost_micros', 'known_settled_micros', 'conservative_debit_micros', 'unresolved_liability_micros', 'unresolved_attempts'].map(k => [k, money[k]])); }
function reread(base, cap) {
  const money = accounting(JSON.parse(read(path.join(base, 'costs.json'))), cap);
  const output = frames(read(path.join(base, 'stdout.jsonl')).toString());
  const evidence = Object.fromEntries(['tools', 'outputs', 'context'].map(view => [view, JSON.parse(read(path.join(base, view + '.json')))]));
  pendingSafety({ status: output.findLast(x => x.type === 'result')?.exit_code }, output, evidence, JSON.parse(read(path.join(base, 'profile.json'))), money);
  return money;
}
module.exports = { count, accounting, pendingSafety, captureResponses, fields, reread, validateSpec, allocations, priorRuntime };
