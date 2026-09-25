// SPDX-License-Identifier: Apache-2.0
'use strict';
// Assertion library for a separately qualified child adapter. This module does
// not load candidate paths, spawn processes, or accept model-supplied transcripts.
// Local tests invoke it only in disposable children with trusted test functions.
const assert = require('node:assert/strict');
const { load } = require('./developer-oracle.cjs');
const supported = ['LLM-normal-request-v2', 'LLM-normal-stream-v2', 'LLM-boundary-partial-v2', 'LLM-near-miss-parser-v1', 'MCP-normal-tools-v2', 'MCP-normal-resources-v1', 'MCP-boundary-pages-v2', 'MCP-near-miss-rest-v2'];

async function boundaryAssertions(implementation, initial, observe) {
  const labels = JSON.parse(initial.get('labels.json'));
  const packet = (id, method, params) => ({ jsonrpc: '2.0', id, method, params });
  const success = (id, result) => ({ jsonrpc: '2.0', id, result });
  const oversized = id => ({ jsonrpc: '2.0', id, error: { code: -32001, message: 'Reply exceeds 4096 bytes' } });
  async function boot() {
    const state = { initialized: false, pending: new Map() };
    assert.deepEqual(await implementation.handle(packet('init', 'initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'independent', version: '1' } }), state), success('init', { protocolVersion: '2025-11-25' }));
    assert.equal(state.initialized, true); assert.equal(typeof state.flush, 'function');
    return state;
  }
  async function request(state, id, method, params) {
    const reply = await implementation.handle(packet(id, method, params), state);
    if (reply !== null) {
      assert.equal(reply.jsonrpc, '2.0'); assert.equal(reply.id, id);
      assert(Buffer.byteLength(JSON.stringify(reply)) <= 4096);
    }
    return reply;
  }
  const cancel = (state, params) => implementation.handle({ jsonrpc: '2.0', method: 'notifications/cancelled', params }, state);
  await observe('preinitialization and unknown method errors', async () => {
    const untouched = { initialized: false, pending: new Map() };
    assert.equal((await request(untouched, 'early', 'fixture/list_labels', {})).error.code, -32000);
    assert.equal(untouched.pending.size, 0);
    assert.equal((await request(await boot(), 'unknown', 'fixture/unknown', {})).error.code, -32601);
  });
  await observe('exact pagination and opaque cursor rejection', async () => {
    const state = await boot();
    for (const [index, params] of [{}, { cursor: 'page-2' }, { cursor: 'page-4' }].entries()) {
      const result = { labels: labels.slice(index * 2, index * 2 + 2) };
      if (index < 2) result.nextCursor = `page-${index * 2 + 2}`;
      assert.deepEqual(await request(state, `page${index}`, 'fixture/list_labels', params), success(`page${index}`, result));
    }
    for (const params of [null, [], { cursor: '' }, { cursor: null }, { cursor: 2 }, { cursor: 'page-0' }, { cursor: 'page-6' }, { cursor: 'page-2', extra: true }]) assert.equal((await request(state, 'bad', 'fixture/list_labels', params)).error.code, -32602);
  });
  await observe('targeted cancellation and ordered synchronous flush', async () => {
    const state = await boot();
    assert.equal(await request(state, 'slow-a', 'fixture/delay', { text: 'A' }), null);
    assert.equal(await request(state, 'slow-b', 'fixture/delay', { text: 'B' }), null);
    assert.deepEqual([...state.pending], [['slow-a', 'A'], ['slow-b', 'B']]);
    for (const params of [{ requestId: 'unknown' }, {}, { requestId: 5 }, { requestId: 'slow-b', extra: true }]) { assert.equal(await cancel(state, params), null); assert.equal(state.pending.size, 2); }
    assert.equal(await cancel(state, { requestId: 'slow-a' }), null);
    assert.equal(await cancel(state, { requestId: 'slow-a' }), null);
    assert.deepEqual([...state.pending], [['slow-b', 'B']]);
    assert.deepEqual(state.flush(), [success('slow-b', { text: 'B' })]);
    assert.equal(state.pending.size, 0); assert.deepEqual(state.flush(), []);
    assert.equal(await cancel(state, { requestId: 'slow-b' }), null); assert.deepEqual(state.flush(), []);
  });
  await observe('pending bound and invalid requests preserve queued work', async () => {
    const state = await boot();
    for (let index = 0; index < 16; index++) assert.equal(await request(state, `q${index}`, 'fixture/delay', { text: String(index) }), null);
    const before = [...state.pending];
    assert.equal((await request(state, 'overflow', 'fixture/delay', { text: 'extra' })).error.code, -32002);
    assert.equal((await request(state, 'q0', 'fixture/delay', { text: 'replacement' })).error.code, -32602);
    for (const params of [null, { text: 3 }, { text: 'x', extra: true }, { text: 'é'.repeat(4097) }]) assert.equal((await request(state, 'bad', 'fixture/delay', params)).error.code, -32602);
    assert.deepEqual([...state.pending], before);
    await request(state, 'init-again', 'initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'independent', version: '1' } });
    assert.deepEqual([...state.pending], before);
    assert.deepEqual(state.flush(), before.map(([id, text]) => success(id, { text })));
  });
  await observe('full envelope UTF-8 and escaping ceiling for immediate and delayed replies', async () => {
    for (const character of ['x', 'é', '"']) {
      const state = await boot(), id = 'sized';
      const overhead = Buffer.byteLength(JSON.stringify(success(id, { text: '' })));
      const cost = Buffer.byteLength(JSON.stringify(character)) - 2;
      const count = Math.floor((4096 - overhead) / cost);
      const text = character.repeat(count) + 'x'.repeat(4096 - overhead - cost * count);
      assert.equal(Buffer.byteLength(JSON.stringify(success(id, { text }))), 4096);
      assert.deepEqual(await request(state, id, 'fixture/echo', { text }), success(id, { text }));
      assert.deepEqual(await request(state, id, 'fixture/echo', { text: text + 'x' }), oversized(id));
      assert.equal(await request(state, id, 'fixture/delay', { text }), null);
      assert.deepEqual(state.flush(), [success(id, { text })]);
      assert.equal(await request(state, id, 'fixture/delay', { text: text + 'x' }), null);
      assert.deepEqual(state.flush(), [oversized(id)]); assert.equal(state.pending.size, 0);
    }
  });
}

async function evaluate(caseId, implementation) {
  if (!supported.includes(caseId)) throw Error('No executable contract assertions for this case');
  const { initial } = load(caseId);
  const observations = [], errors = [];
  async function observe(name, action) {
    try { await action(); observations.push({ name, passed: true }); }
    catch (error) { observations.push({ name, passed: false }); errors.push({ name, message: String(error.message).slice(0, 2048) }); }
  }
  if (caseId === 'MCP-boundary-pages-v2') {
    await boundaryAssertions(implementation, initial, observe);
  } else if (caseId === 'LLM-normal-request-v2') {
    const identity = JSON.parse(initial.get('provider.json'));
    for (const response of [
      { ok: true, text: 'Independent local answer', usage: { input_tokens: 9, output_tokens: 4 } },
      { ok: true, text: 'No settled usage' },
      { ok: true, text: '', usage: null },
      { ok: true, text: 'Zero usage', usage: { input_tokens: 0, output_tokens: 0 } },
      { ok: false, error: { code: 'SYNTHETIC_RATE_LIMIT', message: 'Synthetic refusal' } },
    ]) await observe(response.ok ? `request ${response.text}` : 'retained error code', async () => {
      const calls = [], transport = { async send(request) { calls.push(structuredClone(request)); return structuredClone(response); } };
      if (response.ok) assert.deepEqual(await implementation.summarize('Local input', transport), { text: response.text, usage: response.usage ?? null });
      else await assert.rejects(() => implementation.summarize('Local input', transport), error => error instanceof Error && error.code === response.error.code && error.message === response.error.message);
      assert.deepEqual(calls, [{ provider: identity.provider, model: identity.model, max_output_tokens: identity.output_limit, input: 'Local input' }]);
    });
    for (const invalid of [null, 5, {}, '', 'x'.repeat(1001), 'é'.repeat(501)]) await observe(`reject invalid input ${typeof invalid}:${String(invalid).length}`, async () => {
      let calls = 0;
      await assert.rejects(async () => implementation.summarize(invalid, { async send() { calls++; return { ok: true, text: 'bad' }; } }), TypeError);
      assert.equal(calls, 0, 'Invalid input reached transport');
    });
    await observe('accept exact UTF-8 input ceiling', async () => {
      let calls = 0;
      await implementation.summarize('é'.repeat(500), { async send() { calls++; return { ok: true, text: 'bounded' }; } });
      assert.equal(calls, 1);
    });
    const malformed = [null, [], {}, { ok: 'true', text: 'x' }, { ok: true }, { ok: true, text: 3 }, { ok: true, text: 'x', extra: true },
      { ok: false }, { ok: false, error: null }, { ok: false, error: { code: '', message: 'x' } },
      { ok: false, error: { code: 'E', message: 3 } }, { ok: false, error: { code: 'E', message: 'x', extra: true } },
      { ok: false, error: { code: 'E', message: 'x' }, text: 'extra' },
      ...[0, [], {}, { input_tokens: 1 }, { input_tokens: -1, output_tokens: 2 },
        { input_tokens: 1, output_tokens: 0.5 }, { input_tokens: '1', output_tokens: 2 },
        { input_tokens: Number.MAX_SAFE_INTEGER + 1, output_tokens: 2 },
        { input_tokens: 1, output_tokens: 2, extra: true }].map(usage => ({ ok: true, text: 'x', usage }))];
    for (const [index, response] of malformed.entries()) await observe(`reject malformed response ${index}`, async () => {
      let calls = 0;
      await assert.rejects(() => implementation.summarize('Local input', { async send() { calls++; return structuredClone(response); } }), TypeError);
      assert.equal(calls, 1, 'Malformed response retried');
    });
  } else if (caseId.startsWith('LLM-') && caseId !== 'LLM-near-miss-parser-v1') {
    const success = Buffer.from('{"type":"delta","text":"Caf"}\n{"type":"delta","text":"é"}\n{"type":"done","usage":{"input_tokens":5,"output_tokens":2}}\n');
    async function collect(parts, signal = new AbortController().signal) {
      return implementation.collect((async function* () { for (const part of parts) yield new Uint8Array(part); })(), signal);
    }
    // Every two-chunk split includes both sides of each multibyte boundary.
    await observe('all byte split boundaries and one-byte fragments', async () => {
      for (let offset = 0; offset <= success.length; offset++) {
        const result = await collect([success.subarray(0, offset), success.subarray(offset)]);
        assert.equal(result.text, 'Café'); assert.equal(result.status, 'completed');
        assert.deepEqual(result.usage, { input_tokens: 5, output_tokens: 2 });
      }
      assert.equal((await collect([...success].map(byte => Buffer.from([byte])))).text, 'Café');
    });
    for (const [name, events, status, usage, code] of [
      ['truncation', [{ type: 'delta', text: 'Part' }], 'incomplete', null],
      ['terminal error', [{ type: 'delta', text: 'Part' }, { type: 'error', code: 'RATE_LIMIT' }], 'error', null, 'RATE_LIMIT'],
      ['missing usage', [{ type: 'done' }], 'completed', null],
    ]) await observe(name, async () => {
      const result = await collect([Buffer.from(events.map(event => JSON.stringify(event) + '\n').join(''))]);
      assert.equal(result.status, status); assert.equal(result.usage, usage);
      assert.equal(result.text, events[0].type === 'delta' ? 'Part' : '');
      if (code) assert.equal(typeof result.error === 'string' ? result.error : result.error?.code, code);
    });
    await observe('pre-cancelled stream is not consumed', async () => {
      const controller = new AbortController(); controller.abort(); let consumed = 0;
      const result = await implementation.collect((async function* () { consumed++; yield success; })(), controller.signal);
      assert.equal(result.status, 'cancelled'); assert.equal(consumed, 0);
    });
    await observe('abort between chunks discards in-flight data and stops reads', async () => {
      const controller = new AbortController(); let reads = 0;
      const chunks = { [Symbol.asyncIterator]() { return this; }, async next() {
        reads++;
        if (reads === 1) return { done: false, value: Buffer.from('{"type":"delta","text":"Part"}\n') };
        if (reads === 2) { controller.abort(); return { done: false, value: Buffer.from('{"type":"delta","text":"discard"}\n') }; }
        return { done: true };
      } };
      const result = await implementation.collect(chunks, controller.signal);
      assert.equal(result.status, 'cancelled'); assert.equal(result.text, 'Part');
      assert.equal(result.usage, null); assert.equal(reads, 2, 'Consumed after cancellation');
    });
    const sized = count => Buffer.from(JSON.stringify({ type: 'delta', text: 'x'.repeat(count) }) + '\n' + JSON.stringify({ type: 'done' }) + '\n');
    const exactTextLength = 4096 - sized(0).length;
    const exact = sized(exactTextLength), oversized = sized(exactTextLength + 1);
    assert.equal(exact.length, 4096); assert.equal(oversized.length, 4097);
    await observe('valid NDJSON at exact byte ceiling is accepted', async () => {
      const result = await collect([exact]);
      assert.equal(result.status, 'completed'); assert.equal(result.text, 'x'.repeat(exactTextLength));
    });
    await observe('exact event ceiling is accepted', async () => {
      const bytes = Buffer.from((JSON.stringify({ type: 'delta', text: 'x' }) + '\n').repeat(31) + '{"type":"done"}\n');
      const result = await collect([bytes]);
      assert.equal(result.status, 'completed'); assert.equal(result.text, 'x'.repeat(31));
    });
    for (const [name, bytes] of [['valid NDJSON byte ceiling', oversized], ['event ceiling', Buffer.from((JSON.stringify({ type: 'delta', text: 'x' }) + '\n').repeat(32) + '{"type":"done"}\n')]]) await observe(name, async () => {
      // The local contract leaves rejection representation open: an exception
      // or an explicit error outcome is acceptable, silent truncation is not.
      let rejected = false;
      try { rejected = (await collect([bytes])).status === 'error'; } catch { rejected = true; }
      assert.equal(rejected, true);
    });
  } else if (caseId === 'LLM-near-miss-parser-v1') {
    for (const [input, expected] of [['  READY  ', 'ready'], ['A  B', 'a  b'], ['ÉCOLE', 'École'], ['', '']]) await observe(`label ${input}`, () => assert.equal(implementation.normalizeLabel(input), expected));
    for (const input of [null, 5, {}, []]) await observe(`nonstring ${typeof input}`, () => assert.throws(() => implementation.normalizeLabel(input), TypeError));
  } else if (caseId === 'MCP-near-miss-rest-v2') {
    for (const [index, [input, expected]] of [
      [{ name: ' Ada ' }, true], [{ name: 'Ada Lovelace' }, true], [{ name: 'A\tB' }, true],
      [{ name: ' x ' }, true], [{ name: 'x'.repeat(40) }, true], [{ name: ' ' + 'x'.repeat(40) + ' ' }, true], [{ name: 'x'.repeat(41) }, false],
      [{ name: 'a' + ' '.repeat(39) + 'b' }, false], [{ name: '😀'.repeat(20) }, true], [{ name: '😀'.repeat(21) }, false],
      [null, false], [[], false], ['Ada', false], [5, false], [{}, false], [{ name: 3 }, false],
      [{ name: '' }, false], [{ name: ' \t\n' }, false], [{ name: 'Ada', role: 'owner' }, false],
    ].entries()) await observe(`REST name validation ${index}`, () => {
      const before = structuredClone(input);
      assert.equal(implementation.validate(input), expected); assert.deepEqual(input, before, 'Input mutated');
    });
  } else {
    const state = {}; let sequence = 0;
    async function request(method, params) {
      const id = ++sequence, result = await implementation.handle({ jsonrpc: '2.0', id, method, params }, state);
      assert.equal(result?.id, id); assert.equal(result?.jsonrpc, '2.0');
      assert.notEqual(Object.hasOwn(result, 'result'), Object.hasOwn(result, 'error'));
      return result;
    }
    await observe('initialization identity', async () => {
      const response = await request('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'independent-fixture', version: '1' } });
      assert.equal(response.result?.protocolVersion, '2025-11-25');
      assert.equal(await implementation.handle({ jsonrpc: '2.0', method: 'notifications/initialized' }, state), null);
    });
    await observe('unknown method error', async () => assert.equal((await request('fixture/unknown', {})).error?.code, -32601));
    if (caseId === 'MCP-normal-tools-v2') {
      await observe('exact tool identities and declared input schemas', async () => {
        const tools = (await request('tools/list', {})).result?.tools;
        assert.deepEqual(tools.map(tool => tool.name).sort(), ['count_labels', 'lookup_label']);
        const schemas = {
          lookup_label: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'], additionalProperties: false },
          count_labels: { type: 'object', properties: {}, additionalProperties: false },
        };
        for (const tool of tools) assert.deepEqual(tool.inputSchema, schemas[tool.name]);
      });
      const labels = JSON.parse(initial.get('labels.json'));
      const vectors = [...labels.map(label => ['lookup_label', { id: label.id }, label.text]), ['count_labels', {}, String(labels.length)]];
      for (const [name, arguments_, expected] of vectors) await observe(`tool ${name} ${JSON.stringify(arguments_)}`, async () => {
        const result = (await request('tools/call', { name, arguments: arguments_ })).result;
        assert.equal(result?.isError === true, false);
        assert.deepEqual(result?.content, [{ type: 'text', text: expected }]);
      });
      await observe('unknown record id returns a tool error', async () => {
        const response = await request('tools/call', { name: 'lookup_label', arguments: { id: 'missing-record' } });
        assert.equal(response.result?.isError, true);
        assert(Array.isArray(response.result.content) && response.result.content.length > 0);
        for (const entry of response.result.content) { assert.equal(entry.type, 'text'); assert.equal(typeof entry.text, 'string'); assert(entry.text.length > 0); }
      });
      for (const params of [{ name: 'unknown', arguments: {} }, { name: 'lookup_label', arguments: {} }, { name: 'lookup_label', arguments: { id: 3 } }, { name: 'lookup_label', arguments: { id: 'l3', extra: true } }, { name: 'count_labels', arguments: { extra: true } }]) await observe(`reject tool ${JSON.stringify(params)}`, async () => {
        const response = await request('tools/call', params);
        assert(response.error || response.result?.isError === true, 'Invalid tool request succeeded');
      });
    } else {
      const resources = JSON.parse(initial.get('resources.json'));
      await observe('exact resource identities', async () => {
        const actual = (await request('resources/list', {})).result?.resources;
        assert.deepEqual(actual.map(({ uri, name, mimeType }) => ({ uri, name, mimeType })), resources.map(({ uri, name, mimeType }) => ({ uri, name, mimeType })));
      });
      for (const resource of resources) await observe(`read ${resource.uri}`, async () => assert.deepEqual((await request('resources/read', { uri: resource.uri })).result?.contents, [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }]));
      await observe('unknown resource rejected', async () => assert((await request('resources/read', { uri: 'fixture://missing' })).error));
    }
  }
  return { case_id: caseId, contract_assertions_pass: errors.length === 0, observations, errors,
    adapter_qualified: false, observed_task_success: false,
    not_run: ['Process isolation and lifecycle qualification', 'MCP stdio transport and full schema conformance', 'Selected SDK and live provider compatibility', 'Human usefulness and preservation review'] };
}
module.exports = { evaluate, supported };
