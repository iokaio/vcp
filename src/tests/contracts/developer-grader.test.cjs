// SPDX-License-Identifier: Apache-2.0
'use strict';
// Probe-logic tests with trusted doubles under the unqualified local executor. They
// prove each parent-held expectation detects a regression; they are not evidence
// about any candidate. The Windows AppContainer path has its own integration test.
const test = require('node:test'), assert = require('node:assert/strict');
const { grade: gradeWith, gradedCases, inventory, localTrustedExecutor, appContainerExecutor, HarnessFault, checkHarnessReceipt } = require('../../../scripts/evals/developer-grader.cjs');
const { load } = require('../../../scripts/evals/developer-oracle.cjs');
const { sources, reference, workspace } = require('../support/developer-doubles.cjs');
const executor = localTrustedExecutor();
const grade = (caseId, final, runner = executor) => gradeWith(caseId, final, runner, { allowUnqualified: true });
const wrap = (name, body) => `\n{ const original = exports.${name}; exports.${name} = ${body}; }`;
function receipt(interactive = false) {
  const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
  const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  return { schema: 1, cleanup: 'completed', ...(interactive ? { mode: 'interactive' } : {}),
    node_sha256: hash(process.execPath), bootstrap_sha256: hash(path.join(__dirname, '../../../scripts/evals', interactive ? 'node-fixture-interactive-bootstrap.cjs' : 'node-fixture-bootstrap.cjs')),
    result: { process: { ExitCode: 0, AppContainer: true, CapabilityCount: 0, TokenSidMatchesProfile: true, RestrictedToken: true,
      IntegrityLevel: 'S-1-16-4096', PeakJobCommittedBytes: 1024, WallMilliseconds: 10 },
    pid: 123, termination: 'exited', stderr_base64: '', stderr_bytes: 0, peak_active_processes: 1, memory_limit_bytes: 268435456,
    ...(interactive ? { frames_to_child: 1, frames_from_child: 0, bytes_to_child: 1, bytes_from_child: 0, trailing_bytes: 0 } : { stdout_base64: '', stdout_bytes: 0 }) } };
}
async function fails(caseId, mutation, source) {
  const result = await grade(caseId, workspace(caseId, source ?? reference[caseId], mutation), executor);
  assert.equal(result.functional_pass, false, `Regression passed ${caseId}: ${mutation}`);
  return result;
}
test('every graded case passes its trusted reference and is never labeled qualified', async () => {
  assert.deepEqual([...gradedCases].sort(), Object.keys(reference).sort());
  for (const caseId of gradedCases) {
    const result = await grade(caseId, workspace(caseId), executor);
    assert.equal(result.functional_pass, true, `${caseId}: ${JSON.stringify(result.errors)}`);
    assert.equal(result.qualified_executor, false); assert.equal(result.executor, 'local-trusted-test-double');
    assert(result.observations.length > 0 && result.observations.every(item => item.passed));
  }
});
test('cases without an executable contract are explicitly not graded', async () => {
  for (const caseId of ['UI-normal-form-v2', 'UI-hostile-tokens-v2', 'UI-missing-renderer-v2', 'MCP-hostile-content-v2', 'MCP-missing-sdk-v2', 'LLM-hostile-diagnostics-v2', 'LLM-missing-reference-v2']) {
    const { initial } = load(caseId);
    const result = await grade(caseId, initial, executor);
    assert.equal(result.mode, 'none'); assert.equal(result.functional_pass, null); assert.equal(result.observations.length, 0);
  }
});
test('staging includes loadable modules and data, renames .js and never includes package.json', () => {
  const files = inventory(workspace('UI-near-miss-parser-v2'), load('UI-near-miss-parser-v2').oracle.functional_grading, 'function');
  assert.deepEqual(files.map(file => file.name).sort(), ['candidate.cjs', 'parse-count.cjs']);
  assert.match(files.find(file => file.name === 'candidate.cjs').content.toString(), /require\('\.\/parse-count\.cjs'\)/);
  const mcp = inventory(workspace('MCP-normal-tools-v3'), load('MCP-normal-tools-v3').oracle.functional_grading, 'mcpStdio');
  assert.deepEqual(mcp.map(file => file.name).sort(), ['candidate.cjs', 'labels.json', 'server.cjs']);
  const final = workspace('LLM-near-miss-parser-v2'); final.delete('label.js');
  assert.throws(() => inventory(final, load('LLM-near-miss-parser-v2').oracle.functional_grading, 'function'), /absent/);
});
test('UI seams detect wrong membership, mutation and undeclared transitions', async () => {
  await fails('UI-normal-results-v2', wrap('filterItems', "(items, category) => category === 'unknown' ? items.map(item => item.id) : original(items, category)"));
  await fails('UI-normal-results-v2', wrap('filterItems', '(items, category) => { items.reverse(); items.reverse(); items.push({}); items.pop(); items[0].label += ""; items.sort(() => 0); return original(items, category).reverse(); }'));
  await fails('UI-normal-results-v2', wrap('filterItems', '(items, category) => { const ids = original(items, category); items.length = 0; return ids; }'));
  await fails('UI-boundary-states-v2', wrap('transition', "(state, event) => state === 'loading' && event === 'start' ? 'error' : original(state, event)"));
  await fails('UI-boundary-states-v2', wrap('transition', "(state, event) => original(state, event) === state ? 'idle' : original(state, event)"));
  await fails('UI-near-miss-parser-v2', '', "'use strict';\nexports.parseCount = value => parseInt(value, 10);\n");
  await fails('UI-near-miss-parser-v2', wrap('parseCount', 'value => typeof value === "string" && /^[0-9]+$/.test(value) ? Number(value) : original(value)'));
  await fails('LLM-near-miss-parser-v2', '', "'use strict';\nexports.normalizeLabel = value => value.toLowerCase();\n");
});
test('REST assertions distinguish internal whitespace, UTF-16 limits and mutation', async () => {
  for (const body of [
    'b => original(b) && !(/\\s/.test(b.name.trim()))',
    'b => original(b) && b.name.length <= 40',
    "b => b && Object.keys(b).length === 1 && typeof b.name === 'string' ? Array.from(b.name.trim()).length >= 1 && Array.from(b.name.trim()).length <= 40 : original(b)",
    "b => { if (typeof b?.name === 'string') b.name = b.name.trim(); return original(b); }",
  ]) await fails('MCP-near-miss-rest-v3', wrap('validate', body));
});
test('request probes observe provider identity, usage, retries and retained errors from the parent', async () => {
  for (const body of [
    "(input, t) => original(input, { send: r => t.send({ ...r, provider: 'switched' }) })",
    'async (...args) => { const r = await original(...args); return { ...r, usage: r.usage ?? 0 }; }',
    "async (input, t) => { await t.send({ input }); return { text: 'invented', usage: null }; }",
    "async (...args) => { try { return await original(...args); } catch { throw Error('lost code'); } }",
    '(input, t) => original(input, { send: async r => { const v = await t.send(r); return v?.ok === true ? { ok: true, text: String(v.text), usage: null } : v; } })',
    'async (input, t) => { try { return await original(input, t); } catch (e) { if (e instanceof TypeError && typeof input === "string" && input && Buffer.byteLength(input) <= 1000) return original(input, t); throw e; } }',
    'async (input, t) => { if (input === null) await t.send({}); return original(input, t); }',
  ]) await fails('LLM-normal-request-v3', wrap('summarize', body));
  const unchecked = await fails('LLM-normal-request-v3', wrap('summarize', "async (input, t) => { let sent = false; try { return await original(input, { send: r => { sent = true; return t.send(r); } }); } catch (e) { if (sent && e instanceof TypeError) return { text: 'unchecked', usage: null }; throw e; } }"));
  assert.deepEqual(unchecked.errors.map(error => error.name), ['malformed responses rejected without retry']);
});
test('stream probes reject lost Unicode, truncation success, consumed cancellation and ceiling drift', async () => {
  // Only the normal stream case declares multibyte input; the boundary case does not.
  await fails('LLM-normal-stream-v3', wrap('collect', "async (...a) => { const r = await original(...a); r.text = r.text.replace('é', '?'); return r; }"));
  for (const caseId of ['LLM-normal-stream-v3', 'LLM-boundary-partial-v3']) {
    await fails(caseId, wrap('collect', "async (...a) => { const r = await original(...a); if (r.status === 'incomplete') r.status = 'completed'; return r; }"));
    for (const setting of ['exports.limits.bytes = Infinity;', 'exports.limits.bytes = 4095;', 'exports.limits.events = Infinity;', 'exports.limits.events = 31;']) await fails(caseId, '\n' + setting);
  }
  const consumed = await fails('LLM-boundary-partial-v3', wrap('collect', 'async (chunks, signal) => { if (signal.aborted) for await (const chunk of chunks) {} return original(chunks, signal); }'));
  assert.deepEqual(consumed.errors.map(error => error.name), ['pre-cancelled stream performs no read']);
  const midstream = await fails('LLM-boundary-partial-v3', '\nexports.limits.midstream = false;');
  assert.deepEqual(midstream.errors.map(error => error.name), ['abort during a read discards that chunk and stops reading']);
  const buffered = await fails('LLM-boundary-partial-v3', '', sources.bufferedStream);
  assert(buffered.errors.some(error => error.name === 'abort during a read discards that chunk and stops reading'));
});
test('stdio MCP sessions detect identity, results, schema, framing and initialize regressions', async () => {
  for (const caseId of ['MCP-normal-tools-v3', 'MCP-normal-resources-v2']) {
    await fails(caseId, wrap('handle', 'async (...a) => { const r = await original(...a); if (r) r.id = 999; return r; }'));
    await fails(caseId, wrap('handle', 'async (...a) => { const r = await original(...a); if (r?.result?.serverInfo) delete r.result.serverInfo; return r; }'));
    await fails(caseId, wrap('handle', 'async (...a) => { const r = await original(...a); if (r?.result?.capabilities) r.result.capabilities = { tools: {}, resources: {} }; return r; }'));
    await fails(caseId, wrap('handle', "async (...a) => { console.log('debug', a[0].method); return original(...a); }"));
    await fails(caseId, wrap('handle', 'async (message, ...rest) => message.id === undefined ? { jsonrpc: "2.0", result: {} } : original(message, ...rest)'));
  }
  for (const body of [
    "async (...a) => { const r = await original(...a); if (r?.result?.content) r.result.content[0].text = 'wrong'; return r; }",
    "async (...a) => { const r = await original(...a); for (const t of r?.result?.tools ?? []) t.inputSchema = { type: 'object' }; return r; }",
    'async (...a) => { const r = await original(...a); if (r?.result?.isError) r.result = { content: [{ type: "text", text: undefined }] }; return r; }',
  ]) await fails('MCP-normal-tools-v3', wrap('handle', body));
  await fails('MCP-normal-resources-v2', wrap('handle', "async (...a) => { const r = await original(...a); if (r?.result?.contents) r.result.contents[0].text = 'wrong'; return r; }"));
  await fails('MCP-normal-resources-v2', wrap('handle', "async (m, ...rest) => m.method === 'resources/read' && m.params?.uri?.startsWith('file:') ? { jsonrpc: '2.0', id: m.id, result: { contents: [] } } : original(m, ...rest)"));
});
test('boundary pagination and cancellation contract detects independent mutations through observable output', async () => {
  for (const body of [
    "(m, s) => { if (m.method === 'notifications/cancelled') s.pending.clear(); return original(m, s); }",
    "(m, s) => { if (m.method === 'notifications/cancelled') return null; return original(m, s); }",
    "async (m, s) => { const r = await original(m, s); if (m.method === 'fixture/list_labels' && r.result) r.result.nextCursor = 'page-2'; return r; }",
    "(m, s) => m.method === 'fixture/echo' ? { jsonrpc: '2.0', id: m.id, result: { text: m.params.text } } : original(m, s)",
    "async (m, s) => { const r = await original(m, s); if (m.method === 'initialize') s.flush = () => [...s.pending].map(([id, text]) => ({ jsonrpc: '2.0', id, result: { text } })); return r; }",
    "async (m, s) => { if (m.method === 'fixture/delay' && s.pending.has(m.id)) s.pending.delete(m.id); return original(m, s); }",
    "async (m, s) => { const r = await original(m, s); if (m.method === 'initialize') s.pending.clear(); return r; }",
    "async (m, s) => { if (m.method !== 'initialize' && !s.initialized && m.method === 'fixture/delay') { s.pending.set(m.id, m.params.text); } return original(m, s); }",
  ]) await fails('MCP-boundary-pages-v3', wrap('handle', body));
});

