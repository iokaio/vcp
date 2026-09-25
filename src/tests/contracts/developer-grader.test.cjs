// SPDX-License-Identifier: Apache-2.0
'use strict';
// Probe-logic tests with trusted doubles under the unqualified local executor. They
// prove each parent-held expectation detects a regression; they are not evidence
// about any candidate. The Windows AppContainer path has its own integration test.
const test = require('node:test'), assert = require('node:assert/strict');
const { grade, gradedCases, inventory, localTrustedExecutor } = require('../../../scripts/evals/developer-grader.cjs');
const { load } = require('../../../scripts/evals/developer-oracle.cjs');
const { sources, reference, workspace } = require('../support/developer-doubles.cjs');
const executor = localTrustedExecutor();
const wrap = (name, body) => `\n{ const original = exports.${name}; exports.${name} = ${body}; }`;
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
