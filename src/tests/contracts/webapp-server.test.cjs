// SPDX-License-Identifier: Apache-2.0
'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const http = require('node:http');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { freezeInventory, startOwnedServer } = require('../../../scripts/evals/webapp-server.cjs');

const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-webapp-server-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const files = {
    'index.html': Buffer.from('<!doctype html><form><label>Name<input name="name"></label></form>'),
    'poll.json': Buffer.from('{"ready":true}\n'),
  };
  for (const [name, bytes] of Object.entries(files)) fs.writeFileSync(path.join(root, name), bytes);
  const inventory = freezeInventory({
    root,
    entries: [
      { route: '/', source: 'index.html', sha256: digest(files['index.html']), contentType: 'text/html; charset=utf-8' },
      { route: '/poll', source: 'poll.json', sha256: digest(files['poll.json']), contentType: 'application/json; charset=utf-8', delayMs: 35 },
    ],
    limits: { maxFiles: 2, maxFileBytes: 1024, maxTotalBytes: 2048, maxDelayMs: 100 },
  });
  return { root, files, inventory };
}

function request(port, requestPath = '/', options = {}) {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const req = http.request({ host: '127.0.0.1', port, path: requestPath, method: options.method || 'GET', headers: options.headers }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks), elapsed: Date.now() - started }));
    });
    req.once('error', reject);
    if (options.body) req.write(options.body);
    req.end();
  });
}

function raw(port, bytes) {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host: '127.0.0.1', port });
    const chunks = [];
    socket.once('connect', () => socket.write(bytes));
    socket.on('data', chunk => chunks.push(chunk));
    socket.once('error', reject);
    socket.once('close', () => resolve(Buffer.concat(chunks).toString('latin1')));
  });
}

test('snapshots an explicit hashed inventory and serves exact loopback routes from memory', async t => {
  const f = fixture(t);
  const owned = await startOwnedServer(f.inventory, { limits: { requestMs: 500, shutdownMs: 100 } });
  t.after(() => owned.stop());
  assert.match(owned.origin, /^http:\/\/127\.0\.0\.1:\d+$/);

  fs.writeFileSync(path.join(f.root, 'index.html'), 'changed after accepted snapshot');
  const page = await request(owned.port);
  assert.equal(page.status, 200);
  assert.deepEqual(page.body, f.files['index.html']);
  assert.equal(page.headers['set-cookie'], undefined);
  assert.equal(page.headers['x-content-type-options'], 'nosniff');
  assert.match(page.headers['content-security-policy'], /connect-src 'self'/);

  const head = await request(owned.port, '/', { method: 'HEAD' });
  assert.equal(head.status, 200);
  assert.equal(head.body.length, 0);
  assert.equal(Number(head.headers['content-length']), f.files['index.html'].length);

  const poll = await request(owned.port, '/poll');
  assert.equal(poll.status, 200);
  assert.deepEqual(poll.body, f.files['poll.json']);
  assert.ok(poll.elapsed >= 20, `bounded delayed fixture returned after ${poll.elapsed}ms`);
});

test('rejects changed, oversized, linked, aliased and traversal inventory sources', t => {
  const f = fixture(t);
  const base = { root: f.root, entries: [{ route: '/', source: 'index.html', sha256: digest(f.files['index.html']), contentType: 'text/html; charset=utf-8' }] };
  assert.throws(() => freezeInventory({ ...base, entries: [{ ...base.entries[0], sha256: '0'.repeat(64) }] }), /hash mismatch/);
  assert.throws(() => freezeInventory({ ...base, limits: { maxFileBytes: 4 } }), /maxFileBytes/);
  assert.throws(() => freezeInventory({ ...base, entries: [{ ...base.entries[0], source: '../outside' }] }), /safe relative path/);
  for (const source of ['index.html:stream', 'CON', 'folder./file']) assert.throws(() => freezeInventory({ ...base, entries: [{ ...base.entries[0], source }] }), /safe relative path/);
  assert.throws(() => freezeInventory({ ...base, entries: [{ ...base.entries[0], route: '/%2e%2e/outside' }] }), /safe raw path/);
  assert.throws(() => freezeInventory({ ...base, entries: [{ ...base.entries[0], route: '/a\\b' }] }), /safe raw path/);

  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-webapp-outside-'));
  t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
  fs.writeFileSync(path.join(outside, 'outside.html'), 'outside');
  const alias = path.join(f.root, 'alias');
  fs.symlinkSync(outside, alias, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => freezeInventory({ root: f.root, entries: [{ route: '/', source: 'alias/outside.html', sha256: digest('outside'), contentType: 'text/html; charset=utf-8' }] }), /symlink or reparse/);
  const rootAlias = path.join(path.dirname(f.root), `vcp-webapp-root-alias-${crypto.randomUUID()}`);
  fs.symlinkSync(f.root, rootAlias, process.platform === 'win32' ? 'junction' : 'dir');
  try { assert.throws(() => freezeInventory({ ...base, root: rootAlias }), /symlink or reparse/); }
  finally { fs.unlinkSync(rootAlias); }
});

