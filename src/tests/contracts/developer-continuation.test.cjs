// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { developerHost } = require('../support/developer-host.cjs');
const { identity } = require('../../../scripts/evals/authoring-prepare.cjs');
const candidates = require('../../../scripts/evals/developer-candidates.cjs');
const repository = path.resolve(__dirname, '../../..');
const assets = path.join(repository, 'src/skills/builtin');
const manifest = JSON.parse(fs.readFileSync(path.join(repository, 'src/evals/skills/developer/manifest.json')));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(fs.readFileSync(file));
const { createRequire } = require('node:module');
const save = (file, value) => fs.writeFileSync(file, JSON.stringify(value));

function mockRuntime(prep, root) {
  // Synthetic build-receipt fixture; production validation has no test bypass.
  const checker = path.join(root, 'source-checker.exe'), build_receipt = path.join(root, 'checker-build.json');
  fs.writeFileSync(checker, 'synthetic developer checker; never executed');
  const source = 'src/crates/vcp-cli/src/bin/vcp-developer-check.rs', fixture_manifest = 'src/evals/skills/developer/manifest.json';
  const builder = path.join(repository, 'scripts/evals/developer-check-build.ps1');
  fs.writeFileSync(build_receipt, JSON.stringify({ schema: 'cs2-developer-check-build/1', source, source_sha256: sha(fs.readFileSync(path.join(repository, source))), fixture_manifest, fixture_manifest_sha256: sha(fs.readFileSync(path.join(repository, fixture_manifest))), executable: checker, executable_sha256: sha(fs.readFileSync(checker)),
    cargo_command: ['cargo', 'build', '--manifest-path', 'src/third_party/codex/codex-rs/Cargo.toml', '--locked', '--offline', '--target-dir', path.join(root, 'target'), '-j2', '-p', 'vcp-cli', '--features', 'qualification', '--bin', 'vcp-developer-check'],
    exit_code: 0, toolchain: { rustc: 'rustc synthetic-receipt-fixture' }, source_scope: prep.checkerBuildScope, source_inputs: identity(repository, prep.checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 })), source_inputs_unchanged: true, builder, builder_sha256: sha(fs.readFileSync(builder)) }));
  return { checker, build_receipt };
}
function fixture(t, { windowMs = 23 * 3600000 } = {}) {
  const owner = ownedRoot(os.tmpdir());
  t.after(() => owner.cleanup());
  const root = owner.root, node = path.join(root, 'node.exe'), git = path.join(root, 'git-common');
  fs.writeFileSync(node, 'synthetic grading node; never executed');
  fs.mkdirSync(git);
  const host = developerHost({ nodeSha256: sha(fs.readFileSync(node)), gitCommonDir: git });
  const executable = path.join(root, 'vcp.exe'), profileFile = path.join(root, 'profile.json'), specFile = path.join(root, 'spec.json'), catalog = path.join(root, 'provider-catalog.json');
  fs.cpSync(assets, path.join(root, 'skills/builtin'), { recursive: true });
  fs.writeFileSync(executable, Buffer.concat([Buffer.from('synthetic executable; never run'), fs.readFileSync(path.join(assets, 'catalog.json'))]));
  fs.writeFileSync(catalog, '{}');
  const future = String(Date.now() + windowMs);
  const profile = { version: 1, trust_workspace: true, maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], workspace: 'rebound', provider: { observed_at: String(Date.now()), max_input: '1000', valid_until: future, max_output: '16384', price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) }, compatibility: { byte_ceiling_qualified: false, valid_until: future, responses_text_tools: true, provider_preferences_qualified: true } }, catalog, max_requests: 16, max_transport_retries: 0, output_tokens: '2048', deadline_seconds: 600, processes: [], checks: [], mcp: [], mcp_http: [] };
  const spec = { executable, profile: profileFile, runtime: mockRuntime(host.prep, root), grader: { node, node_sha256: sha(fs.readFileSync(node)) }, aggregate_cap_usd: '162.000000', aggregate_call_ceiling: 864, propose_checker_process: true };
  const save = () => { fs.writeFileSync(profileFile, JSON.stringify(profile)); fs.writeFileSync(specFile, JSON.stringify(spec)); };
  save();
  return { root, git, host, executable, profile, spec, specFile, save, prepare: (name = 'campaign') => host.prep.prepare(specFile, path.join(root, name)) };
}
// A fake CLI inspector that serves exact ranged reads for in-memory artifacts.
function artifactStore() {
  const store = new Map();
  return {
    add(id, channel, bytes, schema) { store.set(id, bytes); return { collection: 'artifact', id, visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec: { channel, ...(schema ? { schema } : {}) } } }; },
    call(_exe, args) {
      const id = args[args.indexOf('inspect') + 1], offset = Number(args[args.indexOf('--offset') + 1]), bytes = store.get(id), end = Math.min(offset + 65536, bytes.length);
      return { status: 0, stderr: '', stdout: JSON.stringify({ type: 'result', data: { items: [{ artifact: id, visibility: 'available', range: { start: offset, end }, bytes: [...bytes.subarray(offset, end)] }], gaps: [], next_cursor: null } }) };
    },
  };
}

