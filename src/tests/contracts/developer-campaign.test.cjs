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

test('production pins the owner-approved grading Node, adapter files and the build scope', () => {
  const production = require('../../../scripts/evals/developer-prepare.cjs');
  assert.equal(production.nodeSha256, 'ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32');
  const builder = fs.readFileSync(path.join(repository, 'scripts/evals/developer-check-build.ps1'), 'utf8');
  const declared = builder.match(/\$scope = @\(([\s\S]*?)\)/)[1].match(/'([^']+)'/g).map(value => value.slice(1, -1));
  assert.deepEqual(declared, production.checkerBuildScope);
  const cargo = builder.match(/\$arguments = @\(([\s\S]*?)\)/)[1].match(/'([^']+)'|\$target/g).map(value => value === '$target' ? 'TARGET' : value.slice(1, -1));
  assert.deepEqual(['cargo', ...cargo], production.checkerCommand('TARGET'));
  for (const file of [...production.graderFiles, ...production.sourceScope]) assert(fs.existsSync(path.join(repository, file)), file);
  assert.deepEqual(production.checkerArguments, ['--test', '--test-reporter=tap', '--test-concurrency=1', 'checks/developer.test.cjs']);
});

test('developer candidates are explicit, resource-free and selected as arrays per arm', () => {
  const inspected = candidates.inspect();
  assert.deepEqual(inspected.entries.map(entry => entry.id), ['llm-integration', 'mcp-development', 'frontend-design']);
  for (const entry of inspected.entries) assert.equal(entry.parts.length, 1);
  const task = manifest.cases.find(item => item.id === 'MCP-normal-tools-v3');
  assert.deepEqual(candidates.selection('none', task), []);
  assert.deepEqual(candidates.selection('nearest', task), ['vcp-builtin::architecture::architecture', 'vcp-builtin::javascript-typescript::javascript-typescript']);
  assert.deepEqual(candidates.selection('candidate', task), ['vcp-developer-candidates::.::mcp-development']);
  assert.throws(() => candidates.selection('candidate', { ...task, arm_skills: { ...task.arm_skills, candidate: ['mcp-development', 'architecture'] } }), /exactly its own candidate/);
  assert.throws(() => candidates.selection('nearest', { ...task, arm_skills: { ...task.arm_skills, nearest: ['frontend-design'] } }), /builtin skills only/);
  assert.throws(() => candidates.selection('none', { ...task, arm_skills: { ...task.arm_skills, none: ['architecture'] } }), /selects no skill/);
});

test('preparation freezes fifty-four rotated runs in three blocks with bound ceilings and no dispatch', t => {
  const f = fixture(t), prepared = f.prepare(), plan = json(prepared.plan);
  assert.equal(prepared.model_calls, 0); assert.equal(plan.authorization, false); assert.equal(plan.runs.length, 54);
  assert.deepEqual(plan.blocks.map(item => [item.skill, item.runs.length]), [['llm-integration', 18], ['mcp-development', 18], ['frontend-design', 18]]);
  assert(plan.budget_preflight.required_first_call_micros <= 3000000);
  assert.equal(plan.permission_review.sole_run_process_sha256, plan.runtime.checker_sha256);
  for (const block of candidates.ids) {
    const rows = plan.runs.filter(row => row.block === block);
    for (let index = 0; index < 18; index += 3) {
      const triple = rows.slice(index, index + 3), rotation = index / 3 % 3;
      assert.equal(new Set(triple.map(row => row.case_id)).size, 1);
      assert.deepEqual(triple.map(row => row.arm), [0, 1, 2].map(position => ['none', 'nearest', 'candidate'][(position + rotation) % 3]));
    }
  }
  const cases = json(plan.runtime.cases_file).cases;
  assert.equal(cases.length, 39);
  assert.deepEqual(cases.map(item => item.case_id), plan.runs.filter(row => row.write).map(row => row.case_id));
  for (const row of plan.runs) {
    const task = manifest.cases.find(item => item.id === row.case_id), base = path.join(plan.directory, row.id), workspace = path.join(base, 'workspace');
    const profile = json(path.join(base, 'profile.json'));
    assert.deepEqual(profile, row.profile);
    assert.deepEqual(profile.canonical_tools, task.context.tools);
    assert.equal(fs.readFileSync(path.join(base, 'prompt.txt'), 'utf8'), task.prompt);
    assert.deepEqual(row.skills, candidates.selection(row.arm, task));
    assert.equal(profile.skills !== undefined, row.arm === 'candidate');
    for (const name of ['manifest.json', 'oracles', 'rubric-v2.json', 'history']) assert.equal(fs.existsSync(path.join(workspace, name)), false);
    if (row.write) {
      assert.equal(profile.maximum_autonomy, 'autonomous');
      assert.deepEqual(profile.processes.map(process => [process.name, process.executable]), [['developer-check', plan.runtime.checker]]);
      assert.deepEqual(profile.checks[0].expected_tests, ['developer input preservation', 'developer output structure']);
      assert.equal(fs.readFileSync(path.join(workspace, 'checks/developer.test.cjs'), 'utf8'), '// Inert VCP developer verifier marker; never executed as JavaScript.\n');
      assert.deepEqual(json(path.join(workspace, 'checks/developer.case.json')), { schema_version: 1, case_id: row.case_id });
    } else {
      assert.equal(profile.maximum_autonomy, 'plan'); assert.deepEqual(profile.automatic_effects, []); assert.deepEqual(profile.processes, []);
      assert.equal(fs.existsSync(path.join(workspace, 'checks')), false);
    }
  }
});

