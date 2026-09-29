// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const helper = require('./cs3-document-remediation-preflight.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-remediation-preflight-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const put = (name, value) => { const file = path.join(directory, name); fs.writeFileSync(file, Buffer.isBuffer(value) ? value : typeof value === 'string' ? value : JSON.stringify(value)); return { path: file, sha256: sha(fs.readFileSync(file)) }; };
  return { directory, put };
}
function evidence(t) {
  const f = fixture(t), bytes = Buffer.from('{"observed":"fixture"}'), id = 'captured-id';
  const item = { collection: 'artifact', id, record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec: { schema: 'test-only' } } };
  const report = { raw: {}, artifacts: [{ id, view: 'outputs', ref: f.put('artifact-' + sha(id) + '.bin', bytes) }] };
  for (const view of ['costs', 'routing', 'outputs', 'context', 'tools', 'verification']) report.raw[view] = f.put(view + '.json', [{ gaps: [], items: ['outputs', 'context'].includes(view) ? [item] : [] }]);
  report.raw['stdout.jsonl'] = f.put('stdout.jsonl', ''); report.raw['stderr.txt'] = f.put('stderr.txt', ''); report.raw['exit.json'] = f.put('exit.json', { status: 0, error: null });
  return { ...f, report, item, bytes };
}
test('retained preflight inventory joins duplicate view descriptors to exact local captured bytes', t => {
  const f = evidence(t), raw = helper.retained(f.report, f.directory);
  assert.equal(raw.artifacts.length, 1); assert.deepEqual(raw.artifacts[0].bytes, f.bytes); assert.equal(raw.evidence.context[0].items.length, 1);
});
test('preflight retained evidence rejects missing, extra, relocated and rehashed malformed inventories', t => {
  let f = evidence(t); delete f.report.raw.tools; assert.throws(() => helper.retained(f.report, f.directory), /inventory/);
  f = evidence(t); f.report.raw.extra = f.put('extra.json', {}); assert.throws(() => helper.retained(f.report, f.directory), /inventory/);
  f = evidence(t); f.report.raw.outputs = f.put('relocated.json', [{ gaps: [], items: [f.item] }]); assert.throws(() => helper.retained(f.report, f.directory), /escaped/);
  f = evidence(t); f.report.raw.tools = f.put('tools.json', []); assert.throws(() => helper.retained(f.report, f.directory), /Incomplete/);
  f = evidence(t); f.report.raw.tools = f.put('tools.json', [{ gaps: [{ reason: 'missing capture' }], items: [] }]); assert.throws(() => helper.retained(f.report, f.directory), /Incomplete/);
});
test('preflight capture validation rejects descriptor, coverage, provenance-view and file tampering', t => {
  let f = evidence(t); f.report.artifacts.push(f.report.artifacts[0]); assert.throws(() => helper.retained(f.report, f.directory), /coverage/);
  f = evidence(t); f.report.artifacts[0].view = 'routing'; assert.throws(() => helper.retained(f.report, f.directory), /identity/);
  f = evidence(t); f.report.artifacts[0].view = 'tools'; assert.throws(() => helper.retained(f.report, f.directory), /identity/);
  f = evidence(t); f.report.artifacts[0].ref = f.put('other.bin', f.bytes); assert.throws(() => helper.retained(f.report, f.directory), /identity/);
  f = evidence(t); f.put(path.basename(f.report.artifacts[0].ref.path), 'changed'); assert.throws(() => helper.retained(f.report, f.directory), /changed/);
  f = evidence(t); f.report.raw.context = f.put('context.json', [{ gaps: [], items: [{ ...f.item, visibility: 'unavailable' }] }]); assert.throws(() => helper.retained(f.report, f.directory), /Repeated/);
  f = evidence(t); f.report.raw.outputs = f.put('outputs.json', [{ gaps: [], items: [{ ...f.item, record: { ...f.item.record, state: 'aborted' } }] }]); f.report.raw.context = f.put('context.json', [{ gaps: [], items: [] }]); assert.throws(() => helper.retained(f.report, f.directory), /descriptor/);
  f = evidence(t); f.put('artifact-unbound.bin', f.bytes); assert.throws(() => helper.retained(f.report, f.directory), /Unbound/);
});
// Scope only the external allocation/qualification prerequisites and Git claims.
// The adapter, input guards, exclusive writes and invocation path remain real.
function synthetic(t, syntheticOriginal = false) {
  const f = fixture(t), state = { approved: true, source: { exact: 'synthetic-source' }, qualification: true, allocationChecks: 0 };
  const file = path.join(__dirname, 'cs3-document-remediation-preflight.cjs'), actual = createRequire(file), module = { exports: {} };
  const git = path.join(f.directory, 'git'); fs.mkdirSync(git);
  const local = name => {
    if (name === './cs3-skill-remediation.cjs') return { validateQualification: spec => {
      state.skillAllocationChecks = (state.skillAllocationChecks || 0) + 1;
      assert(spec.skill_remediation && !spec.remediation);
      if (!state.approved) throw Error('Skill allocation/history/recovery/package/build proof rejected');
      if (state.foreignQualification) throw Error('Funded qualification claim rejected');
      if (!state.qualification) throw Error('Skill qualification rejected');
    } };
    if (name === 'test:original-failure') return state.original;
    if (name === './cs3-preflight-supplement.cjs') return { validate: () => { assert(state.supplement, 'Supplement unavailable'); return state.supplement; } };
    if (name === 'node:child_process') return { ...actual(name), execFileSync: (command, args) => { assert.equal(command, 'git'); assert.deepEqual(args, ['rev-parse', '--git-common-dir']); return git; } };
    if (name === './cs3-document-remediation.cjs') return { validateAllocation: () => { state.allocationChecks++; if (!state.approved) throw Error('Allocation/terminal/build proof rejected'); } };
    if (name === './cs3-comparison.cjs') return { sourceIdentity: () => state.source };
    if (name === './cs3-read-preflight.cjs') return { ...actual(name), validateQualification: (_ref,spec) => { assert(spec.remediation && !spec.skill_remediation); if (!state.qualification) throw Error('Qualification rejected'); },
      validateSkillQualification: (_ref,spec) => { assert(spec.skill_remediation && !spec.remediation); if (!state.qualification) throw Error('Skill qualification rejected'); } };
    if (name === './builtin-live-runner.cjs') return { ...actual(name), fixedProfileReasons: () => [] };
    if (name === './p6-live-runner.cjs') { const real = actual(name); return { ...real, boundaries: { ...real.boundaries, privateDirectory: () => {}, noParentInstructions: () => {},
      inspection: (...args) => state.native ? structuredClone(state.native.evidence[args[3]]) : real.boundaries.inspection(...args) } }; }
    if (name === './developer-runner.cjs') { const real = actual(name); return { ...real, retained: (...args) => state.native ? state.native.artifacts.find(row => row.item.id === args[2].id).bytes : real.retained(...args) }; }
    return actual(name);
  };
  let source = fs.readFileSync(file, 'utf8');
  // Synthetic-only replacement of the two fixed historical hashes; no production
  // hook or live receipt bypass. Every archive/raw/claim/inventory check is real.
  if (syntheticOriginal) source = source.replace("'7750247906d72b676161760a04fc3e572d41469b2c8e514cffb1defc66ab1e88'", "require('test:original-failure').result_sha256")
    .replace("'819fda929dda9c2d658f80ee7c66fc7c659e8dea8f4076ef1bbebe1db3fb3b14'", "require('test:original-failure').plan_sha256");
  new Function('require', 'module', 'exports', '__dirname', source)(local, module, module.exports, __dirname);
  const catalog = f.put('catalog.json', {}), profile = { max_requests: 16, output_tokens: '2048', deadline_seconds: 600, provider_timeout_seconds: 120,
    max_transport_retries: 0, maximum_autonomy: 'plan', automatic_effects: [], provider: { raw_sha256: catalog.sha256, compatibility: { model: 'deepseek/deepseek-v3.2', endpoint: 'deepinfra/fp4' } } };
  const blank = f.put('proof.json', {}), node = fs.realpathSync(process.execPath);
  const spec = { executable: blank, build_receipt: blank, catalog, node: { path: node, sha256: sha(fs.readFileSync(node)) }, profile: f.put('profile-input.json', profile),
    remediation: { decision: blank, prior_terminal: blank, runtime_decision: blank, allocation: blank, qualification: blank } };
  const specFile = f.put('spec.json', spec).path, destination = path.join(f.directory, 'observation');
  return { ...f, state, spec, specFile, destination, profile, helper: module.exports };
}
test('prepare requires original allocation and qualification proofs before creating any observation', t => {
  for (const key of ['approved', 'qualification']) { const f = synthetic(t); f.state[key] = false; assert.throws(() => f.helper.prepare(f.specFile, f.destination), /rejected/i); assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile())); }
  const f = synthetic(t); f.profile.processes = [{ id: 'forbidden' }]; f.spec.profile = f.put('profile-input.json', f.profile); f.put('spec.json', f.spec);
  assert.throws(() => f.helper.prepare(f.specFile, f.destination), /read-only/); assert(!fs.existsSync(f.destination));
});

