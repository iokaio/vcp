// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const file = path.join(__dirname, 'cs3-skill-remediation.cjs'), actualRequire = createRequire(file), real = actualRequire('./cs3-skill-remediation.cjs');
const doc = actualRequire('./cs3-document-remediation.cjs'), prep = actualRequire('./authoring-prepare.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex'), sourceRoot = path.resolve(__dirname, '../..');
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-skl-fixed-')), root = path.join(directory, 'source'), old = path.join(directory, 'old'), common = path.join(directory, 'common');
  t.after(() => fs.rmSync(directory, { recursive: true, force: true })); fs.mkdirSync(common);
  const ref = p => ({ path: p, sha256: sha(fs.readFileSync(p)) });
  const put = (p, value) => { fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value)); return ref(p); };
  const decisionFile = path.join(root, 'src/evals/skills/cs3-skill-remediation/decision.json'), approved = JSON.parse(fs.readFileSync(path.join(sourceRoot, 'src/evals/skills/cs3-skill-remediation/decision.json')));
  fs.cpSync(path.join(sourceRoot, 'src/evals/skills/cs3-skill-remediation/candidates'), path.join(root, 'src/evals/skills/cs3-skill-remediation/candidates'), { recursive: true });
  const oldSource = path.join(directory, 'old-source'), archiveRoot = path.join(directory, 'archive'), reviewed = path.join(directory, 'prior-review');
  for (const selected of [oldSource, archiveRoot, reviewed]) fs.mkdirSync(selected);
  const state = { historical: true, native: true, calls: 0 }, fakeCore = {
    claimFile: () => path.join(common, 'old.json'), workspaceFiles: base => prep.identity(path.join(base, 'workspace'), ['.']).files.map(row => ({ ...row, path: row.path.slice(2) })).sort((a, b) => a.path.localeCompare(b.path)),
    qualificationWindow: profile => actualRequire('./cs3-comparison.cjs').qualificationWindow(profile),
    profile: spec => JSON.parse(doc.bound(spec.profile)), prompt: task => task.request,
    claimed: (plan, id) => !!plan.money?.[id], slotReport: (plan, id) => plan.money[id]
  };
  const module = { exports: {} }, local = name => {
    if (name === './cs3-comparison.cjs') return fakeCore;
    // Synthetic native/prerequisite/build substitutes are not qualification.
    // Actual file/hash/package/log/release/ownership/allocation checks run.
    if (name === './cs3-controller-recovery-qualification.cjs') return { skillRemediationPrerequisites() { assert(state.historical, 'Historical validator failed'); return { terminal: 'synthetic-unqualified', model_calls: 0, protected_inventories: [{ directory: reviewed, inventory: {} }] }; }, nativePrerequisites() { assert(state.native, 'Native validator failed'); return { status: 'synthetic' }; } };
    if (name === './cs3-read-preflight.cjs' || name === './cs3-document-remediation-preflight.cjs') return { validateSkillQualification: () => ({ status: 'passed' }), validate: () => ({ status: 'passed' }) };
    if (name === './cs3-comparison-isolated.cjs') return { safeReport: (plan, row, report) => report };
    if (name === './cs3-document-remediation.cjs') return { ...doc, build: () => ({ status: 'synthetic' }), executionPlan(spec, destination, tasks) {
      return { directory: destination, spec, runs: tasks.flatMap((task, i) => ['none', 'nearest', 'candidate'].map((_, j) => ({ id: task.id + '--' + ['none', 'nearest', 'candidate'][(i + j) % 3], case_id: task.id, skill: task.skill, arm: ['none', 'nearest', 'candidate'][(i + j) % 3] }))) };
    } };
    return actualRequire(name);
  };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(local, module, module.exports, path.join(root, 'scripts/evals'));
  const helper = module.exports, ids = approved.released_ids, controls = path.join(old, 'control'); fs.mkdirSync(path.join(controls, 'claims'), { recursive: true });
  const rows = ids.map(id => { const base = path.join(old, 'slots', id); fs.mkdirSync(path.join(base, 'data'), { recursive: true }); put(path.join(base, 'workspace/input.txt'), id);
    return { id, skill: 'skill-authoring', cap_micros: 600000, call_ceiling: 16, profile_sha256: put(path.join(base, 'profile.json'), {}).sha256,
      prompt_sha256: put(path.join(base, 'prompt.txt'), id).sha256, files: fakeCore.workspaceFiles(base) }; });
  const oldPlan = put(path.join(old, 'plan.json'), { directory: path.join(old, 'slots'), control_directory: controls, runs: rows });
  const retired = Array.from({ length: 72 }, (_, i) => 'retired-' + i), halt = put(path.join(old, 'halt.json'), { action: 'All original isolated dispatch permanently stopped; no replay.', retired_ids: retired });
  const retirementClaim = put(path.join(old, 'retirement-claim.json'), { retired_ids: retired });
  const audit = { groups: [{ skill: 'skill-authoring', status: 'terminal_unqualified', plan: oldPlan, undispatched_ids: ids, claimed_ids: Array.from({ length: 12 }, (_, i) => 'consumed-' + i) }], retired_ids: retired, retirement_halt: halt, retirement_claim: retirementClaim };
  const retirement = put(path.join(old, 'audit.json'), audit), history = put(path.join(directory, 'history.json'), { retirement, terminal_disposition: halt });
  // Only the hardcoded historical audit digest is replaced at this explicit
  // synthetic boundary; the real retirement slot inventories remain checked.
  const originalBound = doc.bound, historicalPin = approved.retirement_audit_sha256;
  const pinnedHistory = { retirement: { ...retirement, sha256: historicalPin }, terminal_disposition: halt,
    archive: put(path.join(directory, 'archive-receipt.json'), { archive: archiveRoot }) };
  const wrappedDoc = local('./cs3-document-remediation.cjs'); wrappedDoc.bound = (reference, ...args) => reference.path === retirement.path && reference.sha256 === historicalPin ? fs.readFileSync(retirement.path) : originalBound(reference, ...args);
  const second = { exports: {} }, secondRequire = name => name === './cs3-document-remediation.cjs' ? wrappedDoc : local(name);
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(file, 'utf8'))(secondRequire, second, second.exports, path.join(root, 'scripts/evals'));
  const h = second.exports;
  const native = put(path.join(directory, 'native.json'), { decision: put(path.join(directory, 'recovery-decision.json'), { historical_root: oldSource }), node: halt, boundary: halt, node_fixture: halt, web: halt, ui_matrix: halt, web_evidence: [] });
  const binary = put(path.join(directory, 'installed/vcp.exe'), 'synthetic native executable'), catalog = put(path.join(directory, 'installed/skills/builtin/catalog.json'), { skills: [] });
  const runner = put(path.join(root, 'scripts/evals/cs3-installed-skills-qualification.ps1'), 'synthetic qualification runner'), testSource = put(path.join(root, 'src/crates/vcp-cli/tests/executable.rs'), 'synthetic tests');
  const oldStage = { release: 'old', executable_sha256: 'a'.repeat(64), catalog_sha256: 'b'.repeat(64), protected_data_unchanged: true }, newStage = { release: 'new', executable_sha256: binary.sha256, catalog_sha256: catalog.sha256, protected_data_unchanged: true };
  const installation = put(path.join(directory, 'installation.json'), { schema: 'cs-authoring-package-qualification/1', status: 'pass', executable_sha256: binary.sha256,
    archive_sha256: 'c'.repeat(64), previous_archive_sha256: 'd'.repeat(64), installed_candidate: path.dirname(binary.path),
    stages: [{ ...oldStage, action: 'Install' }, { ...newStage, action: 'Upgrade' }, { ...oldStage, action: 'Rollback' }, { ...newStage, action: 'Upgrade' }] });
  const oldCandidates = actualRequire('./cs3-comparison-candidates.cjs').inspect(), docCandidates = doc.candidateRegistry.inspect(), skl = h.candidateRegistry.inspect();
  const candidates = oldCandidates.entries.flatMap(oldEntry => { const registry = oldEntry.id === 'skill-authoring' ? skl : oldEntry.id === 'document-authoring' ? docCandidates : oldCandidates;
    const entry = registry.entries.find(row => row.id === oldEntry.id); return registry.files.filter(row => row.path.startsWith(entry.id + '/')).map(row => ({ skill: entry.id, version: entry.version, path: row.path.slice(entry.id.length + 1), sha256: row.sha256 })); });
  const filters = ['executable_six_candidate', 'executable_packaged_skills_are_relocatable_lazy_and_integrity_checked', 'executable_skills_inspection_is_lazy_without_provider_or_budget_admission', 'executable_terminal_skill_activation_reports_source_version_reason_and_setup_failures'];
  const installed = put(path.join(directory, 'acceptance/result.json'), { schema: 'cs3-installed-skills-qualification/1', status: 'pass', inputs_unchanged: true, paid_requests: 0,
    installation_report_sha256: installation.sha256, executable_sha256: binary.sha256, archive_sha256: 'c'.repeat(64), catalog_sha256: catalog.sha256,
    runner_sha256: runner.sha256, test_source_sha256: testSource.sha256, candidates_before: candidates, candidates_after: candidates,
    stages: filters.map((filter, index) => ({ filter, exit_code: 0, log_sha256: put(path.join(directory, 'acceptance', filter + '.log'), 'test result: ok. ' + (index ? 1 : 2) + ' passed; 0 failed; 0 ignored;').sha256 })) });
  const pkg = put(path.join(directory, 'package.json'), { installation, installed });
  const selected = { decision: null, history: put(history.path, pinnedHistory), recovery_native: native, package_acceptance: pkg, executable: binary, build_receipt: put(path.join(directory, 'build.json'), {}) };
  Object.assign(approved, { history_sha256: selected.history.sha256, recovery_native_sha256: native.sha256, package_acceptance_sha256: pkg.sha256,
    candidate_inventory_sha256: sha(JSON.stringify(h.candidateRegistry.inspect())), cohort_sha256: sha(JSON.stringify(h.tasks())), executable_sha256: binary.sha256 });
  const saveDecision = () => { selected.decision = put(decisionFile, approved); return selected.decision; }; saveDecision();
  return { helper: h, directory, root, old, oldSource, archiveRoot, reviewed, common, put, ref, approved, selected, saveDecision, audit, retirement, pinnedHistory, state, installed, pkg,
    allocationInput: () => ({ ...selected, qualification_claim_path: path.join(directory, 'qualification/probe-claim.json') }) };
}
test('exact held-out SKL package and six-task envelope remain separate and closed', () => {
  assert.equal(real.candidateRegistry.inspect().entries[0].version, '1.0.3'); assert.equal(real.tasks().length, 6);
  assert.equal(real.tasks().filter(row => row.kind === 'normal').length, 2); assert.throws(() => real.candidateRegistry.qualified('document-authoring'));
  const p = path.join(sourceRoot, 'src/evals/skills/cs3-skill-remediation/decision.json'), r = { path: p, sha256: sha(fs.readFileSync(p)) };
  assert.equal(real.decision(r).combined_cap_micros, 98913737);
  assert.throws(() => real.prerequisites({ decision: r, executable: { sha256: 'a'.repeat(64) } }), /pin/);
});
test('release proof admits exactly six pristine non-transferred slots', t => {
  const f = fixture(t), proof = f.helper.releaseProof(f.pinnedHistory, f.approved); assert.equal(proof.rows.length, 6); assert.equal(proof.released_cap_micros, 3600000);
  const first = f.approved.released_ids[0], data = path.join(f.old, 'slots', first, 'data', 'attempt.json'); f.put(data, {});
  assert.throws(() => f.helper.releaseProof(f.pinnedHistory, f.approved), /claims, effects/); fs.unlinkSync(data);
  f.put(path.join(f.old, 'control/claims', first + '.json'), {}); assert.throws(() => f.helper.releaseProof(f.pinnedHistory, f.approved), /claims, effects/);
});
test('already-transferred or altered undispatched identities cannot be released twice', t => {
  const f = fixture(t); f.audit.retired_ids[0] = f.approved.released_ids[0]; f.put(f.retirement.path, f.audit);
  assert.throws(() => f.helper.releaseProof(f.pinnedHistory, f.approved), /never-transferred/);
  f.audit.retired_ids[0] = 'retired-0'; f.audit.groups[0].undispatched_ids = f.approved.released_ids.slice(1); f.put(f.retirement.path, f.audit);
  assert.throws(() => f.helper.releaseProof(f.pinnedHistory, f.approved), /six never/);
});
test('actual raw installed log and exact candidate bytes are mandatory', t => {
  const f = fixture(t); assert.equal(f.helper.packageAcceptance(f.pkg, f.selected.executable.sha256).candidates.filter(row => row.skill === 'skill-authoring')[0].version, '1.0.3');
  const record = JSON.parse(fs.readFileSync(f.installed.path)); record.stages[0].exit_code = 1; const bad = f.put(f.installed.path, record);
  const wrapper = JSON.parse(fs.readFileSync(f.pkg.path)); wrapper.installed = bad;
  assert.throws(() => f.helper.packageAcceptance(f.put(f.pkg.path, wrapper), f.selected.executable.sha256), /raw test log/);
});
test('fresh one-use allocation binds observed predecessor and never modifies it', t => {
  const f = fixture(t), before = fs.readFileSync(f.retirement.path), input = f.put(path.join(f.directory, 'reserve-input.json'), f.allocationInput());
  const allocation = f.helper.reserve(input.path, path.join(f.directory, 'allocation'));
  const spec = { executable: f.selected.executable, build_receipt: f.selected.build_receipt, skill_remediation: { ...f.selected, allocation } };
  assert.equal(f.helper.validateAllocation(spec).combined_cap_micros, 98913737); assert.deepEqual(fs.readFileSync(f.retirement.path), before);
  assert.throws(() => f.helper.reserve(input.path, path.join(f.directory, 'again')), /Fresh external/);
  f.state.historical = false; assert.throws(() => f.helper.validateAllocation(spec), /Historical validator/);
});

