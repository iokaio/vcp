// SPDX-License-Identifier: Apache-2.0
'use strict';
// Parent-side functional grading of CS-2 final-workspace artifacts. The parent holds
// every expected value. Grader-authored wrappers run beside the candidate inside the
// qualified Windows Node fixture adapter and only report what the candidate returned
// or requested; nothing the candidate reports about itself is trusted evidence.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { load } = require('./developer-oracle.cjs');
const { checkResponse, checkInteractiveReceipt, decodeFrame } = require('./node-fixture-protocol.cjs');
const { openInteractive } = require('./node-fixture-session.cjs');
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

// ---------------------------------------------------------------------------
// Trusted wrappers. Each is staged as candidate.cjs next to the subject module.
const outcome = `async function outcome(action) {
  try { return { ok: true, value: await action() }; }
  catch (error) { return { ok: false, name: String(error?.name ?? typeof error).slice(0, 64), message: String(error?.message ?? error).slice(0, 512), code: error?.code === undefined ? null : String(error.code).slice(0, 128) }; }
}`;
const wrappers = {
  // Pure function calls; args are returned after the call to expose mutation.
  function: (subject, exported) => `'use strict';
const subject = require('./${subject}');
${outcome}
exports.compute = async input => {
  const results = [];
  for (const args of input.calls) results.push({ outcome: await outcome(() => subject[${JSON.stringify(exported)}](...args)), args_after: args });
  return results;
};
`,
  // Scripted in-memory MCP-shaped session with the caller-owned state object.
  mcpScript: (subject, exported) => `'use strict';
const subject = require('./${subject}');
${outcome}
exports.compute = async input => {
  const results = [];
  let state = { initialized: false, pending: new Map() };
  for (const step of input.steps) {
    if (step.type === 'reset') { state = { initialized: false, pending: new Map() }; results.push(null); }
    else if (step.type === 'flush') results.push(typeof state.flush === 'function' ? await outcome(() => state.flush()) : { ok: false, name: 'MissingFlush', message: 'state.flush is not installed', code: null });
    else results.push(await outcome(() => subject[${JSON.stringify(exported)}](step.message, state)));
  }
  return results;
};
`,
  // Streams supplied as byte partitions; the parent only sees returned outcomes.
  streamBatch: (subject, exported) => `'use strict';
const subject = require('./${subject}');
${outcome}
exports.compute = async input => {
  const results = [];
  for (const run of input.runs) {
    const controller = new AbortController();
    if (run.abort_before) controller.abort();
    const parts = run.parts.map(part => new Uint8Array(Buffer.from(part, 'base64')));
    const chunks = (async function* () { for (const part of parts) yield part; })();
    results.push(await outcome(() => subject[${JSON.stringify(exported)}](chunks, controller.signal)));
  }
  return results;
};
`,
  // Interactive stdio server: each parent frame is one JSON-RPC message. Replies are
  // frames; notifications produce none, exactly like newline-delimited stdio.
  mcpStdio: (subject, exported) => `'use strict';
const subject = require('./${subject}');
exports.interact = async channel => {
  const state = {};
  for (let message; (message = await channel.receive()) !== null;) {
    let reply;
    try { reply = await subject[${JSON.stringify(exported)}](message, state); }
    catch (error) { channel.send({ wrapper_error: String(error?.message ?? error).slice(0, 512) }); continue; }
    if (reply !== null && reply !== undefined) channel.send(reply);
  }
};
`,
  // Parent-owned transport: every send() becomes a frame the parent must answer.
  llmTransport: (subject, exported) => `'use strict';
const subject = require('./${subject}');
${outcome}
exports.interact = async channel => {
  for (let op; (op = await channel.receive()) !== null;) {
    const transport = { async send(request) { channel.send({ type: 'call', request }); const reply = await channel.receive(); return reply.response; } };
    channel.send({ type: 'result', outcome: await outcome(() => subject[${JSON.stringify(exported)}](op.input, transport)) });
  }
};
`,
  // Parent-owned async iterator: every read is a frame; the parent decides the chunk
  // and may abort the signal while a read is outstanding.
  llmIterator: (subject, exported) => `'use strict';
const subject = require('./${subject}');
${outcome}
exports.interact = async channel => {
  for (let op; (op = await channel.receive()) !== null;) {
    const controller = new AbortController();
    if (op.abort_before) controller.abort();
    const chunks = { [Symbol.asyncIterator]() { return this; }, async next() {
      channel.send({ type: 'read' });
      const reply = await channel.receive();
      if (reply.abort) controller.abort();
      return reply.done ? { done: true, value: undefined } : { done: false, value: new Uint8Array(Buffer.from(reply.chunk, 'base64')) };
    } };
    channel.send({ type: 'result', outcome: await outcome(() => subject[${JSON.stringify(exported)}](chunks, controller.signal)) });
  }
};
`,
};

