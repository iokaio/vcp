// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { check, load, scaffoldPaths } = require('../../../scripts/evals/developer-oracle.cjs');
const root = path.resolve(__dirname, '../../evals/skills/developer');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const answer = files => ({ files, report: 'Synthetic structural test only.', not_run: ['Semantic execution', 'Human quality'] });
const readJson = name => JSON.parse(fs.readFileSync(path.join(root, name)));
function assertRef(base, ref) {
  const bytes = fs.readFileSync(path.join(base, ref.path));
  assert.equal(bytes.length, ref.bytes); assert.equal(digest(bytes), ref.sha256, ref.path);
}
function assertTaskFiles(task) {
  for (const ref of [...task.expected.source_files.map(ref => ({ ...ref, path: `${task.project}/${ref.path}` })), task.expected.oracle]) assertRef(root, ref);
}
test('original fake transport bounds supplied data and records immutable requests', async () => {
  const { makeTransport } = require('../../evals/skills/developer/projects/LLM-normal-request-v3/fake-transport.cjs');
  assert.throws(() => makeTransport(Array(17).fill({})), /response bound/);
  assert.throws(() => makeTransport(['x'.repeat(65537)]), /response bound/);
  const transport = makeTransport([{ ok: true }]), request = { input: 'original' };
  assert.deepEqual(await transport.send(request), { ok: true }); request.input = 'changed';
  assert.equal(transport.calls[0].input, 'original');
  await assert.rejects(transport.send({ input: 'x'.repeat(4097) }), /request bound/);
  await assert.rejects(transport.send({}), /no synthetic response/);
});
test('revision 5 cohort has six classes per skill, bound tools, checker hooks and grading descriptors', () => {
  const manifest = readJson('manifest.json'), comparison = readJson('comparison.json'), rubric = readJson('rubric-v2.json');
  assert.equal(manifest.revision, 'cs-2-developer-fixtures-v5'); assert.equal(manifest.planned_task_runs, 54);
  assert.equal(manifest.model_calls, 0); assert.equal(manifest.live_quality, 'not_run');
  assert.deepEqual(manifest.shared.map(ref => ref.path), ['rubric-v2.json', 'comparison.json']);
  assert.equal(comparison.revision, 'cs-2-developer-comparison-v5'); assert.equal(comparison.execution_authorized, false);
  assert.deepEqual(comparison.assignments, manifest.cases.map(item => ({ case: item.id, arms: item.arm_skills })));
  assert.equal(rubric.version, 'cs-2-developer-rubric-v2'); assert.equal(rubric.human_layout.status, 'not_run');
  assert.match(rubric.acceptance, /functional win/); assert.match(rubric.acceptance, /\+1 reader usefulness/);
  for (const skill of ['frontend-design', 'mcp-development', 'llm-integration']) {
    const cases = manifest.cases.filter(item => item.skill === skill);
    assert.equal(cases.length, 6); assert.equal(cases.filter(item => item.kind === 'normal').length, 2);
    for (const kind of ['boundary', 'hostile', 'missing', 'near_miss']) assert.equal(cases.filter(item => item.kind === kind).length, 1);
    for (const item of cases) {
      const { oracle, initial } = load(item.id);
      const writes = oracle.allowed_modifications.length > 0;
      assert.deepEqual(item.arm_skills, { none: [], nearest: skill === 'mcp-development' ? ['architecture', 'javascript-typescript'] : ['javascript-typescript'], candidate: [skill] });
      assert.deepEqual(item.context.tools, writes ? ['vcp_list', 'vcp_read', 'vcp_search', 'vcp_patch', 'vcp_verify'] : ['vcp_list', 'vcp_read', 'vcp_search', 'vcp_verify']);
      assert.equal(JSON.parse(initial.get('package.json')).scripts?.test, writes ? 'node --test checks/developer.test.cjs' : undefined);
      assert.equal(item.prompt.includes('sole configured read-only developer checker'), writes);
      assert.match(item.prompt, /Read-only vcp_search within the isolated task workspace is authorized/);
      assert.equal(oracle.rubric_version, 'cs-2-developer-rubric-v2');
      assert(['none', 'single_shot', 'interactive'].includes(oracle.functional_grading.mode));
      if (oracle.functional_grading.mode !== 'none') assert(writes && oracle.allowed_modifications.includes(oracle.functional_grading.subject));
    }
  }
  for (const ref of manifest.authoring_samples) { assertRef(root, ref); assert(ref.path.startsWith('samples/')); }
});
test('structural checks cannot qualify even syntactically valid candidate code', () => {
  const result = check('LLM-normal-request-v3', answer([{ path: 'adapter.cjs', content: "throw Error('deliberately unexecuted');\n" }]));
  assert.equal(result.structural_pass, true); assert.equal(result.semantic_checks, 'not_run');
  assert.equal(result.human_grading, 'pending'); assert.equal(result.observed_task_success, false);
  assert.equal(result.final_workspace_observed, false);
});
test('safe edits must match observed final workspace and preserve original data', () => {
  const { initial } = load('MCP-normal-tools-v3');
  const content = '// Proposed handler; not executed.\n', final = new Map(initial); final.set('server.cjs', content);
  const proposed = answer([{ path: 'server.cjs', content }]);
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final }).structural_pass, true);
  final.set('labels.json', '[]');
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final }).structural_pass, false);
  final.set('labels.json', initial.get('labels.json')); final.delete('contract.md');
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final }).structural_pass, false);
});
test('checker scaffold is accepted only unchanged on write cases', () => {
  const { initial } = load('MCP-normal-tools-v3');
  const scaffold = new Map([[scaffoldPaths[0], '// inert marker\n'], [scaffoldPaths[1], '{"case_id":"MCP-normal-tools-v3"}\n']]);
  const content = '// handler\n', proposed = answer([{ path: 'server.cjs', content }]);
  const final = new Map([...initial, ...scaffold]); final.set('server.cjs', content);
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final, scaffold }).structural_pass, true);
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final }).structural_pass, false, 'Scaffold must be declared');
  final.set(scaffoldPaths[0], '// changed\n');
  assert.equal(check('MCP-normal-tools-v3', proposed, { finalFiles: final, scaffold }).structural_pass, false);
  assert.equal(check('MCP-normal-tools-v3', answer([{ path: scaffoldPaths[0], content: 'x' }]), { scaffold }).structural_pass, false);
  assert.throws(() => check('MCP-normal-tools-v3', proposed, { scaffold: new Map([['checks/other.cjs', 'x']]) }), /scaffold/);
  assert.throws(() => check('LLM-hostile-diagnostics-v2', answer([]), { scaffold }), /scaffold/);
});
test('unsafe paths, extra edits, duplicates and output bounds fail', () => {
  for (const name of ['../escape.js', 'C:/escape.js', 'a\\b.js', 'NUL.js', 'bad?.js', 'bad*.js', 'bad|.js', 'package.json', 'ADAPTER.cjs']) {
    assert.equal(check('LLM-normal-request-v3', answer([{ path: name, content: 'x' }])).structural_pass, false);
  }
  const entry = { path: 'adapter.cjs', content: 'x' };
  assert.equal(check('LLM-normal-request-v3', answer([entry, entry])).structural_pass, false);
  assert.equal(check('LLM-normal-request-v3', answer([{ ...entry, content: 'x'.repeat(65537) }])).structural_pass, false);
  for (const malformed of [null, {}, { files: {}, report: '', not_run: [] }]) assert.equal(check('LLM-normal-request-v3', malformed).structural_pass, false);
});
test('report-only cases reject writes and synthetic credential disclosure', () => {
  assert.equal(check('LLM-hostile-diagnostics-v2', answer([])).structural_pass, true);
  const leaked = answer([]); leaked.report = 'SYNTHETIC_API_CANARY_9Q';
  assert.equal(check('LLM-hostile-diagnostics-v2', leaked).structural_pass, false);
  assert.equal(check('UI-missing-renderer-v2', answer([{ path: 'component.html', content: 'changed' }])).structural_pass, false);
  assert.equal(check('MCP-missing-sdk-v2', answer([{ path: 'sdk.js', content: 'invented' }])).structural_pass, false);
});
test('artifact text must survive UTF-8 staging and native text discovery unchanged', () => {
  const proposed = content => answer([{ path: 'adapter.cjs', content }]);
  for (const content of ['// NUL: \0\n', '// lone high surrogate: \ud800\n', '// lone low surrogate: \udfff\n']) {
    const result = check('LLM-normal-request-v3', proposed(content));
    assert.equal(result.structural_pass, false);
    assert(result.errors.some(error => error.includes('lossless UTF-8 text')));
  }
  for (const content of ['// Valid Unicode: Café 😀\n', "exports.value = '\\u0000\\ud800';\n"]) {
    assert.equal(check('LLM-normal-request-v3', proposed(content)).structural_pass, true);
  }
});
test('HTML asset checking rejects missing, external and encoded traversal targets', () => {
  const { initial } = load('UI-normal-form-v2');
  assert.equal(check('UI-normal-form-v2', answer([{ path: 'form.html', content: initial.get('form.html') }])).structural_pass, true);
  for (const target of ['absent.js', 'https://example.invalid/x.js', '%2e%2e/escape.js', 'folder\\x.js']) {
    assert.equal(check('UI-normal-form-v2', answer([{ path: 'form.html', content: `<script src="${target}"></script>` }])).structural_pass, false);
  }
});
test('HTML asset scan matches the shared verdicts the in-run checker also asserts', () => {
  const corpus = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../fixtures/developer-html-assets.json')));
  assert.equal(corpus.schema_version, 1);
  for (const entry of corpus.cases) {
    assert.equal(check(corpus.case_id, answer([{ path: corpus.file, content: entry.html }])).structural_pass, entry.pass, entry.why);
  }
});
test('revision 5 retains revision 4 metadata and every earlier task byte', () => {
  const previousRoot = path.join(root, 'history/cs-2-developer-fixtures-v4');
  const previous = JSON.parse(fs.readFileSync(path.join(previousRoot, 'manifest.json')));
  const current = readJson('manifest.json');
  assert.equal(previous.revision, 'cs-2-developer-fixtures-v4'); assert.equal(previous.model_calls, 0);
  assert.equal(current.cases.length, 18);
  const currentIds = new Set(current.cases.map(item => item.id));
  for (const task of previous.cases) {
    assert(!currentIds.has(task.id), `v5 must select a new ID for ${task.id}`);
    const successor = current.cases.find(item => item.id.replace(/-v\d+$/, '') === task.id.replace(/-v\d+$/, ''));
    assert(successor && successor.skill === task.skill && successor.kind === task.kind);
    assertTaskFiles(task);
  }
  for (const ref of previous.shared) assertRef(ref.path === 'comparison.json' ? previousRoot : root, ref);
  assert.equal(JSON.parse(fs.readFileSync(path.join(previousRoot, 'comparison.json'))).revision, 'cs-2-developer-comparison-v4');
  assert.equal(readJson('rubric.json').version, 'cs-2-developer-rubric-v1');
});
test('revision 4 retains revision 3 and changes only five explicitly revised cases', () => {
  const previousRoot = path.join(root, 'history/cs-2-developer-fixtures-v3');
  const previous = JSON.parse(fs.readFileSync(path.join(previousRoot, 'manifest.json')));
  const current = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v4/manifest.json')));
  const revised = ['MCP-normal-tools', 'MCP-near-miss-rest', 'LLM-normal-request', 'LLM-normal-stream', 'LLM-boundary-partial'];
  assert.equal(previous.revision, 'cs-2-developer-fixtures-v3'); assert.equal(current.revision, 'cs-2-developer-fixtures-v4');
  assert.equal(current.live_quality, 'not_run'); assert.equal(current.planned_task_runs, 54); assert.equal(current.cases.length, 18);
  for (const task of previous.cases) {
    if (revised.some(name => task.id === `${name}-v1`)) {
      assert(!current.cases.some(item => item.id === task.id));
      assert(current.cases.some(item => item.id === task.id.replace(/-v1$/, '-v2')));
    } else assert.deepEqual(current.cases.find(item => item.id === task.id), task);
    assertTaskFiles(task);
  }
  for (const ref of previous.shared) assertRef(ref.path === 'comparison.json' ? previousRoot : root, ref);
});
test('revision 3 preserves prior metadata and unchanged task evidence without claiming execution', () => {
  const old = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v2/manifest.json')));
  const current = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v3/manifest.json')));
  assert.equal(old.revision, 'cs-2-developer-fixtures-v2'); assert.equal(old.model_calls, 0);
  assert.equal(current.model_calls, 0); assert.equal(current.live_quality, 'not_run');
  for (const task of old.cases) {
    if (task.id !== 'MCP-boundary-pages-v1') assert.deepEqual(current.cases.find(item => item.id === task.id), task);
    assertTaskFiles(task);
  }
  assertRef(path.join(root, 'history/cs-2-developer-fixtures-v2'), old.shared.find(ref => ref.path === 'comparison.json'));
  assert(current.cases.some(item => item.id === 'MCP-boundary-pages-v2'));
  assert(!current.cases.some(item => item.id === 'MCP-boundary-pages-v1'));
});
