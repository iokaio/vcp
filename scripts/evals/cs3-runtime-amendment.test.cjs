// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const prep = require('./authoring-prepare.cjs'), actualCore = require('./cs3-comparison.cjs'), isolated = require('./cs3-comparison-isolated.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex'), json = file => JSON.parse(fs.readFileSync(file));
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-runtime-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const put = (name, value) => { const file = path.isAbsolute(name) ? name : path.join(directory, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' ? value : JSON.stringify(value)); return { path: file, sha256: sha(fs.readFileSync(file)) }; };
  const root = path.join(directory, 'source'), module = { exports: {} }, filename = require.resolve('./cs3-runtime-amendment.cjs'), actual = createRequire(filename);
  const decision = put(path.join(root, 'src/evals/skills/cs3-runtime-remediation/decision.json'), require('../../src/evals/skills/cs3-runtime-remediation/decision.json'));
  for (const name of ['cs3-comparison-oracle.cjs', 'cs3-runtime-boundary-oracle.cjs']) put(path.join(root, 'scripts/evals', name), fs.readFileSync(path.join(__dirname, name), 'utf8'));
  const state = { source: { exact: 'initial' }, approved: true, preflight: true, qualification: true, runtimeChecks: 0 };
  const skills = require('./cs3-runtime-amendment.cjs').skills;
  const originalTasks = require('../../src/evals/skills/cs3-comparison/cohort.cjs').tasks;
  const old = [...require('../../src/evals/skills/cs3-comparison/cohort-doc-successor.cjs').tasks, ...originalTasks.filter(task => task.skill !== 'document-authoring'),
    ...Array.from({ length: 6 }, (_, i) => ({ id: 'original-web-' + i, skill: 'webapp-testing', kind: ['normal', 'normal', 'boundary', 'hostile', 'missing', 'near_miss'][i],
      request: 'Original untouched WEB request ' + i, nearest: ['testing'], files: { 'source.txt': 'Original facts ' + i }, outputs: [] }))];
  const registry = require('./cs3-comparison-candidates.cjs'), assets = registry.inspect();
  function executionPlan(spec, destination, tasks) {
    const runs = tasks.flatMap((task, index) => Array.from({ length: 3 }, (_, offset) => {
      const arm = ['none', 'nearest', 'candidate'][(index + offset) % 3], id = task.id + '--' + arm;
      return { id, case_id: task.id, skill: task.skill, arm, cap_micros: 600000, call_ceiling: 16,
        skills: arm === 'none' ? [] : arm === 'candidate' ? [registry.qualified(task.skill)] : task.nearest.map(id => `vcp-builtin::${id}::${id}`),
        profile_sha256: sha(JSON.stringify(profile(spec, task, path.join(destination, id, 'workspace'), arm), null, 2) + '\n'),
        prompt_sha256: sha(actualCore.prompt(task)), files: Object.entries(task.files).map(([path, content]) => ({ path, bytes: Buffer.byteLength(content), sha256: sha(content) })) };
    }));
    return { directory: destination, spec, source: state.source, runs, candidate_assets: assets, task_sha256: sha(JSON.stringify(tasks)), executable: spec.executable.path };
  }
  const profile = (spec, _task, workspace) => ({ ...json(spec.profile.path), workspace, maximum_autonomy: 'plan', automatic_effects: [], canonical_tools: ['vcp_read', 'vcp_verify'] });
  const common = path.join(directory, 'git'); fs.mkdirSync(common);
  const core = { ...actualCore, cohort: () => structuredClone(old), claimFile: () => path.join(common, 'original.json'), profile, sourceIdentity: () => state.source,
    validateExecution() { state.runtimeChecks++; }, qualificationWindow: () => {}, planTasks: plan => module.exports.tasks(plan.spec) };
  const doc = { bound(ref) { const bytes = fs.readFileSync(ref.path); if (sha(bytes) !== ref.sha256) throw Error('Evidence changed'); return bytes; },
    reference: file => ({ path: file, sha256: sha(fs.readFileSync(file)) }), executionPlan,
    validateAllocation() { if (!state.approved) throw Error('Synthetic prerequisite denied'); } };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(filename, 'utf8'))(name => {
    if (name === './cs3-comparison.cjs') return core;
    if (name === './cs3-document-remediation.cjs') return doc;
    if (name === './cs3-read-preflight.cjs') return { validateQualification() { if (!state.qualification) throw Error('Synthetic qualification denied'); } };
    if (name === './cs3-document-remediation-preflight.cjs') return { validate() { if (!state.preflight) throw Error('Synthetic preflight denied'); } };
    return actual(name);
  }, module, module.exports, path.join(root, 'scripts/evals'));
  const blank = put('blank.json', {}), sourceProfile = put('profile.json', { deadline_seconds: 600, provider_timeout_seconds: 120, max_requests: 16, output_tokens: '2048', max_transport_retries: 0 });
  const spec = { executable: blank, build_receipt: blank, catalog: blank, node: blank, profile: sourceProfile, gates: {}, web_evidence: [],
    remediation: { decision: blank, prior_terminal: blank, runtime_decision: decision, allocation: blank, qualification: blank, runtime_preflight: blank }, runtime_amendment: { decision } };
  const original = executionPlan(spec, path.join(directory, 'original-slots'), old), zero = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  const groups = skills.map(skill => ({ skill, plan: put('old-' + skill + '.json', { runs: original.runs.filter(row => row.skill === skill), candidate_assets: { ...assets, path: path.join(directory, 'old-main-candidates') } }), undispatched_ids: skill === 'skill-authoring' ? ['old-SKL-unrun'] : [] }));
  spec.remediation.prior_terminal = put('retirement.json', { groups, retired_ids: groups.slice(1).flatMap(group => json(group.plan.path).runs.map(row => row.id)), totals: zero, excluded_ids: ['old-DOC-unrun'] });
  return { directory, put, module: module.exports, spec, state, groups, core, old, decision, assets };
}