function skillFixture(t) {
  const f = synthetic(t), blank = f.spec.remediation.decision;
  delete f.spec.remediation;
  f.profile.provider.compatibility.endpoint = 'friendli'; f.spec.profile = f.put('profile-input.json', f.profile);
  f.spec.skill_remediation = { decision: blank, allocation: blank, history: blank, recovery_native: blank, package_acceptance: blank, qualification: blank };
  f.put('spec.json', f.spec); return f;
}

test('SKL selects its exact funded lineage and qualification before any observation or claim', t => {
  const f = skillFixture(t);
  assert.deepEqual(f.helper.specIdentity({ ...f.spec, skill_remediation: { ...f.spec.skill_remediation, runtime_preflight: { pending: true } }, gates: {} }), f.spec);
  for (const key of ['approved', 'qualification', 'foreignQualification']) {
    const rejected = skillFixture(t); rejected.state[key] = key === 'foreignQualification';
    assert.throws(() => rejected.helper.prepare(rejected.specFile, rejected.destination), /rejected/i);
    assert(!fs.existsSync(rejected.destination)); assert(!fs.existsSync(rejected.helper.claimFile(0, 'skill')));
  }
  const plan = f.helper.prepare(f.specFile, f.destination), stored = JSON.parse(fs.readFileSync(plan.path));
  assert.equal(stored.schema, 'cs3-skill-remediation-preflight-plan/1');
  assert.deepEqual(stored.spec, f.spec); assert.equal(f.state.skillAllocationChecks, 1); assert.equal(f.state.allocationChecks, 0);
  assert.equal(fs.readFileSync(path.join(f.destination, 'prompt.txt'), 'utf8'), require('./cs3-read-preflight.cjs').PROMPT);
  assert.equal(fs.readFileSync(path.join(f.destination, 'workspace/status.txt'), 'utf8'), require('./cs3-read-preflight.cjs').CONTENT);
});