function fakeCli(f, plan, { canaryCase, onDispatch, missingCheckCase, incompleteCase, malformedBeforeCase } = {}) {
  const store = artifactStore(), views = new Map();
  let dispatches = 0;
  const call = (exe, args) => {
    if (args.includes('run')) {
      const row = plan.runs.find(item => path.join(plan.directory, item.id, 'workspace') === args[args.indexOf('--workspace') + 1]);
      dispatches++;
      if (onDispatch) onDispatch(dispatches, row);
      const expected = row.skills.flatMap(skill => ['--skill', skill]);
      assert.deepEqual(args.slice(args.indexOf('--autonomy') + 2), expected);
      const answer = { files: [], report: row.case_id === canaryCase ? 'Leaked SYNTHETIC_API_CANARY_9Q' : 'Synthetic report.', not_run: ['Live provider compatibility'] };
      const response = store.add(`response-${row.id}`, 'response', Buffer.from(`data: ${JSON.stringify({ type: 'response.completed', response: { id: `resp-${row.id}`, status: 'completed', model: 'synthetic', output: [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify(answer) }] }] } })}\n\n`));
      const context = store.add(`context-${row.id}`, 'evidence', Buffer.from(JSON.stringify({ request_sha256: `req-${row.id}`, included: f.host.runner.skillParts(plan, row).map(part => ({ kind: 'skill', id: part.id, source_hash: part.hash, trust: 'active_skill' })) })), 'context-manifest/1');
      const attempt = { id: `a-${row.id}`, phase: 'settled', role: 'main', charged: '200', provider_request: `resp-${row.id}`, request_digest: `req-${row.id}` };
      const costs = [{ collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: String(row.cap_micros), active: '0', unresolved: '0', settled: '200', overrun: false } }, { collection: 'attempt', visibility: 'available', record: attempt }, { collection: 'settlement', visibility: 'available', record: { attempt: attempt.id, applied: true, observation: { final_usage: {} } } }];
      const tools = [], verification = [];
      if (row.write && row.case_id !== missingCheckCase) {
        tools.push(store.add(`stdout-${row.id}`, 'stdout', Buffer.from('TAP version 13\nok 1 - developer input preservation\nok 2 - developer output structure\n1..2\n')));
        tools.push(store.add(`outcome-${row.id}`, 'evidence', Buffer.from(JSON.stringify({ outcome: { status: 'passed' }, exit_code: 0, artifacts: [`stdout-${row.id}`], native_preparation: { executable: { sha256: plan.runtime.checker_sha256 } }, plan: { specification: 'package.json#test', request: { arguments: f.host.prep.checkerArguments }, expected_tests: f.host.prep.checkerTests } }))));
        verification.push({ collection: 'verification', record: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' }, exit_code: 0, output: `outcome-${row.id}` }] } });
      }
      const outputs = row.case_id === malformedBeforeCase ? [store.add(`malformed-${row.id}`, 'response', Buffer.from('data: invalid-json\n\n')), response] : [response];
      views.set(`task-${row.id}`, { costs, routing: [], outputs, context: [context], tools, verification });
      return { status: 0, stderr: '', stdout: [JSON.stringify({ type: 'accepted', scope: { task: `task-${row.id}` } }), JSON.stringify({ type: 'result', conditions: { completed: row.case_id !== incompleteCase } })].join('\n') };
    }
    if (args.includes('--offset')) return store.call(exe, args);
    const items = views.get(args[args.indexOf('inspect') + 1])[args[args.indexOf('--view') + 1]];
    return { status: 0, stderr: '', stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }) };
  };
  return { call, dispatches: () => dispatches };
}

