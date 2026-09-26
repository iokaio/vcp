// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { ownedRoot } = require('../support/experiments.cjs');
const runner = require('../../../scripts/evals/authoring-skl-continuation.cjs');
const frozen = require('../../../scripts/evals/authoring-qualification.cjs');
const review = require('../../../scripts/evals/authoring-skl-review.cjs');
const rec = require('../../../scripts/evals/authoring-skl-reconciliation.cjs');
const prior = require('../../../scripts/evals/p6-live-runner.cjs');
const { sha } = runner, clone = value => JSON.parse(JSON.stringify(value));
const save = (file, value) => fs.writeFileSync(file, Buffer.isBuffer(value) || typeof value === 'string' ? value : JSON.stringify(value));
function owned(t) { const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup()); return owner.root; }
function budget() { return { grant: 'test-existing-grant', cap_micros: 100000000, settled_micros: 644378, consumed_ids: [rec.consumed, 'document-authoring--normal--00--candidate'], task_ids: ['old-cs2', 'old-doc', 'old-skl', 'historical'], active_micros: 0, unresolved_micros: 0 }; }
test('exact 26 original SKL slots retain conditional5/18/3 rotation, caps and no DOC/consumed member', () => {
  const slots = runner.allocation(frozen.slots());
  assert.equal(slots.length, 26); assert.deepEqual(['normal', 'inherited', 'confirmation'].map(p => slots.filter(r => r.phase === p).length), [5, 18, 3]);
  assert.equal(slots.reduce((s, r) => s + r.cap_micros, 0), 45500000); assert.equal(slots.reduce((s, r) => s + r.call_ceiling, 0), 416);
  assert.equal(slots[0].position, 1); assert.equal(slots[1].position, 2); assert(slots.every(r => r.id !== rec.consumed && r.candidate === 'skill-authoring'));
  const altered = frozen.slots(); altered.find(r => r.id === rec.consumed).arm = 'candidate'; assert.throws(() => runner.allocation(altered), /allocation/);
});
test('common-Git one-shot claim survives staging crash and excludes alternate worktrees/directories', t => {
  const root = owned(t), oldClaim = path.join(root, 'common-grant.json'), file = runner.claimFile(oldClaim);
  const first = { budget: budget(), directory: path.join(root, 'never-staged') }; runner.claimEnvelope(file, first, 'a'.repeat(64));
  assert(!fs.existsSync(first.directory));
  assert.equal(file, runner.claimFile(path.join(root, 'another-checkout-claim-name.json')));
  assert.throws(() => runner.claimEnvelope(file, { ...first, directory: path.join(root, 'different-worktree-stage') }, 'b'.repeat(64)), /EEXIST/);
  assert.equal(JSON.parse(fs.readFileSync(file)).directory, first.directory);
});
test('shared authorization accounts predecessor exactly once; no replay, DOC transfer, duplicate task or unknown liability', () => {
  const slots = runner.allocation(frozen.slots()), next = slots[0];
  assert.equal(runner.admit(budget(), slots, [], next).current_authorization_settled_micros, 644378);
  const done = { id: next.id, task_id: 'fresh', status: 'failed', actual_cost_micros: 1750000, observed_attempts: 16, active_micros: 0, unresolved_micros: 0 };
  assert.equal(runner.admit(budget(), slots, [done], slots[1]).successor_requests, 16);
  for (const change of [{ id: rec.consumed }, { id: 'document-authoring--normal--00--candidate' }, { task_id: 'old-skl' }, { unresolved_micros: 1 }, { actual_cost_micros: 1750001 }, { observed_attempts: 17 }]) assert.throws(() => runner.admit(budget(), slots, [{ ...done, ...change }], slots[1]));
  assert.throws(() => runner.admit(budget(), slots, [done, { ...done, id: slots[1].id }], slots[2]), /Duplicate/);
  assert.throws(() => runner.admit(budget(), slots, [done], next), /reserve/);
  const completed = slots.map((r, i) => ({ ...done, id: r.id, task_id: 'fresh-' + i })); assert.throws(() => runner.admit(budget(), slots, completed, next));
  assert.throws(() => runner.admit({ ...budget(), settled_micros: 99000000 }, slots, [], next), /reserve/);
});
function comparison() {
  const original = frozen.slots().filter(r => r.candidate === 'skill-authoring' && r.phase === 'normal').map(r => ({ ...r, scaffold_paths: [], profile: { canonical_tools: ['vcp_read'], maximum_autonomy: 'plan' } }));
  const phaseHash = 'a'.repeat(64), envelopeHash = 'b'.repeat(64), resultHash = 'c'.repeat(64);
  const plan = { phase: 'normal', directory: path.resolve('synthetic-phase'), envelope_sha256: envelopeHash, qualification_prerequisites_pass: true, runs: original.slice(1) };
  const report = row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'completed', actual_cost_micros: 1, observed_attempts: 1, preserved: true, tool_audit: { passed: true }, canary_disclosed: false, skill_evidence: {}, oracle: { structural_pass: true } });
  const predecessor = { row: original[0], report: { ...report(original[0]), status: 'failed', actual_cost_micros: null, reason: 'Canonical inspection unavailable' }, manifest: { supplemental: { status: 'failed', actual_cost_micros: 30538, observed_attempts: 12 } }, capture_directory: path.resolve('synthetic-capture'), original_base: path.resolve('synthetic-original') };
  const current = { schema: 'cs1-skl-continuation-result/1', stopped: false, final_inputs_unchanged: true, runs: original.slice(1).map(report) };
  const result = runner.composeProjection({}, plan, current, phaseHash, resultHash, predecessor);
  const logical = { ...plan, runs: result.planned_rows }, caseIds = [...new Set(original.map(r => r.case_id))];
  const owner = { schema: 'cs1-followup-owner-review/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, result_sha256: resultHash, owner_reviewed: true, owner: 'Synthetic owner', integrity_pass: true, reviews: [{}, {}], native_checks: caseIds.map(case_id => ({ case_id, status: 'not_applicable', evidence: [] })) };
  const gates = Object.fromEntries(frozen.hardGates.map(k => [k, true]));
  const reviews = ['one', 'two'].map(reviewer_id => ({ schema: 'cs1-followup-review-projection/1', reviewer_id, independent_blinded: true, phase_sha256: phaseHash, result_sha256: resultHash, source_review: { path: path.resolve('reader-' + reviewer_id + '.json'), sha256: 'd'.repeat(64) }, label_mappings: caseIds.map(case_id => ({ case_id, none: 'x', nearest: 'y', candidate: 'z' })), cases: caseIds.map(case_id => ({ case_id, arms: ['none', 'nearest', 'candidate'].map(arm => ({ arm, scores: { completeness: 2, clarity: 2, usefulness: arm === 'candidate' ? 2 : 1 }, hard_gates: { ...gates }, findings: 'Synthetic pure gate comparison' })) })) }));
  return { predecessor, current, plan: logical, result, owner, reviews };
}
test('mixed-source projection never upgrades old failed nearest and preserves all original bytes/objects', () => {
  const f = comparison(), before = JSON.stringify(f.predecessor);
  assert.equal(f.result.schema, 'cs1-skl-mixed-source-projection/1'); assert.equal(f.result.runs.length, 6);
  assert.equal(f.result.runs[0].status, 'failed'); assert.equal(f.result.runs[0].actual_cost_micros, 30538);
  assert.equal(f.result.origins[0].kind, 'failed_predecessor'); assert.equal(f.result.origins[0].result_sha256, rec.PIN.result);
  assert.equal(f.predecessor.report.actual_cost_micros, null); assert.equal(JSON.stringify(f.predecessor), before);
  assert.equal(f.result.actual_cost_micros, 30543);
});
test('new projection rule exactly reproduces old pure gate decisions without old completed-phase fabrication', () => {
  const f = comparison();
  const compare = () => assert.deepEqual(review.reviewDecision(f.plan, f.result, f.owner, f.reviews), frozen.reviewDecision(f.plan, { ...f.result, schema: 'cs1-fresh-qualification-result/1' }, f.owner, f.reviews));
  compare(); assert.equal(review.reviewDecision(f.plan, f.result, f.owner, f.reviews).winning_case_ids.length, 2);
  f.reviews[1].cases[0].arms.find(a => a.arm === 'candidate').scores.usefulness = 1; compare();
  f.result.runs.find(r => r.arm === 'candidate').status = 'failed'; compare();
  f.reviews[0].cases[1].arms[0].hard_gates.authority = false; assert.throws(() => review.reviewDecision(f.plan, f.result, f.owner, f.reviews), e => e.code === 'CS1_REVIEW_INTEGRITY');
  assert.throws(() => review.reviewDecision(f.plan, { ...f.result, schema: 'cs1-fresh-qualification-result/1' }, f.owner, f.reviews), /binding/);
});
test('projection refuses integrity-stopped or final-input-drift results', () => {
  const f = comparison(); for (const change of [{ stopped: true }, { final_inputs_unchanged: false }]) assert.throws(() => runner.composeProjection({}, f.plan, { ...f.current, ...change }, 'a'.repeat(64), 'b'.repeat(64), f.predecessor), /complete/);
});
function canonical() {
  const scope = { workspace: 'workspace', session: 'session', task: 'task' };
  const page = (view, items = [], gaps = []) => ({ schema_version: 1, scope, view, items, gaps, next_cursor: null });
  const attempt = { id: 'attempt', collection: 'attempt', visibility: 'available', record: { id: 'attempt' } }, settlement = { id: 'settlement', collection: 'settlement', visibility: 'available', record: { attempt: 'attempt' } };
  const costs = [page('costs', [attempt, settlement])], evidence = Object.fromEntries(['routing', 'outputs', 'context', 'tools', 'verification'].map(v => [v, [page(v)]]));
  evidence.routing = [page('routing', [attempt, settlement], [rec.fixedRoutingGap])]; return { scope, costs, evidence, page };
}
test('canonical audit accepts only exact fixed-routing omission and authenticates scope/accounting/cross-view artifacts', () => {
  const f = canonical(); rec.canonicalViews(f.evidence, f.costs, {}, f.scope);
  for (const mutate of [f => f.evidence.routing[0].gaps[0].reason = 'other', f => f.evidence.routing[0].scope.task = 'foreign', f => f.evidence.routing[0].items.pop(), f => f.evidence.routing[0].items.push(f.evidence.routing[0].items[0]), f => f.evidence.outputs[0].view = 'tools']) { const c = clone(f); mutate(c); assert.throws(() => rec.canonicalViews(c.evidence, c.costs, {}, f.scope)); }
  const artifact = { id: 'response', collection: 'artifact', visibility: 'available', record: { spec: { id: 'response', scope: f.scope }, sha256: 'a'.repeat(64) } };
  f.evidence.outputs[0].items.push(artifact); f.evidence.routing[0].items.push(clone(artifact)); rec.canonicalViews(f.evidence, f.costs, {}, f.scope);
  f.evidence.routing[0].items.at(-1).record.sha256 = 'b'.repeat(64); assert.throws(() => rec.canonicalViews(f.evidence, f.costs, {}, f.scope), /descriptor|absent/);
});
test('response artifacts exactly and uniquely cover settled provider requests', () => {
  const item = id => ({ item: { record: { spec: { channel: 'response' } } }, bytes: Buffer.from(`data: ${JSON.stringify({ type: 'response.created', response: { id } })}\n\n`) });
  const attempts = ['one', 'two'].map(provider_request => ({ phase: 'settled', provider_request }));
  assert.deepEqual(rec.canonicalResponseLinks([item('one'), item('two')], attempts), { settled_requests: 2, response_artifacts: 2 });
  assert.throws(() => rec.canonicalResponseLinks([item('one')], attempts), /cover/);
  assert.throws(() => rec.canonicalResponseLinks([item('one'), item('one')], attempts), /cover/);
  assert.throws(() => rec.canonicalResponseLinks([{ ...item('one'), bytes: Buffer.from('data: {}\n\n') }, item('two')], attempts), /exactly one/);
});
test('raw canonical replay is bounded, binds views/ranges, rejects missing bytes and invokes no executable', t => {
  const root = owned(t), capture = path.join(root, 'capture'), source = path.join(root, 'source'); fs.mkdirSync(capture);
  const f = canonical(), plan = { executable: path.join(root, 'never-run.exe') }, bytes = Buffer.from('retained failed response');
  const item = { id: 'response', collection: 'artifact', visibility: 'available', record: { spec: { id: 'response', scope: f.scope, channel: 'response' }, state: 'complete', length: String(bytes.length), sha256: sha(bytes) } };
  f.evidence.outputs[0].items = [item];
  const receipt = { calls: [] }; let cumulative = 0;
  function raw(id, view, page, extra = []) {
    const args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(source, 'workspace'), '--data-dir', path.join(source, 'data'), 'inspect', id, '--view', view, '--limit', '128', ...extra];
    const result = { status: 0, error: null, stdout: JSON.stringify({ type: 'result', data: page }), stderr: '' }; cumulative += Buffer.byteLength(JSON.stringify(result));
    const metadata = { index: receipt.calls.length + 1, id, view, args, original_timeout_ms: 30000, timeout_ms: 120000, elapsed_ms: 1, status: 0, error: null, cumulative_output_bytes: cumulative }; receipt.calls.push(metadata);
    save(path.join(capture, `inspection-${String(metadata.index).padStart(3, '0')}.json`), { metadata, result });
  }
  for (const [view, pages] of Object.entries(f.evidence)) { raw('task', view, pages[0]); save(path.join(capture, view + '.json'), pages); }
  raw('response', 'outputs', f.page('outputs', [{ artifact: 'response', descriptor: item.record, visibility: 'available', range: { start: 0, end: bytes.length }, bytes: [...bytes] }]), ['--offset', '0', '--length', '65536']);
  const artifactFile = path.join(capture, 'artifact-' + sha(Buffer.from('response')) + '.bin'); save(artifactFile, bytes);
  const replay = rec.replayCapture(capture, source, plan, { profile: {} }, 'task', receipt, prior);
  assert.deepEqual(replay.artifact('outputs', 'response'), bytes); replay.finish();
  assert.throws(() => replay.call(plan.executable, ['run'], 30000), /read-only/);
  save(artifactFile, Buffer.from('changed')); assert.throws(() => replay.artifact('outputs', 'response'), /differs/);
  assert.throws(() => replay.artifact('outputs', 'missing'), /unavailable/);
  const initialClosure = rec.closure(capture); save(path.join(capture, 'extra.json'), {}); assert.notDeepEqual(rec.closure(capture), initialClosure);
  fs.mkdirSync(path.join(capture, 'unexpected-directory')); assert.throws(() => rec.closure(capture));
});
