// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const ui = require('./cs3-ui-artifact.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
test('UI compilation preserves exact UTF-8 bytes as data, never C# source', () => {
  const html = '<!doctype html><script>/* "}; dangerous(); // é */</script>';
  const source = ui.renderResource(ui.cases[0], { 'index.html': html }, 'a'.repeat(64));
  assert(!source.includes('dangerous()'));
  assert(source.includes(Buffer.from(html).toString('base64')));
  assert(source.includes('ArtifactSha256="' + 'a'.repeat(64) + '"'));
});
test('UI resource input rejects expanded paths, unknown cases and malformed Unicode', () => {
  for (const files of [{ '../index.html': 'x' }, { 'index.html': 'x', 'extra.js': '' }, { 'index.html': '\ud800' }, { 'index.html': '' }, []]) {
    assert.throws(() => ui.htmlBytes(ui.cases[0], files));
  }
  assert.throws(() => ui.htmlBytes('unknown', { 'index.html': 'x' }));
  assert.equal(ui.htmlBytes(ui.cases[0], { 'index.html': 'x'.repeat(65536) }).length, 65536);
  assert.throws(() => ui.htmlBytes(ui.cases[0], { 'index.html': 'x'.repeat(65537) }));
});
test('UI artifact inventory binds source bytes, serialized input and compiled resource', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-resource-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const file = path.join(directory, 'files.json'), raw = JSON.stringify({ 'index.html': '<h1>É</h1>\n' });
  fs.writeFileSync(file, raw);
  const result = ui.compile(ui.cases[1], file);
  assert.equal(result.artifact_sha256, sha(raw));
  assert.equal(result.html_sha256, sha('<h1>É</h1>\n'));
  assert.equal(result.source_sha256, sha(result.source));
  assert.equal(result.html_bytes, Buffer.byteLength('<h1>É</h1>\n'));
  assert.throws(() => ui.compile(ui.cases[1], 'relative.json'), /Absolute/);
});
test('UI required oracle coverage includes interaction, narrow/wide layout and motion', () => {
  for (const caseId of ui.cases) {
    const names = ui.assertions(caseId);
    assert.equal(names.length, 11); assert.equal(new Set(names).size, 11);
    for (const name of ['keyboard_reachability', 'focus_visible', 'layout_320', 'layout_1024', 'reduced_motion', 'no_outbound_request']) assert(names.includes(name));
  }
});

test('UI motion controls distinguish inactive declarations from mixed active animation', () => {
  const controls = require('./cs3-ui-controls.cjs');
  for (const caseId of ui.cases) {
    const inactive = controls.artifact(caseId, 'inactive-motion');
    const mixed = controls.artifact(caseId, 'mixed-motion');
    for (const files of [inactive, mixed]) assert(ui.htmlBytes(caseId, files).length < 65536);
    assert(inactive['index.html'].includes('animation-name:controlPulse,none!important'));
    assert(mixed['index.html'].includes('animation-name:controlPulse,controlPulse!important'));
    assert(inactive['index.html'].includes('animation-play-state:paused,running!important'));
    assert(inactive['index.html'].includes('animation-iteration-count:infinite,infinite,0!important'));
    assert(inactive['index.html'].includes('transition-property:none!important;transition-duration:2s!important'));
  }
});
test('UI grades cannot be invented from generic boolean assertions or absent native evidence', () => {
  for (const receipt of [null, {}, { schema: 'cs3-ui-artifact-browser/1', case_id: ui.cases[0], artifact_sha256: 'a'.repeat(64), status: 'passed', cleanup: 'completed', containment: 'qualified', visual_review: 'not_run', assertions: ui.assertions(ui.cases[0]).map(name => ({ name, passed: true })) }]) {
    assert.throws(() => ui.validateUiArtifact(receipt, { 'index.html': '<p>Source only</p>' }, ui.cases[0]));
  }
});

test('UI build binds all reviewed native wrappers and exact generated resource', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-ui-sources-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const resource = ui.renderResource(ui.cases[0], { 'index.html': '<p>Exact data</p>' }, 'a'.repeat(64));
  const sources = ui.sourceNames.map(name => {
    const original = fs.readFileSync(path.resolve(__dirname, '../../src/tests/support/windows/webapp', name));
    const generated = name === 'UiArtifactResource.cs', bytes = generated ? Buffer.from(resource) : original;
    fs.writeFileSync(path.join(directory, name), bytes);
    return { path: name, sha256: sha(bytes), ...(generated ? { template_sha256: sha(original) } : {}) };
  });
  const inputs = { sources };
  ui.validateSources(inputs, directory, resource);
  for (const name of ['WebViewHost.cs', 'NativeProbe.cs', 'WorkerGuardian.cs', 'Invoke-NativeProbe.ps1']) {
    const file = path.join(directory, name), bytes = fs.readFileSync(file), entry = sources.find(source => source.path === name), prior = entry.sha256;
    fs.writeFileSync(file, 'altered wrapper'); entry.sha256 = sha('altered wrapper');
    assert.throws(() => ui.validateSources(inputs, directory, resource), /reviewed source/);
    fs.writeFileSync(file, bytes); entry.sha256 = prior;
  }
  assert.throws(() => ui.validateSources({ sources: sources.slice(1) }, directory, resource), /closure/);
  assert.throws(() => ui.validateSources({ sources: [...sources, sources[0]] }, directory, resource), /closure/);
  assert.throws(() => ui.validateSources(inputs, directory, resource + 'changed'), /resource/);
  sources.find(source => source.path === 'UiArtifactResource.cs').template_sha256 = '0'.repeat(64);
  assert.throws(() => ui.validateSources(inputs, directory, resource), /reviewed source/);
});
