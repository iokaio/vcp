// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const { fixture, json, sha } = require('./cs3-friendli-transfer.integration.test.cjs');
const os = require('node:os'), { createRequire } = require('node:module');

test('fixed decision and exact funded SKL base are authenticated without an extra qualification allowance', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-transfer-policy-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const filename = path.join(__dirname, 'cs3-friendli-transfer.cjs'), actual = createRequire(filename), module = { exports: {} };
  const put = (name, value) => { const file = path.join(directory, name), bytes = JSON.stringify(value); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes); return { path: file, sha256: sha(bytes) }; };
  const decision = json(path.resolve(__dirname, '../../src/evals/skills/cs3-friendli-transfer/decision.json'));
  const retirement = put('retirement.json', {}); decision.retirement_audit_sha256 = retirement.sha256;
  const approved = put('source/src/evals/skills/cs3-friendli-transfer/decision.json', decision), calls = [];
  const base = { skill_remediation: { qualification: put('qualification.json', {}), runtime_preflight: put('preflight.json', {}) }, executable: put('binary', {}) };
  const plan = put('skl/plan.json', { schema: 'cs3-skill-remediation-plan/1', spec: base, directory: path.join(directory, 'skl') });
  const disposition = put('skl/terminal-disposition-skill-authoring.json', { schema: 'cs3-skill-remediation-terminal-disposition/1', status: 'terminal_unqualified' });
  const spec = { ...base, friendli_transfer: { decision: approved, retirement, skill_terminal: { plan, disposition } } };
  const local = name => {
    if (name === './cs3-skill-remediation.cjs') return { validateSpec(value) { assert.deepEqual(value, base); calls.push(value); } };
    if (name === './cs3-friendli-retirement.cjs') return { validate(ref) { assert.deepEqual(ref, retirement); return { groups: [{}, {}, {}, {}], document: { runs: Array.from({ length: 18 }) } }; } };
    if (name === './cs3-comparison.cjs') return { controlDirectory: value => value.directory };
    if (name === './cs3-comparison-review.cjs') return { validateTerminalDisposition(file, hash) { assert.equal(file, plan.path); assert.equal(hash, plan.sha256); return json(disposition.path); } };
    return actual(name);
  };
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(filename, 'utf8'))(local, module, module.exports, path.join(directory, 'source/scripts/evals'));
  const helper = module.exports; assert.equal(helper.validateSpec(spec).terminal.status, 'terminal_unqualified'); assert.equal(calls.length, 1);
  assert.throws(() => helper.validateSpec({ ...spec, friendli_transfer: { ...spec.friendli_transfer, extra_probe: true } }), /selector/);
  assert.throws(() => helper.validateSpec({ ...spec, friendli_transfer: { ...spec.friendli_transfer, retirement: { ...retirement, sha256: 'a'.repeat(64) } } }), /audit is not approved/);
  for (const [field, value] of [['aggregate_micros', 54000001], ['fixed_conservative_micros', 44913736], ['combined_request_ceiling', 2632], ['qualification_waiver', true], ['retirement_audit_sha256', null]]) {
    const changed = { ...decision, [field]: value }, reference = put('source/src/evals/skills/cs3-friendli-transfer/decision.json', changed);
    assert.throws(() => helper.decision(reference), /Exact pinned/);
  }
});

test('each original UI, MCP and LLM hostile marker remains local only when supplied and descriptor-bound', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-transfer-canary-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const filename = path.join(__dirname, 'cs3-friendli-transfer.cjs'), actual = createRequire(filename), module = { exports: {} };
  const tasks = actual('../../src/evals/skills/cs3-comparison/cohort.cjs').tasks.filter(task => task.kind === 'hostile' && ['frontend-design', 'mcp-development', 'llm-integration'].includes(task.skill));
  new Function('require', 'module', 'exports', '__dirname', '__tasks', fs.readFileSync(filename, 'utf8') + '\ntasks = () => __tasks; module.exports.canary = canary;')(actual, module, module.exports, __dirname, tasks);
  assert.equal(tasks.length, 3);
  for (const task of tasks) {
    const row = { id: task.id + '--none', case_id: task.id }, base = path.join(directory, row.id), id = 'response-' + task.id;
    fs.mkdirSync(base); assert.equal(task.forbidden_literals, undefined);
    const marker = 'CS3_' + task.id.split('-')[0] + '_PRIVATE_CANARY_82D4', response = value => [value.slice(0, 10), value.slice(10)].map(delta => 'data: ' + JSON.stringify({ type: 'response.output_text.delta', response_id: 'r', item_id: 'a', delta })).join('\n\n') + '\n\n';
    const writeResponse = value => { const bytes = Buffer.from(response(value)); fs.writeFileSync(path.join(base, 'response-' + sha(id) + '.sse'), bytes);
      fs.writeFileSync(path.join(base, 'outputs.json'), JSON.stringify([{ items: [{ collection: 'artifact', id, record: { spec: { channel: 'response' }, sha256: sha(bytes), length: String(bytes.length) } }] }])); };
    writeResponse(marker); assert.equal(module.exports.canary({ directory, spec: {} }, row), true);
    writeResponse('CS3_SKL_PRIVATE_CANARY_82D4'); assert.equal(module.exports.canary({ directory, spec: {} }, row), false);
    fs.appendFileSync(path.join(base, 'response-' + sha(id) + '.sse'), marker); assert.throws(() => module.exports.canary({ directory, spec: {} }, row), /response changed/);
  }
});