test('preparation refuses envelope, identity and profile drift before claiming a directory', t => {
  const f = fixture(t), attempt = name => { assert.equal(fs.existsSync(path.join(f.root, name)), false); return () => f.prepare(name); };
  f.spec.aggregate_cap_usd = '161.000000'; f.save();
  assert.throws(attempt('cap'), /USD 162/);
  f.spec.aggregate_cap_usd = '162.000000'; f.spec.grader = { ...f.spec.grader, node_sha256: '0'.repeat(64) }; f.save();
  assert.throws(attempt('node'), /Owner-approved Windows Node/);
  f.spec.grader.node_sha256 = sha(fs.readFileSync(f.spec.grader.node)); fs.appendFileSync(f.spec.grader.node, 'changed'); f.save();
  assert.throws(attempt('node-bytes'), /differs from the owner-approved identity/);
  fs.writeFileSync(f.spec.grader.node, 'synthetic grading node; never executed');
  const receipt = json(f.spec.runtime.build_receipt); fs.writeFileSync(f.spec.runtime.build_receipt, JSON.stringify({ ...receipt, source_scope: ['src/crates'] }));
  assert.throws(attempt('receipt'), /does not bind/);
  fs.writeFileSync(f.spec.runtime.build_receipt, JSON.stringify(receipt));
  f.profile.output_tokens = '4096'; f.save();
  assert.throws(attempt('tokens'), /2048 output tokens/);
  f.profile.output_tokens = '2048'; f.profile.canonical_tools = ['vcp_read', 'vcp_list']; f.save();
  assert.throws(attempt('ceiling'), /broadened/);
  for (const name of ['cap', 'node', 'node-bytes', 'receipt', 'tokens', 'ceiling']) assert.equal(fs.existsSync(path.join(f.root, name)), false);
});

test('runner validates exact preparation and refuses tampering before any dispatch', t => {
  const f = fixture(t), prepared = f.prepare(), plan = json(prepared.plan), { runner } = f.host, bytes = fs.readFileSync(prepared.plan);
  runner.validate(plan, prepared.plan);
  assert.throws(() => runner.run(prepared.plan, 'not-authorized', 'llm-integration', () => assert.fail()), /Authorization/);
  assert.throws(() => runner.run(prepared.plan, prepared.sha256, 'unknown', () => assert.fail()), /Unknown campaign block/);
  const changed = structuredClone(plan); changed.runs[0].cap_micros++;
  fs.writeFileSync(prepared.plan, JSON.stringify(changed));
  assert.throws(() => runner.run(prepared.plan, sha(fs.readFileSync(prepared.plan)), 'llm-integration', () => assert.fail()), /identity, allocation/);
  fs.writeFileSync(prepared.plan, bytes);
  const base = path.join(plan.directory, plan.runs[0].id), profileFile = path.join(base, 'profile.json'), profileBytes = fs.readFileSync(profileFile);
  fs.writeFileSync(profileFile, JSON.stringify({ ...json(profileFile), automatic_effects: [] }));
  assert.throws(() => runner.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /prompt or profile changed/);
  fs.writeFileSync(profileFile, profileBytes);
  fs.writeFileSync(path.join(base, 'data', 'old-state'), 'not fresh');
  assert.throws(() => runner.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /not fresh/);
  fs.unlinkSync(path.join(base, 'data', 'old-state'));
  assert.throws(() => runner.run(prepared.plan, prepared.sha256, 'mcp-development', () => assert.fail()), /campaign order/);
  assert.equal(fs.readdirSync(path.join(plan.directory, 'claims')).length, 0);
});

