// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), os = require('node:os');
const { ownedRoot } = require('../support/experiments.cjs');
const runner = require('../../../scripts/evals/authoring-requalification-runner.cjs');
const candidates = require('../../../scripts/evals/authoring-requalification-candidates.cjs');
const prep = require('../../../scripts/evals/authoring-prepare.cjs');
const review = require('../../../scripts/evals/authoring-requalification-review-runner.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const all = runner.tasks(), slots = runner.slots(all);

test('fixed 54-slot rq1 cohort uses four fresh v4 normals and twelve inherited v2 tasks', () => {
  assert.equal(all.length, 16); assert.equal(slots.length, 54); assert.equal(new Set(slots.map(row => row.id)).size, 54);
  assert.deepEqual(all.filter(item => item.fresh).map(item => item.task.id).sort(), [
    'DOC-requal-retention-matrix-v4', 'DOC-requal-rollout-brief-v4', 'SKL-requal-audit-package-v4', 'SKL-requal-prune-resource-v4']);
  assert(all.filter(item => !item.fresh).every(item => item.task.id.endsWith('-v2')));
  assert.equal(slots.reduce((sum, row) => sum + row.cap_micros, 0), 94500000);
  assert.equal(slots.reduce((sum, row) => sum + row.call_ceiling, 0), 864);
  for (const candidate of candidates.ids) {
    assert.deepEqual(['normal', 'inherited', 'confirmation'].map(phase => slots.filter(row => row.candidate === candidate && row.phase === phase).length), [6, 18, 3]);
    for (let triplet = 0; triplet < 9; triplet++) assert.deepEqual(slots.filter(row => row.candidate === candidate && row.triplet === triplet).map(row => row.arm).sort(), ['candidate', 'nearest', 'none']);
  }
  assert(slots.every(row => row.id.startsWith('rq1--') && row.output_tokens === '2048'));
  assert(slots.filter(row => row.phase === 'confirmation').every(row => row.case_id === null));
  assert(slots.filter(row => row.phase === 'normal').every(row => row.case_id.endsWith('-v4')));
  assert(slots.filter(row => row.phase === 'inherited').every(row => row.case_id.endsWith('-v2')));
});
test('fresh slot IDs cannot replay original qualification or successor slots', () => {
  const old = require('../../../scripts/evals/authoring-qualification.cjs').slots();
  assert(slots.every(row => !old.some(prior => prior.id === row.id)));
  assert.equal(new Set([...old.map(row => row.id), ...slots.map(row => row.id)]).size, old.length + 54);
});
function planned(t, candidate = 'skill-authoring') {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const profile = path.join(owner.root, 'source-profile.json');
  fs.writeFileSync(profile, JSON.stringify({ maximum_autonomy: 'workspace', automatic_effects: ['read', 'write'], canonical_tools: ['vcp_list', 'vcp_read', 'vcp_search', 'vcp_patch', 'vcp_verify'] }));
  const runtime = { checker: path.join(owner.root, 'never-executed-checker.exe'), system_root: owner.root };
  const rows = runner.planRows({ slots, profile_source: profile, provider_catalog: path.join(owner.root, 'unused-catalog.json') }, candidate, 'normal', null, owner.root, runtime, all);
  return { root: owner.root, rows };
}
test('candidate rows install only the exact new source and versions; baselines select no candidate', t => {
  assert.deepEqual(candidates.versions, { 'document-authoring': '1.0.3', 'skill-authoring': '1.0.2' });
  for (const id of candidates.ids) for (const row of planned(t, id).rows) {
    const task = all.find(item => item.task.id === row.case_id).task;
    assert.equal(row.skill, candidates.selection(row.arm, task));
    if (row.arm === 'candidate') {
      assert.deepEqual(row.profile.skills, candidates.configuration(id));
      assert.equal(row.profile.skills.sources[0].id, 'vcp-authoring-requalification-candidates');
    } else assert.equal(row.profile.skills, undefined);
    assert.deepEqual(row.profile.canonical_tools, task.context.tools);
  }
});
test('oracle routing uses frozen cohort membership, rejecting old normals and unknown v4 IDs', () => {
  for (const item of all) assert.equal(runner.oracleFor(item.task.id), require(item.fresh ? '../../../scripts/evals/authoring-requalification-oracle.cjs' : '../../../scripts/evals/authoring-oracle.cjs'));
  for (const id of ['DOC-fresh-acceptance-plan-v1', 'SKL-followup-create-v3', 'DOC-invented-v4']) assert.throws(() => runner.oracleFor(id), /Unknown/);
});
function context(skill, parts, mutate = () => {}) {
  const manifest = { request_sha256: 'request-digest', included: parts.map((part, index) => ({ kind: 'skill', id: 'skill-' + sha(Buffer.from(skill)) + '-' + index, source_hash: part.sha256, trust: 'active_skill' })) };
  mutate(manifest); const bytes = Buffer.from(JSON.stringify(manifest));
  const item = { collection: 'artifact', id: 'context', record: { state: 'complete', length: String(bytes.length), sha256: sha(bytes), spec: { schema: 'context-manifest/1' } } };
  const pages = [{ items: [item], gaps: [] }];
  const call = (exe, args, timeout) => {
    assert.equal(exe, 'never-executed'); assert(args.includes('inspect')); assert(!args.includes('run')); assert.equal(timeout, 30000);
    return { status: 0, stdout: JSON.stringify({ type: 'result', data: { items: [{ artifact: item.id, visibility: 'available', range: { start: 0, end: bytes.length }, bytes: [...bytes] }], gaps: [], next_cursor: null } }) };
  };
  return { pages, call };
}
test('settled context audit requires exact requalification body/resources and refuses historical identity', () => {
  for (const candidate of candidates.inspect().entries) {
    const row = { skill: candidate.qualified_id }, attempt = [{ phase: 'settled', request_digest: 'request-digest' }];
    const check = c => runner.skillEvidence({ executable: 'never-executed' }, 'synthetic', row, c.pages, attempt, c.call);
    assert.equal(check(context(row.skill, candidate.parts)).parts, candidate.parts.length);
    assert.throws(() => check(context(row.skill, candidate.parts, m => { m.included[0].source_hash = '0'.repeat(64); })), /context differs/);
    assert.throws(() => check(context(row.skill, candidate.parts, m => { m.request_sha256 = 'other'; })), /No canonical context/);
    const old = context('vcp-authoring-candidates::.::' + candidate.id, candidate.parts);
    assert.throws(() => check(old), /context differs/);
    assert.throws(() => runner.skillEvidence({ executable: 'never-executed' }, 'synthetic', { skill: 'vcp-authoring-candidates::.::' + candidate.id }, old.pages, attempt, old.call), /Unknown selected/);
  }
});
function deletionFixture(t) {
  const f = planned(t), row = f.rows.find(row => row.case_id === 'SKL-requal-prune-resource-v4' && row.arm === 'candidate');
  const item = all.find(item => item.task.id === row.case_id), base = path.join(f.root, row.id), workspace = path.join(base, 'workspace');
  fs.mkdirSync(workspace, { recursive: true });
  for (const directory of row.directories) fs.mkdirSync(path.join(workspace, directory), { recursive: true });
  for (const [relative, bytes] of item.files) fs.writeFileSync(path.join(workspace, relative), bytes);
  return { row, base, workspace, deletion: 'package/references/legacy-checklist.md', check: () => runner.finalWorkspace(base, row, row.profile.affected_paths) };
}
test('prune task can delete only its declared obsolete resource while retaining directories and scaffold', t => {
  const f = deletionFixture(t); assert(f.row.profile.affected_paths.includes(f.deletion));
  assert(f.check().has(f.deletion)); fs.unlinkSync(path.join(f.workspace, f.deletion));
  assert.equal(f.check().has(f.deletion), false);
  const preserved = path.join(f.workspace, 'package/references/compatibility.md'); fs.unlinkSync(preserved);
  assert.throws(f.check, /source file deleted/);
});
test('deletion authority does not allow rewrite, unauthorized removal or scaffold edits', t => {
  const f = deletionFixture(t), target = path.join(f.workspace, f.deletion), before = fs.readFileSync(target);
  fs.writeFileSync(target, 'unauthorized rewrite'); assert.throws(f.check, /outside bounded/); fs.writeFileSync(target, before);
  assert.throws(() => runner.finalWorkspace(f.base, f.row, f.row.profile.affected_paths.filter(relative => relative !== f.deletion)), /Deletion exceeds/);
  fs.appendFileSync(path.join(f.workspace, 'checks/authoring.test.cjs'), 'changed'); assert.throws(f.check, /outside bounded|scaffold changed/);
});
test('blind review redacts both candidate namespaces and builtin identities without mutating source payload', () => {
  const raw = { text: 'vcp-authoring-requalification-candidates::.::document-authoring vcp-authoring-candidates::.::skill-authoring', files: [{ path: 'note.md', content: 'vcp-builtin::architecture::architecture vcp-builtin::testing::testing' }], source_fact: 'a/b remains 7' };
  const before = structuredClone(raw), redacted = review.redactOutput(raw);
  assert.deepEqual(raw, before); assert.equal(redacted.text, '[selected skill] [selected skill]');
  assert.equal(redacted.files[0].content, '[selected skill] [selected skill]'); assert.equal(redacted.source_fact, raw.source_fact);
  assert.deepEqual(JSON.parse(review.encodePacket(redacted)), redacted);
});
test('CLI wrappers export exactly the tested runner and review APIs without invoking them on import', () => {
  assert.equal(require('../../../scripts/evals/authoring-requalification.cjs'), runner);
  assert.equal(require('../../../scripts/evals/authoring-requalification-review.cjs'), review);
});
test('reader and owner destinations protect frozen successor code and every referenced evidence directory', () => {
  const root = path.resolve('synthetic-review-boundary'), successor = { repository: path.join(root, 'frozen-successor') };
  for (const name of ['envelope', 'plan', 'result', 'halt']) successor[name] = { file: path.join(root, 'successor-' + name, name + '.json') };
  const envelope = { directory: path.join(root, 'new-campaign'), budget: { reference: { successor },
    current: { reference: { repository: path.join(root, 'cs2-source'), plan: { file: path.join(root, 'cs2', 'plan.json') } } }, history: { phases: [] } } };
  const protectedRoots = review.protectedRoots(envelope);
  assert(protectedRoots.includes(successor.repository));
  for (const name of ['envelope', 'plan', 'result', 'halt']) assert(protectedRoots.includes(path.dirname(successor[name].file)));
});
