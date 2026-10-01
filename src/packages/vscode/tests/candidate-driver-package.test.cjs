// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');
const { inventory, sha256 } = require('../scripts/inventory.cjs');

test('candidate drivers package only their declared runtime files while capture files are locked', { skip: process.platform !== 'win32' }, async t => {
  const repo = path.resolve(__dirname, '../../../..');
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-driver-package-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const runner = path.join(root, 'package-driver.ps1');
  fs.writeFileSync(runner, String.raw`param([string]$Repo,[string]$Node,[string]$Source,[string]$Name,[string]$Directory,[switch]$ReproduceUnlisted)
$ErrorActionPreference='Stop'
. (Join-Path $Repo 'scripts/release/candidate-runtime.ps1')
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Invalid source runner'}
$manifests=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.HashtableAst] -and $item.Extent.Text.Contains("name='$Name'")},$true))
$commands=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.CommandAst] -and $item.GetCommandName() -ceq 'Invoke-BetaProcess' -and $item.Extent.Text.Contains('vsce/vsce')},$true))
if($manifests.Count -ne 1 -or $commands.Count -ne 1){throw 'Expected one driver manifest and packaging command'}
if($commands[0].Extent.Text -match 'allow-package.*secrets'){throw 'Driver packaging must retain secret scanning'}
$editor=@{version='1.138.0'}
$manifest=& ([scriptblock]::Create($manifests[0].Extent.Text))
if($ReproduceUnlisted){$manifest.Remove('files')}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $Directory 'package.json') -Encoding utf8NoBOM
$driver=$Directory;$driverArchive=Join-Path (Split-Path -Parent $Directory) 'driver.vsix'
# Execute the runner's actual VSCE command through its real capture wrapper.
# Invoke-BetaProcess holds process-*.stdout/stderr exclusively during VSCE.
$result=& ([scriptblock]::Create($commands[0].Extent.Text))
if($result.exit_code -ne 0 -or -not (Test-Path -LiteralPath $driverArchive)){throw 'Driver was not packaged'}
`);
  const cases = [
    ['editor-smoke.ps1', 'candidate-observer-driver', 'candidate-editor-driver.cjs', false],
    ['editor-lifecycle.ps1', 'candidate-lifecycle-driver', 'editor-lifecycle-driver.cjs', true],
    ['editor-refusals.ps1', 'candidate-refusal-driver', 'editor-refusals-driver.cjs', false],
  ];
  for (const [source, name, driverSource, host] of cases) {
    const directory = path.join(root, name, 'driver');
    fs.mkdirSync(directory, { recursive: true });
    fs.copyFileSync(path.join(repo, 'scripts/release', driverSource), path.join(directory, 'driver.cjs'));
    if (host) fs.copyFileSync(path.join(__dirname, 'package-host.cjs'), path.join(directory, 'package-host.cjs'));
    const sentinel = path.join(directory, 'private-observation.json');
    fs.writeFileSync(sentinel, '{"private":"fixture retained outside the package"}\n');
    const before = fs.readFileSync(sentinel);
    const args = ['-NoProfile', '-File', runner, '-Repo', repo, '-Node', fs.realpathSync.native(process.execPath),
      '-Source', path.join(repo, 'scripts/release', source), '-Name', name, '-Directory', directory];
    if (source === 'editor-smoke.ps1') {
      assert.throws(() => execFileSync('pwsh', [...args, '-ReproduceUnlisted'], { windowsHide: true, timeout: 30000, stdio: 'pipe' }), error => {
        assert.match(String(error.stderr), /EBUSY/);
        assert.match(String(error.stderr), /process-[a-f0-9-]+\.(?:stdout|stderr)/);
        return true;
      }, 'the old unlisted manifest must reproduce the real locked-capture failure');
    }
    execFileSync('pwsh', args, { windowsHide: true, timeout: 30000, stdio: 'pipe' });
    const files = await inventory(path.join(directory, '../driver.vsix'));
    const shipped = files.filter(row => row.path.startsWith('extension/'));
    const expected = ['driver.cjs', 'package.json', ...(host ? ['package-host.cjs'] : [])].sort();
    assert.deepEqual(shipped.map(row => row.path.slice('extension/'.length)).sort(), expected);
    for (const row of shipped) assert.equal(row.sha256, sha256(fs.readFileSync(path.join(directory, row.path.slice('extension/'.length)))));
    assert.ok(fs.readdirSync(directory).some(name => /^process-.*\.stderr$/.test(name)), 'actual capture files must exist beside the packaged driver');
    assert.deepEqual(fs.readFileSync(sentinel), before, 'excluded private evidence must be preserved');
  }
});