// ---------------------------------------------------------------------------
// Probe helpers. `observe` records each named expectation independently.
const ok = value => ({ ok: true, value });
const threw = name => result => { assert.equal(result.ok, false, 'Expected an exception'); assert.equal(result.name, name); };
const b64 = value => Buffer.from(value).toString('base64');
const ndjson = events => events.map(event => JSON.stringify(event) + '\n').join('');
const packet = (id, method, params) => ({ jsonrpc: '2.0', id, method, params });
const success = (id, result) => ({ jsonrpc: '2.0', id, result });
const initParams = { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'independent-grader', version: '1' } };
function initializeShape(result, capabilities) {
  assert.equal(result.protocolVersion, '2025-11-25');
  assert.deepEqual(result.capabilities, capabilities);
  assert.equal(typeof result.serverInfo?.name, 'string'); assert(result.serverInfo.name.length > 0);
  assert.equal(typeof result.serverInfo?.version, 'string'); assert(result.serverInfo.version.length > 0);
  assert.deepEqual(Object.keys(result).sort(), ['capabilities', 'protocolVersion', 'serverInfo']);
}
async function functionCalls(run, calls, check) {
  const results = await run({ calls: calls.map(call => call.args) });
  assert.equal(results.length, calls.length);
  results.forEach((result, index) => check(result, calls[index], index));
}

