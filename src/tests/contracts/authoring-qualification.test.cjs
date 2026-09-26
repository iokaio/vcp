// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const { ownedRoot } = require('../support/experiments.cjs');
const repository = path.resolve(__dirname, '../../..'), assets = path.join(repository, 'src/skills/builtin');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
const json = file => JSON.parse(fs.readFileSync(file));
const budget = require('../../../scripts/evals/authoring-qualification-budget.cjs');
const qualification = require('../../../scripts/evals/authoring-qualification.cjs');
function host(root, successful = false) {
  const directory = path.join(repository, 'scripts/evals'), cache = new Map();
  const fakeBudget = { ...budget, claimFile: () => path.join(root, 'common-grant.json'), inspect: input => ({ grant: budget.grant, cap_micros: 100000000, active_micros: 0, unresolved_micros: 0, current: { reference: input.cs2, actual_cost_micros: 400000, task_ids: ['old-cs2'] }, history: { phases: [], actual_cost_micros: 401397, task_ids: ['old-cs1'] } }), document: ref => json(ref.file) };
  function load(name) {
    if (cache.has(name)) return cache.get(name).exports;
    const filename = path.join(directory, name), module = { exports: {} }; cache.set(name, module);
    const originalRequire = createRequire(filename), request = id => successful && ['./authoring-oracle.cjs', './authoring-followup-oracle.cjs', './authoring-fresh-doc-oracle.cjs'].includes(id) ? { check: () => ({ structural_pass: true, synthetic: 'Gate-plumbing fixture only; semantic fixture suites are separate.' }) } : id === './authoring-qualification-budget.cjs' ? fakeBudget : ['./authoring-qualification.cjs', './authoring-qualification-review.cjs', './authoring-prepare.cjs', './authoring-runner.cjs'].includes(id) ? load(id.slice(2)) : originalRequire(id);
    const hostProcess = new Proxy(process, { get: (target, property) => property === 'platform' ? 'win32' : property === 'env' ? { ...target.env, SystemRoot: target.env.SystemRoot || os.tmpdir() } : Reflect.get(target, property) });
    new Function('exports', 'require', 'module', '__filename', '__dirname', 'process', fs.readFileSync(filename, 'utf8'))(module.exports, request, module, filename, directory, hostProcess);
    return module.exports;
  }
  return { qualification: load('authoring-qualification.cjs'), prep: load('authoring-prepare.cjs'), review: load('authoring-qualification-review.cjs') };
}
function fixture(t, successful = false) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const { qualification, prep, review } = host(owner.root, successful), api = qualification;
  const root = owner.root, executable = path.join(root, 'vcp.exe'), profileFile = path.join(root, 'profile.json'), specFile = path.join(root, 'spec.json');
  fs.cpSync(assets, path.join(root, 'skills/builtin'), { recursive: true });
  fs.writeFileSync(executable, Buffer.concat([Buffer.from('synthetic; never executed'), fs.readFileSync(path.join(assets, 'catalog.json'))]));
  const catalog = path.join(root, 'provider-catalog.json'); save(catalog, {});
  const future = String(Date.now() + 86400000);
  const profile = { version: 1, trust_workspace: true, maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], workspace: 'rebound', provider: { observed_at: String(Date.now()), max_input: '1000', valid_until: future, max_output: '16384', price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) }, compatibility: { byte_ceiling_qualified: false, valid_until: future, responses_text_tools: true, provider_preferences_qualified: true } }, catalog, max_requests: 16, max_transport_retries: 0, output_tokens: '2048', deadline_seconds: 600, processes: [], checks: [], mcp: [], mcp_http: [] };
  const checker = path.join(root, 'source-checker.exe'), build_receipt = path.join(root, 'checker-build.json');
  fs.writeFileSync(checker, 'synthetic checker; never executed');
  const manifests = qualification.manifests;
  const source = 'src/crates/vcp-cli/src/bin/vcp-authoring-check.rs', builder = path.join(repository, 'scripts/evals/authoring-check-build.ps1');
  save(build_receipt, { schema: 'cs1-authoring-check-build/3', source, source_sha256: sha(fs.readFileSync(path.join(repository, source))), fixture_manifest: manifests[0], fixture_manifest_sha256: sha(fs.readFileSync(path.join(repository, manifests[0]))), fixture_manifests: manifests.map(p => ({ path: p, sha256: sha(fs.readFileSync(path.join(repository, p))) })), executable: checker, executable_sha256: sha(fs.readFileSync(checker)), cargo_command: prep.checkerCargoCommand, exit_code: 0, toolchain: { rustc: 'rustc synthetic-fixture' }, source_inputs: prep.identity(repository, qualification.checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 })), source_inputs_unchanged: true, builder, builder_sha256: sha(fs.readFileSync(builder)) });
  const previous = path.join(root, 'old-cs2'); fs.mkdirSync(previous);
  save(path.join(previous, 'plan.json'), { profile_source: profileFile, provider_catalog_sha256: sha(fs.readFileSync(catalog)) });
  const spec = { budget: { cs2: { plan: { file: path.join(previous, 'plan.json'), sha256: 'a'.repeat(64) } } }, executable, profile: profileFile, aggregate_cap_usd: '94.500000', aggregate_call_ceiling: 864, runtime: { checker, build_receipt }, propose_opaque_checker_effects: true };
  const persist = () => { save(profileFile, profile); save(specFile, spec); }; persist();
  return { root, qualification, review, executable, specFile, profileFile, profile, spec, persist, prepare: () => api.prepare(specFile, path.join(root, 'proposal')) };
}
function transport(plan, forbidden = false, successful = false) {
  // Synthetic canonical transport with actual filesystem preservation. Artifact
  // oracles are separately covered; this fixture isolates phase/receipt gating.
  const catalog = json(path.join(assets, 'catalog.json')); let current, context, response;
  const nativeStdout = Buffer.from('ok 1 - authoring input preservation\nok 2 - authoring output structure\n');
  const nativeOutcome = Buffer.from(JSON.stringify({ native_preparation: { executable: { sha256: plan.runtime.checker_sha256 } }, plan: { request: { arguments: ['--test', '--test-reporter=tap', '--test-concurrency=1', 'checks/authoring.test.cjs'] }, expected_tests: ['authoring input preservation', 'authoring output structure'], specification: 'package.json#test' }, artifacts: ['native-stdout'] }));
  const artifact = (id, bytes, spec) => ({ id, collection: 'artifact', visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec } });
  return (_exe, args) => {
    if (args.includes('run')) {
      const workspace = args[args.indexOf('--workspace') + 1]; current = plan.runs.find(row => path.join(plan.directory, row.id, 'workspace') === workspace);
      const entry = current.skill ? catalog.skills.find(s => current.skill === `vcp-builtin::${s.id}::${s.id}`) : null;
      const candidate = require('../../../scripts/evals/authoring-candidates.cjs').inspect().entries.find(s => s.qualified_id === current.skill);
      const included = (candidate ? candidate.parts : entry ? [entry.body, ...(entry.resources || [])] : []).map((part, index) => ({ kind: 'skill', trust: 'active_skill', source_hash: part.sha256, id: 'skill-' + sha(Buffer.from(current.skill)) + '-' + index }));
      context = Buffer.from(JSON.stringify({ request_sha256: 'synthetic-request', included }));
      response = Buffer.from((forbidden ? 'data: ' + JSON.stringify({ type: 'response.output_item.added', item: { type: 'function_call', name: 'vcp_exec', arguments: '{}' } }) + '\n\n' : '') + 'data: ' + JSON.stringify({ type: 'response.completed', response: { id: 'synthetic-provider-request', status: 'completed', output: [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify({ files: [], report: 'Synthetic stage-control fixture', not_run: ['Real generation, native checker and quality review'] }) }] }] } }) + '\n\n');
      return { status: 0, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'task-' + current.id } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: true } }), stderr: '' };
    }
    const view = args[args.indexOf('--view') + 1], inspectId = args[args.indexOf('inspect') + 1]; let items = [];
    if (args.includes('--offset')) { const bytes = inspectId === 'response' ? response : inspectId === 'native-outcome' ? nativeOutcome : inspectId === 'native-stdout' ? nativeStdout : context; items = [{ range: { start: 0, end: bytes.length }, bytes: [...bytes] }]; }
    else if (view === 'costs') items = [
      { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '1750000', active: '0', unresolved: '0', settled: '1', overrun: false } },
      { collection: 'attempt', visibility: 'available', record: { id: 'attempt', phase: 'settled', role: 'main', charged: '1', request_digest: 'synthetic-request', provider_request: 'synthetic-provider-request' } },
      { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt', applied: true, observation: { final_usage: true } } },
    ];
    else if (successful && current.scaffold_paths.length && view === 'verification') items = [{ collection: 'verification', record: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' }, exit_code: 0, output: 'native-outcome' }] } }];
    else if (successful && current.scaffold_paths.length && view === 'tools') items = [artifact('native-outcome', nativeOutcome, { channel: 'outcome' }), artifact('native-stdout', nativeStdout, { channel: 'stdout' })];
    else if (view === 'outputs') items = [artifact('response', response, { channel: 'response' })];
    else if (view === 'context') items = [artifact('context', context, { schema: 'context-manifest/1' })];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
}

function phase(f) { const envelope = f.prepare(), prepared = f.qualification.preparePhase(envelope.envelope, envelope.sha256, 'document-authoring', 'normal'); return { ...envelope, prepared, plan: json(prepared.plan) }; }
test('fixed mixed cohort reserves 54 conditional slots without reusing old DOC normals', () => {
  const tasks = qualification.tasks(), slots = qualification.slots(tasks);
  assert.equal(tasks.length, 16); assert.equal(slots.length, 54);
  assert.equal(slots.reduce((sum, row) => sum + row.cap_micros, 0), 94500000);
  assert.equal(slots.reduce((sum, row) => sum + row.call_ceiling, 0), 864);
  assert.deepEqual(tasks.filter(item => item.fresh).map(item => item.task.id).sort(), ['DOC-fresh-acceptance-plan-v1', 'DOC-fresh-format-reference-v1', 'SKL-followup-create-v3', 'SKL-followup-maintain-v3']);
  assert.equal(new Set(qualification.sourceScope).size, qualification.sourceScope.length);
  for (const candidate of ['document-authoring', 'skill-authoring']) assert.deepEqual(['normal', 'inherited', 'confirmation'].map(phase => slots.filter(row => row.candidate === candidate && row.phase === phase).length), [6, 18, 3]);
});
test('exact envelope/phase preparation is offline, shared-grant exclusive and source-bound', t => {
  const f = fixture(t), p = phase(f), envelope = json(p.envelope);
  assert.equal(p.model_calls, 0); assert.equal(envelope.execution_mode, 'conditional');
  assert.equal(p.plan.runs.length, 6); assert.equal(p.plan.aggregate_cap_micros, 10500000);
  assert(p.plan.runs.every(row => row.profile.canonical_tools.includes('vcp_verify') && !row.profile.canonical_tools.includes('vcp_exec')));
  assert.throws(() => f.qualification.prepare(f.specFile, path.join(f.root, 'parallel-envelope')), /EEXIST/);
  assert(!fs.existsSync(path.join(f.root, 'parallel-envelope')));
  assert.throws(() => f.qualification.preparePhase(p.envelope, p.sha256, 'document-authoring', 'inherited'), /ENOENT/);
  assert.throws(() => f.qualification.preparePhase(p.envelope, p.sha256, 'skill-authoring', 'normal'), /ENOENT/);
  f.qualification.validatePhase(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256);
});
test('old bootstrap/full diagnostic spec and changed reserve reject before ownership', t => {
  const f = fixture(t);
  f.spec.execution_mode = 'full_diagnostic'; f.persist(); assert.throws(f.prepare, /exactly 54/);
  delete f.spec.execution_mode; f.spec.aggregate_cap_usd = '94.500001'; f.persist(); assert.throws(f.prepare, /exactly 54/);
  assert(!fs.existsSync(path.join(f.root, 'common-grant.json')));
});
test('hash-authorized phase drift permanently halts before any dispatch', t => {
  const f = fixture(t), p = phase(f), row = p.plan.runs[0];
  fs.writeFileSync(path.join(p.plan.directory, row.id, 'prompt.txt'), 'tampered');
  assert.throws(() => f.qualification.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail('No provider dispatch')), /prompt\/profile changed/);
  assert(fs.existsSync(path.join(path.dirname(p.envelope), 'halt.json')));
});
test('failed rows retain all provider output and canonical tools/verification; stop after current triplet', t => {
  const f = fixture(t), p = phase(f), result = f.qualification.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, transport(p.plan));
  assert.equal(result.stopped, false, JSON.stringify(result)); assert.equal(result.candidate_stopped, true);
  assert.equal(result.actual_cost_micros, 3); assert.equal(result.observed_attempts, 3);
  assert.deepEqual(result.runs.map(row => row.status), ['failed', 'failed', 'failed', 'not_run', 'not_run', 'not_run']);
  for (const row of result.runs.slice(0, 3)) { const base = path.join(p.plan.directory, row.id); assert(fs.readdirSync(base).some(name => name.startsWith('response-'))); assert(fs.existsSync(path.join(base, 'tools.json'))); assert(fs.existsSync(path.join(base, 'verification.json'))); assert.equal(row.evidence_sha256, f.qualification.runEvidence(base)); assert.equal(row.tool_audit.passed, true); }
  const review = host(f.root).review, readerDirectory = path.join(f.root, 'readers');
  const packet = review.packets(p.envelope, p.sha256, 'document-authoring', 'normal', readerDirectory);
  const template = json(path.join(readerDirectory, 'review-template.json'));
  const bytes = [0, 1].map(index => Buffer.from(JSON.stringify({ ...template, reviewer_id: 'reader-' + index })));
  const resultHash = sha(fs.readFileSync(path.join(p.plan.directory, 'result.json')));
  const projections = bytes.map((raw, index) => review.project(p.plan.directory, p.prepared.sha256, resultHash, raw, { path: path.join(f.root, 'reader-' + index + '.json'), sha256: sha(raw) }));
  review.validate(p.plan.directory, p.prepared.sha256, resultHash, projections, bytes);
  const changed = structuredClone(projections); changed[0].cases[0].arms[0].scores.usefulness++;
  assert.throws(() => review.validate(p.plan.directory, p.prepared.sha256, resultHash, changed, bytes), /Projection altered/);
  assert.throws(() => review.packets(p.envelope, p.sha256, 'document-authoring', 'normal', path.join(f.root, 'remapped-readers')), /EEXIST/);
  assert.equal(packet.packets.sha256, sha(fs.readFileSync(packet.packets.file)));
  const mappingFile = path.join(p.plan.directory, 'private-mapping.json'), commitFile = path.join(p.plan.directory, 'blind-commitment.json');
  const mappingBytes = fs.readFileSync(mappingFile), commitBytes = fs.readFileSync(commitFile), mapping = JSON.parse(mappingBytes), commit = JSON.parse(commitBytes);
  assert.match(mapping.salt, /^[a-f0-9]{64}$/);
  assert(!fs.readFileSync(packet.packets.file, 'utf8').includes(mapping.salt));
  [mapping.mappings[0].none, mapping.mappings[0].candidate] = [mapping.mappings[0].candidate, mapping.mappings[0].none];
  save(mappingFile, mapping); commit.mapping_sha256 = sha(fs.readFileSync(mappingFile)); save(commitFile, commit);
  assert.throws(() => review.project(p.plan.directory, p.prepared.sha256, resultHash, bytes[0], projections[0].source_review), /packet does not bind/);
  fs.writeFileSync(mappingFile, mappingBytes); fs.writeFileSync(commitFile, commitBytes);
  assert.throws(() => f.qualification.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, () => assert.fail('No replay')), /Phase inputs changed|data|changed|EEXIST/);
});
test('out-of-ceiling tool calls halt before a second paid slot and retain offending stream', t => {
  const f = fixture(t), p = phase(f), result = f.qualification.run(p.envelope, p.sha256, p.prepared.plan, p.prepared.sha256, transport(p.plan, true));
  assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, 1); assert.equal(result.observed_attempts, 1);
  assert.match(result.runs[0].reason, /authority ceiling/);
  assert.equal(result.runs[0].evidence_sha256, f.qualification.runEvidence(path.join(p.plan.directory, result.runs[0].id)));
  assert(fs.existsSync(path.join(path.dirname(p.envelope), 'halt.json')));
});
test('incomplete and completed response tool calls both obey exact frozen ceiling', () => {
  const event = value => ({ bytes: Buffer.from('data: ' + JSON.stringify(value) + '\n\n') });
  assert.equal(qualification.auditTools([event({ item: { type: 'function_call', name: 'vcp_read' } })], ['vcp_read']).passed, true);
  assert.throws(() => qualification.auditTools([event({ response: { output: [{ type: 'function_call', name: 'vcp_exec' }] } })], ['vcp_read']), /authority ceiling/);
});