test('SKL rejects DOC selectors, unknown namespaces, supplements and replacement ordinals', t => {
  const mutations = [spec => spec.remediation = {}, spec => spec.skill_remediation = null,
    spec => spec.skill_remediation.preflight_supplement = {}, spec => spec.skill_remediation.unfunded = {},
    spec => { spec.skl_remediation = spec.skill_remediation; delete spec.skill_remediation; }];
  for (const mutate of mutations) {
    const f = skillFixture(t); mutate(f.spec); f.put('spec.json', f.spec);
    assert.throws(() => f.helper.prepare(f.specFile, f.destination));
    assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile(0, 'skill')));
  }
  const f = skillFixture(t), predecessors = f.put('predecessors.json', []);
  assert.throws(() => f.helper.prepareReplacement(f.specFile, f.destination, 1, predecessors.path), /no replacement/);
  assert.throws(() => f.helper.claimFile(1, 'skill'), /no replacement/);
  assert.throws(() => f.helper.claimFile(0, 'custom'), /Fixed remediation claim mode/);
  assert(!fs.existsSync(f.destination));
});

test('fixed preflight profile cannot switch between legacy DeepInfra and fresh-SKL Friendli', t => {
  for (const [skill, endpoint] of [[false, 'friendli'], [true, 'deepinfra/fp4'], [true, 'foreign']]) {
    const f = skill ? skillFixture(t) : synthetic(t);
    f.profile.provider.compatibility.endpoint = endpoint; f.spec.profile = f.put('profile-input.json', f.profile); f.put('spec.json', f.spec);
    assert.throws(() => f.helper.prepare(f.specFile, f.destination), /Fixed read-only remediation profile/);
    assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile(0, skill ? 'skill' : 'document')));
  }
});

