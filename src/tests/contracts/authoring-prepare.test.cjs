// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { ownedRoot } = require('../support/experiments.cjs');
const { authoringHost } = require('../support/authoring-host.cjs');
const { prepare, budgetPreflight } = authoringHost().prep;
const assets = path.resolve(__dirname, '../../skills/builtin');

test('production checker guard rejects non-Windows hosts before reading inputs', () => {
  const unsupported = authoringHost('linux').prep;
  assert.throws(() => unsupported.checkerRuntime({ checker: path.resolve('absent.exe'), build_receipt: path.resolve('absent.json') }, os.tmpdir(), {}), /Explicit absolute Windows/);
});

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

function fixture(t) {
  const owner = ownedRoot(os.tmpdir());
  t.after(() => owner.cleanup());
  const root = owner.root, executable = path.join(root, 'vcp.exe'), profileFile = path.join(root, 'profile.json'), specFile = path.join(root, 'spec.json');
  fs.cpSync(assets, path.join(root, 'skills/builtin'), { recursive: true });
  fs.writeFileSync(executable, Buffer.concat([Buffer.from('synthetic executable; never run'), fs.readFileSync(path.join(assets, 'catalog.json'))]));
  const catalog = path.join(root, 'provider-catalog.json');
  fs.writeFileSync(catalog, '{}');
  const future = String(Date.now() + 3600000);
  const profile = { version: 1, trust_workspace: true, maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], workspace: 'rebound', provider: { observed_at: String(Date.now()), max_input: '1000', valid_until: future, max_output: '16384', price: { currency: 'USD', valid_until: future, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) }, compatibility: { byte_ceiling_qualified: false, valid_until: future, responses_text_tools: true, provider_preferences_qualified: true } }, catalog, max_requests: 16, max_transport_retries: 0, output_tokens: '4096', deadline_seconds: 600, processes: [], checks: [], mcp: [], mcp_http: [] };
  const spec = { executable, profile: profileFile, aggregate_cap_usd: '0.360001', aggregate_call_ceiling: 73, runtime: mockRuntime(root), propose_opaque_checker_effects: true };
  const save = () => { fs.writeFileSync(profileFile, JSON.stringify(profile)); fs.writeFileSync(specFile, JSON.stringify(spec)); };
  save();
  return { root, executable, profile, spec, specFile, save };
}
test('CS-1 preparation retains 36 matched isolated arms without oracle answers or dispatch', t => {
  const f = fixture(t), result = prepare(f.specFile, path.join(f.root, 'proposal'));
  const plan = JSON.parse(fs.readFileSync(result.plan));
  assert.equal(result.model_calls, 0);
  assert.equal(result.runnable, true);
  assert.equal(plan.authorization, false);
  assert.equal(plan.runs.length, 36);
  assert.equal(plan.allocated_cap_micros, 360000);
  assert.equal(plan.aggregate_cap_micros, 360001);
  assert.equal(plan.allocated_call_ceiling, 72);
  assert.equal(plan.schema, 'cs-1-authoring-preparation/3');
  assert.equal(plan.permission_review.approval, 'pending_explicit_exact_plan_checker_process_authorization');
  assert.deepEqual(plan.permission_review.write_case_automatic_effects, ['read', 'write', 'execute', 'network', 'install', 'publish', 'opaque']);
  assert(plan.source.files.length > 0);
  assert.match(plan.toolchain.node_sha256, /^[a-f0-9]{64}$/);
  for (let index = 0; index < 36; index += 3) {
    const rows = plan.runs.slice(index, index + 3);
    assert.deepEqual(rows.map(row => row.arm), ['none', 'nearest', 'candidate']);
    assert.equal(rows[0].skill, null);
    assert.equal(rows[0].prompt_sha256, rows[2].prompt_sha256);
    assert.deepEqual(rows[0].files, rows[2].files);
    for (const row of rows) {
      const base = path.join(plan.directory, row.id), workspace = path.join(base, 'workspace');
      assert.equal(fs.existsSync(path.join(workspace, 'manifest.json')), false);
      assert.equal(fs.existsSync(path.join(workspace, 'oracles')), false);
      assert.equal(fs.existsSync(path.join(workspace, 'rubric.json')), false);
      const profile = JSON.parse(fs.readFileSync(path.join(base, 'profile.json')));
      assert(profile.affected_paths.length > 0);
      if (profile.maximum_autonomy === 'plan') {
        assert.deepEqual(profile.automatic_effects, []);
        assert.deepEqual(profile.affected_paths, row.files.map(file => file.path));
      } else {
        assert.equal(profile.maximum_autonomy, 'autonomous');
        assert.deepEqual(profile.automatic_effects, ['read', 'write', 'execute', 'network', 'install', 'publish', 'opaque']);
        assert.equal(profile.processes.length, 1);
        assert.equal(profile.processes[0].executable, plan.runtime.checker);
        assert.deepEqual(Object.keys(profile.processes[0].environment), ['SystemRoot']);
        assert.deepEqual(profile.checks[0].expected_tests, ['authoring input preservation', 'authoring output structure']);
        assert.equal(row.scaffold_paths.length, 3);
        assert(fs.existsSync(path.join(workspace, 'checks/authoring.case.json')));
      }
      if (profile.maximum_autonomy === 'plan') { assert.deepEqual(profile.processes, []); assert.deepEqual(profile.checks, []); assert.deepEqual(row.scaffold_paths, []); }
      assert.equal(profile.max_requests, 2);
      assert.equal(profile.budget_usd, '0.010000');
    }
  }
  assert(plan.runs.some(row => JSON.parse(fs.readFileSync(path.join(plan.directory, row.id, 'profile.json'))).maximum_autonomy === 'plan'));
  assert.throws(() => prepare(f.specFile, plan.directory), /New private directory/);
  assert.equal(fs.existsSync(path.join(plan.directory, 'execution-claim.json')), false);
});
test('preparation rejects stale executable, asset tampering and invalid caps before output ownership', t => {
  const f = fixture(t), target = path.join(f.root, 'rejected');
  for (const ceiling of [35, 577, 36.5]) {
    f.spec.aggregate_call_ceiling = ceiling; f.save();
    assert.throws(() => prepare(f.specFile, target), /Call ceiling/);
  }
  f.spec.aggregate_call_ceiling = 36; f.save();
  const original = fs.readFileSync(f.executable);
  fs.writeFileSync(f.executable, 'stale');
  assert.throws(() => prepare(f.specFile, target), /does not embed/);
  fs.writeFileSync(f.executable, original);
  fs.appendFileSync(path.join(f.root, 'skills/builtin/architecture/SKILL.md'), 'tampered');
  assert.throws(() => prepare(f.specFile, target), /hash mismatch/);
  assert.equal(fs.existsSync(target), false);
});
test('preparation refuses expanded authority, expired qualification, retries and embedded credentials', t => {
  const f = fixture(t), original = structuredClone(f.profile), target = path.join(f.root, 'rejected');
  for (const mutation of [
    p => { p.automatic_effects = ['write']; },
    p => { p.maximum_autonomy = 'autonomous'; },
    p => { p.processes = [{ executable: 'unapproved' }]; },
    p => { p.mcp = [{}]; },
    p => { p.hooks = [{}]; },
    p => { p.observers = {}; },
    p => { p.future_authority = {}; },
    p => { p.routing = {}; },
    p => { p.max_transport_retries = 1; },
    p => { p.provider.price.valid_until = '0'; },
    p => { p.provider.observed_at = String(Date.now() + 3600000); },
    p => { p.api_key = 'synthetic-secret'; },
  ]) {
    for (const key of Object.keys(f.profile)) delete f.profile[key];
    Object.assign(f.profile, structuredClone(original));
    mutation(f.profile); f.save();
    assert.throws(() => prepare(f.specFile, target));
    assert.equal(fs.existsSync(target), false);
  }
});
function pricedProfile() {
  const rates = { input: '440000', cache_read: '440000', cache_write: '550000', output: '1980000', request: '0', provider_tool: '0' };
  return { output_tokens: '2048', provider: { max_input: '922000', compatibility: { byte_ceiling_qualified: false }, price: { rates: Object.fromEntries(Object.entries(rates).map(([category, micros]) => [category, { micros, per_units: '1000000' }])) } } };
}
test('native first-call quote includes all cache partitions and rounds each category upward', () => {
  const profile = pricedProfile();
  const quote = budgetPreflight(profile, 1322516);
  assert.equal(quote.required_first_call_micros, 1322516);
  assert.deepEqual(quote.charges_micros, { input: 405680, output: 4056, cache_read: 405680, cache_write: 507100, request: 0, provider_tool: 0 });
  assert.throws(() => budgetPreflight(profile, 1322515), /requires USD 1.322516; allocated USD 1.322515/);
  profile.output_tokens = '512';
  assert.equal(budgetPreflight(profile, 3000000).required_first_call_micros, 1319474);
  // Tiny partitions must each round upward, rather than rounding their sum.
  profile.output_tokens = '1'; profile.provider.max_input = '1';
  assert.equal(budgetPreflight(profile, 10).required_first_call_micros, 5);
  delete profile.provider.price.rates.provider_tool;
  assert.throws(() => budgetPreflight(profile, 10), /complete rates/);
  profile.provider.compatibility.byte_ceiling_qualified = true;
  assert.throws(() => budgetPreflight(profile, 10), /unqualified full-input/);
});
test('underfunded arms fail before output ownership with exact required and allocated amounts', t => {
  const f = fixture(t), priced = pricedProfile();
  Object.assign(f.profile.provider, { max_input: priced.provider.max_input });
  f.profile.provider.price.rates = priced.provider.price.rates;
  f.profile.output_tokens = '2048'; f.spec.aggregate_cap_usd = '18.000000'; f.save();
  const destination = path.join(f.root, 'underfunded');
  assert.throws(() => prepare(f.specFile, destination), /requires USD 1.322516; allocated USD 0.500000/);
  assert.equal(fs.existsSync(destination), false);
});
test('checker authority requires explicit proposal and exact stable recorded build inputs', t => {
  const f = fixture(t), destination = path.join(f.root, 'refused');
  f.spec.propose_opaque_checker_effects = false; f.save();
  assert.throws(() => prepare(f.specFile, destination), /explicit propose_opaque_checker_effects/);
  f.spec.propose_opaque_checker_effects = true; f.save();
  const runtime = { ...f.spec.runtime };
  for (const field of ['checker', 'build_receipt']) {
    f.spec.runtime = { ...runtime, [field]: 'relative-path' }; f.save();
    assert.throws(() => prepare(f.specFile, destination), /Explicit absolute Windows/);
    assert.equal(fs.existsSync(destination), false);
  }
  f.spec.runtime = runtime; f.save();
  const receiptFile = f.spec.runtime.build_receipt, original = fs.readFileSync(receiptFile), receipt = JSON.parse(original);
  for (const change of [
    r => { r.source_inputs_unchanged = false; },
    r => { r.executable_sha256 = '0'.repeat(64); },
    r => { r.source_inputs[0].sha256 = '0'.repeat(64); },
    r => { r.cargo_command = ['cargo', 'build']; },
    r => { r.builder_sha256 = '0'.repeat(64); },
  ]) {
    const changed = structuredClone(receipt); change(changed); fs.writeFileSync(receiptFile, JSON.stringify(changed));
    assert.throws(() => prepare(f.specFile, destination), /Checker build receipt/);
    assert.equal(fs.existsSync(destination), false);
  }
  fs.writeFileSync(receiptFile, original);
});
