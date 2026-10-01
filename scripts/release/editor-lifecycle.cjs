// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const modes = ['install', 'restart', 'failed-update', 'missing', 'reconnect'];
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function files(directory) {
  const root = path.resolve(directory), rows = [];
  function visit(current) {
    const stat = fs.lstatSync(current);
    assert(!stat.isSymbolicLink(), 'Installed extension must not contain links');
    if (stat.isDirectory()) {
      for (const name of fs.readdirSync(current).sort()) visit(path.join(current, name));
    } else {
      assert(stat.isFile(), 'Installed extension must contain ordinary files');
      rows.push({path: path.relative(root, current).split(path.sep).join('/'), bytes: stat.size, sha256: hash(fs.readFileSync(current))});
    }
  }
  visit(root);
  assert(rows.length > 0, 'Installed extension inventory is empty');
  return rows;
}
function inventory(directory) {
  const rows = files(directory);
  return {sha256: hash(JSON.stringify(rows)), files: rows.length};
}
function verifiedInventory(directory, manifest) {
  assert.equal(manifest.schema, 'vcp-vsix-package/1', 'Final VSIX manifest required');
  assert(Array.isArray(manifest.files) && manifest.files.length > 0, 'Shipped VSIX inventory required');
  const expected = new Map(), names = new Set(), installedNames = new Set();
  for (const row of manifest.files) {
    assert(typeof row.path === 'string' && !row.path.includes('\\') && !row.path.includes(':') &&
      row.path.split('/').every(part => part && part !== '.' && part !== '..'), 'Invalid shipped VSIX path');
    assert(!names.has(row.path.toLowerCase()), 'Duplicate shipped VSIX path');
    names.add(row.path.toLowerCase());
    assert(Number.isSafeInteger(row.bytes) && row.bytes >= 0 && /^[a-f0-9]{64}$/.test(row.sha256), 'Invalid shipped VSIX digest');
    let installed;
    if (row.path.startsWith('extension/')) installed = row.path.slice('extension/'.length);
    // The pinned VS Code installer retains this container entry under a new
    // filename. Its complete bytes still belong to the final VSIX inventory.
    else if (row.path === 'extension.vsixmanifest') installed = '.vsixmanifest';
    else assert(row.path === '[Content_Types].xml', 'Unknown VSIX container entry');
    if (installed) {
      assert(!installedNames.has(installed.toLowerCase()), 'Colliding installed VSIX paths');
      installedNames.add(installed.toLowerCase());
      expected.set(installed, row);
    }
  }
  assert(expected.has('package.json') && expected.has('dist/extension.js'), 'Missing shipped extension entrypoints');
  const rows = files(directory);
  for (const row of rows) {
    const shipped = expected.get(row.path);
    assert(shipped, `Unexpected installed extension file: ${row.path}`);
    let digest = row.sha256, bytes = row.bytes;
    if (row.path === 'package.json') {
      // VS Code rewrites only the root package manifest to add installation
      // __metadata. Restore the exact formatting emitted by package.cjs and
      // remove only that installer-owned object; every runtime field stays bound.
      const installed = JSON.parse(fs.readFileSync(path.join(directory, row.path), 'utf8'));
      assert(installed && !Array.isArray(installed) && typeof installed === 'object');
      if (Object.hasOwn(installed, '__metadata')) {
        assert(installed.__metadata && !Array.isArray(installed.__metadata) && typeof installed.__metadata === 'object', 'Invalid installer metadata');
        delete installed.__metadata;
      }
      const normalized = Buffer.from(JSON.stringify(installed, null, 2) + '\n');
      digest = hash(normalized); bytes = normalized.length;
    }
    assert.equal(digest, shipped.sha256, `Installed extension differs from final VSIX: ${row.path}`);
    assert.equal(bytes, shipped.bytes, `Installed extension length differs from final VSIX: ${row.path}`);
    expected.delete(row.path);
  }
  assert.equal(expected.size, 0, 'Installed extension omits shipped VSIX files');
  return {sha256: hash(JSON.stringify(rows)), files: rows.length, shipped_payload_verified: true};
}
function observation(input, result, previous) {
  const index = modes.indexOf(input.mode);
  assert(index >= 0, 'Unknown lifecycle mode');
  if (index === 0) assert.equal(previous, null, 'Initial observation must be fresh');
  else {
    assert.equal(previous?.status, 'pass', 'Prior observation did not pass');
    assert.equal(previous.mode, modes[index - 1], 'Lifecycle observations must run in order');
    assert.equal(previous.extension_version, input.version, 'Original VSIX version changed');
  }
  assert.equal(result.ok, true, 'Actual extension-host observation failed');
  assert.equal(result.mode, input.mode, 'Wrong actual observation mode');
  assert.equal(result.version, input.version, 'Installed VSIX version differs');
  assert.equal(result.developmentPathAbsent, true, 'Development runtime PATH present');
  assert(Number.isSafeInteger(result.extensionHostPid) && result.extensionHostPid > 0, 'Actual extension-host PID required');
  if (previous) assert.notEqual(result.extensionHostPid, previous.extension_host_pid, 'Restart must use a new extension host');
  if (input.mode === 'missing') {
    assert.equal(result.missingEngine, true, 'Missing engine was not refused');
    assert.equal(result.observer, false);
  } else {
    assert.equal(result.observer, true, 'Observer-only connection required');
    assert.deepEqual(result.scope, input.scope, 'Workspace/session scope changed');
    assert.equal(result.task, input.task, 'Paused task changed');
  }
  const report = {status: 'pass', mode: input.mode, extension_version: result.version,
    extension_host_pid: result.extensionHostPid, observer: result.observer,
    development_path_absent: true, model_calls: 0};
  if (input.mode === 'restart') report.restart = {new_extension_host: true, connection: 'explicit-observer'};
  if (input.mode === 'install') {
    assert.equal(result.reloaded, true, 'Actual window reload not observed');
    assert(Number.isSafeInteger(result.beforeReloadHostPid) && result.beforeReloadHostPid > 0);
    assert.notEqual(result.beforeReloadHostPid, result.extensionHostPid, 'Reload did not replace extension host');
    assert.equal(result.unsupportedNegotiation?.data?.kind, 'unsupported_version', 'Native incompatible negotiation not refused');
    assert(Number.isSafeInteger(result.unsupportedNegotiation.code) && result.unsupportedNegotiation.code < 0);
    report.reload = {before_host_pid: result.beforeReloadHostPid, after_host_pid: result.extensionHostPid, observer: true};
    report.incompatible_protocol = {requested: '99.0', error_code: result.unsupportedNegotiation.code, kind: 'unsupported_version'};
  }
  return report;
}
module.exports = {inventory, verifiedInventory, observation};
if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
  if (command === 'inventory' && args.length === 1) process.stdout.write(JSON.stringify(inventory(args[0])));
  else if (command === 'verified-inventory' && args.length === 2) process.stdout.write(JSON.stringify(verifiedInventory(args[0], read(args[1]))));
  else if (command === 'observation' && args.length === 3) process.stdout.write(JSON.stringify(observation(read(args[0]), read(args[1]), args[2] === '-' ? null : read(args[2]))));
  else throw Error('Expected inventory DIRECTORY, verified-inventory DIRECTORY MANIFEST, or observation INPUT RESULT PREVIOUS-OR-DASH');
}