test('transfer projection carries the fixed qualification supplement without adding comparison slots', () => {
  const helper = require('./cs3-friendli-transfer.cjs'), reference = { path: 'unused-manifest', sha256: 'a'.repeat(64) };
  const manifest = { directory: 'unused-transfer', spec: { skill_remediation: {} }, base: { runs: [{ id: 'one', skill: 'frontend-design' }] } };
  const ordinary = helper.project(manifest, reference, 'frontend-design');
  manifest.spec.skill_remediation.qualification_supplement = { path: 'unvalidated-projection-only', sha256: 'b'.repeat(64) };
  const supplemented = helper.project(manifest, reference, 'frontend-design');
  assert.equal(ordinary.friendli_transfer.accounting.fixed_conservative_micros, 44913737);
  assert.equal(supplemented.friendli_transfer.accounting.fixed_conservative_micros, 45413737);
  assert.equal(supplemented.friendli_transfer.accounting.outer_cap_micros, 100000000);
  assert.deepEqual(supplemented.runs, ordinary.runs);
  assert.deepEqual(supplemented.limits, ordinary.limits);
});

test('ninety exact old assignments preserve global arm order, candidate identities and literal WEB input', t => {
  const f = fixture(t), manifest = json(f.prepared.manifest.path);
  assert.equal(manifest.base.runs.length, 90); assert.equal(manifest.mapping.length, 90);
  for (const group of f.state.proof.groups) {
    const plan = json(f.prepared.plans.find(row => row.skill === group.skill).path);
    assert.deepEqual(plan.runs.map(row => row.id), group.plan.runs.map(row => row.id));
    assert.equal(plan.isolated, undefined); assert.equal(plan.skill_remediation, undefined);
  }
  assert.deepEqual(manifest.base.runs.slice(72).map(row => row.id), f.state.proof.document.runs.map(row => row.id));
  const web = f.helper.tasks(f.spec).filter(task => task.skill === 'webapp-testing');
  assert(web.every(task => task.files['host-browser-evidence.json'].includes('case_id')));
  assert.deepEqual(f.helper.baseSpec(f.spec), Object.fromEntries(Object.entries(f.spec).filter(([key]) => key !== 'friendli_transfer')));
  const old = f.state.proof.groups[0].plan.runs[0].prompt_sha256;
  f.state.proof.groups[0].plan.runs[0].prompt_sha256 = 'b'.repeat(64);
  assert.throws(() => f.helper.describe(f.spec, manifest.directory), /assignment changed/); f.state.proof.groups[0].plan.runs[0].prompt_sha256 = old;
  assert.throws(() => f.helper.prepare(path.join(f.directory, 'spec.json'), path.join(f.directory, 'duplicate')), /one-shot/);
});

test('shared serial claims fence concurrent handoff and refuse mutated pristine slots', t => {
  const f = fixture(t), first = f.prepared.plans[0], second = f.prepared.plans[1];
  assert.throws(() => f.helper.begin(json(second.path), second.sha256), /skill order/);
  f.helper.begin(f.plan, first.sha256);
  assert.throws(() => f.helper.begin(f.plan, first.sha256), /EEXIST/);
  const active = path.join(path.dirname(f.prepared.manifest.path), 'active-skill.json'), bytes = fs.readFileSync(active), changed = JSON.parse(bytes);
  changed.skill = second.skill; fs.writeFileSync(active, JSON.stringify(changed));
  assert.throws(() => f.helper.validate(f.plan, first.sha256), /owner\/claim/); fs.writeFileSync(active, bytes);
  const target = path.join(f.plan.directory, f.plan.runs[1].id, 'prompt.txt'), original = fs.readFileSync(target);
  fs.writeFileSync(target, 'modified'); assert.throws(() => f.helper.validate(f.plan, first.sha256), /Undispatched|input bytes/); fs.writeFileSync(target, original);
  const root = path.dirname(f.prepared.manifest.path), unknown = path.join(root, 'slots', 'unknown'); fs.mkdirSync(unknown);
  assert.throws(() => f.helper.validate(f.plan, first.sha256), /ninety-slot inventory/); fs.rmdirSync(unknown);
  assert.equal(f.helper.controlInventory(json(f.prepared.manifest.path)).files.some(file => file.path.startsWith('slots/')), false);
});

test('destination cannot overlap any protected old or fresh SKL evidence even before exclusive claim', t => {
  const f = fixture(t), proof = f.state.proof;
  proof.oldRoot = path.join(f.directory, 'old-source');
  const claim = fs.readFileSync(f.helper.claimFile()), before = fs.readdirSync(f.directory);
  for (const protectedRoot of [proof.oldRoot, path.join(f.directory, 'prior-skl'), path.join(f.directory, 'historical-source')]) {
    assert.throws(() => f.helper.protectedDestination(f.spec, path.join(protectedRoot, 'new-unconsumed'), proof), /overlaps protected/);
    assert.equal(fs.existsSync(path.join(protectedRoot, 'new-unconsumed')), false);
  }
  assert.deepEqual(fs.readFileSync(f.helper.claimFile()), claim); assert.deepEqual(fs.readdirSync(f.directory), before);
});
