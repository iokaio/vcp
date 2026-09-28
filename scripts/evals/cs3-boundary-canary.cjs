// SPDX-License-Identifier: Apache-2.0
'use strict';
// Fixed synthetic operations only; runs under the existing AppContainer fixture.
const fs = require('node:fs'), net = require('node:net'), path = require('node:path');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
if (!['control', 'restricted'].includes(input.mode) || !Array.isArray(input.files) || input.files.length !== 4
  || !Number.isInteger(input.ipv4) || !Number.isInteger(input.ipv6)) throw Error('Invalid canary input');
const rows = [];
function file(id, target, write) {
  try {
    if (write) { const fd = fs.openSync(target, 'r+'); fs.closeSync(fd); }
    else { const data = fs.readFileSync(target); if (data.toString() !== 'CS3_PUBLIC_CANARY\n') throw Error('Canary differs'); }
    rows.push({ id, outcome: 'allowed' });
  } catch (error) { rows.push({ id, outcome: 'denied', code: error.code || 'unexpected' }); }
}
async function connect(id, host, port) {
  await new Promise(resolve => {
    const started = performance.now();
    const socket = net.createConnection({ host, port }); let done = false;
    function end(row) { if (done) return; done = true; socket.destroy(); rows.push({ id, ...row, elapsed_ms: Math.ceil(performance.now() - started) }); resolve(); }
    socket.setTimeout(1500, () => end({ outcome: 'timeout' }));
    socket.once('connect', () => end({ outcome: 'allowed' }));
    socket.once('error', error => end({ outcome: 'denied', code: error.code }));
  });
}
(async () => {
  file('owned_read', path.join(input.inside, 'allowed.txt'), false);
  file('owned_write', path.join(input.inside, 'allowed.txt'), true);
  for (let index = 0; index < input.files.length; index++) {
    file(`host_read_${index}`, input.files[index], false);
    file(`host_write_${index}`, input.files[index], true);
  }
  file('junction_read', path.join(input.inside, 'redirect', 'owner.txt'), false);
  file('junction_write', path.join(input.inside, 'redirect', 'owner.txt'), true);
  file('staged_read', input.readonly, false);
  file('staged_write', input.readonly, true);
  await connect('ipv4_loopback', '127.0.0.1', input.ipv4);
  await connect('ipv6_loopback', '::1', input.ipv6);
  if (input.mode === 'restricted') {
    // RFC 5737/3849 documentation addresses: no DNS, proxy, payload or service.
    // Only immediate access denial is accepted; timeout/refusal is inconclusive.
    await connect('external_ipv4', '192.0.2.1', 443);
    await connect('external_ipv6', '2001:db8::1', 443);
  }
  process.stdout.write(JSON.stringify({ schema: 'cs3-native-denial-canary/1', rows }));
})().catch(() => { process.exitCode = 1; });
