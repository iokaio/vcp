// SPDX-License-Identifier: Apache-2.0
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const { randomUUID } = require('node:crypto');
const { sha } = require('./prerequisites.cjs');
const playwrightRoot = path.resolve(__dirname, '../../../src/tests/skills/browser/node_modules/playwright');
async function keyboardActivate(page, locator) {
  await locator.waitFor({ state: 'visible' });
  for (let n = 0; n < 100; n++) {
    if (await locator.evaluate(element => element === document.activeElement)) { await page.keyboard.press('Enter'); return; }
    await page.keyboard.press('Tab');
  }
  throw Error('Control is not keyboard reachable');
}
async function inspect({ kind, base, state, output, requireHelp = false }) {
  const origin = new URL(base); assert.equal(origin.protocol, 'http:'); assert(['127.0.0.1', '[::1]'].includes(origin.hostname));
  const declared = require(path.resolve(__dirname, '../../../src/tests/skills/browser/package.json')).dependencies.playwright;
  assert.equal(require(path.join(playwrightRoot, 'package.json')).version, declared);
  const { chromium } = require(playwrightRoot);
  const browser = await chromium.launch({ headless: true });
  const report = { schema: 'vcp-engagement-browser/1', kind, playwright: declared, status: 'failed', checks: [], screenshots: [] };
  const context = await browser.newContext({ acceptDownloads: true });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  try {
    // Prevent authored application code from using the browser to contact external services.
    await context.route('**/*', route => {
      const url = new URL(route.request().url());
      return url.origin === origin.origin ? route.continue() : route.abort('blockedbyclient');
    });
    const api = async route => { const response = await context.request.get(new URL(route, origin).href, { maxRedirects: 0 }); assert.equal(response.status(), 200); return response.json(); };
    if (kind === 'A') {
      await page.goto(base); const before = await api('/api/tasks/export');
      if (requireHelp) { const help = page.getByTestId('transfer-format-help'); await help.waitFor({state:'visible'}); assert.match(await help.innerText(), /version\s*1/i); report.checks.push('post-pause transfer format help is visible'); }
      const downloadPromise = page.waitForEvent('download');
      await keyboardActivate(page, page.getByRole('button', { name: 'Export tasks', exact: true }));
      const download = await downloadPromise; const file = path.join(output, 'browser-export.json'); await download.saveAs(file);
      assert.deepEqual(JSON.parse(fs.readFileSync(file)), before); report.checks.push('keyboard export contains all task fields');
      const fileInput = page.getByLabel('Import tasks', { exact: true });
      await fileInput.setInputFiles({ name: 'invalid.json', mimeType: 'application/json', buffer: Buffer.from('{"schemaVersion":2,"tasks":[]}') });
      await page.getByRole('alert').filter({ hasText: /.+/ }).waitFor({ state: 'visible' });
      assert.deepEqual(await api('/api/tasks/export'), before); report.checks.push('visible invalid-import error preserves tasks');
      await fileInput.setInputFiles(file);
      await page.getByRole('status').filter({ hasText: /import|unchanged/i }).waitFor({ state: 'visible' });
      assert.deepEqual(await api('/api/tasks/export'), before); report.checks.push('actual downloaded document imports idempotently with summary');
      const added = { ...before.tasks[0], id: randomUUID(), title: 'Browser-imported résumé' };
      assert(before.tasks.length, 'Browser fixture requires representative exported data');
      await fileInput.setInputFiles({ name: 'new-task.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify({ schemaVersion: 1, tasks: [added] })) });
      await page.getByRole('status').filter({ hasText: /import|unchanged/i }).waitFor({ state: 'visible' });
      const until = Date.now() + 10000; let imported;
      do { imported = await api('/api/tasks/export'); if (imported.tasks.some(task => task.id === added.id)) break; await page.waitForTimeout(50); } while (Date.now() < until);
      assert.deepEqual(imported.tasks.find(task => task.id === added.id), added); assert.equal(imported.tasks.length, before.tasks.length + 1);
      let reachable = false;
      for (let n = 0; n < 100; n++) { await page.keyboard.press('Tab'); if (await fileInput.evaluate(element => element === document.activeElement)) { reachable = true; break; } }
      assert(reachable, 'Import control is not keyboard reachable'); report.checks.push('keyboard-reachable import applies a complete new task');
    } else {
      assert.equal(kind, 'B'); assert(state?.productId);
      const historyRoute = `/api/products/${state.productId}/adjustments`;
      await page.goto(new URL(`/Products/Adjust?id=${state.productId}`, origin).href);
      if (requireHelp) { const help = page.getByTestId('adjustment-help'); await help.waitFor({state:'visible'}); assert.match(await help.innerText(), /operation/i); report.checks.push('post-pause operation/retry help is visible'); }
      const before = await api(historyRoute);
      await page.getByLabel('Delta', { exact: true }).fill('0'); await page.getByLabel('Reason', { exact: true }).fill('browser invalid');
      await keyboardActivate(page, page.getByRole('button', { name: 'Apply adjustment', exact: true }));
      await page.getByRole('alert').filter({ hasText: /.+/ }).waitFor({ state: 'visible' });
      assert.deepEqual(await api(historyRoute), before); report.checks.push('keyboard submit rejects zero delta without audit write');
      const stockBefore = (await api(`/api/products/${state.productId}/stock`)).onHand;
      await page.getByLabel('Delta', { exact: true }).fill('1'); await page.getByLabel('Reason', { exact: true }).fill('Browser stock receipt');
      await keyboardActivate(page, page.getByRole('button', { name: 'Apply adjustment', exact: true }));
      await page.getByRole('status').filter({ hasText: /.+/ }).waitFor({ state: 'visible' });
      assert.equal((await api(`/api/products/${state.productId}/stock`)).onHand, stockBefore + 1);
      assert.equal((await api(historyRoute)).length, before.length + 1); report.checks.push('successful browser adjustment changes stock and appends one audit');
      await page.goto(new URL(`/Products/Adjust?id=${state.productId}`, origin).href);
      const current = await context.request.get(new URL(`/api/products/${state.productId}`, origin).href, { maxRedirects: 0 });
      assert.equal(current.status(), 200);
      const currentProduct = await current.json();
      const rowVersion = currentProduct.rowVersion ?? current.headers().etag?.replace(/^"|"$/g, '');
      const advanced = await context.request.post(new URL(`/api/products/${state.productId}/adjustments`, origin).href, { maxRedirects: 0, data: { operationId: randomUUID(), delta: 1, reason: 'Browser concurrency challenger', rowVersion } });
      assert.equal(advanced.status(), 201);
      const raced = await api(historyRoute);
      await page.getByLabel('Delta', { exact: true }).fill('1'); await page.getByLabel('Reason', { exact: true }).fill('Stale browser form');
      await keyboardActivate(page, page.getByRole('button', { name: 'Apply adjustment', exact: true }));
      await page.getByRole('alert').filter({ hasText: /.+/ }).waitFor({ state: 'visible' });
      assert.deepEqual(await api(historyRoute), raced); report.checks.push('stale browser form shows an error without changing audit');
      await page.goto(new URL(`/Products/History?id=${state.productId}`, origin).href);
      const table = page.getByRole('table', { name: 'Stock adjustment history', exact: true });
      await table.waitFor({ state: 'visible' });
      for (const row of raced) assert((await table.innerText()).includes(row.reason), 'Audit UI omitted adjustment reason');
      report.checks.push('accessible audit table contains retained adjustments');
      report.final_application_state = { productId: state.productId, stock: (await api(`/api/products/${state.productId}/stock`)).onHand, history: raced };
    }
    const screenshot = path.join(output, 'browser.png'); await page.screenshot({ path: screenshot, fullPage: true });
    report.screenshots.push({ path: 'browser.png', sha256: sha(fs.readFileSync(screenshot)), observation: 'captured after actual automated browser assertions' });
    report.status = 'passed';
  } catch (error) { report.error = error.message; }
  finally { await context.close(); await browser.close(); }
  return report;
}
module.exports = { inspect, keyboardActivate };
if (require.main === module) (async () => {
  const [kind, base, statePath, output, phase] = process.argv.slice(2);
  const state = statePath === '-' ? null : JSON.parse(fs.readFileSync(statePath));
  fs.mkdirSync(output, { recursive: false });
  const report = await inspect({ kind, base, state, output, requireHelp: phase === 'require-help' });
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  if (report.status !== 'passed') process.exitCode = 1;
})().catch(error => { console.error(error.message); process.exitCode = 1; });