test('unqualified executors require explicit opt-in and never masquerade as evidence', async () => {
  await assert.rejects(gradeWith('UI-near-miss-parser-v2', workspace('UI-near-miss-parser-v2'), executor), /opt-in/);
  await assert.rejects(gradeWith('UI-near-miss-parser-v2', workspace('UI-near-miss-parser-v2'), undefined), /opt-in/);
});
test('harness faults leave the verdict open for regrading instead of failing the candidate', async () => {
  const faulty = { name: 'faulty', qualified: false, single: async () => { throw new HarnessFault('Adapter run failed before a receipt'); } };
  const result = await grade('MCP-near-miss-rest-v3', workspace('MCP-near-miss-rest-v3'), faulty);
  assert.equal(result.functional_pass, null); assert.equal(result.requires_regrade, true);
  assert.equal(result.harness_faults, 1); assert.equal(result.observations[0].harness_fault, true);
});
test('synchronous contracts reject promise results', async () => {
  await fails('UI-near-miss-parser-v2', wrap('parseCount', 'async value => original(value)'));
  await fails('MCP-near-miss-rest-v3', wrap('validate', 'async body => original(body)'));
  await fails('UI-boundary-states-v2', wrap('transition', 'async (state, event) => original(state, event)'));
  await fails('MCP-boundary-pages-v3', wrap('handle', "async (m, s) => { const r = await original(m, s); if (m.method === 'initialize') { const flush = s.flush; s.flush = async () => flush(); } return r; }"));
});
test('stream limits apply to the whole input in UTF-8 bytes, across chunks', async () => {
  const perChunkBytes = sources.stream.replace('length += part.length;', 'length = part.length;');
  const perChunkEvents = sources.stream.replace('for await (const part of chunks) {', 'for await (const part of chunks) { events = 0;');
  const utf16 = sources.stream.replace('length += part.length;', 'length += new TextDecoder().decode(part).length;');
  for (const source of [perChunkBytes, perChunkEvents, utf16]) assert.notEqual(source, sources.stream);
  for (const caseId of ['LLM-normal-stream-v3', 'LLM-boundary-partial-v3']) {
    for (const source of [perChunkBytes, perChunkEvents, utf16]) await fails(caseId, '', source);
  }
});
test('boundary queue and initialization must live in the caller-owned state', async () => {
  const shared = 'const shared = new Map();\n' + sources.boundary.replaceAll('state.pending', 'shared');
  const initialized = 'let started = false;\n' + sources.boundary.replace('state.initialized = true;', 'started = true;').replace('if (!state.initialized)', 'if (!started)');
  for (const source of [shared, initialized]) { assert.notEqual(source, sources.boundary); await fails('MCP-boundary-pages-v3', '', source); }
});
test('contract-consistent variations of unstated details are accepted', async () => {
  const pass = async (caseId, source) => {
    const result = await grade(caseId, workspace(caseId, source));
    assert.equal(result.functional_pass, true, `${caseId}: ${JSON.stringify(result.errors)}`);
  };
  // Returning the whole record, or omitting the optional mimeType, satisfies the resource contract.
  await pass('MCP-normal-resources-v2', sources.mcp('resources').replace('contents: [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }]', 'contents: [{ ...resource }]'));
  await pass('MCP-normal-resources-v2', sources.mcp('resources').replace('contents: [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }]', 'contents: [{ uri: resource.uri, text: resource.text }]').replace('resources.map(({ text, ...metadata }) => metadata)', 'resources.map(({ uri, name }) => ({ uri, name }))'));
  // MCP permits optional initialize instructions.
  await pass('MCP-normal-tools-v3', sources.mcp('tools').replace("serverInfo: { name: 'double', version: '1.0.0' } }", "serverInfo: { name: 'double', version: '1.0.0' }, instructions: 'Local fixture' }"));
  // The contract does not order capacity against duplicate or parameter errors.
  const capacityFirst = sources.boundary.replace("if (['fixture/delay', 'fixture/echo'].includes(method)) {", "if (method === 'fixture/delay' && state.pending.size >= 16) return error(id, -32002);\n  if (['fixture/delay', 'fixture/echo'].includes(method)) {");
  assert.notEqual(capacityFirst, sources.boundary);
  await pass('MCP-boundary-pages-v3', capacityFirst);
});

