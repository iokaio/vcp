// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const oracle = require('./authoring-fresh-doc-oracle.cjs');
const root = path.resolve(__dirname, '../../src/evals/skills/authoring-qualification');
function answer(caseId) {
  const { oracle: spec, initial } = oracle.load(caseId);
  const file = [...spec.allowed_outputs, ...spec.allowed_modifications][0];
  let content = 'neutral '.repeat(caseId === oracle.cases[0] ? 350 : 450);
  if (caseId === oracle.cases[0]) {
    const parts = oracle.region(initial.get(file));
    content = parts.prefix + '\n' + content + '\n' + parts.suffix;
  }
  return { files: [{ path: file, content }], report: 'Only static document inspection; semantic and native checks not run.', not_run: ['native verification', 'runtime tests'] };
}
test('frozen fixture refs, original source/private bytes and task authority adaptation', () => {
  const provenance = JSON.parse(fs.readFileSync(path.join(root, 'provenance.json')));
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
  for (const caseId of oracle.cases) {
    const { task, initial } = oracle.load(caseId);
    assert.equal(task.comparison_arms.join(','), 'none,nearest,candidate');
    assert.equal(task.context.tools.join(','), 'vcp_list,vcp_read,vcp_search,vcp_patch,vcp_verify');
    assert.ok([...initial.keys()].every(name => !name.includes('private-grader')));
    for (const ref of task.expected.source_files) {
      const original = provenance.original_file_inventory.find(ref2 => ref2.path === `tasks/${caseId}/project/${ref.path}`);
      assert.equal(ref.sha256, original.sha256);
    }
    const oldOracle = provenance.original_file_inventory.find(ref => ref.path === task.expected.oracle.path);
    assert.equal(task.expected.oracle.sha256, oldOracle.sha256);
    const adapted = fs.readFileSync(path.join(root, task.task_input.path), 'utf8');
    const original = adapted.split(' For document verification only,')[0].replace(' The CSV example must include at least two valid records and demonstrate a quoted comma in a label, a zero quantity, and a blank location.', '') + '\n';
    assert.equal(hash(Buffer.from(original)), provenance.original_tasks.find(ref => ref.path === `tasks/${caseId}/task.md`).sha256);
  }
  const rubric = provenance.original_file_inventory.find(ref => ref.path === 'private-grader/rubric.json');
  assert.equal(hash(fs.readFileSync(path.join(root, rubric.path))), rubric.sha256);
  assert.equal(manifest.execution_authorized, false);
});
test('valid mechanics leave native/semantic/authority gates pending', () => {
  for (const caseId of oracle.cases) {
    const result = oracle.check(caseId, answer(caseId));
    assert.equal(result.structural_pass, true);
    assert.equal(result.observed_task_success, false);
    assert.equal(result.human_grading, 'pending');
    assert.equal(result.native_markdown_and_csv_checks, 'pending_recorded_native_receipt');
    assert.equal(result.authority_receipt_audit, 'pending_owner_review');
  }
});
test('exact preserved region rejects outside edits, missing/duplicate/reversed markers', () => {
  const caseId = oracle.cases[0];
  for (const mutate of [s => s + '\nextra', s => s.replace('<!-- FORMAT END -->', ''), s => s.replace('<!-- FORMAT START -->', '<!-- FORMAT START --><!-- FORMAT START -->'), s => s.replace('<!-- FORMAT START -->', 'TEMP').replace('<!-- FORMAT END -->', '<!-- FORMAT START -->').replace('TEMP', '<!-- FORMAT END -->')]) {
    const value = answer(caseId); value.files[0].content = mutate(value.files[0].content);
    assert.equal(oracle.check(caseId, value).structural_pass, false);
  }
});
test('word bounds use Unicode whitespace and letters/numbers exactly', () => {
  assert.equal(oracle.words('hello\u0085世界\u00a042 --- | ###'), 3);
  assert.equal(oracle.words('\u0301 \u0345'), 0);
  assert.equal(oracle.words('left\ufeffright'), 1);
  for (const [caseId, min, max] of [[oracle.cases[0], 300, 550], [oracle.cases[1], 400, 650]]) {
    for (const count of [min - 1, min, max, max + 1]) {
      const value = answer(caseId), parts = caseId === oracle.cases[0] ? oracle.region(value.files[0].content) : { prefix: '', suffix: '' };
      value.files[0].content = parts.prefix + ' word'.repeat(count) + '\n' + parts.suffix;
      assert.equal(oracle.check(caseId, value).structural_pass, count >= min && count <= max);
    }
  }
});
test('actual workspace identity and output allowlist reject mutation/deletion/extra files', () => {
  const caseId = oracle.cases[1], value = answer(caseId), { initial } = oracle.load(caseId);
  const final = new Map(initial); final.set(value.files[0].path, value.files[0].content);
  assert.equal(oracle.check(caseId, value, { finalFiles: final }).structural_pass, true);
  for (const mutate of [files => files.set('extra.md', 'extra'), files => files.delete('sources/evidence.md'), files => files.set('sources/evidence.md', 'changed'), files => files.set(value.files[0].path, 'different')]) {
    const files = new Map(final); mutate(files);
    assert.equal(oracle.check(caseId, value, { finalFiles: files }).structural_pass, false);
  }
  const extra = answer(caseId); extra.files.push({ path: 'private-grader/leak.md', content: 'forbidden' });
  assert.equal(oracle.check(caseId, extra).structural_pass, false);
});
test('answer shape, unsafe paths and invalid Unicode cannot receive structural pass', () => {
  for (const value of [null, {}, { ...answer(oracle.cases[1]), invented: true }, { ...answer(oracle.cases[1]), not_run: [1] }]) assert.equal(oracle.check(oracle.cases[1], value).structural_pass, false);
  for (const target of ['../outside.md', 'C:/outside.md', 'docs/NUL.md', 'docs/a.md.']) {
    const value = answer(oracle.cases[1]); value.files[0].path = target;
    assert.equal(oracle.check(oracle.cases[1], value).structural_pass, false);
  }
  const invalid = answer(oracle.cases[1]); invalid.files[0].content += '\ud800';
  assert.equal(oracle.check(oracle.cases[1], invalid).structural_pass, false);
});

