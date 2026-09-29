// SPDX-License-Identifier: Apache-2.0
'use strict';
// Scoped synthetic CLI/inspector. No executable here is run and no provider or
// credential is accessed. Git claims and all output belong to a fresh temp root.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const candidates = require('./cs3-comparison-candidates.cjs');
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(fs.readFileSync(file));
function syntheticHost(gitDirectory, directory = __dirname) {
  const modules = new Map();
  function load(name) {
    if (modules.has(name)) return modules.get(name).exports;
    const filename = path.join(directory, name), module = { exports: {} }, actualRequire = createRequire(filename);
    modules.set(name, module);
    const localRequire = requested => {
      if (requested === 'node:child_process') {
        const actual = actualRequire(requested);
        return { ...actual, execFileSync: (command, args) => {
          assert.equal(command, 'git'); assert.deepEqual(args, ['rev-parse', '--git-common-dir']); return gitDirectory + '\n';
        } };
      }
      if (requested === './webapp-execution.cjs') return { ...actualRequire(requested), validateUiArtifact() { throw Error('Synthetic host never grades UI'); } };
      // These tests exercise canonical orchestration, not native qualification.
      // Production always imports the real, schema-specific prerequisite module.
      if (requested === './cs3-comparison-gates.cjs') return { validate() { return { synthetic_test_only: true }; } };
      if (requested === './cs3-comparison-policy.cjs') return { ...actualRequire(requested), validateSpec() { return { synthetic_test_only: true, fixed_conservative_micros: 1813737, outer_cap_micros: 100000000 }; } };
      if (['./cs3-comparison.cjs', './cs3-comparison-review.cjs'].includes(requested)) return load(requested.slice(2));
      return actualRequire(requested);
    };
    new Function('exports', 'require', 'module', '__filename', '__dirname', 'process', fs.readFileSync(filename, 'utf8'))(module.exports, localRequire, module, filename, directory, process);
    return module.exports;
  }
  return { campaign: load('cs3-comparison.cjs'), review: load('cs3-comparison-review.cjs') };
}
function fixture(t, expiryOffset = 86400000, successor = false) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-host-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const gitDirectory = path.join(directory, 'git-common'); fs.mkdirSync(gitDirectory);
  // Run against an immutable local copy so another agent editing an unrelated
  // verifier during this test cannot masquerade as campaign source corruption.
  // Production source-identity validation itself remains fully enabled.
  const sourceRoot = path.join(directory, 'source');
  for (const relative of ['scripts/evals', 'scripts/skills', 'src/tests/support/windows', 'src/evals/skills/cs3-comparison', 'src/skills/builtin']) fs.cpSync(path.join(repository, relative), path.join(sourceRoot, relative), { recursive: true });
  const host = syntheticHost(gitDirectory, path.join(sourceRoot, 'scripts/evals'));
  const ref = (name, content) => { const file = path.join(directory, name), bytes = typeof content === 'string' || Buffer.isBuffer(content) ? content : JSON.stringify(content); fs.writeFileSync(file, bytes); return { path: file, sha256: sha(bytes) }; };
  fs.cpSync(path.join(repository, 'src/skills/builtin'), path.join(directory, 'skills/builtin'), { recursive: true });
  const executable = ref('vcp.exe', Buffer.concat([Buffer.from('synthetic never-executed CLI\n'), fs.readFileSync(path.join(directory, 'skills/builtin/catalog.json'))]));
  const catalog = ref('catalog.json', '{}'), node = ref('node.exe', 'synthetic never-executed node');
  const future = String(Date.now() + expiryOffset);
  const profile = ref('profile.json', { version: 1, trust_workspace: true, workspace: directory, maximum_autonomy: 'plan', automatic_effects: [], catalog: catalog.path,
    max_requests: 16, max_transport_retries: 0, output_tokens: '2048', deadline_seconds: 600,
    provider: { raw_sha256: catalog.sha256, observed_at: String(Date.now()), max_input: '1000', max_output: '2048', valid_until: future,
      compatibility: { byte_ceiling_qualified: false, responses_text_tools: true, provider_preferences_qualified: true, valid_until: future, model: 'deepseek/deepseek-v3.2', endpoint: 'gmicloud/fp8' },
      price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) } } });
  const build_receipt = ref('build.json', { schema: 'cs3-comparison-build/1', status: 'passed', exit_code: 0,
    executable_sha256: executable.sha256, expected_executable_sha256: executable.sha256, executable: executable.path,
    source_inputs_unchanged: true, toolchain_unchanged: true, expected_executable_matches: true, qualification_build: true,
    production_release: false, provider_calls: 0, tests_executed: 0, builder_sha256: sha(fs.readFileSync(path.join(sourceRoot, 'scripts/evals/cs3-comparison-build.ps1'))),
    target_directory: directory, compiler_artifact: { reason: 'compiler-artifact', target: { name: 'vcp' }, profile: { test: false }, features: ['qualification'], executable: path.join(directory, 'debug/vcp.exe') } });
  const gates = Object.fromEntries(['browser_boundary', 'web_oracles', 'ui_qualification', 'node_fixture'].map(name => [name, ref(name + '.json', { status: 'passed', model_calls: 0 })]));
  const web_evidence = require('./fixtures/webapp/manifest.json').cases.map(task => ({ case_id: task.id, ...ref(task.id + '.json', { case_id: task.id, status: task.kind === 'missing' ? 'expected_not_run' : task.kind === 'near_miss' ? 'bug_detected' : 'passed' }) }));
  let specification = { executable, catalog, node, profile, build_receipt, gates, web_evidence };
  if (successor) {
    const predecessor = ref('predecessor.json', host.campaign.describe(specification, path.join(directory, 'old-campaign')));
    const next = json(profile.path); next.provider.compatibility.endpoint = 'deepinfra/fp4';
    specification = { ...specification, profile: ref('successor-profile.json', next), successor: { predecessor, synthetic_test_only: true } };
  }
  const spec = ref('spec.json', specification);
  const prepared = host.campaign.prepare(spec.path, path.join(directory, 'campaign')), plan = json(prepared.plan);
  return { directory, sourceRoot, host, prepared, plan, ref };
}
function fakeCli(plan, options = {}) {
  const rows = new Map(plan.runs.map(row => [row.id, row])), stores = new Map(), calls = [];
  const descriptor = (store, id, channel, bytes, schema) => {
    store.set(id, bytes);
    return { collection: 'artifact', id, visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec: { channel, ...(schema ? { schema } : {}) } } };
  };
  const cli = (_executable, args) => {
    const base = path.dirname(args[args.indexOf('--data-dir') + 1]), id = path.basename(base), row = rows.get(id);
    assert(row, 'CLI command must reference one frozen slot');
    if (args.includes('run')) {
      const selected = args.flatMap((arg, index) => arg === '--skill' ? [args[index + 1]] : []);
      assert.deepEqual(selected, row.skills);
      const profile = json(args[args.indexOf('--config') + 1]); assert.equal(profile.maximum_autonomy, 'plan'); assert.deepEqual(profile.automatic_effects, []);
      calls.push({ id, skills: selected, cap: args[args.indexOf('--budget-usd') + 1] });
      const store = new Map(), task = [...require('../../src/evals/skills/cs3-comparison/cohort.cjs').tasks, ...require('../../src/evals/skills/cs3-comparison/cohort-doc-successor.cjs').tasks].find(t => t.id === row.case_id);
      const answer = { files: Object.fromEntries(task.outputs.map(name => [name, '# Deliberately incomplete synthetic draft\n'])), report: 'Amber passed; violet was not run. Silver failed its required-field check. The absent runner remains unavailable.', not_run: ['Native runner absent.'] };
      let text = JSON.stringify(answer);
      if (options.malformedAt === calls.length) text = '{not valid JSON';
      let response = 'data: ' + JSON.stringify({ type: 'response.completed', response: { id: 'request-' + id, status: 'completed', output: [{ type: 'message', content: [{ type: 'output_text', text }] }] } }) + '\n\n';
      if (options.canaryAt === calls.length) {
        const canary = options.canary || 'CS3_DOC_PRIVATE_CANARY_82D4';
        response = ['data: {malformed earlier frame}', ...[canary.slice(0, 12), canary.slice(12)].map(delta => 'data: ' + JSON.stringify({ type: 'response.output_text.delta', response_id: 'request-' + id, item_id: 'answer', output_index: 0, content_index: 0, delta }))].join('\n\n') + '\n\n';
      }
      const pending = Boolean(plan.successor && options.unknownAt === calls.length);
      if (pending && options.canaryAt !== calls.length) response = '{"error":{"message":"synthetic provider failure without billing"}}';
      const output = descriptor(store, 'response-' + id, 'response', Buffer.from(response));
      const scope = { task: 'task-' + id, workspace: 'synthetic-workspace', session: 'synthetic-session' };
      if (plan.successor) {
        Object.assign(output.record.spec, { scope, source: 'retained-codex-attempt:attempt-' + id, schema: 'responses-sse-observed-through-terminal/1', omissions: ['authentication_headers', 'recovery_material', ...(pending ? ['explicit_abort'] : [])] });
        output.record.retained = [{ start: '0', end: output.record.length }];
        if (pending) output.record.state = 'aborted';
      }
      const builtin = json(path.join(path.dirname(plan.executable), 'skills/builtin/catalog.json'));
      let included = selected.flatMap(qualified => {
        const candidate = plan.candidate_assets.entries.find(entry => entry.qualified_id === qualified), built = builtin.skills.find(skill => qualified === `vcp-builtin::${skill.id}::${skill.id}`);
        return (candidate ? candidate.parts : [built.body, ...(built.resources || [])]).map((part, index) => ({ kind: 'skill', id: `skill-${sha(qualified)}-${index}`, source_hash: part.sha256, trust: 'active_skill' }));
      });
      if (options.contextAt === calls.length) included = included.length ? [] : [{ kind: 'skill', id: 'unexpected', source_hash: 'a'.repeat(64), trust: 'active_skill' }];
      const context = descriptor(store, 'context-' + id, 'context', Buffer.from(JSON.stringify({ request_sha256: 'digest-' + id, included })), 'context-manifest/1');
      let money = [
        { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '600000', settled: '37', active: '0', unresolved: options.unknownAt === calls.length ? '1' : '0', overrun: false } },
        { collection: 'attempt', visibility: 'available', record: { id: 'attempt-' + id, phase: 'settled', uncertain: false, previous: null, role: 'main', charged: '37', provider_request: 'request-' + id, request_digest: 'digest-' + id } },
        { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt-' + id, applied: true, observation: { final_usage: {} } } }
      ];
      if (plan.successor) {
        const phase = pending ? 'reconciliation_pending' : 'settled', charged = pending ? '0' : '37', liability = pending ? '100000' : '0', amount = { currency: 'USD', micros: '100000' };
        money = [
          { collection: 'ledger', id: scope.task, visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: charged, active: '0', unresolved: liability, overrun: false, allocations: {}, protected: '0' } },
          { collection: 'attempt', id: 'attempt-' + id, visibility: 'available', record: { id: 'attempt-' + id, scope, root: scope.task, phase, uncertain: pending ? 'retained response did not complete' : null, previous: null, role: 'main', charged,
            send_intent: 'sent-' + id, reservation: 'reservation-' + id, quote: { amount }, provider_request: pending ? null : 'request-' + id, request_digest: 'digest-' + id } },
          { collection: 'reservation', id: 'reservation-' + id, visibility: 'available', record: { id: 'reservation-' + id, attempt: 'attempt-' + id, scope, root: scope.task, role: 'main', phase, charged, amount, liability, protected_draw: '0', protected_returned: '0' } },
          ...pending ? [] : [{ collection: 'settlement', id: 'settlement-' + id, visibility: 'available', record: { scope, attempt: 'attempt-' + id, applied: true, total: charged, observation: { scope, attempt: 'attempt-' + id, provider_request: 'request-' + id, final_usage: true, amount: { currency: 'USD', micros: charged } } } }],
        ];
        if (options.activeAt === calls.length) money[0].record.active = '1';
      }
      stores.set(id, { store, output, context, money });
      if (options.haltAt === calls.length) fs.writeFileSync(path.join(plan.directory, 'halt.json'), JSON.stringify({ plan_sha256: options.planHash, reason: 'Synthetic concurrent integrity stop', action: 'Read-only reconciliation' }), { flag: 'wx' });
      if (options.mutateSourceAt === calls.length) fs.appendFileSync(options.sourceTarget, '\nSynthetic source drift\n');
      if (plan.successor) {
        const conditions = pending ? { unresolved_effect: true, cancelled: false, budget_exhausted: false, required_input: false, incomplete: false, invalid_configuration: false, internal_failure: false, durably_paused: true, completed: false } : { completed: true, unresolved_effect: false };
        const paused = pending ? ['task', 'turn'].map(collection => ({ type: 'event', event: { event: { data: { facts: [{ collection, value: { scope, state: 'paused', reason: 'provider outcome requires accounting reconciliation' } }] } } } })) : [];
        return { status: pending ? 7 : 0, error: null, stderr: '', stdout: [{ type: 'accepted', scope }, ...paused, { type: 'result', scope, exit_code: pending ? 7 : 0, conditions }].map(JSON.stringify).join('\n') + '\n' };
      }
      return { status: 0, error: null, stderr: '', stdout: JSON.stringify({ type: 'accepted', scope: { task: 'task-' + id } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: options.canaryAt !== calls.length } }) + '\n' };
    }
    const records = stores.get(id); assert(records);
    let items;
    if (args.includes('--offset')) {
      const artifact = args[args.indexOf('inspect') + 1], bytes = records.store.get(artifact); assert(bytes);
      const offset = Number(args[args.indexOf('--offset') + 1]), end = Math.min(offset + 65536, bytes.length);
      items = [{ artifact, visibility: 'available', range: { start: offset, end }, bytes: [...bytes.subarray(offset, end)], ...(records.output.id === artifact ? { descriptor: records.output.record } : {}) }];
    } else {
      const view = args[args.indexOf('--view') + 1];
      items = view === 'costs' ? records.money : view === 'outputs' ? [records.output] : view === 'context' ? [records.context] : [];
    }
    return { status: 0, error: null, stderr: '', stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }) + '\n' };
  };
  return { cli, calls };
}
test('synthetic canonical eighteen-slot block settles accounting, proves arms and consumes claims once', async t => {
  const f = fixture(t), fake = fakeCli(f.plan);
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, false); assert.equal(result.runs.length, 18); assert.equal(result.actual_cost_micros, 18 * 37); assert.equal(result.observed_attempts, 18);
  assert.equal(fake.calls.length, 18); assert(fake.calls.every(call => call.cap === '0.600000'));
  assert(result.runs.every(row => row.preserved && row.skill_evidence.checked_attempts === 1));
  assert(result.runs.some(row => row.status === 'failed'), 'Ordinary weak artifacts remain failed while fully accounted block completes');
  assert.equal(f.host.review.block(f.prepared.plan, f.prepared.sha256, 'document-authoring').result.actual_cost_micros, 666);
  const reviewDirectory = path.join(f.directory, 'blind-review'), packets = f.host.review.prepare(f.prepared.plan, f.prepared.sha256, 'document-authoring', reviewDirectory);
  const readerFiles = packets.packets.map(binding => {
    const packet = json(binding.path), file = path.join(f.directory, `synthetic-reader-${binding.reader}.json`);
    fs.writeFileSync(file, JSON.stringify({ reader: binding.reader, reviewer_id: 'synthetic-independent-' + binding.reader, packet_sha256: binding.packet_sha256, independent_blind: true,
      rows: packet.rows.map(row => ({ id: row.id, ...Object.fromEntries(f.host.review.gates.map(gate => [gate, gate === 'correctness' ? row.execution.status === 'completed' : true])), completeness: 1, clarity: 1, usefulness: 1, reason: 'Synthetic tied outputs with correctly retained failures.' })) }));
    return file;
  });
  const disposition = f.host.review.settle(f.prepared.plan, f.prepared.sha256, 'document-authoring', reviewDirectory, ...readerFiles);
  assert.equal(disposition.status, 'unqualified'); assert.equal(disposition.zero_unresolved_liability, true);
  assert.deepEqual(f.host.review.validateDisposition(f.prepared.plan, f.prepared.sha256, 'document-authoring'), disposition);
  const mappingsFile = path.join(reviewDirectory, 'private-mappings.json'), mappingBytes = fs.readFileSync(mappingsFile), mappings = JSON.parse(mappingBytes);
  [mappings.bindings[0].mapping[0].run_id, mappings.bindings[0].mapping[1].run_id] = [mappings.bindings[0].mapping[1].run_id, mappings.bindings[0].mapping[0].run_id];
  fs.writeFileSync(mappingsFile, JSON.stringify(mappings));
  assert.throws(() => f.host.review.validateDisposition(f.prepared.plan, f.prepared.sha256, 'document-authoring'), /mapping identity changed/);
  fs.writeFileSync(mappingsFile, mappingBytes);
  await assert.rejects(f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli), /EEXIST/);
  assert.equal(fake.calls.length, 18);
  const resultFile = path.join(f.plan.directory, 'result-document-authoring.json'), changed = json(resultFile); changed.runs[0].arm = 'candidate'; fs.writeFileSync(resultFile, JSON.stringify(changed));
  assert.throws(() => f.host.review.block(f.prepared.plan, f.prepared.sha256, 'document-authoring'), /changed|differs/);
});
test('unknown canonical charge halts before another paid slot and cannot replay', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { unknownAt: 1 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.equal(result.actual_cost_micros, null);
  assert.match(result.runs[0].reason, /unknown liability/);
  assert(fs.existsSync(path.join(f.plan.directory, 'claims', result.runs[0].id + '.json')));
  await assert.rejects(f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli), /halted/);
  assert.equal(fake.calls.length, 1);
});

