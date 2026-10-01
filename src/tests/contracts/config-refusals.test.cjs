// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const script = path.resolve(__dirname, '../../../scripts/release/config-refusals.ps1');
const windows = { skip: process.platform !== 'win32' };
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-config-refusals-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (relative, value) => { const file = path.join(root, relative); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, value); return file; };
  const invoke = args => {
    const env = { ...process.env };
    for (const name of ['OneDrive', 'OneDriveConsumer', 'OneDriveCommercial', 'VCP_TEST_SYNC_ROOT']) delete env[name];
    const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', script, ...args],
      { env, windowsHide: true, encoding: 'utf8', timeout: 15000 });
    assert.ifError(result.error); return result;
  };
  const runner = write('functions.ps1', `param([string]$Script,[string]$InputFile)
$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Script,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Runner parse failed'}
$names=@('Get-ConfigRefusalHash','Assert-ConfigRefusalPath','Test-ConfigRefusalOverlap','Assert-ConfigRefusalPrivateRoot',
    'Get-ConfigRefusalSyncDeclarations','Resolve-ConfigRefusalSyncExclusions','Invoke-ConfigRefusalEnvironment','Get-ConfigRefusalTree','Assert-ConfigRefusalAggregate')
$functions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names},$true))
if($functions.Count -ne $names.Count){throw 'Expected focused runner functions'}
foreach($function in $functions){. ([scriptblock]::Create($function.Extent.Text))}
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
if($settings.action -ceq 'paths') {
    $results=@(foreach($row in $settings.rows){
        try {$null=Assert-ConfigRefusalPath $row.path $row.kind;@{accepted=$true}}
        catch {@{accepted=$false;reason=$_.Exception.Message}}
    })
    ConvertTo-Json -InputObject $results -Depth 10 -Compress
} elseif($settings.action -ceq 'aggregates') {
    $results=@(foreach($row in $settings.rows){
        try {Assert-ConfigRefusalAggregate $row ('a'*64) ('b'*64) ('c'*64) ('d'*64);@{accepted=$true}}
        catch {@{accepted=$false;reason=$_.Exception.Message}}
    })
    ConvertTo-Json -InputObject $results -Depth 10 -Compress
} elseif($settings.action -ceq 'sync') {
    [Environment]::SetEnvironmentVariable('NODE_OPTIONS','--this-inherited-option-must-not-reach-node','Process')
    $results=@(foreach($row in $settings.rows){
        try {
            $root=Assert-ConfigRefusalPrivateRoot $row.output @($row.declarations)
            $declarations=@(Get-ConfigRefusalSyncDeclarations @($row.declarations))
            $resolved=@(Resolve-ConfigRefusalSyncExclusions $root $declarations $settings.node)
            @{accepted=$true;resolved=$resolved}
        } catch {@{accepted=$false;reason=$_.Exception.Message}}
    })
    ConvertTo-Json -InputObject $results -Depth 10 -Compress
} elseif($settings.action -ceq 'tree') {
    @(Get-ConfigRefusalTree $settings.root) | ConvertTo-Json -Depth 10 -Compress
} elseif($settings.action -ceq 'environment') {
    [Environment]::SetEnvironmentVariable('VCP_CONTRACT_SECRET','synthetic-private-value','Process')
    [Environment]::SetEnvironmentVariable('NODE_OPTIONS','synthetic-not-a-node-option','Process')
    [Environment]::SetEnvironmentVariable('VCP_TEST_PWSH','unselected-inherited-runtime','Process')
    $before=[Environment]::GetEnvironmentVariables('Process')
    $selected=@{TEMP=$settings.root;TMP=$settings.root;VCP_TEST_NODE='selected-node';VCP_TEST_PWSH='selected-powershell';VCP_BETA_IMPORT_OUTPUT='selected-private-output'}
    $observed=Invoke-ConfigRefusalEnvironment $selected {
        @{secret_absent=($null -eq [Environment]::GetEnvironmentVariable('VCP_CONTRACT_SECRET'));
          node_options_absent=($null -eq [Environment]::GetEnvironmentVariable('NODE_OPTIONS'));
          selected=($env:VCP_TEST_NODE -ceq 'selected-node' -and $env:VCP_TEST_PWSH -ceq 'selected-powershell' -and $env:VCP_BETA_IMPORT_OUTPUT -ceq 'selected-private-output');
          private_temporary=($env:TEMP -ceq $settings.root -and $env:TMP -ceq $settings.root);
          safe_path=($env:PATH -ceq ($env:SystemRoot+'\\System32;'+$env:SystemRoot))}
    }
    try {Invoke-ConfigRefusalEnvironment $selected { $env:UNEXPECTED_CONTRACT_CHILD_VALUE='temporary';throw 'synthetic failure' }} catch {}
    $after=[Environment]::GetEnvironmentVariables('Process')
    $restored=$before.Count -eq $after.Count
    foreach($name in $before.Keys){if($before[$name] -cne $after[$name]){$restored=$false}}
    @{observed=$observed;restored=$restored} | ConvertTo-Json -Compress
} else {throw 'Unknown contract action'}
`);
  const functions = input => {
    const env = { ...process.env };
    for (const name of ['OneDrive', 'OneDriveConsumer', 'OneDriveCommercial', 'VCP_TEST_SYNC_ROOT']) delete env[name];
    const inputFile = write('input.json', JSON.stringify(input));
    const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Script', script, '-InputFile', inputFile],
      { env, windowsHide: true, encoding: 'utf8', timeout: 15000 });
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr); return JSON.parse(result.stdout);
  };
  const unused = ['-NativeResult', 'unavailable', '-InstalledEngine', 'unavailable', '-Node', 'unavailable',
    '-QualificationExecutable', 'unavailable', '-QualificationSha256', '0'.repeat(64)];
  return { root, write, invoke, functions, unused };
}

