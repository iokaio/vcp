// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const {observation} = require('../../../scripts/release/editor-refusals.cjs');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const {execFileSync} = require('node:child_process');

test('actual refusal payload selector resolves the current extension identity alongside earlier installations', {skip: process.platform !== 'win32'}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-refusal-publisher-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const manifest = require('../../packages/vscode/package.json');
  const extensions = path.join(root, 'extensions');
  for (const [publisher, name] of [[manifest.publisher, manifest.name], ['vcp', 'vcp-local'], ['iokaio', 'vcp-local']]) {
    const directory = path.join(extensions, `${publisher}.${name}`);
    fs.mkdirSync(directory, {recursive: true});
    fs.writeFileSync(path.join(directory, 'package.json'), JSON.stringify({publisher, name, version: manifest.version}));
  }
  const runner = path.join(root, 'selector.ps1');
  fs.writeFileSync(runner, String.raw`param([string]$Source,[string]$Extensions,[string]$Version,[string]$Expected)
$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Refusal runner parse failed'}
$selector=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -ceq 'Installed-Extension'},$true))
$payload=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -ceq 'Verify-Payload'},$true))
if($selector.Count -ne 1 -or $payload.Count -ne 1){throw 'Expected exact runner functions'}
$calls=@($payload[0].FindAll({param($item) $item -is [Management.Automation.Language.CommandAst] -and $item.GetCommandName() -ceq 'Installed-Extension' -and $item.Extent.Text.Contains('$vsix.extension.version')},$true))
if($calls.Count -ne 1){throw 'Expected exact payload selector call'}
. ([scriptblock]::Create($selector[0].Extent.Text))
$vsix=@{extension=@{version=$Version}}
$actual=& ([scriptblock]::Create($calls[0].Extent.Text))
if($actual -cne $Expected){throw 'Payload selector chose the wrong extension identity'}
`);
  execFileSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner,
    '-Source', path.resolve(__dirname, '../../../scripts/release/editor-refusals.ps1'),
    '-Extensions', extensions, '-Version', manifest.version, '-Expected', path.join(extensions, `${manifest.publisher}.${manifest.name}`)],
  {windowsHide: true, timeout: 15000, stdio: 'pipe'});
});

const input = mode => ({mode, version: '0.2.1', scope: {workspace: 'w', session: 's'}, task: 'paused', sourceText: 'original human work\n', driverSha256: 'a'.repeat(64)});
function result(mode) {
  const selected = input(mode), source = selected.sourceText;
  return {ok: true, ...selected, extensionHostPid: 200, actualTrusted: mode === 'trusted', developmentPathAbsent: true,
    finalObserver: true, ordinaryAttachment: true, forbiddenRpcCount: 0,
    contextAttempts: mode === 'trusted' ? 2 : 0, contextRefused: mode === 'trusted' ? 2 : 0, contextAccepted: 0,
    observations: mode === 'restricted' ? {
      restricted: {actualTrusted: false, grantRefused: true, editsRefused: 3, workspaceRevision: '0'},
      uninitialized: {phase: 'unavailable', connected: false}, wrongData: {phase: 'unavailable', connected: false},
      unselected: {phase: 'disconnected', generationUnchanged: true},
    } : {
      trust: {granted: true, returnedToObserver: true, explicitControllerReacquired: true},
      outsideSource: {refused: true, diskSha256: hash(source)},
      drafts: ['typing', 'undo', 'reopen'].map(action => ({action, capturedDirty: action !== 'reopen', dirtyAfter: action !== 'reopen',
        reopened: action === 'reopen', capturedVersion: 2, afterVersion: action === 'reopen' ? 1 : 4,
        capturedSha256: hash((action === 'reopen' ? '' : 'human ') + source),
        afterSha256: hash((action === 'typing' ? 'later human ' : action === 'undo' ? 'human ' : '') + source),
        diskBeforeSha256: hash(source), diskAfterSha256: hash(source), reviewRefused: true, bufferPreserved: true})),
    }};
}
test('installed refusal observations bind actual trust, paused identity, driver bytes and no mutation attempts', () => {
  for (const mode of ['restricted', 'trusted']) {
    assert.equal(observation(input(mode), result(mode)).status, 'pass');
    for (const change of [r => {r.ok = false;}, r => {r.version = 'wrong';}, r => {r.task = 'other';},
      r => {r.scope.workspace = 'other';}, r => {r.driverSha256 = 'b'.repeat(64);}, r => {r.actualTrusted = !r.actualTrusted;},
      r => {r.developmentPathAbsent = false;}, r => {r.finalObserver = false;}, r => {r.ordinaryAttachment = false;},
      r => {r.forbiddenRpcCount = 1;}, r => {r.contextAccepted = 1;}, r => {r.contextRefused += 1;},
      r => {r.contextAttempts = -1;}, r => {delete r.extensionHostPid;}]) {
      const candidate = result(mode); change(candidate); assert.throws(() => observation(input(mode), candidate));
    }
  }
});
test('restricted phase requires both native discovery refusals and editor trust/edit refusals', () => {
  for (const change of [r => {r.observations.restricted.grantRefused = false;}, r => {r.observations.restricted.editsRefused = 2;},
    r => {r.observations.restricted.workspaceRevision = '1';}, r => {r.observations.wrongData.connected = true;},
    r => {r.observations.uninitialized.phase = 'connected';}, r => {r.observations.unselected.generationUnchanged = false;}]) {
    const candidate = result('restricted'); change(candidate); assert.throws(() => observation(input('restricted'), candidate));
  }
});
test('dirty draft refusal requires real typing and undo versions, reopen identity and unchanged disk', () => {
  for (const change of [r => {r.observations.drafts.pop();}, r => {r.observations.trust.returnedToObserver = false;},
    r => {r.observations.outsideSource.refused = false;}, r => {r.observations.drafts[0].capturedDirty = false;},
    r => {r.observations.drafts[0].afterSha256 = r.observations.drafts[0].capturedSha256;},
    r => {r.observations.drafts[1].afterVersion = r.observations.drafts[1].capturedVersion;},
    r => {r.observations.drafts[1].afterSha256 = hash(input('trusted').sourceText);},
    r => {r.observations.drafts[2].reopened = false;}, r => {r.observations.drafts[0].diskAfterSha256 = '0'.repeat(64);},
    r => {r.observations.drafts[1].reviewRefused = false;}, r => {r.observations.drafts[2].bufferPreserved = false;}]) {
    const candidate = result('trusted'); change(candidate); assert.throws(() => observation(input('trusted'), candidate));
  }
});
