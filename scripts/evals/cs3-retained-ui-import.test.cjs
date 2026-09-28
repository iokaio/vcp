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