test('private output refuses checkout, sync overlap, existing and redirected roots before any candidate launch', windows, t => {
  const f = fixture(t), checkout = path.join(f.root, 'checkout'), sync = path.join(f.root, 'sync');
  fs.mkdirSync(path.join(checkout, '.git'), { recursive: true }); fs.mkdirSync(sync);
  const outside = path.join(f.root, 'ordinary'); fs.mkdirSync(outside);
  const alias = path.join(f.root, 'alias'); fs.symlinkSync(outside, alias, 'junction');
  const rows = [
    [path.join(checkout, 'fresh'), [], /outside repository trees/],
    [path.join(sync, 'fresh'), ['-SyncRoots', sync], /overlaps a known or declared sync root/],
    [path.join(f.root, 'fresh-parent'), ['-SyncRoots', path.join(f.root, 'fresh-parent', 'nested-sync')], /overlaps a known or declared sync root/],
    [path.join(f.root, 'fresh'), ['-SyncRoots', 'relative-sync'], /Absolute ordinary local-drive/],
    [path.join(alias, 'fresh'), [], /Redirected qualification path/],
    [outside, [], /Fresh private output/],
    ['relative-output', [], /Absolute ordinary local-drive/],
  ];
  for (const [output, extra, reason] of rows) {
    const existed = fs.existsSync(output), result = f.invoke([...f.unused, '-OutputRoot', output, ...extra]);
    assert.notEqual(result.status, 0); assert.match(result.stderr, reason);
    assert.equal(fs.existsSync(output), existed, 'Rejected preflight must not create output');
  }
});