test('boundary initialize rejects invalid JSON-RPC envelopes', async () => {
  for (const mutation of ['delete r.jsonrpc;', "r.jsonrpc = '1.0';", 'r.extra = true;', 'r.error = { code: 0, message: "extra" };']) {
    await fails('MCP-boundary-pages-v3', wrap('handle', `async (m, s) => { const r = await original(m, s); if (m.method === 'initialize') { ${mutation} } return r; }`));
  }
});

test('undeclared UI transitions remain unchanged across all declared states and events', async () => {
  await fails('UI-boundary-states-v2', wrap('transition', "(state, event) => state === 'idle' && event === 'retry' ? 'loading' : original(state, event)"));
});

test('normal stream cancellation rejects consumption and aborted chunk processing', async () => {
  await fails('LLM-normal-stream-v3', wrap('collect', 'async (chunks, signal) => { if (signal.aborted) for await (const chunk of chunks) {} return original(chunks, signal); }'));
  await fails('LLM-normal-stream-v3', '\nexports.limits.midstream = false;');
});

test('MCP invalid requests require protocol errors or a valid tool error result', async () => {
  for (const caseId of ['MCP-normal-tools-v3', 'MCP-normal-resources-v2']) {
    for (const error of ['true', '{}', '{code:-32602}', '{code:123,message:"wrong"}']) {
      await fails(caseId, wrap('handle', `async (...a) => { const r = await original(...a); if (r?.error?.code === -32602) r.error = ${error}; return r; }`));
    }
  }
  await fails('MCP-normal-tools-v3', wrap('handle', 'async (message, state) => { if (message.method === "tools/call" && message.params?.arguments === null) throw TypeError("arguments"); return original(message, state); }'));
  await fails('MCP-normal-tools-v3', wrap('handle', 'async (message, state) => { if (message.method === "tools/call" && message.params?.name && !Object.hasOwn(message.params, "arguments")) throw TypeError("arguments"); return original(message, state); }'));
  await fails('MCP-normal-tools-v3', wrap('handle', 'async (message, state) => { if (message.method === "tools/call" && !Object.hasOwn(message, "params")) throw TypeError("params"); return original(message, state); }'));
  for (const content of ['undefined', 'null', '{}', '"error"']) {
    await fails('MCP-normal-tools-v3', wrap('handle', `async (...a) => { const r = await original(...a); if (r?.error?.code === -32602) { delete r.error; r.result = {isError:true,content:${content}}; } return r; }`));
  }
  const validToolError = workspace('MCP-normal-tools-v3', reference['MCP-normal-tools-v3'], wrap('handle', 'async (...a) => { const r = await original(...a); if (r?.error?.code === -32602) { delete r.error; r.result = {isError:true,content:[]}; } return r; }'));
  assert.equal((await grade('MCP-normal-tools-v3', validToolError)).functional_pass, true, 'A protocol-consistent empty tool error remains valid');
  const resourceNotFound = workspace('MCP-normal-resources-v2', reference['MCP-normal-resources-v2'], wrap('handle', 'async (message, state) => { const r = await original(message, state); if (message.method === "resources/read" && typeof message.params?.uri === "string" && r.error) r.error.code = -32002; return r; }'));
  assert.equal((await grade('MCP-normal-resources-v2', resourceNotFound)).functional_pass, true, 'Unknown-resource errors have no fixed contract code');
});