test('rejects forged frozen inventories before allocating an unbounded decoded body', () => {
  const forged = Object.freeze({
    schema: 'vcp-webapp-memory-inventory/1',
    limits: Object.freeze({ maxFiles: 1, maxFileBytes: 1, maxTotalBytes: 1, maxDelayMs: 0 }),
    totalBytes: 1,
    entries: Object.freeze([Object.freeze({ route: '/', sha256: '0'.repeat(64), contentType: 'text/plain; charset=utf-8', delayMs: 0, bytes: 'A'.repeat(1024) })]),
  });
  assert.rejects(startOwnedServer(forged), /encoded body exceeds bounds/);
});

test('enforces exact host, method, raw path, credential and body boundaries', async t => {
  const f = fixture(t);
  const owned = await startOwnedServer(f.inventory, { limits: { requestMs: 500, shutdownMs: 100 } });
  t.after(() => owned.stop());
  assert.equal((await request(owned.port, '/', { headers: { Host: `localhost:${owned.port}` } })).status, 400);
  assert.equal((await request(owned.port, '/', { method: 'POST' })).status, 400);
  assert.equal((await request(owned.port, '/', { headers: { Cookie: 'secret=value' } })).status, 400);
  assert.equal((await request(owned.port, '/', { headers: { Authorization: 'Bearer secret' } })).status, 400);
  assert.equal((await request(owned.port, '/', { method: 'GET', body: 'x', headers: { 'Content-Length': '1' } })).status, 413);
  assert.equal((await request(owned.port, '/missing')).status, 404);
  for (const target of ['/../outside', '/%2e%2e/outside', '/a%2fb', '/a\\b', '//double', '/?query=1']) {
    const result = await raw(owned.port, `GET ${target} HTTP/1.1\r\nHost: 127.0.0.1:${owned.port}\r\nConnection: close\r\n\r\n`);
    assert.match(result, /^HTTP\/1\.1 400 /, target);
  }
});

test('uses a distinct ephemeral port and never adopts or stops a user-owned server', async t => {
  const user = http.createServer((_request, response) => response.end('user-owned'));
  await new Promise((resolve, reject) => { user.once('error', reject); user.listen({ host: '127.0.0.1', port: 0, exclusive: true }, resolve); });
  t.after(() => new Promise(resolve => user.close(resolve)));
  const userPort = user.address().port;
  const owned = await startOwnedServer(fixture(t).inventory, { limits: { shutdownMs: 100 } });
  assert.notEqual(owned.port, userPort);
  await owned.stop();
  const response = await request(userPort);
  assert.equal(response.status, 200);
  assert.equal(response.body.toString(), 'user-owned');
});

test('bounded shutdown destroys incomplete clients and cancels delayed responses', async t => {
  const owned = await startOwnedServer(fixture(t).inventory, { limits: { requestMs: 2_000, shutdownMs: 50 } });
  const hanging = net.connect({ host: '127.0.0.1', port: owned.port });
  await new Promise((resolve, reject) => { hanging.once('connect', resolve); hanging.once('error', reject); });
  hanging.write('GET / HTTP/1.1\r\nHost:');
  const closed = new Promise(resolve => hanging.once('close', resolve));
  await owned.stop();
  await closed;
  assert.deepEqual(owned.observation(), { requests: 0, connection_attempts: 1, active_sockets: 0, pending_timers: 0, stopping: true, terminal_reason: 'manual_stop', listener_closed: true });
  await assert.rejects(request(owned.port), /ECONNREFUSED|socket hang up/);
});