test('every embedded native fixture is bound by all checker build modes', () => {
  const repository = path.resolve(__dirname, '../..');
  const prep = require('./authoring-prepare.cjs');
  const directory = path.join(repository, 'src/crates/vcp-cli/src/bin');
  const sources = [path.join(directory, 'vcp-authoring-check.rs'),
    ...fs.readdirSync(path.join(directory, 'authoring_check')).filter(name => name.endsWith('.rs')).map(name => path.join(directory, 'authoring_check', name))];
  const embedded = new Set();
  for (const source of sources) {
    for (const match of fs.readFileSync(source, 'utf8').matchAll(/include_str!\(\s*"([^"]+)"\s*\)/g)) {
      const file = path.resolve(path.dirname(source), match[1]);
      const relative = path.relative(repository, file).replaceAll('\\', '/');
      if (relative.startsWith('src/evals/')) embedded.add(relative);
    }
  }
  assert.ok(embedded.size > 0, 'native fixture dependencies must be found');
  for (const [mode, scope] of [['original', prep.checkerBuildScope],
    ['followup', require('./authoring-followup.cjs').checkerBuildScope],
    ['qualification', require('./authoring-qualification.cjs').checkerBuildScope]]) {
    const inventory = new Map(prep.identity(repository, scope).files.map(file => [file.path, file.sha256]));
    for (const relative of embedded) {
      const expected = crypto.createHash('sha256').update(fs.readFileSync(path.join(repository, relative))).digest('hex');
      assert.equal(inventory.get(relative), expected, `${mode} must bind ${relative}`);
    }
  }
});

// Working files can pass on Windows even when Git's text normalization changes
// the committed bytes. Validate the actual index a clean CI checkout receives.
test('indexed prospective fixture bytes match every frozen manifest reference', () => {
  const { execFileSync } = require('node:child_process');
  const repository = path.resolve(__dirname, '../..'), fixture = 'src/evals/skills/authoring-qualification';
  const indexed = relative => execFileSync('git', ['-c', `safe.directory=${repository.replaceAll('\\', '/')}`, 'show', ':' + fixture + '/' + relative], { cwd: repository, maxBuffer: 1024 * 1024 });
  const manifest = JSON.parse(indexed('manifest.json'));
  const refs = [...manifest.shared, ...manifest.cases.flatMap(item => [item.task_input, item.expected.oracle, ...item.expected.source_files.map(ref => ({ ...ref, path: item.project + '/' + ref.path }))])];
  for (const ref of refs) {
    const bytes = indexed(ref.path);
    assert.equal(bytes.length, ref.bytes, 'Indexed frozen byte length changed: ' + ref.path);
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), ref.sha256, 'Indexed frozen SHA-256 changed: ' + ref.path);
  }
});