test('SKL cannot consume or overwrite DOC ownership and has its own one-use failed claim', t => {
  const f = skillFixture(t), docClaim = f.helper.claimFile();
  fs.writeFileSync(docClaim, 'historical DOC ownership'); const preserved = fs.readFileSync(docClaim);
  const plan = f.helper.prepare(f.specFile, f.destination); let calls = 0;
  const result = f.helper.run(plan.path, plan.sha256, () => { calls++; throw Error('Synthetic SKL native failure'); });
  assert.equal(result.status, 'failed'); assert.equal(result.actual_cost_micros, null);
  assert.equal(JSON.parse(fs.readFileSync(result.path)).schema, 'cs3-skill-remediation-preflight/1');
  const claim = JSON.parse(fs.readFileSync(f.helper.claimFile(0, 'skill')));
  assert.equal(claim.schema, 'cs3-skill-remediation-preflight-claim/1'); assert.deepEqual(claim.plan, plan);
  assert.deepEqual(claim.allocation, f.spec.skill_remediation.allocation); assert.equal(claim.cap_micros, 600000); assert.equal(claim.request_ceiling, 16);
  assert.deepEqual(fs.readFileSync(docClaim), preserved);
  assert.throws(() => f.helper.run(plan.path, plan.sha256, () => assert.fail('No replay')), /EEXIST/);
  assert.throws(() => f.helper.prepare(f.specFile, path.join(f.directory, 'second')), /unconsumed/);
  assert.equal(calls, 1);
});

test('SKL retains the unchanged behavioral oracle and rejects schema, claim and source substitutions', t => {
  const f = skillFixture(t); f.state.native = nativeFixture();
  const plan = f.helper.prepare(f.specFile, f.destination), result = f.helper.run(plan.path, plan.sha256, () => f.state.native);
  assert.equal(f.helper.validate(result, { ...f.spec, skill_remediation: { ...f.spec.skill_remediation, runtime_preflight: result } }).status, 'passed');
  const report = JSON.parse(fs.readFileSync(result.path));
  report.schema = 'cs3-document-remediation-preflight/1'; fs.writeFileSync(result.path, JSON.stringify(report));
  assert.throws(() => f.helper.validate({ path: result.path, sha256: sha(fs.readFileSync(result.path)) }, f.spec), /exact remediation/);
  report.schema = 'cs3-skill-remediation-preflight/1'; fs.writeFileSync(result.path, JSON.stringify(report));
  const ref = { path: result.path, sha256: sha(fs.readFileSync(result.path)) }, claimPath = f.helper.claimFile(0, 'skill'), claim = fs.readFileSync(claimPath);
  fs.writeFileSync(claimPath, JSON.stringify({ ...JSON.parse(claim), schema: 'cs3-document-remediation-preflight-claim/1' }));
  assert.throws(() => f.helper.validate(ref, f.spec), /ownership differs/);
  fs.writeFileSync(claimPath, claim); f.state.source = { changed: 'candidate source' };
  assert.throws(() => f.helper.validate(ref, f.spec), /source\/runtime/);
});
test('frozen source or workspace drift prevents invocation and durable paid claim', t => {
  let f = synthetic(t), ref = f.helper.prepare(f.specFile, f.destination); f.state.source = { exact: 'changed' };
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => assert.fail('No invocation')), /source\/runtime/); assert(!fs.existsSync(f.helper.claimFile()));
  f = synthetic(t); ref = f.helper.prepare(f.specFile, f.destination); fs.writeFileSync(path.join(f.destination, 'workspace/status.txt'), 'changed');
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => assert.fail('No invocation')), /workspace changed/); assert(!fs.existsSync(f.helper.claimFile()));
});

