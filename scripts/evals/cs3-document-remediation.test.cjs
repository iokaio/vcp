// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const prep = require('./authoring-prepare.cjs'), core = require('./cs3-comparison.cjs'), policy = require('./cs3-comparison-policy.cjs'), capture = require('./developer-runner.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex'), json = file => JSON.parse(fs.readFileSync(file));
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-doc-remediation-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const put = (file, value) => { file = path.isAbsolute(file) ? file : path.join(directory, file); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value)); return { path: file, sha256: sha(fs.readFileSync(file)) }; };
  const root = path.join(directory, 'source'), controls = path.join(directory, 'old-controls'), slots = path.join(directory, 'old-slots');
  const raw = row => {
    const base = path.join(slots, row.id), scope = { workspace: 'workspace', session: 'session', task: row.id };
    const costs = [{ gaps: [], items: [{ collection: 'ledger', id: 'ledger', visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: '0', active: '0', unresolved: '0', protected: '0', allocations: {}, overrun: false } }] }];
    put(path.join(base, 'costs.json'), costs); for (const view of ['context', 'outputs', 'tools']) put(path.join(base, view + '.json'), [{ gaps: [], items: [] }]);
    put(path.join(base, 'stdout.jsonl'), [{ type: 'accepted', scope }, { type: 'result', scope, exit_code: 1, conditions: { completed: false, internal_failure: true, unresolved_effect: false } }].map(JSON.stringify).join('\n'));
    const report = { id: row.id, case_id: row.case_id, arm: row.arm, scope, status: 'failed', accounted: true, preserved: true, ...policy.fields(policy.accounting(costs, 600000)), observed_attempts: 0, evidence_sha256: capture.runEvidence(base) };
    put(path.join(base, 'result.json'), report); return report;
  };
  const skills = ['document-authoring', 'skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
  const rows = Array.from({ length: 108 }, (_, index) => ({ id: 'old-' + index, case_id: 'oldcase-' + index, skill: skills[Math.floor(index / 18)], arm: ['none', 'nearest', 'candidate'][index % 3], cap_micros: 600000, call_ceiling: 16, files: [], skills: [] }));
  for (const row of rows) {
    const base = path.join(slots, row.id); fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    row.profile_sha256 = put(path.join(base, 'profile.json'), { maximum_autonomy: 'plan', automatic_effects: [], canonical_tools: ['vcp_read', 'vcp_verify'] }).sha256;
    row.prompt_sha256 = put(path.join(base, 'prompt.txt'), 'original input').sha256;
  }
  const previous = { directory: slots, control_directory: path.join(directory, 'old-segment'), segment: { addenda: [] }, runs: rows };
  const previousRef = put('old-segment/plan.json', previous), segmentClaim = put('git/segment.json', { original: true });
  const prefix = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  const prefixRows = rows.slice(0, 11).map(row => {
    const report = raw(row), base = path.join(slots, row.id);
    return { id: row.id, inventory: prep.identity(base, ['.']), result_sha256: sha(fs.readFileSync(path.join(base, 'result.json'))), evidence_sha256: report.evidence_sha256 };
  });
  const priorAudit = put('old-segment-audit.json', { schema: 'cs3-comparison-segment-terminal-audit/1', plan_sha256: previousRef.sha256, consumed: prefixRows, totals: prefix,
    claim_sha256: segmentClaim.sha256, halt_sha256: put('old-segment/halt.json', {}).sha256, block_sha256: put('old-segment/result-document-authoring.json', {}).sha256,
    active_block_sha256: put('old-segment/active-block.json', {}).sha256, claims: [] });
  put('old-source/frozen.txt', 'old source'); const source = prep.identity(path.join(directory, 'old-source'), ['frozen.txt']);
  const manifest = { schema: 'cs3-comparison-isolation/1', directory: controls, source, prefix, excluded_ids: rows.slice(11, 18).map(r => r.id),
    spec: { predecessor: previousRef, audit: priorAudit }, groups: skills.slice(1).map(skill => ({ skill, ids: rows.filter(r => r.skill === skill).map(r => r.id) })) };
  const manifestRef = put('old-controls/manifest.json', manifest), oldClaim = put('git/isolation.json', { directory: controls, manifest_sha256: manifestRef.sha256 });
  const groups = manifest.groups.map(group => {
    const plan = { directory: slots, control_directory: path.join(controls, group.skill), runs: rows.filter(r => r.skill === group.skill), isolated: { manifest: manifestRef, skill: group.skill } };
    const planRef = put(path.join(plan.control_directory, 'plan.json'), plan); fs.mkdirSync(path.join(plan.control_directory, 'claims'));
    if (group.skill !== 'skill-authoring') return { skill: group.skill, plan: planRef, disposition: null, status: 'retired_undispatched', claimed_ids: [], undispatched_ids: group.ids, slots: [], review_evidence: [],
      untouched: plan.runs.map(row => ({ id: row.id, inventory: prep.identity(path.join(slots, row.id), ['.']) })) };
    const row = plan.runs[0], report = raw(row), base = path.join(slots, row.id);
    const disposition = put(path.join(plan.control_directory, 'terminal.json'), { schema: 'cs3-comparison-isolated-terminal-disposition/1', plan_sha256: planRef.sha256, skill: group.skill, status: 'terminal_unqualified', candidate_qualified: false,
      claimed_ids: [row.id], undispatched_ids: group.ids.slice(1), evidence: { reason: 'supplied_synthetic_canary', no_unresolved_execution_effects: true } });
    return { skill: group.skill, plan: planRef, disposition, status: 'terminal_unqualified', claimed_ids: [row.id], undispatched_ids: group.ids.slice(1), review_evidence: [],
      slots: [{ id: row.id, inventory: prep.identity(base, ['.']), result_sha256: sha(fs.readFileSync(path.join(base, 'result.json'))), evidence_sha256: report.evidence_sha256, accounting: policy.fields(report), observed_attempts: 0 }] };
  });
  const retired_ids = manifest.groups.slice(1).flatMap(group => group.ids), halt = { schema: 'cs3-comparison-runtime-retirement-halt/1', manifest_sha256: manifestRef.sha256,
    reason: 'untouched_skills_superseded_by_source_bound_runtime_amendment', retired_ids, action: 'All original isolated dispatch permanently stopped; no replay.' };
  const pre_controls_inventory = prep.identity(controls, ['.']), haltBytes = JSON.stringify(halt), retirement_halt = { path: path.join(controls, 'global-halt.json'), sha256: sha(haltBytes) };
  const proof = { schema: 'cs3-comparison-isolation-retirement-prepared/1', manifest: manifestRef, claim: oldClaim, source_archive: { path: path.join(directory, 'old-source'), source },
    pre_controls_inventory, groups, prefix, totals: prefix, excluded_ids: manifest.excluded_ids, model_calls: 0, retired_ids, retirement_halt, transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 };
  const prepared = put('retirement-prepared.json', proof), auditPath = path.join(directory, 'terminal-audit.json');
  const retirement_claim = put('git/vcp-cs3-runtime-remediation-retirement1.json', { schema: 'cs3-runtime-retirement-claim/1', manifest: manifestRef, prepared, audit_path: auditPath,
    retirement_halt, retired_ids, transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 });
  put(retirement_halt.path, haltBytes);
  const auditRef = put(auditPath, { ...proof, schema: 'cs3-comparison-isolation-retirement/1', controls_inventory: prep.identity(controls, ['.']), retirement_claim, archived_dispatch_denied_groups: 5 });
  const executable = put('binary/vcp.exe', 'synthetic never executed'), node = put('node.exe', 'synthetic node');
  fs.cpSync(path.resolve(__dirname, '../../src/skills/builtin'), path.join(directory, 'binary/skills/builtin'), { recursive: true });
  const candidate = path.resolve(__dirname, '../../src/evals/skills/cs3-document-remediation/candidates/document-authoring');
  fs.cpSync(candidate, path.join(root, 'src/evals/skills/cs3-document-remediation/candidates/document-authoring'), { recursive: true });
  const approved = { ...require('../../src/evals/skills/cs3-document-remediation/decision.json'), prior_terminal_audit_sha256: auditRef.sha256, executable_sha256: executable.sha256 };
  const decisionRef = put(path.join(root, 'src/evals/skills/cs3-document-remediation/decision.json'), approved);
  const runtimeDecision = put(path.join(root, 'src/evals/skills/cs3-runtime-remediation/decision.json'), require('../../src/evals/skills/cs3-runtime-remediation/decision.json'));
  for (const name of ['cs3-comparison-oracle.cjs', 'cs3-runtime-boundary-oracle.cjs']) put(path.join(root, 'scripts/evals', name), fs.readFileSync(path.join(__dirname, name), 'utf8'));
  put(path.join(root, 'src/crates/native.rs'), 'native'); put(path.join(root, 'src/skills/builtin/catalog.json'), '{}');
  const builder = put(path.join(root, 'scripts/evals/cs3-document-remediation-build.ps1'), 'synthetic builder');
  const buildScope = require('./cs3-document-remediation.cjs').buildScope;
  for (const relative of buildScope) if (!fs.existsSync(path.join(root, relative))) {
    if (/\.(?:toml|lock|json|cjs)$/.test(relative)) put(path.join(root, relative), 'synthetic source'); else fs.mkdirSync(path.join(root, relative), { recursive: true });
  }
  const { build_attestation, ...native } = prep.identity(root, buildScope), sourceRef = put('source-manifest.json', native);
  const buildReceipt = put('build.json', { schema: 'cs3-document-remediation-build/1', status: 'passed', exit_code: 0, executable: executable.path, executable_sha256: executable.sha256,
    source_inputs_unchanged: true, toolchain_unchanged: true, qualification_build: true, production_release: false, provider_calls: 0, tests_executed: 0, builder_sha256: builder.sha256,
    source_manifest: sourceRef, source_inputs: native, target_directory: path.join(directory, 'target'), compiler_artifact: { reason: 'compiler-artifact', target: { name: 'vcp' }, profile: { test: false }, features: ['qualification'], executable: path.join(directory, 'target/debug/vcp.exe') } });
  const catalog = put('catalog.json', '{}'), now = Date.now(), expiry = String(now + 86400000);
  const profile = put('profile.json', { version: 1, workspace: directory, trust_workspace: true, maximum_autonomy: 'plan', automatic_effects: [], max_requests: 16, output_tokens: '2048', deadline_seconds: 600, provider_timeout_seconds: 120, max_transport_retries: 0,
    provider: { raw_sha256: catalog.sha256, observed_at: String(now), max_input: '1000', max_output: '2048', valid_until: expiry,
      compatibility: { byte_ceiling_qualified: false, responses_text_tools: true, provider_preferences_qualified: true, valid_until: expiry, model: 'deepseek/deepseek-v3.2', endpoint: 'deepinfra/fp4' },
      price: { currency: 'USD', valid_until: expiry, rates: Object.fromEntries(['input', 'output', 'cache_read', 'cache_write', 'request', 'provider_tool'].map(category => [category, { micros: category === 'request' ? '1' : '0', per_units: '1' }])) } } });
  const filename = require.resolve('./cs3-document-remediation.cjs'), actualRequire = createRequire(filename), module = { exports: {} }, calls = { preflight: 0, qualification: 0, runtimeTerminal: 0 };
  const runtime = { exports: {} }, runtimeFile = require.resolve('./cs3-runtime-amendment.cjs');
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(runtimeFile, 'utf8'))(name => name === './cs3-comparison.cjs' ? { ...core, claimFile: () => path.join(directory, 'git/base.json') }
    : name === './cs3-document-remediation.cjs' ? module.exports : actualRequire(name), runtime, runtime.exports, path.join(root, 'scripts/evals'));
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, name => {
    if (name === './cs3-comparison.cjs') return { ...core, claimFile: () => path.join(directory, 'git/base.json'), sourceIdentity: () => prep.identity(root, ['scripts/evals', 'src/evals/skills/cs3-document-remediation']),
      profile: (spec, task, workspace, arm, current) => ({ ...core.profile(spec, task, workspace, 'none', current), ...(arm === 'candidate' ? { skills: module.exports.candidateRegistry.configuration(task.skill) } : {}) }) };
    if (name === './cs3-comparison-segment.cjs') return { ...actualRequire(name), claimFile: () => segmentClaim.path };
    if (name === './builtin-generation-prepare.cjs') return { requireEmbeddedCatalog() {} }; // Synthetic executable is never executed.
    if (name === './cs3-comparison-gates.cjs') return { validate: () => ({ synthetic_test_only: true }) };
    if (name === './cs3-read-preflight.cjs') return { validateQualification() { calls.qualification++; } };
    if (name === './cs3-document-remediation-preflight.cjs') return { validate() { calls.preflight++; } };
    if (name === './cs3-preflight-supplement.cjs') return { validate(ref) {
      assert.deepEqual(ref, calls.supplementRef, 'Synthetic supplemental approval is exact');
      return { original_build_receipt: buildReceipt, additional_cap_micros: 1800000 };
    } };
    if (name === './cs3-runtime-amendment.cjs') return { ...runtime.exports, terminal(ref) { assert(ref); calls.runtimeTerminal++; return { synthetic_terminal_only: true }; } };
    return actualRequire(name);
  }, module, filename, path.join(root, 'scripts/evals'));
  const spec = { executable, build_receipt: buildReceipt, catalog, node, profile, gates: {}, web_evidence: [], remediation: { decision: decisionRef, prior_terminal: auditRef, runtime_decision: runtimeDecision, qualification: put('qualification.json', {}), runtime_preflight: put('preflight.json', {}), runtime_terminal: put('runtime-terminal.json', {}) } };
  const reserveSpec = put('reserve-spec.json', { decision: decisionRef, prior_terminal: auditRef, runtime_decision: runtimeDecision, executable, build_receipt: buildReceipt });
  return { directory, root, put, module: module.exports, spec, reserveSpec, approved, auditRef, calls, groups, rows, slots };
}
test('fixed allocation admits one independently bound eighteen-slot experiment without releasing older reservations', t => {
  const f = fixture(t), helper = f.module;
  f.spec.remediation.allocation = helper.reserve(f.reserveSpec.path, path.join(f.directory, 'allocation'));
  assert.equal(json(f.spec.remediation.allocation.path).combined_cap_micros, 89063737);
  assert.equal(json(f.spec.remediation.allocation.path).cap_micros, 22450000);
  assert.throws(() => helper.reserve(f.reserveSpec.path, path.join(f.directory, 'second-allocation')), /EEXIST/);
  const profile = json(f.spec.profile.path);
  for (const mutation of [{ deadline_seconds: 180 }, { deadline_seconds: 601 }, { provider_timeout_seconds: 60 }, { provider_timeout_seconds: 121 }, { max_requests: 17 }, { output_tokens: '2049' }, { max_transport_retries: 1 }]) {
    assert.throws(() => helper.validateSpec({ ...f.spec, profile: f.put('rejected-profile.json', { ...profile, ...mutation }) }), /Fixed prospective profile bounds/);
  }
  const specRef = f.put('campaign-spec.json', f.spec), target = path.join(f.directory, 'new-campaign');
  assert.equal(helper.prepare(specRef.path, target, true).status, 'validated_not_claimed'); assert.equal(fs.existsSync(target), false);
  const prepared = helper.prepare(specRef.path, target), plan = json(prepared.path);
  assert.equal(plan.runs.length, 18); assert.equal(new Set(plan.runs.map(r => r.id)).size, 18);
  assert(plan.runs.every(r => r.skill === 'document-authoring' && r.cap_micros === 600000 && r.call_ceiling === 16));
  for (const arm of ['none', 'nearest', 'candidate']) assert.equal(plan.runs.filter(r => r.arm === arm).length, 6);
  for (const run of plan.runs) {
    const selected = json(path.join(target, run.id, 'profile.json'));
    assert.equal(selected.deadline_seconds, 600); assert.equal(selected.provider_timeout_seconds, 120);
    assert.equal(selected.max_requests, 16); assert.equal(selected.output_tokens, '2048'); assert.equal(selected.max_transport_retries, 0);
  }
  assert.equal(plan.candidate_assets.entries[0].version, '1.0.5');
  assert.deepEqual(helper.validate(plan, prepared.sha256), plan); assert(f.calls.preflight > 0 && f.calls.qualification > 0);
  assert.equal(helper.admission(plan).reserved_micros, 600000);
  const owner = { plan_sha256: prepared.sha256, skill: 'document-authoring' }, block = path.join(target, 'claims/block-document-authoring.json'), active = path.join(target, 'active-block.json');
  f.put(block, owner); f.put(active, owner); f.put(path.join(target, 'claims', plan.runs[0].id + '.json'), { plan_sha256: prepared.sha256, id: plan.runs[0].id });
  assert.deepEqual(helper.validate(plan, prepared.sha256), plan, 'Legitimate current slot is permitted before its first result');
  fs.unlinkSync(block); assert.throws(() => helper.validate(plan, prepared.sha256), /block and active ownership/);
  f.put(block, { ...owner, skill: 'foreign' }); assert.throws(() => helper.validate(plan, prepared.sha256), /block and active ownership/);
  f.put(block, owner); f.put(active, { ...owner, plan_sha256: '0'.repeat(64) }); assert.throws(() => helper.validate(plan, prepared.sha256), /block and active ownership/);
  assert.throws(() => helper.prepare(specRef.path, path.join(f.directory, 'replay')), /one-shot/);
});
test('supplement preserves the original allocation and adds its full reservation to DOC admission', t => {
  const f = fixture(t), helper = f.module;
  f.spec.remediation.allocation = helper.reserve(f.reserveSpec.path, path.join(f.directory, 'allocation'));
  const originalAllocation = fs.readFileSync(f.spec.remediation.allocation.path);
  const originalBuild = f.spec.build_receipt;
  f.spec.build_receipt = f.put('new-verifier-build.json', json(originalBuild.path));
  assert.throws(() => helper.validateAllocation(f.spec), /one-shot remediation allocation/);
  f.calls.supplementRef = f.put('supplement.json', { synthetic_test_only: true });
  f.spec.remediation.preflight_supplement = f.calls.supplementRef;
  assert.equal(helper.validateSpec(f.spec).accounting.fixed_conservative_micros, 80063737);
  assert.deepEqual(fs.readFileSync(f.spec.remediation.allocation.path), originalAllocation);
  f.spec.build_receipt = f.put('bad-new-build.json', { ...json(originalBuild.path), status: 'failed' });
  assert.throws(() => helper.validateSpec(f.spec), /qualification build/);
});