test('one shared runtime claim materializes fresh18 and exact transferred72 without replay', t => {
  const f = fixture(t), spec = f.put('spec.json', f.spec), target = path.join(f.directory, 'new-runtime');
  const dry = f.module.prepare(spec.path, target, true);
  assert.equal(dry.runs, 90); assert.equal(dry.transferred, 72); assert.equal(dry.fresh, 18); assert(!fs.existsSync(target)); assert(!fs.existsSync(f.module.claimFile()));
  const ready = f.module.prepare(spec.path, target), manifest = json(ready.manifest.path);
  assert.equal(ready.plans.length, 5); assert.deepEqual(manifest.mapping.map(row => row.id), json(f.spec.remediation.prior_terminal.path).retired_ids);
  assert.equal(manifest.oracle_amendment.affected_case_ids.length, 6); assert.equal(manifest.oracle_amendment.id, 'cs3-runtime-boundary-attribution-v1');
  const ui = manifest.base.runs.filter(row => row.skill === 'frontend-design');
  assert.deepEqual(ui.map(row => row.id), json(f.groups[1].plan.path).runs.map(row => row.id), 'Retain actual original global-cohort arm rotation, not regrouped task-index rotation');
  assert.equal(manifest.base.runs.length, 90); assert.equal(new Set(manifest.base.runs.map(row => row.id)).size, 90);
  const controls = f.module.controlInventory(manifest);
  assert(!controls.scope.includes('slots')); assert(controls.files.every(file => !file.path.startsWith('slots/')));
  for (const ref of ready.plans) {
    const plan = json(ref.path); assert.equal(plan.runs.length, 18); assert.equal(plan.limits.aggregate_micros, 54000000);
    assert.deepEqual(f.module.validate(plan, ref.sha256), plan);
    assert.equal(f.module.admission(plan).reserved_micros, 600000);
    for (const row of plan.runs) {
      assert.equal(json(path.join(plan.directory, row.id, 'profile.json')).deadline_seconds, 600);
      assert.equal(sha(fs.readFileSync(path.join(plan.directory, row.id, 'prompt.txt'))), row.prompt_sha256);
    }
  }
  assert.throws(() => f.module.prepare(spec.path, path.join(f.directory, 'replay')), /one-shot/);
  const first = json(ready.plans[0].path), block = path.join(first.control_directory, 'claims/block-skill-authoring.json');
  const extra = path.join(manifest.base.directory, 'unexpected'); fs.mkdirSync(extra);
  assert.throws(() => f.module.validate(first, ready.plans[0].sha256), /directory inventory/); fs.rmdirSync(extra);
  const prompt = path.join(first.directory, first.runs[0].id, 'prompt.txt'), original = fs.readFileSync(prompt);
  fs.appendFileSync(prompt, 'changed'); assert.throws(() => f.module.validate(first, ready.plans[0].sha256), /input bytes changed/); fs.writeFileSync(prompt, original);
  const active = path.join(first.control_directory, 'active-block.json'); f.put(active, { plan_sha256: '0'.repeat(64), skill: 'skill-authoring' });
  assert.throws(() => f.module.validate(first, ready.plans[0].sha256), /active block ownership/); fs.unlinkSync(active);
  f.put(block, { plan_sha256: ready.plans[0].sha256, skill: 'skill-authoring' });
  assert.throws(() => f.module.validate(first, ready.plans[0].sha256), /transition/); fs.unlinkSync(block);
  f.put(path.join(target, 'global-halt.json'), { failed: true }); assert.throws(() => f.module.validate(first, ready.plans[0].sha256), /integrity halted/);
});

