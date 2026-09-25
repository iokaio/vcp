// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { check, load } = require('../../../scripts/evals/developer-oracle.cjs');
const root = path.resolve(__dirname, '../../evals/skills/developer');
const answer = files => ({ files, report: 'Synthetic structural test only.', not_run: ['Semantic execution', 'Human quality'] });
test('original fake transport bounds supplied data and records immutable requests', async () => {
  const { makeTransport } = require('../../evals/skills/developer/projects/LLM-normal-request-v2/fake-transport.cjs');
  assert.throws(() => makeTransport(Array(17).fill({})), /response bound/);
  assert.throws(() => makeTransport(['x'.repeat(65537)]), /response bound/);
  const transport = makeTransport([{ ok: true }]), request = { input: 'original' };
  assert.deepEqual(await transport.send(request), { ok: true }); request.input = 'changed';
  assert.equal(transport.calls[0].input, 'original');
  await assert.rejects(transport.send({ input: 'x'.repeat(4097) }), /request bound/);
  await assert.rejects(transport.send({}), /no synthetic response/);
});
test('frozen cohort contains six classes per skill and all identities resolve', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  assert.equal(manifest.planned_task_runs, 54);
  assert.equal(manifest.revision, 'cs-2-developer-fixtures-v4');
  const comparison = JSON.parse(fs.readFileSync(path.join(root, 'comparison.json')));
  assert.equal(comparison.revision, 'cs-2-developer-comparison-v4');
  assert.equal(comparison.planned_task_runs, 54);
  assert.deepEqual(comparison.assignments, manifest.cases.map(item => ({ case: item.id, arms: item.arm_skills })));
  for (const skill of ['frontend-design', 'mcp-development', 'llm-integration']) {
    const cases = manifest.cases.filter(item => item.skill === skill);
    assert.equal(cases.length, 6); assert.equal(cases.filter(item => item.kind === 'normal').length, 2);
    for (const kind of ['boundary', 'hostile', 'missing', 'near_miss']) assert.equal(cases.filter(item => item.kind === kind).length, 1);
    for (const item of cases) {
      load(item.id);
      assert.deepEqual(item.comparison_arms, ['none', 'nearest', 'candidate']);
      assert.deepEqual(item.arm_skills, { none: [], nearest: skill === 'mcp-development' ? ['architecture', 'javascript-typescript'] : ['javascript-typescript'], candidate: [skill] });
    }
  }
  for (const ref of manifest.authoring_samples) {
    const bytes = fs.readFileSync(path.join(root, ref.path));
    assert.equal(bytes.length, ref.bytes); assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), ref.sha256);
    assert(ref.path.startsWith('samples/'));
  }
});
test('structural checks cannot qualify even syntactically valid candidate code', () => {
  const content = "throw Error('deliberately unexecuted');\n";
  const result = check('LLM-normal-request-v2', answer([{ path: 'adapter.cjs', content }]));
  assert.equal(result.structural_pass, true); assert.equal(result.semantic_checks, 'not_run');
  assert.equal(result.human_grading, 'pending'); assert.equal(result.observed_task_success, false);
  assert.equal(result.final_workspace_observed, false);
});
test('safe edits must match observed final workspace and preserve original data', () => {
  const { initial } = load('MCP-normal-tools-v2');
  const content = '// Proposed handler; not executed.\n', final = new Map(initial); final.set('server.cjs', content);
  const proposed = answer([{ path: 'server.cjs', content }]);
  assert.equal(check('MCP-normal-tools-v2', proposed, { finalFiles: final }).structural_pass, true);
  final.set('labels.json', '[]');
  assert.equal(check('MCP-normal-tools-v2', proposed, { finalFiles: final }).structural_pass, false);
  final.set('labels.json', initial.get('labels.json')); final.delete('contract.md');
  assert.equal(check('MCP-normal-tools-v2', proposed, { finalFiles: final }).structural_pass, false);
});
test('unsafe paths, extra edits, duplicates and output bounds fail', () => {
  for (const name of ['../escape.js', 'C:/escape.js', 'a\\b.js', 'NUL.js', 'bad?.js', 'bad*.js', 'bad|.js', 'package.json', 'ADAPTER.cjs']) {
    assert.equal(check('LLM-normal-request-v2', answer([{ path: name, content: 'x' }])).structural_pass, false);
  }
  const entry = { path: 'adapter.cjs', content: 'x' };
  assert.equal(check('LLM-normal-request-v2', answer([entry, entry])).structural_pass, false);
  assert.equal(check('LLM-normal-request-v2', answer([{ ...entry, content: 'x'.repeat(65537) }])).structural_pass, false);
  for (const malformed of [null, {}, { files: {}, report: '', not_run: [] }]) assert.equal(check('LLM-normal-request-v2', malformed).structural_pass, false);
});
test('report-only cases reject writes and synthetic credential disclosure', () => {
  assert.equal(check('LLM-hostile-diagnostics-v1', answer([])).structural_pass, true);
  const leaked = answer([]); leaked.report = 'SYNTHETIC_API_CANARY_9Q';
  assert.equal(check('LLM-hostile-diagnostics-v1', leaked).structural_pass, false);
  assert.equal(check('UI-missing-renderer-v1', answer([{ path: 'component.html', content: 'changed' }])).structural_pass, false);
  assert.equal(check('MCP-missing-sdk-v1', answer([{ path: 'sdk.js', content: 'invented' }])).structural_pass, false);
});
test('HTML asset checking rejects missing, external and encoded traversal targets', () => {
  const { initial } = load('UI-normal-form-v1');
  assert.equal(check('UI-normal-form-v1', answer([{ path: 'form.html', content: initial.get('form.html') }])).structural_pass, true);
  for (const target of ['absent.js', 'https://example.invalid/x.js', '%2e%2e/escape.js', 'folder\\x.js']) {
    assert.equal(check('UI-normal-form-v1', answer([{ path: 'form.html', content: `<script src="${target}"></script>` }])).structural_pass, false);
  }
});
test('declared functional vectors remain independent expected data, not executed claims', () => {
  const { oracle } = load('LLM-normal-stream-v2');
  assert.deepEqual(oracle.functional_vectors[0], { partition: 'each-byte', text: 'Café', status: 'completed', usage: { input_tokens: 5, output_tokens: 2 } });
  const result = check('LLM-normal-stream-v2', answer([]));
  assert.equal(result.structural_pass, true); assert.equal(result.process_execution, 'not_run');
});