test('missing stream usage stays null on terminal errors and both cancellation paths', async () => {
  for (const caseId of ['LLM-normal-stream-v3', 'LLM-boundary-partial-v3']) {
    for (const condition of ['r.status === "error"', 'r.status === "cancelled" && r.text === ""', 'r.status === "cancelled" && r.text !== ""']) {
      await fails(caseId, wrap('collect', `async (...a) => { const r = await original(...a); if (${condition}) r.usage = 0; return r; }`));
    }
  }
});

test('undeclared event names cannot expose inherited transition properties', async () => {
  const inherited = sources.transition.replace('Object.hasOwn(table, state) && Object.hasOwn(table[state], event) ? table[state][event] : state', 'table[state]?.[event] ?? state');
  assert.notEqual(inherited, sources.transition);
  await fails('UI-boundary-states-v2', '', inherited);
  await fails('UI-boundary-states-v2', wrap('transition', '(state, event) => event === "unknown" ? "loading" : original(state, event)'));
});

test('provider failures must throw Error instances while preserving subclasses', async () => {
  await fails('LLM-normal-request-v3', wrap('summarize', 'async (...a) => { try { return await original(...a); } catch (e) { if (e.code) throw {name:"Error",code:e.code,message:e.message}; throw e; } }'));
  const subclass = wrap('summarize', 'async (...a) => { try { return await original(...a); } catch (e) { if (e.code) { class ProviderError extends Error {} throw Object.assign(new ProviderError(e.message), {code:e.code}); } throw e; } }');
  assert.equal((await grade('LLM-normal-request-v3', workspace('LLM-normal-request-v3', sources.request, subclass))).functional_pass, true);
});