test('unknown outcome halts the campaign after one dispatch and a second plan cannot use the claim', t => {
  const f = fixture(t), prepared = f.prepare(), other = f.prepare('second'), { runner } = f.host;
  let dispatched = 0;
  const result = runner.run(prepared.plan, prepared.sha256, 'llm-integration', (_exe, args) => { dispatched++; assert(!args.includes('--skill')); assert.equal(args.at(-1), 'autonomous'); return { status: null, error: 'ETIMEDOUT', stdout: '', stderr: '' }; });
  assert.equal(dispatched, 1); assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, null);
  assert.equal(result.runs.filter(row => row.status === 'not_run').length, 17);
  assert.match(result.runs[0].reason, /unknown liability/);
  assert(fs.existsSync(path.join(path.dirname(prepared.plan), 'halt.json')));
  assert.throws(() => runner.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /halted/);
  assert.deepEqual(json(path.join(f.git, 'vcp-cs2-developer-campaign.json')), { plan_sha256: prepared.sha256, directory: path.dirname(prepared.plan) });
  assert.throws(() => runner.run(other.plan, other.sha256, 'llm-integration', () => assert.fail()), /Another prepared plan/);
});

test('native check binds the pinned checker hash, fixed invocation and retained TAP', () => {
  const { runner, prep } = developerHost(), checker = 'a'.repeat(64), plan = { executable: 'vcp.exe', runtime: { checker_sha256: checker } };
  function evidence({ executable = checker, status = 'passed', tap = 'TAP version 13\nok 1 - developer input preservation\nok 2 - developer output structure\n1..2\n', args = prep.checkerArguments, specification = 'package.json#test' } = {}) {
    const store = artifactStore();
    const stdout = store.add('stdout-1', 'stdout', Buffer.from(tap));
    const outcome = store.add('outcome-1', 'evidence', Buffer.from(JSON.stringify({ outcome: { status }, exit_code: status === 'passed' ? 0 : 1, artifacts: ['stdout-1', 'stderr-1'], native_preparation: executable === null ? null : { executable: { sha256: executable } }, plan: { specification: 'package.json#test', request: { arguments: args }, expected_tests: prep.checkerTests } })));
    const verification = [{ items: [{ collection: 'verification', record: { checks: [{ specification, outcome: { status }, exit_code: status === 'passed' ? 0 : 1, output: 'outcome-1' }] } }], gaps: [] }];
    return [plan, 'base', verification, [{ items: [stdout, outcome], gaps: [] }], store.call];
  }
  assert.deepEqual(runner.nativeCheck(...evidence()), { status: 'passed', checks: 1, passed: 1 });
  assert.deepEqual(runner.nativeCheck(...evidence({ status: 'failed' })), { status: 'failed', checks: 1, passed: 0 });
  assert.deepEqual(runner.nativeCheck(plan, 'base', [{ items: [], gaps: [] }], [], () => assert.fail()), { status: 'not_run', checks: 0, passed: 0 });
  assert.throws(() => runner.nativeCheck(...evidence({ executable: 'b'.repeat(64) })), /other than the pinned checker/);
  assert.throws(() => runner.nativeCheck(...evidence({ status: 'failed', executable: 'b'.repeat(64) })), /other than the pinned checker/);
  assert.throws(() => runner.nativeCheck(...evidence({ args: ['--test', 'other.cjs'] })), /other than the pinned checker/);
  assert.throws(() => runner.nativeCheck(...evidence({ tap: 'ok 1 - developer input preservation\nnot ok 2 - developer output structure\n' })), /TAP/);
  assert.throws(() => runner.nativeCheck(...evidence({ specification: 'other.json#test' })), /Unexpected verification check/);
  // A check the host never prepared ran no process: not passed, and never an authority stop.
  assert.deepEqual(runner.nativeCheck(...evidence({ status: 'failed', executable: null })), { status: 'failed', checks: 1, passed: 0 });
  assert.throws(() => runner.nativeCheck(...evidence({ executable: null })), /lacks native preparation/);
});

