// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { execFileSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { inventory } = require('../support/upstream-inventory.cjs');
const { readComponent, reconstruct, compareTree, compareIndex, applyPatches, sha256, validateSelection } = require('../support/upstream-selection.cjs');
function prepared(file, text, mode = '100644') {
  const content = Buffer.from(text);
  return { path: file, content, result: { bytes: content.length, sha256: sha256(content), mode } };
}
test('ordinary source verification rejects tampered declared patch bytes', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const base = path.join(fixture.root, 'src/third_party');
    fs.mkdirSync(path.join(base, 'components'), { recursive: true });
    fs.mkdirSync(path.join(base, 'patches/fixture'), { recursive: true });
    const commit = 'a'.repeat(40), patchPath = 'src/third_party/patches/fixture/change.patch';
    const patch = Buffer.from('Synthetic patch bytes, no application in this identity test\n');
    fs.writeFileSync(path.join(fixture.root, patchPath), patch);
    fs.writeFileSync(path.join(base, 'upstreams.toml'), 'schema_version=1\n[[upstream]]\nid="fixture"\norigin="https://github.com/example/fixture"\ncommit="' + commit + '"\nstate="imported_unqualified"\nselection="components/fixture.json"\n');
    const selection = { schema_version: 1, component: 'fixture', commit, destination: 'src/third_party/fixture', owner_task: 'P0-07',
      include: ['LICENSE'], materialize_links: {}, result_inventory: 'src/third_party/components/files.json', licenses: ['LICENSE'], closure: ['LICENSE'],
      patches: [{ path: patchPath, sha256: sha256(patch) }] };
    fs.writeFileSync(path.join(base, 'components/fixture.json'), JSON.stringify(selection));
    assert.equal(readComponent(fixture.root, 'fixture').component.commit, commit);
    fs.appendFileSync(path.join(fixture.root, patchPath), 'tampered');
    assert.throws(() => readComponent(fixture.root, 'fixture'), /Patch digest mismatch/);
  } finally { fixture.cleanup(); }
});
test('reconstruction reproduces pinned bytes and detects changed, extra and missing files', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const repository = path.resolve(__dirname, '../../..');
    const commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repository, encoding: 'utf8' }).trim();
    const original = inventory(repository, commit);
    const component = { id: 'fixture', commit, tree: original.tree, inventory_sha256: original.files_sha256 };
    const selection = { schema_version: 1, component: 'fixture', commit, destination: 'src/third_party/fixture', owner_task: 'P0-07',
      include: ['LICENSE', 'README.md'], materialize_links: {}, patches: [], result_inventory: 'src/third_party/components/fixture.json',
      licenses: ['LICENSE'], closure: ['README.md'] };
    const output = path.join(fixture.root, 'copy');
    const record = reconstruct({ repository, source: repository, output, component, selection });
    assert.deepEqual(compareTree(output, record), []);
    assert.throws(() => reconstruct({ repository, source: repository, output, component, selection }), /already exists/);
    fs.appendFileSync(path.join(output, 'README.md'), 'changed');
    fs.unlinkSync(path.join(output, 'LICENSE'));
    fs.writeFileSync(path.join(output, 'extra'), 'extra');
    assert.deepEqual(compareTree(output, record).sort(), ['Changed: README.md', 'Missing: LICENSE', 'Unexpected: extra']);
    const bad = { ...selection, patches: [{ path: 'src/third_party/patches/fixture/missing.patch', sha256: '0'.repeat(64) }] };
    const absent = path.join(fixture.root, 'absent');
    assert.throws(() => reconstruct({ repository, source: repository, output: absent, component, selection: bad }), /ENOENT/);
    assert.equal(fs.existsSync(absent), false);
    const patchRoot = path.join(fixture.root, 'src/third_party/patches/fixture');
    fs.mkdirSync(patchRoot, { recursive: true });
    const firstLines = execFileSync('git', ['show', commit + ':README.md'], { cwd: repository }).toString().split('\n').slice(0, 4);
    const edit = Buffer.from('diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,4 +1,5 @@\n ' + firstLines[0] + '\n+Synthetic qualification marker\n' + firstLines.slice(1).map(line => ' ' + line + '\n').join(''));
    fs.writeFileSync(path.join(patchRoot, 'readme.patch'), edit);
    const edited = reconstruct({ repository: fixture.root, source: repository, output: path.join(fixture.root, 'edited'), component,
      selection: { ...selection, patches: [{ path: 'src/third_party/patches/fixture/readme.patch', sha256: sha256(edit) }] } });
    assert.equal(edited.files.find(file => file.path === 'README.md').transformation, 'patch-series');
    assert.equal(edited.files.find(file => file.path === 'LICENSE').transformation, 'none');
    const originalLicense = execFileSync('git', ['show', commit + ':LICENSE'], { cwd: repository }).toString();
    const lines = originalLicense.trimEnd().split('\n');
    const deletion = Buffer.from('diff --git a/LICENSE b/LICENSE\ndeleted file mode 100644\n--- a/LICENSE\n+++ /dev/null\n@@ -1,' + lines.length + ' +0,0 @@\n' + lines.map(line => '-' + line + '\n').join(''));
    fs.writeFileSync(path.join(patchRoot, 'delete.patch'), deletion);
    const removingLicense = { ...selection, patches: [{ path: 'src/third_party/patches/fixture/delete.patch', sha256: sha256(deletion) }] };
    const tampered = { ...removingLicense, patches: [{ ...removingLicense.patches[0], sha256: '0'.repeat(64) }] };
    assert.throws(() => reconstruct({ repository: fixture.root, source: repository, output: absent, component, selection: tampered }), /Patch digest mismatch/);
    assert.equal(fs.existsSync(absent), false);
    assert.throws(() => reconstruct({ repository: fixture.root, source: repository, output: absent, component, selection: removingLicense }), /removed required selection input: LICENSE/);
    assert.throws(() => validateSelection({ ...selection, include: ['../outside'] }, component));
    assert.throws(() => validateSelection({ ...selection, patches: [{ path: 'README.md', sha256: '0'.repeat(64) }] }, component));
  } finally { fixture.cleanup(); }
});
test('ordered patches preserve changed bytes and new executable mode without nested Git metadata', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const entry = prepared('file.txt', 'before\n');
    const longEntry = prepared(('long-segment-'.repeat(5) + '/').repeat(4) + 'input.txt', 'preserved long-path bytes\n');
    fs.mkdirSync(path.dirname(path.join(fixture.root, longEntry.path)), { recursive: true });
    fs.writeFileSync(path.join(fixture.root, longEntry.path), longEntry.content);
    fs.writeFileSync(path.join(fixture.root, entry.path), entry.content);
    const patch = Buffer.from('diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-before\n+after\n' +
      'diff --git a/run.sh b/run.sh\nnew file mode 100755\n--- /dev/null\n+++ b/run.sh\n@@ -0,0 +1 @@\n+echo test\n');
    const actual = applyPatches(fixture.root, [entry, longEntry], [patch]);
    assert.equal(actual.find(file => file.path === longEntry.path).sha256, longEntry.result.sha256);
    assert.equal(actual.find(file => file.path === 'file.txt').sha256, sha256(Buffer.from('after\n')));
    assert.equal(actual.find(file => file.path === 'run.sh').mode, '100755');
    assert.equal(fs.existsSync(path.join(fixture.root, '.git')), false);
  } finally { fixture.cleanup(); }
});
test('patches cannot escape the reconstructed root', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const entry = prepared('file.txt', 'before\n');
    fs.writeFileSync(path.join(fixture.root, entry.path), entry.content);
    const patch = Buffer.from('diff --git a/../outside b/../outside\nnew file mode 100644\n--- /dev/null\n+++ b/../outside\n@@ -0,0 +1 @@\n+escape\n');
    assert.throws(() => applyPatches(fixture.root, [entry], [patch]));
    assert.equal(fs.existsSync(path.join(fixture.container, 'outside')), false);
  } finally { fixture.cleanup(); }
});
test('index verification detects Windows executable-mode drift and changed staged bytes', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const git = args => execFileSync('git', args, { cwd: fixture.root, stdio: ['ignore', 'pipe', 'pipe'] });
    git(['init']);
    fs.mkdirSync(path.join(fixture.root, 'source'));
    const file = path.join(fixture.root, 'source/run.sh');
    fs.writeFileSync(file, 'echo test\n');
    git(['-c', 'core.autocrlf=false', 'add', '--', 'source/run.sh']);
    const entry = prepared('run.sh', 'echo test\n', '100755');
    const expected = { files: [{ path: entry.path, result: entry.result }] };
    git(['update-index', '--chmod=-x', '--', 'source/run.sh']);
    assert.throws(() => compareIndex(fixture.root, 'source', expected), /mode/);
    git(['update-index', '--chmod=+x', '--', 'source/run.sh']);
    assert.equal(compareIndex(fixture.root, 'source', expected), 1);
    fs.writeFileSync(file, 'echo FAIL\n');
    git(['-c', 'core.autocrlf=false', 'add', '--', 'source/run.sh']);
    git(['update-index', '--chmod=+x', '--', 'source/run.sh']);
    assert.throws(() => compareIndex(fixture.root, 'source', expected), /Changed indexed bytes/);
  } finally { fixture.cleanup(); }
});
