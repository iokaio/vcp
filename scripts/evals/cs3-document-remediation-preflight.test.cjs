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
function synthetic(t) {
  const f = fixture(t), state = { approved: true, source: { exact: 'synthetic-source' }, qualification: true, allocationChecks: 0 };
  const file = path.join(__dirname, 'cs3-document-remediation-preflight.cjs'), actual = createRequire(file), module = { exports: {} };
  const git = path.join(f.directory, 'git'); fs.mkdirSync(git);
  const local = name => {
    if (name === 'node:child_process') return { ...actual(name), execFileSync: (command, args) => { assert.equal(command, 'git'); assert.deepEqual(args, ['rev-parse', '--git-common-dir']); return git; } };
    if (name === './cs3-document-remediation.cjs') return { validateAllocation: () => { state.allocationChecks++; if (!state.approved) throw Error('Allocation/terminal/build proof rejected'); } };
    if (name === './cs3-comparison.cjs') return { sourceIdentity: () => state.source };
    if (name === './cs3-read-preflight.cjs') return { ...actual(name), validateQualification: () => { if (!state.qualification) throw Error('Qualification rejected'); } };
    if (name === './builtin-live-runner.cjs') return { ...actual(name), fixedProfileReasons: () => [] };
    if (name === './p6-live-runner.cjs') { const real = actual(name); return { ...real, boundaries: { ...real.boundaries, privateDirectory: () => {}, noParentInstructions: () => {},
      inspection: (...args) => state.native ? structuredClone(state.native.evidence[args[3]]) : real.boundaries.inspection(...args) } }; }
    if (name === './developer-runner.cjs') { const real = actual(name); return { ...real, retained: (...args) => state.native ? state.native.artifacts.find(row => row.item.id === args[2].id).bytes : real.retained(...args) }; }
    return actual(name);
  };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(local, module, module.exports, __dirname);
  const catalog = f.put('catalog.json', {}), profile = { max_requests: 16, output_tokens: '2048', deadline_seconds: 180, provider_timeout_seconds: 60,
    max_transport_retries: 0, maximum_autonomy: 'plan', automatic_effects: [], provider: { raw_sha256: catalog.sha256, compatibility: { model: 'deepseek/deepseek-v3.2', endpoint: 'deepinfra/fp4' } } };
  const blank = f.put('proof.json', {}), node = fs.realpathSync(process.execPath);
  const spec = { executable: blank, build_receipt: blank, catalog, node: { path: node, sha256: sha(fs.readFileSync(node)) }, profile: f.put('profile-input.json', profile),
    remediation: { decision: blank, prior_terminal: blank, allocation: blank, qualification: blank } };
  const specFile = f.put('spec.json', spec).path, destination = path.join(f.directory, 'observation');
  return { ...f, state, spec, specFile, destination, profile, helper: module.exports };
}
test('prepare requires original allocation and qualification proofs before creating any observation', t => {
  for (const key of ['approved', 'qualification']) { const f = synthetic(t); f.state[key] = false; assert.throws(() => f.helper.prepare(f.specFile, f.destination), /rejected/i); assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile())); }
  const f = synthetic(t); f.profile.processes = [{ id: 'forbidden' }]; f.spec.profile = f.put('profile-input.json', f.profile); f.put('spec.json', f.spec);
  assert.throws(() => f.helper.prepare(f.specFile, f.destination), /read-only/); assert(!fs.existsSync(f.destination));
});
test('frozen source or workspace drift prevents invocation and durable paid claim', t => {
  let f = synthetic(t), ref = f.helper.prepare(f.specFile, f.destination); f.state.source = { exact: 'changed' };
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => assert.fail('No invocation')), /source\/runtime/); assert(!fs.existsSync(f.helper.claimFile()));
  f = synthetic(t); ref = f.helper.prepare(f.specFile, f.destination); fs.writeFileSync(path.join(f.destination, 'workspace/status.txt'), 'changed');
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => assert.fail('No invocation')), /workspace changed/); assert(!fs.existsSync(f.helper.claimFile()));
});
test('a failed native preflight remains failed, claimed and unrepeatable with its complete cap reserved', t => {
  const f = synthetic(t), ref = f.helper.prepare(f.specFile, f.destination); let calls = 0;
  const result = f.helper.run(ref.path, ref.sha256, (_exe, args, timeout) => { calls++; assert.equal(timeout, 360000); assert(args.includes('--non-interactive')); assert(args.includes('0.600000')); throw Error('Synthetic native failure'); });
  assert.equal(result.status, 'failed'); assert.equal(result.actual_cost_micros, null); assert.equal(calls, 1);
  const claim = JSON.parse(fs.readFileSync(f.helper.claimFile())); assert.equal(claim.cap_micros, 600000); assert.equal(claim.request_ceiling, 16); assert.deepEqual(claim.allocation, f.spec.remediation.allocation);
  assert.throws(() => f.helper.run(ref.path, ref.sha256, () => calls++), /EEXIST/); assert.equal(calls, 1);
  assert.throws(() => f.helper.prepare(f.specFile, path.join(f.directory, 'replacement')), /unconsumed/);
  assert(f.state.allocationChecks >= 3);
});
test('campaign compatibility identity includes binary, build, allocation, controller and qualification references', () => {
  const spec = { executable: 1, build_receipt: 2, catalog: 3, node: 4, profile: 5, remediation: { decision: 6, prior_terminal: 7, allocation: 8, qualification: 9, runtime_preflight: 10 }, gates: {}, web_evidence: [] };
  assert.deepEqual(helper.specIdentity(spec), { executable: 1, build_receipt: 2, catalog: 3, node: 4, profile: 5, remediation: { decision: 6, prior_terminal: 7, allocation: 8, qualification: 9 } });
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
