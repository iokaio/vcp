// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {observation, inventory, verifiedInventory} = require('../../../scripts/release/editor-lifecycle.cjs');
const scope = {workspace: 'workspace', session: 'session'};
const input = mode => ({mode, version: '0.2.1', scope, task: 'paused-task'});
const result = mode => ({ok: true, mode, version: '0.2.1', scope, task: 'paused-task',
  observer: mode !== 'missing', developmentPathAbsent: true, extensionHostPid: 200,
  reloaded: true, beforeReloadHostPid: 100, missingEngine: mode === 'missing',
  unsupportedNegotiation: {code: -32602, data: {kind: 'unsupported_version'}}});
test('actual lifecycle observations require reload, native refusal and original observer identity', () => {
  const report = observation(input('install'), result('install'), null);
  assert.deepEqual(report.reload, {before_host_pid: 100, after_host_pid: 200, observer: true});
  assert.equal(report.incompatible_protocol.requested, '99.0');
  for (const change of [r => {r.observer = false;}, r => {r.scope = {workspace: 'other', session: 'session'};},
    r => {r.task = 'other';}, r => {r.version = '0.2.2';}, r => {r.reloaded = false;},
    r => {r.beforeReloadHostPid = 200;}, r => {r.unsupportedNegotiation.data.kind = 'permission_denied';},
    r => {r.developmentPathAbsent = false;}, r => {delete r.extensionHostPid;}, r => {r.ok = false;}]) {
    const candidate = result('install'); change(candidate);
    assert.throws(() => observation(input('install'), candidate, null));
  }
});
test('ordered restarts use fresh actual hosts and never claim a missing engine connected', () => {
  let previous = observation(input('install'), result('install'), null);
  for (const mode of ['restart', 'failed-update', 'missing', 'reconnect']) {
    const current = result(mode); current.extensionHostPid = previous.extension_host_pid + 1;
    assert.throws(() => observation(input(mode), {...current, extensionHostPid: previous.extension_host_pid}, previous));
    const report = observation(input(mode), current, previous);
    assert.equal(report.observer, mode !== 'missing');
    previous = report;
  }
  assert.throws(() => observation(input('restart'), result('restart'), null));
  assert.throws(() => observation(input('reconnect'), result('reconnect'), {status: 'pass', mode: 'install', extension_version: '0.2.1'}));
  assert.throws(() => observation(input('missing'), {...result('missing'), missingEngine: false}, {status: 'pass', mode: 'failed-update', extension_version: '0.2.1', extension_host_pid: 100}));
});
test('installed extension inventory detects additions, modifications and removed original files', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-editor-inventory-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  fs.mkdirSync(path.join(root, 'dist'));
  fs.writeFileSync(path.join(root, 'package.json'), '{"version":"0.2.1"}');
  fs.writeFileSync(path.join(root, 'dist/extension.js'), 'original');
  const before = inventory(root);
  fs.writeFileSync(path.join(root, 'dist/extension.js'), 'changed');
  assert.notEqual(inventory(root).sha256, before.sha256);
  fs.writeFileSync(path.join(root, 'dist/extension.js'), 'original');
  assert.deepEqual(inventory(root), before);
  fs.writeFileSync(path.join(root, 'extra.js'), 'unexpected');
  assert.notEqual(inventory(root).sha256, before.sha256);
  fs.unlinkSync(path.join(root, 'extra.js'));
  fs.unlinkSync(path.join(root, 'dist/extension.js'));
  assert.notEqual(inventory(root).sha256, before.sha256);
  const link = path.join(root, 'linked-dist');
  fs.symlinkSync(path.join(root, 'dist'), link, process.platform === 'win32' ? 'junction' : 'dir');
  try { assert.throws(() => inventory(root), /must not contain links/); }
  finally { fs.unlinkSync(link); }
});
test('initial installed runtime binds final VSIX files and normalizes only root installer metadata', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-editor-shipped-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const crypto = require('node:crypto');
  const packageJson = {name: 'vcp-local', version: '0.2.1', main: './dist/extension.js'};
  const payload = new Map([
    ['package.json', JSON.stringify(packageJson, null, 2) + '\n'],
    ['dist/extension.js', 'reviewed runtime'],
    ['dist/engine_connection.js', 'reviewed engine connection'],
    ['node_modules/@vcp/sdk/package.json', '{"version":"0.2.1"}'],
  ]);
  const manifest = {schema: 'vcp-vsix-package/1', files: [...payload].map(([name, text]) => ({path: 'extension/' + name, bytes: Buffer.byteLength(text), sha256: crypto.createHash('sha256').update(text).digest('hex')}))};
  const restore = () => { for (const [name, text] of payload) { fs.mkdirSync(path.dirname(path.join(root, name)), {recursive: true}); fs.writeFileSync(path.join(root, name), text); } };
  restore();
  assert.equal(verifiedInventory(root, manifest).shipped_payload_verified, true);
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({...packageJson, __metadata: {installedTimestamp: 1, targetPlatform: 'win32-x64', isPreReleaseVersion: true}}));
  assert.equal(verifiedInventory(root, manifest).shipped_payload_verified, true);
  for (const name of ['dist/extension.js', 'dist/engine_connection.js']) {
    fs.writeFileSync(path.join(root, name), 'wrong initial installed content');
    assert.throws(() => verifiedInventory(root, manifest), /differs from final VSIX/);
    restore();
    fs.unlinkSync(path.join(root, name));
    assert.throws(() => verifiedInventory(root, manifest), /omits shipped VSIX files/);
    restore();
  }
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({...packageJson, main: './wrong.js', __metadata: {installedTimestamp: 1}}));
  assert.throws(() => verifiedInventory(root, manifest), /differs from final VSIX/);
  restore();
  fs.writeFileSync(path.join(root, 'node_modules/@vcp/sdk/package.json'), '{"version":"0.2.1","__metadata":{}}');
  assert.throws(() => verifiedInventory(root, manifest), /differs from final VSIX/);
  restore();
  fs.writeFileSync(path.join(root, 'extra.js'), 'not shipped');
  assert.throws(() => verifiedInventory(root, manifest), /Unexpected installed extension file/);
});
