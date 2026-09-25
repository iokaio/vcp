// SPDX-License-Identifier: Apache-2.0
'use strict';
// This entire process, including this adapter, is untrusted to the parent.
// Frames are external behavior for the parent to judge; no child verdict counts.
const decoder = new TextDecoder('utf-8', { fatal: true });
const lines = [];
let pending = '', ended = false, wake = null;
function notify() { if (wake) { const resume = wake; wake = null; resume(); } }
process.stdin.on('data', chunk => {
  pending += decoder.decode(chunk, { stream: true });
  for (let index; (index = pending.indexOf('\n')) >= 0; pending = pending.slice(index + 1)) lines.push(pending.slice(0, index));
  notify();
});
process.stdin.on('end', () => { pending += decoder.decode(); ended = true; notify(); });
async function line() {
  while (lines.length === 0) {
    if (ended) { if (pending !== '') throw Error('Partial parent frame'); return null; }
    await new Promise(resolve => { wake = resolve; });
  }
  return lines.shift();
}
(async () => {
  const header = JSON.parse(await line());
  if (!header || Object.keys(header).join(',') !== 'session' ||
      typeof header.session !== 'string' || !/^[a-f0-9]{32}$/.test(header.session)) throw Error('Invalid session');
  let sent = 0, received = 0;
  const channel = Object.freeze({
    send(body) { process.stdout.write(JSON.stringify({ session: header.session, seq: ++sent, body }) + '\n'); },
    async receive() {
      const text = await line();
      if (text === null) return null;
      const frame = JSON.parse(text);
      if (!frame || Object.keys(frame).sort().join(',') !== 'body,seq' || frame.seq !== ++received) throw Error('Invalid parent frame');
      return frame.body;
    },
  });
  await require('./candidate.cjs').interact(channel);
})().catch(() => { process.exitCode = 1; });