// Real preparation/runtime/runner boundaries, a read-only predecessor inspector
// stand-in. The real frozen predecessor audit is covered separately. No live
// provider, credential, real claim or executable is accessed by these tests.
function continuationFixture(t) {
  const f = fixture(t), original = f.prepare('predecessor'), oldPlan = json(original.plan);
  const retained = oldPlan.runs.slice(0, 8).map((row, index) => {
    const base = path.join(oldPlan.directory, row.id);
    const report = { id: row.id, case_id: row.case_id, arm: row.arm, status: index === 7 ? 'failed' : 'completed', actual_cost_micros: 200, observed_attempts: 1, preserved: true, native_check: { status: index === 7 ? 'not_run' : 'passed' }, conditions: { completed: index !== 7 }, evidence_sha256: f.host.runner.runEvidence(base), workspace_sha256: identity(path.join(base, 'workspace'), ['.']).content_sha256 };
    save(path.join(base, 'result.json'), report);
    return { row, report, result_sha256: sha(fs.readFileSync(path.join(base, 'result.json'))) };
  });
  const oldClaim = path.join(f.git, 'vcp-cs2-developer-campaign.json');
  save(oldClaim, { plan_sha256: original.sha256, directory: oldPlan.directory });
  save(path.join(oldPlan.directory, 'halt.json'), { reason: 'synthetic owner reboot' });
  const oldClaimBytes = fs.readFileSync(oldClaim), oldHaltBytes = fs.readFileSync(path.join(oldPlan.directory, 'halt.json'));
  const inspect = (file, hash) => {
    assert.equal(file, original.plan); assert.equal(hash, original.sha256);
    assert.equal(sha(fs.readFileSync(file)), hash);
    assert(fs.readFileSync(oldClaim).equals(oldClaimBytes)); assert(fs.readFileSync(path.join(oldPlan.directory, 'halt.json')).equals(oldHaltBytes));
    f.host.runner.identical(oldPlan, original.plan);
    for (const item of retained) assert.equal(sha(fs.readFileSync(path.join(oldPlan.directory, item.row.id, 'result.json'))), item.result_sha256);
    return { plan: oldPlan, result: {}, result_sha256: 'a'.repeat(64), halt_sha256: sha(oldHaltBytes), retained, pending: oldPlan.runs.slice(8), actual_cost_micros: 1600, observed_attempts: 8 };
  };
  const filename = path.join(repository, 'scripts/evals/developer-continuation.cjs'), module = { exports: {} }, actualRequire = createRequire(filename);
  const scopedRequire = requested => requested === './developer-reconcile.cjs' ? { inspect } : requested === './developer-prepare.cjs' ? f.host.prep : requested === './developer-runner.cjs' ? f.host.runner : actualRequire(requested);
  new Function('require', 'module', 'exports', '__dirname', '__filename', fs.readFileSync(filename, 'utf8'))(scopedRequire, module, module.exports, path.dirname(filename), filename);
  const continuation = module.exports;
  const specFile = path.join(f.root, 'continuation-spec.json'), spec = { predecessor: { file: original.plan, sha256: original.sha256 }, profile: oldPlan.profile_source, aggregate_cap_usd: '92.000000', aggregate_call_ceiling: 736 };
  save(specFile, spec);
  return { ...f, continuation, oldPlan, oldClaim, oldClaimBytes, oldHaltBytes, retained, specFile, spec, prepare: (name = 'continuation') => continuation.prepare(specFile, path.join(f.root, name)) };
}

