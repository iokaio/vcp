// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { spawnSync } = require('node:child_process');
const helper = path.resolve(__dirname, '../../../scripts/release/editor-layout.ps1');
const pins = require('../../../release/candidate-tools.json').editor;
const runtimeFiles = ['ffmpeg.dll', 'libEGL.dll', 'libGLESv2.dll', 'icudtl.dat', 'v8_context_snapshot.bin',
  'resources/app/out/cli.js', 'resources/app/out/main.js', 'resources/app/out/vs/workbench/workbench.desktop.main.js'];
function harness(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-editor-layout-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const runner = path.join(root, 'resolve.ps1');
  fs.writeFileSync(runner, `param([string]$Helper,[string]$Code,[string]$Archive)
$ErrorActionPreference='Stop'
. $Helper
$layout=Resolve-BetaEditor -Code $Code
if($Archive){
  if((Get-FileHash -LiteralPath $Archive).Hash.ToLowerInvariant() -cne $layout.archive_sha256){throw 'Real pinned archive hash differs'}
  $zip=[IO.Compression.ZipFile]::OpenRead($Archive)
  try {
    foreach($file in @($layout.code,(Join-Path $layout.app 'package.json'),(Join-Path $layout.app 'product.json'),$layout.cli)){
      $relative=[IO.Path]::GetRelativePath($layout.root,$file).Replace('\\','/')
      $entries=@($zip.Entries | Where-Object FullName -CEQ $relative)
      if($entries.Count -ne 1){throw 'Exactly one bound archive entry required'}
      $stream=$entries[0].Open(); $digest=[Security.Cryptography.SHA256]::Create()
      try {$expected=[Convert]::ToHexString($digest.ComputeHash($stream)).ToLowerInvariant()} finally {$digest.Dispose();$stream.Dispose()}
      if($expected -cne (Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant()){throw 'Extracted tool differs from pinned archive entry'}
    }
  } finally {$zip.Dispose()}
  if((Get-Item -LiteralPath $layout.code).VersionInfo.ProductVersion -cne $layout.version){throw 'Actual Code.exe product version differs'}
  $layout.archive_entries_verified=$true
}
$layout | ConvertTo-Json -Depth 6
`);
  const run = (code, archive) => spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Helper', helper, '-Code', code,
    ...(archive ? ['-Archive', archive] : [])], { encoding: 'utf8', windowsHide: true, timeout: 20000 });
  return { root, run };
}
function fixture(h, name, layout = 'versioned') {
  const root = path.join(h.root, name), code = path.join(root, 'Code.exe');
  const runtime = layout === 'flat' ? root : path.join(root, pins.commit.slice(0, 10));
  const write = (name, bytes) => { const file = path.join(runtime, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes); return file; };
  fs.mkdirSync(root, { recursive: true }); fs.writeFileSync(code, 'synthetic resolver fixture; never executed');
  for (const file of runtimeFiles) write(file, `synthetic ${file}`);
  const packageFile = write('resources/app/package.json', JSON.stringify({ version: pins.version }));
  const productFile = write('resources/app/product.json', JSON.stringify({ commit: pins.commit }));
  return { root, code, runtime, packageFile, productFile };
}

test('editor resolver selects the exact unique flat or commit-prefix runtime and CLI', { skip: process.platform !== 'win32' }, t => {
  const h = harness(t);
  for (const layout of ['flat', 'versioned']) {
    const f = fixture(h, layout, layout), result = h.run(f.code);
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
    const value = JSON.parse(result.stdout);
    assert.equal(value.root, f.root); assert.equal(value.runtime, f.runtime);
    assert.equal(value.cli, path.join(f.runtime, 'resources/app/out/cli.js'));
    assert.equal(value.version, pins.version); assert.equal(value.commit, pins.commit);
    assert.match(value.code_sha256, /^[a-f0-9]{64}$/);
  }
});

test('editor resolver refuses ambiguous, mismatched, missing or redirected runtime inputs', { skip: process.platform !== 'win32' }, t => {
  const h = harness(t);
  const cases = [
    f => fs.writeFileSync(f.productFile, JSON.stringify({ commit: 'a'.repeat(40) })),
    f => fs.writeFileSync(f.packageFile, JSON.stringify({ version: '1.137.0' })),
    f => fs.unlinkSync(path.join(f.runtime, 'resources/app/out/cli.js')),
    f => fs.unlinkSync(path.join(f.runtime, 'ffmpeg.dll')),
    f => { fs.mkdirSync(path.join(f.root, 'resources/app'), { recursive: true }); fs.copyFileSync(f.packageFile, path.join(f.root, 'resources/app/package.json')); },
    f => { const other = path.join(f.root, 'unexpected-runtime'); fs.renameSync(f.runtime, other); },
    f => { const other = path.join(h.root, 'outside-runtime'); fs.renameSync(f.runtime, other); fs.symlinkSync(other, f.runtime, 'junction'); },
    f => { const other = path.join(h.root, 'outside-app'); fs.renameSync(path.join(f.runtime, 'resources/app'), other); fs.symlinkSync(other, path.join(f.runtime, 'resources/app'), 'junction'); },
    f => { const alias = path.join(h.root, 'redirected-root'); fs.symlinkSync(f.root, alias, 'junction'); f.code = path.join(alias, 'Code.exe'); },
  ];
  for (let i = 0; i < cases.length; i++) {
    const f = fixture(h, `refusal-${i}`); cases[i](f); const result = h.run(f.code);
    assert.ifError(result.error); assert.notEqual(result.status, 0, `unsafe fixture ${i} accepted`);
    assert(fs.existsSync(f.code), 'resolver must not mutate supplied tool files');
  }
});

test('prepared official archive resolves actual versioned layout and binds executable/metadata/CLI bytes', {
  skip: process.platform !== 'win32' || !process.env.VCP_TEST_BETA_EDITOR_ARCHIVE || !process.env.VCP_TEST_BETA_EDITOR_CODE,
}, t => {
  const h = harness(t), result = h.run(process.env.VCP_TEST_BETA_EDITOR_CODE, process.env.VCP_TEST_BETA_EDITOR_ARCHIVE);
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  const value = JSON.parse(result.stdout);
  assert.equal(value.runtime, path.join(value.root, pins.commit.slice(0, 10)));
  assert.equal(value.archive_entries_verified, true); assert.equal(value.archive_sha256, pins.sha256);
});
