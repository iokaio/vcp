// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const review = require('../../../scripts/evals/developer-continuation-review.cjs');
const runner = require('../../../scripts/evals/developer-continuation.cjs');
const grader = require('../../../scripts/evals/developer-grader.cjs');
const prep = require('../../../scripts/evals/developer-prepare.cjs');
const { identity } = require('../../../scripts/evals/authoring-prepare.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const rows = Array.from({ length: 6 }, (_, n) => ['none', 'nearest', 'candidate'].map(arm => ({ id: `case-${n}--${arm}`, case_id: `case-${n}`, arm, block: 'llm-integration', evidence_directory: path.join(owner.root, `case-${n}--${arm}`), write: true, functional_grading: 'single_shot' }))).flat();
  const runs = rows.map(row => ({ id: row.id, status: 'completed', preserved: true, canary_disclosed: false, oracle: { structural_pass: true }, native_check: { status: 'passed' }, skill_evidence: {} }));
  runs[7].status = 'failed'; runs[7].native_check.status = 'not_run';
  const value = { plan: { directory: owner.root }, rows, result: { block: 'llm-integration', runs }, result_sha256: 'a'.repeat(64) };
  const grades = rows.map((row, n) => ({ id: row.id, functional: n === 7 ? 'not_graded' : 'passed' }));
  return { ...value, value, grades };
}

test('composite review requires every retained and fresh arm exactly once', t => {
  const { value } = fixture(t);
  assert.equal(review.validateBlock(value), value);
  const missing = structuredClone(value); missing.rows.pop(); missing.result.runs.pop();
  assert.throws(() => review.validateBlock(missing), /coverage/);
  const duplicate = structuredClone(value); duplicate.rows[1].id = duplicate.rows[0].id;
  assert.throws(() => review.validateBlock(duplicate), /coverage/);
  const wrongArm = structuredClone(value); wrongArm.rows[1].arm = 'none';
  assert.throws(() => review.validateBlock(wrongArm), /coverage/);
  const swapped = structuredClone(value); swapped.result.runs.reverse();
  assert.throws(() => review.validateBlock(swapped), /coverage/);
});

test('a permanent continuation halt prevents review even with intact inherited evidence', t => {
  const { value } = fixture(t);
  fs.writeFileSync(path.join(value.plan.directory, 'halt.json'), '{}');
  assert.throws(() => review.validateBlock(value), /Campaign halted/);
});

test('reader packet output cannot modify either campaign evidence directory', t => {
  const { plan } = fixture(t), root = plan.directory;
  plan.directory = path.join(root, 'fresh'); plan.predecessor = { file: path.join(root, 'original', 'plan.json') };
  const outside = path.join(root, 'readers');
  assert.equal(review.readerDirectory(plan, outside), outside);
  for (const destination of [path.join(plan.directory, 'readers'), path.join(root, 'original', 'readers'), root]) {
    assert.throws(() => review.readerDirectory(plan, destination), /both campaign directories/);
    assert.equal(fs.existsSync(path.join(destination, 'index.json')), false);
  }
});

test('an inherited incomplete run cannot be upgraded by grading or regrading', t => {
  const { rows, result, grades } = fixture(t);
  review.validateGrading(grades, rows, result);
  for (const functional of ['passed', 'failed', 'requires_regrade', 'not_applicable']) {
    const changed = structuredClone(grades); changed[7].functional = functional;
    assert.throws(() => review.validateGrading(changed, rows, result), /upgrade/);
    assert.throws(() => review.validateGrading([changed[7]], rows, result, true), /upgrade/);
  }
  assert.equal(review.executable(rows[7], result.runs[7], 'passed'), false);
  const missingCheck = { ...result.runs[0], native_check: { status: 'not_run' } };
  assert.equal(review.executable(rows[0], missingCheck, 'passed'), false);
});

test('functional records require exact coverage, applicability and bounded regrades', t => {
  const { rows, result, grades } = fixture(t);
  assert.throws(() => review.validateGrading(grades.slice(1), rows, result), /coverage/);
  assert.throws(() => review.validateGrading([...grades, grades[0]], rows, result), /coverage/);
  assert.throws(() => review.validateGrading([], rows, result, true), /coverage/);
  assert.throws(() => review.validateGrading([{ id: 'unknown', functional: 'passed' }], rows, result, true), /coverage/);
  assert.throws(() => review.validateGrading([{ ...grades[0], functional: 'accepted' }], rows, result, true), /Unknown/);
  rows[0].functional_grading = 'none';
  assert.throws(() => review.validateGrading(grades, rows, result), /misclassify/);
  grades[0].functional = 'not_applicable';
  review.validateGrading(grades, rows, result);
});

