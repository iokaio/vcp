// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto'), Module = require('node:module');
const importer = require('./cs3-retained-ui-import.cjs'), oracle = require('./developer-oracle.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-import-')), packets = path.join(root, 'packets'), mappingFile = path.join(root, 'mapping.json');
  fs.mkdirSync(packets);
  const salt = 'a'.repeat(64), mapping = importer.caseIds.map(case_id => ({ case_id, none: 'A', nearest: 'B', candidate: 'C' }));
  for (const case_id of importer.caseIds) {
    const frozen = oracle.load(case_id);
    const value = { schema: 'cs-2-developer-reader-packet/1', case_id, kind: frozen.task.kind, prompt: frozen.task.prompt, sources: Object.fromEntries(frozen.initial), rubric: 'src/evals/skills/developer/rubric-v2.json',
      score_keys: ['completeness', 'clarity', 'usefulness'], hard_gates: ['correctness', 'preservation', 'authority_and_secrets', 'honest_not_run', 'evidence_honesty'], halt_items: ['effect_beyond_authority', 'real_secret_exposed'],
      variants: ['A', 'B', 'C'].map(label => ({ label, completed: !(case_id.includes('near-miss') && label === 'C'),
        final_files: Object.fromEntries(frozen.oracle.allowed_modifications.map(name => [name, frozen.initial.get(name)])), report: 'Synthetic reader fixture; not historical evidence.', not_run: ['Browser'],
        checks: { structural_oracle: 'passed', in_run_checker: 'not_run', functional: 'not_graded', synthetic_canary_disclosed: false } })) };
    fs.writeFileSync(path.join(packets, case_id + '.json'), JSON.stringify(value));
  }
  function seal() {
    const index = { schema: 'cs-2-developer-reader-packets/1', plan_sha256: importer.historical.plan, block: 'frontend-design', result_sha256: importer.historical.result, grading_sha256: importer.historical.grading,
      mapping_commitment: sha(salt + JSON.stringify(mapping)), packets: importer.caseIds.map(case_id => ({ case_id, sha256: sha(fs.readFileSync(path.join(packets, case_id + '.json'))) })) };
    const bytes = JSON.stringify(index); fs.writeFileSync(path.join(packets, 'index.json'), bytes);
    fs.writeFileSync(mappingFile, JSON.stringify({ schema: 'cs-2-developer-review-mapping/2', private: 'Synthetic test mapping', destination: 'Z:\\historical-path-never-read', index_sha256: sha(bytes), salt, mapping }));
    return { ...importer.historical, index: sha(bytes), mapping: sha(fs.readFileSync(mappingFile)) };
  }
  return { root, packets, mappingFile, mapping, seal, cleanup() { fs.rmSync(root, { recursive: true, force: true }); } };
}
function rewrite(f, update) {
  const file = path.join(f.packets, importer.caseIds[0] + '.json'), value = JSON.parse(fs.readFileSync(file)); update(value); fs.writeFileSync(file, JSON.stringify(value));
}
test('authenticates all eighteen projected artifacts and preserves incomplete historical outcome', () => {
  const f = fixture(); try {
    const result = importer.inspect(f.packets, f.mappingFile, f.seal());
    assert.equal(result.runs.length, 18); assert.equal(result.input_files.length, 8);
    const failed = result.runs.find(r => r.case_id === 'UI-near-miss-parser-v2' && r.arm === 'candidate');
    assert.equal(failed.completed, false); assert(failed.files.some(file => file.provenance === 'packet-final-file'));
    assert(result.runs.every(row => row.files.every(file => file.sha256 === sha(file.content))));
  } finally { f.cleanup(); }
});
test('production staging cannot accept synthetic commitments or missing originals', () => {
  const f = fixture(); try {
    f.seal(); const out = path.join(f.root, 'output');
    assert.throws(() => importer.prepare(f.packets, f.mappingFile, out), /commitment differs/); assert(!fs.existsSync(out));
    assert.throws(() => importer.prepare(path.join(f.root, 'missing'), f.mappingFile, out), /originals are unavailable/); assert(!fs.existsSync(out));
  } finally { f.cleanup(); }
});
test('rejects altered payload hashes and extra packet inventory', () => {
  const f = fixture(); try {
    const commitments = f.seal(); rewrite(f, p => { p.variants[0].report += 'changed'; });
    assert.throws(() => importer.inspect(f.packets, f.mappingFile, commitments), /packet changed/);
    const fresh = f.seal(); fs.writeFileSync(path.join(f.packets, 'extra.json'), '{}');
    assert.throws(() => importer.inspect(f.packets, f.mappingFile, fresh), /inventory differs/);
  } finally { f.cleanup(); }
});
test('rejects expanded paths, missing final output, changed input sources and duplicate arm labels', () => {
  for (const change of [
    p => { p.variants[0].final_files['../escape.html'] = 'bad'; },
    p => { delete p.variants[0].final_files['form.js']; },
    p => { p.sources['form.html'] += 'changed'; },
    p => { p.variants[1].label = 'A'; },
    p => { p.variants[0].final_files['FORM.HTML'] = 'colliding'; },
  ]) {
    const f = fixture(); try { rewrite(f, change); assert.throws(() => importer.inspect(f.packets, f.mappingFile, f.seal())); } finally { f.cleanup(); }
  }
});
test('rejects packet directory junctions and changed salted mappings', () => {
  const f = fixture(); try {
    const commitments = f.seal(), link = path.join(f.root, 'linked'); fs.symlinkSync(f.packets, link, process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => importer.inspect(link, f.mappingFile, commitments), /Symlink|junction/);
    const mapping = JSON.parse(fs.readFileSync(f.mappingFile)); mapping.mapping[0].candidate = 'A'; fs.writeFileSync(f.mappingFile, JSON.stringify(mapping));
    assert.throws(() => importer.inspect(f.packets, f.mappingFile, commitments), /commitment differs/);
  } finally { f.cleanup(); }
});
test('synthetic staging exercises production code without exposing a production commitment override', () => {
  const f = fixture();
  const outputRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-stage-'));
  try {
    const commitments = f.seal(), file = require.resolve('./cs3-retained-ui-import.cjs');
    // Change only the two test trust roots in this in-memory module. The CLI and
    // production module retain their fixed historical hashes, tested above.
    const source = fs.readFileSync(file, 'utf8').replace(importer.historical.index, commitments.index).replace(importer.historical.mapping, commitments.mapping);
    const isolated = new Module(file, module); isolated.filename = file; isolated.paths = module.paths; isolated._compile(source, file);
    const before = importer.inspect(f.packets, f.mappingFile, commitments), output = path.join(outputRoot, 'prepared');
    const result = isolated.exports.prepare(f.packets, f.mappingFile, output);
    const manifest = JSON.parse(fs.readFileSync(result.manifest));
    assert.equal(manifest.status, 'prepared'); assert.equal(manifest.browser_regrade, 'not_run'); assert.equal(manifest.originals_unchanged, true);
    assert.equal(manifest.runs.length, 18); assert.match(manifest.representation, /redacted/);
    for (const row of manifest.runs) for (const file of row.files) assert.equal(sha(fs.readFileSync(path.join(output, row.workspace, file.path))), file.sha256);
    assert.deepEqual(importer.inspect(f.packets, f.mappingFile, commitments), before);
    assert.throws(() => isolated.exports.prepare(f.packets, f.mappingFile, output), /New separate/);
  } finally { f.cleanup(); fs.rmSync(outputRoot, { recursive: true, force: true }); }
});