test('new preflight admits exactly the prospective time bound without extending other limits', t => {
  for (const mutation of [{ deadline_seconds: 180 }, { deadline_seconds: 601 }, { provider_timeout_seconds: 60 }, { provider_timeout_seconds: 121 }, { max_requests: 17 }, { output_tokens: '2049' }, { max_transport_retries: 1 }]) {
    const f = synthetic(t); f.spec.profile = f.put('profile-input.json', { ...f.profile, ...mutation }); f.put('spec.json', f.spec);
    assert.throws(() => f.helper.prepare(f.specFile, f.destination), /Fixed read-only remediation profile/);
    assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile()));
  }
});
test('a failed native preflight remains failed, claimed and unrepeatable with its complete cap reserved', t => {
  const f = synthetic(t), ref = f.helper.prepare(f.specFile, f.destination); let calls = 0;
  const result = f.helper.run(ref.path, ref.sha256, (_exe, args, timeout) => { calls++; assert.equal(timeout, 780000); assert(args.includes('--non-interactive')); assert(args.includes('0.600000')); assert.equal(JSON.parse(fs.readFileSync(args[args.indexOf('--config') + 1])).deadline_seconds, 600); throw Error('Synthetic native failure'); });
  assert.equal(result.status, 'failed'); assert.equal(result.actual_cost_micros, null); assert.equal(calls, 1);
  const claim = JSON.parse(fs.readFileSync(f.helper.claimFile())); assert.equal(claim.cap_micros, 600000); assert.equal(claim.request_ceiling, 16); assert.deepEqual(claim.allocation, f.spec.remediation.allocation);
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => calls++), /EEXIST/); assert.equal(calls, 1);
  assert.throws(() => f.helper.prepare(f.specFile, path.join(f.directory, 'replacement')), /unconsumed/);
  assert(f.state.allocationChecks >= 3);
});
test('campaign compatibility identity includes binary, build, allocation, controller and qualification references', () => {
  const spec = { executable: 1, build_receipt: 2, catalog: 3, node: 4, profile: 5, remediation: { decision: 6, prior_terminal: 7, allocation: 8, qualification: 9, runtime_preflight: 10, runtime_decision: 11, runtime_terminal: 12 }, gates: {}, web_evidence: [] };
  assert.deepEqual(helper.specIdentity(spec), { executable: 1, build_receipt: 2, catalog: 3, node: 4, profile: 5, remediation: { decision: 6, prior_terminal: 7, allocation: 8, qualification: 9, runtime_decision: 11 } });
});

// Synthetic native transport only; all captured requests, tool continuations,
// descriptors and final output are checked by the real historical raw oracle.
function nativeFixture() {
  const original = require('./cs3-read-preflight.cjs'), scope = { workspace: 'workspace', session: 'session', task: 'task' };
  const attempts = [0, 1, 2, 3].map(i => ({ id: 'attempt-' + i, scope, root: scope.task, phase: 'settled', uncertain: false, previous: null,
    role: 'main', charged: '1', provider_request: 'response-' + i, request_digest: '' }));
  const verification = { id: 'verification', scope, outputs: ['whole', 'range'], outstanding_issues: [], unresolved_effects: [] };
  const evidence = Object.fromEntries(['costs', 'routing', 'outputs', 'context', 'tools', 'verification'].map(view => [view, [{ gaps: [], items: [] }]]));
  evidence.costs[0].items = [{ collection: 'ledger', visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: '4', active: '0', unresolved: '0', overrun: false } },
    ...attempts.map(record => ({ collection: 'attempt', visibility: 'available', record })),
    ...attempts.map(a => ({ collection: 'settlement', visibility: 'available', record: { attempt: a.id, applied: true, observation: { final_usage: {} } } }))];
  evidence.verification[0].items = [{ collection: 'verification', record: verification }];
  const artifacts = [], add = (id, schema, value, channel = 'evidence') => {
    const bytes = Buffer.from(typeof value === 'string' ? value : JSON.stringify(value));
    const item = { collection: 'artifact', id, visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes),
      spec: { schema, channel, scope, source: channel === 'response' ? 'retained-codex-attempt:attempt-' + id.split('-').at(-1) : 'synthetic' } } };
    artifacts.push({ item, bytes }); evidence[channel === 'request_body' || schema === 'context-manifest/1' ? 'context' : 'outputs'][0].items.push(item);
  };
  const outputs = [
    [{ type: 'function_call', call_id: 'call-0', name: 'vcp_read', arguments: JSON.stringify(original.WHOLE) }],
    [{ type: 'function_call', call_id: 'call-1', name: 'vcp_read', arguments: JSON.stringify(original.RANGE) }],
    [{ type: 'function_call', call_id: 'call-2', name: 'vcp_verify', arguments: JSON.stringify({ citations: ['whole', 'range'] }) }],
    [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify(original.ANSWER) }] }],
  ];
  outputs.forEach((output, i) => add('response-' + i, 'responses', 'data: ' + JSON.stringify({ type: 'response.completed', response: { id: 'response-' + i,
    model: 'deepseek/deepseek-v3.2', status: 'completed', output } }) + '\n\n', 'response'));
  const whole = { text: original.CONTENT, complete: true }, range = { text: 'CS3_READ_MIDDLE_A25E\n', returned_range: { start_line: 2, end_line: 2 } };
  add('whole', 'vcp-tool-result-v1', whole); add('range', 'vcp-tool-result-v1', range);
  attempts.forEach((attempt, i) => {
    const input = [];
    for (let n = 0; n < i && n < 3; n++) {
      input.push({ ...outputs[n][0] }); const body = n === 0 ? { evidence: 'whole', result: whole } : n === 1 ? { evidence: 'range', result: range } : { verification };
      input.push({ type: 'function_call_output', call_id: 'call-' + n, output: JSON.stringify(body) });
    }
    const request = JSON.stringify({ model: 'deepseek/deepseek-v3.2', input }); attempt.request_digest = sha(request);
    add('request-' + i, 'responses-request/1', request, 'request_body'); add('context-' + i, 'context-manifest/1', { request_sha256: attempt.request_digest, included: [] });
  });
  return { evidence, artifacts, stdout: JSON.stringify({ type: 'accepted', scope }) + '\n' + JSON.stringify({ type: 'result', scope, conditions: { completed: true } }) + '\n', stderr: '', status: 0, error: null };
}
test('adapter retains and revalidates an actual-oracle positive synthetic four-request observation', t => {
  const f = synthetic(t); f.state.native = nativeFixture(); const plan = f.helper.prepare(f.specFile, f.destination);
  const result = f.helper.run(plan.path, plan.sha256, () => f.state.native);
  assert.equal(result.status, 'passed'); assert.equal(result.actual_cost_micros, 4);
  const observed = f.helper.validate(result, { ...f.spec, remediation: { ...f.spec.remediation, runtime_preflight: result }, gates: {}, web_evidence: [] });
  assert.equal(observed.status, 'passed'); assert.equal(observed.observed_attempts, 4); assert.equal(observed.reads, 2);
  assert.throws(() => f.helper.run(plan.path, plan.sha256, () => assert.fail('Cannot replay')), /EEXIST/);
  const report = JSON.parse(fs.readFileSync(result.path)); report.reads = 1; fs.writeFileSync(result.path, JSON.stringify(report));
  assert.throws(() => f.helper.validate({ path: result.path, sha256: sha(fs.readFileSync(result.path)) }, f.spec), /outcome differs/);
});

