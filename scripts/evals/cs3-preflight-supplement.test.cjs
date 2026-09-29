// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
// Only immutable historical proofs and their independently tested validator/build
// are substitutes here. Supplement hash binding, publication, claims and guards
// execute normally; these tests are not native/provider qualification evidence.
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-preflight-supplement-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const put = (name, value) => {
    const file = path.join(directory, name); fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, typeof value === 'string' ? value : JSON.stringify(value));
    return { path: file, sha256: sha(fs.readFileSync(file)) };
  };
  const approved = require('../../src/evals/skills/cs3-runtime-remediation/preflight-supplement.json');
  const fixed = new Map(), pinned = (name, value, digest) => { const ref = put(name, value); ref.sha256 = digest; fixed.set(digest, { path: ref.path, value }); return ref; };
  const common = put('common.json', {}), manifest = put('source.json', { source: 'historical' });
  const executable = pinned('old/vcp.exe', 'not executable', approved.original_executable_sha256);
  const build = pinned('old/build.json', { schema: 'cs3-document-remediation-build/1', status: 'passed', exit_code: 0,
    executable: executable.path, executable_sha256: executable.sha256, qualification_build: true, production_release: false,
    provider_calls: 0, source_manifest: manifest, source_inputs: { source: 'historical' } }, approved.original_build_receipt_sha256);
  const allocation = pinned('old/allocation.json', { schema: 'cs3-document-remediation-allocation/2', directory: path.join(directory, 'old'),
    spec: { decision: common, prior_terminal: common, runtime_decision: common, executable, build_receipt: build },
    cap_micros: 22450000, request_ceiling: 594, combined_cap_micros: 89063737, model_calls: 0 }, approved.original_allocation_sha256);
  const originalSpec = { executable, build_receipt: build, catalog: common, node: common, profile: common,
    remediation: { decision: common, prior_terminal: common, runtime_decision: common, allocation, qualification: common } };
  const plan = pinned('old/plan.json', { spec: originalSpec, source: { content_sha256: approved.original_source_sha256 } }, approved.original_plan_sha256);
  const result = pinned('old/result.json', { plan }, approved.original_failure_sha256);
  const originalFailure = { result, source_archive: path.join(directory, 'archive'), inventory: {}, terminal_response: common };
  const allocationClaim = put('git/original-claim.json', { allocation });
  const decision = put('decision.json', approved);
  const spec = structuredClone(originalSpec); spec.build_receipt = put('new/build.json', { new: true });
  spec.executable = { path: put('new/vcp.exe', 'not executable').path, sha256: executable.sha256 };
  const state = { failed: true, build: true, terminal: true, calls: 0, builds: 0 };
  const file = path.join(__dirname, 'cs3-preflight-supplement.cjs'), actual = createRequire(file), module = { exports: {} };
  const bound = ref => {
    const historical = fixed.get(ref?.sha256);
    if (historical) { assert.equal(ref.path, historical.path); return Buffer.from(typeof historical.value === 'string' ? historical.value : JSON.stringify(historical.value)); }
    const bytes = fs.readFileSync(ref.path); if (sha(bytes) !== ref.sha256) throw Error('Evidence changed'); return bytes;
  };
  const local = name => {
    if (name === './cs3-document-remediation.cjs') return { bound, allocationClaim: () => allocationClaim.path,
      decision: () => ({}), priorTerminal: () => { if (!state.terminal) throw Error('Terminal rejected'); },
      build: () => { state.builds++; if (!state.build) throw Error('Current build rejected'); } };
    if (name === './cs3-document-remediation-preflight.cjs') return { validateFailedPredecessor: (ref, current, expected) => {
      state.calls++; assert.deepEqual(ref, originalFailure); assert.equal(current.executable.sha256, executable.sha256);
      assert.deepEqual(expected, { ordinal: 0, predecessors: [] }); if (!state.failed) throw Error('Raw failure proof rejected');
      return { status: 'conservative_failed_preflight_preserved', conservative_debit_micros: 600000, reserved_requests: 16, actual_cost_micros: null,
        known_settled_micros: 1342, unresolved_micros: 129576, observed_attempts: 3 };
    } };
    if (name === './p6-live-runner.cjs') { const real = actual(name); return { ...real, boundaries: { ...real.boundaries, privateDirectory: () => {}, noParentInstructions: () => {} } }; }
    return actual(name);
  };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(local, module, module.exports, __dirname);
  const input = { decision, original_failure: originalFailure, spec }, specFile = put('input.json', input).path;
  return { directory, put, helper: module.exports, input, spec, specFile, state, destination: path.join(directory, 'supplement'), fixed, approved };
}

