// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { authoringHost } = require('../support/authoring-host.cjs');
const { prep: { prepare }, runner } = authoringHost();
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const assets = path.resolve(__dirname, '../../skills/builtin');

function mockRuntime(root) {
  // Synthetic build-receipt fixture; production validation has no test bypass.
  const prep = require('../../../scripts/evals/authoring-prepare.cjs');
  const repository = path.resolve(__dirname, '../../..');
  const hash = bytes => require('node:crypto').createHash('sha256').update(bytes).digest('hex');
  const checker = path.join(root, 'source-checker.exe'), build_receipt = path.join(root, 'checker-build.json');
  fs.writeFileSync(checker, 'synthetic native checker; never executed');
  const source = 'src/crates/vcp-cli/src/bin/vcp-authoring-check.rs', fixture_manifest = 'src/evals/skills/authoring/manifest.json';
  const builder = path.join(repository, 'scripts/evals/authoring-check-build.ps1');
  fs.writeFileSync(build_receipt, JSON.stringify({ schema: 'cs1-authoring-check-build/1', source, source_sha256: hash(fs.readFileSync(path.join(repository, source))), fixture_manifest, fixture_manifest_sha256: hash(fs.readFileSync(path.join(repository, fixture_manifest))), executable: checker, executable_sha256: hash(fs.readFileSync(checker)), cargo_command: prep.checkerCargoCommand, exit_code: 0, toolchain: { rustc: 'rustc synthetic-receipt-fixture' }, source_inputs: prep.identity(repository, prep.checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 })), source_inputs_unchanged: true, builder, builder_sha256: hash(fs.readFileSync(builder)) }));
  return { checker, build_receipt };
}