function failedNative() {
  const scope = { workspace: 'w', session: 's', task: 't' }, amount = { currency: 'USD', micros: '129576' };
  const error = Buffer.from(JSON.stringify({ error: { code: 429, metadata: { provider_name: 'DeepInfra', is_byok: false, provider_error_code: 'engine_overloaded', limit_source: 'upstream_provider_shared_pool' } } }));
  const attempt = { id: 'attempt', scope, root: 't', role: 'main', previous: null, reservation: 'reservation', phase: 'reconciliation_pending',
    charged: '0', quote: { amount }, uncertain: 'retained response did not complete', send_intent: 'sent', request_digest: 'a'.repeat(64) };
  const reservation = { id: 'reservation', attempt: 'attempt', scope, root: 't', role: 'main', phase: attempt.phase, amount, charged: '0', liability: '129576', protected_draw: '0', protected_returned: '0' };
  const costs = [{ gaps: [], items: [
    { collection: 'ledger', id: 'ledger', visibility: 'available', record: { scope, currency: 'USD', cap: '600000', active: '0', protected: '0', allocations: {}, settled: '0', unresolved: '129576', overrun: false } },
    { collection: 'attempt', id: 'attempt', visibility: 'available', record: attempt }, { collection: 'reservation', id: 'reservation', visibility: 'available', record: reservation },
  ] }];
  const facts = [
    ...['task', 'turn'].map(collection => ({ collection, id: collection, value: { scope, state: 'paused', reason: 'provider outcome requires accounting reconciliation' } })),
    { collection: 'effect', id: 'read', value: { id: 'read', scope, state: 'succeeded', exit_code: null, reason: 'broker observed bounded file results; no automatic replay' } },
    { collection: 'artifact', id: 'response', value: { state: 'aborted', length: String(error.length), sha256: sha(error), spec: { id: 'response', scope, source: 'retained-codex-attempt:attempt', channel: 'response' } } },
  ];
  const output = [{ type: 'accepted', scope }, { type: 'event', event: { event: { data: { facts } } } },
    { type: 'result', scope, exit_code: 7, conditions: { unresolved_effect: true, cancelled: false, budget_exhausted: false, required_input: false, incomplete: false, invalid_configuration: false, internal_failure: false, durably_paused: true, completed: false } }];
  return { evidence: { costs }, artifacts: [], output, errorBytes: error, stdout: output.map(row => JSON.stringify(row)).join('\n') + '\n', stderr: '', status: 7, error: null };
}
function replacementFixture(t) {
  const f = synthetic(t, true), sourceArchive = path.join(f.directory, 'source-archive');
  fs.mkdirSync(path.join(sourceArchive, 'scripts/evals'), { recursive: true });
  fs.copyFileSync(path.join(__dirname, 'cs3-read-preflight.cjs'), path.join(sourceArchive, 'scripts/evals/cs3-read-preflight.cjs'));
  const prep = require('./authoring-prepare.cjs'); f.state.source = prep.identity(sourceArchive, ['scripts']);
  f.state.native = failedNative(); const plan = f.helper.prepare(f.specFile, f.destination), failed = f.helper.run(plan.path, plan.sha256, () => f.state.native);
  assert.equal(failed.status, 'failed');
  f.state.original = { result_sha256: failed.sha256, plan_sha256: plan.sha256 };
  const predecessor = { result: { path: failed.path, sha256: failed.sha256 }, source_archive: sourceArchive,
    inventory: prep.identity(f.destination, ['.']), terminal_response: f.put('terminal-429.bin', f.state.native.errorBytes) };
  f.state.supplement = { slots: 3, slot_cap_micros: 600000, slot_requests: 16, additional_cap_micros: 1800000, original_failure_sha256: failed.sha256 };
  f.spec.remediation.preflight_supplement = f.put('supplement.json', {}); f.put('spec.json', f.spec);
  return { ...f, predecessor, sourceArchive, prep, predecessorsFile: f.put('predecessors.json', [predecessor]).path };
}
test('provider-only failed proof conservatively preserves liability and rejects effect, scope, response and accounting substitutions', () => {
  const profile = { maximum_autonomy: 'plan', automatic_effects: [], canonical_tools: ['vcp_read', 'vcp_verify'] };
  const verify = f => helper.failedObservation(f.output.map(x => JSON.stringify(x)).join('\n'), f.evidence.costs, { status: f.status, error: f.error }, profile, f.errorBytes);
  assert.deepEqual(verify(failedNative()), { status: 'conservative_failed_preflight_preserved', conservative_debit_micros: 600000, reserved_requests: 16, actual_cost_micros: null,
    known_settled_micros: 0, unresolved_micros: 129576, observed_attempts: 1, scope: { workspace: 'w', session: 's', task: 't' } });
  const mutations = [f => f.evidence.costs[0].items[0].record.active = '1', f => f.output[2].scope.task = 'foreign',
    f => f.output[1].event.event.data.facts.find(x => x.collection === 'effect').value.state = 'failed',
    f => f.output[1].event.event.data.facts.find(x => x.collection === 'turn').value.reason = 'owner lost',
    f => f.output[1].event.event.data.facts.find(x => x.collection === 'artifact').value.spec.scope = { task: 'foreign' },
    f => f.output[1].event.event.data.facts = f.output[1].event.event.data.facts.filter(x => x.collection !== 'artifact'),
    f => { const extra = structuredClone(f.output[1].event.event.data.facts.find(x => x.collection === 'artifact')); extra.id = extra.value.spec.id = 'extra'; extra.value.spec.source = 'retained-codex-attempt:foreign'; f.output[1].event.event.data.facts.push(extra); },
    f => f.errorBytes = Buffer.from('{}'), f => f.error = 'controller timed out', f => f.output[2].conditions.completed = true];
  for (const mutate of mutations) { const f = failedNative(); mutate(f); assert.throws(() => verify(f)); }
});
test('one-shot supplemental replacement preserves original ownership and runs the unchanged success oracle', t => {
  const f = replacementFixture(t), oldClaim = fs.readFileSync(f.helper.claimFile()), oldInventory = f.prep.identity(f.destination, ['.']);
  const observation = f.helper.validateFailedPredecessor(f.predecessor, f.spec);
  assert.equal(observation.conservative_debit_micros, 600000); assert.equal(observation.actual_cost_micros, null);
  const next = path.join(f.directory, 'replacement-1'), plan = f.helper.prepareReplacement(f.specFile, next, 1, f.predecessorsFile);
  assert(!fs.existsSync(f.helper.claimFile(1))); f.state.native = nativeFixture(); let calls = 0;
  const result = f.helper.runReplacement(plan.path, plan.sha256, (_exe, _args, timeout) => { calls++; assert.equal(timeout, 780000); return f.state.native; });
  assert.equal(result.status, 'passed'); assert.equal(f.helper.validateReplacementResult(result, f.spec).reads, 2); assert.equal(calls, 1);
  assert.deepEqual(fs.readFileSync(f.helper.claimFile()), oldClaim); assert.deepEqual(f.prep.identity(f.destination, ['.']), oldInventory);
  assert.throws(() => f.helper.runReplacement(plan.path, plan.sha256, () => assert.fail('No replay')), /EEXIST/);
  assert.throws(() => f.helper.prepare(f.specFile, path.join(f.directory, 'hidden-original')), /explicit replacement/);
  const passed = { result: { path: result.path, sha256: result.sha256 }, source_archive: f.sourceArchive, inventory: f.prep.identity(next, ['.']), terminal_response: f.predecessor.terminal_response };
  const chain = f.put('after-success.json', [f.predecessor, passed]);
  assert.throws(() => f.helper.prepareReplacement(f.specFile, path.join(f.directory, 'replacement-2'), 2, chain.path), /Failed preflight plan\/result/);
});
test('replacement admission rejects skipped, duplicate, changed, unallocated and extra ordinal histories without claims', t => {
  const f = replacementFixture(t);
  for (const [ordinal, predecessors] of [[0, []], [4, [f.predecessor]], [2, [f.predecessor]], [2, [f.predecessor, f.predecessor]]]) {
    const chain = f.put('bad-chain.json', predecessors); assert.throws(() => f.helper.prepareReplacement(f.specFile, path.join(f.directory, 'never'), ordinal, chain.path), /replacement chain/);
  }
  const changed = structuredClone(f.predecessor); changed.inventory.files.pop();
  assert.throws(() => f.helper.validateFailedPredecessor(changed, f.spec), /full inventory/);
  const relocated = structuredClone(f.spec); relocated.executable = f.put('relocated-executable', fs.readFileSync(f.spec.executable.path));
  assert.equal(f.helper.validateFailedPredecessor(f.predecessor, relocated).status, 'conservative_failed_preflight_preserved');
  relocated.executable = f.put('wrong-executable', 'different'); assert.throws(() => f.helper.validateFailedPredecessor(f.predecessor, relocated), /identities/);
  f.state.supplement.additional_cap_micros = 2400000; assert.throws(() => f.helper.prepareReplacement(f.specFile, path.join(f.directory, 'never'), 1, f.predecessorsFile), /supplement differs/);
  assert(!fs.existsSync(f.helper.claimFile(1))); assert(!fs.existsSync(path.join(f.directory, 'never')));
});
test('at most three separately claimed upstream failures preserve every predecessor and reserve each complete slot', t => {
  const f = replacementFixture(t), predecessors = [f.predecessor]; let calls = 0;
  for (let ordinal = 1; ordinal <= 3; ordinal++) {
    const chain = f.put('chain-' + ordinal + '.json', predecessors), base = path.join(f.directory, 'replacement-' + ordinal);
    const plan = f.helper.prepareReplacement(f.specFile, base, ordinal, chain.path);
    const result = f.helper.runReplacement(plan.path, plan.sha256, () => { calls++; return f.state.native; });
    assert.equal(result.status, 'failed'); assert.equal(result.actual_cost_micros, null);
    const claim = JSON.parse(fs.readFileSync(f.helper.claimFile(ordinal)));
    assert.equal(claim.cap_micros, 600000); assert.equal(claim.request_ceiling, 16); assert.equal(claim.replacement.ordinal, ordinal);
    assert.deepEqual(claim.replacement.predecessors, predecessors);
    const next = { result: { path: result.path, sha256: result.sha256 }, source_archive: f.sourceArchive,
      inventory: f.prep.identity(base, ['.']), terminal_response: f.predecessor.terminal_response };
    if (ordinal <= 2) assert.equal(f.helper.validateFailedPredecessor(next, f.spec, { ordinal, predecessors: [...predecessors] }).conservative_debit_micros, 600000);
    predecessors.push(next);
  }
  const chain = f.put('fourth.json', predecessors);
  assert.throws(() => f.helper.prepareReplacement(f.specFile, path.join(f.directory, 'fourth'), 4, chain.path), /replacement chain/);
  assert.equal(calls, 3); assert(!fs.existsSync(path.join(f.directory, 'fourth')));
});