test('packet binding includes all cases, arm permutations and the committed mapping', t => {
  const { rows } = fixture(t), cases = [...new Set(rows.map(row => row.case_id))];
  const mapping = { schema: 'cs-2-developer-review-mapping/2', index_sha256: 'packets', salt: 'b'.repeat(64), mapping: cases.map(case_id => ({ case_id, none: 'B', nearest: 'C', candidate: 'A' })) };
  const index = { schema: 'cs-2-developer-reader-packets/1', plan_sha256: 'plan', block: rows[0].block, result_sha256: 'result', grading_sha256: 'grading', packets: cases.map(case_id => ({ case_id, sha256: 'a'.repeat(64) })), mapping_commitment: sha(mapping.salt + JSON.stringify(mapping.mapping)) };
  const validate = (m = mapping, i = index) => review.validatePackets(m, i, rows, 'plan', 'result', 'grading', 'packets');
  assert.deepEqual(validate(), cases);
  assert.throws(() => validate(mapping, { ...index, packets: index.packets.slice(1) }), /coverage/);
  assert.throws(() => validate(mapping, { ...index, block: 'frontend-design' }), /bind/);
  assert.throws(() => validate(mapping, { ...index, grading_sha256: 'old' }), /bind/);
  const duplicate = structuredClone(mapping); duplicate.mapping[0].candidate = 'B';
  assert.throws(() => validate(duplicate), /mapping/);
  const changed = structuredClone(mapping); [changed.mapping[0].none, changed.mapping[0].candidate] = [changed.mapping[0].candidate, changed.mapping[0].none];
  assert.throws(() => validate(changed), /commitment/);
});

test('each blind reader must score all three anonymous variants for every case', () => {
  const variant = label => ({ label, scores: { completeness: 2, clarity: 2, usefulness: 2 }, hard_gates: Object.fromEntries(review.hardGates.map(key => [key, true])), halt: Object.fromEntries(review.haltItems.map(key => [key, false])), forbidden_action_proposed: false, findings: '' });
  const value = { schema: 'cs-2-developer-review/1', reviewer_id: 'reader', independent_blinded: true, packets_sha256: 'packets', cases: [{ case_id: 'case', variants: ['A', 'B', 'C'].map(variant) }] };
  review.review(value, 'packets', ['case']);
  const partial = structuredClone(value); partial.cases[0].variants.pop();
  assert.throws(() => review.review(partial, 'packets', ['case']), /variants/);
  assert.throws(() => review.review(value, 'packets', ['case', 'missing']), /coverage/);
  assert.throws(() => review.review({ ...value, independent_blinded: false }, 'packets', ['case']), /binding/);
});

// Synthetic composite inspection and containment outcomes replace only external
// boundaries. All grading/regrading, file reads, packet writes, commitments,
// review parsing and decision paths below are the production review functions.
function integration(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const root = owner.root, original = path.join(root, 'original'), directory = path.join(root, 'fresh'), block = 'llm-integration';
  fs.mkdirSync(original); fs.mkdirSync(directory);
  fs.writeFileSync(path.join(original, 'halt.json'), '{"reason":"owner reboot"}');
  const plan = { directory, predecessor: { file: path.join(original, 'plan.json') }, grader: { node: 'synthetic', node_sha256: 'synthetic' }, benefit_rule: 'Frozen CS-2 rule' };
  const rows = [], reports = [];
  for (const item of prep.tasks().items.filter(item => item.task.skill === block)) for (const arm of prep.arms) {
    const id = item.task.id + '--' + arm, base = path.join(rows.length < 8 ? original : directory, id), workspace = path.join(base, 'workspace');
    fs.mkdirSync(workspace, { recursive: true });
    for (const [relative, bytes] of item.files) { const file = path.join(workspace, relative); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes); }
    const inputs = identity(workspace, ['.']);
    rows.push({ id, case_id: item.task.id, arm, block, kind: item.task.kind, write: !!item.edits.length, functional_grading: item.edits.length ? 'single_shot' : 'none', evidence_directory: base,
      profile: { maximum_autonomy: item.edits.length ? 'workspace' : 'plan', affected_paths: item.edits }, files: inputs.files.map(file => ({ ...file, path: file.path.slice(2) })), directories: inputs.directories.filter(name => name !== '.').map(name => name.slice(2)).sort(), scaffold_paths: item.edits.length ? ['checks/developer.test.cjs', 'checks/developer-case.json'] : [] });
    const failed = rows.length === 8;
    reports.push({ id, status: failed ? 'failed' : 'completed', preserved: true, canary_disclosed: false, oracle: { structural_pass: true }, native_check: { status: failed ? 'not_run' : 'passed' }, skill_evidence: {} });
    fs.writeFileSync(path.join(base, 'answer.json'), JSON.stringify({ report: 'Used the llm-integration skill and preserved project choices.', not_run: ['Live compatibility'] }));
  }
  const originalHash = identity(original, ['.']).content_sha256;
  const value = { plan, rows, result: { block, runs: reports }, result_sha256: 'a'.repeat(64) };
  t.mock.method(runner, 'inspectBlock', () => {
    if (identity(original, ['.']).content_sha256 !== originalHash) throw Error('Original evidence changed');
    return value;
  });
  t.mock.method(grader, 'grade', async () => ({ functional_pass: true, requires_regrade: false, observations: [{ name: 'synthetic qualified probe', passed: true }], errors: [] }));
  const factory = () => ({ qualified: true, name: 'synthetic containment boundary' });
  const args = ['synthetic-plan.json', 'plan', block];
  const readers = path.join(root, 'readers');
  function reviews(halt = false) {
    const mapping = JSON.parse(fs.readFileSync(path.join(directory, `packets-${block}.json`))), packetsHash = sha(fs.readFileSync(path.join(readers, 'index.json')));
    return ['one', 'two'].map(reviewer_id => {
      const cases = mapping.mapping.map(entry => ({ case_id: entry.case_id, variants: prep.arms.map(arm => ({ label: entry[arm], scores: Object.fromEntries(review.scores.map(key => [key, arm === 'candidate' ? 3 : 2])), hard_gates: Object.fromEntries(review.hardGates.map(key => [key, true])), halt: { effect_beyond_authority: halt && arm === 'none', real_secret_exposed: false }, forbidden_action_proposed: false, findings: 'Synthetic independent judgment' })) }));
      const file = path.join(root, `review-${reviewer_id}.json`);
      fs.writeFileSync(file, JSON.stringify({ schema: 'cs-2-developer-review/1', reviewer_id, independent_blinded: true, packets_sha256: packetsHash, cases })); return file;
    });
  }
  return { root, original, originalHash, value, args, factory, readers, reviews };
}

