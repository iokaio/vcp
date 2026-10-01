// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const script = path.resolve(__dirname, '../../../scripts/release/candidate-state.ps1');
const ids = ['source-gate', 'provision', 'portable-contracts', 'production-build', 'native-package',
  'setup-package', 'vsix-package', 'pair', 'native-boundaries', 'installed-native', 'installed-editor'];
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const windows = { skip: process.platform !== 'win32' };

function fixture(t, next = 8) {
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-state-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const output = path.join(root, 'candidate'), repository = path.join(root, 'repository');
  fs.mkdirSync(repository); fs.mkdirSync(path.join(output, 'logs'), { recursive: true });
  const write = (file, bytes) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes); return file; };
  const run = { schema: 'vcp-candidate-run/1', status: 'running', reviewed_commit: 'a'.repeat(40),
    repository_root: repository, output_root: output, stop_after: 'installed-editor', stages: [], receipts: {}, receipt_sha256: {} };
  for (const id of ids.slice(0, next)) {
    const bytes = `${id} observed pass\n`, log = write(path.join(output, 'logs', `${id}.log`), bytes);
    run.stages.push({ id, status: 'pass', exit_code: 0, log, log_sha256: digest(bytes) });
  }
  const guid = '11111111-2222-3333-4444-555555555555';
  const receipts = [
    ['build', 3, `build/${guid}/build-receipt.json`], ['native', 4, `native/${guid}/result.json`],
    ['setup', 5, `setup/${'1'.repeat(32)}/setup-result.json`], ['vsix', 6, 'vsix/manifest.json'],
  ];
  for (const [name, index, relative] of receipts) if (next > index) {
    const bytes = JSON.stringify({ fixture_receipt: name });
    run.receipts[name] = write(path.join(output, relative), bytes); run.receipt_sha256[name] = digest(bytes);
  }
  if (next > 7) {
    write(path.join(output, 'pair.json'), 'independently verified pair bytes');
    run.stages[7].verified_pair_sha256 = digest('independently verified pair bytes');
  }
  const runPath = path.join(output, 'run.json');
  const input = { output, repository, reviewed_commit: run.reviewed_commit, stage: ids[next], stop_after: run.stop_after, run_path: runPath };
  const runner = write(path.join(root, 'exercise.ps1'), `param([string]$Script,[string]$InputFile,[string]$Mode)
$ErrorActionPreference='Stop'
. $Script
$input=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json -AsHashtable
$run=Get-Content -LiteralPath $input.run_path -Raw | ConvertFrom-Json -AsHashtable
if($Mode -ceq 'assert') {
    Assert-CandidateContinuation $run $input.output $input.repository $input.reviewed_commit $input.stage $input.stop_after
    @{status='pass';stages=@(Get-CandidateStageIds)} | ConvertTo-Json -Compress
} elseif($Mode -ceq 'save') {
    $run.receipts.new_key='round-trip'; $run.receipt_sha256.new_key='new hash'
    Save-CandidateRun $run $input.run_path
    $loaded=Get-Content -LiteralPath $input.run_path -Raw | ConvertFrom-Json -AsHashtable
    $loaded.receipts.second_key='still mutable'
    Save-CandidateRun $loaded $input.run_path
    @{status='pass'} | ConvertTo-Json -Compress
} elseif($Mode -ceq 'locked-save') {
    $before=[IO.File]::ReadAllText($input.run_path)
    $lock=[IO.FileStream]::new($input.run_path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $failed=$false
    try { $run.status='changed'; Save-CandidateRun $run $input.run_path } catch { $failed=$true } finally { $lock.Dispose() }
    if(-not $failed -or [IO.File]::ReadAllText($input.run_path) -cne $before) { throw 'Failed rename did not preserve previous state' }
    $null=Get-Content -LiteralPath $input.run_path -Raw | ConvertFrom-Json -AsHashtable
    if(@(Get-ChildItem -LiteralPath $input.output -Filter '.run-*.tmp' -Force).Count) { throw 'Failed save leaked a temporary checkpoint' }
    @{status='pass';previous_preserved=$true} | ConvertTo-Json -Compress
} else { throw 'Unknown fixture action' }
`);
  const execute = (mode = 'assert') => {
    write(runPath, JSON.stringify(run));
    const inputFile = write(path.join(root, 'input.json'), JSON.stringify(input));
    const before = fs.readFileSync(runPath);
    const result = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Script', script, '-InputFile', inputFile, '-Mode', mode],
      { encoding: 'utf8', windowsHide: true, timeout: 15000 });
    assert.ifError(result.error);
    if (mode === 'assert') assert.deepEqual(fs.readFileSync(runPath), before, 'Continuation must not rewrite its checkpoint');
    return result;
  };
  return { root, output, repository, run, runPath, input, execute, write };
}