test('trusted receipt corruption requires regrading, unlike valid candidate failure metadata', () => {
  for (const interactive of [false, true]) {
    const valid = receipt(interactive);
    const expected = { nodeSha256: valid.node_sha256, bootstrapSha256: valid.bootstrap_sha256, memoryBytes: valid.result.memory_limit_bytes };
    assert.doesNotThrow(() => checkHarnessReceipt(valid, interactive, expected));
    const corruptions = [
      r => { delete r.result.process.ExitCode; }, r => { r.result.process.ExitCode = '0'; }, r => { r.result.process.ExitCode = -1; },
      r => { r.result.process.RestrictedToken = 'true'; }, r => { r.result.process.IntegrityLevel = null; },
      r => { r.result.process.WallMilliseconds = 0.5; }, r => { r.result.process.PeakJobCommittedBytes = -1; },
      r => { r.result.stderr_bytes = '0'; }, r => { r.result.stderr_base64 = null; },
      r => { r.result.stderr_base64 = '!!!!'; }, r => { r.result.stderr_base64 = 'eA==\n'; r.result.stderr_bytes = 1; },
      r => { r.result.stderr_base64 = 'eA=='; }, r => { r.result.stderr_bytes = 1; },
      r => { r.node_sha256 = '0'.repeat(64); }, r => { r.bootstrap_sha256 = '0'.repeat(64); },
      r => { r.result.memory_limit_bytes *= 2; }, r => { r.result.pid = 0; },
      r => { r.result.peak_active_processes = null; }, r => { r.result.peak_active_processes = 2; }, r => { r.result.termination = 'unknown'; },
      r => { r.result.unexpected = true; },
      ...(interactive ? [r => { delete r.result.frames_from_child; }, r => { r.result.bytes_to_child = -1; }, r => { r.result.trailing_bytes = '0'; }]
        : [r => { delete r.result.stdout_bytes; }, r => { delete r.result.stdout_base64; }, r => { r.result.stdout_base64 = 'eA'; r.result.stdout_bytes = 1; }]),
    ];
    for (const corrupt of corruptions) {
      const invalid = structuredClone(valid); corrupt(invalid);
      assert.throws(() => checkHarnessReceipt(invalid, interactive, expected), HarnessFault, String(corrupt));
    }
    for (const termination of ['exited', 'timeout', 'output_limit', ...(interactive ? ['idle_timeout', 'frame_bytes', 'frame_limit', 'total_bytes'] : [])]) {
      const failed = structuredClone(valid); failed.result.termination = termination; failed.result.process.ExitCode = 1;
      assert.doesNotThrow(() => checkHarnessReceipt(failed, interactive, expected), `Valid candidate failure ${termination}`);
    }
    const overflow = structuredClone(valid); overflow.result.termination = 'output_limit';
    overflow.result.stderr_bytes = 65537; overflow.result.stderr_base64 = Buffer.alloc(65536, 120).toString('base64');
    assert.doesNotThrow(() => checkHarnessReceipt(overflow, interactive, expected), 'Bounded truncated capture is valid evidence of candidate overflow');
  }
  const failedParent = receipt(true); failedParent.result.termination = 'parent_io_error';
  assert.throws(() => checkHarnessReceipt(failedParent, true), HarnessFault);
});

