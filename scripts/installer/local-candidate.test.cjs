// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const { validateReceipt } = require('./local-candidate.cjs');
function fixture() {
  const selected = { native_version: '0.2.6', target: 'x86_64-pc-windows-msvc' };
  const source = { commit: 'a'.repeat(40), dirty: true, content_sha256: 'b'.repeat(64), files: [{ path: 'input', sha256: 'c'.repeat(64) }] };
  const artifact = name => ({ target: { name }, profile: { test: false, opt_level: '3' }, features: [], package_id: 'path+file:///repo/vcp-cli#0.2.6' });
  const receipt = { schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, source_stable: true,
    toolchain_stable: true, qualification_build: false, profile: 'release', target: selected.target,
    source_commit: source.commit, source_dirty: source.dirty, source_content_sha256: source.content_sha256,
    inputs: source.files, executable_sha256: 'engine', launcher_sha256: 'launcher',
    vcp_features: [{ target: 'vcp', features: [] }], compiler_artifact: artifact('vcp'), launcher_compiler_artifact: artifact('vcp-launch'),
    command: ['cargo', '+1.95.0', 'build', '--locked', '--offline', '--release', '--no-default-features', '-p', 'vcp-cli',
      '--bin', 'vcp', '--bin', 'vcp-launch', '--target', selected.target, '--target-dir', 'D:\\repo\\artifacts\\codex-target', '-j', '4', '--message-format=json-render-diagnostics'],
    rustflags: ['-C', 'link-arg=/STACK:8388608', '-C', 'target-feature=+crt-static'], rustc: ['release: 1.95.0'] };
  return { receipt, selected, source };
}
test('dirty local production receipt is accepted without fabricating release evidence', () => {
  const { receipt, selected, source } = fixture();
  assert.doesNotThrow(() => validateReceipt(receipt, selected, source, 'engine', 'launcher'));
});
for (const [name, change] of [
  ['stale input bytes', r => { r.inputs = []; }], ['wrong engine', r => { r.executable_sha256 = 'other'; }],
  ['wrong launcher', r => { r.launcher_sha256 = 'other'; }], ['qualification flag', r => { r.qualification_build = true; }],
  ['qualification dependency', r => { r.vcp_features[0].features = ['qualification']; }],
  ['version drift', r => { r.launcher_compiler_artifact.package_id = 'path+file:///repo/vcp-cli#0.2.5'; }],
  ['release envelope', r => { r.release = {}; }], ['unrecorded flags', r => { r.command.push('--features=qualification'); }],
  ['failed build', r => { r.cargo_exit_code = 1; }], ['changed toolchain', r => { r.toolchain_stable = false; }],
]) test('rejects ' + name, () => {
  const { receipt, selected, source } = fixture(); change(receipt);
  assert.throws(() => validateReceipt(receipt, selected, source, 'engine', 'launcher'));
});