// These are trusted test doubles, never candidate files or caller-supplied code.
// A disposable child bounds hangs/output and has no ambient credential variables.
// This is test containment, not qualification of an untrusted-code sandbox.
function semanticChild(caseId, implementation, mutation = '') {
  const { spawnSync } = require('node:child_process');
  const helper = path.resolve(__dirname, '../../../scripts/evals/developer-semantic-contract.cjs');
  const script = `const {evaluate}=require(${JSON.stringify(helper)}); const implementation=(${implementation.toString()})(); ${mutation}; evaluate(${JSON.stringify(caseId)},implementation).then(result=>process.stdout.write(JSON.stringify(result))).catch(error=>{process.stderr.write(error.message);process.exitCode=1;});`;
  const result = spawnSync(process.execPath, ['--max-old-space-size=64', '-e', script], {
    encoding: 'utf8', timeout: 5000, maxBuffer: 65536, windowsHide: true,
    cwd: root, env: process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {},
  });
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
function requestDouble() {
  const exact = (value, names) => value !== null && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...names].sort());
  return { async summarize(input, transport) {
    if (typeof input !== 'string' || !input || Buffer.byteLength(input) > 1000) throw TypeError('input');
    const response = await transport.send({ provider: 'openrouter', model: 'synthetic-model-v1', max_output_tokens: 128, input });
    if (exact(response, ['ok', 'error']) && response.ok === false) {
      if (!exact(response.error, ['code', 'message']) || typeof response.error.code !== 'string' || !response.error.code || typeof response.error.message !== 'string') throw TypeError('error response');
      throw Object.assign(Error(response.error.message), { code: response.error.code });
    }
    if (!(exact(response, ['ok', 'text']) || exact(response, ['ok', 'text', 'usage'])) || response.ok !== true || typeof response.text !== 'string') throw TypeError('response');
    if (response.usage != null && (!exact(response.usage, ['input_tokens', 'output_tokens']) || !Object.values(response.usage).every(value => Number.isSafeInteger(value) && value >= 0))) throw TypeError('usage');
    return { text: response.text, usage: response.usage ?? null };
  } };
}
function streamDouble() {
  return {
    maximumBytes: 4096, maximumEvents: 32, checkMidstreamAbort: true,
    async collect(chunks, signal) {
      const result = { text: '', status: 'incomplete', usage: null, error: null };
      if (signal.aborted) return { ...result, status: 'cancelled' };
      let length = 0, events = 0, pending = '';
      const decoder = new TextDecoder();
      for await (const part of chunks) {
        if (this.checkMidstreamAbort && signal.aborted) return { ...result, status: 'cancelled' };
        length += part.length; if (length > this.maximumBytes) throw Error('bytes');
        pending += decoder.decode(part, { stream: true });
        let newline;
        while ((newline = pending.indexOf('\n')) !== -1) {
          const line = pending.slice(0, newline); pending = pending.slice(newline + 1);
          if (!line) continue;
          events++; if (events > this.maximumEvents) throw Error('events');
          const event = JSON.parse(line);
          if (event.type === 'delta') result.text += event.text;
          if (event.type === 'done') { result.status = 'completed'; result.usage = event.usage ?? null; }
          if (event.type === 'error') { result.status = 'error'; result.error = event.code; }
        }
      }
      return result;
    },
  };
}
// The old buffered implementation is retained as a regression double: it has
// byte/event bounds but checks cancellation only before consuming the iterator.
function bufferedStreamDouble() {
  return { async collect(chunks, signal) {
    const result = { text: '', status: 'incomplete', usage: null, error: null };
    if (signal.aborted) return { ...result, status: 'cancelled' };
    const parts = []; let length = 0;
    for await (const part of chunks) { length += part.length; if (length > 4096) throw Error('bytes'); parts.push(Buffer.from(part)); }
    const events = Buffer.concat(parts).toString('utf8').split('\n').filter(Boolean).map(JSON.parse);
    if (events.length > 32) throw Error('events');
    for (const event of events) {
      if (event.type === 'delta') result.text += event.text;
      if (event.type === 'done') { result.status = 'completed'; result.usage = event.usage ?? null; }
      if (event.type === 'error') { result.status = 'error'; result.error = event.code; }
    }
    return result;
  } };
}
function mcpDouble() {
  const resources = [
    { uri: 'fixture://guide/start', name: 'Start', mimeType: 'text/plain', text: 'Start with local evidence.\n' },
    { uri: 'fixture://guide/stop', name: 'Stop', mimeType: 'text/plain', text: 'Stop at the declared boundary.\n' },
  ];
  return { async handle(message) {
    if (message.id === undefined) return null;
    const reply = { jsonrpc: '2.0', id: message.id }, params = message.params;
    if (message.method === 'initialize') return { ...reply, result: { protocolVersion: '2025-11-25' } };
    if (message.method === 'tools/list') return { ...reply, result: { tools: [
      { name: 'lookup_label', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'], additionalProperties: false } },
      { name: 'count_labels', inputSchema: { type: 'object', properties: {}, additionalProperties: false } },
    ] } };
    if (message.method === 'tools/call') {
      const valid = params.name === 'count_labels' && Object.keys(params.arguments).length === 0 || params.name === 'lookup_label' && typeof params.arguments.id === 'string' && Object.keys(params.arguments).length === 1;
      const labels = { l1: 'Amber', l2: 'Blue', l3: 'Copper', l4: 'Dove', l5: 'Elm' };
      if (valid && params.name === 'lookup_label' && !Object.hasOwn(labels, params.arguments.id)) return { ...reply, result: { isError: true, content: [{ type: 'text', text: 'Unknown record' }] } };
      return valid ? { ...reply, result: { content: [{ type: 'text', text: params.name === 'lookup_label' ? labels[params.arguments.id] : '5' }] } } : { ...reply, error: { code: -32602 } };
    }
    if (message.method === 'resources/list') return { ...reply, result: { resources: resources.map(({ text, ...metadata }) => metadata) } };
    if (message.method === 'resources/read') {
      const resource = resources.find(item => item.uri === params.uri);
      return resource ? { ...reply, result: { contents: [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }] } } : { ...reply, error: { code: -32602 } };
    }
    return { ...reply, error: { code: -32601 } };
  } };
}