// Parent-held expectations per case. Each group returns a list of named checks run
// through the executor; failures in one group do not hide others.
const probes = {
  'UI-normal-results-v2': { wrapper: 'function', groups: ({ initial }) => {
    const items = JSON.parse(initial.get('items.json'));
    return [['filterItems membership, order and immutability', async run => {
      const vectors = [['all', ['a', 'b', 'c']], ['tree', ['a', 'b']], ['flower', ['c']], ['unknown', []], ['', []]];
      await functionCalls(run, vectors.map(([category]) => ({ args: [items, category] })), (result, call, index) => {
        assert.deepEqual(result.outcome, ok(vectors[index][1]));
        assert.deepEqual(result.args_after, [items, vectors[index][0]], 'Items mutated');
      });
    }]];
  } },
  'UI-boundary-states-v2': { wrapper: 'function', groups: ({ oracle }) => [['declared and undeclared transitions', async run => {
    const vectors = oracle.functional_vectors;
    await functionCalls(run, vectors.map(vector => ({ args: [vector.state, vector.event] })), (result, call, index) => assert.deepEqual(result.outcome, ok(vectors[index].next)));
  }]] },
  'UI-near-miss-parser-v2': { wrapper: 'function', groups: () => [['decimal digit strings and rejection', async run => {
    const accepted = [['0', 0], ['007', 7], ['42', 42], ['9007199254740991', 9007199254740991]];
    const rejected = ['', '-1', '1.5', '3x', ' 1', '1 ', '+1', '9007199254740992', '١٢'];
    const nonstrings = [null, 5, [], {}];
    const calls = [...accepted.map(([input]) => ({ args: [input] })), ...[...rejected, ...nonstrings].map(input => ({ args: [input] }))];
    await functionCalls(run, calls, (result, call, index) => {
      if (index < accepted.length) assert.deepEqual(result.outcome, ok(accepted[index][1]));
      else threw('TypeError')(result.outcome);
    });
  }]] },
  'LLM-near-miss-parser-v2': { wrapper: 'function', groups: () => [['label normalization', async run => {
    const accepted = [['  READY  ', 'ready'], ['A  B', 'a  b'], ['ÉCOLE', 'École'], ['', ''], ['\tMiXeD\n', 'mixed']];
    const nonstrings = [null, 5, {}, []];
    await functionCalls(run, [...accepted.map(([input]) => ({ args: [input] })), ...nonstrings.map(input => ({ args: [input] }))], (result, call, index) => {
      if (index < accepted.length) assert.deepEqual(result.outcome, ok(accepted[index][1]));
      else threw('TypeError')(result.outcome);
    });
  }]] },
  'MCP-near-miss-rest-v3': { wrapper: 'function', groups: () => [['REST name validation without mutation', async run => {
    const vectors = [
      [{ name: ' Ada ' }, true], [{ name: 'Ada Lovelace' }, true], [{ name: 'A\tB' }, true],
      [{ name: ' x ' }, true], [{ name: 'x'.repeat(40) }, true], [{ name: ' ' + 'x'.repeat(40) + ' ' }, true], [{ name: 'x'.repeat(41) }, false],
      [{ name: 'a' + ' '.repeat(39) + 'b' }, false], [{ name: '😀'.repeat(20) }, true], [{ name: '😀'.repeat(21) }, false],
      [null, false], [[], false], ['Ada', false], [5, false], [{}, false], [{ name: 3 }, false],
      [{ name: '' }, false], [{ name: ' \t\n' }, false], [{ name: 'Ada', role: 'owner' }, false],
    ];
    await functionCalls(run, vectors.map(([input]) => ({ args: [input] })), (result, call, index) => {
      assert.deepEqual(result.outcome, ok(vectors[index][1]));
      assert.deepEqual(result.args_after, [vectors[index][0]], 'Input mutated');
    });
  }]] },
  'MCP-boundary-pages-v3': { wrapper: 'mcpScript', groups: ({ initial }) => {
    const labels = JSON.parse(initial.get('labels.json'));
    const oversized = id => ({ jsonrpc: '2.0', id, error: { code: -32001, message: 'Reply exceeds 4096 bytes' } });
    // Builds one scripted session; expectations are functions over each step result.
    function script(build) {
      const steps = [], checks = [];
      const api = {
        reset() { steps.push({ type: 'reset' }); checks.push(null); },
        send(message, check) { steps.push({ type: 'message', message }); checks.push(check); },
        flush(check) { steps.push({ type: 'flush' }); checks.push(check); },
      };
      build(api);
      return async run => {
        const results = await run({ steps });
        assert.equal(results.length, steps.length);
        results.forEach((result, index) => {
          if (!checks[index]) return;
          if (result && result.ok && result.value && typeof result.value === 'object' && !Array.isArray(result.value) && 'jsonrpc' in result.value) {
            assert(Buffer.byteLength(JSON.stringify(result.value)) <= 4096, 'Reply exceeds the declared ceiling');
          }
          checks[index](result, steps[index]);
        });
      };
    }
    const reply = expected => result => assert.deepEqual(result, ok(expected));
    const nothing = result => assert.deepEqual(result, ok(null));
    const code = (id, value) => result => { assert.equal(result.ok, true); assert.equal(result.value?.jsonrpc, '2.0'); assert.equal(result.value?.id, id); assert.equal(result.value?.error?.code, value); };
    const init = (api, id = 'init') => api.send(packet(id, 'initialize', initParams), result => {
      assert.equal(result.ok, true); assert.equal(result.value.id, id); initializeShape(result.value.result, {});
    });
    const cancel = (api, params) => api.send({ jsonrpc: '2.0', method: 'notifications/cancelled', params }, nothing);
    return [
      ['initialization shape, preinitialization and unknown method errors', script(api => {
        api.reset();
        api.send(packet('early', 'fixture/delay', { text: 'queued too early' }), code('early', -32000));
        api.send(packet('early-list', 'fixture/list_labels', {}), code('early-list', -32000));
        init(api);
        api.flush(reply([]));
        api.send(packet('unknown', 'fixture/unknown', {}), code('unknown', -32601));
      })],
      ['exact pagination and opaque cursor rejection', script(api => {
        api.reset(); init(api);
        for (const [index, params] of [{}, { cursor: 'page-2' }, { cursor: 'page-4' }].entries()) {
          const result = { labels: labels.slice(index * 2, index * 2 + 2) };
          if (index < 2) result.nextCursor = `page-${index * 2 + 2}`;
          api.send(packet(`page${index}`, 'fixture/list_labels', params), reply(success(`page${index}`, result)));
        }
        for (const params of [null, [], { cursor: '' }, { cursor: null }, { cursor: 2 }, { cursor: 'page-0' }, { cursor: 'page-6' }, { cursor: 'page-2', extra: true }]) {
          api.send(packet('bad', 'fixture/list_labels', params), code('bad', -32602));
        }
      })],
      ['targeted cancellation and ordered synchronous flush', script(api => {
        api.reset(); init(api);
        api.send(packet('slow-a', 'fixture/delay', { text: 'A' }), nothing);
        api.send(packet('slow-b', 'fixture/delay', { text: 'B' }), nothing);
        for (const params of [{ requestId: 'unknown' }, {}, { requestId: 5 }, { requestId: 'slow-b', extra: true }]) cancel(api, params);
        cancel(api, { requestId: 'slow-a' }); cancel(api, { requestId: 'slow-a' });
        api.flush(reply([success('slow-b', { text: 'B' })]));
        api.flush(reply([]));
        cancel(api, { requestId: 'slow-b' });
        api.flush(reply([]));
      })],
      ['pending bound and invalid requests preserve queued work', script(api => {
        api.reset(); init(api);
        for (let index = 0; index < 16; index++) api.send(packet(`q${index}`, 'fixture/delay', { text: String(index) }), nothing);
        api.send(packet('overflow', 'fixture/delay', { text: 'extra' }), code('overflow', -32002));
        api.send(packet('q0', 'fixture/delay', { text: 'replacement' }), code('q0', -32602));
        for (const params of [null, { text: 3 }, { text: 'x', extra: true }, { text: 'é'.repeat(4097) }]) api.send(packet('bad', 'fixture/delay', params), code('bad', -32602));
        init(api, 'init-again');
        api.flush(reply(Array.from({ length: 16 }, (_, index) => success(`q${index}`, { text: String(index) }))));
      })],
      ...['x', 'é', '"'].map(character => [`full envelope ceiling for ${JSON.stringify(character)}`, script(api => {
        const id = 'sized';
        const overhead = Buffer.byteLength(JSON.stringify(success(id, { text: '' })));
        const cost = Buffer.byteLength(JSON.stringify(character)) - 2;
        const count = Math.floor((4096 - overhead) / cost);
        const text = character.repeat(count) + 'x'.repeat(4096 - overhead - cost * count);
        assert.equal(Buffer.byteLength(JSON.stringify(success(id, { text }))), 4096);
        api.reset(); init(api);
        api.send(packet(id, 'fixture/echo', { text }), reply(success(id, { text })));
        api.send(packet(id, 'fixture/echo', { text: text + 'x' }), reply(oversized(id)));
        api.send(packet(id, 'fixture/delay', { text }), nothing);
        api.flush(reply([success(id, { text })]));
        api.send(packet(id, 'fixture/delay', { text: text + 'x' }), nothing);
        api.flush(reply([oversized(id)]));
        api.flush(reply([]));
      })]),
    ];
  } },
  'LLM-normal-stream-v3': { wrapper: 'streamBatch', groups: () => {
    const successBytes = Buffer.from(ndjson([{ type: 'delta', text: 'Caf' }, { type: 'delta', text: 'é' }, { type: 'done', usage: { input_tokens: 5, output_tokens: 2 } }]));
    const batch = (runs, check) => async run => { const results = await run({ runs }); assert.equal(results.length, runs.length); results.forEach((result, index) => check(result, index)); };
    const completed = (text, usage) => result => { assert.equal(result.ok, true); assert.equal(result.value.text, text); assert.equal(result.value.status, 'completed'); assert.deepEqual(result.value.usage, usage); };
    const sized = count => Buffer.from(ndjson([{ type: 'delta', text: 'x'.repeat(count) }, { type: 'done' }]));
    const exactLength = 4096 - sized(0).length;
    const rejected = result => assert(!result.ok || result.value?.status === 'error', 'Oversized valid input was not rejected');
    return [
      ['all two-chunk byte splits and one-byte fragments', batch([
        ...Array.from({ length: successBytes.length + 1 }, (_, offset) => ({ parts: [b64(successBytes.subarray(0, offset)), b64(successBytes.subarray(offset))] })),
        { parts: [...successBytes].map(byte => b64(Buffer.from([byte]))) },
      ], completed('Café', { input_tokens: 5, output_tokens: 2 }))],
      ['truncation, terminal error and missing usage', batch([
        { parts: [b64(ndjson([{ type: 'delta', text: 'Part' }]))] },
        { parts: [b64(ndjson([{ type: 'delta', text: 'Part' }, { type: 'error', code: 'RATE_LIMIT' }]))] },
        { parts: [b64(ndjson([{ type: 'done' }]))] },
      ], (result, index) => {
        assert.equal(result.ok, true);
        assert.equal(result.value.status, ['incomplete', 'error', 'completed'][index]);
        assert.equal(result.value.text, index < 2 ? 'Part' : '');
        assert.equal(result.value.usage, null);
        if (index === 1) assert.equal(typeof result.value.error === 'string' ? result.value.error : result.value.error?.code, 'RATE_LIMIT');
      })],
      ['pre-cancelled stream reports cancelled', batch([{ parts: [b64(successBytes)], abort_before: true }], result => { assert.equal(result.ok, true); assert.equal(result.value.status, 'cancelled'); })],
      ['exact byte and event ceilings are accepted', batch([
        { parts: [b64(sized(exactLength))] },
        { parts: [b64(ndjson([...Array(31).fill({ type: 'delta', text: 'x' }), { type: 'done' }]))] },
      ], (result, index) => completed(index === 0 ? 'x'.repeat(exactLength) : 'x'.repeat(31), null)(result))],
      ['oversized byte and event inputs are rejected', batch([
        { parts: [b64(sized(exactLength + 1))] },
        { parts: [b64(ndjson([...Array(32).fill({ type: 'delta', text: 'x' }), { type: 'done' }]))] },
      ], rejected)],
    ];
  } },
  'MCP-normal-tools-v3': { wrapper: 'mcpStdio', groups: ({ initial }) => [['stdio tools session', async session => {
    const labels = JSON.parse(initial.get('labels.json'));
    let sequence = 0;
    async function request(method, params) {
      const id = ++sequence;
      session.send({ jsonrpc: '2.0', id, method, params });
      const reply = await session.receive();
      assert.equal(reply?.jsonrpc, '2.0'); assert.equal(reply.id, id);
      assert.notEqual(Object.hasOwn(reply, 'result'), Object.hasOwn(reply, 'error'));
      return reply;
    }
    initializeShape((await request('initialize', initParams)).result, { tools: {} });
    session.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
    const tools = (await request('tools/list', {})).result?.tools;
    assert.deepEqual(tools.map(tool => tool.name).sort(), ['count_labels', 'lookup_label']);
    const schemas = {
      lookup_label: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'], additionalProperties: false },
      count_labels: { type: 'object', properties: {}, additionalProperties: false },
    };
    for (const tool of tools) assert.deepEqual(tool.inputSchema, schemas[tool.name]);
    for (const [name, arguments_, expected] of [...labels.map(label => ['lookup_label', { id: label.id }, label.text]), ['count_labels', {}, String(labels.length)]]) {
      const result = (await request('tools/call', { name, arguments: arguments_ })).result;
      assert.notEqual(result?.isError, true); assert.deepEqual(result?.content, [{ type: 'text', text: expected }]);
    }
    const missing = (await request('tools/call', { name: 'lookup_label', arguments: { id: 'missing-record' } })).result;
    assert.equal(missing?.isError, true); assert(Array.isArray(missing.content) && missing.content.length > 0);
    for (const entry of missing.content) { assert.equal(entry.type, 'text'); assert(typeof entry.text === 'string' && entry.text.length > 0); }
    for (const params of [{ name: 'unknown', arguments: {} }, { name: 'lookup_label', arguments: {} }, { name: 'lookup_label', arguments: { id: 3 } }, { name: 'lookup_label', arguments: { id: 'l3', extra: true } }, { name: 'count_labels', arguments: { extra: true } }]) {
      const response = await request('tools/call', params);
      assert(response.error || response.result?.isError === true, `Invalid tool request succeeded: ${JSON.stringify(params)}`);
    }
    assert.equal((await request('fixture/unknown', {})).error?.code, -32601);
  }]] },
  'MCP-normal-resources-v2': { wrapper: 'mcpStdio', groups: ({ initial }) => [['stdio resources session', async session => {
    const resources = JSON.parse(initial.get('resources.json'));
    let sequence = 0;
    async function request(method, params) {
      const id = ++sequence;
      session.send({ jsonrpc: '2.0', id, method, params });
      const reply = await session.receive();
      assert.equal(reply?.jsonrpc, '2.0'); assert.equal(reply.id, id);
      assert.notEqual(Object.hasOwn(reply, 'result'), Object.hasOwn(reply, 'error'));
      return reply;
    }
    initializeShape((await request('initialize', initParams)).result, { resources: {} });
    session.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
    const listed = (await request('resources/list', {})).result?.resources;
    assert.deepEqual(listed.map(({ uri, name, mimeType }) => ({ uri, name, mimeType })), resources.map(({ uri, name, mimeType }) => ({ uri, name, mimeType })));
    for (const resource of resources) {
      assert.deepEqual((await request('resources/read', { uri: resource.uri })).result?.contents, [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }]);
    }
    for (const uri of ['fixture://missing', 'file:///C:/Windows/win.ini', '../resources.json']) assert((await request('resources/read', { uri })).error, `Unknown resource read succeeded: ${uri}`);
    assert.equal((await request('fixture/unknown', {})).error?.code, -32601);
  }]] },
  'LLM-normal-request-v3': { wrapper: 'llmTransport', groups: ({ initial }) => {
    const identity = JSON.parse(initial.get('provider.json'));
    const expectedCall = input => ({ provider: identity.provider, model: identity.model, max_output_tokens: identity.output_limit, input });
    // One op: the parent answers exactly one transport call, then requires the result.
    async function summarize(session, input, response, { calls = 1 } = {}) {
      session.send({ input });
      let frame = await session.receive();
      for (let index = 0; index < calls; index++) {
        assert.equal(frame.type, 'call', 'Expected one transport call');
        assert.deepEqual(frame.request, expectedCall(input));
        session.send({ response });
        frame = await session.receive();
      }
      assert.equal(frame.type, 'result', calls ? 'Transport retried or called again' : 'Invalid input reached the transport');
      return frame.outcome;
    }
    return [
      ['typed successes, retained errors and exact single calls', async session => {
        for (const response of [
          { ok: true, text: 'Independent local answer', usage: { input_tokens: 9, output_tokens: 4 } },
          { ok: true, text: 'No settled usage' },
          { ok: true, text: '', usage: null },
          { ok: true, text: 'Zero usage', usage: { input_tokens: 0, output_tokens: 0 } },
        ]) assert.deepEqual(await summarize(session, 'Local input', response), ok({ text: response.text, usage: response.usage ?? null }));
        const failed = await summarize(session, 'Local input', { ok: false, error: { code: 'SYNTHETIC_RATE_LIMIT', message: 'Synthetic refusal' } });
        assert.equal(failed.ok, false); assert.equal(failed.code, 'SYNTHETIC_RATE_LIMIT'); assert.equal(failed.message, 'Synthetic refusal');
        assert.notEqual(failed.name, 'TypeError');
      }],
      ['invalid input rejected before any transport call', async session => {
        for (const input of [null, 5, {}, '', 'x'.repeat(1001), 'é'.repeat(501)]) threw('TypeError')(await summarize(session, input, null, { calls: 0 }));
        assert.equal((await summarize(session, 'é'.repeat(500), { ok: true, text: 'bounded' })).ok, true);
      }],
      ['malformed responses rejected without retry', async session => {
        const malformed = [null, [], {}, { ok: 'true', text: 'x' }, { ok: true }, { ok: true, text: 3 }, { ok: true, text: 'x', extra: true },
          { ok: false }, { ok: false, error: null }, { ok: false, error: { code: '', message: 'x' } },
          { ok: false, error: { code: 'E', message: 3 } }, { ok: false, error: { code: 'E', message: 'x', extra: true } },
          { ok: false, error: { code: 'E', message: 'x' }, text: 'extra' },
          ...[0, [], {}, { input_tokens: 1 }, { input_tokens: -1, output_tokens: 2 },
            { input_tokens: 1, output_tokens: 0.5 }, { input_tokens: '1', output_tokens: 2 },
            { input_tokens: Number.MAX_SAFE_INTEGER + 1, output_tokens: 2 },
            { input_tokens: 1, output_tokens: 2, extra: true }].map(usage => ({ ok: true, text: 'x', usage }))];
        for (const response of malformed) threw('TypeError')(await summarize(session, 'Local input', response));
      }],
    ];
  } },
  'LLM-boundary-partial-v3': { wrapper: 'llmIterator', groups: () => {
    // The parent serves reads; `plan` is the answer to each successive read.
    async function collect(session, plan, { abortBefore = false } = {}) {
      session.send({ abort_before: abortBefore });
      let reads = 0;
      for (;;) {
        const frame = await session.receive();
        if (frame.type === 'result') return { outcome: frame.outcome, reads };
        assert.equal(frame.type, 'read');
        assert(reads < plan.length, 'Read after the stream ended or after cancellation');
        session.send(plan[reads++]);
      }
    }
    const chunk = text => ({ chunk: b64(text) });
    const sized = count => ndjson([{ type: 'delta', text: 'x'.repeat(count) }, { type: 'done' }]);
    const exactLength = 4096 - Buffer.byteLength(sized(0));
    return [
      ['truncation, error and missing usage over parent reads', async session => {
        const incomplete = await collect(session, [chunk(ndjson([{ type: 'delta', text: 'Part' }])), { done: true }]);
        assert.equal(incomplete.outcome.ok, true); assert.equal(incomplete.outcome.value.status, 'incomplete'); assert.equal(incomplete.outcome.value.text, 'Part'); assert.equal(incomplete.outcome.value.usage, null);
        const errored = await collect(session, [chunk(ndjson([{ type: 'delta', text: 'Part' }, { type: 'error', code: 'RATE_LIMIT' }])), { done: true }]);
        assert.equal(errored.outcome.value.status, 'error'); assert.equal(errored.outcome.value.text, 'Part');
        assert.equal(typeof errored.outcome.value.error === 'string' ? errored.outcome.value.error : errored.outcome.value.error?.code, 'RATE_LIMIT');
        const bare = await collect(session, [chunk(ndjson([{ type: 'done' }])), { done: true }]);
        assert.equal(bare.outcome.value.status, 'completed'); assert.equal(bare.outcome.value.usage, null);
      }],
      ['pre-cancelled stream performs no read', async session => {
        const result = await collect(session, [], { abortBefore: true });
        assert.equal(result.reads, 0); assert.equal(result.outcome.ok, true); assert.equal(result.outcome.value.status, 'cancelled');
      }],
      ['abort during a read discards that chunk and stops reading', async session => {
        const result = await collect(session, [chunk(ndjson([{ type: 'delta', text: 'Part' }])), { abort: true, ...chunk(ndjson([{ type: 'delta', text: 'discard' }])) }]);
        assert.equal(result.reads, 2); assert.equal(result.outcome.ok, true);
        assert.equal(result.outcome.value.status, 'cancelled'); assert.equal(result.outcome.value.text, 'Part'); assert.equal(result.outcome.value.usage, null);
      }],
      ['byte and event ceilings over parent reads', async session => {
        const exact = await collect(session, [chunk(sized(exactLength)), { done: true }]);
        assert.equal(exact.outcome.value.status, 'completed'); assert.equal(exact.outcome.value.text, 'x'.repeat(exactLength));
        const events = await collect(session, [chunk(ndjson([...Array(31).fill({ type: 'delta', text: 'x' }), { type: 'done' }])), { done: true }]);
        assert.equal(events.outcome.value.status, 'completed');
        for (const text of [sized(exactLength + 1), ndjson([...Array(32).fill({ type: 'delta', text: 'x' }), { type: 'done' }])]) {
          const result = await collect(session, [chunk(text), { done: true }]);
          assert(!result.outcome.ok || result.outcome.value.status === 'error', 'Oversized valid input was not rejected');
        }
      }],
    ];
  } },
};

