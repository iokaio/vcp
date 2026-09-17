// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { StringDecoder } = require('node:string_decoder');
const { environment, digest, writeManifest } = require('./harness.cjs');

const PROMPT = 'VCP_SYNTHETIC_TRACE_PROMPT';
const ANSWER = 'VCP_SYNTHETIC_TRACE_COMPLETE';
const PATCH_TEXT = 'VCP synthetic native patch\n';
const CASES = ['completion', 'patch-read-only', 'patch', 'retry', 'provider-denied'];
const isPatch = kind => kind === 'patch' || kind === 'patch-read-only';
function events(kind, index) {
  const id = 'fixture-response-' + index;
  const item = isPatch(kind) && index === 0
    ? { type: 'custom_tool_call', call_id: 'fixture-patch', name: 'apply_patch',
      input: '*** Begin Patch\n*** Add File: synthetic.txt\n+' + PATCH_TEXT + '*** End Patch' }
    : { type: 'message', role: 'assistant', id: 'fixture-message', content: [{ type: 'output_text', text: ANSWER }] };
  return [{ type: 'response.created', response: { id } }, { type: 'response.output_item.done', item },
    { type: 'response.completed', response: { id, usage: { input_tokens: 10, output_tokens: 2, total_tokens: 12 } } }]
    .map(event => 'data: ' + JSON.stringify(event) + '\n\n').join('');
}
async function fixtureProvider(kind, onRequest = () => {}) {
  if (!CASES.includes(kind)) throw Error('Unknown trace case');
  const requests = [], errors = [];
  const server = http.createServer((req, res) => {
    const chunks = []; let bytes = 0;
    req.on('error', error => errors.push(error.message));
    req.on('data', chunk => {
      bytes += chunk.length;
      if (bytes > 4 * 1024 * 1024) { errors.push('request_limit'); req.destroy(); }
      else chunks.push(chunk);
    });
    req.on('end', () => {
      try {
        const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
        const observation = { method: req.method, path: req.url,
          synthetic_auth: req.headers.authorization === 'Bearer synthetic-local-only', body };
        const index = requests.length;
        requests.push(observation); onRequest(observation, index);
        if (req.method !== 'POST' || req.url !== '/v1/responses') throw Error('Unexpected provider route');
        if (index >= (isPatch(kind) || kind === 'retry' ? 2 : 1)) throw Error('Unexpected extra request');
        if (kind === 'provider-denied' || kind === 'retry' && index === 0) {
          res.writeHead(kind === 'provider-denied' ? 401 : 503, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ error: { message: 'Synthetic fixture response', type: 'fixture_error' } }));
        } else { res.writeHead(200, { 'content-type': 'text/event-stream' }); res.end(events(kind, index)); }
      } catch (error) { errors.push(error.message); res.writeHead(400); res.end(); }
    });
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return { requests, errors, url: 'http://127.0.0.1:' + server.address().port + '/v1',
    close: () => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }) };
}
function verifyTrace(kind, { requests, errors, code, stdout, workspace }) {
  assert.ok(CASES.includes(kind), 'Known trace case');
  assert.deepEqual(errors, [], 'Provider observer errors');
  assert.equal(requests.length, isPatch(kind) || kind === 'retry' ? 2 : 1, 'Exact request count');
  for (const request of requests) {
    assert.equal(request.method, 'POST'); assert.equal(request.path, '/v1/responses');
    assert.equal(request.synthetic_auth, true); assert.equal(request.body.model, 'gpt-5.4');
    assert.ok(JSON.stringify(request.body.input).includes(PROMPT), 'Synthetic prompt reaches provider');
  }
  const output = stdout.trim().split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  assert.equal(output.filter(event => event.type === 'thread.started').length, 1);
  assert.equal(output.filter(event => event.type === 'turn.started').length, 1);
  if (kind === 'provider-denied') {
    assert.ok(Number.isInteger(code) && code !== 0, 'Denied provider produces a nonzero exit');
    assert.equal(output.filter(event => event.type === 'turn.failed').length, 1);
    assert.equal(output.filter(event => event.type === 'turn.completed').length, 0);
  } else {
    assert.equal(code, 0);
    assert.equal(output.filter(event => event.type === 'turn.completed').length, 1);
    assert.equal(output.filter(event => event.type === 'item.completed' && event.item?.type === 'agent_message' && event.item.text === ANSWER).length, 1);
    assert.equal(output.filter(event => event.type === 'turn.failed').length, 0);
  }
  if (isPatch(kind)) {
    assert.ok(requests[0].body.tools.some(tool => tool.type === 'custom' && tool.name === 'apply_patch'));
    const result = requests[1].body.input.find(item => item.type === 'custom_tool_call_output' && item.call_id === 'fixture-patch');
    if (kind === 'patch') {
      assert.ok(result && JSON.stringify(result.output).includes('Success'), 'Observed tool receipt reaches next request');
      assert.equal(fs.readFileSync(path.join(workspace, 'synthetic.txt'), 'utf8'), PATCH_TEXT);
      assert.equal(output.filter(event => event.type === 'item.completed' && event.item?.type === 'file_change' && event.item.status === 'completed').length, 1);
    } else {
      assert.ok(result && JSON.stringify(result.output).includes('read-only sandbox'), 'Read-only rejection reaches next request');
      assert.equal(fs.existsSync(path.join(workspace, 'synthetic.txt')), false);
      assert.equal(output.filter(event => event.item?.type === 'file_change' && event.item.status === 'completed').length, 0);
    }
  } else assert.equal(fs.existsSync(path.join(workspace, 'synthetic.txt')), false);
  if (kind === 'retry') assert.deepEqual(requests[0].body.input, requests[1].body.input, 'Retry preserves input');
  return { requests: requests.length, output_events: output.length, tool_effects: kind === 'patch' ? 1 : 0 };
}
async function traceCase(binary, kind, directory, signal) {
  fs.mkdirSync(directory);
  for (const part of ['workspace', 'home', 'temp']) fs.mkdirSync(path.join(directory, part));
  const workspace = path.join(directory, 'workspace');
  const provider = await fixtureProvider(kind, (request, index) => writeManifest(path.join(directory, 'request-' + index + '.json'), request));
  const config = ['model_provider="fixture"',
    'model_providers.fixture={name="fixture",base_url="' + provider.url + '",wire_api="responses",env_key="VCP_SYNTHETIC_KEY",requires_openai_auth=false,request_max_retries=1,stream_max_retries=0}',
    'analytics.enabled=false', 'feedback.enabled=false', 'check_for_update_on_startup=false',
    'web_search="disabled"', 'features.memories=false', 'features.shell_snapshot=false', 'project_doc_max_bytes=0'];
  const args = ['exec', '--skip-git-repo-check', '--ignore-user-config', '--ignore-rules', '--ephemeral', '--json',
    '--model', 'gpt-5.4', '--sandbox', kind === 'patch' ? 'danger-full-access' : 'read-only', '-c', 'approval_policy="never"', '-C', workspace,
    ...config.flatMap(value => ['-c', value]), PROMPT];
  const home = path.join(directory, 'home'), temporary = path.join(directory, 'temp');
  const env = { ...environment(), HOME: home, USERPROFILE: home, APPDATA: home, LOCALAPPDATA: home,
    CODEX_HOME: home, TEMP: temporary, TMP: temporary, VCP_SYNTHETIC_KEY: 'synthetic-local-only' };
  let stdout = '', stderr = '', reason = null, code = null;
  const started = new Date().toISOString();
  try {
    await new Promise((resolve, reject) => {
      let closed = false, stopping = false, killTimer;
      let observedBytes = 0;
      const decoders = [new StringDecoder('utf8'), new StringDecoder('utf8')];
      const child = spawn(binary, args, { cwd: workspace, env, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
      const stop = why => {
        if (closed || stopping) return;
        stopping = true; reason = why;
        // Only this still-owned live child's tree is targeted. Timeout is not a VCP pause.
        if (child.pid) {
          const killer = spawn('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore', env });
          killer.on('error', () => child.kill());
          killer.on('close', status => { if (status !== 0 && !closed) child.kill(); });
        }
        killTimer = setTimeout(() => {
          child.stdout.destroy(); child.stderr.destroy(); child.unref();
          done(Error('Process cleanup could not be confirmed'), null);
        }, 6000);
      };
      const cancel = () => stop('cancelled');
      const timer = setTimeout(() => stop('timeout'), 60000);
      [child.stdout, child.stderr].forEach((stream, index) => stream.on('data', bytes => {
        observedBytes += bytes.length;
        if (observedBytes > 8 * 1024 * 1024) { stop('output_limit'); return; }
        const text = decoders[index].write(bytes);
        if (index === 0) stdout += text; else stderr += text;
      }));
      function done(error, exitCode) {
        if (closed) return;
        closed = true; clearTimeout(timer); clearTimeout(killTimer); signal?.removeEventListener('abort', cancel);
        stdout += decoders[0].end(); stderr += decoders[1].end();
        code = exitCode; error ? reject(error) : resolve();
      }
      child.once('error', error => done(error, null)); child.once('close', exitCode => done(null, exitCode));
      signal?.addEventListener('abort', cancel, { once: true }); if (signal?.aborted) cancel();
    });
    if (reason) throw Error(reason);
    const result = verifyTrace(kind, { ...provider, code, stdout, workspace });
    return { kind, status: 'pass', ...result, exit_code: code, started_at: started, ended_at: new Date().toISOString(), command: [binary, ...args] };
  } finally {
    await provider.close();
    fs.writeFileSync(path.join(directory, 'stdout.log'), stdout);
    fs.writeFileSync(path.join(directory, 'stderr.log'), stderr);
    writeManifest(path.join(directory, 'process.json'), { exit_code: code, reason, command: [binary, ...args],
      stdout_sha256: digest(stdout), stderr_sha256: digest(stderr), provider_errors: provider.errors, requests: provider.requests.length });
  }
}
module.exports = { CASES, PROMPT, ANSWER, PATCH_TEXT, events, fixtureProvider, verifyTrace, traceCase };