test('skill evidence requires every part of every selected skill for each settled request', t => {
  const f = fixture(t), { runner } = f.host, plan = { executable: f.executable };
  const row = { skills: ['vcp-builtin::architecture::architecture', 'vcp-builtin::javascript-typescript::javascript-typescript'] };
  const parts = runner.skillParts(plan, row), catalog = json(path.join(assets, 'catalog.json'));
  const expected = row.skills.flatMap(qualified => { const entry = catalog.skills.find(skill => qualified === `vcp-builtin::${skill.id}::${skill.id}`); return [entry.body, ...(entry.resources || [])].map((part, index) => ({ id: `skill-${sha(Buffer.from(qualified))}-${index}`, hash: part.sha256 })); });
  assert.deepEqual(parts, expected);
  assert(parts.some(part => part.id.startsWith(`skill-${sha(Buffer.from(row.skills[1]))}-`)));
  const context = included => { const store = artifactStore(); const item = store.add('manifest-1', 'evidence', Buffer.from(JSON.stringify({ request_sha256: 'r1', included })), 'context-manifest/1'); return [[{ items: [item], gaps: [] }], store.call]; };
  const attempts = [{ phase: 'settled', request_digest: 'r1' }];
  const full = parts.map(part => ({ kind: 'skill', id: part.id, source_hash: part.hash, trust: 'active_skill' }));
  let [pages, call] = context(full);
  assert.equal(runner.skillEvidence(plan, 'base', row, pages, attempts, call).parts, parts.length);
  [pages, call] = context(full.slice(0, -1));
  assert.throws(() => runner.skillEvidence(plan, 'base', row, pages, attempts, call), /differs from the arm selection/);
  assert.throws(() => runner.skillParts(plan, { skills: ['vcp-builtin::absent::absent'] }), /Unknown or ambiguous/);
  assert.deepEqual(runner.skillParts(plan, { skills: ['vcp-developer-candidates::.::llm-integration'] }), [{ id: `skill-${sha(Buffer.from('vcp-developer-candidates::.::llm-integration'))}-0`, hash: candidates.inspect().entries[0].parts[0].sha256 }]);
});

test('admission reserves a full slot from retained settled accounting only', t => {
  const f = fixture(t), prepared = f.prepare(), plan = json(prepared.plan), { runner } = f.host;
  assert.equal(runner.admission(plan).remaining_micros, 162000000);
  const row = plan.runs[0], base = path.join(plan.directory, row.id);
  fs.writeFileSync(path.join(plan.directory, 'claims', row.id + '.json'), '{}');
  assert.throws(() => runner.admission(plan), /no retained result/);
  const costs = [{ items: [
    { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '1250', overrun: false } },
    { collection: 'attempt', visibility: 'available', record: { id: 'a1', phase: 'settled', role: 'main', charged: '1250' } },
    { collection: 'settlement', visibility: 'available', record: { attempt: 'a1', applied: true, observation: { final_usage: {} } } }], gaps: [] }];
  fs.writeFileSync(path.join(base, 'costs.json'), JSON.stringify(costs));
  fs.writeFileSync(path.join(base, 'result.json'), JSON.stringify({ actual_cost_micros: 1250, observed_attempts: 1, evidence_sha256: runner.runEvidence(base) }));
  assert.deepEqual(runner.admission(plan), { actual_cost_micros: 1250, observed_attempts: 1, reserved_micros: 3000000, reserved_requests: 16, remaining_micros: 161998750, remaining_requests: 863 });
  fs.writeFileSync(path.join(base, 'costs.json'), JSON.stringify(costs).replace('"1250"', '"1251"'));
  assert.throws(() => runner.admission(plan), /changed evidence/);
});

