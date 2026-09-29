// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const policy = require('./cs3-comparison-policy.cjs');
const scope = { workspace: 'workspace', session: 'session', task: 'task' };
function fixture(pending = false) {
  const phase = pending ? 'reconciliation_pending' : 'settled', charged = pending ? '0' : '37', liability = pending ? '100' : '0';
  const records = {
    ledger: [{ scope, currency: 'USD', cap: '600000', settled: charged, active: '0', unresolved: liability, protected: '0', allocations: {}, overrun: false }],
    attempt: [{ id: 'attempt', scope, root: 'task', role: 'main', previous: null, phase, charged, uncertain: pending ? 'retained response did not complete' : null,
      send_intent: 'sent', request_digest: 'request-digest', reservation: 'reservation', provider_request: pending ? null : 'provider-request', quote: { amount: { currency: 'USD', micros: '100' } } }],
    reservation: [{ id: 'reservation', attempt: 'attempt', scope, root: 'task', role: 'main', phase, charged, liability, protected_draw: '0', protected_returned: '0', amount: { currency: 'USD', micros: '100' } }],
    settlement: pending ? [] : [{ scope, attempt: 'attempt', applied: true, total: charged, observation: { scope, attempt: 'attempt', provider_request: 'provider-request', amount: { currency: 'USD', micros: charged }, final_usage: true } }],
  };
  return [{ gaps: [], items: Object.entries(records).flatMap(([collection, rows]) => rows.map((record, i) => ({ collection, id: record.id || collection + i, visibility: 'available', record }))) }];
}
test('successor conservatively debits a full pending slot without inventing actual settlement', () => {
  const settled = policy.accounting(fixture(), 600000), pending = policy.accounting(fixture(true), 600000);
  assert.deepEqual(policy.fields(settled), { actual_cost_micros: 37, known_settled_micros: 37, conservative_debit_micros: 37, unresolved_liability_micros: 0, unresolved_attempts: 0 });
  assert.deepEqual(policy.fields(pending), { actual_cost_micros: null, known_settled_micros: 0, conservative_debit_micros: 600000, unresolved_liability_micros: 100, unresolved_attempts: 1 });
  assert.throws(() => require('./p6-live-runner.cjs').accounting(fixture(true), 600000), /unknown liability/);
});
test('successor accounting rejects active, duplicate, misbound, over-cap and falsely settled ledgers', () => {
  for (const change of [
    (r, p) => p[0].gaps.push({}), (r, p) => p[0].items.push(p[0].items[0]), r => { r.ledger.active = '1'; }, r => { r.ledger.overrun = true; },
    r => { r.ledger.unresolved = '99'; }, r => { r.ledger.settled = '1'; }, r => { r.attempt.previous = 'old'; }, r => { r.attempt.role = 'helper'; },
    r => { r.attempt.scope = { ...scope, task: 'other' }; }, r => { r.reservation.attempt = 'other'; }, r => { r.reservation.liability = '600001'; },
    r => { r.reservation.charged = '1'; }, r => { r.reservation.phase = 'submitted'; }, r => { r.attempt.send_intent = null; }, r => { r.attempt.uncertain = null; },
    r => { r.attempt.phase = r.reservation.phase = 'settled'; r.ledger.unresolved = r.reservation.liability = '0'; r.attempt.uncertain = null; },
  ]) {
    const pages = fixture(true), records = Object.fromEntries(pages[0].items.map(i => [i.collection, i.record])); change(records, pages);
    assert.throws(() => policy.accounting(pages, 600000));
  }
});
test('zero-dispatch costs require a complete zero ledger bound to failed task and no hidden activity', () => {
  const pages = fixture(); pages[0].items = pages[0].items.filter(i => i.collection === 'ledger'); pages[0].items[0].record.settled = '0';
  const money = policy.accounting(pages, 600000);
  assert.deepEqual(policy.fields(money), { actual_cost_micros: 0, known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0 });
  const f = pause(); f.execution.status = 1; f.output.at(-1).exit_code = 1; f.output.at(-1).conditions.unresolved_effect = false; f.output.at(-1).conditions.internal_failure = true;
  f.evidence = Object.fromEntries(['tools', 'outputs', 'context'].map(k => [k, [{ gaps: [], items: [] }]]));
  const verify = (evidence = f.evidence, cost = money) => policy.pendingSafety(f.execution, f.output, evidence, f.profile, cost);
  assert.doesNotThrow(() => verify());
  const prepared = structuredClone(f.evidence);
  for (const [channel, schema] of [['evidence', 'context-manifest/1'], ['evidence', 'canonical-coding-capabilities/1'], ['evidence', 'openrouter-endpoints/1'], ['request_body', 'responses-request/1']]) {
    prepared.context[0].items.push({ collection: 'artifact', visibility: 'available', record: { spec: { scope, source: 'retained-codex', channel, schema } } });
  }
  assert.doesNotThrow(() => verify(prepared));
  assert.throws(() => verify(f.evidence, { ...money, ledger_scope: { ...scope, task: 'foreign' } }), /scope differs/);
  for (const field of ['active', 'settled', 'unresolved', 'protected']) { const changed = structuredClone(pages); changed[0].items[0].record[field] = '1'; assert.throws(() => policy.accounting(changed, 600000)); }
  for (const view of ['tools', 'outputs', 'context']) {
    const gap = structuredClone(f.evidence); gap[view][0].gaps.push({}); assert.throws(() => verify(gap), /incomplete/);
    const hidden = structuredClone(f.evidence); hidden[view][0].items.push({ collection: 'effect', visibility: 'available', record: {} }); assert.throws(() => verify(hidden), /contradicts/);
    const manifest = structuredClone(f.evidence); manifest[view][0].items.push({ collection: 'artifact', visibility: 'available', record: { spec: { scope, channel: 'context', schema: 'context-manifest/1' } } }); assert.throws(() => verify(manifest), /contradicts/);
  }
  const dispatched = structuredClone(f.output); dispatched.splice(1, 0, { type: 'event', event: { event: { data: { attempt: { id: 'hidden' } } } } });
  assert.throws(() => policy.pendingSafety(f.execution, dispatched, f.evidence, f.profile, money), /canonical activity/);
  for (const [channel, schema] of [['response', 'responses-sse-observed-through-terminal/1'], ['stdout', 'retained-full-output/1'], ['evidence', 'canonical-coding-pair/1']]) {
    const hidden = structuredClone(f.output); hidden.splice(1, 0, { type: 'event', event: { event: { data: { facts: [{ collection: 'artifact', value: { spec: { scope, source: 'retained-codex', channel, schema } } }] } } } });
    assert.throws(() => policy.pendingSafety(f.execution, hidden, f.evidence, f.profile, money), /contradicts/);
  }
  f.output.at(-1).conditions.completed = true; assert.throws(() => verify(), /cannot qualify/);
  assert.deepEqual(policy.captureResponses({}, 'unused', [{ gaps: [], items: [] }], [], () => { assert.fail('No inspection/provider call permitted'); }), []);
});
test('successor outer envelope cannot borrow from reserved fixed allocations', () => {
  const campaign = require('./cs3-comparison.cjs');
  assert.throws(() => campaign.admission({ runs: [], successor: { fixed_conservative_micros: 99400001, outer_cap_micros: 100000000 } }), /Outer conservative/);
  const admitted = campaign.admission({ runs: [], successor: { fixed_conservative_micros: 1813737, outer_cap_micros: 100000000 } });
  assert.equal(admitted.conservative_debit_micros, 0); assert.equal(admitted.reserved_micros, 600000);
});
test('single replacement preserves both preflight ceilings inside the fixed outer envelope', () => {
  const decision = require('../../src/evals/skills/cs3-comparison/continuation-decision.json');
  assert.equal(policy.allocations(decision), 1813737);
  assert.equal(policy.allocations(decision) + decision.campaign_cap_micros, 66613737);
  // Preserved first campaign: thirteen dispatched attempts plus two qualification requests.
  assert.equal(decision.campaign_requests + decision.qualification_requests + decision.runtime_preflight_requests
    + decision.runtime_preflight_replacement_requests + decision.refresh_requests + 15, 1779);
  for (const key of ['runtime_preflight_cap_micros', 'runtime_preflight_requests', 'runtime_preflight_replacement_cap_micros', 'runtime_preflight_replacement_requests']) {
    assert.throws(() => policy.allocations({ ...decision, [key]: 0 }), /allocation differs/);
    assert.throws(() => policy.allocations({ ...decision, [key]: decision[key] * 2 }), /allocation differs/);
  }
});
test('replacement requires pinned failed receipt and authenticated full-debit unresolved evidence', () => {
  const decision = require('../../src/evals/skills/cs3-comparison/continuation-decision.json');
  const filename = require.resolve('./cs3-comparison-policy.cjs'), actualRequire = require('node:module').createRequire(filename);
  let answer = { status: 'conservative_failed_preflight_preserved', conservative_debit_micros: 600000, actual_cost_micros: null }, calls = 0;
  const module = { exports: {} }, reference = { path: path.resolve('synthetic-failed-result.json'), sha256: decision.prior_runtime_preflight_sha256,
    source_archive: path.resolve('synthetic-archived-source'), terminal_response: { path: path.resolve('synthetic-terminal-pages.json'), sha256: 'a'.repeat(64) } };
  const spec = { successor: { prior_runtime_preflight: reference } };
  require('node:vm').runInThisContext(require('node:module').wrap(fs.readFileSync(filename, 'utf8')), { filename })(module.exports, name => {
    if (name !== './cs3-read-preflight.cjs') return actualRequire(name);
    return { validatePriorRuntime(ref, passedSpec) { calls++; assert.equal(ref, reference); assert.equal(passedSpec, spec); if (answer instanceof Error) throw answer; return answer; } };
  }, module, filename, path.dirname(filename));
  assert.doesNotThrow(() => module.exports.priorRuntime(spec, decision)); assert.equal(calls, 1);
  for (const changed of [undefined, { ...reference, sha256: 'b'.repeat(64) }, { ...reference, source_archive: 'relative' }]) {
    assert.throws(() => module.exports.priorRuntime({ successor: { prior_runtime_preflight: changed } }, decision), /Exact failed/);
  }
  assert.throws(() => module.exports.priorRuntime(spec, { ...decision, prior_runtime_preflight_sha256: 'b'.repeat(64) }), /Exact failed/);
  for (const invalid of [undefined, { ...answer, status: 'passed' }, { ...answer, conservative_debit_micros: 129576 }, { ...answer, actual_cost_micros: 1248 }, { ...answer, actual_cost_micros: 0 }]) {
    const previous = answer; answer = invalid;
    assert.throws(() => module.exports.priorRuntime(spec, decision), /preserve unresolved/); answer = previous;
  }
  answer = Error('Authentic archived raw evidence changed');
  assert.throws(() => module.exports.priorRuntime(spec, decision), /archived raw evidence changed/);
});
function pause() {
  const conditions = { unresolved_effect: true, cancelled: false, budget_exhausted: false, required_input: false, incomplete: false, invalid_configuration: false, internal_failure: false, durably_paused: true, completed: false };
  const output = [{ type: 'accepted', scope }, ...['task', 'turn'].map(collection => ({ type: 'event', event: { event: { data: { facts: [{ collection, value: { scope, state: 'paused', reason: 'provider outcome requires accounting reconciliation' } }] } } } })), { type: 'result', scope, exit_code: 7, conditions }];
  return { execution: { status: 7 }, output, evidence: { tools: [{ gaps: [], items: [] }] }, profile: { maximum_autonomy: 'plan', automatic_effects: [], canonical_tools: ['vcp_read', 'vcp_verify'] } };
}
test('unresolved_effect flag is allowed only for exact terminated provider-accounting-only pause', () => {
  const money = policy.accounting(fixture(true), 600000), invoke = f => policy.pendingSafety(f.execution, f.output, f.evidence, f.profile, money);
  assert.doesNotThrow(() => invoke(pause()));
  assert.throws(() => policy.pendingSafety(pause().execution, pause().output, pause().evidence, pause().profile, { ...money, attempts: money.attempts.map(a => ({ ...a, scope: { ...scope, task: 'other' }, root: 'other' })) }), /scope differs/);
  const settled = pause(); settled.output.at(-1).scope = { ...scope, task: 'foreign' }; settled.output.at(-1).conditions.unresolved_effect = false;
  assert.throws(() => policy.pendingSafety(settled.execution, settled.output, settled.evidence, settled.profile, policy.accounting(fixture(), 600000)), /scope differs/);
  for (const change of [f => { f.execution.error = 'timeout'; }, f => { f.execution.status = 0; }, f => { f.output.at(-1).conditions.completed = true; },
    f => { f.output[1].event.event.data.facts[0].value.reason = 'unresolved process'; }, f => { f.profile.processes = [{}]; }, f => { f.profile.canonical_tools.push('vcp_exec'); },
    f => { f.evidence.tools[0].gaps.push({}); }, f => { f.evidence.tools[0].items.push({ collection: 'effect', visibility: 'available', record: { scope, state: 'running' } }); }]) {
    const f = pause(); change(f); assert.throws(() => invoke(f));
  }
});
test('acknowledged aborted response imports exact observed bytes but rejects capture gaps and canary leakage', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-aborted-')); t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const bytes = Buffer.from('data: {"type":"response.output_text.delta","item_id":"a","delta":"CS3_DOC_"}\n\ndata: {"type":"response.output_text.delta","item_id":"a","delta":"PRIVATE_CANARY_82D4"}\n\n');
  const record = { length: String(bytes.length), retained: [{ start: '0', end: String(bytes.length) }], sha256: crypto.createHash('sha256').update(bytes).digest('hex'), state: 'aborted',
    spec: { source: 'retained-codex-attempt:attempt', scope, channel: 'response', schema: 'responses-sse-observed-through-terminal/1', omissions: ['authentication_headers', 'recovery_material', 'explicit_abort'] } };
  const pages = [{ gaps: [], items: [{ id: 'artifact', collection: 'artifact', record }] }];
  const attempts = policy.accounting(fixture(true), 600000).attempts;
  const call = () => ({ status: 0, stdout: JSON.stringify({ type: 'result', data: { gaps: [{ artifact: 'artifact', visibility: 'omitted', capture_state: 'aborted', omissions: record.spec.omissions, reason: 'only retained observed bytes are available; not reconstructed' }], items: [{ artifact: 'artifact', visibility: 'available', descriptor: record, range: { start: 0, end: bytes.length }, bytes: [...bytes] }], next_cursor: null } }) });
  assert.equal(policy.captureResponses({ executable: 'synthetic' }, directory, pages, attempts, call)[0].bytes.equals(bytes), true);
  assert.equal(require('./developer-runner.cjs').canaryDisclosed(directory, { forbidden_output_literals: ['CS3_DOC_PRIVATE_CANARY_82D4'] }), true);
  const bad = structuredClone(pages); bad[0].items[0].record.spec.omissions.push('capture_failure');
  assert.throws(() => policy.captureResponses({}, directory, bad, attempts, call), /provenance/);
  assert.throws(() => policy.captureResponses({}, directory, [{ gaps: [], items: [] }], attempts, call), /one retained response/);
  assert.throws(() => policy.captureResponses({}, directory, [{ gaps: [], items: [] }], policy.accounting(fixture(), 600000).attempts, call), /one retained response/);
  const foreign = structuredClone(pages); foreign[0].items[0].record.state = 'complete'; foreign[0].items[0].record.spec.scope.task = 'foreign';
  assert.throws(() => policy.captureResponses({}, directory, foreign, attempts, call), /source\/scope/);
});
module.exports = { fixture, pause };
