// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const { spawnSync } = require('node:child_process');
const script = path.resolve(__dirname, '../../../scripts/evals/prepare-recovery-fixture.ps1');
test('private recovery fixture preparation refuses unsafe roots before invoking a generator', { skip: process.platform !== 'win32' }, t => {
  const privateRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-fixture-boundary-'));
  t.after(() => fs.rmSync(privateRoot, { recursive: true, force: true }));
  const sentinel = path.join(privateRoot, 'preserve.txt');
  fs.writeFileSync(sentinel, 'preserve existing private contents');
  const cases = [
    [privateRoot, [], /never reuses an existing output directory/],
    [path.resolve(__dirname, '../fixture-must-not-be-created'), [], /outside repository trees/],
    [path.join(privateRoot, 'sync', 'new'), ['-SyncRoots', path.join(privateRoot, 'sync')], /overlaps a declared or known sync root/],
  ];
  const linked = path.join(privateRoot, 'redirected');
  fs.symlinkSync(privateRoot, linked, 'junction');
  cases.push([path.join(linked, 'new'), [], /redirected or non-directory ancestors/]);
  for (const [root, extra, diagnostic] of cases) {
    const existed = fs.existsSync(root);
    const result = spawnSync('pwsh', ['-NoProfile', '-File', script, '-TestExecutable', process.execPath,
      '-ExpectedSha256', 'a'.repeat(64), '-GitExecutable', process.execPath, '-OutputRoot', root, ...extra],
    { encoding: 'utf8', windowsHide: true, timeout: 15000 });
    assert.ifError(result.error);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr + result.stdout, diagnostic);
    assert.equal(fs.existsSync(root), existed, 'refusal must not create an output directory');
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'preserve existing private contents');
  }
});