function setup(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = owner.root, executable = path.join(root, 'vcp.exe'), profileFile = path.join(root, 'profile.json'), catalog = path.join(root, 'catalog.json'), specFile = path.join(root, 'spec.json');
  fs.cpSync(assets, path.join(root, 'skills/builtin'), { recursive: true });
  fs.writeFileSync(executable, Buffer.concat([Buffer.from('never executed'), fs.readFileSync(path.join(assets, 'catalog.json'))]));
  fs.writeFileSync(catalog, '{}');
  const future = String(Date.now() + 3600000);
  const profile = { version: 1, trust_workspace: true, maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], workspace: 'rebound', provider: { observed_at: String(Date.now()), max_input: '1000', valid_until: future, max_output: '16384', price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) }, compatibility: { byte_ceiling_qualified: false, valid_until: future, responses_text_tools: true, provider_preferences_qualified: true } }, catalog, max_requests: 16, max_transport_retries: 0, output_tokens: '4096', deadline_seconds: 600, processes: [], checks: [], mcp: [], mcp_http: [] };
  fs.writeFileSync(profileFile, JSON.stringify(profile));
  fs.writeFileSync(specFile, JSON.stringify({ executable, profile: profileFile, aggregate_cap_usd: '0.360000', aggregate_call_ceiling: 72, runtime: mockRuntime(root), propose_opaque_checker_effects: true }));
  const prepared = prepare(specFile, path.join(root, 'trial'));
  return { ...prepared, root, executable, profileFile, planData: JSON.parse(fs.readFileSync(prepared.plan)) };
}
test('runner validates exact preparation and refuses cap/input/authority tampering before dispatch', t => {
  const f = setup(t), bytes = fs.readFileSync(f.plan);
  runner.validate(f.planData, f.plan);
  assert.throws(() => runner.run(f.plan, 'not-authorized', () => assert.fail()), /Authorization/);
  const changed = structuredClone(f.planData); changed.runs[0].cap_micros++;
  fs.writeFileSync(f.plan, JSON.stringify(changed));
  assert.throws(() => runner.run(f.plan, sha(fs.readFileSync(f.plan)), () => assert.fail()), /allocation/);
  fs.writeFileSync(f.plan, bytes);
  const changedQuote = structuredClone(f.planData); changedQuote.budget_preflight.required_first_call_micros = 0;
  fs.writeFileSync(f.plan, JSON.stringify(changedQuote));
  assert.throws(() => runner.run(f.plan, sha(fs.readFileSync(f.plan)), () => assert.fail()), /budget preflight changed/);
  fs.writeFileSync(f.plan, bytes);
  const base = path.join(f.planData.directory, f.planData.runs[0].id), file = path.join(base, 'profile.json'), profileBytes = fs.readFileSync(file);
  const profile = JSON.parse(profileBytes); profile.automatic_effects.push('execute'); fs.writeFileSync(file, JSON.stringify(profile));
  assert.throws(() => runner.run(f.plan, f.sha256, () => assert.fail()), /inputs changed/);
  fs.writeFileSync(file, profileBytes);
  fs.writeFileSync(path.join(base, 'data', 'old-state'), 'not fresh');
  assert.throws(() => runner.run(f.plan, f.sha256, () => assert.fail()), /not fresh/);
  assert.equal(fs.existsSync(path.join(f.planData.directory, 'execution-claim.json')), false);
});
test('output parent scaffolding is exact across arms and rejects missing, extra and linked directories', t => {
  const f = setup(t), plan = f.planData;
  const rows = plan.runs.filter(row => row.case_id === 'SKL-normal-package-v1');
  assert.equal(rows.length, 3);
  for (const row of rows) {
    assert.deepEqual(row.directories, ['checks', 'package', 'package/references']);
    const base = path.join(plan.directory, row.id), root = path.join(base, 'workspace');
    assert.deepEqual(fs.readdirSync(path.join(root, 'package/references')), []);
    assert.equal(runner.preserved(base, row), true);
  }
  const row = rows[0], base = path.join(plan.directory, row.id), root = path.join(base, 'workspace');
  const parent = path.join(root, 'package/references');
  fs.rmdirSync(parent);
  assert.throws(() => runner.validate(plan, f.plan, 36), /directory scaffold changed/);
  assert.throws(() => runner.finalWorkspace(base, row, []), /directory scaffold changed/);
  fs.mkdirSync(parent);
  fs.mkdirSync(path.join(root, 'extra'));
  assert.throws(() => runner.preserved(base, row), /directory scaffold changed/);
  assert.throws(() => runner.finalWorkspace(base, row, []), /directory scaffold changed/);
  fs.rmdirSync(path.join(root, 'extra'));
  fs.rmdirSync(parent);
  fs.symlinkSync(path.join(root, 'checks'), parent, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => runner.validate(plan, f.plan), /Symlink or junction/);
  fs.unlinkSync(parent); fs.mkdirSync(parent);
  const changed = structuredClone(plan); changed.runs.find(item => item.id === row.id).directories = ['checks'];
  assert.throws(() => runner.validate(changed, f.plan), /Frozen directory scaffold changed/);
  runner.validate(plan, f.plan);
});

