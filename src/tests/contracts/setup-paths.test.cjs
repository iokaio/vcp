// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { derive, inspect } = require('../../../scripts/installer/path-limits.cjs');
const repository = path.resolve(__dirname, '../../..');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'vcp-setup-paths-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'maintenance'));
  fs.writeFileSync(path.join(root, 'vcp.exe'), 'Synthetic launcher; not executable.');
  fs.copyFileSync(path.join(repository, 'scripts/installer/shell.ps1'), path.join(root, 'maintenance/shell.ps1'));
  fs.copyFileSync(path.join(repository, 'scripts/package-install.ps1'), path.join(root, 'maintenance/package-install.ps1'));
  fs.cpSync(path.join(repository, 'release/installer-notices'), path.join(root, 'setup-notices'), { recursive: true });
  return root;
}

test('actual setup integration inventory fits the computed maximum and fails one UTF-16 unit later', t => {
  const result = inspect(fixture(t));
  assert.equal(result.max_app_root_utf16, 207);
  assert.equal(result.max_file_relative_utf16, 51);
  assert.equal(result.max_temporary_relative_utf16, 51);
  assert(result.files.some(row => row.path === 'setup-notices/sources/theme/NewUxTheme.TmSchema.pas'));
  assert(!result.files.some(row => row.path === 'native.zip'));
  const app = 'C:\\Program Files β\\' + 'x'.repeat(207 - 'C:\\Program Files β\\'.length);
  for (const file of result.files) {
    const destination = app + '\\' + file.path.replaceAll('/', '\\');
    assert(destination.length <= 259, file.path);
    const directory = destination.slice(0, destination.lastIndexOf('\\'));
    assert(directory.length <= 248, file.path);
    assert((directory + '\\is-0123456789.tmp').length <= 259, file.path);
  }
  const longest = 'setup-notices\\sources\\theme\\NewUxTheme.TmSchema.pas';
  assert.equal((app + '\\' + longest).length, 259);
  assert.equal((app + 'x\\' + longest).length, 260);
});

test('temporary destination names and non-BMP characters contribute to the real bound', () => {
  const directory = '目录😀'.repeat(10); // 40 UTF-16 units, 30 Unicode code points.
  const result = derive([`${directory}/x`]);
  assert.equal(directory.length, 40);
  assert.equal(result.max_app_root_utf16, 200);
  assert.equal(result.max_file_relative_utf16, 42);
  assert.equal(result.max_temporary_relative_utf16, 58);
  assert.equal(200 + 1 + directory.length + 1 + 'is-0123456789.tmp'.length, 259);
  assert.equal(derive(['vcp.exe']).max_app_root_utf16, 238); // Includes ownership marker.
});

test('inventory records changed bytes and newly added destination depth', t => {
  const root = fixture(t), before = inspect(root);
  fs.appendFileSync(path.join(root, 'vcp.exe'), 'changed');
  const changed = inspect(root);
  assert.notEqual(changed.files_sha256, before.files_sha256);
  assert.equal(changed.max_app_root_utf16, before.max_app_root_utf16);
  const deeper = path.join(root, 'setup-notices', 'd'.repeat(60));
  fs.mkdirSync(deeper);
  fs.writeFileSync(path.join(deeper, 'x.txt'), 'new integration file');
  const added = inspect(root);
  assert.equal(added.max_app_root_utf16, 166);
  assert.notEqual(added.files_sha256, changed.files_sha256);
});

test('unsafe, ambiguous, reserved, or unrepresentable destinations are refused', () => {
  for (const names of [[], ['../escape'], ['C:/escape'], ['nested\\file'], ['A', 'a'],
    ['unins000.exe'], ['.vcp-setup-owned.ini'], [`${'x'.repeat(250)}/file`]]) {
    assert.throws(() => derive(names));
  }
});

test('redirected staged entries are refused without touching their targets', t => {
  const root = fixture(t), outside = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'vcp-setup-external-'));
  t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
  fs.writeFileSync(path.join(outside, 'sentinel'), 'preserve');
  fs.symlinkSync(outside, path.join(root, 'redirected'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => inspect(root), /Linked/);
  assert.equal(fs.readFileSync(path.join(outside, 'sentinel'), 'utf8'), 'preserve');
});

test('compiled setup uses the measured tree and guards before mutations and zero exit', () => {
  const source = fs.readFileSync(path.join(repository, 'scripts/installer/vcp.iss'), 'utf8');
  const files = source.split('[Files]')[1].split('[Icons]')[0];
  assert.match(files, /Source: "\{#NativeArchive\}"; DestName: "native.zip"; Flags: dontcopy/);
  assert.match(files, /Source: "\{#SetupFiles\}\\\*"; DestDir: "\{app\}"; Flags: ignoreversion recursesubdirs/);
  assert(!files.includes('createallsubdirs'), 'Unmeasured empty directories must not be installed');
  const prepare = source.split('function PrepareToInstall(')[1].split('procedure CurStepChanged')[0];
  assert(prepare.indexOf("AppRootLengthError(ExpandConstant('{app}'))") < prepare.indexOf('ExtractTemporaryFile('));
  assert.match(prepare, /if Result <> '' then Exit;/);
  assert.match(source, /if Length\(Root\) > \{#MaxAppRootLength\} then/);
  const post = source.split('procedure CurStepChanged(')[1].split('procedure DeinitializeSetup')[0];
  assert(post.indexOf('PostInstallVerificationFailed := True') < post.indexOf('RunScript('));
  assert(post.indexOf('PostInstallVerificationFailed := False') > post.indexOf('RaiseException('));
  assert.match(post, /function GetCustomSetupExitCode: Integer;[\s\S]*if PostInstallVerificationFailed then Result := 1001 else Result := 0;/);
});
