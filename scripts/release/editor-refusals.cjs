// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const hash = text => crypto.createHash('sha256').update(text).digest('hex');
function observation(input, result) {
  assert(['restricted', 'trusted'].includes(input.mode));
  assert.equal(result.ok, true, 'Actual installed editor observation failed');
  assert.equal(result.mode, input.mode); assert.equal(result.version, input.version);
  assert.deepEqual(result.scope, input.scope); assert.equal(result.task, input.task);
  assert.equal(result.actualTrusted, input.mode === 'trusted', 'Actual editor trust differs');
  assert.equal(result.developmentPathAbsent, true); assert.equal(result.finalObserver, true);
  assert.equal(result.ordinaryAttachment, true); assert.equal(result.forbiddenRpcCount, 0);
  assert(Number.isSafeInteger(result.contextAttempts) && result.contextAttempts >= 0);
  assert.equal(result.contextRefused, result.contextAttempts); assert.equal(result.contextAccepted, 0);
  if (input.mode === 'restricted') assert.equal(result.contextAttempts, 0);
  assert(Number.isSafeInteger(result.extensionHostPid) && result.extensionHostPid > 0);
  assert(/^[a-f0-9]{64}$/.test(input.driverSha256)); assert.equal(result.driverSha256, input.driverSha256);
  const seen = result.observations;
  if (input.mode === 'restricted') {
    assert.deepEqual(seen.restricted, {actualTrusted: false, grantRefused: true, editsRefused: 3, workspaceRevision: '0'});
    for (const name of ['uninitialized', 'wrongData']) assert.deepEqual(seen[name], {phase: 'unavailable', connected: false});
    assert.deepEqual(seen.unselected, {phase: 'disconnected', generationUnchanged: true});
  } else {
    assert.deepEqual(seen.trust, {granted: true, returnedToObserver: true, explicitControllerReacquired: true});
    assert.deepEqual(seen.outsideSource, {refused: true, diskSha256: hash(input.sourceText)});
    assert.equal(seen.drafts.length, 3);
    for (const [index, action] of ['typing', 'undo', 'reopen'].entries()) {
      const row = seen.drafts[index]; assert.equal(row.action, action);
      assert.equal(row.reviewRefused, true); assert.equal(row.bufferPreserved, true);
      assert.equal(row.diskBeforeSha256, hash(input.sourceText)); assert.equal(row.diskAfterSha256, row.diskBeforeSha256);
      assert.equal(row.capturedDirty, action !== 'reopen'); assert.equal(row.dirtyAfter, action !== 'reopen');
      assert.equal(row.reopened, action === 'reopen');
      assert(Number.isSafeInteger(row.capturedVersion) && row.capturedVersion > 0);
      assert(Number.isSafeInteger(row.afterVersion) && row.afterVersion > 0);
      if (action !== 'reopen') assert(row.afterVersion > row.capturedVersion, 'Typing and undo must change the real document version');
      const captured = (action === 'reopen' ? '' : 'human ') + input.sourceText;
      assert.equal(row.capturedSha256, hash(captured));
      assert.equal(row.afterSha256, hash((action === 'typing' ? 'later ' : '') + captured));
    }
  }
  return {schema: 'vcp-editor-refusal-observation/1', status: 'pass', mode: input.mode,
    extension_version: result.version, extension_host_pid: result.extensionHostPid,
    actual_editor_trusted: result.actualTrusted, final_observer: true, forbidden_rpc_count: 0,
    context_attempts: result.contextAttempts, context_refused: result.contextRefused, context_accepted: 0,
    driver_sha256: result.driverSha256, observations: seen,
    limitations: ['Synthetic paused history; no live binding, provider request, successful native prepare/apply or accounting work.',
      'Real installed extension-host and editor commands with deterministic prompt answers; no human UI acceptance.']};
}
function seedTrust(database, paths) {
  // The pinned editor uses this exact private shared-storage preference. Trust
  // stays enabled; no user profile or production authority record is modified.
  const {DatabaseSync} = require('node:sqlite'), {pathToFileURL} = require('node:url');
  assert(!fs.existsSync(database), 'Fresh private trust preferences required');
  const db = new DatabaseSync(database);
  try {
    db.exec('CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)');
    const uriTrustInfo = paths.map(file => { const url = pathToFileURL(file); return {trusted: true, uri: {scheme: 'file', authority: url.host, path: decodeURIComponent(url.pathname)}}; });
    db.prepare('INSERT INTO ItemTable(key,value) VALUES(?,?)').run('content.trust.model.key', JSON.stringify({uriTrustInfo}));
  } finally { db.close(); }
}
module.exports = {observation};
if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'observation' && args.length === 2) process.stdout.write(JSON.stringify(observation(...args.map(file => JSON.parse(fs.readFileSync(file, 'utf8'))))));
  else if (command === 'seed-trust' && args.length >= 2) seedTrust(args[0], args.slice(1));
  else throw Error('Expected observation INPUT RESULT or seed-trust DATABASE PATH...');
}