test('unknown outcome stops all later dispatch and immutable claim prevents replay', t => {
  const f = setup(t); let calls = 0;
  const result = runner.run(f.plan, f.sha256, (_exe, args) => { calls++; assert.equal(args.at(-1), 'autonomous'); assert(!args.includes('--skill')); return { status: null, error: 'ETIMEDOUT', stdout: '', stderr: '' }; });
  assert.equal(calls, 1); assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, null);
  assert.equal(result.runs.length, 36); assert.equal(result.runs.filter(r => r.status === 'not_run').length, 35);
  assert.throws(() => runner.run(f.plan, f.sha256, () => assert.fail()), /EEXIST/);
});
test('canonical unknown ledger stops subsequent model calls without recycling allocation', t => {
  const f = setup(t); let dispatch = 0;
  const call = (_exe, args) => {
    if (args.includes('run')) { dispatch++; return { status: 0, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'task' } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: true } }), stderr: '' }; }
    const view = args[args.indexOf('--view') + 1];
    const items = view === 'costs' ? [{ collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '10000', active: '0', unresolved: '1', settled: '0', overrun: false } }] : [];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
  const result = runner.run(f.plan, f.sha256, call);
  assert.equal(dispatch, 1); assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, null);
  assert.match(result.runs[0].reason, /unknown liability/);
});
test('context evidence requires all pinned skill resource parts for every settled request', () => {
  const catalog = JSON.parse(fs.readFileSync(path.join(assets, 'catalog.json'))), entry = catalog.skills.find(s => s.id === 'skill-authoring');
  const skill = 'vcp-builtin::skill-authoring::skill-authoring', id = 'skill-' + sha(Buffer.from(skill));
  const attempt = { phase: 'settled', request_digest: 'request' };
  const check = included => {
    const bytes = Buffer.from(JSON.stringify({ request_sha256: 'request', included }));
    const pages = [{ gaps: [], items: [{ id: 'context', collection: 'artifact', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec: { schema: 'context-manifest/1' } } }] }];
    const call = () => ({ status: 0, stdout: JSON.stringify({ type: 'result', data: { items: [{ range: { start: 0, end: bytes.length }, bytes: [...bytes] }], gaps: [], next_cursor: null } }) });
    return { pages, call };
  };
  const parts = [entry.body, ...entry.resources].map((part, index) => ({ kind: 'skill', trust: 'active_skill', source_hash: part.sha256, id: id + '-' + index }));
  const valid = check(parts);
  assert.equal(runner.skillEvidence({ executable: 'unused' }, os.tmpdir(), { skill }, valid.pages, [attempt], valid.call).parts, 2);
  const missing = check(parts.slice(0, 1));
  assert.throws(() => runner.skillEvidence({ executable: 'unused' }, os.tmpdir(), { skill }, missing.pages, [attempt], missing.call), /body\/resource/);
  assert.throws(() => runner.skillEvidence({ executable: 'unused' }, os.tmpdir(), { skill: null }, valid.pages, [attempt], valid.call), /body\/resource/);
  assert.throws(() => runner.skillEvidence({ executable: 'unused' }, os.tmpdir(), { skill }, valid.pages, [{ ...attempt, request_digest: 'wrong' }], valid.call), /No canonical context/);
});
test('final workspace accepts only bounded authorized changes and preserves every input', t => {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const base = owner.root, workspace = path.join(base, 'workspace'); fs.mkdirSync(workspace); fs.writeFileSync(path.join(workspace, 'source.md'), 'original');
  const row = { files: runner.inventory(workspace), directories: [] };
  fs.writeFileSync(path.join(workspace, 'output.md'), 'new artifact');
  assert.equal(runner.finalWorkspace(base, row, ['output.md']).get('output.md'), 'new artifact');
  fs.writeFileSync(path.join(workspace, 'source.md'), 'changed');
  assert.throws(() => runner.finalWorkspace(base, row, ['output.md']), /outside bounded/);
});
test('staged checker, external case map and immutable scaffolds cannot be replaced', t => {
  const f = setup(t), plan = f.planData, base = path.join(plan.directory, plan.runs[0].id);
  for (const file of [plan.runtime.checker, plan.runtime.cases_file, path.join(base, 'workspace/checks/authoring.case.json')]) {
    const original = fs.readFileSync(file); fs.appendFileSync(file, 'tampered');
    assert.throws(() => runner.run(f.plan, f.sha256, () => assert.fail()), /checker|mapping|inputs changed/);
    fs.writeFileSync(file, original);
    assert.equal(fs.existsSync(path.join(plan.directory, 'execution-claim.json')), false);
  }
  const projected = runner.finalWorkspace(base, plan.runs[0], ['runbook.md']);
  assert.equal(projected.has('package.json'), false);
  assert.equal(projected.has('checks/authoring.case.json'), false);
  fs.writeFileSync(path.join(base, 'workspace/checks/authoring.case.json'), '{}');
  assert.throws(() => runner.finalWorkspace(base, plan.runs[0], ['runbook.md']), /outside bounded|scaffold/);
});
for (const scenario of ['timeout', 'changed-profile']) test(`settled execution preserves evidence and stops at ${scenario} before further spend`, t => {
  const f = setup(t), plan = f.planData; let dispatch = 0;
  const content = 'Investigate 40 messages for 10 minutes. Read last 20 logs; stop and escalate E17. Get operator approval before one restart; recover below 10 within 5 minutes. [Service](service.md) [Operations](operations.md)\n';
  const answer = { files: [{ path: 'runbook.md', content }], report: 'Drafted from local sources.', not_run: ['No operational actions run.'] };
  const response = Buffer.from('data: ' + JSON.stringify({ type: 'response.completed', response: { id: 'provider-request', status: 'completed', output: [{ type: 'message', content: [{ type: 'output_text', text: JSON.stringify(answer) }] }] } }) + '\n\n');
  const context = Buffer.from(JSON.stringify({ request_sha256: 'request-digest', included: [] }));
  const artifact = (id, bytes, spec) => ({ id, collection: 'artifact', visibility: 'available', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec } });
  const call = (_exe, args) => {
    if (args.includes('run')) {
      dispatch++;
      if (dispatch === 2) { assert.equal(args.at(-1), 'vcp-builtin::architecture::architecture'); return { status: null, error: 'ETIMEDOUT', stdout: '', stderr: '' }; }
      fs.writeFileSync(path.join(plan.directory, plan.runs[0].id, 'workspace/runbook.md'), content);
      return { status: 0, stdout: JSON.stringify({ type: 'accepted', scope: { task: 'task' } }) + '\n' + JSON.stringify({ type: 'result', conditions: { completed: true } }), stderr: '' };
    }
    const view = args[args.indexOf('--view') + 1], inspectId = args[args.indexOf('inspect') + 1]; let items = [];
    if (args.includes('--offset')) {
      const bytes = inspectId === 'response' ? response : context;
      items = [{ range: { start: 0, end: bytes.length }, bytes: [...bytes] }];
      if (scenario === 'changed-profile' && inspectId === 'response') {
        const nextProfile = path.join(plan.directory, plan.runs[1].id, 'profile.json');
        const changed = JSON.parse(fs.readFileSync(nextProfile));
        changed.max_requests = 16;
        fs.writeFileSync(nextProfile, JSON.stringify(changed));
      }
    } else if (view === 'costs') items = [
      { collection: 'ledger', visibility: 'available', record: { currency: 'USD', cap: '10000', active: '0', unresolved: '0', settled: '1', overrun: false } },
      { collection: 'attempt', visibility: 'available', record: { id: 'attempt', phase: 'settled', role: 'main', charged: '1', request_digest: 'request-digest', provider_request: 'provider-request' } },
      { collection: 'settlement', visibility: 'available', record: { attempt: 'attempt', applied: true, observation: { final_usage: true } } },
    ];
    else if (view === 'outputs') items = [artifact('response', response, { channel: 'response' })];
    else if (view === 'context') items = [artifact('context', context, { schema: 'context-manifest/1' })];
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items, gaps: [], next_cursor: null } }), stderr: '' };
  };
  const result = runner.run(f.plan, f.sha256, call);
  assert.equal(dispatch, scenario === 'timeout' ? 2 : 1); assert.equal(result.runs[0].status, 'completed');
  assert.equal(result.runs[0].oracle.structural_pass, true);
  assert.equal(result.runs[0].actual_cost_micros, 1);
  assert.equal(result.stopped, true); assert.equal(result.actual_cost_micros, scenario === 'timeout' ? null : 1);
  if (scenario === 'changed-profile') {
    assert.match(result.runs[1].reason, /inputs changed/);
    assert.equal(result.final_inputs_unchanged, false);
    assert.equal(fs.existsSync(path.join(plan.directory, plan.runs[1].id, 'attempted.json')), false);
  }
  assert.equal(result.runs.filter(row => row.status === 'not_run').length, 34);
  assert(fs.existsSync(path.join(plan.directory, plan.runs[0].id, 'answer.json')));
});