test('v2 pending provider billing consumes a full slot debit, continues fresh slots and remains unqualified', async t => {
  const f = fixture(t, 86400000, true), fake = fakeCli(f.plan, { unknownAt: 3 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, false); assert.equal(fake.calls.length, 18); assert.equal(result.actual_cost_micros, null);
  assert.equal(result.conservative_debit_micros, 600000 + 17 * 37); assert.equal(result.known_settled_micros, 17 * 37); assert.equal(result.unresolved_attempts, 1);
  assert.equal(result.runs[2].status, 'failed'); assert.equal(result.runs[2].skill_evidence.checked_attempts, 1);
  const rebound = f.host.review.block(f.prepared.plan, f.prepared.sha256, 'document-authoring'); assert.equal(rebound.result.actual_cost_micros, null);
  const directory = path.join(f.directory, 'reviews'), packets = f.host.review.prepare(f.prepared.plan, f.prepared.sha256, 'document-authoring', directory);
  const reviews = packets.packets.map(binding => {
    const packet = json(binding.path);
    return f.ref('review-' + binding.reader + '.json', { reader: binding.reader, reviewer_id: 'independent-' + binding.reader, packet_sha256: binding.packet_sha256, independent_blind: true,
      rows: packet.rows.map(row => ({ id: row.id, ...Object.fromEntries(f.host.review.gates.map(gate => [gate, gate === 'correctness' ? row.execution.status === 'completed' : true])), completeness: 0, clarity: 0, usefulness: 0, reason: 'Retained failed outputs, not qualified.' })) }).path;
  });
  const disposition = f.host.review.settle(f.prepared.plan, f.prepared.sha256, 'document-authoring', directory, ...reviews);
  assert.equal(disposition.zero_unresolved_liability, false); assert.equal(disposition.actual_cost_micros, null); assert.equal(disposition.status, 'unqualified');
  assert.equal(f.host.review.validateDisposition(f.prepared.plan, f.prepared.sha256, 'document-authoring').conservative_debit_micros, result.conservative_debit_micros);
  await assert.rejects(f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli), /EEXIST/);
});

