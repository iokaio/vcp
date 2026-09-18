// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { validatePath, parseTree, attachContent, inventory } = require('../support/upstream-inventory.cjs');
// These test argument/path rejection, not PowerShell startup performance. A
// hosted Windows preflight exceeded 20 seconds; keep a bounded native deadline.
const preflightTimeout = 60000;
function completedPreflight(result) {
  assert.equal(result.error, undefined,
    `PowerShell preflight failed: ${result.error?.code || 'unknown'}\n${result.stdout || ''}\n${result.stderr || ''}`);
}
// Git's well-known blob identity for the exact bytes "hello\n".
const object = 'ce013625030ba8dba906f756967f9e9ca394464a';
test('rejects traversal, metadata, device paths and Windows case collisions', () => {
  assert.throws(() => inventory('.', 'main'), /immutable/);
  for (const file of ['../outside', '/absolute', 'a/../b', 'a\\b', 'a/.git/config', 'nul.txt', 'a/stream:secret', 'a/trailing.']) assert.throws(() => validatePath(file));
  assert.equal(validatePath('src/Ångström/file.rs'), 'src/Ångström/file.rs');
  assert.throws(() => parseTree(Buffer.from(`100644 blob ${object} 6\tsrc/Name\0` + `100644 blob ${object} 6\tsrc/name\0`)), /colliding/);
  assert.throws(() => parseTree(Buffer.from(`160000 commit ${object} -\tsubmodule\0`)), /Unsupported/);
  assert.throws(() => parseTree(Buffer.from(`100644 blob ${object} 6\tDir/a\0` + `100644 blob ${object} 6\tdir/b\0`)), /colliding/);
  assert.throws(() => parseTree(Buffer.from(`100644 blob ${object} 6\tdir\0` + `100644 blob ${object} 6\tdir/b\0`)), /colliding/);
  assert.throws(() => parseTree(Buffer.from([0xff])), /encoded data/);
});
test('baseline command refuses build output inside the unmodified source root', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/upstream/build-baseline.ps1'),
      '-SourceRoot', fixture.root, '-Commit', '0'.repeat(40), '-OutputRoot', path.join(fixture.root, 'output')],
    { encoding: 'utf8', timeout: preflightTimeout, windowsHide: true });
    completedPreflight(result);
    assert.equal(result.status, 2, result.stdout + result.stderr);
    assert.match(result.stderr, /\[BASELINE_OUTPUT_IN_SOURCE\]/);
    assert.equal(fs.existsSync(path.join(fixture.root, 'output')), false);
  } finally { fixture.cleanup(); }
});
test('hashes original bytes and rejects changed, truncated or extra batch output', () => {
  const entries = parseTree(Buffer.from(`100644 blob ${object} 6\tsrc/hello.txt\0`));
  const batch = Buffer.from(`${object} blob 6\nhello\n\n`);
  const result = attachContent(entries, batch);
  assert.equal(result[0].sha256, '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03');
  assert.equal(result[0].kind, 'file');
  assert.throws(() => attachContent(entries, batch.subarray(0, batch.length - 1)), /Truncated/);
  assert.throws(() => attachContent(entries, Buffer.concat([batch, Buffer.from('extra')])));
  assert.throws(() => attachContent(entries, Buffer.from(`${object} blob 6\nHello\n\n`)), /identity mismatch/);
});
test('either shared component protects both selected source trees before output allocation', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const script = path.join(fixture.root, 'scripts/upstream/build-baseline.ps1');
    fs.mkdirSync(path.dirname(script), { recursive: true });
    fs.copyFileSync(path.resolve(__dirname, '../../../scripts/upstream/build-baseline.ps1'), script);
    const components = path.join(fixture.root, 'src/third_party/components');
    fs.mkdirSync(components, { recursive: true });
    for (const component of ['codex', 'munarium']) {
      fs.mkdirSync(path.join(fixture.root, 'src/third_party', component));
      fs.writeFileSync(path.join(components, component + '-selection.json'), JSON.stringify({ commit: '0'.repeat(40) }));
    }
    const outside = path.join(fixture.container, 'unallocated-evidence');
    for (const [selection, component] of [['-SelectedCodex', 'munarium'], ['-SelectedMunarium', 'codex']]) {
      const inside = path.join(fixture.root, 'src/third_party', component, 'forbidden-output');
      for (const args of [['-OutputRoot', inside], ['-OutputRoot', outside, '-TargetRoot', inside]]) {
        const result = spawnSync('pwsh', ['-NoProfile', '-File', script, selection, ...args],
          { encoding: 'utf8', timeout: preflightTimeout, windowsHide: true });
        completedPreflight(result);
        assert.equal(result.status, 2, result.stdout + result.stderr);
        assert.match(result.stderr, /\[BASELINE_OUTPUT_IN_SOURCE\]/);
        assert.equal(fs.existsSync(inside), false);
        assert.equal(fs.existsSync(outside), false);
      }
    }
  } finally { fixture.cleanup(); }
});
test('Windows short source aliases cannot bypass evidence or target containment', { skip: process.platform !== 'win32' }, t => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const alias = spawnSync('pwsh', ['-NoProfile', '-Command',
      '(New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:VCP_PATH_FIXTURE).ShortPath'],
    { env: { ...process.env, VCP_PATH_FIXTURE: fixture.root }, encoding: 'utf8', timeout: preflightTimeout, windowsHide: true });
    completedPreflight(alias);
    assert.equal(alias.status, 0, alias.stderr);
    const shortRoot = alias.stdout.trim();
    if (shortRoot.toLowerCase() === fixture.root.toLowerCase()) { t.skip('Fixture volume has no Windows short alias'); return; }
    const inside = path.join(shortRoot, 'forbidden-output');
    const outside = path.join(fixture.container, 'evidence');
    for (const args of [['-OutputRoot', inside], ['-OutputRoot', outside, '-TargetRoot', inside]]) {
      const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/upstream/build-baseline.ps1'),
        '-SourceRoot', shortRoot, '-Commit', '0'.repeat(40), ...args],
      { encoding: 'utf8', timeout: preflightTimeout, windowsHide: true });
      completedPreflight(result);
      assert.equal(result.status, 2, result.stdout + result.stderr);
      assert.match(result.stderr, /\[BASELINE_OUTPUT_IN_SOURCE\]/);
      assert.equal(fs.existsSync(inside), false);
      assert.equal(fs.existsSync(outside), false);
    }
  } finally { fixture.cleanup(); }
});
test('compiler experiments reject mutable aliases before allocating output', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const output = path.join(fixture.container, 'experiment-output');
    const result = spawnSync('pwsh', ['-NoProfile', '-File', path.resolve(__dirname, '../../../scripts/upstream/build-baseline.ps1'),
      '-SourceRoot', fixture.root, '-Commit', '0'.repeat(40), '-OutputRoot', output, '-ExperimentToolchain', 'stable'],
    { encoding: 'utf8', timeout: preflightTimeout, windowsHide: true });
    completedPreflight(result);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /ExperimentToolchain/);
    assert.equal(fs.existsSync(output), false);
  } finally { fixture.cleanup(); }
});