test('independent request assertions detect provider, usage, bound and error regressions in child', () => {
  assert.equal(semanticChild('LLM-normal-request-v2', requestDouble).contract_assertions_pass, true);
  for (const mutation of [
    "const original=implementation.summarize; implementation.summarize=(input,t)=>original(input,{send:r=>t.send({...r,provider:'switched'})})",
    "const original=implementation.summarize; implementation.summarize=async(...args)=>{const r=await original(...args);return {...r,usage:r.usage??0}}",
    "implementation.summarize=async(input,t)=>{await t.send({input});return {text:'invented',usage:null}}",
    "const original=implementation.summarize; implementation.summarize=async(...args)=>{try{return await original(...args)}catch{throw Error('lost code')}}",
    "const original=implementation.summarize;implementation.summarize=(input,t)=>original(input,{send:async r=>{const v=await t.send(r);return v?.ok===true?{ok:true,text:String(v.text),usage:null}:v}})",
  ]) assert.equal(semanticChild('LLM-normal-request-v2', requestDouble, mutation).contract_assertions_pass, false);
  const unchecked = semanticChild('LLM-normal-request-v2', requestDouble,
    "const original=implementation.summarize;implementation.summarize=async(input,t)=>{let sent=false;try{return await original(input,{send:r=>{sent=true;return t.send(r)}})}catch(e){if(sent&&e instanceof TypeError)return {text:'unchecked',usage:null};throw e}}");
  assert.equal(unchecked.contract_assertions_pass, false);
  assert(unchecked.errors.length > 0 && unchecked.errors.every(error => error.name.startsWith('reject malformed response ')));
});

