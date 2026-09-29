// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const prep = require('./authoring-prepare.cjs'), capture = require('./developer-runner.cjs'), policy = require('./cs3-comparison-policy.cjs'), core = require('./cs3-comparison.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex'), json = file => JSON.parse(fs.readFileSync(file));
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-isolation-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const put = (name, value) => { const file = path.isAbsolute(name) ? name : path.join(root, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' ? value : JSON.stringify(value)); return { path: file, sha256: sha(fs.readFileSync(file)) }; };
  put('archive/source.txt', 'frozen'); put('repository/src/crates/native.rs', '// immutable native');
  const source = prep.identity(path.join(root, 'archive'), ['source.txt']), native = prep.identity(path.join(root, 'repository'), ['src/crates']);
  const names = require('./cs3-comparison-candidates.cjs').ids;
  const rows = Array.from({ length: 108 }, (_, index) => ({ id: 'case-' + index, case_id: 'case-' + index, arm: ['none', 'nearest', 'candidate'][index % 3],
    skill: names[Math.floor(index / 18)], cap_micros: 600000, call_ceiling: 16, files: [], skills: [] }));
  const directory = path.join(root, 'original'), control = path.join(root, 'segment');
  const profile = { maximum_autonomy: 'plan', automatic_effects: [], canonical_tools: ['vcp_read', 'vcp_verify'] };
  for (const row of rows) { const base = path.join(directory, row.id); fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    row.profile_sha256 = put(path.join(base, 'profile.json'), profile).sha256; row.prompt_sha256 = put(path.join(base, 'prompt.txt'), 'frozen prompt').sha256; }
  const build = put('build.json', { source_inputs: native });
  const origin = { schema: 'cs3-comparison-plan/2', directory, source, runs: rows }, originRef = put('original/plan.json', origin);
  const originalGlobal = put('git/original.json', { directory, plan_sha256: originRef.sha256 });
  const oldHashes = Object.fromEntries(['halt.json', 'active-block.json', 'result-document-authoring.json'].map(n => [n, put('original/' + n, { immutable: n }).sha256]));
  const oldClaims = ['block-document-authoring', rows[0].id, rows[1].id].map(id => ({ path: 'claims/' + id + '.json', sha256: put('original/claims/' + id + '.json', { old: id }).sha256 }));
  const oldAudit = put('old-audit.json', { global_claim_sha256: originalGlobal.sha256, halt_sha256: oldHashes['halt.json'], block_result_sha256: oldHashes['result-document-authoring.json'], active_block_sha256: oldHashes['active-block.json'], original_claims: oldClaims });
  const raw = (row, text = null) => {
    const base = path.join(directory, row.id), scope = { workspace: 'workspace', session: 'session', task: row.id }, paid = text !== null, charged = paid ? '37' : '0';
    const records = [{ collection: 'ledger', id: 'ledger', visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: charged, active: '0', unresolved: '0', protected: '0', allocations: {}, overrun: false } }];
    if (paid) records.push(...[
      ['attempt', { id: 'attempt', scope, root: row.id, role: 'main', previous: null, phase: 'settled', charged, uncertain: null, send_intent: 'sent', request_digest: 'digest', reservation: 'reservation', provider_request: 'request', quote: { amount: { currency: 'USD', micros: '100' } } }],
      ['reservation', { id: 'reservation', scope, root: row.id, role: 'main', attempt: 'attempt', phase: 'settled', charged, liability: '0', protected_draw: '0', protected_returned: '0', amount: { currency: 'USD', micros: '100' } }],
      ['settlement', { scope, attempt: 'attempt', applied: true, total: charged, observation: { scope, attempt: 'attempt', provider_request: 'request', amount: { currency: 'USD', micros: charged }, final_usage: true } }],
    ].map(([collection, record]) => ({ collection, id: collection, visibility: 'available', record })));
    const costs = [{ gaps: [], items: records }]; put(path.join(base, 'costs.json'), costs);
    const output = paid ? Buffer.from('data: ' + JSON.stringify({ type: 'response.completed', response: { output: [{ type: 'message', content: [{ type: 'output_text', text }] }] } }) + '\n\n') : null;
    const items = paid ? [{ collection: 'artifact', id: 'response', visibility: 'available', record: { state: 'complete', length: String(output.length), sha256: sha(output), spec: { scope, source: 'retained-codex-attempt:attempt', channel: 'response' } } }] : [];
    put(path.join(base, 'outputs.json'), [{ gaps: [], items }]);
    if (paid) put(path.join(base, 'response-' + sha('response') + '.sse'), output.toString());
    for (const view of ['context', 'tools']) put(path.join(base, view + '.json'), [{ gaps: [], items: [] }]);
    put(path.join(base, 'stdout.jsonl'), [{ type: 'accepted', scope }, { type: 'result', scope, exit_code: 1, conditions: { completed: false, unresolved_effect: false, internal_failure: true } }].map(JSON.stringify).join('\n'));
    return { id: row.id, case_id: row.case_id, arm: row.arm, scope, status: 'failed', accounted: true, preserved: true, ...policy.fields(policy.accounting(costs, 600000)), observed_attempts: paid ? 1 : 0, evidence_sha256: capture.runEvidence(base) };
  };
  const consumed = rows.slice(0, 11).map((row, index) => {
    const report = raw(row), base = path.join(directory, row.id), file = put(path.join(base, 'result.json'), report);
    const result = index < 2 ? put(path.join(control, 'addenda', row.id + '.json'), report) : file;
    return { id: row.id, status: 'failed', result_sha256: result.sha256, evidence_sha256: report.evidence_sha256, inventory: prep.identity(base, ['.']), ...policy.fields(report), observed_attempts: 0 };
  });
  const previous = { ...origin, schema: 'cs3-comparison-segment-plan/1', control_directory: control, successor: { fixed_conservative_micros: 1813737, outer_cap_micros: 100000000 },
    spec: { profile: put('source-profile.json', profile), build_receipt: build, executable: {}, successor: true },
    segment: { spec: { origin: { ...originRef, source_archive: path.join(root, 'archive') }, audit: oldAudit }, addenda: rows.slice(0, 2).map(r => json(path.join(control, 'addenda', r.id + '.json'))) } };
  const previousRef = put('segment/plan.json', previous), segmentGlobal = put('git/segment.json', { control_directory: control, original_plan_sha256: originRef.sha256, plan_sha256: previousRef.sha256 });
  const halt = { reason: 'Synthetic canary disclosed in canonical output' }, halted = put('segment/halt.json', halt), active = put('segment/active-block.json', {}), block = put('segment/result-document-authoring.json', {});
  const claims = ['block-document-authoring', ...rows.slice(2, 11).map(r => r.id)].map(id => ({ name: id + '.json', sha256: put('segment/claims/' + id + '.json', {}).sha256 }));
  const totals = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  const audit = put('audit.json', { schema: 'cs3-comparison-segment-terminal-audit/1', plan_sha256: previousRef.sha256, claim_sha256: segmentGlobal.sha256, halt, halt_sha256: halted.sha256,
    block_sha256: block.sha256, active_block_sha256: active.sha256, claims, consumed, totals, remaining_ids: rows.slice(11).map(r => r.id) });
  const decision = { ...require('../../src/evals/skills/cs3-comparison/isolation-decision.json'), predecessor_plan_sha256: previousRef.sha256, predecessor_audit_sha256: audit.sha256, predecessor_source_identity: source.content_sha256 };
  const specification = put('specification.json', { decision: put('decision.json', decision), predecessor: { ...previousRef, source_archive: path.join(root, 'archive') }, audit });
  const filename = require.resolve('./cs3-comparison-isolated.cjs'), actualRequire = require('node:module').createRequire(filename), module = { exports: {} }, state = { checks: 0, interleave: null };
  const scopedFs = { ...fs, unlinkSync(file) { fs.unlinkSync(file); if (state.interleave && file.endsWith('active-skill.json')) { const action = state.interleave; state.interleave = null; action(); } } };
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, name => {
    if (name === 'node:fs') return scopedFs;
    if (name === '../../src/evals/skills/cs3-comparison/isolation-decision.json') return decision;
    if (name === './cs3-comparison-segment.cjs') return { pristine: actualRequire(name).pristine, claimFile: () => segmentGlobal.path };
    if (name === './cs3-comparison.cjs') return { ...core, claimFile: () => originalGlobal.path, sourceIdentity: () => source, buildProvenance() {}, validateExecution() { state.checks++; }, qualificationWindow() {},
      cohort: () => rows.map(r => ({ id: r.case_id, forbidden_literals: ['TEST_CANARY'] })) };
    if (name === './cs3-comparison-review.cjs') return { validateTerminalDisposition(file, hash) { return module.exports.validateTerminal(json(file), hash); }, validateDisposition(file, hash) { return module.exports.validateReaderTerminal(json(file), hash); } };
    return actualRequire(name);
  }, module, filename, path.join(root, 'repository/scripts/evals'));
  return { root, put, raw, rows, previous, state, helper: module.exports, spec: specification, destination: path.join(root, 'isolated') };
}
function prepared(f) { const result = f.helper.prepare(f.spec.path, f.destination); return result.plans.map(ref => ({ ...ref, plan: json(ref.path) })); }
function localFailure(f, group) {
  f.helper.begin(group.plan, group.sha256);
  const row = group.plan.runs[0]; f.put(path.join(group.plan.control_directory, 'claims', 'block-' + row.skill + '.json'), { plan_sha256: group.sha256, skill: row.skill });
  f.put(path.join(group.plan.control_directory, 'claims', row.id + '.json'), { plan_sha256: group.sha256, id: row.id });
  const report = f.raw(row, 'TEST_CANARY'), reason = 'Synthetic canary disclosed in canonical output';
  const failure = f.helper.failure(group.plan, group.sha256, row, report, Error(reason)); assert.equal(failure.failure_scope, 'skill');
  f.put(path.join(group.plan.directory, row.id, 'result.json'), report);
  f.put(path.join(group.plan.control_directory, 'halt.json'), { plan_sha256: group.sha256, slot: row.id, reason, ...failure });
  f.put(path.join(group.plan.control_directory, 'result-' + row.skill + '.json'), { schema: 'cs3-comparison-block/1', plan_sha256: group.sha256, skill: row.skill, stopped: true, runs: [report], ...policy.fields(report), observed_attempts: 1 });
  f.put(path.join(group.plan.control_directory, 'terminal-disposition-' + row.skill + '.json'), {}); // Reader wrapper is independently tested; mock rederives the actual native proof.
  return { row, report };
}
test('one shared allocation owns90 unchanged slots and preserves eleven consumed plus seven excluded DOC slots', t => {
  const f = fixture(t), before = prep.identity(path.join(f.root, 'original'), ['.']);
  const dry = f.helper.prepare(f.spec.path, f.destination, true); assert.equal(dry.remaining, 90); assert.equal(dry.excluded, 7); assert.equal(f.state.checks, 1); assert.equal(fs.existsSync(f.helper.claimFile()), false);
  const groups = prepared(f); assert.equal(groups.length, 5); assert.equal(new Set(groups.flatMap(g => g.plan.runs.map(r => r.id))).size, 90);
  assert.deepEqual(prep.identity(path.join(f.root, 'original'), ['.']), before);
  for (const group of groups) assert.deepEqual(f.helper.validate(group.plan, group.sha256), group.plan);
  assert.throws(() => f.helper.prepare(f.spec.path, path.join(f.root, 'second')), /unclaimed envelope/);
  localFailure(f, groups[0]); const proof = f.helper.validateTerminal(groups[0].plan, groups[0].sha256); assert.equal(proof.accounting.actual_cost_micros, 37);
  assert.equal(f.helper.admission(groups[1].plan).conservative_debit_micros, 37);
  f.helper.begin(groups[1].plan, groups[1].sha256); f.helper.assertActive(groups[1].plan, groups[1].sha256);
  assert.throws(() => f.helper.begin(groups[0].plan, groups[0].sha256), /already consumed/);
  assert.throws(() => f.helper.assertActive(groups[0].plan, groups[0].sha256), /ownership changed/);
});
test('exclusive handoff rejects a concurrent second controller without replacing the winner', t => {
  const f = fixture(t), groups = prepared(f); localFailure(f, groups[0]);
  let rejected = false; f.state.interleave = () => { assert.throws(() => f.helper.begin(groups[2].plan, groups[2].sha256), /ownership differs|EEXIST/); rejected = true; };
  f.helper.begin(groups[1].plan, groups[1].sha256); assert.equal(rejected, true); f.helper.assertActive(groups[1].plan, groups[1].sha256);
  assert.deepEqual(fs.readdirSync(path.join(f.destination, 'transitions')).sort(), ['0.json', '1.json']);
});
test('artifact-only read error is safe; hidden responses, foreign scope and real failed effects are not', t => {
  const f = fixture(t), group = prepared(f)[0], row = group.plan.runs[0], base = path.join(group.plan.directory, row.id), report = f.raw(row, 'ordinary output');
  const effect = { collection: 'effect', visibility: 'available', record: { scope: report.scope, state: 'succeeded', exit_code: null, reason: 'broker observed bounded file results; no automatic replay' } };
  f.put(path.join(base, 'tools.json'), [{ gaps: [], items: [effect, { collection: 'artifact', record: { spec: { schema: 'canonical-coding-pair/1' } } }] }]);
  report.evidence_sha256 = capture.runEvidence(base); assert.doesNotThrow(() => f.helper.safeReport(group.plan, row, report));
  effect.record.state = 'failed'; f.put(path.join(base, 'tools.json'), [{ gaps: [], items: [effect] }]); report.evidence_sha256 = capture.runEvidence(base); assert.throws(() => f.helper.safeReport(group.plan, row, report), /native effect/);
  f.put(path.join(base, 'tools.json'), [{ gaps: [], items: [] }]);
  f.put(path.join(base, 'response-' + 'a'.repeat(64) + '.sse'), 'TEST_CANARY'); report.evidence_sha256 = capture.runEvidence(base); assert.throws(() => f.helper.safeReport(group.plan, row, report), /Unbound/);
  fs.unlinkSync(path.join(base, 'response-' + 'a'.repeat(64) + '.sse')); report.evidence_sha256 = capture.runEvidence(base); report.scope = { ...report.scope, task: 'foreign' }; assert.throws(() => f.helper.safeReport(group.plan, row, report), /identity/);
});
test('only descriptor-bound model canaries remain local; unbound answer and unknown errors halt all groups', t => {
  for (const reason of ['Synthetic canary disclosed in canonical output', 'Interrupted CLI; unknown liability must be reconciled']) {
    const f = fixture(t), group = prepared(f)[0]; f.helper.begin(group.plan, group.sha256); const row = group.plan.runs[0];
    f.put(path.join(group.plan.control_directory, 'claims', 'block-' + row.skill + '.json'), { plan_sha256: group.sha256, skill: row.skill });
    f.put(path.join(group.plan.control_directory, 'claims', row.id + '.json'), { plan_sha256: group.sha256, id: row.id });
    const report = f.raw(row, 'ordinary output'); f.put(path.join(group.plan.directory, row.id, 'answer.json'), { report: 'TEST_CANARY' });
    const failure = f.helper.failure(group.plan, group.sha256, row, report, Error(reason)); assert.equal(failure.failure_scope, 'global');
    assert.throws(() => f.helper.validate(group.plan, group.sha256), /integrity halted/);
  }
});
test('canonical streaming canary detector joins only matching response/item/content deltas', () => {
  const helper = require('./cs3-comparison-isolated.cjs'), event = (id, delta) => 'data: ' + JSON.stringify({ type: 'response.output_text.delta', item_id: id, delta }) + '\n\n';
  assert.equal(helper.responseCanary(event('same', 'TEST_') + event('same', 'CANARY'), ['TEST_CANARY']), true);
  assert.equal(helper.responseCanary(event('first', 'TEST_') + event('other', 'CANARY'), ['TEST_CANARY']), false);
});
