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
