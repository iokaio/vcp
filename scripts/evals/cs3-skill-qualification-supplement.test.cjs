// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const sourceFile = path.join(__dirname, 'cs3-skill-qualification-supplement.cjs'), realRequire = createRequire(sourceFile), prep = realRequire('./authoring-prepare.cjs');
const doc = realRequire('./cs3-document-remediation.cjs'), sha = data => crypto.createHash('sha256').update(data).digest('hex');
function fixture(t) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-skl-qual-supplement-')), root = path.join(base, 'current'), old = path.join(base, 'old'), common = path.join(base, 'common');
  for (const directory of [root, old, common]) fs.mkdirSync(directory);
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const ref = file => ({ path: file, sha256: sha(fs.readFileSync(file)) });
  const put = (file, value) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, Buffer.isBuffer(value) || typeof value === 'string' ? value : JSON.stringify(value)); return ref(file); };
  const candidates = { path: path.join(old, 'candidates'), source_id: 'synthetic-package', entries: [{ id: 'skill-authoring', version: '1.0.3' }], files: [{ path: 'skill-authoring/skill.json', sha256: 'a'.repeat(64) }] }, tasks = [{ id: 'synthetic-task' }];
  const legacy = path.join(base, 'legacy'); fs.mkdirSync(legacy); put(path.join(legacy, 'source.txt'), 'synthetic previous source');
  function archive(directory, name) {
    const copy = path.join(base, name), source = prep.identity(directory, fs.readdirSync(directory).sort()); fs.cpSync(directory, copy, { recursive: true });
    return put(path.join(base, name + '.json'), { archive: copy, source, source_before_after_equal: true, archive_source_equal: true, model_calls: 0, claims_created: 0 });
  }
  const legacyArchive = archive(legacy, 'legacy-archive'), historicalDecision = put(path.join(base, 'historical-decision.json'), { historical_root: legacy });
  const history = put(path.join(base, 'history.json'), { archive: legacyArchive }), native = put(path.join(base, 'native.json'), { decision: historicalDecision });
  const nativeBinary = put(path.join(base, 'native/vcp.exe'), 'synthetic unchanged native'), pkg = put(path.join(base, 'package.json'), { synthetic: 'package' });
  const oldDecision = put(path.join(base, 'original-decision.json'), { model: 'deepseek/deepseek-v3.2', endpoint: 'friendli' });
  const originalBuild = put(path.join(base, 'old-build.json'), { synthetic: 'original verified build' });
  const oldClaimPath = path.join(base, 'probe1/probe-output/claim.json');
  const originalSpec = { executable: nativeBinary, build_receipt: originalBuild, decision: oldDecision, history, recovery_native: native, package_acceptance: pkg, qualification_claim_path: oldClaimPath };
  const releasePlan = put(path.join(base, 'released/plan.json'), { directory: path.join(base, 'released/slots') });
  fs.mkdirSync(path.join(base, 'protected'));
  const allocation = put(path.join(base, 'original-allocation/allocation.json'), { schema: 'cs3-skill-remediation-allocation/1', spec: originalSpec,
    proof: { historical: { protected_inventories: [{ directory: path.join(base, 'protected') }] }, release: { plan: releasePlan, rows: [] }, package: { authenticated: 'archived installed evidence' } } });
  // Explicit synthetic original validator: real guarded Module loading, source
  // identities, cache eviction and read-footprint capture execute around it.
  put(path.join(old, 'scripts/evals/cs3-skill-remediation.cjs'), `const fs=require('node:fs');module.exports={validateAllocation(spec){const value=JSON.parse(fs.readFileSync(spec.skill_remediation.allocation.path,'utf8'));if(value.spec.executable.sha256!==spec.executable.sha256)throw Error('synthetic original mismatch');return {model:'deepseek/deepseek-v3.2',endpoint:'friendli',allocation:value};},candidateRegistry:{inspect:()=>(${JSON.stringify(candidates)})},tasks:()=>(${JSON.stringify(tasks)})};`);
  const sourceFiles = { binary: 'src/crates/vcp-cli/src/bin/vcp-provider-conformance.rs', lease: 'src/crates/vcp-lifecycle/src/foundation/conformance.rs',
    settlement: 'src/crates/vcp-lifecycle/src/foundation/worker/conformance.rs', catalog: 'src/crates/vcp-models/src/catalog.rs' };
  for (const [key, file] of Object.entries(sourceFiles)) put(path.join(old, file), 'synthetic ' + key);
  const originalSource = archive(old, 'original-archive'), conformance = put(path.join(base, 'probe.exe'), 'synthetic conformance binary');
  const catalog = put(path.join(base, 'probe1/catalog.json'), { synthetic: 'catalog' });
  const spec = { model: 'deepseek/deepseek-v3.2', endpoint: 'friendli', cap_usd: '0.250000', max_output_tokens: 2048, catalog: catalog.path, catalog_sha256: catalog.sha256 };
  const probeSpec = put(path.join(base, 'probe1/probe-spec.json'), spec);
  const claim = put(oldClaimPath, { spec, spec_sha256: probeSpec.sha256, binary_sha256: conformance.sha256,
    source_sha256: Object.fromEntries(Object.entries(sourceFiles).map(([key, file]) => [key, ref(path.join(old, file)).sha256])) });
  const scope = { workspace: 'workspace', session: 'session', task: 'task' }, ledger = { scope, cap: '250000', settled: '274', active: '0', unresolved: '0', overrun: false };
  const values = { attempt: { scope, id: 'attempt', reservation: 'reservation', phase: 'settled', charged: '274', send_intent: 'intent', provider_request: 'response', quote: { amount: { micros: '249832' } } },
    reservation: { scope, id: 'reservation', attempt: 'attempt', phase: 'settled', amount: { micros: '249832' }, charged: '274', liability: '0' },
    settlement: { scope, attempt: 'attempt', applied: true, direction: 'debit', total: '274', observation: { amount: { micros: '274' }, provider_request: 'response' } }, ledger };
  const records = put(path.join(base, 'probe1/probe-output/canonical-records.json'), Object.entries(values).map(([collection, value]) => ({ collection, value })));
  const result = put(path.join(base, 'probe1/probe-output/result.json'), { schema: 'p6-provider-conformance/1', status: 'failed', error: 'budget exhausted: root cap', scope,
    actual_cost_micros: '274', responses_text_tools: false, ledger });
  const currentBuild = put(path.join(base, 'current-build.json'), { synthetic: 'new verified build' });
  const input = { decision: null, original_allocation: allocation, original_source: originalSource, failed_probe: { binary: conformance, claim, result, records },
    current: { ...originalSpec, build_receipt: currentBuild }, qualification_claim_path: path.join(base, 'probe2/probe-output/claim.json') };
  delete input.current.qualification_claim_path;
  input.failed_probe.inventory = prep.identity(path.dirname(result.path), ['.']);
  const approved = JSON.parse(fs.readFileSync(path.join(__dirname, '../../src/evals/skills/cs3-skill-remediation/qualification-supplement.json')));
  Object.assign(approved, { original_allocation_sha256: allocation.sha256, original_archive_sha256: originalSource.sha256, original_root: old,
    original_conformance_sha256: conformance.sha256, failed_claim_sha256: claim.sha256, failed_result_sha256: result.sha256, failed_records_sha256: records.sha256,
    failed_inventory_sha256: sha(JSON.stringify(input.failed_probe.inventory)),
    qualification_claim_path: input.qualification_claim_path, new_executable_sha256: nativeBinary.sha256, new_native_sha256: native.sha256, new_package_sha256: pkg.sha256 });
  const decisionPath = path.join(root, 'src/evals/skills/cs3-skill-remediation/qualification-supplement.json'), save = () => input.decision = put(decisionPath, approved); save();
  const state = { builds: 0, native: 0, installed: 0 }, fakeSkill = { candidateRegistry: { inspect: () => ({ ...structuredClone(candidates), path: path.join(root, 'candidates') }) }, tasks: () => structuredClone(tasks),
    claimFile: () => path.join(common, 'skill-plan.json'), packageAcceptance: () => { state.installed++; return { synthetic: 'new installed package' }; } };
  const module = { exports: {} }, local = name => {
    if (name === './cs3-document-remediation.cjs') return { ...doc, build(selected, pin) { assert.equal(selected.executable.sha256, pin.executable_sha256); state.builds++; return { synthetic: 'new-source build' }; } };
    if (name === './cs3-skill-remediation.cjs') return fakeSkill;
    if (name === './cs3-controller-recovery-qualification.cjs') return { nativePrerequisites() { state.native++; } };
    if (name === './cs3-comparison.cjs') return { claimFile: () => path.join(common, 'legacy.json') };
    if (name === './cs3-document-remediation-preflight.cjs') return { claimFile: () => path.join(common, 'preflight.json') };
    return realRequire(name);
  };
  local.cache = require.cache;
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(sourceFile, 'utf8'))(local, module, module.exports, path.join(root, 'scripts/evals'));
  const helper = module.exports;
  const campaign = extra => ({ executable: input.current.executable, build_receipt: input.current.build_receipt, skill_remediation: { ...input.current,
    allocation, qualification_supplement: extra } });
  return { helper, input, approved, save, put, ref, root, old, base, common, state, fakeSkill, campaign, result, records, values, candidates };
}
test('fixed supplemental pair preserves the original full allocation and exact observed failure', t => {
  const f = fixture(t), before = fs.readFileSync(f.result.path), proof = f.helper.projection(f.input);
  assert.equal(proof.observed.actual_cost_micros, 274); assert.equal(proof.observed.original_reserved_cap_micros, 250000);
  assert.equal(f.state.installed, 0, 'same executable uses independently rerun archived installed proof');
  const ref = f.helper.reserve(f.put(path.join(f.base, 'input.json'), f.input).path, path.join(f.base, 'supplement'));
  const observed = f.helper.validate(ref, f.campaign(ref));
  assert.equal(observed.combined_cap_micros, 99413737); assert.equal(observed.combined_request_ceiling, 2633);
  const oldSpec = JSON.parse(fs.readFileSync(f.input.failed_probe.claim.path)).spec;
  assert.deepEqual(f.helper.authorizeProbe(ref), { qualification_claim_path: f.input.qualification_claim_path, cap_micros: 500000, request_ceiling: 2, combined_cap_micros: 99413737, combined_request_ceiling: 2633,
    original_catalog: { path: oldSpec.catalog, sha256: oldSpec.catalog_sha256 } });
  assert.deepEqual(fs.readFileSync(f.result.path), before);
  assert.throws(() => f.helper.reserve(f.put(path.join(f.base, 'again.json'), f.input).path, path.join(f.base, 'another')), /Only one/);
  f.put(f.input.qualification_claim_path, {}); assert.throws(() => f.helper.authorizeProbe(ref), /already claimed/);
});
test('failed pair raw hash, scope, liability, multiplicity and exact admission cause are mandatory', t => {
  for (const mutate of [
    f => { const r = JSON.parse(fs.readFileSync(f.result.path)); r.error = 'provider HTTP429'; f.put(f.result.path, r); },
    f => { f.values.attempt.scope = { task: 'foreign' }; }, f => { f.values.ledger.unresolved = '1'; },
    f => { f.values.reservation.amount.micros = '1'; }, f => { f.values.settlement.applied = false; }
  ]) {
    const f = fixture(t); mutate(f);
    f.input.failed_probe.records = f.put(f.records.path, Object.entries(f.values).map(([collection, value]) => ({ collection, value })));
    f.input.failed_probe.result = f.ref(f.result.path);
    f.input.failed_probe.inventory = prep.identity(path.dirname(f.result.path), ['.']); f.approved.failed_inventory_sha256 = sha(JSON.stringify(f.input.failed_probe.inventory));
    f.approved.failed_records_sha256 = f.input.failed_probe.records.sha256; f.approved.failed_result_sha256 = f.input.failed_probe.result.sha256; f.save();
    assert.throws(() => f.helper.failedProbe(f.input, f.approved, JSON.parse(fs.readFileSync(f.input.original_allocation.path))), /eligible|accounting/);
  }
});
test('source archive and cached dependencies cannot stand in for original allocation validation', t => {
  const f = fixture(t); assert(f.helper.archivedAllocation(f.input, f.approved));
  const file = path.join(f.old, 'scripts/evals/cs3-skill-remediation.cjs'); assert.equal(require.cache[file], undefined);
  require.cache[file] = { exports: {} };
  try { assert.throws(() => f.helper.archivedAllocation(f.input, f.approved), /module cache/); } finally { delete require.cache[file]; }
  fs.appendFileSync(file, '\n// changed'); assert.throws(() => f.helper.archivedAllocation(f.input, f.approved), /identity changed|bytes/);
});
test('no claim is created for changed candidate, unpinned executable or protected destination', t => {
  for (const variant of ['candidate', 'executable', 'protected', 'preflight']) {
    const f = fixture(t), input = f.put(path.join(f.base, 'input.json'), f.input); let destination = path.join(f.base, 'new');
    assert(f.helper.projection(f.input));
    if (variant === 'candidate') f.fakeSkill.tasks = () => [{ id: 'changed' }];
    if (variant === 'executable') { f.approved.new_executable_sha256 = null; f.save(); f.put(input.path, f.input); }
    if (variant === 'protected') destination = path.join(f.old, 'new-child');
    if (variant === 'preflight') f.put(path.join(f.common, 'preflight.json'), {});
    assert.throws(() => f.helper.reserve(input.path, destination));
    assert.equal(fs.existsSync(f.helper.claimFile()), false); assert.equal(fs.existsSync(destination), false);
  }
});
test('decision cannot release past reservations, enlarge the fixed pair or waive qualification', t => {
  for (const [key, value] of [['additional_cap_micros', 600000], ['additional_requests', 3], ['preserved_cap_micros', 98663737],
    ['combined_cap_micros', 98913737], ['consumed_liabilities_released', true], ['qualification_waiver', true]]) {
    const f = fixture(t); f.approved[key] = value; f.save(); assert.throws(() => f.helper.decision(f.input.decision), /decision/);
  }
});
