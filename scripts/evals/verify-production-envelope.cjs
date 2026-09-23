// SPDX-License-Identifier: Apache-2.0
'use strict';
// Independent production write check. Private recovery bytes are consumed only
// by pinned Go age; decrypted payloads stay in this bounded process memory.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const {spawnSync} = require('node:child_process');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const AGE_SHA256 = '2821a4ed191da07372acd302e5f6feae7a7985e285e1417765ebe74025af45f0';
let input = [], size = 0;
process.stdin.on('data', chunk => {
  size += chunk.length;
  if (size > 4 * 1024 * 1024) process.exit(1);
  input.push(chunk);
});
process.stdin.on('end', () => {
  let phase = 'input-and-ciphertext';
  try {
    const {age, key, object, expected: x} = JSON.parse(Buffer.concat(input));
    assert.equal(hash(fs.readFileSync(age)), AGE_SHA256);
    const ciphertext = fs.readFileSync(object);
    assert(ciphertext.length <= 65 * 1024 * 1024);
    assert(ciphertext.subarray(0, 22).equals(Buffer.from('age-encryption.org/v1\n')));
    for (const marker of ['AGE-SECRET-KEY-', 'VCP writer Ed25519', ...Object.values(x.files)]) {
      assert(!ciphertext.includes(Buffer.from(marker)));
    }
    phase = 'independent-age-decryption';
    const decrypted = spawnSync(age, ['--decrypt', '--identity', key, object], {
      windowsHide: true, timeout: 60000, maxBuffer: 32 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    assert(!decrypted.error && decrypted.status === 0);
    const e = JSON.parse(decrypted.stdout);
    phase = 'signature-and-manifest';
    const bytes = value => {
      assert(Array.isArray(value) && value.every(n => Number.isInteger(n) && n >= 0 && n <= 255));
      return Buffer.from(value);
    };
    const body = bytes(e.body), writer = bytes(e.writer), signature = bytes(e.signature);
    assert.equal(writer.length, 32); assert.equal(signature.length, 64);
    assert.deepEqual(e.writer, x.writer);
    const publicKey = crypto.createPublicKey({key: Buffer.concat([
      Buffer.from('302a300506032b6570032100', 'hex'), writer,
    ]), format: 'der', type: 'spki'});
    assert(crypto.verify(null, Buffer.concat([
      Buffer.from('vcp-portable-manifest-signature-v1\0'), body,
    ]), publicKey, signature));
    const m = JSON.parse(body);
    assert.equal(m.format, 'vcp-signed-age/1');
    for (const name of ['workspace', 'lineage', 'sequence', 'deletion', 'parent']) assert.deepEqual(m[name], x[name]);
    assert.equal(hash(body), x.manifest_sha256);
    const names = Object.keys(m.objects).sort();
    assert(names.length > 0 && names.length <= 4096);
    assert.deepEqual(names, Object.keys(e.payloads).sort());
    const payloads = new Map(); let payloadBytes = 0;
    phase = 'payload-digests';
    for (const name of names) {
      assert.match(name, /^[a-f0-9]{64}$/);
      const payload = bytes(e.payloads[name]);
      assert.equal(m.objects[name].sha256, name);
      assert.equal(payload.length, m.objects[name].bytes);
      assert.equal(hash(payload), name);
      payloadBytes += payload.length; assert(payloadBytes <= 16 * 1024 * 1024);
      for (const marker of ['AGE-SECRET-KEY-', 'VCP writer Ed25519']) assert(!payload.includes(marker));
      payloads.set(name, payload);
    }
    const inventories = [...payloads.values()].flatMap(payload => {
      try { const value = JSON.parse(payload); return value.format === 'vcp-neutral-history/1' ? [value] : []; }
      catch { return []; }
    });
    assert.equal(inventories.length, 1);
    const inv = inventories[0];
    phase = 'canonical-history';
    assert.equal(inv.workspace, x.workspace);
    assert.equal(inv.authority, 'historical_only_rebind_required');
    assert.equal(inv.coverage.workspace_checkpoint, true);
    const state = JSON.parse(payloads.get(inv.canonical));
    assert.equal(state.watermark, inv.watermark);
    for (const row of Object.values(state.records)) assert.equal(row.workspace, x.workspace);
    assert(state.events.length > 0 && Object.keys(state.commands).length > 0);
    for (const row of state.events) assert.equal(row.event.workspace, x.workspace);
    for (const row of Object.values(state.commands)) assert.equal(row.workspace, x.workspace);
    assert(x.records.some(row => row.collection === 'task'));
    assert(x.records.some(row => row.collection === 'ledger'));
    for (const row of x.records) {
      const actual = state.records[row.collection + ':' + row.id];
      assert.equal(actual.workspace, x.workspace); assert.deepEqual(actual.value, row.value);
    }
    let sourceBytes = 0;
    phase = 'source-checkpoint';
    for (const [path, contents] of Object.entries(x.files)) {
      const source = inv.inputs.checkpoint.sources[path]; assert.equal(typeof source, 'string');
      const descriptor = state.records['artifact:' + source].value;
      assert.equal(descriptor.spec.scope.workspace, x.workspace); assert.equal(descriptor.state, 'complete');
      const chunks = inv.parts.filter(part => part.artifact === source && part.role.kind === 'chunk').sort((a,b) => a.role.index - b.role.index);
      assert(chunks.length > 0);
      const full = Buffer.concat(chunks.map((part,index) => {
        assert.equal(part.role.index,index); const chunk=payloads.get(part.digest); assert.equal(part.bytes,chunk.length); return chunk;
      }));
      assert(full.equals(Buffer.from(contents))); assert.equal(descriptor.sha256,hash(full));
      assert.equal(String(descriptor.length),String(full.length)); sourceBytes += full.length;
    }
    process.stdout.write(JSON.stringify({schema:'vcp-production-independent-envelope/1',status:'pass',age_sha256:AGE_SHA256,
      ciphertext_sha256:hash(ciphertext),signature_verified:true,writer_matches_enrollment:true,payloads_verified:names.length,
      payload_bytes_verified:payloadBytes,source_files_verified:Object.keys(x.files).length,source_bytes_verified:sourceBytes,
      task_ledger_records_verified:x.records.length,events_scope_verified:state.events.length,commands_scope_verified:Object.keys(state.commands).length,
      manifest_sha256:hash(body),ciphertext_marker_scan:true,excluded_recovery_markers:true})+'\n');
  } catch { process.stderr.write(`Independent production envelope validation failed (${phase})\n`); process.exitCode=1; }
});