test('preparation authenticates eight outcomes and stages only 46 pending original IDs with fresh USD 92', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan);
  assert.equal(prepared.model_calls, 0); assert.equal(plan.authorization, false);
  assert.deepEqual(plan.runs.map(row => row.id), f.oldPlan.runs.slice(8).map(row => row.id));
  assert.equal(plan.retained.length, 8); assert.equal(plan.retained[7].status, 'failed');
  assert.equal(plan.runs[0].id, f.oldPlan.runs[8].id);
  assert.equal(plan.limits.cap_micros, 92000000); assert.equal(plan.limits.requests, 736);
  assert.equal(plan.historical_accounting.allocation_transferred_micros, 0);
  assert(plan.runs.every(row => row.cap_micros === 2000000 && row.call_ceiling === 16 && row.profile.budget_usd === '2.000000' && row.profile.max_requests === 16 && row.profile.output_tokens === '2048'));
  assert.equal(json(plan.runtime.cases_file).cases.length, plan.runs.filter(row => row.write).length);
  for (const row of plan.retained) assert.equal(fs.existsSync(path.join(plan.directory, row.id)), false);
  assert(fs.readFileSync(f.oldClaim).equals(f.oldClaimBytes)); assert(fs.readFileSync(path.join(f.oldPlan.directory, 'halt.json')).equals(f.oldHaltBytes));
  f.continuation.validate(plan, prepared.plan);
  assert.equal(f.continuation.admission(plan).remaining_micros, 92000000);
});

test('envelope, profile authority and frozen predecessor changes fail before any claim or dispatch', t => {
  const f = continuationFixture(t);
  const nested = path.join(f.oldPlan.directory, 'forbidden-continuation');
  assert.throws(() => f.continuation.prepare(f.specFile, nested), /must not overlap/);
  assert.equal(fs.existsSync(nested), false);
  for (const [field, value] of [['aggregate_cap_usd', '100.000000'], ['aggregate_call_ceiling', 864]]) {
    save(f.specFile, { ...f.spec, [field]: value });
    assert.throws(() => f.prepare(), /exactly USD 92/);
    assert.equal(fs.existsSync(path.join(f.root, 'continuation')), false);
  }
  save(f.specFile, f.spec);
  const prepared = f.prepare(), plan = json(prepared.plan);
  const changed = structuredClone(plan); changed.runs[0].cap_micros = 3000000;
  assert.throws(() => f.continuation.identical(changed, prepared.plan), /identity or allocation changed/);
  assert.throws(() => f.continuation.run(prepared.plan, '0'.repeat(64), 'llm-integration', () => assert.fail('no provider dispatch')), /exact prepared plan hash/);
  fs.appendFileSync(path.join(f.oldPlan.directory, f.retained[7].row.id, 'result.json'), ' ');
  assert.throws(() => f.continuation.validate(plan, prepared.plan));
  assert.equal(fs.existsSync(f.continuation.campaignClaim()), false);
});

