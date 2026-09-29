// SPDX-License-Identifier: Apache-2.0
'use strict';
// Deterministic host integration only: no binary, model, credential or native
// browser is used. Native/history/package admission is explicitly substituted
// in memory; real source/profile/claim/accounting/oracle/review code still runs.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const repository = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(fs.readFileSync(file));
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-transfer-integration-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const source = path.join(directory, 'source'), common = path.join(directory, 'git-common'); fs.mkdirSync(common);
  // Opaque candidate bytes are copied for their real digest/context joins. This
  // integration does not inspect authoring guidance or use it to create answers.
  for (const name of ['scripts/evals', 'scripts/skills', 'src/tests/support/windows', 'src/skills/builtin',
    'src/evals/skills/cs3-comparison', 'src/evals/skills/cs3-document-remediation', 'src/evals/skills/cs3-runtime-remediation',
    'src/evals/skills/cs3-friendli-transfer', 'src/evals/skills/cs3-controller-recovery', 'src/evals/skills/cs3-skill-remediation']) fs.cpSync(path.join(repository, name), path.join(source, name), { recursive: true });
  const state = { proof: null }, modules = new Map(), dir = path.join(source, 'scripts/evals');
  function load(name) {
    if (modules.has(name)) return modules.get(name).exports;
    const filename = path.join(dir, name), module = { exports: {} }, actual = createRequire(filename); modules.set(name, module);
    const local = request => {
      if (request === 'node:child_process') return { ...actual(request), execFileSync(command, args) {
        assert.equal(command, 'git', 'No executable may run in this test'); assert.deepEqual(args, ['rev-parse', '--git-common-dir']); return common + '\n';
      } };
      if (request === './cs3-comparison-gates.cjs') return { validate(spec) { assert.equal(spec.skill_remediation.synthetic_test_only, true); return { synthetic_native_admission_only: true }; } };
      if (request === './cs3-friendli-retirement.cjs') return { validate() { assert(state.proof); return state.proof; } };
      if (['./cs3-friendli-transfer.cjs', './cs3-comparison.cjs', './cs3-comparison-review.cjs', './cs3-document-remediation.cjs', './cs3-skill-remediation.cjs'].includes(request)) return load(request.slice(2));
      return actual(request);
    };
    const syntheticAdmission = spec => {
      assert.equal(spec.skill_remediation.synthetic_test_only, true);
      assert.equal(json(spec.profile.path).provider.compatibility.endpoint, 'friendli');
      return { synthetic_test_only: true, accounting: { fixed_conservative_micros: 88113737, outer_cap_micros: 100000000 } };
    };
    const seam = name === 'cs3-friendli-transfer.cjs' ? '\nvalidateSpec = __syntheticAdmission;\n' : '';
    new Function('require', 'module', 'exports', '__dirname', '__filename', '__syntheticAdmission', fs.readFileSync(filename, 'utf8') + seam)(local, module, module.exports, dir, filename, syntheticAdmission);
    return module.exports;
  }
  const campaign = load('cs3-comparison.cjs'), helper = load('cs3-friendli-transfer.cjs'), review = load('cs3-comparison-review.cjs');
  const put = (name, value) => { const file = path.join(directory, name), bytes = typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value, null, 2) + '\n'; fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes); return { path: file, sha256: sha(bytes) }; };
  fs.cpSync(path.join(source, 'src/skills/builtin'), path.join(directory, 'installed/skills/builtin'), { recursive: true });
  const executable = put('installed/vcp.exe', Buffer.concat([Buffer.from('Never executed synthetic binary\n'), fs.readFileSync(path.join(directory, 'installed/skills/builtin/catalog.json'))]));
  const catalog = put('catalog.json', {}), node = put('node.exe', 'Never executed synthetic Node'), future = String(Date.now() + 86400000);
  const profile = put('profile.json', { version: 1, workspace: directory, trust_workspace: true, maximum_autonomy: 'plan', automatic_effects: [], catalog: catalog.path,
    max_requests: 16, max_transport_retries: 0, output_tokens: '2048', deadline_seconds: 600, provider_timeout_seconds: 120,
    provider: { raw_sha256: catalog.sha256, observed_at: String(Date.now()), max_input: '1000', max_output: '2048', valid_until: future,
      compatibility: { byte_ceiling_qualified: false, responses_text_tools: true, provider_preferences_qualified: true, valid_until: future, model: 'deepseek/deepseek-v3.2', endpoint: 'friendli' },
      price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) } } });
  const gates = Object.fromEntries(['browser_boundary', 'node_fixture', 'web_oracles', 'ui_qualification'].map(name => [name, put(name + '.json', { synthetic: true })]));
  const spec = { executable, catalog, node, profile, gates, web_evidence: [], build_receipt: put('build.json', { synthetic: true }),
    skill_remediation: { synthetic_test_only: true, allocation: put('allocation.json', { synthetic: true }) } };
  spec.skill_remediation.history = put('history.json', { archive: put('archive.json', { archive: path.join(directory, 'historical-source') }) });
  spec.friendli_transfer = { decision: put('decision.json', {}), retirement: put('retirement.json', {}), skill_terminal: {
    plan: put('prior-skl/plan.json', { directory: path.join(directory, 'prior-skl') }), disposition: put('prior-skl/disposition.json', {}) } };
  // Source-real old cohorts and independent original global arm rotation: not
  // produced by the transfer's reordered executionPlan implementation.
  const webManifest = json(path.join(source, 'scripts/evals/fixtures/webapp/manifest.json'));
  const oldWeb = webManifest.cases.map(item => ({ ...put('old-web/' + item.id + '.json', { case_id: item.id, status: item.id.includes('near-miss') ? 'bug_detected' : item.id.includes('missing') ? 'expected_not_run' : 'passed' }), case_id: item.id }));
  const originalTasks = campaign.cohort(oldWeb, true), docHelper = load('cs3-document-remediation.cjs'), originalRegistry = require(path.join(source, 'scripts/evals/cs3-comparison-candidates.cjs'));
  const originalRow = (task, index, offset, registry) => { const arm = campaign.arms[(index + offset) % 3], prompt = campaign.prompt(task); return {
    id: task.id + '--' + arm, case_id: task.id, skill: task.skill, arm, cap_micros: 600000, call_ceiling: 16,
    skills: arm === 'none' ? [] : arm === 'candidate' ? [registry.qualified(task.skill)] : task.nearest.map(id => 'vcp-builtin::' + id + '::' + id),
    prompt, prompt_sha256: sha(prompt), files: Object.entries(task.files).map(([path, content]) => ({ path, sha256: sha(content), bytes: Buffer.byteLength(content) })) }; };
  state.proof = { groups: helper.skills.slice(0, 4).map(skill => ({ skill, tasks: originalTasks.filter(task => task.skill === skill),
    plan_ref: put('old-' + skill + '.json', { synthetic: true }), plan: { candidate_assets: originalRegistry.inspect(),
      runs: originalTasks.flatMap((task, index) => task.skill === skill ? [0, 1, 2].map(offset => originalRow(task, index, offset, originalRegistry)) : []) } })),
    document: { tasks: docHelper.tasks(), candidate_assets: docHelper.candidateRegistry.inspect(),
      runs: docHelper.tasks().flatMap((task, index) => [0, 1, 2].map(offset => originalRow(task, index, offset, docHelper.candidateRegistry))) } };
  const specRef = put('spec.json', spec), prepared = helper.prepare(specRef.path, path.join(directory, 'campaign'));
  return { directory, source, campaign, helper, review, prepared, plan: json(prepared.plans[0].path), put, state, spec };
}
function answer(task) {
  // Invalid normal output intentionally requires no native/browser execution.
  // Other cases fail honestly; this test does not create qualification evidence.
  return { files: {}, report: 'Synthetic incomplete response.', not_run: [] };
}
function transport(f, options = {}) {
  const calls = [], stores = new Map(), rows = new Map(f.plan.runs.map(row => [row.id, row])), tasks = f.helper.tasks(f.spec);
  function invoke(_executable, args, timeout) {
    const base = path.dirname(args[args.indexOf('--data-dir') + 1]), id = path.basename(base), row = rows.get(id); assert(row);
    if (args.includes('run')) {
      const selected = args.flatMap((value, index) => value === '--skill' ? [args[index + 1]] : []), profile = json(path.join(base, 'profile.json'));
      assert.deepEqual(selected, row.skills); assert.equal(timeout, 780000); assert.equal(args[args.indexOf('--budget-usd') + 1], '0.600000');
      assert.equal(profile.maximum_autonomy, 'plan'); assert.deepEqual(profile.automatic_effects, []);
      assert.equal(profile.deadline_seconds, 600); assert.equal(profile.provider_timeout_seconds, 120); assert.equal(profile.max_requests, 16);
      assert.equal(profile.output_tokens, '2048'); assert.equal(profile.max_transport_retries, 0); assert.equal(profile.provider.compatibility.endpoint, 'friendli');
      assert.equal(Boolean(profile.skills), row.arm === 'candidate'); calls.push(id);
      const task = tasks.find(task => task.id === row.case_id), pending = calls.length === options.pendingAt;
      const scope = { task: 'task-' + id, workspace: 'workspace', session: 'session' }, attempt = 'attempt-' + id, request = 'request-' + id;
      let response = 'data: ' + JSON.stringify({ type: 'response.completed', response: { id: request, status: 'completed', output: [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify(answer(task)) }] }] } }) + '\n\n';
      if (pending) response = '{"error":{"message":"Synthetic unavailable billing"}}';
      if (calls.length === options.canaryAt || calls.length === options.foreignCanaryAt) {
        if (calls.length === options.canaryAt) assert.equal(task.kind, 'hostile');
        const marker = calls.length === options.foreignCanaryAt ? 'CS3_' + task.id.split('-')[0] + '_PRIVATE_CANARY_82D4' : (task.forbidden_literals?.[0] || 'CS3_' + task.id.split('-')[0] + '_PRIVATE_CANARY_82D4'), split = Math.floor(marker.length / 2);
        response = [marker.slice(0, split), marker.slice(split)].map(delta => 'data: ' + JSON.stringify({ type: 'response.output_text.delta', response_id: request, item_id: 'answer', output_index: 0, content_index: 0, delta })).join('\n\n') + '\n\n';
      }
      const bytes = new Map(), artifact = (artifactId, channel, content, schema) => { const value = Buffer.from(content); bytes.set(artifactId, value); return { collection: 'artifact', id: artifactId, visibility: 'available', record: { state: 'complete', length: String(value.length), sha256: sha(value), spec: { channel, schema } } }; };
      const output = artifact('response-' + id, 'response', response, 'responses-sse-observed-through-terminal/1');
      Object.assign(output.record.spec, { scope, source: 'retained-codex-attempt:' + attempt, omissions: ['authentication_headers', 'recovery_material', ...(pending ? ['explicit_abort'] : [])] });
      output.record.retained = [{ start: '0', end: output.record.length }]; if (pending) output.record.state = 'aborted';
      const builtin = json(path.join(path.dirname(f.plan.executable), 'skills/builtin/catalog.json'));
      const included = selected.flatMap(id => { const candidate = f.plan.candidate_assets.entries.find(item => item.qualified_id === id), built = builtin.skills.find(item => id === `vcp-builtin::${item.id}::${item.id}`);
        return (candidate ? candidate.parts : [built.body, ...(built.resources || [])]).map((part, index) => ({ kind: 'skill', id: `skill-${sha(id)}-${index}`, source_hash: part.sha256, trust: 'active_skill' })); });
      const context = artifact('context-' + id, 'context', JSON.stringify({ request_sha256: 'digest-' + id, included }), 'context-manifest/1');
      const phase = pending ? 'reconciliation_pending' : 'settled', charged = pending ? '0' : '37', liability = pending ? '100000' : '0', amount = { currency: 'USD', micros: '100000' };
      const money = [
        { collection: 'ledger', id: scope.task, visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: charged, active: '0', unresolved: liability, overrun: false, allocations: {}, protected: '0' } },
        { collection: 'attempt', id: attempt, visibility: 'available', record: { id: attempt, scope, root: scope.task, phase, uncertain: pending ? 'retained response did not complete' : null, previous: null, role: 'main', charged, send_intent: 'sent-' + id, reservation: 'reservation-' + id, quote: { amount }, provider_request: pending ? null : request, request_digest: 'digest-' + id } },
        { collection: 'reservation', id: 'reservation-' + id, visibility: 'available', record: { id: 'reservation-' + id, attempt, scope, root: scope.task, role: 'main', phase, charged, amount, liability, protected_draw: '0', protected_returned: '0' } },
        ...pending ? [] : [{ collection: 'settlement', id: 'settlement-' + id, visibility: 'available', record: { scope, attempt, applied: true, total: charged, observation: { scope, attempt, provider_request: request, final_usage: true, amount: { currency: 'USD', micros: charged } } } }]
      ];
      const outputs = [output], contexts = [context];
      if (calls.length === options.secondAttemptAt) {
        assert.equal(pending, false); const second = 'second-' + attempt, secondRequest = 'second-' + request;
        const extra = artifact('second-response-' + id, 'response', 'data: ' + JSON.stringify({ type: 'response.completed', response: { id: secondRequest, status: 'completed', output: [] } }) + '\n\n', 'responses-sse-observed-through-terminal/1');
        Object.assign(extra.record.spec, { scope, source: 'retained-codex-attempt:' + second, omissions: ['authentication_headers', 'recovery_material'] });
        extra.record.retained = [{ start: '0', end: extra.record.length }]; outputs.push(extra);
        contexts.push(artifact('second-context-' + id, 'context', JSON.stringify({ request_sha256: 'second-digest-' + id, included }), 'context-manifest/1'));
        money[0].record.settled = '74';
        money.push(
          { collection: 'attempt', id: second, visibility: 'available', record: { ...money[1].record, id: second, previous: null, send_intent: 'second-sent-' + id, reservation: 'second-reservation-' + id, provider_request: secondRequest, request_digest: 'second-digest-' + id } },
          { collection: 'reservation', id: 'second-reservation-' + id, visibility: 'available', record: { ...money[2].record, id: 'second-reservation-' + id, attempt: second } },
          { collection: 'settlement', id: 'second-settlement-' + id, visibility: 'available', record: { ...money[3].record, attempt: second, observation: { ...money[3].record.observation, attempt: second, provider_request: secondRequest } } }
        );
      }
      stores.set(id, { bytes, outputs, contexts, money });
      const conditions = pending ? { unresolved_effect: true, cancelled: false, budget_exhausted: false, required_input: false, incomplete: false, invalid_configuration: false, internal_failure: false, durably_paused: true, completed: false } : { completed: true, unresolved_effect: false };
      const paused = pending ? ['task', 'turn'].map(collection => ({ type: 'event', event: { event: { data: { facts: [{ collection, value: { scope, state: 'paused', reason: 'provider outcome requires accounting reconciliation' } }] } } } })) : [];
      return { status: pending ? 7 : 0, error: null, stderr: '', stdout: [{ type: 'accepted', scope }, ...paused, { type: 'result', scope, exit_code: pending ? 7 : 0, conditions }].map(JSON.stringify).join('\n') + '\n' };
    }
    const retained = stores.get(id); assert(retained); let items;
    if (args.includes('--offset')) {
      const artifact = args[args.indexOf('inspect') + 1], bytes = retained.bytes.get(artifact); assert(bytes);
      const start = Number(args[args.indexOf('--offset') + 1]), end = Math.min(start + 65536, bytes.length);
      const output = retained.outputs.find(item => item.id === artifact);
      items = [{ artifact, visibility: 'available', range: { start, end }, bytes: [...bytes.subarray(start, end)], ...(output ? { descriptor: output.record } : {}) }];
    } else { const view = args[args.indexOf('--view') + 1]; items = view === 'costs' ? retained.money : view === 'outputs' ? retained.outputs : view === 'context' ? retained.contexts : []; }
    return { status: 0, error: null, stderr: '', stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }) + '\n' };
  }
  return { calls, invoke };
}

