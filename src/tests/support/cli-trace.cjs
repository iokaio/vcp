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
const COMPACT_PROMPT = 'VCP_SYNTHETIC_COMPACTION_PROMPT';
const COMPACT_SUMMARY = 'VCP_SYNTHETIC_COMPACTION_SUMMARY';
const USAGE = Object.freeze({ input_tokens: 10, output_tokens: 2, total_tokens: 12 });
const CASES = ['completion', 'patch-read-only', 'patch', 'retry', 'provider-denied',
  'review', 'review-denied', 'compaction', 'compaction-denied'];
const isReview = kind => kind === 'review' || kind === 'review-denied';
const isCompaction = kind => kind === 'compaction' || kind === 'compaction-denied';
const isPatch = kind => kind === 'patch' || kind === 'patch-read-only' || isCompaction(kind);
const isDenied = kind => kind === 'provider-denied' || kind === 'review-denied' || kind === 'compaction-denied';
const requestCount = kind => kind === 'compaction' ? 3 : isPatch(kind) || kind === 'retry' ? 2 : 1;
const responseStatus = (kind, index) => isDenied(kind) && (!isCompaction(kind) || index === 1) ? 401
  : kind === 'retry' && index === 0 ? 503 : 200;
function events(kind, index) {
  const id = 'fixture-response-' + index;
  const item = isPatch(kind) && index === 0
    ? { type: 'custom_tool_call', call_id: 'fixture-patch', name: 'apply_patch',
      input: '*** Begin Patch\n*** Add File: synthetic.txt\n+' + PATCH_TEXT + '*** End Patch' }
    : { type: 'message', role: 'assistant', id: 'fixture-message', content: [{ type: 'output_text', text: isCompaction(kind) && index === 1 ? COMPACT_SUMMARY : ANSWER }] };
  return [{ type: 'response.created', response: { id } }, { type: 'response.output_item.done', item },
    { type: 'response.completed', response: { id, usage: USAGE } }]
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
        const validRoute = req.method === 'POST' && req.url === '/v1/responses';
        const status = validRoute && index < requestCount(kind) ? responseStatus(kind, index) : 400;
        observation.response_status = status;
        observation.response_usage = status === 200 ? { ...USAGE } : null;
        requests.push(observation);
        onRequest(observation, index);
        if (!validRoute) throw Error('Unexpected provider route');
        if (index >= requestCount(kind)) throw Error('Unexpected extra request');
        if (status !== 200) {
          res.writeHead(status, { 'content-type': 'application/json' });
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
  assert.equal(requests.length, requestCount(kind), 'Exact request count');
  for (const [index, request] of requests.entries()) {
    const status = responseStatus(kind, index);
    assert.equal(request.response_status, status, 'Observed response status');
    assert.deepEqual(request.response_usage, status === 200 ? USAGE : null, 'Observer records supplied usage');
    assert.equal(request.method, 'POST'); assert.equal(request.path, '/v1/responses');
    assert.equal(request.synthetic_auth, true); assert.equal(request.body.model, 'gpt-5.4');
    assert.ok(JSON.stringify(request.body.input).includes(PROMPT), 'Synthetic prompt reaches provider');
  }
  const output = stdout.trim().split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  assert.equal(output.filter(event => event.type === 'thread.started').length, 1);
  assert.equal(output.filter(event => event.type === 'turn.started').length, 1);
  if (isDenied(kind)) {
    assert.ok(Number.isInteger(code) && code !== 0, 'Denied provider produces a nonzero exit');
    assert.equal(output.filter(event => event.type === 'turn.failed').length, 1);
    assert.equal(output.filter(event => event.type === 'turn.completed').length, 0);
    assert.equal(output.filter(event => event.item?.type === 'agent_message' && event.item.text === ANSWER).length, 0, 'Denied helper cannot report synthetic success');
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
  if (isReview(kind)) {
    assert.ok(requests[0].body.instructions.startsWith('# Review guidelines:'), 'Review delegate uses the retained rubric');
    assert.ok(!requests[0].body.tools.some(tool => ['web_search', 'web_search_preview'].includes(tool.type)), 'Review disables provider web search');
  }
  if (isCompaction(kind)) {
    assert.deepEqual(requests[1].body.tools, [], 'Compaction request cannot invoke tools');
    const promptText = request => JSON.stringify(request.body.input);
    assert.ok(!promptText(requests[0]).includes(COMPACT_PROMPT), 'Coding request precedes compaction');
    assert.ok(promptText(requests[1]).includes(COMPACT_PROMPT), 'Separate compaction prompt reaches provider');
    if (kind === 'compaction') {
      assert.ok(promptText(requests[2]).includes(COMPACT_SUMMARY), 'Next coding request consumes summary');
      assert.ok(!promptText(requests[2]).includes(COMPACT_PROMPT), 'Compaction instructions are not replayed');
      assert.ok(!requests[2].body.input.some(item => item.call_id === 'fixture-patch'), 'Replaced prompt history drops the earlier tool receipt');
      assert.ok(requests[2].body.tools.some(tool => tool.name === 'apply_patch'), 'Coding tools are restored after compaction');
    }
  }
  if (kind === 'retry') assert.deepEqual(requests[0].body.input, requests[1].body.input, 'Retry preserves input');
  const providerUsage = requests.reduce((sum, request) => ({
    input_tokens: sum.input_tokens + (request.response_usage?.input_tokens || 0),
    output_tokens: sum.output_tokens + (request.response_usage?.output_tokens || 0)
  }), { input_tokens: 0, output_tokens: 0 });
  const cliUsage = output.find(event => event.type === 'turn.completed')?.usage || null;
  if (!isDenied(kind)) {
    const expected = isReview(kind) ? { input_tokens: 0, output_tokens: 0 } : providerUsage;
    assert.equal(cliUsage?.input_tokens, expected.input_tokens, 'Pinned parent CLI input usage');
    assert.equal(cliUsage?.output_tokens, expected.output_tokens, 'Pinned parent CLI output usage');
  }
  return { requests: requests.length, output_events: output.length, tool_effects: kind === 'patch' ? 1 : 0,
    provider_usage: providerUsage, cli_usage: cliUsage,
    usage_matches_provider: cliUsage === null ? null
      : cliUsage.input_tokens === providerUsage.input_tokens && cliUsage.output_tokens === providerUsage.output_tokens };
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
  if (isCompaction(kind)) config.push('model_auto_compact_token_limit=1', 'compact_prompt="' + COMPACT_PROMPT + '"');
  const args = ['exec', '--skip-git-repo-check', '--ignore-user-config', '--ignore-rules', '--ephemeral', '--json',
    '--model', 'gpt-5.4', '--sandbox', kind === 'patch' ? 'danger-full-access' : 'read-only', '-c', 'approval_policy="never"', '-C', workspace,
    ...config.flatMap(value => ['-c', value]), ...(isReview(kind) ? ['review', PROMPT] : [PROMPT])];
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
module.exports = { CASES, PROMPT, ANSWER, PATCH_TEXT, COMPACT_PROMPT, COMPACT_SUMMARY, USAGE, events, fixtureProvider, verifyTrace, traceCase };