test('each stage accepts only its successful prior prefix loaded in a fresh PowerShell process', windows, t => {
  for (let next = 1; next < ids.length; next++) {
    const f = fixture(t, next), result = f.execute();
    assert.equal(result.status, 0, `${ids[next]}: ${result.stderr}`);
    assert.deepEqual(JSON.parse(result.stdout).stages, ids);
  }
  for (const stop of ['portable-contracts', 'production-build', 'pair', 'installed-editor']) {
    const f = fixture(t, ids.indexOf(stop)); f.run.stop_after = stop; f.input.stop_after = stop;
    assert.equal(f.execute().status, 0, stop);
  }
});

test('continuation refuses changed source, roots, scope, replay, skipped and incomplete stages', windows, t => {
  const mutations = [
    f => { f.input.reviewed_commit = 'b'.repeat(40); },
    f => { f.run.schema = 'vcp-candidate-run/2'; },
    f => { f.run.status = 'pass'; },
    f => { f.run.status = 'fail'; },
    f => { f.run.repository_root = f.output; },
    f => { f.run.output_root = f.repository; },
    f => { f.input.stop_after = 'pair'; },
    f => { f.input.stage = 'source-gate'; },
    f => { f.input.stage = 'NATIVE-BOUNDARIES'; },
    f => { f.input.stage = 'all'; },
    f => { f.input.stage = 'installed-editor'; },
    f => { f.run.stages.pop(); },
    f => { f.run.stages.push(f.run.stages[0]); },
    f => { f.run.stages[1] = { ...f.run.stages[0] }; },
    f => { [f.run.stages[0], f.run.stages[1]] = [f.run.stages[1], f.run.stages[0]]; },
    f => { f.run.stages[0].status = 'running'; },
    f => { f.run.stages[0].status = 'fail'; },
    f => { f.run.stages[0].exit_code = 1; },
    f => { f.run.stages[0].exit_code = '0'; },
  ];
  for (const mutate of mutations) {
    const f = fixture(t); mutate(f); const result = f.execute();
    assert.notEqual(result.status, 0, mutate.toString());
  }
});

test('continuation refuses changed, missing, misplaced or unbound logs and receipts', windows, t => {
  const mutations = [
    f => { fs.appendFileSync(f.run.stages[0].log, 'changed'); },
    f => { fs.unlinkSync(f.run.stages[0].log); },
    f => { delete f.run.stages[0].log_sha256; },
    f => { f.run.stages[0].log = f.write(path.join(f.output, 'wrong.log'), 'source-gate observed pass\n'); },
    f => { fs.appendFileSync(f.run.receipts.build, 'changed'); },
    f => { fs.unlinkSync(f.run.receipts.native); },
    f => { delete f.run.receipts.setup; delete f.run.receipt_sha256.setup; },
    f => { delete f.run.receipt_sha256.vsix; },
    f => { f.run.receipt_sha256.unknown = '0'.repeat(64); },
    f => { f.run.receipts.unknown = f.run.receipts.build; f.run.receipt_sha256.unknown = f.run.receipt_sha256.build; },
    f => { f.run.receipts.build = f.write(path.join(f.output, 'build', 'not-a-guid', 'build-receipt.json'), fs.readFileSync(f.run.receipts.build)); },
    f => { f.run.receipts.native = f.write(path.join(f.repository, 'native', path.basename(path.dirname(f.run.receipts.native)), 'result.json'), fs.readFileSync(f.run.receipts.native)); },
    f => { fs.appendFileSync(path.join(f.output, 'pair.json'), 'changed'); },
    f => { fs.unlinkSync(path.join(f.output, 'pair.json')); },
    f => { delete f.run.stages[7].verified_pair_sha256; },
  ];
  for (const mutate of mutations) {
    const f = fixture(t); mutate(f); assert.notEqual(f.execute().status, 0, mutate.toString());
  }
  const f = fixture(t, 1);
  f.run.receipts.build = f.write(path.join(f.output, 'future.json'), '{}'); f.run.receipt_sha256.build = digest('{}');
  assert.notEqual(f.execute().status, 0, 'Future receipts cannot create an accepted production state');
});