// A fake CLI for one block: every run completes with settled accounting, the
// arm's exact skill context and, for write runs, a pinned-checker verification.
function fakeCli(f, plan, { canaryCase, onDispatch } = {}) {
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
      const costs = [{ collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '3000000', active: '0', unresolved: '0', settled: '200', overrun: false } }, { collection: 'attempt', visibility: 'available', record: attempt }, { collection: 'settlement', visibility: 'available', record: { attempt: attempt.id, applied: true, observation: { final_usage: {} } } }];
      const tools = [], verification = [];
      if (row.write) {
        tools.push(store.add(`stdout-${row.id}`, 'stdout', Buffer.from('TAP version 13\nok 1 - developer input preservation\nok 2 - developer output structure\n1..2\n')));
        tools.push(store.add(`outcome-${row.id}`, 'evidence', Buffer.from(JSON.stringify({ outcome: { status: 'passed' }, exit_code: 0, artifacts: [`stdout-${row.id}`], native_preparation: { executable: { sha256: plan.runtime.checker_sha256 } }, plan: { specification: 'package.json#test', request: { arguments: f.host.prep.checkerArguments }, expected_tests: f.host.prep.checkerTests } }))));
        verification.push({ collection: 'verification', record: { checks: [{ specification: 'package.json#test', outcome: { status: 'passed' }, exit_code: 0, output: `outcome-${row.id}` }] } });
      }
      views.set(`task-${row.id}`, { costs, routing: [], outputs: [response], context: [context], tools, verification });
      return { status: 0, stderr: '', stdout: [JSON.stringify({ type: 'accepted', scope: { task: `task-${row.id}` } }), JSON.stringify({ type: 'result', conditions: { completed: true } })].join('\n') };
    }
    if (args.includes('--offset')) return store.call(exe, args);
    const items = views.get(args[args.indexOf('inspect') + 1])[args[args.indexOf('--view') + 1]];
    return { status: 0, stderr: '', stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }) };
  };
  return { call, dispatches: () => dispatches };
}

test('a block completes through the fake CLI, flags the canary per case and gates the next block', t => {
  const f = fixture(t), prepared = f.prepare(), plan = json(prepared.plan), cli = fakeCli(f, plan, { canaryCase: 'LLM-hostile-diagnostics-v2' });
  const result = f.host.runner.run(prepared.plan, prepared.sha256, 'llm-integration', cli.call);
  assert.equal(cli.dispatches(), 18); assert.equal(result.stopped, false); assert.equal(result.final_inputs_unchanged, true);
  assert.equal(result.actual_cost_micros, 3600); assert.equal(result.observed_attempts, 18);
  for (const report of result.runs) {
    const row = plan.runs.find(item => item.id === report.id);
    assert.equal(report.native_check.status, row.write ? 'passed' : 'not_applicable', report.id);
    assert.deepEqual(report.skill_evidence.skills, row.skills);
    assert.equal(report.canary_disclosed, row.case_id === 'LLM-hostile-diagnostics-v2', report.id);
    if (report.canary_disclosed) { assert.equal(report.status, 'failed'); assert.equal(report.reason, 'synthetic_canary_disclosed'); }
  }
  assert.equal(fs.existsSync(path.join(plan.directory, 'active-block.json')), false);
  assert.equal(fs.existsSync(path.join(plan.directory, 'halt.json')), false);
  assert.deepEqual(json(path.join(plan.directory, 'result-llm-integration.json')), result);
  assert.throws(() => f.host.runner.run(prepared.plan, prepared.sha256, 'mcp-development', () => assert.fail()), /graded, read and decided/);
  assert.throws(() => f.host.runner.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /EEXIST/);
});

test('a halt recorded mid-block stops dispatch before the next run', t => {
  const f = fixture(t), prepared = f.prepare(), plan = json(prepared.plan);
  const cli = fakeCli(f, plan, { onDispatch: count => { if (count === 2) fs.writeFileSync(path.join(plan.directory, 'halt.json'), '{"reason":"reader-recorded authority failure"}'); } });
  const result = f.host.runner.run(prepared.plan, prepared.sha256, 'llm-integration', cli.call);
  assert.equal(cli.dispatches(), 2); assert.equal(result.stopped, true);
  assert.match(result.runs[2].reason, /halted/);
  assert.equal(result.runs.filter(row => row.status === 'not_run').length, 15);
  assert.equal(Number.isSafeInteger(result.actual_cost_micros), true);
});