function stagedFixture(t, onDirectory) {
  const f = fixture(), outputRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-verify-'));
  t.after(() => { f.cleanup(); fs.rmSync(outputRoot, { recursive: true, force: true }); });
  const commitments = f.seal(), file = require.resolve('./cs3-retained-ui-import.cjs');
  const source = fs.readFileSync(file, 'utf8').replace(importer.historical.index, commitments.index).replace(importer.historical.mapping, commitments.mapping);
  const isolated = new Module(file, module); isolated.filename = file; isolated.paths = module.paths;
  if (onDirectory) {
    const originalRequire = isolated.require.bind(isolated);
    isolated.require = name => name === 'node:fs' ? { ...fs, readdirSync(directory, ...args) { onDirectory(directory); return fs.readdirSync(directory, ...args); } } : originalRequire(name);
  }
  isolated._compile(source, file);
  const output = path.join(outputRoot, 'prepared'), staged = isolated.exports.prepare(f.packets, f.mappingFile, output);
  return { ...f, output, staged, api: isolated.exports, manifest: JSON.parse(fs.readFileSync(staged.manifest)) };
}
test('read-only verify authenticates exact staged metadata, all eighteen runs and original commitments', t => {
  const f = stagedFixture(t), before = fs.readFileSync(f.staged.manifest);
  const result = f.api.verify(f.packets, f.mappingFile, f.output);
  assert.equal(result.schema, 'cs3-retained-ui-staging-verification/1'); assert.equal(result.status, 'verified');
  assert.equal(result.browser_regrade, 'not_run'); assert.equal(result.model_calls, 0); assert.equal(result.runs, 18);
  assert.equal(result.originals_unchanged, true); assert.equal(result.staging_unchanged, true);
  assert.equal(result.files, 1 + f.manifest.runs.reduce((count, run) => count + run.files.length, 0));
  assert.equal(result.manifest_sha256, sha(before)); assert(fs.readFileSync(f.staged.manifest).equals(before));
  assert.throws(() => importer.verify(f.packets, f.mappingFile, f.output), /commitment differs/);
  assert.throws(() => importer.verify(path.join(f.root, 'absent'), f.mappingFile, f.output), /originals are unavailable/);
});
test('verify rejects changed schema, metadata, hashes, source locators, ordering and importer identity', t => {
  const f = stagedFixture(t), original = fs.readFileSync(f.staged.manifest);
  for (const update of [
    m => { m.status = 'passed'; }, m => { m.browser_regrade = 'passed'; }, m => { m.extra = true; },
    m => { m.runs[0].arm = 'candidate'; }, m => { m.runs[0].completed = !m.runs[0].completed; },
    m => { m.runs[0].checks.functional = 'passed'; }, m => { m.runs[0].files[0].sha256 = '0'.repeat(64); },
    m => { m.runs[0].workspace = '../escape'; }, m => { m.runs.reverse(); },
    m => { m.importer_sha256 = '0'.repeat(64); }, m => { m.input_files[0].path = 'Z:/untrusted-locator'; },
  ]) {
    const manifest = JSON.parse(original); update(manifest); fs.writeFileSync(f.staged.manifest, JSON.stringify(manifest));
    assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /source manifest differs/);
  }
  fs.writeFileSync(f.staged.manifest, original);
  assert.equal(f.api.verify(f.packets, f.mappingFile, f.output).status, 'verified');
});
test('verify rejects changed or missing staged bytes and any extra file or empty directory', t => {
  const f = stagedFixture(t), run = f.manifest.runs[0], target = path.join(f.output, run.workspace, run.files[0].path), original = fs.readFileSync(target);
  fs.appendFileSync(target, 'changed'); assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /bytes differ/);
  fs.unlinkSync(target); assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /inventory incomplete/); fs.writeFileSync(target, original);
  const extra = path.join(f.output, 'unexpected.txt'); fs.writeFileSync(extra, 'extra');
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /Unexpected or linked/); fs.unlinkSync(extra);
  const extraDirectory = path.join(f.output, 'unexpected'); fs.mkdirSync(extraDirectory);
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /Unexpected staged directory/); fs.rmdirSync(extraDirectory);
  assert.equal(f.api.verify(f.packets, f.mappingFile, f.output).status, 'verified');
});
test('verify rejects staged hard links, directory junctions and linked staging roots', t => {
  const f = stagedFixture(t), run = f.manifest.runs[0], target = path.join(f.output, run.workspace, run.files[0].path);
  const backup = path.join(f.root, 'linked-content'); fs.writeFileSync(backup, fs.readFileSync(target)); fs.unlinkSync(target); fs.linkSync(backup, target);
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /linked staged file/);
  fs.unlinkSync(target); fs.copyFileSync(backup, target);
  const link = path.join(path.dirname(f.output), 'linked-root'); fs.symlinkSync(f.output, link, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, link), /Symlink|junction/);
  const childLink = path.join(f.output, 'linked-child'); fs.symlinkSync(path.dirname(target), childLink, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /Symlink|junction/);
});
test('verify re-authenticates originals after staged reads rather than trusting unchanged flags', t => {
  let armed = false, f;
  f = stagedFixture(t, directory => {
    if (armed && directory === f.output) { armed = false; fs.appendFileSync(path.join(f.packets, importer.caseIds[0] + '.json'), '\n'); }
  });
  armed = true;
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /packet changed/);
});
test('verify repeats staged reads and rejects mid-verification artifact mutation', t => {
  let visits = 0, armed = false, f;
  f = stagedFixture(t, directory => {
    if (armed && directory === f.output && ++visits === 2) {
      const run = f.manifest.runs[0]; fs.appendFileSync(path.join(f.output, run.workspace, run.files[0].path), 'changed');
    }
  });
  armed = true;
  assert.throws(() => f.api.verify(f.packets, f.mappingFile, f.output), /bytes differ/);
});