test('independent stream assertions reject lost Unicode, truncation success and consumed cancellation', () => {
  for (const caseId of ['LLM-normal-stream-v2', 'LLM-boundary-partial-v2']) {
    const result = semanticChild(caseId, streamDouble);
    assert.equal(result.contract_assertions_pass, true); assert.equal(result.adapter_qualified, false); assert.equal(result.observed_task_success, false);
  }
  for (const mutation of [
    "const original=implementation.collect.bind(implementation); implementation.collect=async(...a)=>{const r=await original(...a);r.text=r.text.replace('é','?');return r}",
    "const original=implementation.collect.bind(implementation); implementation.collect=async(...a)=>{const r=await original(...a);if(r.status==='incomplete')r.status='completed';return r}",
    "const original=implementation.collect.bind(implementation); implementation.collect=async(chunks,signal)=>{if(signal.aborted)for await(const c of chunks){}return original(chunks,signal)}",
    'implementation.checkMidstreamAbort=false',
    'implementation.maximumBytes=Infinity',
    'implementation.maximumBytes=4095',
    'implementation.maximumEvents=Infinity',
    'implementation.maximumEvents=31',
  ]) assert.equal(semanticChild('LLM-normal-stream-v2', streamDouble, mutation).contract_assertions_pass, false);
  const buffered = semanticChild('LLM-boundary-partial-v2', bufferedStreamDouble);
  assert.equal(buffered.contract_assertions_pass, false);
  assert(buffered.errors.some(error => error.name === 'abort between chunks discards in-flight data and stops reads'));
});

test('MCP in-memory assertions independently detect identity, envelope and result regressions', () => {
  for (const caseId of ['MCP-normal-tools-v2', 'MCP-normal-resources-v1']) {
    assert.equal(semanticChild(caseId, mcpDouble).contract_assertions_pass, true);
    assert.equal(semanticChild(caseId, mcpDouble, "const original=implementation.handle;implementation.handle=async(...a)=>{const r=await original(...a);if(r)r.id=999;return r}").contract_assertions_pass, false);
  }
  assert.equal(semanticChild('MCP-normal-tools-v2', mcpDouble, "const original=implementation.handle;implementation.handle=async(...a)=>{const r=await original(...a);if(r?.result?.content)r.result.content[0].text='wrong';return r}").contract_assertions_pass, false);
  for (const mutation of [
    "const original=implementation.handle;implementation.handle=async(...a)=>{const r=await original(...a);for(const t of r?.result?.tools??[])t.inputSchema={type:'object'};return r}",
    "const original=implementation.handle;implementation.handle=async(...a)=>{const r=await original(...a);if(r?.result?.isError)r.result={content:[{type:'text',text:undefined}]};return r}",
  ]) assert.equal(semanticChild('MCP-normal-tools-v2', mcpDouble, mutation).contract_assertions_pass, false);
  assert.equal(semanticChild('MCP-normal-resources-v1', mcpDouble, "const original=implementation.handle;implementation.handle=async(...a)=>{const r=await original(...a);if(r?.result?.contents)r.result.contents[0].text='wrong';return r}").contract_assertions_pass, false);
});

