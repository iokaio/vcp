// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const repository = path.resolve(__dirname, '../../..');
const workflow = fs.readFileSync(path.join(repository, '.github/workflows/ci.yml'), 'utf8');
const native = workflow.split(/^  native-boundaries:\r?\n/m)[1];
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
const commit = 'a'.repeat(40);
function blocks(text) {
  return [...text.matchAll(/^        run: \|\r?\n((?:          .*\r?\n|\r?\n)+)/gm)]
    .map(match => match[1].split(/\r?\n/).map(line => line.slice(10)).join('\n').trim());
}
const runs = blocks(native);
const gate = runs.find(text => text.startsWith('if ($env:STORAGE_ONLY'));
const exercise = runs.find(text => text.startsWith("$ErrorActionPreference='Stop'"));
const collectorBlock = runs.find(text => text.startsWith('$collector ='));
const collector = collectorBlock.split("$collector = @'\n")[1].split("\n'@")[0];
function temporary(t) {
  const root = fs.mkdtempSync(path.join(fs.realpathSync.native(os.tmpdir()), 'vcp-native-ci-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}
function powershell(t, body, inputs = {}) {
  const root = temporary(t), runner = path.join(root, 'test.ps1');
  for (const [name, bytes] of Object.entries(inputs)) fs.writeFileSync(path.join(root, name), bytes);
  fs.writeFileSync(runner, "$ErrorActionPreference='Stop'\n$Root=$PSScriptRoot\n" + body);
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner], {
    encoding: 'utf8', windowsHide: true, timeout: 15000, maxBuffer: 1024 * 1024,
  });
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr + result.stdout);
  return { root, result };
}