test('v2 active accounting or a canary in aborted response still halts immediately', async t => {
  for (const options of [{ unknownAt: 1, activeAt: 1 }, { unknownAt: 1, canaryAt: 1 }]) {
    const f = fixture(t, 86400000, true), fake = fakeCli(f.plan, options);
    const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
    assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.match(result.runs[0].reason, /Active|canary disclosed/);
  }
});

test('short qualification window denies block before durable claim or dispatch', async t => {
  const f = fixture(t, 3600000), fake = fakeCli(f.plan);
  await assert.rejects(f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli), /qualification window/);
  assert.equal(fake.calls.length, 0);
  assert.deepEqual(fs.readdirSync(path.join(f.plan.directory, 'claims')), []);
  assert.equal(fs.existsSync(path.join(f.plan.directory, 'active-block.json')), false);
  assert.equal(fs.existsSync(path.join(f.plan.directory, 'halt.json')), false);
});
test('wrong canonical skill context halts with settled cost preserved', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { contextAt: 1 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.equal(result.actual_cost_micros, 37); assert.match(result.runs[0].reason, /Skill context differs/);
});
test('malformed ordinary answer fails its slot without replay or unknown-liability fiction', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { malformedAt: 1 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, false); assert.equal(fake.calls.length, 18); assert.equal(result.runs[0].status, 'failed'); assert(result.runs[0].output_error); assert.equal(result.actual_cost_micros, 666);
});
test('incomplete malformed response still detects a canary split across deltas', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { canaryAt: 1 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.equal(result.actual_cost_micros, 37); assert.match(result.runs[0].reason, /canary disclosed/);
});
test('mid-block halt is preserved and prevents the next CLI dispatch', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { haltAt: 1, planHash: f.prepared.sha256 });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1);
  assert.equal(json(path.join(f.plan.directory, 'halt.json')).reason, 'Synthetic concurrent integrity stop');
});
test('isolated source snapshot still rejects verifier drift after a settled dispatch', async t => {
  const f = fixture(t), fake = fakeCli(f.plan, { mutateSourceAt: 1, sourceTarget: path.join(f.sourceRoot, 'src/evals/skills/cs3-comparison/README.md') });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.equal(result.actual_cost_micros, 37); assert.match(result.runs[0].reason, /source or candidate changed/);
});