module.exports = { fixture, transport, json, sha };

if (require.main === module) test('transferred executor preserves old arms and receipts, two readers, local canary and global integrity stops', async t => {
  const f = fixture(t), first = f.prepared.plans[0], fake = transport(f, { pendingAt: 2 });
  const result = await f.campaign.run(first.path, first.sha256, 'frontend-design', fake.invoke);
  assert.equal(result.stopped, false); assert.equal(fake.calls.length, 18); assert.equal(result.runs.length, 18);
  assert.equal(result.actual_cost_micros, null); assert.equal(result.conservative_debit_micros, 600000 + 17 * 37);
  assert.equal(result.unresolved_attempts, 1); assert(result.runs.every(row => row.status === 'failed'));
  const browser = f.put('browser.json', { plan_sha256: first.sha256, skill: first.skill, runs: result.runs.slice(0, 6).map(row => ({
    run_id: row.id, status: 'not_run_output_invalid', canonical_result_sha256: sha(fs.readFileSync(path.join(f.plan.directory, row.id, 'result.json'))) })) });
  const packets = f.review.prepare(first.path, first.sha256, first.skill, path.join(f.directory, 'readers'), browser.path);
  const readers = packets.packets.map(binding => f.put('reader-' + binding.reader + '.json', { reader: binding.reader, reviewer_id: 'independent-transfer-' + binding.reader,
    packet_sha256: binding.packet_sha256, independent_blind: true, rows: json(binding.path).rows.map(row => ({ id: row.id,
      ...Object.fromEntries(f.review.gates.map(gate => [gate, gate !== 'correctness'])), completeness: 0, clarity: 0, usefulness: 0, reason: 'Synthetic incomplete artifact is not qualified.' })) }));
  const disposition = f.review.settle(first.path, first.sha256, first.skill, packets.directory, ...readers.map(ref => ref.path), browser.path);
  assert.equal(disposition.status, 'unqualified'); assert.equal(disposition.independent_blind_readers, 2);
  assert.deepEqual(f.review.validateDisposition(first.path, first.sha256, first.skill), disposition);
  const doc = f.prepared.plans[4]; assert.throws(() => f.helper.begin(json(doc.path), doc.sha256), /skill order/);
  await assert.rejects(f.campaign.run(first.path, first.sha256, first.skill, fake.invoke), /order|consumed/); assert.equal(fake.calls.length, 18);

  const second = f.prepared.plans[1]; f.plan = json(second.path);
  const local = transport(f, { canaryAt: 10, secondAttemptAt: 10 });
  const halted = await f.campaign.run(second.path, second.sha256, second.skill, local.invoke);
  assert.equal(halted.stopped, true); assert.equal(local.calls.length, 10); assert.equal(halted.observed_attempts, 11);
  const terminal = f.review.terminal(second.path, second.sha256);
  assert.equal(terminal.schema, 'cs3-friendli-transfer-terminal-disposition/1'); assert.equal(terminal.status, 'terminal_unqualified');
  assert.equal(terminal.independent_blind_readers, 0); assert.equal(terminal.undispatched_ids.length, 8);
  assert.equal(terminal.evidence.accounting.known_settled_micros, 11 * 37);
  assert.deepEqual(f.review.validateTerminalDisposition(second.path, second.sha256), terminal);
  assert.equal(f.helper.admission(f.plan).conservative_debit_micros, 600000 + 28 * 37);
  const third = f.prepared.plans[2]; f.plan = json(third.path);
  const global = transport(f, { foreignCanaryAt: 1 });
  const bad = await f.campaign.run(third.path, third.sha256, third.skill, global.invoke);
  assert.equal(bad.stopped, true); assert.equal(global.calls.length, 1);
  assert.equal(json(path.join(f.plan.control_directory, 'halt.json')).failure_scope, 'global');
  assert.throws(() => f.review.terminal(third.path, third.sha256), /integrity halted/);
  const fourth = f.prepared.plans[3]; await assert.rejects(f.campaign.run(fourth.path, fourth.sha256, fourth.skill, global.invoke), /integrity halted/);
  assert.equal(global.calls.length, 1);
});
