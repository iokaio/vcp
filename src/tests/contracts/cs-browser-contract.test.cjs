// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), http = require('node:http');
const { createHash } = require('node:crypto');
const { ownedRoot } = require('../support/experiments.cjs');
const { validate, start } = require('../../../scripts/evals/cs-browser-contract.cjs');
async function freePort() { const s = http.createServer(); await new Promise(resolve => s.listen(0, '127.0.0.1', resolve)); const port = s.address().port; await new Promise(resolve => s.close(resolve)); return port; }
async function fixture(t) {
  const owner = ownedRoot(os.tmpdir()); t.after(() => owner.cleanup());
  const bytes = Buffer.from('<!doctype html><title>Original local form</title>'); fs.writeFileSync(path.join(owner.root, 'form.html'), bytes);
  return { schema_version: 1, origin: `http://127.0.0.1:${await freePort()}`, application_id: 'synthetic_form_v1', root: owner.root,
    files: [{ url: '/form.html', path: 'form.html', mime: 'text/html', bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }], actions: [{ kind: 'navigate', path: '/form.html' }], timeout_ms: 5000 };
}
function request(spec, target, options = {}) {
  return new Promise((resolve, reject) => { const req = http.request({ hostname: '127.0.0.1', port: Number(new URL(spec.origin).port), path: target, method: options.method || 'GET', headers: options.headers || {}, agent: false }, res => { const chunks = []; res.on('data', b => chunks.push(b)); res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks).toString('utf8') })); }); req.on('error', reject); req.setTimeout(1500, () => req.destroy(Error('test HTTP deadline'))); req.end(); });
}
test('serves only bound bytes and readiness identity; no workspace script execution', async t => {
  const spec = await fixture(t), script = Buffer.from('throw Error("must never execute on server");');
  fs.writeFileSync(path.join(spec.root, 'mapped.js'), script);
  spec.files.push({ url: '/mapped.js', path: 'mapped.js', mime: 'text/javascript', bytes: script.length, sha256: createHash('sha256').update(script).digest('hex') });
  const server = await start(spec); t.after(() => server.close());
  const ready = await request(spec, '/__vcp_ready'); assert.equal(ready.status, 200); assert.deepEqual(JSON.parse(ready.body), { application_id: spec.application_id, origin: spec.origin });
  const original = await request(spec, '/form.html'); assert.equal(original.status, 200); assert.match(original.headers['content-type'], /^text\/html/); assert.match(original.body, /Original local form/);
  assert.equal((await request(spec, '/mapped.js')).body, script.toString());
  fs.writeFileSync(path.join(spec.root, 'form.html'), 'changed after capture'); assert.equal((await request(spec, '/form.html')).body, original.body);
  fs.writeFileSync(path.join(spec.root, 'unmapped.js'), 'throw Error("must never execute")'); assert.equal((await request(spec, '/unmapped.js')).status, 404);
  assert.equal((await request(spec, '/form.html', { method: 'HEAD' })).body, '');
  await server.close(); await server.close(); assert.equal(server.closed, true); await assert.rejects(request(spec, '/form.html'));
});
test('rejects nonliteral origins, scripts, redirects, unknown fields and unsafe maps', async t => {
  const spec = await fixture(t);
  const mutations = [
    p => { p.origin = 'http://localhost:8080'; }, p => { p.origin = 'http://127.0.0.1:0'; }, p => { p.origin += '/'; }, p => { p.origin = 'http://127.0.0.1:65536'; },
    p => { p.redirect = 'http://127.0.0.1:9000'; }, p => { p.actions = [{ kind: 'evaluate', code: 'process.exit()' }]; }, p => { p.actions[0].script = 'malicious'; },
    p => { p.actions[0].path = 'http://127.0.0.1:9000/form.html'; }, p => { p.actions[0].path = '/missing'; },
    ...['../form.html', '%2e%2e/form.html', 'a\\form.html', 'C:/secret', 'a/../form.html'].map(value => p => { p.files[0].path = value; }),
    ...['/%2e%2e/form.html', '/a/../form.html', '//form.html', '/form.html?redirect=remote', '/__vcp_ready'].map(value => p => { p.files[0].url = value; }),
    p => { p.files[0].mime = 'application/octet-stream'; }, p => { p.files[0].bytes = 65537; }, p => { p.files.push({ ...p.files[0] }); }, p => { p.timeout_ms = 60001; },
  ];
  for (const mutate of mutations) { const value = structuredClone(spec); mutate(value); assert.throws(() => validate(value)); }
  assert.deepEqual(validate(spec), spec);
});
test('HTTP rejects traversal, encoded paths, foreign authorities and mutation methods without redirects', async t => {
  const spec = await fixture(t), server = await start(spec); t.after(() => server.close());
  for (const target of ['/../form.html', '/%2e%2e/form.html', '/%252e%252e/form.html', '//form.html', '/form.html?x=1', 'http://other.invalid/form.html', '/form.html#fragment']) {
    const result = await request(spec, target); assert.equal(result.status, 400, target); assert.equal(result.headers.location, undefined);
  }
  for (const options of [{ headers: { Host: 'localhost:1234' } }, { headers: { Origin: 'http://127.0.0.1:1' } }, { method: 'POST' }]) assert.equal((await request(spec, '/form.html', options)).status, 403);
});
test('occupied port fails without closing existing user server', async t => {
  const user = http.createServer((_req, res) => res.end('user-owned')); await new Promise(resolve => user.listen(0, '127.0.0.1', resolve)); t.after(() => new Promise(resolve => user.close(resolve)));
  const spec = await fixture(t); spec.origin = `http://127.0.0.1:${user.address().port}`;
  await assert.rejects(start(spec), { code: 'EADDRINUSE' }); assert.equal((await request(spec, '/')).body, 'user-owned');
});
test('source hash/type and linked ancestors reject before binding', async t => {
  const spec = await fixture(t), original = fs.readFileSync(path.join(spec.root, 'form.html'));
  fs.writeFileSync(path.join(spec.root, 'form.html'), Buffer.alloc(original.length, 120)); await assert.rejects(start(spec), /identity/);
  fs.writeFileSync(path.join(spec.root, 'form.html'), original);
  fs.mkdirSync(path.join(spec.root, 'actual')); fs.writeFileSync(path.join(spec.root, 'actual/form.html'), original);
  fs.symlinkSync(path.join(spec.root, 'actual'), path.join(spec.root, 'linked'), process.platform === 'win32' ? 'junction' : 'dir');
  spec.files[0].path = 'linked/form.html'; await assert.rejects(start(spec), /Linked/);
});
test('lifetime closes owned server while leaving another server alive', async t => {
  const spec = await fixture(t); spec.timeout_ms = 40; const owned = await start(spec); t.after(() => owned.close());
  const user = http.createServer((_req, res) => res.end('alive')); await new Promise(resolve => user.listen(0, '127.0.0.1', resolve)); t.after(() => new Promise(resolve => user.close(resolve)));
  await new Promise(resolve => setTimeout(resolve, 100)); assert.equal(owned.closed, true); await assert.rejects(request(spec, '/form.html'));
  assert.equal((await request({ origin: `http://127.0.0.1:${user.address().port}` }, '/')).body, 'alive');
});