test('fixed supplement preserves full original allocation and permits only three bounded replacement slots', t => {
  const f = fixture(t), ref = f.helper.reserve(f.specFile, f.destination); f.spec.remediation.preflight_supplement = ref;
  const observed = f.helper.validate(ref, f.spec);
  assert.equal(observed.additional_cap_micros, 1800000); assert.equal(observed.additional_requests, 48);
  assert.equal(observed.combined_cap_micros, 90863737); assert.equal(observed.combined_requests, 2421);
  assert.equal(observed.slots, 3); assert.equal(observed.slot_requests, 16); assert.equal(observed.slot_cap_micros, 600000);
  assert.equal(observed.original_observation.actual_cost_micros, null); assert.equal(observed.original_observation.conservative_debit_micros, 600000);
  assert.notDeepEqual(observed.original_build_receipt, f.spec.build_receipt); assert.notDeepEqual(observed.original_executable, f.spec.executable);
  assert.equal(f.state.builds, 1); assert.equal(f.state.calls, 2);
  assert.deepEqual(f.helper.validate(f.spec), observed);
  assert.throws(() => f.helper.reserve(f.specFile, path.join(f.directory, 'second')), /one-shot/);
});

test('raw failed predecessor, terminal history and genuine current build are required before publication', t => {
  for (const key of ['failed', 'build', 'terminal']) {
    const f = fixture(t); f.state[key] = false;
    assert.throws(() => f.helper.reserve(f.specFile, f.destination), /rejected/i);
    assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.helper.claimFile()));
  }
});

test('decision, original pins and byte identity cannot be replaced by generic approval', t => {
  let f = fixture(t); f.input.decision = f.put('altered.json', { ...f.approved, slots: 4 }); f.put('input.json', f.input);
  assert.throws(() => f.helper.reserve(f.specFile, f.destination), /Source-pinned/);
  f = fixture(t); f.input.original_failure.result.sha256 = 'a'.repeat(64); f.put('input.json', f.input);
  assert.throws(() => f.helper.reserve(f.specFile, f.destination), /Exact original failed/);
  f = fixture(t); f.spec.executable.sha256 = 'b'.repeat(64); f.put('input.json', f.input);
  assert.throws(() => f.helper.reserve(f.specFile, f.destination), /Same native/);
  f = fixture(t); f.spec.profile = f.put('different-profile.json', {}); f.put('input.json', f.input);
  assert.throws(() => f.helper.reserve(f.specFile, f.destination), /profile identity/);
});

test('validation is read-only and rejects changed current build, receipt, raw proof and global claim', t => {
  const f = fixture(t), ref = f.helper.reserve(f.specFile, f.destination); f.spec.remediation.preflight_supplement = ref;
  const before = fs.readFileSync(ref.path), claim = fs.readFileSync(f.helper.claimFile());
  assert(f.helper.validate(ref, f.spec)); assert.deepEqual(fs.readFileSync(ref.path), before); assert.deepEqual(fs.readFileSync(f.helper.claimFile()), claim);
  const changed = structuredClone(f.spec); changed.build_receipt = f.put('changed-build.json', {});
  assert.throws(() => f.helper.validate(ref, changed), /current supplementary/);
  f.state.failed = false; assert.throws(() => f.helper.validate(ref, f.spec), /Raw failure/); f.state.failed = true;
  fs.writeFileSync(f.helper.claimFile(), '{}'); assert.throws(() => f.helper.validate(ref, f.spec), /exclusive claim/);
  fs.writeFileSync(f.helper.claimFile(), claim); fs.writeFileSync(ref.path, '{}'); assert.throws(() => f.helper.validate(ref, f.spec), /Evidence changed/);
});

test('rehashed receipts cannot enlarge allocation or turn an unknown original cost into settlement', t => {
  for (const change of [receipt => { receipt.slots = 4; }, receipt => { receipt.additional_cap_micros = 2400000; },
    receipt => { receipt.original_observation.actual_cost_micros = 1342; }, receipt => { receipt.extra_approval = true; }]) {
    const f = fixture(t), ref = f.helper.reserve(f.specFile, f.destination), receipt = JSON.parse(fs.readFileSync(ref.path));
    change(receipt); fs.writeFileSync(ref.path, JSON.stringify(receipt)); ref.sha256 = sha(fs.readFileSync(ref.path));
    fs.writeFileSync(f.helper.claimFile(), JSON.stringify({ schema: 'cs3-preflight-supplement-claim/1', allocation: ref }));
    f.spec.remediation.preflight_supplement = ref;
    assert.throws(() => f.helper.validate(ref, f.spec), /accounting or history differs/);
  }
});