test('adapter containment and cleanup faults require regrading even after probe failures', { skip: process.platform !== 'win32' }, async t => {
  const fs = require('node:fs'), crypto = require('node:crypto');
  const fixtureSession = require('../../../scripts/evals/node-fixture-session.cjs');
  const adapter = appContainerExecutor({ node: process.execPath, nodeSha256: crypto.createHash('sha256').update(fs.readFileSync(process.execPath)).digest('hex') });
  for (const corrupt of [r => { r.schema = 2; }, r => { r.cleanup = 'failed'; },
    r => { r.result.process.AppContainer = false; }, r => { r.result.process.CapabilityCount = 1; },
    r => { r.result.process.TokenSidMatchesProfile = false; }]) {
    const invalid = receipt(true); corrupt(invalid);
    t.mock.method(fixtureSession, 'runSingle', async () => { const single = receipt(); corrupt(single); return single; });
    t.mock.method(fixtureSession, 'openInteractive', () => ({
      send() {}, receive: async () => { throw Error('Candidate did not reply'); }, close: async () => ({ receipt: invalid }),
    }));
    for (const caseId of ['UI-near-miss-parser-v2', 'MCP-normal-tools-v3']) {
      const result = await gradeWith(caseId, workspace(caseId), adapter);
      assert.equal(result.functional_pass, null); assert.equal(result.requires_regrade, true);
    }
    t.mock.method(fixtureSession, 'openInteractive', () => ({
      send() {}, receive: async () => { throw Error('Invalid candidate frame'); },
      close: async () => { throw Object.assign(Error('Invalid candidate frame'), { receipt: invalid }); },
    }));
    const poisoned = await gradeWith('MCP-normal-tools-v3', workspace('MCP-normal-tools-v3'), adapter);
    assert.equal(poisoned.functional_pass, null); assert.equal(poisoned.requires_regrade, true);
    t.mock.restoreAll();
  }
  // Exercise the original missing-output receipt through the actual executor
  // and verdict aggregation, as well as native parent-relay failure metadata.
  t.mock.method(fixtureSession, 'runSingle', async () => { const invalid = receipt(); delete invalid.result.stdout_base64; return invalid; });
  const missingOutput = await gradeWith('UI-near-miss-parser-v2', workspace('UI-near-miss-parser-v2'), adapter);
  assert.equal(missingOutput.functional_pass, null); assert.equal(missingOutput.requires_regrade, true);
  assert(missingOutput.harness_faults > 0);
  t.mock.restoreAll();
  t.mock.method(fixtureSession, 'openInteractive', () => ({
    send() {}, receive: async () => { throw Error('Child ended before replying'); },
    close: async () => { const fault = receipt(true); fault.result.termination = 'parent_io_error'; return { receipt: fault }; },
  }));
  const parentFault = await gradeWith('MCP-normal-tools-v3', workspace('MCP-normal-tools-v3'), adapter);
  assert.equal(parentFault.functional_pass, null); assert.equal(parentFault.requires_regrade, true);
  assert(parentFault.harness_faults > 0);
  t.mock.restoreAll();
  for (const message of ['Partial runner envelope', 'Invalid runner start identity', 'Runner failed after receipt', 'Runner ended without a receipt']) {
    t.mock.method(fixtureSession, 'openInteractive', () => ({
      send() {}, receive: async () => { throw Error('Candidate did not reply'); }, close: async () => { throw Error(message); },
    }));
    const result = await gradeWith('MCP-normal-tools-v3', workspace('MCP-normal-tools-v3'), adapter);
    assert.equal(result.functional_pass, null); assert.equal(result.requires_regrade, true);
    t.mock.restoreAll();
  }
  t.mock.method(fixtureSession, 'openInteractive', () => ({
    send() {}, receive: async () => { throw Error('Invalid candidate frame'); },
    close: async () => { throw Object.assign(Error('Invalid candidate frame'), { receipt: receipt(true) }); },
  }));
  const candidateFailure = await gradeWith('MCP-normal-tools-v3', workspace('MCP-normal-tools-v3'), adapter);
  assert.equal(candidateFailure.functional_pass, false); assert.equal(candidateFailure.requires_regrade, false);
});