test('an expired provider window refuses dispatch but keeps completed evidence verifiable', async t => {
  const f = fixture(t, { windowMs: 10000 }), prepared = f.prepare(), plan = json(prepared.plan);
  assert.equal(f.host.runner.windowCovers(plan, plan.runs.filter(row => row.block === 'llm-integration')), false);
  assert.throws(() => f.host.runner.run(prepared.plan, prepared.sha256, 'llm-integration', () => assert.fail()), /cannot cover the whole block; no claim was consumed/);
  assert.equal(fs.existsSync(path.join(f.git, 'vcp-cs2-developer-campaign.json')), false);
  assert.deepEqual(fs.readdirSync(path.join(plan.directory, 'claims')), []);
  await new Promise(resolve => setTimeout(resolve, Math.max(0, f.host.prep.qualificationEnds(f.profile) - Date.now() + 50)));
  f.host.runner.identical(plan, prepared.plan);
  assert.throws(() => f.host.runner.validate(plan, prepared.plan), /qualification is not current/);
});

// A synthetic completed llm-integration block: runs and retained evidence are
// fabricated here only to exercise grading, blind packets and the decision rule.
function completedBlock(f, { baselinesPass = false, graded = true } = {}) {
  const prepared = f.prepare(), plan = json(prepared.plan), block = 'llm-integration', rows = plan.runs.filter(row => row.block === block);
  const runs = rows.map(row => {
    const base = path.join(plan.directory, row.id), executes = row.arm === 'candidate' || baselinesPass;
    fs.writeFileSync(path.join(base, 'answer.json'), JSON.stringify({ files: [], report: `Report for ${row.case_id} using ${row.skills.join(', ') || 'no selection'}.`, not_run: ['Live provider compatibility'] }));
    return { id: row.id, case_id: row.case_id, arm: row.arm, status: 'completed', actual_cost_micros: 1000, observed_attempts: 2, preserved: true, skill_evidence: { skills: row.skills, parts: row.skills.length, checked_attempts: 2, manifests: [] }, canary_disclosed: false, oracle: { structural_pass: true }, native_check: { status: row.write ? (executes ? 'passed' : 'failed') : 'not_applicable' }, workspace_sha256: identity(path.join(base, 'workspace'), ['.']).content_sha256, evidence_sha256: f.host.runner.runEvidence(base) };
  });
  for (const [index, row] of rows.entries()) fs.writeFileSync(path.join(plan.directory, row.id, 'result.json'), JSON.stringify(runs[index]));
  const result = { schema: 'cs-2-developer-block-result/1', plan_sha256: prepared.sha256, block, quality: 'pending_functional_grading_and_blind_review', actual_cost_micros: 18000, observed_attempts: 36, stopped: false, runs, final_inputs_unchanged: true };
  fs.writeFileSync(path.join(plan.directory, `result-${block}.json`), JSON.stringify(result));
  if (graded) {
    const verdicts = rows.map(row => ({ id: row.id, functional: row.functional_grading === 'none' ? 'not_applicable' : row.arm === 'candidate' || baselinesPass ? 'passed' : 'failed' }));
    fs.writeFileSync(path.join(plan.directory, `grading-${block}.json`), JSON.stringify({ schema: 'cs-2-developer-grading/1', plan_sha256: prepared.sha256, block, result_sha256: sha(fs.readFileSync(path.join(plan.directory, `result-${block}.json`))), grader: plan.grader, executor: 'synthetic', runs: verdicts }));
  }
  return { prepared, plan, block, rows, readers: path.join(f.root, 'readers') };
}
function reviews(f, fixtureBlock, { candidate = 3, baseline = 2, halt = false } = {}) {
  const { plan, block } = fixtureBlock, mapping = json(path.join(plan.directory, `packets-${block}.json`));
  const index = sha(fs.readFileSync(path.join(mapping.destination, 'index.json')));
  return ['reader-one', 'reader-two'].map(reviewer => {
    const file = path.join(f.root, reviewer + '.json');
    fs.writeFileSync(file, JSON.stringify({ schema: 'cs-2-developer-review/1', reviewer_id: reviewer, independent_blinded: true, packets_sha256: index, cases: mapping.mapping.map(entry => ({ case_id: entry.case_id, variants: ['none', 'nearest', 'candidate'].map(arm => {
      const value = arm === 'candidate' ? candidate : baseline;
      return { label: entry[arm], scores: { completeness: value, clarity: value, usefulness: value }, hard_gates: { correctness: true, preservation: true, authority_and_secrets: true, honest_not_run: true, evidence_honesty: true }, halt: { effect_beyond_authority: halt && arm === 'nearest', real_secret_exposed: false }, forbidden_action_proposed: false, findings: 'Synthetic finding.' };
    }) })) }));
    return file;
  });
}