test('runtime preparation denies missing prerequisites, old time bounds and transferred mutations before claim', t => {
  for (const flag of ['approved', 'preflight', 'qualification']) {
    const f = fixture(t); f.state[flag] = false;
    assert.throws(() => f.module.prepare(f.put('spec.json', f.spec).path, path.join(f.directory, 'denied')), /denied/); assert(!fs.existsSync(f.module.claimFile()));
  }
  const f = fixture(t), target = path.join(f.directory, 'new-runtime');
  for (const mutation of [{ deadline_seconds: 180 }, { deadline_seconds: 601 }, { provider_timeout_seconds: 60 }, { provider_timeout_seconds: 121 }, { max_requests: 17 }, { max_transport_retries: 1 }, { output_tokens: '2049' }]) {
    const spec = { ...f.spec, profile: f.put('bad-profile.json', { ...json(f.spec.profile.path), ...mutation }) };
    assert.throws(() => f.module.describe(spec, target), /profile limits/);
  }
  const audit = json(f.spec.remediation.prior_terminal.path), old = json(audit.groups[1].plan.path);
  old.runs[0].prompt_sha256 = '0'.repeat(64); audit.groups[1].plan = f.put('bad-old-plan.json', old);
  assert.throws(() => f.module.describe({ ...f.spec, remediation: { ...f.spec.remediation, prior_terminal: f.put('bad-audit.json', audit) } }, target), /Retired assignment differs/);
  old.runs[0].prompt_sha256 = json(f.groups[1].plan.path).runs[0].prompt_sha256; old.candidate_assets = { changed: true }; audit.groups[1].plan = f.put('bad-old-plan.json', old);
  assert.throws(() => f.module.describe({ ...f.spec, remediation: { ...f.spec.remediation, prior_terminal: f.put('bad-audit.json', audit) } }, target), /candidate bytes changed/);
  assert(!fs.existsSync(f.module.claimFile()));
});

test('frozen runtime source and late DOC terminal evidence cannot be substituted', t => {
  const f = fixture(t), ready = f.module.prepare(f.put('spec.json', f.spec).path, path.join(f.directory, 'runtime')), plan = json(ready.plans[0].path);
  f.state.source = { changed: true }; assert.throws(() => f.module.validate(plan, ready.plans[0].sha256), /Frozen runtime/);
  const empty = f.put('fake-terminal.json', { manifest: ready.manifest, status: 'passed' });
  assert.throws(() => f.module.terminal(empty, { ...f.spec, runtime_amendment: undefined }), /five-group terminal|Frozen|ENOENT|Exact envelope/);
  assert.equal(89063737 - 54000000, plan.runtime_amendment.accounting.fixed_conservative_micros);
});