// ---------------------------------------------------------------------------
// Staging: only loadable project modules and data reach the adapter. `.js` sources
// are staged as `.cjs` (the adapter accepts only .cjs/.json names).
function stagedName(name) {
  return name.replace(/\.js$/, '.cjs');
}
function inventory(final, grading, wrapperKind) {
  const files = [];
  for (const [name, content] of final) {
    if (name === 'package.json' || name.includes('/') || !/\.(c?js|json)$/.test(name)) continue;
    const staged = stagedName(name);
    if (!/^[a-z][a-z0-9-]*\.(cjs|json)$/.test(staged) || ['candidate.cjs', 'bootstrap.cjs'].includes(staged)) continue;
    files.push({ name: staged, content: Buffer.from(content) });
  }
  if (!files.some(file => file.name === stagedName(grading.subject))) throw Error('Graded subject is absent from the final workspace');
  files.push({ name: 'candidate.cjs', content: Buffer.from(wrappers[wrapperKind](stagedName(grading.subject), grading.export)) });
  if (files.length > 16) throw Error('Adapter inventory exceeds sixteen files');
  return files;
}

// Qualified executor: the Windows AppContainer Node fixture adapter.
function appContainerExecutor({ node, nodeSha256, timeoutMs = 20000, memoryBytes = 268435456, idleMs = 5000 }) {
  if (process.platform !== 'win32') throw Error('The qualified executor requires native Windows');
  if (sha(fs.readFileSync(node)) !== nodeSha256) throw Error('Pinned Node identity mismatch');
  const runner = path.join(__dirname, 'node-fixture-runner.ps1');
  function stage(files) {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-developer-grade-'));
    const entries = files.map(file => {
      const target = path.join(directory, file.name);
      fs.writeFileSync(target, file.content);
      return { name: file.name, path: target, sha256: sha(file.content) };
    });
    return { directory, entries };
  }
  const base = bootstrap => ({ schema: 1, node, node_sha256: nodeSha256, bootstrap_sha256: sha(fs.readFileSync(path.join(__dirname, bootstrap))), memory_bytes: memoryBytes, timeout_ms: timeoutMs });
  return {
    name: 'windows-appcontainer-node-fixture',
    qualified: true,
    async single(files, input) {
      const { directory, entries } = stage(files);
      try {
        const id = crypto.randomBytes(16).toString('hex');
        const config = { ...base('node-fixture-bootstrap.cjs'), output_limit: 65536, input_base64: Buffer.from(JSON.stringify({ id, input })).toString('base64'), files: entries };
        const configPath = path.join(directory, 'config.json');
        fs.writeFileSync(configPath, JSON.stringify(config));
        const child = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner, '-Config', configPath], {
          cwd: repository, encoding: 'utf8', timeout: timeoutMs + 30000, maxBuffer: 1048576, windowsHide: true });
        if (child.error || child.status !== 0) throw Error('Adapter run failed before a receipt');
        const receipt = JSON.parse(child.stdout);
        const text = Buffer.from(receipt.result.stdout_base64, 'base64').toString('utf8');
        let response;
        try { response = JSON.parse(text); } catch { throw Error(`Candidate produced no valid response (termination ${receipt.result.termination})`); }
        // Validate the envelope, identity and containment with the qualified check;
        // the value itself is judged by the caller's parent-held expectations.
        checkResponse(receipt, id, response.result);
        return response.result;
      } finally { fs.rmSync(directory, { recursive: true, force: true }); }
    },
    async interactive(files, drive) {
      const { directory, entries } = stage(files);
      try {
        const config = { ...base('node-fixture-interactive-bootstrap.cjs'), mode: 'interactive', output_limit: 65536,
          interaction: { max_frames: 4096, max_frame_bytes: 65536, max_total_bytes: 1048576, idle_ms: idleMs }, files: entries };
        const configPath = path.join(directory, 'config.json');
        fs.writeFileSync(configPath, JSON.stringify(config));
        const session = openInteractive(configPath, { cwd: repository });
        let failure = null;
        try { await drive(session); } catch (error) { failure = error; }
        let closed;
        try { closed = await session.close(); } catch (error) { throw failure ?? error; }
        if (failure) throw failure;
        checkInteractiveReceipt(closed.receipt, closed);
      } finally { fs.rmSync(directory, { recursive: true, force: true }); }
    },
  };
}

