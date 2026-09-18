// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), net = require('node:net');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
async function main(argv) {
  if (argv.length !== 4 || argv[0] !== '--fixture' || argv[2] !== '--output-root') throw Error('Supply --fixture executable --output-root directory');
  if (process.platform !== 'win32') { process.exitCode = 3; console.log('not_run: native Windows required'); return; }
  const repository = path.resolve(__dirname, '../..'), binary = path.resolve(argv[1]), output = path.resolve(argv[3]);
  if (!output.startsWith(path.join(repository, 'artifacts') + path.sep)) throw Error('Use an ignored artifacts output root');
  const directory = path.join(output, crypto.randomUUID()); fs.mkdirSync(directory, { recursive: true });
  const record = { task_id: 'P0-05', status: 'running', started_at: new Date().toISOString(),
    fixture_sha256: crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'), stages: [],
    limitations: ['AppContainer candidate restricts a single fixture process. Arbitrary toolchains and integration with the retained CLI sandbox remain P2 work.'] };
  const manifest = path.join(directory, 'manifest.json');
  const save = () => fs.writeFileSync(manifest, JSON.stringify(record, null, 2) + '\n');
  const traffic = [], sockets = new Set();
  const server = net.createServer(socket => {
    sockets.add(socket); let data = '';
    socket.setTimeout(4000, () => socket.destroy());
    socket.on('data', chunk => { data += chunk; if (data.length > 64) socket.destroy(); });
    socket.on('end', () => { traffic.push(data); socket.end(); });
    socket.on('error', () => {}); socket.on('close', () => sockets.delete(socket));
  });
  try {
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
    const controls = [];
    for (const mode of ['control-before', 'restricted', 'control-after']) {
      const outside = path.join(directory, mode); fs.mkdirSync(outside);
      fs.writeFileSync(path.join(outside, 'read-canary.txt'), 'synthetic private canary');
      const nonce = crypto.randomBytes(16).toString('hex'), outcomePath = path.join(directory, mode + '-outcome.json');
      if (mode !== 'restricted') controls.push(nonce);
      const config = path.join(directory, mode + '-input.json');
      fs.writeFileSync(config, JSON.stringify({ mode, nonce, port: server.address().port, binary, outside, outcome: outcomePath }));
      const result = await new Promise((resolve, reject) => {
        const child = spawn('pwsh', ['-NoProfile', '-File', path.join(repository, 'src/tests/support/windows/execution-boundary.ps1'), '-Config', config], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
        let stdout = '', stderr = '';
        const timer = setTimeout(() => { child.kill(); reject(Error('execution fixture timed out')); }, 45000);
        child.stdout.on('data', bytes => { stdout += bytes; if (stdout.length > 1024 * 1024) child.kill(); });
        child.stderr.on('data', bytes => { stderr += bytes; if (stderr.length > 1024 * 1024) child.kill(); });
        child.once('error', error => { clearTimeout(timer); reject(error); });
        child.once('close', (code, signal) => { clearTimeout(timer); resolve({ code, signal, stdout, stderr }); });
      });
      fs.writeFileSync(path.join(directory, mode + '-stdout.log'), result.stdout);
      fs.writeFileSync(path.join(directory, mode + '-stderr.log'), result.stderr);
      assert.equal(result.code, 0, mode + ' exit'); assert.equal(result.signal, null);
      const observed = JSON.parse(result.stdout.trim());
      const outcome = JSON.parse(fs.readFileSync(outcomePath, 'utf8').replace(/^\uFEFF/, ''));
      const allowed = mode !== 'restricted';
      const { inside_error, outside_error, ...access } = observed;
      assert.deepEqual(access, { inside_write: true, outside_read: allowed, outside_write: allowed, junction_write: allowed, network: allowed }, JSON.stringify(observed));
      assert.equal(fs.existsSync(path.join(outside, 'write-canary.txt')), allowed);
      assert.equal(fs.existsSync(path.join(outside, 'junction-canary.txt')), allowed);
      assert.equal(outcome.cleanup, 'completed'); assert.equal(outcome.result.AppContainer, !allowed);
      if (!allowed) { assert.equal(outcome.result.CapabilityCount, 0); assert.equal(outcome.result.TokenSidMatchesProfile, true); }
      record.stages.push({ mode, observed, process: outcome.result, cleanup: outcome.cleanup }); save();
    }
    await new Promise(resolve => server.close(resolve));
    assert.deepEqual(traffic.sort(), controls.sort());
    record.traffic = { controls_observed: 2, restricted_connections: 0 };
    record.status = 'pass'; record.exit_code = 0;
  } catch (error) { record.status = 'fail'; record.exit_code = 1; record.reason = error.message; }
  finally {
    for (const socket of sockets) socket.destroy();
    if (server.listening) await new Promise(resolve => server.close(resolve));
    record.ended_at = new Date().toISOString(); save();
    console.log(JSON.stringify({ status: record.status, reason: record.reason, manifest })); process.exitCode = record.exit_code;
  }
}
main(process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