test('native dispatch is explicit, separately serialized, and leaves ordinary delivery selection intact', () => {
  assert(native); assert.equal(runs.length, 4);
  assert.match(native, /if: github.event_name == 'workflow_dispatch' && inputs.native_boundaries_only/);
  assert.match(native, /needs: windows-supervision/);
  assert.match(native, /group: wingroup\r?\n      labels: vcpwin/);
  assert.match(native, /CARGO_BUILD_JOBS: '16'/);
  assert.match(native, /permissions:\r?\n      contents: read\r?\n      actions: read/);
  assert.match(native, /^    timeout-minutes: 95$/m);
  assert.match(native, /name: Run exact source native boundary targets and package lifecycle fixture\r?\n        timeout-minutes: 78/);
  assert.match(workflow, /group: delivery-\$\{\{ github.workflow \}\}-\$\{\{ github.ref \}\}\$\{\{ inputs.native_boundaries_only && '-native-boundaries' \|\| '' \}\}/);
  for (const name of ['delivery', 'skill-helpers', 'skill-runtime']) {
    const job = workflow.split(new RegExp(`^  ${name}:\\r?\\n`, 'm'))[1].split(/^  [a-z-]+:\r?\n/m)[0];
    assert.match(job, /if: github.event_name != 'workflow_dispatch' \|\| !inputs.native_boundaries_only/);
  }
  assert.match(workflow, /if: github.event_name == 'workflow_dispatch' && !inputs.storage_only && !inputs.native_boundaries_only/);
  assert.match(workflow, /if: github.event_name == 'workflow_dispatch' && inputs.storage_only && !inputs.native_boundaries_only/);
  assert(!native.includes('--ignored')); assert(!native.includes('Download ')); assert(!native.includes('model-assets.cjs'));
  assert(!native.includes('build-production.ps1')); assert(!native.includes('build-setup.ps1'));
  assert.match(native, /path: artifacts\/native-boundaries-public\//);
  assert.equal((native.match(/if: always\(\)/g) || []).length, 2);
  assert(workflow.includes('node --test src/tests/contracts/build-progress.test.cjs src/tests/contracts/build-progress-services.test.cjs src/tests/contracts/ci-native-boundaries.test.cjs'));
});

test('every actual workflow PowerShell run block parses and native commands select the exact supervised scope', { skip: process.platform !== 'win32' }, t => {
  const all = blocks(workflow), files = Object.fromEntries(all.map((text, i) => [`block-${i}.ps1`, text]));
  const commandLines = exercise.split('\n').filter(line => /^\s*\$common=|^\s*Observe '/.test(line)).join('\n');
  const { result } = powershell(t, String.raw`
foreach($file in Get-ChildItem -LiteralPath $Root -Filter 'block-*.ps1') {
  $tokens=$null;$errors=$null
  $null=[Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)
  if($errors.Count){throw ($file.Name+': '+($errors.Message -join ', '))}
}
$calls=@();$env:CARGO_BUILD_JOBS='16';$target='C:\owned\qualification-target';$workspace='C:\owned\workspace';$repo='C:\owned\repository';$cargo='C:\tools\cargo.exe';$pwsh='C:\tools\pwsh.exe'
function Observe([string]$Name,[string]$Program,[string[]]$Arguments,[string]$Directory,[int]$Seconds,[switch]$Msvc) {
  $script:calls+=@{name=$Name;program=$Program;arguments=$Arguments;cwd=$Directory;seconds=$Seconds;msvc=[bool]$Msvc}
}
. (Join-Path $Root 'commands.ps1')
$calls | ConvertTo-Json -Depth 6 -Compress
`, { ...files, 'commands.ps1': commandLines });
  const calls = JSON.parse(result.stdout);
  assert.deepEqual(calls.map(row => [row.name, row.seconds, row.msvc]), [['native', 4500, true], ['package-install', 90, false]]);
  const common = ['+1.98.0', 'test', '--locked', '--offline', '--target', 'x86_64-pc-windows-msvc', '--target-dir', 'C:\\owned\\qualification-target', '-j', '16'];
  assert.deepEqual(calls[0].arguments.slice(0, common.length), common);
  const selected = row => row.arguments.flatMap((arg, i) => arg === '--test' ? [row.arguments[i + 1]] : []);
  assert.deepEqual(selected(calls[0]), ['duplex_process', 'local_execution_parity', 'installed_launcher', 'beta_launcher_console', 'beta_editor_candidate']);
  assert.deepEqual(calls[0].arguments.flatMap((arg, i) => arg === '-p' ? [calls[0].arguments[i + 1]] : []), ['vcp-lifecycle', 'vcp-cli']);
  const candidate = fs.readFileSync(path.join(repository, 'scripts/release/candidate.ps1'), 'utf8');
  const original = candidate.match(/^\s+Stage 'native-boundaries' (.+)$/m)[1];
  assert.deepEqual([...original.matchAll(/'--test','([^']+)'/g)].map(match => match[1]), selected(calls[0]).slice(1));
  assert.deepEqual(calls[0].arguments.slice(-3), ['--', '--nocapture', '--test-threads=1']);
});

test('actual reviewed-source gate refuses conflicting scopes and unreviewed or non-push selections before native work', { skip: process.platform !== 'win32' }, t => {
  const { result } = powershell(t, String.raw`
$env:GITHUB_API_URL='https://api.invalid';$env:GITHUB_REPOSITORY='fixture/repo';$env:GH_READ_TOKEN='synthetic-gate-token'
function Invoke-RestMethod([string]$Uri,[hashtable]$Headers){$script:requests++;return @{workflow_runs=$script:available}}
$rows=@()
for($i=0;$i -lt 8;$i++) {
  $case=Join-Path $Root ('case-'+$i);New-Item -ItemType Directory -Path $case|Out-Null
  Push-Location $case
  try {
    $env:GITHUB_REF='refs/heads/main';$env:GITHUB_SHA='a'*40;$env:REVIEWED_COMMIT='a'*40;$env:STORAGE_ONLY='false';$script:requests=0
    $run=@{head_sha='a'*40;head_branch='main';event='push';conclusion='success';id=123;html_url='https://example.invalid/run/123'}
    switch($i){1{$env:STORAGE_ONLY='true'}2{$env:REVIEWED_COMMIT='b'*40}3{$env:GITHUB_REF='refs/heads/topic'}4{$run.event='workflow_dispatch'}5{$run.conclusion='failure'}6{$run.head_sha='b'*40}7{$env:REVIEWED_COMMIT='invalid'}}
    $script:available=@($run);$accepted=$true
    try{. (Join-Path $Root 'gate.ps1')}catch{$accepted=$false}
    $present=Test-Path -LiteralPath 'artifacts/native-boundaries/delivery.json'
    if($accepted -ne ($i -eq 0) -or $present -ne ($i -eq 0)){throw ('Incorrect gate result '+$i)}
    $rows+=@{case=$i;accepted=$accepted;requests=$script:requests}
  } finally {Pop-Location}
}
$rows | ConvertTo-Json -Compress
`, { 'gate.ps1': gate });
  const rows = JSON.parse(result.stdout);
  assert.equal(rows.length, 8); assert.equal(rows[0].accepted, true);
  for (const i of [1, 2, 3, 7]) assert.equal(rows[i].requests, 0, 'invalid selection queried API');
  for (const i of [4, 5, 6]) assert.equal(rows[i].requests, 1, 'review evidence was not checked');
});

function packet(t) {
  const root = temporary(t), input = path.join(root, 'artifacts/native-boundaries'), output = path.join(root, 'artifacts/native-boundaries-public');
  fs.mkdirSync(input, { recursive: true }); fs.mkdirSync(path.join(root, 'scripts/release'), { recursive: true });
  fs.writeFileSync(path.join(root, 'scripts/release/evidence.cjs'), 'module.exports=require(' + JSON.stringify(path.join(repository, 'scripts/release/evidence.cjs')) + ');');
  const write = (name, value) => fs.writeFileSync(path.join(input, name), typeof value === 'string' ? value : JSON.stringify(value));
  const source = { schema: 'vcp-release-source/1', commit, dirty: false, files: [{ path: 'example', bytes: 7, sha256: digest('example') }], content_sha256: digest('example') };
  write('source-before.json', source); write('source-after.json', source); write('delivery.json', { reviewed_commit: commit }); write('provision.log', 'synthetic tool preparation\n');
  const record = { schema: 'vcp-native-boundary-preflight/1', status: 'pass', scope: 'source-only', reviewed_commit: commit, jobs: 16, source_stable: true, stages: [] };
  for (const id of ['native', 'package-install']) {
    const log = 'synthetic fixture API key: synthetic-private-value-0123456789\n' + (id === 'native' ? 'test result: ok. 1 passed; 0 failed\n'.repeat(5) : ''); write(id + '.log', log);
    write(id + '-progress.json', { status: 'pass' });
    write(id + '-supervision.json', { exit_code: 0, process_exit_code: 0, broker_exit_code: 0, forced_cleanup: false, job_active_processes_zero: true, completion: 'natural' });
    record.stages.push({ id, exit_code: 0, test_summaries: [1, 1, 1, 1, 1], log_sha256: digest(log) });
  }
  write('run.json', record); write('private-recovery-key', 'private file must never be collected');
  const run = (status = 'success') => spawnSync(process.execPath, ['-e', collector], { cwd: root, encoding: 'utf8', timeout: 3000, windowsHide: true,
    env: { ...process.env, REVIEWED_COMMIT: commit, PREFLIGHT_JOB_STATUS: status, FIXTURE_API_KEY: 'synthetic-private-value-0123456789' } });
  return { root, input, output, write, run, record };
}

test('actual collector retains only allowlisted sanitized evidence with raw and transformed hashes', t => {
  const f = packet(t), result = f.run(); assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  const value = JSON.parse(fs.readFileSync(path.join(f.output, 'evidence.json'))); assert.equal(value.status, 'pass');
  assert(!fs.existsSync(path.join(f.output, 'private-recovery-key')));
  for (const row of value.files) {
    const bytes = fs.readFileSync(path.join(f.output, row.path)); assert.equal(row.sha256, digest(bytes));
    assert.equal(row.raw_sha256, digest(fs.readFileSync(path.join(f.input, row.path))));
    assert(!bytes.includes('synthetic-private-value-0123456789'));
  }
  assert.match(fs.readFileSync(path.join(f.output, 'native.log'), 'utf8'), /\[REDACTED\]/);
});

test('actual collector fails missing, changed, incomplete or unsuccessful preflight evidence while retaining diagnosis', t => {
  const cases = [
    f => fs.unlinkSync(path.join(f.input, 'source-after.json')),
    f => f.write('source-after.json', { changed: true }),
    f => fs.appendFileSync(path.join(f.input, 'native.log'), 'changed after successful stage'),
    f => f.write('native-supervision.json', { exit_code: 0, process_exit_code: 0, broker_exit_code: 0, forced_cleanup: true, job_active_processes_zero: true, completion: 'natural' }),
    f => { f.record.stages[0].test_summaries[0] = 0; f.write('run.json', f.record); },
    f => { f.record.stages[0].test_summaries.pop(); f.write('run.json', f.record); },
    f => { const log = 'test result: ok. 1 passed; 0 failed\n'.repeat(4); f.write('native.log', log); f.record.stages[0].log_sha256 = digest(log); f.write('run.json', f.record); },
    f => { f.record.stages.reverse(); f.write('run.json', f.record); },
    f => fs.unlinkSync(path.join(f.input, 'native-supervision.json')),
    f => { f.record.reviewed_commit = 'b'.repeat(40); f.write('run.json', f.record); },
  ];
  for (const mutate of cases) {
    const f = packet(t); mutate(f); const result = f.run(); assert.ifError(result.error); assert.notEqual(result.status, 0, mutate.toString());
    const value = JSON.parse(fs.readFileSync(path.join(f.output, 'evidence.json')));
    assert.equal(value.status, 'fail'); assert(value.validation_failures.length > 0);
    assert(fs.existsSync(path.join(f.output, 'SHA256SUMS')));
  }
  const f = packet(t); assert.notEqual(f.run('failure').status, 0);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.output, 'evidence.json'))).status, 'fail');
});