test('external builtin asset drift halts before another dispatch with settled cost preserved', async t => {
  const f = fixture(t), asset = f.plan.assets.files.find(file => file.path.endsWith('/SKILL.md'));
  assert(asset, 'Frozen external bundle must contain a baseline skill body');
  const target = path.join(path.dirname(f.plan.executable), 'skills/builtin', asset.path);
  const fake = fakeCli(f.plan, { mutateSourceAt: 1, sourceTarget: target });
  const result = await f.host.campaign.run(f.prepared.plan, f.prepared.sha256, 'document-authoring', fake.cli);
  assert.equal(result.stopped, true); assert.equal(fake.calls.length, 1); assert.equal(result.actual_cost_micros, 37);
  assert.match(result.runs[0].reason, /Asset hash mismatch|Frozen external builtin assets changed/);
});
test('missing frontend output keeps canonical failure without fabricated native receipts', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-invalid-ui-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const host = syntheticHost(path.join(directory, 'unused-git'));
  const rows = ['filter-selection', 'disclosure-form'].flatMap(kind => ['none', 'nearest', 'candidate'].map(arm => ({ id: `UI-cs3-${kind}-v1--${arm}`, case_id: `UI-cs3-${kind}-v1`, status: 'failed', output_error: 'Malformed canonical final JSON' })));
  const grades = rows.map(row => {
    const base = path.join(directory, row.id); fs.mkdirSync(base);
    const bytes = JSON.stringify(row); fs.writeFileSync(path.join(base, 'result.json'), bytes);
    return { run_id: row.id, status: 'not_run_output_invalid', canonical_result_sha256: sha(bytes) };
  });
  const file = path.join(directory, 'browser-grades.json'), browser = { plan_sha256: 'a'.repeat(64), skill: 'frontend-design', runs: grades };
  fs.writeFileSync(file, JSON.stringify(browser));
  assert.equal(host.review.browserGrades({ directory }, 'a'.repeat(64), 'frontend-design', { runs: rows }, file).runs.length, 6);
  browser.runs[0].status = 'passed'; fs.writeFileSync(file, JSON.stringify(browser));
  assert.throws(() => host.review.browserGrades({ directory }, 'a'.repeat(64), 'frontend-design', { runs: rows }, file), /Missing UI output/);
  browser.runs[0].status = 'not_run_output_invalid'; browser.runs[0].receipt = { path: file, sha256: sha(fs.readFileSync(file)) }; fs.writeFileSync(file, JSON.stringify(browser));
  assert.throws(() => host.review.browserGrades({ directory }, 'a'.repeat(64), 'frontend-design', { runs: rows }, file), /without invented native receipt/);
});