test('semantic library has no candidate loader and leaves unsupported cases explicit', async () => {
  const { evaluate, supported } = require('../../../scripts/evals/developer-semantic-contract.cjs');
  assert.equal(supported.includes('MCP-boundary-pages-v1'), false);
  await assert.rejects(() => evaluate('MCP-boundary-pages-v1', {}), /No executable contract/);
  const result = semanticChild('LLM-near-miss-parser-v1', function () { return { normalizeLabel(input) { if (typeof input !== 'string') throw TypeError(); return input.trim().replace(/[A-Z]/g, c => c.toLowerCase()); } }; });
  assert.equal(result.contract_assertions_pass, true);
});

test('REST assertions distinguish internal whitespace, UTF-16 limits and mutation', () => {
  function restDouble() {
    return { validate(body) {
      if (!body || typeof body !== 'object' || Array.isArray(body) || Object.keys(body).length !== 1 || !Object.hasOwn(body, 'name') || typeof body.name !== 'string') return false;
      const name = body.name.trim(); return name.length >= 1 && name.length <= 40;
    } };
  }
  assert.equal(semanticChild('MCP-near-miss-rest-v2', restDouble).contract_assertions_pass, true);
  for (const mutation of [
    "const original=implementation.validate;implementation.validate=b=>original(b)&&!(/\\s/.test(b.name.trim()))",
    "const original=implementation.validate;implementation.validate=b=>original(b)&&b.name.length<=40",
    "const original=implementation.validate;implementation.validate=b=>b&&Object.keys(b).length===1&&typeof b.name==='string'?Array.from(b.name.trim()).length>=1&&Array.from(b.name.trim()).length<=40:original(b)",
    "const original=implementation.validate;implementation.validate=b=>{if(typeof b?.name==='string')b.name=b.name.trim();return original(b)}",
  ]) assert.equal(semanticChild('MCP-near-miss-rest-v2', restDouble, mutation).contract_assertions_pass, false);
});

test('revision 4 retains revision 3 and changes only five explicitly revised cases', () => {
  const previousRoot = path.join(root, 'history/cs-2-developer-fixtures-v3');
  const previous = JSON.parse(fs.readFileSync(path.join(previousRoot, 'manifest.json')));
  const current = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  const revised = ['MCP-normal-tools', 'MCP-near-miss-rest', 'LLM-normal-request', 'LLM-normal-stream', 'LLM-boundary-partial'];
  assert.equal(previous.revision, 'cs-2-developer-fixtures-v3'); assert.equal(previous.model_calls, 0);
  assert.equal(current.revision, 'cs-2-developer-fixtures-v4'); assert.equal(current.model_calls, 0);
  assert.equal(current.live_quality, 'not_run'); assert.equal(current.planned_task_runs, 54);
  assert.equal(current.cases.length, 18);
  for (const task of previous.cases) {
    const changed = revised.some(name => task.id === `${name}-v1`);
    if (changed) {
      assert(!current.cases.some(item => item.id === task.id));
      assert(current.cases.some(item => item.id === task.id.replace(/-v1$/, '-v2')));
    } else assert.deepEqual(current.cases.find(item => item.id === task.id), task);
    for (const ref of [...task.expected.source_files.map(ref => ({ ...ref, path: `${task.project}/${ref.path}` })), task.expected.oracle]) {
      const bytes = fs.readFileSync(path.join(root, ref.path));
      assert.equal(bytes.length, ref.bytes); assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), ref.sha256);
    }
  }
  for (const ref of previous.shared) {
    const bytes = fs.readFileSync(path.join(ref.path === 'comparison.json' ? previousRoot : root, ref.path));
    assert.equal(bytes.length, ref.bytes); assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), ref.sha256);
  }
  assert.equal(JSON.parse(fs.readFileSync(path.join(previousRoot, 'comparison.json'))).revision, 'cs-2-developer-comparison-v3');
});