test('missing terminal pin, changed historical raw file, active global halt and excluded DOC drift deny reservation', t => {
  const f = fixture(t), helper = f.module;
  const decisionFile = f.spec.remediation.decision.path, original = fs.readFileSync(decisionFile);
  const noAudit = f.put(decisionFile, { ...f.approved, prior_terminal_audit_sha256: null }); assert.throws(() => helper.decision(noAudit), /Final terminal audit/); fs.writeFileSync(decisionFile, original);
  const raw = path.join(f.slots, f.groups[0].claimed_ids[0], 'stdout.jsonl'); fs.appendFileSync(raw, '\nchanged');
  assert.throws(() => helper.priorTerminal(f.auditRef, f.approved), /raw observation changed/);
  fs.writeFileSync(raw, fs.readFileSync(raw, 'utf8').replace('\nchanged', ''));
  const manifest = json(json(f.auditRef.path).manifest.path), halt = path.join(manifest.directory, 'global-halt.json'), originalHalt = fs.readFileSync(halt); f.put(halt, {});
  assert.throws(() => helper.priorTerminal(f.auditRef, f.approved), /terminal inventory differs/); fs.writeFileSync(halt, originalHalt);
  fs.appendFileSync(path.join(f.slots, f.rows[11].id, 'prompt.txt'), 'changed');
  assert.throws(() => helper.priorTerminal(f.auditRef, f.approved), /changed|pristine|untouched/i);
});
test('failed build flags, wrong native source and changed candidate bytes cannot qualify new executable', t => {
  const f = fixture(t), helper = f.module, original = fs.readFileSync(f.spec.build_receipt.path), receipt = JSON.parse(original);
  for (const mutation of [{ status: 'failed' }, { exit_code: 1 }, { source_inputs_unchanged: false }, { toolchain_unchanged: false }, { qualification_build: false }, { provider_calls: 1 }, { executable_sha256: '0'.repeat(64) }]) {
    const modified = f.put('mutant-build.json', { ...receipt, ...mutation }); assert.throws(() => helper.build({ ...f.spec, build_receipt: modified }, f.approved), /qualification build/);
  }
  for (const omitted of ['src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/.cargo', 'scripts/upstream']) {
    const selected = { ...receipt.source_inputs, scope: receipt.source_inputs.scope.filter(p => p !== omitted) };
    const changed = { ...receipt, source_inputs: selected, source_manifest: f.put('incomplete-source.json', selected) };
    assert.throws(() => helper.build({ ...f.spec, build_receipt: f.put('incomplete-build.json', changed) }, f.approved), /fixed build source closure/);
  }
  const falseDigest = { ...receipt.source_inputs, content_sha256: '0'.repeat(64) };
  assert.throws(() => helper.build({ ...f.spec, build_receipt: f.put('bad-source-build.json', { ...receipt, source_inputs: falseDigest, source_manifest: f.put('bad-source.json', falseDigest) }) }, f.approved), /build inputs differ/);
  const relativeArtifact = { ...receipt, compiler_artifact: { ...receipt.compiler_artifact, executable: 'debug/vcp.exe' } };
  assert.throws(() => helper.build({ ...f.spec, build_receipt: f.put('relative-artifact.json', relativeArtifact) }, f.approved), /compiler artifact/);
  fs.appendFileSync(path.join(f.root, 'src/third_party/codex/codex-rs/Cargo.lock'), 'changed'); assert.throws(() => helper.build(f.spec, f.approved), /build inputs differ/);
  fs.appendFileSync(path.join(f.root, 'src/evals/skills/cs3-document-remediation/candidates/document-authoring/SKILL.md'), 'changed'); assert.throws(() => helper.candidateRegistry.inspect(), /part changed/);
});
test('legacy selectors retain historical cohorts and accounting while remediation selects only fresh tasks', () => {
  const helper = require('./cs3-document-remediation.cjs');
  assert.equal(core.conservative({}), null); const old = { fixed_conservative_micros: 1813737 }; assert.equal(core.conservative({ successor: old }), old);
  assert.deepEqual(core.planTasks({ remediation: {} }), helper.tasks());
  assert.equal(core.candidateRegistry({}), require('./cs3-comparison-candidates.cjs'));
  assert.equal(core.candidateRegistry({ remediation: {} }).inspect().entries[0].version, '1.0.5');
  const builder = fs.readFileSync(path.join(__dirname, 'cs3-document-remediation-build.ps1'), 'utf8');
  const scope = [...builder.match(/\$scope = @\(([\s\S]*?)\)\r?\n\$capture/)[1].matchAll(/'([^']+)'/g)].map(match => match[1]);
  assert.deepEqual(helper.buildScope, scope, 'Builder and admission must bind the same complete inputs');
});