test('unbound or redirected executable inputs refuse before creating a private run', windows, t => {
  const f = fixture(t), dummy = f.write('never-launch.exe', 'nonexecutable input sentinel'), output = path.join(f.root, 'fresh');
  const args = ['-NativeResult', dummy, '-InstalledEngine', dummy, '-Node', dummy, '-QualificationExecutable', dummy,
    '-QualificationSha256', '0'.repeat(64), '-OutputRoot', output];
  const result = f.invoke(args); assert.notEqual(result.status, 0); assert.match(result.stderr, /qualification executable hash differs/);
  assert.equal(fs.existsSync(output), false); assert.equal(fs.readFileSync(dummy, 'utf8'), 'nonexecutable input sentinel');
  const directory = path.join(f.root, 'tools'); fs.mkdirSync(directory); fs.copyFileSync(dummy, path.join(directory, 'tool.exe'));
  const alias = path.join(f.root, 'tools-alias'); fs.symlinkSync(directory, alias, 'junction');
  args[7] = path.join(alias, 'tool.exe'); args[9] = hash(fs.readFileSync(dummy));
  const redirected = f.invoke(args); assert.notEqual(redirected.status, 0); assert.match(redirected.stderr, /Redirected qualification path/);
  assert.equal(fs.existsSync(output), false);
});

test('ordinary paths reject traversal, device spellings, streams and unsafe components', windows, t => {
  const f = fixture(t), file = f.write('ordinary.txt', 'unchanged');
  const accepted = [{ path: file, kind: 'file' }, { path: f.root, kind: 'directory' }, { path: path.join(f.root, 'new'), kind: 'new' }];
  const rejected = [
    { path: file, kind: 'directory' }, { path: f.root, kind: 'file' }, { path: path.join(f.root, 'missing'), kind: 'file' },
    ...['..\\escape', 'bad.\\child', 'NUL.txt', 'file:stream', 'bad?name', 'double\\\\part'].map(relative => ({ path: f.root + '\\' + relative, kind: 'new' })),
    { path: '\\\\?\\' + file, kind: 'file' }, { path: '\\\\server\\share\\file', kind: 'new' },
  ];
  const rows = f.functions({ action: 'paths', rows: [...accepted, ...rejected] });
  assert(rows.slice(0, accepted.length).every(row => row.accepted));
  assert(rows.slice(accepted.length).every(row => !row.accepted)); assert.equal(fs.readFileSync(file, 'utf8'), 'unchanged');
});

test('test environment excludes inherited secrets and injection variables and restores every value on failure', windows, t => {
  const f = fixture(t), report = f.functions({ action: 'environment', root: f.root });
  assert.deepEqual(report.observed, { secret_absent: true, node_options_absent: true, selected: true, private_temporary: true, safe_path: true });
  assert.equal(report.restored, true);
});

test('sync exclusions cover declared aliases and physical trees including missing suffixes, and refuse dangling aliases', windows, t => {
  const f = fixture(t), physical = path.join(f.root, 'physical-sync'), alias = path.join(f.root, 'declared-sync');
  fs.mkdirSync(physical); fs.symlinkSync(physical, alias, 'junction');
  const dangling = path.join(f.root, 'dangling-sync'); fs.symlinkSync(path.join(f.root, 'absent-target'), dangling, 'junction');
  const rows = [
    { output: path.join(f.root, 'safe-output'), declarations: [alias] },
    { output: path.join(alias, 'private-output'), declarations: [alias] },
    { output: path.join(physical, 'private-output'), declarations: [alias] },
    { output: path.join(f.root, 'safe-missing'), declarations: [path.join(alias, 'future', 'nested-sync')] },
    { output: path.join(physical, 'future', 'nested-sync', 'private-output'), declarations: [path.join(alias, 'future', 'nested-sync')] },
    { output: path.join(f.root, 'safe-dangling'), declarations: [dangling] },
    { output: path.join(f.root, 'safe-dangling-child'), declarations: [path.join(dangling, 'future')] },
  ];
  const results = f.functions({ action: 'sync', node: fs.realpathSync.native(process.execPath), rows });
  assert.deepEqual(results.map(row => row.accepted), [true, false, false, true, false, false, false]);
  assert.deepEqual(results[0].resolved, [{ declared: alias, physical }]);
  assert.deepEqual(results[3].resolved, [{ declared: path.join(alias, 'future', 'nested-sync'), physical: path.join(physical, 'future', 'nested-sync') }]);
  for (const row of rows) assert.equal(fs.existsSync(row.output), false, 'Resolver must not create private directories');
});