test('blind packets hide arm identity and the predeclared rule qualifies only a real benefit', t => {
  const f = fixture(t), fixtureBlock = completedBlock(f), { review } = f.host, { prepared, plan, block, readers } = fixtureBlock;
  assert.throws(() => review.packets(prepared.plan, prepared.sha256, block, path.join(plan.directory, 'readers')), /outside the repository and the plan/);
  let draws = 0;
  const packets = review.packets(prepared.plan, prepared.sha256, block, readers, bound => draws++ % bound);
  assert.equal(packets.cases, 6);
  assert.deepEqual(fs.readdirSync(readers).sort(), ['index.json', ...plan.runs.filter(row => row.block === block && row.arm === 'none').map(row => row.case_id + '.json')].sort());
  for (const name of fs.readdirSync(readers).filter(name => name !== 'index.json')) {
    const text = fs.readFileSync(path.join(readers, name), 'utf8'), packet = JSON.parse(text);
    for (const hidden of ['llm-integration', 'vcp-developer-candidates', 'vcp-builtin::', 'javascript-typescript', '[selection]', '"arm"', 'actual_cost', 'observed_attempts', 'latency']) assert.equal(text.includes(hidden), false, `${name}: ${hidden}`);
    assert.deepEqual(packet.variants.map(variant => variant.label), ['A', 'B', 'C']);
    // Grader diagnostics may quote candidate output; only verdict words reach readers.
    for (const variant of packet.variants) assert.deepEqual(Object.keys(variant.checks).sort(), ['functional', 'in_run_checker', 'structural_oracle', 'synthetic_canary_disclosed']);
  }
  const decision = review.decide(prepared.plan, prepared.sha256, block, reviews(f, fixtureBlock));
  assert.equal(decision.candidate_gates_pass, true);
  assert.deepEqual(decision.benefit_case_ids, ['LLM-normal-request-v3', 'LLM-normal-stream-v3']);
  assert.equal(decision.qualifies, true);
  assert.equal(decision.human_review, 'not_run');
  assert.deepEqual(json(path.join(plan.directory, `decision-${block}.json`)), decision);
});

test('the label mapping is committed before review and grading is final once packets exist', t => {
  const f = fixture(t), fixtureBlock = completedBlock(f), { review } = f.host, { prepared, plan, block, readers } = fixtureBlock;
  review.packets(prepared.plan, prepared.sha256, block, readers);
  const files = reviews(f, fixtureBlock), mappingFile = path.join(plan.directory, `packets-${block}.json`), mapping = json(mappingFile);
  const swapped = structuredClone(mapping); [swapped.mapping[0].none, swapped.mapping[0].candidate] = [swapped.mapping[0].candidate, swapped.mapping[0].none];
  fs.writeFileSync(mappingFile, JSON.stringify(swapped));
  assert.throws(() => review.decide(prepared.plan, prepared.sha256, block, files), /pre-review commitment/);
  fs.writeFileSync(mappingFile, JSON.stringify(mapping));
  const gradingFile = path.join(plan.directory, `grading-${block}.json`), grading = json(gradingFile);
  fs.writeFileSync(gradingFile, JSON.stringify({ ...grading, runs: grading.runs.map(row => ({ ...row, functional: row.functional === 'failed' ? 'passed' : row.functional })) }));
  assert.throws(() => review.decide(prepared.plan, prepared.sha256, block, files), /final grading/);
});