test('composite grading, anonymous packets and two-reader decisions preserve the failed origin run', async t => {
  const f = integration(t);
  const graded = await review.grade(...f.args, f.factory);
  assert.equal(graded.runs[7].functional, 'not_graded');
  const packetResult = review.packets(...f.args, f.readers);
  assert.equal(packetResult.cases, 6);
  const failedPacket = JSON.parse(fs.readFileSync(path.join(f.readers, f.value.rows[7].case_id + '.json')));
  assert.equal(failedPacket.variants.filter(variant => !variant.completed).length, 1);
  for (const row of f.value.rows) {
    const bytes = fs.readFileSync(path.join(f.readers, row.case_id + '.json'), 'utf8');
    assert(!bytes.includes('llm-integration')); assert(!bytes.includes('evidence_directory')); assert(!bytes.includes('actual_cost_micros'));
  }
  const decision = review.decide(...f.args, f.reviews());
  assert.equal(decision.qualifies, true); assert.equal(decision.reviews_sha256.length, 2);
  assert.equal(decision.cases.find(item => item.case_id === f.value.rows[7].case_id).executable[f.value.rows[7].arm], false);
  assert.equal(identity(f.original, ['.']).content_sha256, f.originalHash);
  await assert.rejects(review.grade(...f.args, f.factory), /final once reader packets/);
});

test('an async grading halt writes no grading record and forbids future packet writes', async t => {
  const f = integration(t);
  t.mock.method(grader, 'grade', async () => {
    await Promise.resolve(); fs.writeFileSync(path.join(f.value.plan.directory, 'halt.json'), '{}');
    return { functional_pass: true, observations: [], errors: [] };
  });
  await assert.rejects(review.grade(...f.args, f.factory), /Campaign halted/);
  assert.equal(fs.existsSync(path.join(f.value.plan.directory, 'grading-llm-integration.json')), false);
  assert.throws(() => review.packets(...f.args, f.readers), /Campaign halted/);
  assert.equal(fs.existsSync(f.readers), false);
});

test('origin evidence changed during async grading prevents publishing a grading record', async t => {
  const f = integration(t);
  t.mock.method(grader, 'grade', async () => {
    await Promise.resolve(); fs.writeFileSync(path.join(f.original, 'unexpected.json'), '{}');
    return { functional_pass: true, observations: [], errors: [] };
  });
  await assert.rejects(review.grade(...f.args, f.factory), /Original evidence changed/);
  assert.equal(fs.existsSync(path.join(f.value.plan.directory, 'grading-llm-integration.json')), false);
});

test('a blind reader global halt permanently closes every new review writer', async t => {
  const f = integration(t);
  await review.grade(...f.args, f.factory); review.packets(...f.args, f.readers);
  assert.throws(() => review.decide(...f.args, f.reviews(true)), error => error.code === 'CS2_REVIEW_HALT');
  assert.throws(() => review.decide(...f.args, f.reviews(false)), /Campaign halted/);
  await assert.rejects(review.grade(...f.args, f.factory), /Campaign halted/);
  assert.throws(() => review.packets(...f.args, path.join(f.root, 'another-reader-directory')), /Campaign halted/);
  assert.equal(identity(f.original, ['.']).content_sha256, f.originalHash);
});
