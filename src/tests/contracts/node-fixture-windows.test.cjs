// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { checkResponse } = require('../../../scripts/evals/node-fixture-protocol.cjs');
const root = path.resolve(__dirname, '../../..');
const runner = path.join(root, 'scripts/evals/node-fixture-runner.ps1');
const bootstrap = path.join(root, 'scripts/evals/node-fixture-bootstrap.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
test('actual Windows pinned Node runner and hostile protocol outputs', { skip: process.platform !== 'win32', timeout: 120000 }, t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-node-protocol-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const nodeHash = hash(process.execPath);
  const candidate = path.join(directory, 'candidate.cjs');
  const requestId = crypto.randomBytes(16).toString('hex');
  function run(code, modify = () => {}) {
    fs.writeFileSync(candidate, code);
    const config = { schema: 1, node: process.execPath, node_sha256: nodeHash,
      bootstrap_sha256: hash(bootstrap), memory_bytes: 268435456, output_limit: 65536, timeout_ms: 3000,
      input_base64: Buffer.from(JSON.stringify({ id: requestId, input: { value: 41 } })).toString('base64'),
      files: [{ name: 'candidate.cjs', path: candidate, sha256: hash(candidate) }] };
    modify(config);
    const configPath = path.join(directory, 'config.json'); fs.writeFileSync(configPath, JSON.stringify(config));
    const child = spawnSync('pwsh', ['-NoProfile', '-File', runner, '-Config', configPath], {
      cwd: root, encoding: 'utf8', timeout: 15000, maxBuffer: 1048576, windowsHide: true });
    assert.ifError(child.error);
    return child;
  }
  const good = run('exports.compute = input => input.value + 1;');
  assert.equal(good.status, 0, good.stderr);
  assert.deepEqual(checkResponse(JSON.parse(good.stdout), requestId, 42), { external_response_pass: true });
  const readonly = run(`exports.compute = () => {
    const fs = require('node:fs'); let changed = 0;
    for (const file of ['candidate.cjs', 'bootstrap.cjs', 'package.json', 'new-file', 'temp/new-file']) {
      try { fs.writeFileSync(file, 'bad'); changed++; } catch {}
    }
    return changed;
  };`);
  assert.equal(readonly.status, 0, readonly.stderr);
  checkResponse(JSON.parse(readonly.stdout), requestId, 0);
  for (const code of [
    'exports.compute = () => 43;',
    'process.stdout.write(JSON.stringify({passed:true}) + "\\n"); exports.compute = () => 42;',
    'exports.compute = () => { process.stdout.write("{}\\n"); return 42; };',
    'exports.compute = () => { process.stderr.write("diagnostic"); return 42; };',
    'exports.compute = () => { process.exit(0); };',
  ]) {
    const result = run(code); assert.equal(result.status, 0, result.stderr);
    assert.throws(() => checkResponse(JSON.parse(result.stdout), requestId, 42));
  }
  for (const modify of [c => c.node_sha256 = '0'.repeat(64), c => c.files[0].sha256 = '0'.repeat(64),
    c => c.files[0].name = '../candidate.cjs', c => c.files.push({ ...c.files[0] }),
    c => c.input_base64 = Buffer.alloc(65537).toString('base64'), c => c.memory_bytes = 1]) {
    const result = run('exports.compute = () => 42;', modify);
    assert.notEqual(result.status, 0, 'Invalid authority or identity unexpectedly launched');
    assert.equal(result.stdout, '');
  }
  assert.equal(hash(process.execPath), nodeHash);
});
const { checkInteractiveReceipt } = require('../../../scripts/evals/node-fixture-protocol.cjs');
const { openInteractive } = require('../../../scripts/evals/node-fixture-session.cjs');
const interactiveBootstrap = path.join(root, 'scripts/evals/node-fixture-interactive-bootstrap.cjs');
function alive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) { return error.code === 'EPERM'; }
}
test('actual Windows interactive relay with parent-owned transport and hostile frames', { skip: process.platform !== 'win32', timeout: 300000 }, async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-node-interactive-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const nodeHash = hash(process.execPath);
  let count = 0;
  function configure(code, modify = () => {}) {
    const candidate = path.join(directory, `candidate-${++count}.cjs`);
    fs.writeFileSync(candidate, code);
    const config = { schema: 1, mode: 'interactive', node: process.execPath, node_sha256: nodeHash,
      bootstrap_sha256: hash(interactiveBootstrap), memory_bytes: 268435456, output_limit: 65536, timeout_ms: 20000,
      interaction: { max_frames: 16, max_frame_bytes: 4096, max_total_bytes: 65536, idle_ms: 3000 },
      files: [{ name: 'candidate.cjs', path: candidate, sha256: hash(candidate) }] };
    modify(config);
    const configPath = path.join(directory, `config-${count}.json`);
    fs.writeFileSync(configPath, JSON.stringify(config));
    return configPath;
  }
  const echo = 'exports.interact = async ch => { let b; while ((b = await ch.receive()) !== null) ch.send({ echo: b }); };';

  // The parent plays the transport, so the request and the absence of retries are
  // observed outside the container rather than reported by the candidate.
  const transport = openInteractive(configure(`exports.interact = async ch => {
    const input = await ch.receive();
    ch.send({ type: 'transport', request: { model: input.model, prompt: input.prompt } });
    const reply = await ch.receive();
    ch.send({ type: 'result', text: reply.text });
  };`));
  transport.send({ model: 'synthetic-model-v1', prompt: 'hello' });
  assert.deepEqual(await transport.receive(), { type: 'transport', request: { model: 'synthetic-model-v1', prompt: 'hello' } });
  transport.send({ text: 'hi' });
  assert.deepEqual(await transport.receive(), { type: 'result', text: 'hi' });
  const transported = await transport.close();
  assert.deepEqual(checkInteractiveReceipt(transported.receipt, transported), { external_interaction_pass: true });

  // Forged, duplicate, reordered or unframed child output cannot be decoded as a reply.
  // The session is not secret from the candidate, so hostile code learns it by
  // intercepting its own stdout; only the parent's sequence check rejects it.
  const capture = "let session; const write = process.stdout.write.bind(process.stdout); " +
    "process.stdout.write = (chunk, ...rest) => { session ??= JSON.parse(chunk).session; return write(chunk, ...rest); };";
  const forged = [
    ["exports.interact = async ch => { await ch.receive(); process.stdout.write(JSON.stringify({ session: '0'.repeat(32), seq: 1, body: 1 }) + '\\n'); };", false],
    [`${capture} exports.interact = async ch => { await ch.receive(); ch.send(1); await ch.receive(); write(JSON.stringify({ session, seq: 1, body: 2 }) + '\\n'); };`, true],
    [`${capture} exports.interact = async ch => { await ch.receive(); ch.send(1); await ch.receive(); write(JSON.stringify({ session, seq: 3, body: 3 }) + '\\n'); };`, true],
    ["exports.interact = async ch => { await ch.receive(); process.stdout.write('garbage\\n'); };", false],
  ];
  for (const [code, replyFirst] of forged) {
    const hostile = openInteractive(configure(code));
    hostile.send('probe');
    if (replyFirst) { assert.equal(await hostile.receive(), 1); hostile.send('again'); }
    await assert.rejects(hostile.receive());
    await hostile.close();
  }
  const unrequested = openInteractive(configure('exports.interact = async ch => { await ch.receive(); ch.send(1); ch.send(2); };'));
  unrequested.send('probe');
  assert.equal(await unrequested.receive(), 1);
  const extra = await unrequested.close();
  assert.throws(() => checkInteractiveReceipt(extra.receipt, extra), /unrequested/);

  // Ceilings, stalls and side channels stop or fail the run.
  for (const [termination, code] of [
    ['frame_limit', 'exports.interact = async ch => { await ch.receive(); for (;;) ch.send(1); };'],
    ['frame_bytes', 'exports.interact = async ch => { await ch.receive(); ch.send("x".repeat(5000)); await new Promise(() => {}); };'],
    ['idle_timeout', 'exports.interact = async () => { setInterval(() => {}, 1000); await new Promise(() => {}); };'],
  ]) {
    const bounded = openInteractive(configure(code));
    bounded.send('probe');
    if (termination === 'idle_timeout') await assert.rejects(bounded.receive(1000), /Timed out/);
    const closed = await bounded.close(30000);
    assert.equal(closed.receipt.result.termination, termination);
    assert.throws(() => checkInteractiveReceipt(closed.receipt, closed));
  }
  for (const [code, replies] of [
    ['exports.interact = async ch => { await ch.receive(); process.stdout.write("partial"); };', false],
    ['exports.interact = async ch => { const b = await ch.receive(); process.stderr.write("diagnostic"); ch.send(b); };', true],
  ]) {
    const side = openInteractive(configure(code));
    side.send('probe');
    if (replies) assert.equal(await side.receive(), 'probe');
    const closed = await side.close();
    assert.throws(() => checkInteractiveReceipt(closed.receipt, closed));
  }
  const early = openInteractive(configure('exports.interact = async () => { process.exit(0); };'));
  early.send('probe');
  await assert.rejects(early.receive(), /ended/);
  await early.close();

  // Invalid interactive authority or identity never launches the child.
  for (const modify of [c => { c.mode = 'duplex'; }, c => { c.input_base64 = ''; }, c => { delete c.interaction.idle_ms; },
    c => { c.interaction.idle_ms = 30000; }, c => { c.interaction.max_frames = 4097; }, c => { c.output_limit = 65537; },
    c => { c.bootstrap_sha256 = hash(bootstrap); }]) {
    const rejected = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configure(echo, modify)], {
      cwd: root, encoding: 'utf8', input: '', timeout: 15000, maxBuffer: 1048576, windowsHide: true });
    assert.notEqual(rejected.status, 0, 'Invalid interactive request unexpectedly launched');
    assert.equal(rejected.stdout, '');
  }

  // Abrupt owner loss mid-exchange: the kill-on-close job ends the child and the
  // supervisor reconciles the recorded profile identity.
  const owned = openInteractive(configure(echo));
  const started = await owned.waitStarted();
  assert.match(started.profile, /^iokaio\.vcp\.memory\.[a-f0-9]{32}$/);
  owned.send('first');
  assert.deepEqual(await owned.receive(), { echo: 'first' });
  owned.send('second');
  await owned.kill();
  const deadline = Date.now() + 5000;
  while (alive(started.pid) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
  assert.equal(alive(started.pid), false, 'Contained child survived owner loss');
  const folder = path.join(process.env.LOCALAPPDATA, 'Packages', started.profile);
  const cleanup = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class OwnerLossCleanup { [DllImport(\"userenv.dll\", CharSet=CharSet.Unicode)] public static extern int DeleteAppContainerProfile(string name); }'; " +
    `[Runtime.InteropServices.Marshal]::ThrowExceptionForHR([OwnerLossCleanup]::DeleteAppContainerProfile('${started.profile}'))`],
    { encoding: 'utf8', timeout: 30000, windowsHide: true });
  assert.equal(cleanup.status, 0, cleanup.stderr);
  assert.equal(fs.existsSync(folder), false, 'Owner-loss profile survived supervisor cleanup');
  assert.equal(hash(process.execPath), nodeHash);
});