test('funded probe claims and allocation directories cannot overlap protected source or retained evidence', t => {
  for (const key of ['root', 'oldSource', 'archiveRoot', 'reviewed', 'old']) {
    const f = fixture(t), before = prep.identity(f[key], ['.']);
    const input = f.allocationInput(); input.qualification_claim_path = path.join(f[key], ...(key === 'old' ? ['slots', f.approved.released_ids[0]] : []), 'fresh-probe-claim.json');
    const source = f.put(path.join(f.directory, 'reserve-input.json'), input), destination = path.join(f.directory, 'allocation');
    assert.throws(() => f.helper.reserve(source.path, destination), /overlaps protected/);
    assert(!fs.existsSync(f.helper.claimFile('allocation'))); assert(!fs.existsSync(destination));
    assert.deepEqual(prep.identity(f[key], ['.']), before);
  }
  const f = fixture(t), input = f.put(path.join(f.directory, 'reserve-input.json'), f.allocationInput());
  assert.throws(() => f.helper.reserve(input.path, path.join(f.reviewed, 'new-allocation')), /overlaps protected/);
  assert(!fs.existsSync(f.helper.claimFile('allocation')));
});

test('allocation revalidation rejects a rehashed funded claim relocated into protected history', t => {
  const f = fixture(t), input = f.put(path.join(f.directory, 'reserve-input.json'), f.allocationInput());
  const original = f.helper.reserve(input.path, path.join(f.directory, 'allocation')), record = JSON.parse(fs.readFileSync(original.path));
  record.spec.qualification_claim_path = path.join(f.reviewed, 'new-claim.json');
  const changed = f.put(original.path, record); f.put(f.helper.claimFile('allocation'), { allocation: changed, released_ids: f.approved.released_ids });
  assert.throws(() => f.helper.validateAllocation({ executable: f.selected.executable, build_receipt: f.selected.build_receipt,
    skill_remediation: { ...f.selected, allocation: changed } }), /overlaps protected/);
  assert(!fs.existsSync(record.spec.qualification_claim_path));
});
test('unqualified native evidence and increased budget reject before allocation claim', t => {
  const f = fixture(t), input = f.put(path.join(f.directory, 'reserve-input.json'), f.allocationInput());
  f.state.native = false; assert.throws(() => f.helper.reserve(input.path, path.join(f.directory, 'allocation')), /Native validator/);
  assert.equal(fs.existsSync(f.helper.claimFile('allocation')), false);
  f.approved.combined_cap_micros++; const decision = f.saveDecision(); assert.throws(() => f.helper.decision(decision), /fixed SKL/);
});
test('canonical pending observations retain full slot debit and null actual cost', t => {
  const f = fixture(t), money = { known_settled_micros: 1200, conservative_debit_micros: 600000, unresolved_liability_micros: 129576, unresolved_attempts: 1, attempts: [{}, {}] };
  const plan = { runs: [{ id: 'first' }], money: { first: money } }, admitted = f.helper.admission(plan);
  assert.equal(admitted.actual_cost_micros, null); assert.equal(admitted.conservative_debit_micros, 600000); assert.equal(admitted.reserved_micros, 600000);
  plan.runs = Array.from({ length: 18 }, (_, i) => ({ id: '' + i })); plan.money = Object.fromEntries(plan.runs.map(row => [row.id, money]));
  assert.throws(() => f.helper.admission(plan), /reservation exhausted/);
});
test('eighteen fixed slots prepare once and reject foreign block ownership or changed frozen plan', t => {
  const f = fixture(t), reserveInput = f.allocationInput(), input = f.put(path.join(f.directory, 'reserve-input.json'), reserveInput);
  const allocation = f.helper.reserve(input.path, path.join(f.directory, 'allocation'));
  const qualificationClaim = f.put(reserveInput.qualification_claim_path, { synthetic: true }), native = JSON.parse(fs.readFileSync(f.selected.recovery_native.path));
  const profile = f.put(path.join(f.directory, 'profile.json'), { deadline_seconds: 600, provider_timeout_seconds: 120, max_requests: 16, output_tokens: '2048', max_transport_retries: 0,
    provider: { valid_until: Date.now() + 86400000, compatibility: { model: 'deepseek/deepseek-v3.2', endpoint: 'friendli', valid_until: Date.now() + 86400000 }, price: { valid_until: Date.now() + 86400000 } } });
  const spec = { executable: f.selected.executable, build_receipt: f.selected.build_receipt, profile, catalog: native.node, node: native.node,
    gates: { browser_boundary: native.boundary, node_fixture: native.node_fixture, web_oracles: native.web, ui_qualification: native.ui_matrix }, web_evidence: [],
    skill_remediation: { decision: f.selected.decision, allocation, history: f.selected.history, recovery_native: f.selected.recovery_native, package_acceptance: f.selected.package_acceptance,
      qualification: f.put(path.join(f.directory, 'qualification.json'), { probe_claim: qualificationClaim }), runtime_preflight: native.node } };
  const specRef = f.put(path.join(f.directory, 'spec.json'), spec), destination = path.join(f.directory, 'campaign');
  const wrong = structuredClone(spec); wrong.skill_remediation.qualification = f.put(path.join(f.directory, 'foreign-qualification.json'), { probe_claim: { ...qualificationClaim, path: path.join(f.directory, 'unfunded-claim.json') } });
  assert.throws(() => f.helper.validateQualification(wrong), /funded SKL qualification/);
  assert.equal(f.helper.prepare(specRef.path, destination, true).status, 'validated_not_claimed'); assert.equal(fs.existsSync(f.helper.claimFile()), false);
  const planRef = f.helper.prepare(specRef.path, destination), plan = JSON.parse(fs.readFileSync(planRef.path));
  assert.equal(plan.runs.length, 18); assert.equal(new Set(plan.runs.map(row => row.id)).size, 18);
  assert.equal(f.helper.validate(plan, planRef.sha256), plan);
  assert.throws(() => f.helper.prepare(specRef.path, path.join(f.directory, 'again')), /one-shot SKL/);
  const block = path.join(destination, 'claims/block-skill-authoring.json'); f.put(block, { plan_sha256: 'a'.repeat(64), skill: 'skill-authoring' });
  assert.throws(() => f.helper.validate(plan, planRef.sha256), /owner differs/);
  f.put(block, { plan_sha256: planRef.sha256, skill: 'skill-authoring' });
  plan.runs[0].arm = 'foreign'; const changed = f.put(planRef.path, plan); assert.throws(() => f.helper.validate(plan, changed.sha256), /frozen SKL/);
});