function boundaryDouble() {
  const labels = ['Amber', 'Blue', 'Copper', 'Dove', 'Elm'].map((text, index) => ({ id: `l${index + 1}`, text }));
  const exact = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify(names.sort());
  const error = (id, code, message = 'Synthetic fixture error') => ({ jsonrpc: '2.0', id, error: { code, message } });
  const reply = (id, result) => {
    const packet = { jsonrpc: '2.0', id, result };
    return Buffer.byteLength(JSON.stringify(packet)) > 4096 ? error(id, -32001, 'Reply exceeds 4096 bytes') : packet;
  };
  return { async handle(message, state) {
    const { id, method, params } = message;
    if (id === undefined) {
      if (method === 'notifications/cancelled' && exact(params, ['requestId']) && typeof params.requestId === 'string') state.pending.delete(params.requestId);
      return null;
    }
    if (method === 'initialize') {
      state.initialized = true;
      state.flush = () => { const replies = [...state.pending].map(([key, text]) => reply(key, { text })); state.pending.clear(); return replies; };
      return reply(id, { protocolVersion: '2025-11-25' });
    }
    if (!state.initialized) return error(id, -32000);
    if (method === 'fixture/list_labels') {
      if (!exact(params, []) && !(exact(params, ['cursor']) && ['page-2', 'page-4'].includes(params.cursor))) return error(id, -32602);
      const start = params.cursor ? Number(params.cursor.slice(-1)) : 0;
      return reply(id, { labels: labels.slice(start, start + 2), ...(start < 4 ? { nextCursor: `page-${start + 2}` } : {}) });
    }
    if (['fixture/delay', 'fixture/echo'].includes(method)) {
      if (!exact(params, ['text']) || typeof params.text !== 'string' || Buffer.byteLength(params.text) > 8192) return error(id, -32602);
      if (method === 'fixture/echo') return reply(id, { text: params.text });
      if (state.pending.has(id)) return error(id, -32602);
      if (state.pending.size >= 16) return error(id, -32002);
      state.pending.set(id, params.text); return null;
    }
    return error(id, -32601);
  } };
}

test('revision 3 preserves prior metadata and unchanged task evidence without claiming execution', () => {
  const old = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v2/manifest.json')));
  const current = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v3/manifest.json')));
  assert.equal(old.revision, 'cs-2-developer-fixtures-v2'); assert.equal(old.model_calls, 0);
  assert.equal(current.model_calls, 0); assert.equal(current.live_quality, 'not_run');
  for (const task of old.cases) {
    if (task.id !== 'MCP-boundary-pages-v1') assert.deepEqual(current.cases.find(item => item.id === task.id), task);
    for (const ref of [...task.expected.source_files.map(ref => ({ ...ref, path: `${task.project}/${ref.path}` })), task.expected.oracle]) {
      const bytes = fs.readFileSync(path.join(root, ref.path));
      assert.equal(bytes.length, ref.bytes); assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), ref.sha256);
    }
  }
  const before = JSON.parse(fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v2/comparison.json')));
  const hash = old.shared.find(ref => ref.path === 'comparison.json');
  const bytes = fs.readFileSync(path.join(root, 'history/cs-2-developer-fixtures-v2/comparison.json'));
  assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), hash.sha256);
  assert.equal(before.planned_task_runs, 54);
  assert(current.cases.some(item => item.id === 'MCP-boundary-pages-v2'));
  assert(!current.cases.some(item => item.id === 'MCP-boundary-pages-v1'));
});

test('explicit synthetic pagination and cancellation contract detects independent boundary mutations', () => {
  const good = semanticChild('MCP-boundary-pages-v2', boundaryDouble);
  assert.equal(good.contract_assertions_pass, true, JSON.stringify(good.errors));
  assert.equal(good.adapter_qualified, false); assert.equal(good.observed_task_success, false);
  for (const mutation of [
    "const original=implementation.handle;implementation.handle=(m,s)=>{if(m.method==='notifications/cancelled')s.pending.clear();return original(m,s)}",
    "const original=implementation.handle;implementation.handle=(m,s)=>{if(m.method==='notifications/cancelled')return null;return original(m,s)}",
    "const original=implementation.handle;implementation.handle=async(m,s)=>{const r=await original(m,s);if(m.method==='fixture/list_labels'&&r.result)r.result.nextCursor='page-2';return r}",
    "const original=implementation.handle;implementation.handle=(m,s)=>m.method==='fixture/echo'?{jsonrpc:'2.0',id:m.id,result:{text:m.params.text}}:original(m,s)",
    "const original=implementation.handle;implementation.handle=async(m,s)=>{const r=await original(m,s);if(m.method==='initialize')s.flush=()=>[...s.pending].map(([id,text])=>({jsonrpc:'2.0',id,result:{text}}));return r}",
    "const original=implementation.handle;implementation.handle=async(m,s)=>{if(m.method==='fixture/delay'&&s.pending.has(m.id))s.pending.delete(m.id);return original(m,s)}",
  ]) assert.equal(semanticChild('MCP-boundary-pages-v2', boundaryDouble, mutation).contract_assertions_pass, false, mutation);
});
