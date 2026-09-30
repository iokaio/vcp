// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const script = path.resolve(__dirname, '../../../scripts/release/candidate.ps1');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t, id = '11111111-2222-3333-4444-555555555555') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-storage-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const candidate = path.join(root, 'candidate'), build = path.join(candidate, 'build', id);
  const target = path.join(build, 'cargo-target'), preserved = new Map();
  const write = (file, value, retain = false) => {
    fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, value);
    if (retain) preserved.set(file, fs.readFileSync(file));
    return file;
  };
  write(path.join(target, 'nested', 'object.obj'), 'disposable compiler object');
  for (const name of ['vcp.exe', 'vcp-launch.exe', 'vcp.pdb', 'build.log', 'source-before.json', 'dependencies-before.json'])
    write(path.join(build, name), `preserve ${name}`, true);
  write(path.join(candidate, 'native', 'payload.zip'), 'preserve archive', true);
  write(path.join(candidate, 'qualification-target', 'existing-sentinel'), 'preserve qualification tree', true);
  const outside = path.join(root, 'outside'), sentinel = write(path.join(outside, 'sentinel'), 'preserve other directory', true);
  const receipt = {
    schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, qualification_build: false, profile: 'release',
    command: ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features', '-p', 'vcp-cli',
      '--bin', 'vcp', '--bin', 'vcp-launch', '--target', 'x86_64-pc-windows-msvc', '--target-dir', target, '-j', '2', '--message-format=json-render-diagnostics'],
    executable: path.join(build, 'vcp.exe'), launcher: path.join(build, 'vcp-launch.exe'),
    executable_sha256: sha(fs.readFileSync(path.join(build, 'vcp.exe'))), launcher_sha256: sha(fs.readFileSync(path.join(build, 'vcp-launch.exe'))),
    symbols_sha256: sha(fs.readFileSync(path.join(build, 'vcp.pdb'))), release: { candidate_id: 'a'.repeat(64) },
    compiler_artifact: { executable: path.join(target, 'x86_64-pc-windows-msvc', 'release', 'vcp.exe') },
    launcher_compiler_artifact: { executable: path.join(target, 'x86_64-pc-windows-msvc', 'release', 'vcp-launch.exe') },
  };
  const receiptFile = write(path.join(build, 'build-receipt.json'), JSON.stringify(receipt));
  const pairFile = write(path.join(candidate, 'pair.json'), JSON.stringify({ schema: 'vcp-release-pair/1', release: receipt.release }), true);
  const run = { receipts: { build: receiptFile }, stages: [
    ...['production-build', 'native-package', 'setup-package', 'vsix-package'].map(id => ({ id, status: 'pass' })),
    { id: 'pair', status: 'running', verified_pair_sha256: sha(fs.readFileSync(pairFile)) },
  ] };
  const runner = write(path.join(root, 'exercise.ps1'), `param([string]$Script,[string]$InputFile)
$ErrorActionPreference='Stop'
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Script,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Candidate script parse failed'}
$names=@('Get-CandidateDiskEvidence','Assert-CandidateOrdinaryPath','Remove-CandidateProductionTarget')
$functions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names},$true))
if($functions.Count -ne 3){throw 'Expected candidate storage functions'}
foreach($function in $functions){. ([scriptblock]::Create($function.Extent.Text))}
$input=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$before=Get-CandidateDiskEvidence
$removed=Remove-CandidateProductionTarget $input.candidate $input.run
@{disk=$before;cleanup=$removed} | ConvertTo-Json -Depth 10
`);
  const execute = () => {
    write(receiptFile, JSON.stringify(receipt));
    const input = write(path.join(root, 'input.json'), JSON.stringify({ candidate: f.candidate, run }));
    return spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Script', script, '-InputFile', input],
      { encoding: 'utf8', windowsHide: true, timeout: 15000 });
  };
  const checkPreserved = () => { for (const [file, bytes] of preserved) assert.deepEqual(fs.readFileSync(file), bytes, file); };
  const f = { root, candidate, build, target, outside, sentinel, receipt, run, execute, checkPreserved };
  return f;
}

test('paired production target cleanup removes only its exact tree and records disk evidence', { skip: process.platform !== 'win32' }, t => {
  const f = fixture(t), result = f.execute();
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.cleanup.status, 'removed'); assert.equal(report.cleanup.path, f.target);
  assert.equal(report.cleanup.removed_files, 1); assert.equal(report.cleanup.removed_file_bytes, Buffer.byteLength('disposable compiler object'));
  assert(Number.isSafeInteger(report.cleanup.observed_recovered_bytes));
  assert(report.disk.volumes.length > 0);
  for (const volume of report.disk.volumes) {
    assert(volume.capacity_bytes > 0); assert(volume.available_free_bytes >= 0); assert(volume.total_free_bytes >= 0);
  }
  assert.equal(fs.existsSync(f.target), false); f.checkPreserved();
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.build, 'build-receipt.json'))), f.receipt);
});

test('cleanup refuses failed pairing, mismatched targets and redirected paths without deleting sentinels', { skip: process.platform !== 'win32' }, t => {
  const cases = [
    f => { f.run.stages.find(row => row.id === 'pair').status = 'fail'; },
    f => { f.run.stages.find(row => row.id === 'pair').verified_pair_sha256 = 'b'.repeat(64); },
    f => { f.run.stages.find(row => row.id === 'production-build').status = 'fail'; },
    f => { f.receipt.cargo_exit_code = 1; },
    f => { f.receipt.command[16] = path.join(f.candidate, 'qualification-target'); },
    f => { f.receipt.executable_sha256 = 'b'.repeat(64); },
    f => { fs.symlinkSync(f.outside, path.join(f.target, 'redirected'), 'junction'); },
    f => { const alias = path.join(f.root, 'alias'); fs.symlinkSync(f.candidate, alias, 'junction'); f.candidate = alias; },
  ];
  for (const mutate of cases) {
    const f = fixture(t); mutate(f); const result = f.execute();
    assert.ifError(result.error); assert.notEqual(result.status, 0, result.stdout);
    assert(fs.existsSync(path.join(f.target, 'nested', 'object.obj'))); f.checkPreserved();
  }
  const f = fixture(t, 'malformed-build-directory'), result = f.execute();
  assert.ifError(result.error); assert.notEqual(result.status, 0); f.checkPreserved();
  assert(fs.existsSync(path.join(f.target, 'nested', 'object.obj')));
});