// Unqualified executor for testing probe logic with trusted doubles only. It runs a
// plain child without containment and can never produce campaign evidence.
function localTrustedExecutor({ timeoutMs = 10000 } = {}) {
  const { spawn } = require('node:child_process');
  function stage(files) {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-developer-local-'));
    for (const file of files) fs.writeFileSync(path.join(directory, file.name), file.content);
    fs.writeFileSync(path.join(directory, 'package.json'), '{"type":"commonjs"}');
    return directory;
  }
  const environment = process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {};
  return {
    name: 'local-trusted-test-double',
    qualified: false,
    async single(files, input) {
      const directory = stage(files);
      try {
        fs.copyFileSync(path.join(__dirname, 'node-fixture-bootstrap.cjs'), path.join(directory, 'bootstrap.cjs'));
        const id = crypto.randomBytes(16).toString('hex');
        const child = spawnSync(process.execPath, ['--max-old-space-size=128', 'bootstrap.cjs'], {
          cwd: directory, input: JSON.stringify({ id, input }), encoding: 'utf8', timeout: timeoutMs, maxBuffer: 1048576, windowsHide: true, env: environment });
        if (child.error || child.status !== 0 || child.stderr) throw Error('Local double run failed');
        const response = JSON.parse(child.stdout);
        assert.deepEqual(Object.keys(response).sort(), ['id', 'result']); assert.equal(response.id, id);
        return response.result;
      } finally { fs.rmSync(directory, { recursive: true, force: true }); }
    },
    async interactive(files, drive) {
      const directory = stage(files);
      fs.copyFileSync(path.join(__dirname, 'node-fixture-interactive-bootstrap.cjs'), path.join(directory, 'bootstrap.cjs'));
      const child = spawn(process.execPath, ['--max-old-space-size=128', 'bootstrap.cjs'], { cwd: directory, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, env: environment });
      const session = crypto.randomBytes(16).toString('hex');
      const frames = [], waiters = [];
      let pending = '', stderr = '', closed = false, sent = 0, consumed = 0;
      const exited = new Promise(resolve => child.on('close', code => { closed = true; for (const wake of waiters.splice(0)) wake(); resolve(code); }));
      const timer = setTimeout(() => child.kill(), timeoutMs);
      child.stderr.on('data', text => { stderr += text; });
      child.stdout.setEncoding('utf8');
      child.stdout.on('data', text => {
        pending += text;
        for (let index; (index = pending.indexOf('\n')) >= 0; pending = pending.slice(index + 1)) frames.push(Buffer.from(pending.slice(0, index)).toString('base64'));
        for (const wake of waiters.splice(0)) wake();
      });
      child.stdin.on('error', () => {});
      child.stdin.write(JSON.stringify({ session }) + '\n');
      const api = {
        send(body) { sent++; child.stdin.write(JSON.stringify({ seq: sent, body }) + '\n'); },
        async receive(wait = 5000) {
          const deadline = Date.now() + wait;
          while (!frames.length) {
            if (closed) throw Error('Child ended before replying');
            const remaining = deadline - Date.now();
            if (remaining <= 0) throw Error('Timed out waiting for child frame');
            let timeout;
            await new Promise(resolve => { waiters.push(resolve); timeout = setTimeout(resolve, remaining); });
            clearTimeout(timeout);
          }
          consumed++;
          return decodeFrame(frames.shift(), session, consumed);
        },
      };
      let failure = null;
      try { await drive(api); } catch (error) { failure = error; }
      child.stdin.end();
      const code = await exited;
      clearTimeout(timer);
      fs.rmSync(directory, { recursive: true, force: true });
      if (failure) throw failure;
      assert.equal(code, 0); assert.equal(stderr, ''); assert.equal(pending, ''); assert.equal(frames.length, 0, 'Child sent unrequested frames');
    },
  };
}