test('optional delivery is bound to its exact repository receipt and digest', windows, t => {
  const f = fixture(t, 1), bytes = 'delivery gate evidence';
  f.run.receipts.delivery = f.write(path.join(f.repository, 'artifacts/beta-gate/delivery.json'), bytes);
  f.run.receipt_sha256.delivery = digest(bytes);
  assert.equal(f.execute().status, 0);
  fs.appendFileSync(f.run.receipts.delivery, 'changed'); assert.notEqual(f.execute().status, 0);
  f.run.receipts.delivery = f.write(path.join(f.output, 'delivery.json'), bytes); assert.notEqual(f.execute().status, 0);
});

test('continuation refuses redirected output, log and receipt ancestors', windows, t => {
  for (const kind of ['output', 'log', 'receipt']) {
    const f = fixture(t);
    if (kind === 'output') {
      const alias = path.join(f.root, 'alias'); fs.symlinkSync(f.output, alias, 'junction'); f.input.output = alias;
    } else {
      const parent = kind === 'log' ? path.join(f.output, 'logs') : path.dirname(f.run.receipts.build);
      const moved = path.join(f.root, 'preserved-' + kind); fs.renameSync(parent, moved); fs.symlinkSync(moved, parent, 'junction');
    }
    assert.notEqual(f.execute().status, 0, kind);
  }
});

test('atomic save round-trips mutable dictionaries and preserves the old JSON on rename failure', windows, t => {
  const f = fixture(t, 1), saved = f.execute('save'); assert.equal(saved.status, 0, saved.stderr);
  const loaded = JSON.parse(fs.readFileSync(f.runPath));
  assert.equal(loaded.receipts.new_key, 'round-trip'); assert.equal(loaded.receipts.second_key, 'still mutable');
  assert.equal(loaded.receipt_sha256.new_key, 'new hash');
  assert.equal(fs.readFileSync(f.runPath)[0], '{'.charCodeAt(0), 'UTF-8 checkpoint has no BOM');
  assert.deepEqual(fs.readdirSync(f.output).filter(name => name.startsWith('.run-')), []);
  const failed = f.execute('locked-save'); assert.equal(failed.status, 0, failed.stderr);
  assert.equal(JSON.parse(failed.stdout).previous_preserved, true);
});

test('save refuses an unowned destination or redirected parent without modifying that target', windows, t => {
  const f = fixture(t, 1), outside = f.write(path.join(f.root, 'outside.json'), JSON.stringify(f.run));
  const original = fs.readFileSync(outside); f.input.run_path = outside;
  assert.notEqual(f.execute('save').status, 0); assert.deepEqual(fs.readFileSync(outside), original);
  const alias = path.join(f.root, 'alias'); fs.symlinkSync(f.output, alias, 'junction');
  f.input.run_path = f.runPath; f.run.output_root = alias;
  assert.notEqual(f.execute('save').status, 0);
  assert.deepEqual(fs.readdirSync(f.output).filter(name => name.startsWith('.run-')), []);
});