function reviewFixture(plan, options = {}) {
  const cases = [...new Set(plan.runs.map(row => row.case_id))].sort();
  const phaseHash = 'a'.repeat(64), resultHash = 'b'.repeat(64), envelopeHash = 'c'.repeat(64);
  const result = { schema: 'cs1-fresh-qualification-result/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, actual_cost_micros: plan.runs.length, observed_attempts: plan.runs.length, stopped: false, final_inputs_unchanged: true, runs: plan.runs.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'completed', actual_cost_micros: 1, observed_attempts: 1, tool_audit: { passed: true }, canary_disclosed: false, native_check: { status: 'passed' }, preserved: true, skill_evidence: { synthetic: true }, oracle: { structural_pass: true }, scope: { task: 'synthetic-' + row.id }, workspace_sha256: 'd'.repeat(64) })) };
  const reviews = [0, 1].map(i => ({ schema: 'cs1-followup-review-projection/1', reviewer_id: 'independent-' + i, independent_blinded: true, phase_sha256: phaseHash, result_sha256: resultHash, source_review: { path: path.resolve('synthetic-raw-review-' + i), sha256: String(i).repeat(64) }, label_mappings: cases.map(case_id => ({ case_id, none: 'A', nearest: 'B', candidate: 'C' })), cases: cases.map(case_id => ({ case_id, arms: ['none', 'nearest', 'candidate'].map(arm => ({ arm, scores: { completeness: 3, clarity: 3, usefulness: arm === 'candidate' ? 3 : 2 }, hard_gates: Object.fromEntries(qualification.hardGates.map(name => [name, true])), findings: 'Synthetic deterministic score fixture, not a model review.' })) })) }));
  const owner = { schema: 'cs1-followup-owner-review/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, result_sha256: resultHash, owner_reviewed: true, owner: 'synthetic-owner', integrity_pass: true, reviews: [{ path: path.resolve('a'), sha256: 'e'.repeat(64) }, { path: path.resolve('b'), sha256: 'f'.repeat(64) }], native_checks: cases.map(case_id => ({ case_id, status: plan.runs.find(r => r.case_id === case_id).scaffold_paths.length ? 'passed' : 'not_applicable', evidence: plan.runs.find(r => r.case_id === case_id).scaffold_paths.length ? [{ path: path.resolve('synthetic-native'), sha256: 'e'.repeat(64) }] : [] })) };
  return { plan, result, reviews, owner, ...options };
}
const decide = f => qualification.reviewDecision(f.plan, f.result, f.owner, f.reviews);
const barePlan = (phase = 'normal') => ({ phase, runtime: { checker_sha256: 'e'.repeat(64) }, runs: ['DOC-a', 'DOC-b'].flatMap(case_id => ['none', 'nearest', 'candidate'].map(arm => ({ id: case_id + '--' + arm, case_id, arm, scaffold_paths: ['marker'] }))) });

test('conditional benefit needs both readers and every candidate hard gate', () => {
  const f = reviewFixture(barePlan()); assert.equal(decide(f).candidate_gates_pass, true); assert.equal(decide(f).winning_case_ids.length, 2);
  f.reviews[1].cases[0].arms.find(arm => arm.arm === 'candidate').scores.usefulness = 2;
  assert.deepEqual(decide(f).winning_case_ids, ['DOC-b']);
  f.result.runs.find(row => row.arm === 'candidate').status = 'failed';
  assert.equal(decide(f).candidate_gates_pass, false); assert.equal(decide(f).terminal, true);
});
test('native/structural passes cannot override a reader authority failure or failed qualification prerequisites', () => {
  const f = reviewFixture(barePlan()); f.reviews[1].cases[0].arms[0].hard_gates.authority = false;
  assert.throws(() => decide(f), /authority or secret-handling/);
  const g = reviewFixture({ ...barePlan('confirmation'), qualification_prerequisites_pass: false });
  assert.equal(decide(g).qualifies, false);
});

test('blind packet serialization preserves frozen source identifiers and redacts output only', () => {
  const { encodePacket, redactOutput } = require('../../../scripts/evals/authoring-qualification-review.cjs');
  const literal = 'vcp-authoring-candidates::.::document-authoring';
  const packet = JSON.parse(encodePacket({ prompt: literal, sources: [{ content: literal }], oracle: { value: literal }, rubric: { value: literal }, variants: [redactOutput({ answer: { report: literal }, files: [{ content: literal }] })] }));
  assert.equal(packet.prompt, literal); assert.equal(packet.sources[0].content, literal); assert.equal(packet.oracle.value, literal); assert.equal(packet.rubric.value, literal);
  assert.equal(packet.variants[0].answer.report, '[selected skill]'); assert.equal(packet.variants[0].files[0].content, '[selected skill]');
});
test('reviewer identities are distinct after whitespace and case normalization', () => {
  const f = reviewFixture(barePlan()); f.reviews[0].reviewer_id = 'Reader A'; f.reviews[1].reviewer_id = ' reader a ';
  assert.throws(() => decide(f), /Two independent reviewers/);
  const review = require('../../../scripts/evals/authoring-qualification-review.cjs');
  assert.throws(() => review.validate('unused', 'unused', 'unused', f.reviews, [Buffer.from('{}'), Buffer.from('{}')]), /Two distinct/);
});

test('missing hostile output remains a case failure while incomplete stream canary still detects', t => {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup()); const base = owner.root;
  fs.mkdirSync(path.join(base, 'workspace'));
  const definition = { forbidden_output_literals: ['synthetic-forbidden-canary'] }, finalFiles = new Map();
  assert.equal(qualification.canaryDisclosed(base, definition, ['public-update.md'], finalFiles), false);
  const events = ['synthetic-forbidden-', 'canary'].map(delta => 'data: ' + JSON.stringify({ type: 'response.output_text.delta', response_id: 'synthetic', item_id: 'output', content_index: 0, delta }) + '\n\n').join('');
  fs.writeFileSync(path.join(base, 'response-' + 'a'.repeat(64) + '.sse'), events);
  assert.equal(qualification.canaryDisclosed(base, definition, ['public-update.md'], finalFiles), true);
});

function syntheticReview(f, envelope, prepared, plan, result) {
  const directory = path.join(f.root, 'review-' + plan.phase); fs.mkdirSync(directory);
  f.review.packets(envelope.envelope, envelope.sha256, plan.candidate, plan.phase, path.join(directory, 'readers'));
  const template = json(path.join(directory, 'readers/review-template.json')), mapping = json(path.join(plan.directory, 'private-mapping.json'));
  const readers = [0, 1].map(index => {
    const value = structuredClone(template); value.reviewer_id = 'synthetic-reader-' + index;
    for (const item of value.cases) for (const variant of item.variants) {
      const candidate = mapping.mappings.find(row => row.case_id === item.case_id).candidate === variant.label;
      variant.scores = { completeness: 3, clarity: 3, usefulness: candidate ? 3 : 2 };
      variant.hard_gates = Object.fromEntries(qualification.hardGates.map(gate => [gate, true]));
      variant.findings = 'Synthetic plumbing fixture; no real independent judgment or qualification is claimed.';
    }
    const file = path.join(directory, 'raw-' + index + '.json'); save(file, value); return file;
  });
  const projected = f.review.projections(envelope.envelope, envelope.sha256, plan.candidate, plan.phase, ...readers, path.join(directory, 'projections'));
  const nativeChecks = [...new Set(plan.runs.map(row => row.case_id))].map(caseId => {
    const row = plan.runs.find(row => row.case_id === caseId && row.arm === 'candidate'), report = result.runs.find(report => report.id === row.id);
    if (!row.scaffold_paths.length) return { case_id: caseId, status: 'not_applicable', evidence: [] };
    const file = path.join(directory, caseId + '-native.json');
    save(file, { schema: 'cs1-followup-native-verification/1', phase_sha256: prepared.sha256, run_id: row.id, task_id: report.scope.task, checker_sha256: plan.runtime.checker_sha256, workspace_sha256: report.workspace_sha256, source_artifact: 'native-outcome', owner_verified_canonical_source: true, recorded_output: { verification: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' } }] }, diagnostics: [{ exit_code: 0, stdout: { tail: 'ok 1 - authoring input preservation\nok 2 - authoring output structure\n' } }] } });
    return { case_id: caseId, status: 'passed', evidence: [{ path: file, sha256: sha(fs.readFileSync(file)) }] };
  });
  const ownerFile = path.join(directory, 'owner.json');
  save(ownerFile, { schema: 'cs1-followup-owner-review/1', envelope_sha256: envelope.sha256, phase_sha256: prepared.sha256, result_sha256: projected.result_sha256, owner_reviewed: true, owner: 'synthetic-owner', integrity_pass: true, reviews: projected.reviews, native_checks: nativeChecks });
  return f.qualification.recordReview(envelope.envelope, envelope.sha256, plan.candidate, plan.phase, ownerFile);
}
test('successful committed reviews advance normal to inherited to confirmation without resetting paid accounting', t => {
  const f = fixture(t, true), envelope = f.prepare(); let settled = 0, normalWinner;
  for (const phase of ['normal', 'inherited', 'confirmation']) {
    const prepared = f.qualification.preparePhase(envelope.envelope, envelope.sha256, 'document-authoring', phase), plan = json(prepared.plan);
    if (phase === 'confirmation') assert.equal(plan.selected_confirmation_case, normalWinner);
    const result = f.qualification.run(envelope.envelope, envelope.sha256, prepared.plan, prepared.sha256, transport(plan, false, true));
    assert.equal(result.stopped, false, JSON.stringify(result)); assert(result.runs.every(row => row.status === 'completed'));
    const admission = json(path.join(plan.directory, plan.runs[0].id, 'budget-admission.json'));
    assert.equal(admission.current_authorization_settled_micros, 400000 + settled); assert.equal(admission.authoring_settled_micros, settled);
    settled += plan.runs.length;
    if (phase === 'normal') assert.throws(() => f.qualification.preparePhase(envelope.envelope, envelope.sha256, 'document-authoring', 'inherited'), /ENOENT/);
    const gate = syntheticReview(f, envelope, prepared, plan, result);
    assert.equal(gate.decision.candidate_gates_pass, true);
    if (phase === 'normal') normalWinner = [...gate.decision.winning_case_ids].sort()[0];
    assert.equal(gate.decision.qualifies, phase === 'confirmation');
  }
  assert.equal(settled, 27);
  const next = f.qualification.preparePhase(envelope.envelope, envelope.sha256, 'skill-authoring', 'normal'); assert.equal(next.slots, 6);
  const normalGate = path.join(path.dirname(envelope.envelope), 'phases/document-authoring--normal/review-gate.json'), gate = json(normalGate); gate.decision.winning_case_ids = []; save(normalGate, gate);
  assert.throws(() => f.qualification.validatePhase(envelope.envelope, envelope.sha256, next.plan, next.sha256), /eligibility changed|gate or prospective benefit|derivation changed/);
  assert(fs.existsSync(path.join(path.dirname(envelope.envelope), 'halt.json')));
});
