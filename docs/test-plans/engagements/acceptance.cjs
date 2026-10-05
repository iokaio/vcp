// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), crypto = require('node:crypto');
const id = () => crypto.randomUUID();
const clone = value => JSON.parse(JSON.stringify(value));
const ordered = tasks => [...tasks].sort((a, b) => String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0);
const errorCode = response => response.json?.error?.code ?? response.json?.code;
function expect(response, statuses) { assert(statuses.includes(response.status), `Expected ${statuses}; received ${response.status}: ${JSON.stringify(response.json)}`); return response.json; }
async function exportTasks(request) {
  const value = expect(await request('GET', '/api/tasks/export'), [200]);
  assert.equal(value.schemaVersion, 1); assert(Array.isArray(value.tasks));
  assert.deepEqual(value.tasks, ordered(value.tasks), 'Export must use ordinal ID order');
  assert.equal(new Set(value.tasks.map(task => task.id)).size, value.tasks.length);
  return value;
}
async function rejectedImport(request, document, code, status = 400) {
  const before = await exportTasks(request);
  const result = await request('POST', '/api/tasks/import', document);
  expect(result, [status]); assert.equal(errorCode(result), code);
  assert.deepEqual(await exportTasks(request), before, 'Rejected import wrote data');
}
async function taskboard(request, phase, saved) {
  if (phase === 'restart' || phase === 'restart-export') {
    assert(saved?.schema === 'engagement-a-state/1');
    assert.deepEqual(await exportTasks(request), saved.expected, 'Import did not survive restart');
    if (phase === 'restart') expect(await request('POST', '/api/tasks/import', saved.imported), [200]);
    assert.deepEqual(await exportTasks(request), saved.expected, 'Repeated import changed persisted data');
    return saved;
  }
  if (phase === 'fault') {
    await rejectedImport(request, { schemaVersion: 2, tasks: [] }, 'VALIDATION_ERROR');
    return { schema: 'engagement-a-fault-check/1' };
  }
  assert(['export', 'full'].includes(phase));
  const empty = await exportTasks(request);
  assert.equal(empty.tasks.length, 0, 'Independent API qualification requires its own empty data file');
  const created = [];
  for (const body of [
    { title: 'Café 東京 🧪', description: 'line one\nline two', priority: 'high', status: 'todo', labels: ['release', 'unicode'] },
    { title: 'Second record', description: '', priority: 'low', status: 'doing', dueDate: '2026-12-12', labels: [] },
    { title: 'Finished', priority: 'medium', status: 'done' },
  ]) created.push(expect(await request('POST', '/api/tasks', body), [201]));
  const initial = await exportTasks(request);
  assert.deepEqual(initial.tasks, ordered(created), 'Export omitted or changed API task fields');
  for (const task of created) assert.deepEqual(expect(await request('GET', `/api/tasks/${task.id}`), [200]), task);
  if (phase === 'export') return { schema: 'engagement-a-state/1', expected: initial, imported: initial };
  const first = { ...clone(created[0]), id: id(), title: 'Imported Unicode résumé' };
  const second = { ...clone(created[1]), id: id(), title: 'Another imported task' };
  const imported = { schemaVersion: 1, tasks: [second, first] };
  expect(await request('POST', '/api/tasks/import', imported), [200, 201]);
  const expected = { schemaVersion: 1, tasks: ordered([...created, first, second]) };
  assert.deepEqual(await exportTasks(request), expected, 'Import must preserve every field and stable ID');
  expect(await request('POST', '/api/tasks/import', imported), [200]);
  expect(await request('POST', '/api/tasks/import', { schemaVersion: 1, tasks: [] }), [200]);
  assert.deepEqual(await exportTasks(request), expected);
  for (const document of [
    { schemaVersion: 2, tasks: [] }, { tasks: [] },
    { schemaVersion: 1, tasks: [first, first] },
    { schemaVersion: 1, tasks: [{ ...first, id: id() }, { ...second, id: id(), title: '' }] },
    { schemaVersion: 1, tasks: [{ ...first, id: id(), status: 'invalid' }] },
    { schemaVersion: 1, tasks: [{ ...first, id: id(), dueDate: '2026-02-30' }] },
  ]) await rejectedImport(request, document, 'VALIDATION_ERROR');
  await rejectedImport(request, { schemaVersion: 1, tasks: [{ ...first, id: id() }, { ...created[0], title: 'Conflict' }] }, 'IMPORT_CONFLICT', 409);
  return { schema: 'engagement-a-state/1', expected, imported };
}
async function inventory(request, phase, saved) {
  const product = async productId => {
    const response = await request('GET', `/api/products/${productId}`);
    const body = expect(response, [200]);
    const rowVersion = body.rowVersion ?? response.headers?.etag?.replace(/^"|"$/g, '');
    assert(typeof rowVersion === 'string' && rowVersion, 'Missing current row version');
    return { body, rowVersion };
  };
  const view = async productId => {
    const stock = expect(await request('GET', `/api/products/${productId}/stock`), [200]);
    const history = expect(await request('GET', `/api/products/${productId}/adjustments`), [200]);
    assert(Number.isInteger(stock.onHand)); assert(Array.isArray(history));
    for (const row of history) {
      for (const key of ['id', 'productId', 'delta', 'beforeQuantity', 'afterQuantity', 'reason', 'timestamp', 'operationId']) assert(Object.hasOwn(row, key), `Audit missing ${key}`);
      assert.equal(row.productId, productId); assert(Number.isInteger(row.delta) && row.delta !== 0);
      assert.equal(row.beforeQuantity + row.delta, row.afterQuantity); assert(row.afterQuantity >= 0);
      assert(Number.isFinite(Date.parse(row.timestamp))); assert(row.reason.trim());
    }
    assert.equal(new Set(history.map(row => row.operationId)).size, history.length);
    assert.deepEqual(history, [...history].sort((a, b) => Date.parse(a.timestamp) - Date.parse(b.timestamp) || (String(a.id) < String(b.id) ? -1 : String(a.id) > String(b.id) ? 1 : 0)));
    return { stock: stock.onHand, history };
  };
  const adjust = (productId, body) => request('POST', `/api/products/${productId}/adjustments`, body);
  if (phase === 'restart') {
    assert(saved?.schema === 'engagement-b-state/1');
    assert.deepEqual(await view(saved.productId), saved.expected);
    const replay = expect(await adjust(saved.productId, saved.original), [200]);
    assert.equal(replay.adjustmentId, saved.originalAdjustmentId);
    assert.deepEqual(await view(saved.productId), saved.expected, 'Restart retry duplicated stock or audit');
    return saved;
  }
  if (phase === 'fault') {
    assert(saved?.schema === 'engagement-b-state/1');
    const before = await view(saved.productId);
    expect(await adjust(saved.productId, { ...saved.original, operationId: id() }), [412]);
    assert.deepEqual(await view(saved.productId), before);
    return saved;
  }
  assert.equal(phase, 'full');
  const suppliers = expect(await request('GET', '/api/suppliers'), [200]);
  assert(suppliers.length);
  const created = expect(await request('POST', '/api/products', { sku: `EE-${id().slice(0, 12)}`, name: 'Engagement stock', unitPrice: 3, reorderLevel: 1, supplierId: suppliers[0].id }), [201]);
  const productId = created.id;
  const original = { operationId: id(), delta: 10, reason: 'Unicode réception 東京', rowVersion: (await product(productId)).rowVersion };
  const first = expect(await adjust(productId, original), [201]);
  assert(first.adjustmentId && first.product?.rowVersion, 'Adjustment response lacks identity/current product version');
  let current = await view(productId);
  assert.equal(current.stock, 10); assert.equal(current.history.length, 1);
  assert.equal(current.history[0].id, first.adjustmentId);
  assert.equal(expect(await adjust(productId, original), [200]).adjustmentId, first.adjustmentId);
  assert.deepEqual(await view(productId), current);
  const conflict = await adjust(productId, { ...original, delta: 11 });
  expect(conflict, [409]); assert.equal(errorCode(conflict), 'OPERATION_CONFLICT');
  assert.deepEqual(await view(productId), current);
  const other = expect(await request('POST', '/api/products', { sku: `EE-${id().slice(0, 12)}`, name: 'Global operation identity', unitPrice: 3, reorderLevel: 1, supplierId: suppliers[0].id }), [201]);
  const otherBefore = await view(other.id);
  const globalConflict = await adjust(other.id, { ...original, rowVersion: (await product(other.id)).rowVersion });
  expect(globalConflict, [409]); assert.equal(errorCode(globalConflict), 'OPERATION_CONFLICT');
  assert.deepEqual(await view(other.id), otherBefore); assert.deepEqual(await view(productId), current);
  for (const [patch, status] of [
    [{ delta: 0 }, 400], [{ delta: 1.5 }, 400], [{ reason: '  ' }, 400], [{ delta: -11 }, 400],
    [{ rowVersion: original.rowVersion }, 412], [{ rowVersion: null }, 428], [{ rowVersion: undefined }, 428],
  ]) {
    const before = await view(productId);
    expect(await adjust(productId, { ...original, operationId: id(), rowVersion: (await product(productId)).rowVersion, ...patch }), [status]);
    assert.deepEqual(await view(productId), before, 'Rejected adjustment mutated data');
  }
  expect(await adjust(2147483647, { ...original, operationId: id() }), [404]);
  expect(await adjust(productId, { ...original, operationId: id(), delta: -3, rowVersion: (await product(productId)).rowVersion }), [201]);
  assert.equal((await view(productId)).stock, 7);
  const duplicate = { operationId: id(), delta: 2, reason: 'Concurrent duplicate', rowVersion: (await product(productId)).rowVersion };
  const duplicates = await Promise.all([adjust(productId, duplicate), adjust(productId, duplicate)]);
  const receipts = duplicates.map(response => expect(response, [200, 201]));
  assert.equal(receipts[0].adjustmentId, receipts[1].adjustmentId);
  current = await view(productId); assert.equal(current.stock, 9); assert.equal(current.history.length, 3);
  const revision = (await product(productId)).rowVersion;
  const race = await Promise.all([1, 2].map(delta => adjust(productId, { operationId: id(), delta, reason: 'Version race', rowVersion: revision })));
  assert.deepEqual(race.map(response => response.status).sort(), [201, 412], 'Exactly one current-version concurrent adjustment must succeed');
  const expected = await view(productId); assert.equal(expected.history.length, 4);
  assert([10, 11].includes(expected.stock));
  return { schema: 'engagement-b-state/1', productId, original, originalAdjustmentId: first.adjustmentId, expected };
}
function sqlAgreement(saved, observed) {
  assert.equal(saved.schema, 'engagement-b-state/1');
  assert.equal(observed.uniqueOperationId, true, 'Database must enforce global operation-ID uniqueness');
  assert.equal(observed.onHand, saved.expected.stock);
  const timestamp = value => {
    assert(typeof value === 'string' && /(?:Z|\+00:00)$/.test(value), 'Audit timestamp must explicitly identify UTC');
    assert(Number.isFinite(Date.parse(value)));
    return value.replace(/\+00:00$/, 'Z').replace(/(\.\d*?[1-9])0+Z$/, '$1Z').replace(/\.0+Z$/, 'Z');
  };
  const fields = row => ({ ...Object.fromEntries(['id', 'productId', 'delta', 'beforeQuantity', 'afterQuantity', 'reason', 'operationId'].map(key => [key, row[key]])), timestamp: timestamp(row.timestamp) });
  assert.deepEqual(ordered(observed.adjustments).map(fields), ordered(saved.expected.history).map(fields), 'SQL and API audit disagree');
}
module.exports = { taskboard, inventory, sqlAgreement, expect, ordered };
if (require.main === module) (async () => {
  const [kind, phase, base, statePath, reportPath] = process.argv.slice(2);
  const exchanges = [];
  const report = { schema: 'vcp-engagement-api-check/1', kind, phase, status: 'failed', exchanges };
  try {
    const origin = new URL(base); assert(['127.0.0.1', '[::1]'].includes(origin.hostname)); assert.equal(origin.protocol, 'http:');
    const request = async (method, route, body) => {
      const response = await fetch(new URL(route, origin), { method, redirect: 'error', headers: { 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(30000) });
      const reader = response.body.getReader(); let bytes = 0; const chunks = [];
      for (;;) { const part = await reader.read(); if (part.done) break; bytes += part.value.length; assert(bytes <= 2 * 1024 * 1024, 'Response exceeds capture bound'); chunks.push(Buffer.from(part.value)); }
      const text = Buffer.concat(chunks).toString('utf8');
      const result = { status: response.status, headers: Object.fromEntries(response.headers), json: text ? JSON.parse(text) : null };
      exchanges.push({ method, route, body, result }); return result;
    };
    const saved = ['restart', 'restart-export', 'fault'].includes(phase) ? JSON.parse(fs.readFileSync(statePath, 'utf8')) : null;
    assert(['A', 'B'].includes(kind));
    const state = await (kind === 'A' ? taskboard : inventory)(request, phase, saved);
    if (!saved) fs.writeFileSync(statePath, JSON.stringify(state, null, 2) + '\n', { flag: 'wx' });
    report.status = 'passed';
  } catch (error) { report.error = error.message; process.exitCode = 1; }
  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
})().catch(error => { console.error(error.message); process.exitCode = 1; });
