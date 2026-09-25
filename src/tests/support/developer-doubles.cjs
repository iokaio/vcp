// SPDX-License-Identifier: Apache-2.0
'use strict';
// Trusted reference implementations for testing CS-2 probe logic. They are test
// doubles, never candidate evidence. Mutations append source that wraps an export.
const { load } = require('../../../scripts/evals/developer-oracle.cjs');
const exact = `const exact = (value, names) => value !== null && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...names].sort());\n`;
const guard = `if (typeof module === 'object' && module.exports) module.exports = exports;\n`;
const sources = {
  results: `'use strict';\nfunction filterItems(items, category) { return items.filter(item => category === 'all' || item.category === category).map(item => item.id); }\nexports.filterItems = filterItems;\nif (typeof document !== 'undefined') { /* DOM wiring omitted in the double */ }\n`,
  transition: `'use strict';\nconst table = { idle: { start: 'loading' }, loading: { start: 'loading', fail: 'error', succeed: 'success' }, error: { retry: 'loading' } };\nexports.transition = (state, event) => table[state]?.[event] ?? state;\n`,
  parseCount: `'use strict';\nexports.parseCount = value => { if (typeof value !== 'string' || !/^[0-9]+$/.test(value)) throw TypeError('count'); const number = Number(value); if (!Number.isSafeInteger(number)) throw TypeError('count'); return number; };\n`,
  label: `'use strict';\nexports.normalizeLabel = value => { if (typeof value !== 'string') throw TypeError('label'); return value.trim().replace(/[A-Z]/g, c => c.toLowerCase()); };\n`,
  rest: `'use strict';\nexports.validate = body => { if (!body || typeof body !== 'object' || Array.isArray(body) || Object.keys(body).length !== 1 || !Object.hasOwn(body, 'name') || typeof body.name !== 'string') return false; const name = body.name.trim(); return name.length >= 1 && name.length <= 40; };\n`,
  request: `'use strict';\n${exact}exports.summarize = async (input, transport) => {
  if (typeof input !== 'string' || !input || Buffer.byteLength(input) > 1000) throw TypeError('input');
  const response = await transport.send({ provider: 'openrouter', model: 'synthetic-model-v1', max_output_tokens: 128, input });
  if (exact(response, ['ok', 'error']) && response.ok === false) {
    if (!exact(response.error, ['code', 'message']) || typeof response.error.code !== 'string' || !response.error.code || typeof response.error.message !== 'string') throw TypeError('error response');
    throw Object.assign(Error(response.error.message), { code: response.error.code });
  }
  if (!(exact(response, ['ok', 'text']) || exact(response, ['ok', 'text', 'usage'])) || response.ok !== true || typeof response.text !== 'string') throw TypeError('response');
  if (response.usage != null && (!exact(response.usage, ['input_tokens', 'output_tokens']) || !Object.values(response.usage).every(value => Number.isSafeInteger(value) && value >= 0))) throw TypeError('usage');
  return { text: response.text, usage: response.usage ?? null };
};\n`,
  stream: `'use strict';\nexports.limits = { bytes: 4096, events: 32, midstream: true };
exports.collect = async (chunks, signal) => {
  const limits = exports.limits, result = { text: '', status: 'incomplete', usage: null, error: null };
  if (signal.aborted) return { ...result, status: 'cancelled' };
  let length = 0, events = 0, pending = '';
  const decoder = new TextDecoder();
  for await (const part of chunks) {
    if (limits.midstream && signal.aborted) return { ...result, status: 'cancelled' };
    length += part.length; if (length > limits.bytes) throw Error('bytes');
    pending += decoder.decode(part, { stream: true });
    for (let newline; (newline = pending.indexOf('\\n')) !== -1;) {
      const line = pending.slice(0, newline); pending = pending.slice(newline + 1);
      if (!line) continue;
      events++; if (events > limits.events) throw Error('events');
      const event = JSON.parse(line);
      if (event.type === 'delta') result.text += event.text;
      if (event.type === 'done') { result.status = 'completed'; result.usage = event.usage ?? null; }
      if (event.type === 'error') { result.status = 'error'; result.error = event.code; }
    }
  }
  return result;
};\n`,
  // Regression double: bounded, but checks cancellation only before consuming.
  bufferedStream: `'use strict';\nexports.collect = async (chunks, signal) => {
  const result = { text: '', status: 'incomplete', usage: null, error: null };
  if (signal.aborted) return { ...result, status: 'cancelled' };
  const parts = []; let length = 0;
  for await (const part of chunks) { length += part.length; if (length > 4096) throw Error('bytes'); parts.push(Buffer.from(part)); }
  const events = Buffer.concat(parts).toString('utf8').split('\\n').filter(Boolean).map(JSON.parse);
  if (events.length > 32) throw Error('events');
  for (const event of events) {
    if (event.type === 'delta') result.text += event.text;
    if (event.type === 'done') { result.status = 'completed'; result.usage = event.usage ?? null; }
    if (event.type === 'error') { result.status = 'error'; result.error = event.code; }
  }
  return result;
};\n`,
  mcp: capability => `'use strict';
const resources = [
  { uri: 'fixture://guide/start', name: 'Start', mimeType: 'text/plain', text: 'Start with local evidence.\\n' },
  { uri: 'fixture://guide/stop', name: 'Stop', mimeType: 'text/plain', text: 'Stop at the declared boundary.\\n' },
];
exports.handle = async message => {
  if (message.id === undefined) return null;
  const reply = { jsonrpc: '2.0', id: message.id }, params = message.params;
  if (message.method === 'initialize') return { ...reply, result: { protocolVersion: '2025-11-25', capabilities: { ${capability}: {} }, serverInfo: { name: 'double', version: '1.0.0' } } };
  if (message.method === 'tools/list') return { ...reply, result: { tools: [
    { name: 'lookup_label', inputSchema: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'], additionalProperties: false } },
    { name: 'count_labels', inputSchema: { type: 'object', properties: {}, additionalProperties: false } },
  ] } };
  if (message.method === 'tools/call') {
    const valid = params.name === 'count_labels' && Object.keys(params.arguments).length === 0 || params.name === 'lookup_label' && typeof params.arguments.id === 'string' && Object.keys(params.arguments).length === 1;
    const labels = { l1: 'Amber', l2: 'Blue', l3: 'Copper', l4: 'Dove', l5: 'Elm' };
    if (valid && params.name === 'lookup_label' && !Object.hasOwn(labels, params.arguments.id)) return { ...reply, result: { isError: true, content: [{ type: 'text', text: 'Unknown record' }] } };
    return valid ? { ...reply, result: { content: [{ type: 'text', text: params.name === 'lookup_label' ? labels[params.arguments.id] : '5' }] } } : { ...reply, error: { code: -32602, message: 'Invalid params' } };
  }
  if (message.method === 'resources/list') return { ...reply, result: { resources: resources.map(({ text, ...metadata }) => metadata) } };
  if (message.method === 'resources/read') {
    const resource = resources.find(item => item.uri === params.uri);
    return resource ? { ...reply, result: { contents: [{ uri: resource.uri, mimeType: resource.mimeType, text: resource.text }] } } : { ...reply, error: { code: -32602, message: 'Unknown resource' } };
  }
  return { ...reply, error: { code: -32601, message: 'Method not found' } };
};\n`,
  boundary: `'use strict';
const labels = ['Amber', 'Blue', 'Copper', 'Dove', 'Elm'].map((text, index) => ({ id: 'l' + (index + 1), text }));
${exact}const error = (id, code, message = 'Synthetic fixture error') => ({ jsonrpc: '2.0', id, error: { code, message } });
const reply = (id, result) => { const packet = { jsonrpc: '2.0', id, result }; return Buffer.byteLength(JSON.stringify(packet)) > 4096 ? error(id, -32001, 'Reply exceeds 4096 bytes') : packet; };
exports.handle = async (message, state) => {
  const { id, method, params } = message;
  if (id === undefined) {
    if (method === 'notifications/cancelled' && exact(params, ['requestId']) && typeof params.requestId === 'string') state.pending.delete(params.requestId);
    return null;
  }
  if (method === 'initialize') {
    state.initialized = true;
    state.flush = () => { const replies = [...state.pending].map(([key, text]) => reply(key, { text })); state.pending.clear(); return replies; };
    return reply(id, { protocolVersion: '2025-11-25', capabilities: {}, serverInfo: { name: 'double', version: '1.0.0' } });
  }
  if (!state.initialized) return error(id, -32000);
  if (method === 'fixture/list_labels') {
    if (!exact(params, []) && !(exact(params, ['cursor']) && ['page-2', 'page-4'].includes(params.cursor))) return error(id, -32602);
    const start = params.cursor ? Number(params.cursor.slice(-1)) : 0;
    return reply(id, { labels: labels.slice(start, start + 2), ...(start < 4 ? { nextCursor: 'page-' + (start + 2) } : {}) });
  }
  if (['fixture/delay', 'fixture/echo'].includes(method)) {
    if (!exact(params, ['text']) || typeof params.text !== 'string' || Buffer.byteLength(params.text) > 8192) return error(id, -32602);
    if (method === 'fixture/echo') return reply(id, { text: params.text });
    if (state.pending.has(id)) return error(id, -32602);
    if (state.pending.size >= 16) return error(id, -32002);
    state.pending.set(id, params.text); return null;
  }
  return error(id, -32601);
};\n`,
};
// The trusted reference double for each graded case.
const reference = {
  'UI-normal-results-v2': sources.results, 'UI-boundary-states-v2': sources.transition,
  'UI-near-miss-parser-v2': sources.parseCount, 'LLM-near-miss-parser-v2': sources.label,
  'MCP-near-miss-rest-v3': sources.rest, 'MCP-boundary-pages-v3': sources.boundary,
  'MCP-normal-tools-v3': sources.mcp('tools'), 'MCP-normal-resources-v2': sources.mcp('resources'),
  'LLM-normal-request-v3': sources.request, 'LLM-normal-stream-v3': sources.stream, 'LLM-boundary-partial-v3': sources.stream,
};
// A final workspace equal to the frozen inputs with the subject replaced.
function workspace(caseId, source = reference[caseId], mutation = '') {
  const { oracle, initial } = load(caseId);
  const final = new Map(initial);
  final.set(oracle.functional_grading.subject, source + mutation + '\n');
  return final;
}
module.exports = { sources, reference, workspace, guard };