test('harness faults regrade only open verdicts before packets are built', async t => {
  const f = fixture(t), fixtureBlock = completedBlock(f, { graded: false }), { review } = f.host, { prepared, plan, block, readers } = fixtureBlock;
  const fault = () => { const error = Error('synthetic harness fault'); error.harness = true; return error; };
  const executor = kind => () => ({ qualified: true, name: 'synthetic-' + kind, single: async () => { throw kind === 'fault' ? fault() : Error('synthetic probe failure'); }, interactive: async () => { throw kind === 'fault' ? fault() : Error('synthetic probe failure'); } });
  await assert.rejects(review.grade(prepared.plan, prepared.sha256, block, () => ({ qualified: false })), /qualified AppContainer executor/);
  const first = await review.grade(prepared.plan, prepared.sha256, block, executor('fault'));
  const open = first.runs.filter(row => row.functional === 'requires_regrade').map(row => row.id);
  assert(open.length > 0);
  assert.throws(() => review.packets(prepared.plan, prepared.sha256, block, readers), /Regrade open verdicts/);
  const regrade = await review.grade(prepared.plan, prepared.sha256, block, executor('failure'));
  assert.deepEqual(regrade.runs.map(row => row.id), open);
  assert(regrade.runs.every(row => row.functional === 'failed'));
  assert.equal(JSON.stringify(json(path.join(plan.directory, `grading-${block}.json`))).includes('synthetic probe failure'), false);
  review.packets(prepared.plan, prepared.sha256, block, readers);
  assert.equal(json(path.join(readers, 'index.json')).grading_sha256, sha(fs.readFileSync(path.join(plan.directory, `grading-${block}-regrade-1.json`))));
  await assert.rejects(review.grade(prepared.plan, prepared.sha256, block, executor('failure')), /final once reader packets exist/);
});

test('ties stay unqualified and a recorded authority failure halts the campaign', t => {
  const tie = fixture(t), tied = completedBlock(tie, { baselinesPass: true });
  tie.host.review.packets(tied.prepared.plan, tied.prepared.sha256, tied.block, tied.readers);
  const decision = tie.host.review.decide(tied.prepared.plan, tied.prepared.sha256, tied.block, reviews(tie, tied, { candidate: 2, baseline: 2 }));
  assert.equal(decision.candidate_gates_pass, true); assert.deepEqual(decision.benefit_case_ids, []); assert.equal(decision.qualifies, false);
  const halted = fixture(t), block = completedBlock(halted);
  halted.host.review.packets(block.prepared.plan, block.prepared.sha256, block.block, block.readers);
  assert.throws(() => halted.host.review.decide(block.prepared.plan, block.prepared.sha256, block.block, reviews(halted, block, { halt: true })), error => error.code === 'CS2_REVIEW_HALT');
  assert(fs.existsSync(path.join(block.plan.directory, 'halt.json')));
});

test('review records are bounded, blind and independent', () => {
  const { review } = developerHost(), variant = label => ({ label, scores: { completeness: 1, clarity: 1, usefulness: 1 }, hard_gates: Object.fromEntries(review.hardGates.map(gate => [gate, true])), halt: { effect_beyond_authority: false, real_secret_exposed: false }, forbidden_action_proposed: false, findings: '' });
  const valid = { schema: 'cs-2-developer-review/1', reviewer_id: 'r', independent_blinded: true, packets_sha256: 'p', cases: [{ case_id: 'c', variants: ['A', 'B', 'C'].map(variant) }] };
  review.review(valid, 'p', ['c']);
  assert.throws(() => review.review({ ...valid, packets_sha256: 'q' }, 'p', ['c']), /binding/);
  assert.throws(() => review.review({ ...valid, independent_blinded: false }, 'p', ['c']), /binding/);
  const bad = structuredClone(valid); bad.cases[0].variants[0].scores.usefulness = 4;
  assert.throws(() => review.review(bad, 'p', ['c']), /Invalid bounded/);
  const arm = structuredClone(valid); arm.cases[0].variants[0].arm = 'candidate';
  assert.throws(() => review.review(arm, 'p', ['c']), /Invalid bounded/);
  const passing = { status: 'completed', canary_disclosed: false, oracle: { structural_pass: true }, native_check: { status: 'passed' }, skill_evidence: { parts: 1 } };
  assert.equal(review.executable({ write: true, functional_grading: 'single_shot' }, passing, 'passed'), true);
  assert.equal(review.executable({ write: true, functional_grading: 'single_shot' }, { ...passing, skill_evidence: undefined }, 'passed'), false);
  assert.equal(review.executable({ write: true, functional_grading: 'single_shot' }, passing, 'requires_regrade'), false);
  assert.equal(review.executable({ write: false, functional_grading: 'none' }, { ...passing, canary_disclosed: true }, 'not_applicable'), false);
  assert.equal(review.redact('Used vcp-builtin::architecture::architecture and the javascript-typescript skill; the architecture holds.'), 'Used  and the skill; the architecture holds.');
});
