// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {spawnSync} = require('node:child_process');
test('candidate tool lookup selects one PATH winner when the host provides multiple installations', {skip: process.platform !== 'win32'}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-tools-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const directories = ['pinned tools', 'image tools'].map(name => path.join(root, name));
  for (const directory of directories) {
    fs.mkdirSync(directory);
    // Get-Command resolves application filenames without executing them.
    for (const name of ['node', 'pwsh', 'git']) fs.writeFileSync(path.join(directory, name + '.exe'), 'lookup fixture');
  }
  const expressions = [];
  for (const file of ['candidate.ps1', 'candidate-smoke.ps1', 'editor-smoke.ps1', 'editor-lifecycle.ps1']) {
    const source = fs.readFileSync(path.resolve(__dirname, '../../../scripts/release', file), 'utf8');
    const matches = [...source.matchAll(/\(Get-Command (node|pwsh|git) -CommandType Application[^)]*\)\.Source/g)];
    assert(matches.length > 0, `No tool discovery found in ${file}`);
    for (const match of matches) expressions.push({file, name: match[1], expression: match[0]});
  }
  const input = path.join(root, 'input.json');
  fs.writeFileSync(input, JSON.stringify({directories, expressions}));
  const runner = path.join(root, 'exercise.ps1');
  fs.writeFileSync(runner, `param([string]$InputFile)
$ErrorActionPreference='Stop'
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$env:PATH=$settings.directories -join ';'
foreach ($row in $settings.expressions) {
    if (@(Get-Command $row.name -CommandType Application -All).Count -ne 2) { throw 'Two distinct application paths required' }
    $selected=& ([scriptblock]::Create($row.expression))
    if ($selected -isnot [string] -or $selected -cne (Join-Path $settings.directories[0] ($row.name+'.exe'))) { throw ('Not one PATH winner in '+$row.file) }
}
@{lookups=$settings.expressions.Count;status='pass'} | ConvertTo-Json -Compress
`);
  const result = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-InputFile', input], {encoding: 'utf8', windowsHide: true, timeout: 15000});
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).lookups, expressions.length);
  assert.equal(JSON.parse(result.stdout).status, 'pass');
});
test('missing candidate Node leaves a failed source stage and retained diagnostic', {skip: process.platform !== 'win32'}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-missing-tool-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const output = path.join(root, 'candidate'), runner = path.join(root, 'exercise.ps1');
  fs.writeFileSync(runner, `param([string]$Candidate,[string]$Output)
$env:PATH=$PSHOME+';'+$env:SystemRoot+'\\System32'
if (Get-Command node -CommandType Application -ErrorAction SilentlyContinue) { throw 'Fixture requires no Node application' }
& $Candidate -ReviewedCommit ('0'*40) -OutputRoot $Output
`);
  const result = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Candidate', path.resolve(__dirname, '../../../scripts/release/candidate.ps1'), '-Output', output],
    {encoding: 'utf8', windowsHide: true, timeout: 15000});
  assert.ifError(result.error); assert.notEqual(result.status, 0);
  const run = JSON.parse(fs.readFileSync(path.join(output, 'run.json'), 'utf8'));
  assert.equal(run.status, 'fail'); assert.equal(run.environment.node, 'not observed');
  assert.equal(run.stages.length, 1); assert.equal(run.stages[0].id, 'source-gate'); assert.equal(run.stages[0].status, 'fail');
  assert.match(run.failure, /node.*not recognized/i);
  assert.match(fs.readFileSync(run.stages[0].log, 'utf8'), /node.*not recognized/i);
});

