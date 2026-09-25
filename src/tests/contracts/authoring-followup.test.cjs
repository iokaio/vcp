// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { authoringHost } = require('../support/authoring-host.cjs');
const { prep, followup } = authoringHost();
const oracle = require('../../../scripts/evals/authoring-followup-oracle.cjs');
const repository = path.resolve(__dirname, '../../..'), assets = path.join(repository, 'src/skills/builtin');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
const json = file => JSON.parse(fs.readFileSync(file));
function fixture(t, api = followup) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = owner.root, executable = path.join(root, 'vcp.exe'), profileFile = path.join(root, 'profile.json'), specFile = path.join(root, 'spec.json');
  fs.cpSync(assets, path.join(root, 'skills/builtin'), { recursive: true });
  fs.writeFileSync(executable, Buffer.concat([Buffer.from('synthetic; never executed'), fs.readFileSync(path.join(assets, 'catalog.json'))]));
  const catalog = path.join(root, 'provider-catalog.json'); save(catalog, {});
  const future = String(Date.now() + 3600000);
  const profile = { version: 1, trust_workspace: true, maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], workspace: 'rebound', provider: { observed_at: String(Date.now()), max_input: '1000', valid_until: future, max_output: '16384', price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) }, compatibility: { byte_ceiling_qualified: false, valid_until: future, responses_text_tools: true, provider_preferences_qualified: true } }, catalog, max_requests: 16, max_transport_retries: 0, output_tokens: '2048', deadline_seconds: 600, processes: [], checks: [], mcp: [], mcp_http: [] };
  const checker = path.join(root, 'source-checker.exe'), build_receipt = path.join(root, 'checker-build.json');
  fs.writeFileSync(checker, 'synthetic checker; never executed');
  const manifests = ['src/evals/skills/authoring/manifest.json', 'src/evals/skills/authoring-followup/manifest.json'];
  const source = 'src/crates/vcp-cli/src/bin/vcp-authoring-check.rs', builder = path.join(repository, 'scripts/evals/authoring-check-build.ps1');
  save(build_receipt, { schema: 'cs1-authoring-check-build/2', source, source_sha256: sha(fs.readFileSync(path.join(repository, source))), fixture_manifest: manifests[0], fixture_manifest_sha256: sha(fs.readFileSync(path.join(repository, manifests[0]))), fixture_manifests: manifests.map(p => ({ path: p, sha256: sha(fs.readFileSync(path.join(repository, p))) })), executable: checker, executable_sha256: sha(fs.readFileSync(checker)), cargo_command: prep.checkerCargoCommand, exit_code: 0, toolchain: { rustc: 'rustc synthetic-fixture' }, source_inputs: prep.identity(repository, followup.checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 })), source_inputs_unchanged: true, builder, builder_sha256: sha(fs.readFileSync(builder)) });
  const spec = { executable, profile: profileFile, aggregate_cap_usd: '162.000000', aggregate_call_ceiling: 864, runtime: { checker, build_receipt }, propose_opaque_checker_effects: true };
  const persist = () => { save(profileFile, profile); save(specFile, spec); }; persist();
  return { root, executable, specFile, profileFile, profile, spec, persist, prepare: () => api.prepare(specFile, path.join(root, 'proposal')) };
}
function phase(f) {
  const envelope = f.prepare(), prepared = followup.preparePhase(envelope.envelope, envelope.sha256, 'document-authoring', 'normal');
  return { ...envelope, prepared, plan: json(prepared.plan) };
}
function reviewFixture(plan, options = {}) {
  const cases = [...new Set(plan.runs.map(row => row.case_id))].sort();
  const phaseHash = 'a'.repeat(64), resultHash = 'b'.repeat(64), envelopeHash = 'c'.repeat(64);
  const result = { schema: 'cs1-followup-result/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, actual_cost_micros: plan.runs.length, observed_attempts: plan.runs.length, stopped: false, final_inputs_unchanged: true, runs: plan.runs.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'completed', actual_cost_micros: 1, observed_attempts: 1, preserved: true, skill_evidence: { synthetic: true }, oracle: { structural_pass: true }, scope: { task: 'synthetic-' + row.id }, workspace_sha256: 'd'.repeat(64) })) };
  const reviews = [0, 1].map(i => ({ schema: 'cs1-followup-review-projection/1', reviewer_id: 'independent-' + i, independent_blinded: true, phase_sha256: phaseHash, result_sha256: resultHash, source_review: { path: path.resolve('synthetic-raw-review-' + i), sha256: String(i).repeat(64) }, label_mappings: cases.map(case_id => ({ case_id, none: 'A', nearest: 'B', candidate: 'C' })), cases: cases.map(case_id => ({ case_id, arms: ['none', 'nearest', 'candidate'].map(arm => ({ arm, scores: { completeness: 3, clarity: 3, usefulness: arm === 'candidate' ? 3 : 2 }, hard_gates: Object.fromEntries(followup.hardGates.map(name => [name, true])), findings: 'Synthetic deterministic score fixture, not a model review.' })) })) }));
  const owner = { schema: 'cs1-followup-owner-review/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, result_sha256: resultHash, owner_reviewed: true, owner: 'synthetic-owner', integrity_pass: true, reviews: [{ path: path.resolve('a'), sha256: 'e'.repeat(64) }, { path: path.resolve('b'), sha256: 'f'.repeat(64) }], native_checks: cases.map(case_id => ({ case_id, status: plan.runs.find(r => r.case_id === case_id).scaffold_paths.length ? 'passed' : 'not_applicable', evidence: plan.runs.find(r => r.case_id === case_id).scaffold_paths.length ? [{ path: path.resolve('synthetic-native'), sha256: 'e'.repeat(64) }] : [] })) };
  return { plan, result, reviews, owner, ...options };
}
const decide = f => followup.reviewDecision(f.plan, f.result, f.owner, f.reviews);
const barePlan = (phase = 'normal') => ({ phase, runtime: { checker_sha256: 'e'.repeat(64) }, runs: ['DOC-a', 'DOC-b'].flatMap(case_id => ['none', 'nearest', 'candidate'].map(arm => ({ id: case_id + '--' + arm, case_id, arm, scaffold_paths: ['marker'] }))) });
function successfulNative(plan) {
  // Synthetic canonical transport with actual filesystem preservation. Artifact
  // oracles are separately covered; this fixture isolates phase/receipt gating.
  const catalog = json(path.join(assets, 'catalog.json')); let current, context, response;
  const artifact = (id, bytes, spec) => ({ id, collection: 'artifact', visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec } });
  return (_exe, args) => {
    if (args.includes('run')) {
      const workspace = args[args.indexOf('--workspace') + 1]; current = plan.runs.find(row => path.join(plan.directory, row.id, 'workspace') === workspace);
      const entry = current.skill ? catalog.skills.find(s => current.skill === `vcp-builtin::${s.id}::${s.id}`) : null;
      const included = entry ? [entry.body, ...(entry.resources || [])].map((part, index) => ({ kind: 'skill', trust: 'active_skill', source_hash: part.sha256, id: 'skill-' + sha(Buffer.from(current.skill)) + '-' + index })) : [];
      context = Buffer.from(JSON.stringify({ request_sha256: 'synthetic-request', included }));
      response = Buffer.from('data: ' + JSON.stringify({ type: 'response.completed', response: { id: 'synthetic-provider-request', status: 'completed', output: [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify({ files: [], report: 'Synthetic stage-control fixture', not_run: ['Real generation, native checker and quality review'] }) }] }] } }) + '\n\n');
      return { status: 0, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'task-' + current.id } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: true } }), stderr: '' };
    }
    const view = args[args.indexOf('--view') + 1], inspectId = args[args.indexOf('inspect') + 1]; let items = [];
    if (args.includes('--offset')) { const bytes = inspectId === 'response' ? response : context; items = [{ range: { start: 0, end: bytes.length }, bytes: [...bytes] }]; }
    else if (view === 'costs') items = [
      { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '1', overrun: false } },
      { collection: 'attempt', visibility: 'available', record: { id: 'attempt', phase: 'settled', role: 'main', charged: '1', request_digest: 'synthetic-request', provider_request: 'synthetic-provider-request' } },
      { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt', applied: true, observation: { final_usage: true } } },
    ];
    else if (view === 'outputs') items = [artifact('response', response, { channel: 'response' })];
    else if (view === 'context') items = [artifact('context', context, { schema: 'context-manifest/1' })];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
}
function retainReview(root, plan, phaseHash, result, envelopeHash, mutate = () => {}) {
  const f = reviewFixture(plan); f.result = result; f.owner.envelope_sha256 = envelopeHash; f.owner.phase_sha256 = phaseHash;
  f.owner.result_sha256 = sha(fs.readFileSync(path.join(plan.directory, 'result.json')));
  const directory = path.join(root, 'owner-' + plan.phase); fs.mkdirSync(directory);
  const ref = file => ({ path: file, sha256: sha(fs.readFileSync(file)) });
  for (const [index, review] of f.reviews.entries()) {
    review.phase_sha256 = phaseHash; review.result_sha256 = f.owner.result_sha256;
    const raw = path.join(directory, `blind-${index}.json`); save(raw, { reviewer: index, anonymous: true, note: 'Synthetic test original; no actual reviewer or paid call.' }); review.source_review = ref(raw);
  }
  mutate(f);
  f.owner.reviews = f.reviews.map((review, i) => { const file = path.join(directory, `projection-${i}.json`); save(file, review); return ref(file); });
  f.owner.native_checks.forEach((check, i) => {
    if (result.runs.find(r => r.case_id === check.case_id && r.arm === 'candidate').status !== 'completed') check.status = 'not_run';
    if (check.status !== 'passed') { check.evidence = []; return; }
    const row = result.runs.find(r => r.case_id === check.case_id && r.arm === 'candidate'), file = path.join(directory, `native-${i}.json`);
    save(file, { schema: 'cs1-followup-native-verification/1', phase_sha256: phaseHash, run_id: row.id, task_id: row.scope.task, checker_sha256: plan.runtime.checker_sha256, workspace_sha256: row.workspace_sha256, source_artifact: 'synthetic-canonical-export', owner_verified_canonical_source: true, recorded_output: { verification: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' } }] }, diagnostics: [{ exit_code: 0, stdout: { tail: 'ok 1 - authoring input preservation\nok 2 - authoring output structure\n' } }] } }); check.evidence = [ref(file)];
  });
  const file = path.join(directory, 'owner.json'); save(file, f.owner); return file;
}

test('fixed envelope reserves exactly 54 non-reallocatable slots with balanced prospective rotations', t => {
  const f = fixture(t), e = f.prepare(), envelope = json(e.envelope);
  assert.equal(e.model_calls, 0); assert.equal(e.runnable, false); assert.equal(envelope.authorization, false);
  assert.equal(envelope.slots.length, 54); assert.equal(new Set(envelope.slots.map(s => s.id)).size, 54);
  assert.equal(envelope.slots.reduce((n, s) => n + s.cap_micros, 0), 162000000);
  assert.equal(envelope.slots.reduce((n, s) => n + s.call_ceiling, 0), 864);
  assert(envelope.slots.every(s => s.output_tokens === '2048' && s.call_ceiling === 16 && s.cap_micros === 3000000));
  for (const candidate of ['document-authoring', 'skill-authoring']) {
    const rows = envelope.slots.filter(s => s.candidate === candidate);
    assert.deepEqual(['normal', 'inherited', 'confirmation'].map(p => rows.filter(s => s.phase === p).length), [6, 18, 3]);
    assert(rows.filter(s => s.phase === 'confirmation').every(s => s.case_id === null));
    for (const arm of ['none', 'nearest', 'candidate']) for (const position of [0, 1, 2]) assert.equal(rows.filter(s => s.arm === arm && s.position === position).length, 3);
  }
  assert.equal(fs.readdirSync(path.join(envelope.directory, 'phases')).length, 0);
  assert.throws(() => f.prepare(), /New private/);
});
test('aggregate and per-request cap deviations reject before taking ownership', t => {
  const f = fixture(t);
  for (const value of [863, 865, 864.5]) { f.spec.aggregate_call_ceiling = value; f.persist(); assert.throws(f.prepare, /exactly 54/); }
  f.spec.aggregate_call_ceiling = 864;
  for (const value of ['161.999999', '162.000001']) { f.spec.aggregate_cap_usd = value; f.persist(); assert.throws(f.prepare, /exactly 54/); }
  f.spec.aggregate_cap_usd = '162.000000';
  for (const value of ['2047', '2049']) { f.profile.output_tokens = value; f.persist(); assert.throws(f.prepare, /2048/); }
  f.profile.output_tokens = '2048'; f.profile.max_requests = 15; f.persist(); assert.throws(f.prepare, /16 requests/);
  assert(!fs.existsSync(path.join(f.root, 'proposal')));
});
test('only first candidate normal phase is initially preparable; exact parents, isolated data and map are staged once', t => {
  const f = fixture(t), p = phase(f), plan = p.plan;
  for (const [candidate, name] of [['document-authoring', 'inherited'], ['document-authoring', 'confirmation'], ['skill-authoring', 'normal']]) assert.throws(() => followup.preparePhase(p.envelope, p.sha256, candidate, name), /ENOENT/);
  assert.equal(plan.runs.length, 6); assert.equal(plan.aggregate_cap_micros, 18000000); assert.equal(plan.aggregate_call_ceiling, 96);
  assert.equal(json(plan.runtime.cases_file).cases.length, 6);
  for (const row of plan.runs) {
    const base = path.join(plan.directory, row.id);
    assert.equal(row.profile.max_requests, 16); assert.equal(row.profile.output_tokens, '2048'); assert.equal(row.profile.budget_usd, '3.000000');
    assert.equal(fs.readdirSync(path.join(base, 'data')).length, 0);
    assert(fs.readFileSync(path.join(base, 'workspace/checks/authoring.test.cjs'), 'utf8').startsWith('// Inert'));
    assert(!fs.existsSync(path.join(base, 'workspace/rubric.json')));
  }
  assert.throws(() => followup.preparePhase(p.envelope, p.sha256, 'document-authoring', 'normal'), /already claimed/);
  followup.validatePhase(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256);
});
test('phase cap mutation, duplicate row, alternate task and profile mutation cannot be authorized by rehashing', t => {
  const f = fixture(t), p = phase(f), originalBytes = fs.readFileSync(p.prepared.plan);
  for (const mutation of [v => v.runs[0].cap_micros++, v => v.runs[1] = structuredClone(v.runs[0]), v => v.runs[0].case_id = 'DOC-followup-migration-v2', v => v.runs[0].profile.automatic_effects = []]) {
    const changed = structuredClone(p.plan); mutation(changed); save(p.prepared.plan, changed);
    assert.throws(() => followup.run(p.envelope, p.sha256, p.prepared.plan, sha(fs.readFileSync(p.prepared.plan)), () => assert.fail('dispatch forbidden')), /derivation differs/);
  }
  fs.writeFileSync(p.prepared.plan, originalBytes);
  assert(!fs.existsSync(path.join(p.plan.directory, 'execution-claim.json')));
});
test('runtime/source-profile drift permanently halts prepared envelope before dispatch', t => {
  const f = fixture(t), p = phase(f), originalBytes = fs.readFileSync(f.profileFile);
  fs.appendFileSync(f.profileFile, ' ');
  assert.throws(() => followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail()), /identity/);
  fs.writeFileSync(f.profileFile, originalBytes);
  assert.throws(() => followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail()), /halted/);
});
test('unknown native outcome consumes one exclusive slot and blocks replay and all later phases', t => {
  const f = fixture(t), p = phase(f); let dispatch = 0;
  const result = followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => { dispatch++; return { status: null, error: 'synthetic-timeout', stdout: '', stderr: '' }; });
  assert.equal(dispatch, 1); assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, null);
  assert.equal(result.runs.filter(r => r.status === 'not_run').length, 5);
  assert.equal(fs.readdirSync(path.join(path.dirname(p.envelope), 'claims')).length, 1);
  assert.throws(() => followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail()), /halted/);
  assert.throws(() => followup.preparePhase(p.envelope, p.sha256, 'skill-authoring', 'normal'), /halted/);
});
test('fully accounted native candidate failure finishes its matched triplet, leaves remaining slots unused and cannot replay', t => {
  const f = fixture(t), p = phase(f); let dispatch = 0;
  const call = (_exe, args) => {
    if (args.includes('run')) { dispatch++; return { status: 1, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'synthetic-' + dispatch } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: false } }), stderr: '' }; }
    const view = args[args.indexOf('--view') + 1];
    const items = view === 'costs' ? [{ collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '0', overrun: false } }] : [];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
  const result = followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, call);
  assert.equal(dispatch, 3); assert.equal(result.stopped, false); assert.equal(result.candidate_stopped, true); assert.equal(result.actual_cost_micros, 0);
  assert.deepEqual(result.runs.map(r => r.status), ['failed', 'failed', 'failed', 'not_run', 'not_run', 'not_run']);
  const owner = retainReview(f.root, p.plan, p.prepared.sha256, result, p.sha256);
  const gate = followup.recordReview(p.envelope, p.sha256, 'document-authoring', 'normal', owner);
  assert.equal(gate.decision.terminal, true); assert.equal(gate.decision.candidate_gates_pass, false);
  assert.throws(() => followup.preparePhase(p.envelope, p.sha256, 'document-authoring', 'inherited'), /gate or prospective benefit failed/);
  assert.equal(followup.preparePhase(p.envelope, p.sha256, 'skill-authoring', 'normal').slots, 6);
  assert.throws(() => followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail()), /EEXIST/);
});
test('both reviewers must support same-case benefit independently with no score compensation or baseline-success prerequisite', () => {
  const f = reviewFixture(barePlan()); assert.deepEqual(decide(f).winning_case_ids, ['DOC-a', 'DOC-b']);
  f.result.runs.find(r => r.arm === 'none').status = 'failed'; assert.deepEqual(decide(f).winning_case_ids, ['DOC-a', 'DOC-b']);
  f.reviews[0].cases[0].arms.find(a => a.arm === 'candidate').scores.clarity = 2;
  f.reviews[1].cases[1].arms.find(a => a.arm === 'candidate').scores.usefulness = 2;
  assert.deepEqual(decide(f).winning_case_ids, []);
  assert.equal(decide(f).terminal, true);
});
test('hard gate failure, unexecuted baseline, missing native receipt, invalid score and duplicate reviewer cannot advance', () => {
  for (const mutation of [f => f.reviews[1].cases[0].arms[2].hard_gates.evidence_honesty = false, f => f.owner.native_checks[0].status = 'not_run', f => f.result.runs[2].status = 'failed']) {
    const f = reviewFixture(barePlan()); mutation(f); assert.equal(decide(f).candidate_gates_pass, false); assert.equal(decide(f).terminal, true);
  }
  const f = reviewFixture(barePlan()); f.result.runs[0].status = 'not_run'; f.result.actual_cost_micros--; f.result.observed_attempts--;
  assert.deepEqual(decide(f).winning_case_ids, ['DOC-b']);
  for (const mutation of [f => f.reviews[1].reviewer_id = f.reviews[0].reviewer_id, f => f.reviews[0].cases[0].arms[0].scores.usefulness = 4, f => f.reviews[0].cases[0].arms.push(f.reviews[0].cases[0].arms[0]), f => f.owner.integrity_pass = false, f => f.result.runs[0].actual_cost_micros = 3000001, f => f.result.runs[0].observed_attempts = 17, f => f.reviews[0].label_mappings[0].candidate = 'A']) {
    const changed = reviewFixture(barePlan()); mutation(changed); assert.throws(() => decide(changed));
  }
});
test('native verification requires pinned final-workspace receipt and concrete nonempty checks, never checks[]', () => {
  const f = reviewFixture(barePlan()), row = f.result.runs[2];
  const receipt = { schema: 'cs1-followup-native-verification/1', phase_sha256: f.owner.phase_sha256, run_id: row.id, task_id: row.scope.task, checker_sha256: f.plan.runtime.checker_sha256, workspace_sha256: row.workspace_sha256, source_artifact: 'synthetic-canonical-export', owner_verified_canonical_source: true, recorded_output: { verification: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' } }] }, diagnostics: [{ exit_code: 0, stdout: { tail: 'TAP version 13\n1..2\nok 1 - authoring input preservation\nok 2 - authoring output structure\n' } }] } };
  followup.nativeReceipt(f.plan, f.result, row.case_id, JSON.stringify(receipt), f.owner.phase_sha256);
  for (const mutation of [r => r.recorded_output.verification.checks = [], r => r.recorded_output.diagnostics[0].exit_code = 1, r => r.workspace_sha256 = 'f'.repeat(64), r => r.run_id = 'other', r => r.recorded_output.diagnostics[0].stdout.tail += 'not ok 2 - authoring output structure\n']) {
    const changed = structuredClone(receipt); mutation(changed); assert.throws(() => followup.nativeReceipt(f.plan, f.result, row.case_id, JSON.stringify(changed), f.owner.phase_sha256));
  }
});
test('reviewed authority or secret-handling failure in any arm permanently halts despite owner integrity pass', t => {
  const f = fixture(t), p = phase(f);
  const call = (_exe, args) => {
    if (args.includes('run')) return { status: 1, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'synthetic-accounted-failure' } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: false } }), stderr: '' };
    const view = args[args.indexOf('--view') + 1], items = view === 'costs' ? [{ collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '0', overrun: false } }] : [];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
  const result = followup.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, call);
  const owner = retainReview(f.root, p.plan, p.prepared.sha256, result, p.sha256, review => { review.reviews[1].cases[0].arms.find(a => a.arm === 'nearest').hard_gates.authority = false; });
  assert.equal(json(owner).integrity_pass, true);
  assert.throws(() => followup.recordReview(p.envelope, p.sha256, 'document-authoring', 'normal', owner), /global halt/);
  assert.throws(() => followup.preparePhase(p.envelope, p.sha256, 'skill-authoring', 'normal'), /halted/);
  const scores = reviewFixture(barePlan()); scores.reviews[0].cases[0].arms[0].hard_gates.secret_handling = false;
  assert.throws(() => decide(scores), /global halt/);
});
test('new JS oracle maps exact bytes and task bounds without pretending native or semantic checks ran', () => {
  for (const [id, name, max] of [['DOC-followup-handoff-v2', 'handoff.md', 7999], ['DOC-followup-migration-v2', 'migration-notice.md', 7999], ['SKL-followup-maintain-v2', 'package/references/planned-changes.md', 5999], ['SKL-followup-create-v2', 'package/SKILL.md', 6000]]) {
    const { oracle: spec, initial } = oracle.load(id), answer = { files: [...spec.allowed_outputs, ...spec.allowed_modifications].map(path => ({ path, content: initial.get(path) || 'synthetic' })), report: '', not_run: ['Native validation and semantic review are not run.'] };
    answer.files.find(f => f.path === name).content = 'x'.repeat(max);
    const final = new Map(initial); answer.files.forEach(f => final.set(f.path, f.content));
    const good = oracle.check(id, answer, { finalFiles: final }); assert.equal(good.structural_pass, true, good.errors.join(';')); assert.equal(good.observed_task_success, false); assert.equal(good.native_descriptor_validation, 'not_run_by_javascript');
    answer.files.find(f => f.path === name).content += 'x'; assert.equal(oracle.check(id, answer).structural_pass, false);
    answer.files.find(f => f.path === name).content = 'changed'; assert.equal(oracle.check(id, answer, { finalFiles: final }).structural_pass, false);
  }
  const id = 'SKL-followup-create-v2', spec = oracle.load(id).oracle;
  const answer = { files: spec.allowed_outputs.map(path => ({ path, content: path.endsWith('skill.json') ? 'x'.repeat(4000) : 'synthetic' })), report: '', not_run: [] };
  assert.equal(oracle.check(id, answer).structural_pass, true);
  answer.files[0].content += 'x'; assert.equal(oracle.check(id, answer).structural_pass, false);
});
test('retained review gates derive exactly one lexicographic confirmation triplet and reject changed reviews or selection', t => {
  const oracleFixture = { check: () => ({ structural_pass: true, observed_task_success: false, limitation: 'Synthetic structural mock, not task quality evidence.' }) };
  const api = authoringHost('win32', { './authoring-oracle.cjs': oracleFixture, './authoring-followup-oracle.cjs': oracleFixture }).followup;
  const f = fixture(t, api), e = f.prepare();
  const normal = api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'normal'), plan = json(normal.plan);
  const result = api.run(e.envelope, e.sha256, normal.plan, normal.sha256, successfulNative(plan));
  assert.equal(result.stopped, false); assert(result.runs.every(r => r.status === 'completed'));
  assert.throws(() => api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'inherited'), /ENOENT/);
  const owner = retainReview(f.root, plan, normal.sha256, result, e.sha256);
  const gate = api.recordReview(e.envelope, e.sha256, 'document-authoring', 'normal', owner);
  assert.deepEqual(gate.decision.winning_case_ids, ['DOC-followup-handoff-v2', 'DOC-followup-migration-v2']);
  assert.throws(() => api.recordReview(e.envelope, e.sha256, 'document-authoring', 'normal', owner), /EEXIST/);
  const inherited = api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'inherited'), inheritedPlan = json(inherited.plan);
  assert.equal(inheritedPlan.runs.length, 18); assert.equal(json(inheritedPlan.runtime.cases_file).cases.length, 15);
  const inheritedResult = api.run(e.envelope, e.sha256, inherited.plan, inherited.sha256, successfulNative(inheritedPlan));
  assert.equal(inheritedResult.stopped, false); assert(inheritedResult.runs.every(r => r.status === 'completed'));
  assert.throws(() => api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'confirmation'), /ENOENT/);
  api.recordReview(e.envelope, e.sha256, 'document-authoring', 'inherited', retainReview(f.root, inheritedPlan, inherited.sha256, inheritedResult, e.sha256));
  const confirmation = api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'confirmation'), confirmationPlan = json(confirmation.plan);
  assert.equal(confirmationPlan.runs.length, 3); assert.equal(confirmationPlan.selected_confirmation_case, 'DOC-followup-handoff-v2');
  assert(confirmationPlan.runs.every(row => row.case_id === 'DOC-followup-handoff-v2'));
  assert.equal(json(confirmationPlan.runtime.cases_file).cases.length, 3);
  assert.throws(() => api.preparePhase(e.envelope, e.sha256, 'document-authoring', 'confirmation'), /already claimed/);
  assert.throws(() => api.preparePhase(e.envelope, e.sha256, 'skill-authoring', 'normal'), /ENOENT/);
  const bytes = fs.readFileSync(confirmation.plan); confirmationPlan.selected_confirmation_case = 'DOC-followup-migration-v2'; save(confirmation.plan, confirmationPlan);
  assert.throws(() => api.run(e.envelope, e.sha256, confirmation.plan, sha(fs.readFileSync(confirmation.plan)), () => assert.fail()), /derivation/);
  fs.writeFileSync(confirmation.plan, bytes);
  const raw = path.join(plan.directory, 'review/blind-source-0.json'); fs.appendFileSync(raw, ' ');
  assert.throws(() => api.run(e.envelope, e.sha256, confirmation.plan, confirmation.sha256, () => assert.fail()), /evidence changed/);
  assert(fs.existsSync(path.join(path.dirname(e.envelope), 'halt.json')));
});