test('exact ordinary verifier dependency inventory detects changed, added and redirected entries', windows, t => {
  const f = fixture(t), tree = path.join(f.root, 'toml-fixture');
  const code = f.write('toml-fixture/lib/parser.js', 'original parser'); f.write('toml-fixture/package.json', '{"main":"lib/parser.js"}');
  const before = f.functions({ action: 'tree', root: tree });
  assert.deepEqual(before.map(row => row.path), ['lib', 'lib/parser.js', 'package.json']);
  assert.equal(before.find(row => row.path === 'lib/parser.js').sha256, hash('original parser'));
  fs.writeFileSync(code, 'changed parser');
  assert.notDeepEqual(f.functions({ action: 'tree', root: tree }), before);
  f.write('toml-fixture/extra.js', 'new dependency entry');
  assert.equal(f.functions({ action: 'tree', root: tree }).length, 4);
  const outside = path.join(f.root, 'outside'); fs.mkdirSync(outside);
  fs.symlinkSync(outside, path.join(tree, 'redirected'), 'junction');
  assert.throws(() => f.functions({ action: 'tree', root: tree }), /Redirected qualification path/);
});

function aggregate() {
  const controls = [], cases = [];
  for (const backend of ['files', 'sqlite']) {
    for (const mode of ['cli', 'start', 'resume']) controls.push({ backend, mode, status: 'pass', budget_exhausted: true,
      provider_attempts: 0, reservations: 0, send_intents: 0, mcp_dispatched: false });
    for (const mode of ['start', 'resume']) for (const change of ['base', 'revision', 'content', 'corrupt', 'removed', 'first-import']) {
      const cli = ['base', 'corrupt'].includes(change);
      cases.push({ backend, mode, change, status: 'pass', code: 'POLICY_DENIED', immutable_records_preserved: true,
        provider_attempts: 0, reservations: 0, send_intents: 0, mcp_dispatched: false, cli_exit_code: cli ? 2 : null, reconnect_rejected: cli ? true : null });
    }
  }
  return { schema: 'vcp-final-import-refusals/1', status: 'pass', engine_sha256: 'a'.repeat(64),
    qualification_executable_sha256: 'b'.repeat(64), powershell_sha256: 'c'.repeat(64), node_sha256: 'd'.repeat(64), controls, cases };
}
test('aggregate requires the exact bound six controls and 24 distinct refused cases with preserved accounting', windows, t => {
  const f = fixture(t), mutations = [
    row => { row.engine_sha256 = 'c'.repeat(64); }, row => { row.qualification_executable_sha256 = 'c'.repeat(64); },
    row => { delete row.powershell_sha256; }, row => { row.powershell_sha256 = 'd'.repeat(64); },
    row => { delete row.node_sha256; }, row => { row.node_sha256 = 'e'.repeat(64); },
    row => { row.status = 'fail'; }, row => { row.controls.pop(); }, row => { row.cases.pop(); },
    row => { row.controls[1] = row.controls[0]; }, row => { row.cases[1] = row.cases[0]; },
    row => { row.controls[0].budget_exhausted = false; }, row => { row.controls[0].mode = 'unknown'; },
    row => { row.cases[0].backend = 'other'; }, row => { row.cases[0].change = 'unknown'; },
    row => { row.cases[0].provider_attempts = 1; }, row => { row.cases[0].reservations = '0'; },
    row => { row.controls[0].send_intents = 1; }, row => { row.cases[0].mcp_dispatched = true; },
    row => { row.cases[0].immutable_records_preserved = false; }, row => { row.cases[0].code = 'VERSION_CONFLICT'; },
    row => { row.cases[0].cli_exit_code = 0; }, row => { row.cases[0].reconnect_rejected = false; },
    row => { row.cases[1].reconnect_rejected = true; },
  ];
  const rows = [aggregate(), ...mutations.map(mutate => { const row = aggregate(); mutate(row); return row; })];
  const results = f.functions({ action: 'aggregates', rows }); assert.equal(results[0].accepted, true);
  for (let index = 1; index < results.length; index++) assert.equal(results[index].accepted, false, mutations[index - 1].toString());
});