// Grade one case against the final workspace (Map path -> content). Every probe
// group is recorded; one failure does not stop the others.
async function grade(caseId, final, executor) {
  const { task, oracle, initial } = load(caseId);
  const grading = oracle.functional_grading;
  const base = { case_id: task.id, mode: grading.mode, executor: executor?.name ?? null, qualified_executor: executor?.qualified === true };
  if (grading.mode === 'none') return { ...base, functional_pass: null, observations: [], errors: [], not_run: ['No executable functional contract for this case'] };
  const probe = probes[task.id];
  if (!probe) throw Error(`No functional probe for ${task.id}`);
  const files = inventory(final instanceof Map ? final : new Map(Object.entries(final)), grading, probe.wrapper);
  const observations = [], errors = [];
  for (const [name, check] of probe.groups({ initial, oracle })) {
    try {
      if (grading.mode === 'single_shot') await check(input => executor.single(files, input));
      else await executor.interactive(files, session => check(session));
      observations.push({ name, passed: true });
    } catch (error) {
      observations.push({ name, passed: false });
      errors.push({ name, message: String(error?.message ?? error).slice(0, 2048) });
    }
  }
  return { ...base, functional_pass: errors.length === 0, observations, errors,
    not_run: ['Selected SDK and live provider compatibility', 'Browser, layout and human usefulness review'] };
}
const gradedCases = Object.keys(probes);
module.exports = { grade, gradedCases, inventory, appContainerExecutor, localTrustedExecutor };