test('request ceiling closes admission after the final accepted request', async t => {
  const owned = await startOwnedServer(fixture(t).inventory, { limits: { maxHeaderBytes: 1024, maxRequests: 2, requestMs: 500, shutdownMs: 100 } });
  t.after(() => owned.stop());
  const oversized = await raw(owned.port, `GET / HTTP/1.1\r\nHost: 127.0.0.1:${owned.port}\r\nX-Fill: ${'x'.repeat(2000)}\r\nConnection: close\r\n\r\n`);
  assert.match(oversized, /^HTTP\/1\.1 431 /);
  assert.equal((await request(owned.port)).status, 200);
  assert.equal((await request(owned.port)).status, 200);
  await assert.rejects(request(owned.port), /ECONNREFUSED|socket hang up/);
  assert.equal(owned.observation().terminal_reason, 'max_requests');
  assert.equal(owned.observation().requests, 2);
  assert.equal(owned.observation().active_sockets, 0);
  assert.equal(owned.observation().pending_timers, 0);
});

test('connection-attempt ceiling includes parser failures and closes admission', async t => {
  const owned = await startOwnedServer(fixture(t).inventory, { limits: { maxHeaderBytes: 1024, maxConnectionAttempts: 2, requestMs: 500, shutdownMs: 100 } });
  t.after(() => owned.stop());
  assert.match(await raw(owned.port, 'not http\r\n\r\n'), /^HTTP\/1\.1 400 /);
  assert.match(await raw(owned.port, `GET / HTTP/1.1\r\nHost: 127.0.0.1:${owned.port}\r\nX-Fill: ${'x'.repeat(2000)}\r\n\r\n`), /^HTTP\/1\.1 431 /);
  await assert.rejects(request(owned.port), /ECONNREFUSED|socket hang up/);
  assert.equal(owned.observation().terminal_reason, 'max_connection_attempts');
  assert.equal(owned.observation().connection_attempts, 2);
  assert.equal(owned.observation().active_sockets, 0);
  assert.equal(owned.observation().pending_timers, 0);
});

test('owner process loss closes only its ephemeral listener', async t => {
  const f = fixture(t);
  const unrelated = http.createServer((_request, response) => response.end('unrelated'));
  await new Promise((resolve, reject) => { unrelated.once('error', reject); unrelated.listen({ host: '127.0.0.1', port: 0, exclusive: true }, resolve); });
  t.after(() => new Promise(resolve => unrelated.close(resolve)));

  const modulePath = path.resolve(__dirname, '../../../scripts/evals/webapp-server.cjs');
  const childSource = `
    const api = require(${JSON.stringify(modulePath)});
    (async () => {
      const inventory = api.freezeInventory({ root: ${JSON.stringify(f.root)}, entries: [{ route: '/', source: 'index.html', sha256: ${JSON.stringify(digest(f.files['index.html']))}, contentType: 'text/html; charset=utf-8' }] });
      const owned = await api.startOwnedServer(inventory, { limits: { requestMs: 500, shutdownMs: 100 } });
      process.stdout.write('READY ' + owned.port + '\\n');
    })().catch(error => { process.stderr.write(error.stack); process.exitCode = 1; });
  `;
  const child = spawn(process.execPath, ['-e', childSource], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill(); });
  assert.ok(Number.isSafeInteger(child.pid) && child.pid > 0);
  const port = await new Promise((resolve, reject) => {
    let stdout = '', stderr = '';
    const timeout = setTimeout(() => reject(Error(`child readiness timeout: ${stderr}`)), 3_000);
    child.stdout.on('data', chunk => {
      stdout += chunk;
      const match = /^READY ([1-9][0-9]*)\r?\n$/.exec(stdout);
      if (match) { clearTimeout(timeout); resolve(Number(match[1])); }
    });
    child.stderr.on('data', chunk => { stderr += chunk.toString().slice(0, 2048 - stderr.length); });
    child.once('exit', code => { clearTimeout(timeout); reject(Error(`child exited before readiness: ${code}: ${stderr}`)); });
  });
  assert.equal((await request(port)).status, 200);
  assert.equal(child.kill(), true);
  await new Promise(resolve => child.once('exit', resolve));
  await assert.rejects(request(port), /ECONNREFUSED|socket hang up/);
  const survivor = await request(unrelated.address().port);
  assert.equal(survivor.body.toString(), 'unrelated');
});