test('first continuation block dispatches only ten pending IDs and composes original failed eighth unchanged', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan), seen = [];
  const cli = fakeCli(f, plan, { onDispatch: (_count, row) => seen.push(row.id) });
  const result = f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', cli.call);
  assert.equal(result.stopped, false, JSON.stringify(result)); assert.equal(result.final_inputs_unchanged, true); assert.equal(cli.dispatches(), 10);
  assert.deepEqual(seen, f.oldPlan.runs.slice(8, 18).map(row => row.id));
  assert.equal(result.actual_cost_micros, 2000); assert.equal(result.observed_attempts, 10);
  const composed = f.continuation.inspectBlock(prepared.plan, prepared.sha256, 'llm-integration');
  assert.equal(composed.rows.length, 18); assert.equal(composed.result.runs[7].status, 'failed');
  assert.deepEqual(composed.result.runs.slice(0, 8), f.retained.map(item => item.report));
  assert.equal(composed.rows[7].evidence_directory, path.join(f.oldPlan.directory, f.retained[7].row.id));
  assert.equal(composed.rows[8].evidence_directory, path.join(plan.directory, plan.runs[0].id));
  assert(fs.readFileSync(f.oldClaim).equals(f.oldClaimBytes)); assert(fs.readFileSync(path.join(f.oldPlan.directory, 'halt.json')).equals(f.oldHaltBytes));
  assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail('no replay')), /EEXIST/);
  assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'mcp-development', () => assert.fail('no next block without review')));
  const other = f.prepare('second-continuation');
  assert.throws(() => f.continuation.run(other.plan, other.sha256, 'llm-integration', () => assert.fail('no second authorization draw')), /already holds/);
});

test('unknown liability permanently halts fresh dispatch without changing the predecessor', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan); let calls = 0;
  const result = f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => { calls++; return { status: 1, stdout: '', stderr: '', error: Error('synthetic interruption') }; });
  assert.equal(calls, 1); assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, null);
  assert(fs.existsSync(path.join(plan.directory, 'halt.json')));
  assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /halted/);
  assert(fs.readFileSync(f.oldClaim).equals(f.oldClaimBytes)); assert(fs.readFileSync(path.join(f.oldPlan.directory, 'halt.json')).equals(f.oldHaltBytes));
});

test('canonical claim and accounting tampering cannot be reviewed or admitted', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan), cli = fakeCli(f, plan);
  f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', cli.call);
  const claim = path.join(plan.directory, 'claims', plan.runs[0].id + '.json'), bytes = fs.readFileSync(claim);
  save(claim, { run: plan.runs[0].id });
  assert.throws(() => f.continuation.admission(plan), /claim changed/);
  assert.throws(() => f.continuation.inspectBlock(prepared.plan, prepared.sha256, 'llm-integration'), /claim changed/);
  fs.writeFileSync(claim, bytes);
  const resultFile = path.join(plan.directory, 'result-llm-integration.json'), result = json(resultFile);
  result.actual_cost_micros = 0; save(resultFile, result);
  assert.throws(() => f.continuation.inspectBlock(prepared.plan, prepared.sha256, 'llm-integration'), /totals differ/);
});

test('provider window failure consumes no continuation claim', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan);
  const realNow = Date.now;
  try {
    Date.now = () => f.host.prep.qualificationEnds(json(plan.profile_source)) - 1000;
    assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /cannot cover/);
  } finally { Date.now = realNow; }
  assert.equal(fs.existsSync(f.continuation.campaignClaim()), false);
  assert.equal(fs.readdirSync(path.join(plan.directory, 'claims')).length, 0);
});

test('pre-block drift permanently halts even if changed bytes are restored', t => {
  const f = continuationFixture(t), prepared = f.prepare(), plan = json(prepared.plan);
  const prompt = path.join(plan.directory, plan.runs[0].id, 'prompt.txt'), bytes = fs.readFileSync(prompt);
  fs.appendFileSync(prompt, ' changed');
  assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /prompt or profile changed/);
  assert(fs.existsSync(path.join(plan.directory, 'halt.json')));
  fs.writeFileSync(prompt, bytes);
  assert.throws(() => f.continuation.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /halted/);
  assert.equal(fs.existsSync(f.continuation.campaignClaim()), false);
});