test('candidate source gate selects physical temporary paths before child fixtures and private qualification', {skip: process.platform !== 'win32'}, t => {
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-temp-')));
  const target = path.join(root, 'physical temp'), alias = path.join(root, 'redirected temp');
  fs.mkdirSync(target); fs.symlinkSync(target, alias, 'junction');
  t.after(() => { fs.unlinkSync(alias); fs.rmSync(root, {recursive: true, force: true}); });
  const runner = path.join(root, 'exercise.ps1'), input = path.join(root, 'input.json');
  fs.writeFileSync(runner, `param([string]$Script,[string]$InputFile)
$ErrorActionPreference='Stop'
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Script,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Candidate script parse failed'}
$names=@('Assert-CandidateOrdinaryPath','Set-CandidateTemporaryDirectory')
$functions=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -in $names},$true))
if($functions.Count -ne 2){throw 'Expected candidate temporary-directory functions'}
foreach($function in $functions){. ([scriptblock]::Create($function.Extent.Text))}
$gate=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.CommandAst] -and $item.GetCommandName() -ceq 'Stage' -and $item.CommandElements[1].Value -ceq 'source-gate'},$true))
if($gate.Count -ne 1){throw 'Expected one actual source gate'}
# Execute the actual source-gate body with the current test Node selected. Its
# final source-inventory check is outside this environment-selection contract.
$tools=@{node=$settings.node_version}; $run=@{environment=@{}}
$repository=$settings.repository; $ReviewedCommit='0'*40
$script:sourceChecked=$false
function Checked([string]$File,[string[]]$Arguments) {
    if($Arguments[1] -cne 'source'){throw 'Unexpected command after tool selection'}
    $script:sourceChecked=$true
}
$env:PATH=$settings.node_directory+';'+$env:PATH
$env:TEMP=$settings.temporary; $env:TMP=$settings.temporary
$failure=$null; $observed=$null
try {
    & $gate[0].CommandElements[-1].ScriptBlock.GetScriptBlock()
    $observed=& $node '-e' "const fs=require('node:fs'),os=require('node:os'),path=require('node:path');const root=fs.mkdtempSync(path.join(os.tmpdir(),'child-'));console.log(JSON.stringify({temporary:os.tmpdir(),root,physical:fs.realpathSync.native(root)}))"
    if($LASTEXITCODE -ne 0){throw 'Child temporary-directory observation failed'}
    $observed=$observed | ConvertFrom-Json
    $null=Assert-CandidateOrdinaryPath $observed.root $true
} catch { $failure=$_.Exception.Message }
@{environment=$run.environment;temp=$env:TEMP;tmp=$env:TMP;child=$observed;failure=$failure;source_checked=$sourceChecked} | ConvertTo-Json -Depth 10
exit 0
`);
  const physicalNode = fs.realpathSync.native(process.execPath);
  const execute = temporary => {
    fs.writeFileSync(input, JSON.stringify({temporary, node_version: process.versions.node,
      node_directory: path.dirname(physicalNode), repository: path.resolve(__dirname, '../../..')}));
    const result = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Script', path.resolve(__dirname, '../../../scripts/release/candidate.ps1'), '-InputFile', input],
      {encoding: 'utf8', windowsHide: true, timeout: 15000});
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };
  const selected = execute(alias), evidence = selected.environment.temporary_directory;
  assert.equal(selected.failure, null); assert.equal(selected.source_checked, true);
  assert.equal(evidence.inherited_temp, alias); assert.equal(evidence.inherited_tmp, alias);
  assert.equal(evidence.requested, alias); assert.equal(evidence.selected, target);
  assert.equal(selected.temp, target); assert.equal(selected.tmp, target);
  assert.equal(selected.child.temporary, target); assert.equal(selected.child.root, selected.child.physical);
  assert.equal(path.dirname(selected.environment.qualification_root), target);
  assert.match(path.basename(selected.environment.qualification_root), /^vcp-beta-private-[a-f0-9-]{36}$/);
  assert.equal(fs.existsSync(selected.environment.qualification_root), false, 'selection does not create qualification/install state');
  const file = path.join(root, 'not a directory'); fs.writeFileSync(file, 'preserve sentinel');
  for (const temporary of [file, path.join(root, 'missing')]) {
    const refused = execute(temporary);
    assert.equal(typeof refused.failure, 'string'); assert.equal(refused.source_checked, false);
    assert.equal(refused.environment.temporary_directory.selected, 'not observed');
    assert.equal(refused.environment.qualification_root, undefined);
    assert.equal(refused.temp, temporary); assert.equal(refused.tmp, temporary);
  }
  assert.equal(fs.readFileSync(file, 'utf8'), 'preserve sentinel');
});
